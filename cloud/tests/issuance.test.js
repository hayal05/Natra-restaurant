import test from "node:test";
import assert from "node:assert/strict";
import { readFile, mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { PGlite } from "@electric-sql/pglite";
import { generateKeyPair, exportPKCS8, exportJWK, createLocalJWKSet, jwtVerify } from "jose";
import { createSessionTokenIssuer } from "../auth/issue-token.js";
import { createJwtAuthVerifier } from "../auth/jwt.js";
import { createNeonSessionValidator } from "../db/neon-session.js";
import { parseAdminCommand, runAdminCommand } from "../admin/sync-admin.js";
import worker from "../worker.js";

const workspace = "11111111-1111-4111-8111-111111111111";
const installation = "22222222-2222-4222-8222-222222222222";
const otherWorkspace = "44444444-4444-4444-8444-444444444444";
const scope = {workspace_id:workspace,installation_id:installation,subject:"approved-user"};
const pair = await generateKeyPair("RS256",{extractable:true});
const config = {connectionString:"postgresql://test:fake@ep-example.eu-central-1.aws.neon.tech/neondb",
  issuer:"https://issuer.example.test/",audience:"natra-sync",kid:"test-key",
  privateKeyPem:await exportPKCS8(pair.privateKey)};
const keys = createLocalJWKSet({keys:[{...await exportJWK(pair.publicKey),kid:config.kid,alg:"RS256"}]});

test("issuer reserves scoped sessions and signs bounded tokens in embedded PostgreSQL", async t => {
  const db = await PGlite.create();
  try {
    for (const file of ["001_sync_workspaces.sql","002_sync_devices.sql","003_sync_sessions.sql",
      "004_reserve_sync_session.sql","005_complete_sync_session.sql","006_fail_sync_session.sql",
      "009_enroll_sync_device.sql","010_reserve_authorized_sync_session.sql"]) {
      await db.exec(await readFile(new URL("../"+file,import.meta.url),"utf8"));
    }
    await db.query("INSERT INTO sync_workspaces(id,name) VALUES ($1,'A'),($2,'B')",[workspace,otherWorkspace]);
    await db.query("SELECT public.enroll_sync_device($1::uuid,$2::uuid)",[workspace,installation]);
    const createSql = () => ({query:async (sql,params,options) => {
      assert.ok(options.fetchOptions.signal instanceof AbortSignal);
      return (await db.query(sql,params)).rows;
    }});
    const issue = await createSessionTokenIssuer(config,createSql);
    const count = async () => (await db.query("SELECT count(*)::integer AS n FROM sync_sessions")).rows[0].n;
    const scenario = async (name,fn) => t.test(name,async () => {
      await db.exec("DELETE FROM sync_sessions; UPDATE sync_devices SET deactivated_at=NULL");
      await fn();
    });

    await scenario("issued JWT matches a running session and expires by its database deadline", async () => {
      const result = await issue(scope);
      const {payload,protectedHeader} = await jwtVerify(result.token,keys,{issuer:config.issuer,audience:config.audience});
      const row = (await db.query("SELECT * FROM sync_sessions WHERE id=$1",[result.session_id])).rows[0];
      assert.equal(row.status,"running");
      assert.equal(payload.session_id,row.id);
      assert.equal(payload.workspace_id,workspace);
      assert.equal(payload.installation_id,installation);
      assert.equal(payload.sub,scope.subject);
      assert.equal(payload.scope,"natra:sync");
      assert.equal(payload.exp-payload.iat,900);
      assert.ok(payload.exp*1000 <= new Date(row.started_at).getTime()+900000);
      assert.equal(new Date(result.expires_at).getTime(),payload.exp*1000);
      assert.equal(protectedHeader.kid,config.kid);
      const response = await worker.fetch(new Request("https://sync.example.test/sync/upload",{
        method:"POST",headers:{authorization:"Bearer "+result.token},
      }),{
        SYNC_AUTH_VERIFIER:createJwtAuthVerifier({...config,jwksUrl:config.issuer+"keys"},keys),
        SYNC_SESSION_VALIDATOR:createNeonSessionValidator(config.connectionString,createSql),
      });
      assert.equal(response.status,501);
    });

    await scenario("foreign, missing and deactivated installations cannot receive tokens", async () => {
      await assert.rejects(() => issue({...scope,workspace_id:otherWorkspace}),/issuance failed/);
      await assert.rejects(() => issue({...scope,installation_id:otherWorkspace}),/issuance failed/);
      await db.exec("UPDATE sync_devices SET deactivated_at=now()");
      await assert.rejects(() => issue(scope),/issuance failed/);
      assert.equal(await count(),0);
    });

    await scenario("a second attempt cannot replace an active session", async () => {
      const first = await issue(scope);
      await assert.rejects(() => issue(scope),/issuance failed/);
      assert.equal(await count(),1);
      const row = (await db.query("SELECT id,status FROM sync_sessions")).rows[0];
      assert.deepEqual(row,{id:first.session_id,status:"running"});
    });

    await scenario("existing ten-session daily limit is enforced before issuance", async () => {
      for (let i=0;i<10;i++) {
        await db.query("INSERT INTO sync_sessions(id,installation_id,started_at,completed_at,status) VALUES ($1,$2,now()-interval '1 second',now(),'completed')",
          [crypto.randomUUID(),installation]);
      }
      await assert.rejects(() => issue(scope),/issuance failed/);
      assert.equal(await count(),10);
    });

    await scenario("malformed reservation responses release only this attempt's session", async () => {
      const brokenSql = () => ({query:async (sql,params) => {
        const rows = (await db.query(sql,params)).rows;
        return sql.includes("reserve_authorized") ? [{...rows[0],session_id:otherWorkspace}] : rows;
      }});
      const brokenIssue = await createSessionTokenIssuer(config,brokenSql);
      await assert.rejects(() => brokenIssue(scope),/issuance failed/);
      assert.deepEqual((await db.query("SELECT status FROM sync_sessions")).rows,[{status:"failed"}]);
    });

    await scenario("administration entry point enrolls and issues through mocked Neon HTTP", async () => {
      const {neonConfig} = await import("@neondatabase/serverless");
      const originalFetch = neonConfig.fetchFunction;
      const dir = await mkdtemp(join(tmpdir(),"natra-issuer-test-"));
      const keyFile = join(dir,"test-key.pem");
      await writeFile(keyFile,config.privateKeyPem,{mode:0o600});
      neonConfig.fetchFunction = async (url,options) => {
        assert.equal(new URL(url).protocol,"https:");
        const body = JSON.parse(options.body);
        const result = await db.query(body.query,body.params);
        const rows = result.rows.map(row => result.fields.map(field => {
          const value = row[field.name];
          return value instanceof Date ? value.toISOString().replace("T"," ").replace(/Z$/,"+00") : value;
        }));
        return new Response(JSON.stringify({fields:result.fields,rows,rowCount:rows.length,command:"SELECT"}),{
          status:200,headers:{"content-type":"application/json"},
        });
      };
      try {
        const env = {NEON_DATABASE_URL:config.connectionString,SYNC_SIGNING_KEY_FILE:keyFile,
          SYNC_JWT_ISSUER:config.issuer,SYNC_JWT_AUDIENCE:config.audience,SYNC_SIGNING_KEY_ID:config.kid};
        assert.deepEqual(await runAdminCommand(["enroll",workspace,installation],env),{installation_id:installation});
        const result = await runAdminCommand(["issue-token",workspace,installation,scope.subject],env);
        const {payload} = await jwtVerify(result.token,keys,{issuer:config.issuer,audience:config.audience});
        assert.equal(payload.session_id,result.session_id);
        assert.equal(await count(),1);
      } finally {
        neonConfig.fetchFunction = originalFetch;
        await rm(dir,{recursive:true,force:true});
      }
    });

    await scenario("PUBLIC receives no scoped reservation execution permission", async () => {
      await db.exec("CREATE ROLE untrusted_issuer_test");
      const result = await db.query("SELECT has_function_privilege('untrusted_issuer_test','public.reserve_authorized_sync_session(uuid,uuid,uuid)','EXECUTE') AS allowed");
      assert.equal(result.rows[0].allowed,false);
    });
  } finally {
    await db.close();
  }
});

test("invalid issuer keys, settings and scope never reserve a session", async () => {
  let calls = 0;
  const createSql = () => ({query:async () => {calls++;throw new Error("must not query");}});
  for (const settings of [
    {...config,privateKeyPem:"invalid"}, {...config,issuer:"http://issuer.example.test/"},
    {...config,audience:""}, {...config,kid:""},
  ]) await assert.rejects(() => createSessionTokenIssuer(settings,createSql));
  const issue = await createSessionTokenIssuer(config,createSql);
  for (const changed of [{...scope,subject:""},{...scope,workspace_id:"bad"},{...scope,installation_id:null}]) {
    await assert.rejects(() => issue(changed),/Invalid token scope/);
  }
  assert.equal(calls,0);
});

test("administration command validates exact arguments and does not expose public routes", async () => {
  assert.equal(parseAdminCommand(["enroll",workspace,installation]).action,"enroll");
  assert.equal(parseAdminCommand(["issue-token",workspace,installation,"user"]).subject,"user");
  for (const args of [[],["enroll",workspace],["enroll",workspace,installation,"extra"],
    ["issue-token",workspace,installation],["issue-token",workspace,installation,""],
    ["enroll","bad",installation],["unknown",workspace,installation]]) {
    assert.throws(() => parseAdminCommand(args));
  }
  for (const path of ["/sync/enroll","/sync/issue-token","/sync/admin"]) {
    const response = await worker.fetch(new Request("https://sync.example.test"+path,{method:"POST"}));
    assert.equal(response.status,404);
  }
});

test("CLI help and errors do not contact services or print secrets", async () => {
  const run = promisify(execFile);
  const script = fileURLToPath(new URL("../admin/sync-admin.js",import.meta.url));
  const env = {...process.env,NEON_DATABASE_URL:"private-connection-sentinel",SYNC_SIGNING_KEY_FILE:"private-key-sentinel"};
  const help = await run(process.execPath,[script,"--help"],{env});
  assert.match(help.stdout,/Usage:/);
  assert.equal(help.stderr,"");
  await assert.rejects(() => run(process.execPath,[script,"issue-token"],{env}),error => {
    assert.equal(error.code,1);
    assert.equal(error.stdout,"");
    assert.match(error.stderr,/Sync administration failed/);
    assert.doesNotMatch(error.stderr,/private-connection-sentinel|private-key-sentinel/);
    return true;
  });
});
