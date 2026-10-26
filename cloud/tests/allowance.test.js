import test from "node:test";
import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import {PGlite} from "@electric-sql/pglite";
import {createAllowanceReader} from "../db/allowance.js";
import {createSyncNowController} from "../../src/js/sync/sync_now.js";
import {validateAllowance} from "../../src/js/sync/allowance.js";
import worker from "../worker.js";
const scope={workspace_id:"11111111-1111-4111-8111-111111111111",installation_id:"22222222-2222-4222-8222-222222222222",session_id:"33333333-3333-4333-8333-333333333333"};
const other="44444444-4444-4444-8444-444444444444";
test("daily allowance and enforcement use per-device EAT calendar boundaries",async t=>{
  const db=await PGlite.create();
  try{
    for(const file of ["001_sync_workspaces.sql","002_sync_devices.sql","003_sync_sessions.sql","004_reserve_sync_session.sql","005_complete_sync_session.sql","006_fail_sync_session.sql","014_authorized_sync_allowance.sql"]){
      await db.exec(await readFile(new URL("../"+file,import.meta.url),"utf8"));
    }
    await db.query("INSERT INTO sync_workspaces(id,name) VALUES ($1,'A')",[scope.workspace_id]);
    await db.query("INSERT INTO sync_devices(installation_id,workspace_id) VALUES ($1,$2),($3,$2)",[scope.installation_id,scope.workspace_id,other]);
    const read=createAllowanceReader("postgresql://test:fake@ep-example.eu-central-1.aws.neon.tech/neondb",()=>({query:async(q,p)=>(await db.query(q,p)).rows}));
    const reserve=()=>db.query("SELECT reserve_sync_session($1,$2)",[scope.installation_id,scope.session_id]);
    const scenario=async(name,fn)=>t.test(name,async()=>{await db.exec("DELETE FROM sync_sessions");await fn();});
    await scenario("Ethiopia midnight is 21:00 UTC without using client time",async()=>{
      const before=(await db.query("SELECT * FROM sync_day_bounds('2026-10-26T20:59:59Z')")).rows[0];
      const after=(await db.query("SELECT * FROM sync_day_bounds('2026-10-26T21:00:00Z')")).rows[0];
      assert.equal(before.day_start.toISOString(),"2026-10-25T21:00:00.000Z");
      assert.equal(before.next_day_start.toISOString(),"2026-10-26T21:00:00.000Z");
      assert.equal(after.day_start.toISOString(),"2026-10-26T21:00:00.000Z");
    });
    await scenario("the tenth reserved session is allowed and completed retries count once",async()=>{
      for(let i=0;i<9;i++)await db.query("INSERT INTO sync_sessions(id,installation_id,started_at,completed_at,status) VALUES ($1,$2,now()-interval '1 second',now(),'completed')",[crypto.randomUUID(),scope.installation_id]);
      await reserve();let allowance=await read(scope);
      assert.equal(allowance.completed_today,9);assert.equal(allowance.reserved_today,1);assert.equal(allowance.remaining_today,0);assert.equal(allowance.session_permitted,true);
      for(let i=0;i<2;i++)await db.query("SELECT complete_sync_session($1,$2)",[scope.installation_id,scope.session_id]);
      allowance=await read(scope);assert.equal(allowance.completed_today,10);assert.equal(allowance.reserved_today,0);assert.equal(allowance.session_permitted,false);
      await assert.rejects(()=>db.query("SELECT reserve_sync_session($1,$2)",[scope.installation_id,crypto.randomUUID()]),/daily sync limit/);
    });
    await scenario("previous-day and failed sessions do not consume today's device allowance",async()=>{
      await db.query("INSERT INTO sync_sessions(id,installation_id,started_at,completed_at,status) SELECT $1,$2,b.day_start-interval '2 minutes',b.day_start-interval '1 minute','completed' FROM sync_day_bounds(clock_timestamp()) b",[crypto.randomUUID(),scope.installation_id]);
      await db.query("INSERT INTO sync_sessions(id,installation_id,status) VALUES ($1,$2,'failed')",[crypto.randomUUID(),scope.installation_id]);
      await db.query("INSERT INTO sync_sessions(id,installation_id,started_at,completed_at,status) VALUES ($1,$2,now()-interval '1 second',now(),'completed')",[crypto.randomUUID(),other]);
      await reserve();const allowance=await read(scope);
      assert.equal(allowance.completed_today,0);assert.equal(allowance.remaining_today,9);
    });
    await scenario("stale sessions are released and foreign scopes fail closed",async()=>{
      await db.query("INSERT INTO sync_sessions(id,installation_id,started_at) VALUES ($1,$2,now()-interval '16 minutes')",[crypto.randomUUID(),scope.installation_id]);
      await reserve();assert.equal((await read(scope)).session_permitted,true);
      await assert.rejects(()=>read({...scope,installation_id:other}),e=>e.code==="42501");
      const env={SYNC_AUTH_VERIFIER:async()=>({...scope,authorized:true,session_authorized:true,session_workspace_id:scope.workspace_id,session_installation_id:scope.installation_id}),SYNC_ALLOWANCE_ENABLED:"true",SYNC_ALLOWANCE_READER:read};
      const request=query=>new Request("https://sync.example.test/sync/allowance"+query,{headers:{authorization:"Bearer test"}});
      assert.equal((await worker.fetch(request(""),env)).status,200);
      assert.equal((await worker.fetch(request("?installation_id="+other),env)).status,400);
      assert.equal((await worker.fetch(request(""),{...env,SYNC_ALLOWANCE_ENABLED:undefined})).status,501);
    });
  }finally{await db.close();}
});

test("the manual controller refuses an unpermitted session before business transfer",async()=>{
  const token="e30."+Buffer.from(JSON.stringify({installation_id:scope.installation_id})).toString("base64url")+".test";
  const cfg={token};for(const [key,path]of Object.entries({uploadEndpoint:"upload",pullEndpoint:"pull",completeEndpoint:"complete",failEndpoint:"fail",allowanceEndpoint:"allowance"}))cfg[key]="https://sync.example.test/sync/"+path;
  const calls=[];let ended=false;
  const controller=createSyncNowController({getUserKey:()=>1,bridge:{configuration:async()=>cfg,begin:async()=>({exchange_id:"lease",installation_id:scope.installation_id}),end:async()=>{ended=true;},readPending:async()=>{throw new Error("must not read outbox");}},
    fetchImpl:async url=>{calls.push(new URL(url).pathname);return new Response(JSON.stringify(new URL(url).pathname.endsWith("allowance")?
      {daily_limit:10,completed_today:10,reserved_today:0,remaining_today:0,session_permitted:false,day:"2026-10-26",reset_at:"2026-10-26T21:00:00Z"}:{accepted:false}),{headers:{"content-type":"application/json"}});}});
  await assert.rejects(()=>controller.run());assert.equal(ended,true);assert.deepEqual(calls,["/sync/allowance","/sync/fail"]);
});

test("malformed allowance cannot claim another synchronization slot",()=>{
  assert.throws(()=>validateAllowance({daily_limit:10,completed_today:10,reserved_today:1,remaining_today:1,session_permitted:true,day:"2026-10-26",reset_at:"2026-10-26T21:00:00Z"}));
});
