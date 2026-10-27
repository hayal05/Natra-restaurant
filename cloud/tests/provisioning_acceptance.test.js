import test from "node:test";
import assert from "node:assert/strict";
import {readFile,readdir} from "node:fs/promises";
import {Readable} from "node:stream";
import {spawnSync} from "node:child_process";
import {fileURLToPath} from "node:url";
import {PGlite} from "@electric-sql/pglite";
import {generateKeyPair,exportPKCS8,exportJWK,createLocalJWKSet,jwtVerify} from "jose";
import {buildProvisioningPacket,readProvisioningInput} from "../admin/provisioning-packet.js";
import {newSyncCredential} from "../auth/credential.js";
import {runCredentialCommand} from "../admin/sync-credential-admin.js";
import {createCredentialIssuer} from "../db/credential-issuer.js";
import {createSessionTokenIssuer} from "../auth/issue-token.js";
import {createJwtAuthVerifier} from "../auth/jwt.js";
import {createNeonSessionValidator} from "../db/neon-session.js";
import {createBatchUploader} from "../db/upload.js";
import {createEventPuller} from "../db/pull.js";
import {createSessionFinisher} from "../db/session-control.js";
import {createAllowanceReader} from "../db/allowance.js";
import broker from "../broker-worker.js";
import syncWorker from "../worker.js";
const workspace="11111111-1111-4111-8111-111111111111",installation="22222222-2222-4222-8222-222222222222";
const other="44444444-4444-4444-8444-444444444444",localUser="9007199254740993";
const connectionString="postgresql://test:fake@ep-example.eu-central-1.aws.neon.tech/neondb";
const grant=()=>({credential_id:crypto.randomUUID(),workspace_id:workspace,installation_id:installation,local_user_id:localUser,subject:"approved-subject",credential:newSyncCredential()});
const approval=g=>({credential_id:g.credential_id,workspace_id:g.workspace_id,installation_id:g.installation_id,
  local_user_id:g.local_user_id,subject:g.subject,broker_endpoint:"https://broker.example.test/session",sync_origin:"https://sync.example.test/",issuer:"https://issuer.example.test/",audience:"natra-sync"});

test("provisioning packet binds independently approved scope and preserves exact native identity",async()=>{
  const g=grant(),a=approval(g),packet=buildProvisioningPacket(g,a);
  assert.equal(packet.version,1);assert.equal(packet.local_user_id,localUser);assert.equal(packet.credential,g.credential);
  assert.ok(Buffer.byteLength(JSON.stringify(packet)+"\n")<=2560);
  for(const field of ["credential_id","workspace_id","installation_id","local_user_id","subject"]){
    const changed={...a,[field]:field.endsWith("_id")&&field!=="local_user_id"?other:field==="local_user_id"?"1":"other-subject"};
    await assert.rejects(async()=>buildProvisioningPacket(g,changed));
  }
  assert.deepEqual(await readProvisioningInput(Readable.from([Buffer.from(JSON.stringify({grant:g,approval:a}))])),packet);
});
test("provisioning input rejects unsafe endpoints, excess fields and oversized or invalid data",async()=>{
  const g=grant(),a=approval(g);
  for(const [field,value] of [["broker_endpoint","http://broker.example.test/session"],["broker_endpoint","https://user:secret@broker.example.test/session"],
    ["broker_endpoint","https://broker.example.test/session?redirect=bad"],["broker_endpoint","https://broker.example.test/other"],
    ["sync_origin","https://sync.example.test/path"],["sync_origin","https://sync.example.test:444/"],["issuer","https://issuer.example.test/#bad"],["issuer","https://issuer.example.test/#"],["broker_endpoint","https://broker.example.test/session?"],
    ["subject","bad\nsubject"],["audience","é".repeat(129)]])assert.throws(()=>buildProvisioningPacket(g,{...a,[field]:value}));
  assert.throws(()=>buildProvisioningPacket({...g,credential:g.credential+"\n"},a));
  assert.throws(()=>buildProvisioningPacket(g,{...a,unapproved:true}));
  assert.throws(()=>buildProvisioningPacket({...g,local_user_id:Number(localUser)},a));
  await assert.rejects(readProvisioningInput(Readable.from([Buffer.alloc(8193)])));
  const stream=Readable.from([]);stream.isTTY=true;await assert.rejects(readProvisioningInput(stream));
});
test("trusted packet CLI accepts stdin only and never echoes secret input in errors",()=>{
  const g=grant(),input=JSON.stringify({grant:g,approval:approval(g)}),script=fileURLToPath(new URL("../admin/provisioning-packet.js",import.meta.url));
  const valid=spawnSync(process.execPath,[script],{input,encoding:"utf8"});
  assert.equal(valid.status,0);assert.equal(valid.stderr,"");assert.equal(JSON.parse(valid.stdout).credential,g.credential);
  for(const [args,body] of [[["unexpected-argument"],input],[[],input.slice(0,-1)],[[],JSON.stringify({grant:g,approval:{}})]]){
    const result=spawnSync(process.execPath,[script,...args],{input:body,encoding:"utf8"});
    assert.equal(result.status,1);assert.equal(result.stdout,"");assert.ok(!result.stderr.includes(g.credential));
    assert.match(result.stderr,/preparation failed/);
  }
});

test("generated-key provisioning lifecycle revokes terminal bearer access without changing ledger or quota",async t=>{
  const db=await PGlite.create();
  try{
    const files=(await readdir(new URL("../",import.meta.url))).filter(f=>/^0(?:0[1-9]|1[0-6])_.*\.sql$/.test(f)).sort();
    assert.equal(files.length,16);
    for(const file of files)await db.exec(await readFile(new URL("../"+file,import.meta.url),"utf8"));
    await db.query("INSERT INTO public.sync_workspaces(id,name) VALUES ($1,'A'),($2,'B')",[workspace,other]);
    for(const [w,i] of [[workspace,installation],[other,other]])await db.query("SELECT public.enroll_sync_device($1,$2)",[w,i]);
    const createSql=()=>({query:async(sql,params,options)=>{
      assert.ok(options.fetchOptions.signal instanceof AbortSignal);return (await db.query(sql,params)).rows;
    }});
    const pair=await generateKeyPair("RS256",{extractable:true});
    const config={connectionString,issuer:"https://issuer.example.test/",audience:"natra-sync",kid:"acceptance-test",privateKeyPem:await exportPKCS8(pair.privateKey)};
    const keys=createLocalJWKSet({keys:[{...await exportJWK(pair.publicKey),kid:config.kid,alg:"RS256"}]});
    const verify=createJwtAuthVerifier({...config,jwksUrl:"https://issuer.example.test/jwks.json"},keys);
    const issue=await createCredentialIssuer(config,createSql);
    const brokerEnv={SYNC_BROKER_ENABLED:"true",SYNC_BROKER_RATE_LIMITER:{limit:async()=>({success:true})},SYNC_CREDENTIAL_ISSUER:issue};
    const env={SYNC_AUTH_VERIFIER:verify,SYNC_SESSION_VALIDATOR:createNeonSessionValidator(connectionString,createSql),
      SYNC_UPLOAD_ENABLED:"true",SYNC_PULL_ENABLED:"true",SYNC_SESSION_CONTROL_ENABLED:"true",SYNC_ALLOWANCE_ENABLED:"true",
      SYNC_BATCH_UPLOADER:createBatchUploader(connectionString,createSql),SYNC_EVENT_PULLER:createEventPuller(connectionString,createSql),
      SYNC_SESSION_FINISHER:createSessionFinisher(connectionString,createSql),SYNC_ALLOWANCE_READER:createAllowanceReader(connectionString,createSql)};
    const create=()=>runCredentialCommand(["create",workspace,installation,localUser,"approved-subject"],{NEON_DATABASE_URL:connectionString},createSql);
    const revoke=g=>runCredentialCommand(["revoke",workspace,installation,g.credential_id],{NEON_DATABASE_URL:connectionString},createSql);
    const session=async g=>{
      const r=await broker.fetch(new Request("https://broker.example.test/session",{method:"POST",headers:{authorization:"Bearer "+g.credential,"content-type":"application/json"},body:JSON.stringify({installation_id:installation,local_user_id:localUser})}),brokerEnv);
      return {status:r.status,body:await r.json()};
    };
    const event=()=>({event_id:crypto.randomUUID(),entity_type:"categories",entity_id:installation+":1",operation:"upsert",payload:{origin_installation_id:installation,local_id:1,name:"Test category"}});
    const api=(token,path,events=[event()])=>syncWorker.fetch(new Request("https://sync.example.test/sync/"+path,{
      method:["pull","allowance"].includes(path)?"GET":"POST",headers:{authorization:"Bearer "+token,...(path==="upload"?{"content-type":"application/json"}:{})},
      ...(path==="upload"?{body:JSON.stringify({events})}:{}),
    }),env);
    const seed=await create(),old=(await session(seed)).body;
    assert.equal((await api(old.token,"complete")).status,200);await revoke(seed);
    await db.exec(await readFile(new URL("../017_revoke_credential_bearer_sessions.sql",import.meta.url),"utf8"));
    await t.test("migration backfills sessions of previously revoked credentials",async()=>{
      await jwtVerify(old.token,keys,{issuer:config.issuer,audience:config.audience});
      assert.equal((await db.query("SELECT count(*)::int AS n FROM public.sync_revoked_sessions")).rows[0].n,1);
      for(const path of ["complete","fail","allowance"])assert.equal((await api(old.token,path)).status,403,path);
    });
    const g=await create(),first=(await session(g)).body;
    let eventId;
    await t.test("approved packet and real signed token support upload, pull and idempotent completion",async()=>{
      const packet=buildProvisioningPacket(g,approval(g));assert.equal(packet.local_user_id,localUser);
      const payload=(await jwtVerify(first.token,keys,{issuer:config.issuer,audience:config.audience})).payload;
      assert.equal(payload.credential_id,packet.credential_id);assert.equal(payload.local_user_id,localUser);
      const e=event();eventId=e.event_id;
      assert.equal((await api(first.token,"upload",[e])).status,200);
      const pulled=await api(first.token,"pull");assert.equal(pulled.status,200);assert.equal((await pulled.json()).events[0].event_id,eventId);
      for(let i=0;i<2;i++)assert.deepEqual(await (await api(first.token,"complete")).json(),{accepted:true});
      assert.equal((await api(first.token,"allowance")).status,200);
    });
    const running=(await session(g)).body;
    const unrelated=await (await createSessionTokenIssuer(config,createSql))({workspace_id:other,installation_id:other,subject:"legacy-approved-subject"});
    await t.test("revocation denies all five routes for running and completed bearer tokens",async()=>{
      await assert.rejects(()=>runCredentialCommand(["revoke",other,installation,g.credential_id],{NEON_DATABASE_URL:connectionString},createSql));
      await revoke(g);await revoke(g);
      for(const token of [first.token,running.token])for(const path of ["upload","pull","complete","fail","allowance"]){
        const response=await api(token,path);assert.equal(response.status,403,path);assert.equal(response.headers.get("cache-control"),"no-store");
        assert.ok(!(await response.text()).includes(g.credential));
      }
      assert.equal((await session(g)).status,401);
    });
    await t.test("transaction guards reject revoked sessions even if a preflight or status snapshot is stale",async()=>{
      await db.exec("BEGIN");
      try{
        await db.query("UPDATE public.sync_sessions SET status='running',completed_at=NULL,started_at=clock_timestamp() WHERE id=$1",[first.session_id]);
        for(const [sql,args] of [
          ["SELECT * FROM public.upload_sync_batch($1,$2,$3,$4::jsonb)",[workspace,installation,first.session_id,JSON.stringify([event()])]],
          ["SELECT * FROM public.pull_authorized_sync_events($1,$2,$3,0,100)",[workspace,installation,first.session_id]],
          ["SELECT public.finish_authorized_sync_session($1,$2,$3,'complete')",[workspace,installation,first.session_id]],
          ["SELECT * FROM public.authorized_sync_allowance($1,$2,$3)",[workspace,installation,first.session_id]],
        ]){
          await db.exec("SAVEPOINT denied_operation");
          await assert.rejects(()=>db.query(sql,args),e=>e.code==="42501");
          await db.exec("ROLLBACK TO SAVEPOINT denied_operation");
        }
      }finally{await db.exec("ROLLBACK");}
    });
    await t.test("committed history, quota and unrelated sessions survive revocation",async()=>{
      assert.equal((await db.query("SELECT count(*)::int AS n FROM public.sync_events")).rows[0].n,1);
      assert.equal((await db.query("SELECT event_id FROM public.sync_events")).rows[0].event_id,eventId);
      assert.equal((await db.query("SELECT count(*)::int AS n FROM public.sync_sessions WHERE installation_id=$1 AND status='completed'",[installation])).rows[0].n,2);
      assert.equal((await db.query("SELECT status FROM public.sync_sessions WHERE id=$1",[running.session_id])).rows[0].status,"failed");
      assert.equal((await api(unrelated.token,"allowance")).status,200);assert.equal((await api(unrelated.token,"complete")).status,200);
    });
    const replacement=await create(),next=(await session(replacement)).body;
    await t.test("explicit rotation permits only the replacement grant and new session",async()=>{
      assert.notEqual(replacement.credential_id,g.credential_id);assert.notEqual(next.session_id,running.session_id);
      assert.equal((await api(next.token,"allowance")).status,200);
      assert.equal((await session(g)).status,401);assert.equal((await api(first.token,"complete")).status,403);
      assert.equal(buildProvisioningPacket(replacement,approval(replacement)).credential_id,replacement.credential_id);
    });
    await t.test("invoker runtime can check revocations without reading credential tables",async()=>{
      await db.exec("CREATE ROLE acceptance_runtime; GRANT USAGE ON SCHEMA public TO acceptance_runtime");
      const asRole=async fn=>{await db.exec("SET ROLE acceptance_runtime");try{await fn();}finally{await db.exec("RESET ROLE");}};
      await asRole(()=>assert.rejects(()=>db.query("SELECT public.assert_sync_session_not_revoked($1)",[next.session_id]),e=>e.code==="42501"));
      await db.exec(`GRANT SELECT ON public.sync_revoked_sessions TO acceptance_runtime;
        GRANT SELECT,UPDATE ON public.sync_workspaces,public.sync_devices,public.sync_sessions TO acceptance_runtime;
        GRANT EXECUTE ON FUNCTION public.assert_sync_session_not_revoked(uuid),public.authorized_sync_allowance(uuid,uuid,uuid),public.sync_day_bounds(timestamptz) TO acceptance_runtime`);
      await asRole(async()=>{
        await assert.rejects(()=>db.query("SELECT secret_digest FROM public.sync_credentials"),e=>e.code==="42501");
        const row=await db.query("SELECT * FROM public.authorized_sync_allowance($1,$2,$3)",[workspace,installation,next.session_id]);assert.equal(row.rows[0].session_permitted,true);
        await assert.rejects(()=>db.query("SELECT * FROM public.authorized_sync_allowance($1,$2,$3)",[workspace,installation,first.session_id]),e=>e.code==="42501");
      });
      const info=(await db.query("SELECT prosecdef,proconfig FROM pg_proc WHERE oid='public.assert_sync_session_not_revoked(uuid)'::regprocedure")).rows[0];
      assert.equal(info.prosecdef,false);assert.ok(info.proconfig.includes("search_path=pg_catalog, public, pg_temp"));
    });
  }finally{await db.close();}
});
