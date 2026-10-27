import test from "node:test";
import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import {PGlite} from "@electric-sql/pglite";
import {generateKeyPair,exportPKCS8,exportJWK,createLocalJWKSet,jwtVerify} from "jose";
import {newSyncCredential,credentialDigest} from "../auth/credential.js";
import {createCredentialIssuer} from "../db/credential-issuer.js";
import {runCredentialCommand,parseCredentialCommand} from "../admin/sync-credential-admin.js";
import {createNeonSessionValidator} from "../db/neon-session.js";
import broker from "../broker-worker.js";
import syncWorker from "../worker.js";

const workspace="11111111-1111-4111-8111-111111111111",installation="22222222-2222-4222-8222-222222222222";
const other="44444444-4444-4444-8444-444444444444",localUser="9007199254740993";
const connectionString="postgresql://test:fake@ep-example.eu-central-1.aws.neon.tech/neondb";
const pair=await generateKeyPair("RS256",{extractable:true});
const config={connectionString,issuer:"https://issuer.example.test/",audience:"natra-sync",kid:"broker-test",privateKeyPem:await exportPKCS8(pair.privateKey)};
const keys=createLocalJWKSet({keys:[{...await exportJWK(pair.publicKey),kid:config.kid,alg:"RS256"}]});
const scope={installation_id:installation,local_user_id:localUser};
const allow={limit:async()=>({success:true})};
function request(credential,body=scope,extra={}){
  return new Request("https://broker.example.test/session",{method:"POST",headers:{
    authorization:"Bearer "+credential,"content-type":"application/json",...extra},body:JSON.stringify(body)});
}

test("credential broker reserves, renews and revokes scoped server-authorized sessions",async t=>{
  const db=await PGlite.create();
  try{
    const files=["001_sync_workspaces.sql","002_sync_devices.sql","003_sync_sessions.sql","004_reserve_sync_session.sql",
      "005_complete_sync_session.sql","006_fail_sync_session.sql","007_sync_events.sql","008_upload_sync_event.sql",
      "009_enroll_sync_device.sql","010_reserve_authorized_sync_session.sql","011_upload_sync_batch.sql",
      "012_pull_authorized_sync_events.sql","013_finish_authorized_sync_session.sql","014_authorized_sync_allowance.sql",
      "015_harden_sync_function_privileges.sql","016_sync_credentials.sql"];
    for(const file of files)await db.exec(await readFile(new URL("../"+file,import.meta.url),"utf8"));
    await db.query("INSERT INTO public.sync_workspaces(id,name) VALUES ($1,'A'),($2,'B')",[workspace,other]);
    await db.query("SELECT public.enroll_sync_device($1,$2)",[workspace,installation]);
    await db.query("SELECT public.enroll_sync_device($1,$2)",[other,other]);
    const createSql=()=>({query:async(sql,params,options)=>{
      assert.ok(options.fetchOptions.signal instanceof AbortSignal);return (await db.query(sql,params)).rows;
    }});
    const issue=await createCredentialIssuer(config,createSql);
    const env={SYNC_BROKER_ENABLED:"true",SYNC_BROKER_RATE_LIMITER:allow,SYNC_CREDENTIAL_ISSUER:issue};
    let grant;
    const create=()=>runCredentialCommand(["create",workspace,installation,localUser,"approved-cloud-subject"],{NEON_DATABASE_URL:connectionString},createSql);
    const complete=id=>db.query("SELECT public.finish_authorized_sync_session($1,$2,$3,'complete')",[workspace,installation,id]);
    const revoke=()=>runCredentialCommand(["revoke",workspace,installation,grant.credential_id],{NEON_DATABASE_URL:connectionString},createSql);
    await t.test("trusted registration stores a digest and never silently replaces a live grant",async()=>{
      grant=await create();
      const row=(await db.query("SELECT *,local_user_id::text AS exact_user_id FROM public.sync_credentials")).rows[0];
      assert.equal(row.secret_digest,await credentialDigest(grant.credential));
      assert.ok(!JSON.stringify(row,(_,value)=>typeof value==="bigint"?value.toString():value).includes(grant.credential));
      assert.equal(row.exact_user_id,localUser);assert.equal(row.subject,"approved-cloud-subject");
      assert.ok(new Date(row.expires_at)-new Date(row.created_at)<=30*86400000);
      await assert.rejects(create);
      assert.equal((await db.query("SELECT count(*)::int AS n FROM public.sync_credentials")).rows[0].n,1);
    });
    await t.test("fresh token derives scope from the grant and preserves exact native user identity",async()=>{
      const response=await broker.fetch(request(grant.credential),env);assert.equal(response.status,200);
      assert.equal(response.headers.get("cache-control"),"no-store");
      const result=await response.json();
      const {payload}=await jwtVerify(result.token,keys,{issuer:config.issuer,audience:config.audience});
      assert.equal(payload.sub,grant.subject);assert.equal(payload.workspace_id,workspace);
      assert.equal(payload.installation_id,installation);assert.equal(payload.local_user_id,localUser);
      assert.equal(payload.credential_id,grant.credential_id);assert.equal(payload.exp-payload.iat,900);
      assert.equal(result.local_user_id,localUser);
      const validate=createNeonSessionValidator(connectionString,createSql);
      assert.equal(await validate({workspace_id:workspace,installation_id:installation,session_id:result.session_id}),true);
      assert.equal((await broker.fetch(request(grant.credential),env)).status,409); // No replacement/retry loop.
      await complete(result.session_id);
      const second=await (await broker.fetch(request(grant.credential),env)).json();
      assert.notEqual(second.session_id,result.session_id);assert.notEqual(second.token,result.token);
      await complete(second.session_id);
      assert.equal((await db.query("SELECT count(*)::int AS n FROM public.sync_sessions WHERE status='completed'")).rows[0].n,2);
    });
    await t.test("wrong credential, installation and native user cannot choose another scope",async()=>{
      for(const [credential,changed] of [[newSyncCredential(),scope],[grant.credential,{...scope,installation_id:other}],
        [grant.credential,{...scope,local_user_id:"1"}]])assert.equal((await broker.fetch(request(credential,changed),env)).status,401);
      for(const changed of [{...scope,workspace_id:other},{...scope,subject:"attacker"},{...scope,session_id:other},
        {...scope,local_user_id:9007199254740993},{...scope,local_user_id:"01"},{...scope,local_user_id:"1\n"},{...scope,installation_id:installation+"\n"}])
        assert.equal((await broker.fetch(request(grant.credential,changed),env)).status,400);
    });
    await t.test("expiry and device deactivation deny issuance without reserving sessions",async()=>{
      const count=(await db.query("SELECT count(*)::int AS n FROM public.sync_sessions")).rows[0].n;
      await db.exec("UPDATE public.sync_devices SET deactivated_at=clock_timestamp()");
      assert.equal((await broker.fetch(request(grant.credential),env)).status,401);
      await db.exec("UPDATE public.sync_devices SET deactivated_at=NULL; UPDATE public.sync_credentials SET created_at=clock_timestamp()-interval '2 days',expires_at=clock_timestamp()-interval '1 day'");
      assert.equal((await broker.fetch(request(grant.credential),env)).status,401);
      await db.exec("UPDATE public.sync_credentials SET created_at=clock_timestamp(),expires_at=clock_timestamp()+interval '30 days'");
      assert.equal((await db.query("SELECT count(*)::int AS n FROM public.sync_sessions")).rows[0].n,count);
    });
    await t.test("token lifetime cannot outlast the provisioned credential",async()=>{
      await db.exec("UPDATE public.sync_credentials SET expires_at=clock_timestamp()+interval '60 seconds' WHERE revoked_at IS NULL");
      const result=await (await broker.fetch(request(grant.credential),env)).json();
      const {payload}=await jwtVerify(result.token,keys,{issuer:config.issuer,audience:config.audience});
      const row=(await db.query("SELECT expires_at FROM public.sync_credentials WHERE id=$1",[grant.credential_id])).rows[0];
      assert.ok(payload.exp*1000<=new Date(row.expires_at).getTime());
      assert.ok(payload.exp-payload.iat<=60);
      await complete(result.session_id);
      await db.exec("UPDATE public.sync_credentials SET expires_at=clock_timestamp()+interval '30 days' WHERE revoked_at IS NULL");
    });
    await t.test("revocation fails running mapped sessions but preserves completed quota and unrelated attempts",async()=>{
      const unrelated=crypto.randomUUID();
      await db.query("SELECT public.reserve_authorized_sync_session($1,$2,$3)",[other,other,unrelated]);
      const running=await (await broker.fetch(request(grant.credential),env)).json();
      assert.ok(running.token);
      await assert.rejects(()=>runCredentialCommand(["revoke",other,installation,grant.credential_id],{NEON_DATABASE_URL:connectionString},createSql));
      assert.equal((await revoke()).revoked,true);assert.equal((await revoke()).revoked,true);
      const validate=createNeonSessionValidator(connectionString,createSql);
      assert.equal(await validate({workspace_id:workspace,installation_id:installation,session_id:running.session_id}),false);
      assert.equal((await broker.fetch(request(grant.credential),env)).status,401);
      assert.equal((await db.query("SELECT count(*)::int AS n FROM public.sync_sessions WHERE status='completed'")).rows[0].n,3);
      assert.equal((await db.query("SELECT status FROM public.sync_sessions WHERE id=$1",[running.session_id])).rows[0].status,"failed");
      assert.equal((await db.query("SELECT status FROM public.sync_sessions WHERE id=$1",[unrelated])).rows[0].status,"running");
      const rotated=await create();assert.notEqual(rotated.credential,grant.credential);
      grant=rotated;
      const renewed=await (await broker.fetch(request(grant.credential),env)).json();assert.ok(renewed.token);
      await complete(renewed.session_id);
    });
    await t.test("database quota still stops the eleventh completed exchange",async()=>{
      const used=(await db.query("SELECT count(*)::int AS n FROM public.sync_sessions WHERE status='completed'")).rows[0].n;
      for(let i=used;i<10;i++){const result=await (await broker.fetch(request(grant.credential),env)).json();assert.ok(result.token);await complete(result.session_id);}
      assert.equal((await broker.fetch(request(grant.credential),env)).status,409);
      assert.equal((await db.query("SELECT count(*)::int AS n FROM public.sync_sessions WHERE status='completed'")).rows[0].n,10);
    });
    await t.test("PUBLIC cannot read credential tables or execute credential routines",async()=>{
      await db.exec("CREATE ROLE broker_untrusted_test");
      for(const signature of ["register_sync_credential(uuid,uuid,uuid,bigint,text,text)","reserve_credential_sync_session(text,uuid,bigint,uuid)","revoke_sync_credential(uuid,uuid,uuid)"]){
        assert.equal((await db.query("SELECT has_function_privilege('broker_untrusted_test',$1,'EXECUTE') AS allowed",["public."+signature])).rows[0].allowed,false);
      }
      for(const table of ["sync_credentials","sync_credential_sessions"])
        assert.equal((await db.query("SELECT has_table_privilege('broker_untrusted_test',$1,'SELECT') AS allowed",["public."+table])).rows[0].allowed,false);
    });
    await t.test("lost reservation result fails only its generated attempt and never issues a token",async()=>{
      await db.exec("DELETE FROM public.sync_credential_sessions; DELETE FROM public.sync_sessions");
      const broken=await createCredentialIssuer(config,()=>({query:async(sql,params)=>{
        const rows=(await db.query(sql,params)).rows;return sql.includes("reserve_credential")?[{...rows[0],session_id:other}]:rows;
      }}));
      const digest=await credentialDigest(grant.credential);
      await assert.rejects(()=>broken({digest,...scope}));
      assert.deepEqual((await db.query("SELECT status FROM public.sync_sessions")).rows,[{status:"failed"}]);
    });
  }finally{await db.close();}
});

test("broker is closed by default and requires native HTTPS with mandatory throttling",async()=>{
  const secret=newSyncCredential();let calls=0;
  const env={SYNC_BROKER_ENABLED:"true",SYNC_BROKER_RATE_LIMITER:allow,SYNC_CREDENTIAL_ISSUER:async()=>{calls++;return {};}};
  assert.equal((await broker.fetch(request(secret),{...env,SYNC_BROKER_ENABLED:undefined})).status,503);
  assert.equal((await broker.fetch(request(secret),{...env,SYNC_BROKER_RATE_LIMITER:undefined})).status,503);
  assert.equal((await broker.fetch(request(secret),{...env,SYNC_BROKER_RATE_LIMITER:{limit:async()=>({success:false})}})).status,429);
  assert.equal((await broker.fetch(request(secret),{...env,SYNC_BROKER_RATE_LIMITER:{limit:async()=>{throw new Error("secret internal detail");}}})).status,503);
  assert.equal((await broker.fetch(request(secret,scope,{origin:"http://tauri.localhost"}),env)).status,403);
  assert.equal((await broker.fetch(new Request("http://broker.example.test/session",{method:"POST"}),env)).status,403);
  assert.equal((await broker.fetch(new Request("https://broker.example.test/session"),env)).status,405);
  assert.equal((await broker.fetch(new Request("https://broker.example.test/session?subject=attacker",{method:"POST"}),env)).status,404);
  assert.equal(calls,0);
  // Importing the ordinary sync Worker never exposes the issuer route.
  assert.equal((await syncWorker.fetch(new Request("https://sync.example.test/session",{method:"POST"}),env)).status,404);
  assert.equal(calls,0);
});

test("broker validates bounded request bodies before issuance and never logs raw credentials in limiter keys",async()=>{
  const secret=newSyncCredential();let calls=0;const keys=[];
  const env={SYNC_BROKER_ENABLED:"true",SYNC_BROKER_RATE_LIMITER:{limit:async({key})=>{keys.push(key);return {success:true};}},
    SYNC_CREDENTIAL_ISSUER:async()=>{calls++;return {};}};
  assert.equal((await broker.fetch(request("bad"),env)).status,401);
  assert.equal((await broker.fetch(request(secret,scope,{"content-type":"text/plain"}),env)).status,415);
  assert.equal((await broker.fetch(request(secret,{...scope,padding:"x".repeat(3000)}),env)).status,400);
  assert.equal((await broker.fetch(request(secret,{...scope,subject:"arbitrary"}),env)).status,400);
  assert.equal(calls,0);assert.ok(keys.every(key=>!key.includes(secret)));
  const response=await broker.fetch(request(secret),{...env,SYNC_CREDENTIAL_ISSUER:async()=>{throw new Error("database private-key secret");}});
  assert.equal(response.status,503);assert.ok(!(await response.text()).includes("private-key"));
});

test("credential administration rejects malformed mappings before database access",async()=>{
  assert.equal(parseCredentialCommand(["create",workspace,installation,localUser,"subject"]).local_user_id,localUser);
  for(const args of [["create",workspace,installation,"0","subject"],["create",workspace,installation,"9223372036854775808","subject"],
    ["create",workspace,installation,"1",""],["revoke",workspace,installation,"bad"],["create",workspace,installation,"1","subject","extra"]])
    assert.throws(()=>parseCredentialCommand(args));
});
