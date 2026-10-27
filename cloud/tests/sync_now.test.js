import test from "node:test";
import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import {PGlite} from "@electric-sql/pglite";
import {createSessionFinisher} from "../db/session-control.js";
import {createSyncNowController} from "../../src/js/sync/sync_now.js";
import {setHeaderSyncStatus} from "../../src/js/components/header.js";
import worker from "../worker.js";
const scope={workspace_id:"11111111-1111-4111-8111-111111111111",installation_id:"22222222-2222-4222-8222-222222222222",session_id:"33333333-3333-4333-8333-333333333333"};
const other="44444444-4444-4444-8444-444444444444";
const identity={...scope,authorized:true,session_authorized:true,session_workspace_id:scope.workspace_id,session_installation_id:scope.installation_id};
const cfg={token:"e30."+Buffer.from(JSON.stringify({installation_id:scope.installation_id})).toString("base64url")+".test",uploadEndpoint:"https://sync.example.test/sync/upload",pullEndpoint:"https://sync.example.test/sync/pull",completeEndpoint:"https://sync.example.test/sync/complete",failEndpoint:"https://sync.example.test/sync/fail",allowanceEndpoint:"https://sync.example.test/sync/allowance"};
const json=body=>new Response(JSON.stringify(body),{headers:{"content-type":"application/json"}});
const localEvent=()=>({event_id:crypto.randomUUID(),entity_type:"categories",entity_id:crypto.randomUUID(),operation:"upsert",payload:{name:"Local",origin_installation_id:scope.installation_id}});
function harness(){
  const events=[localEvent(),localEvent()];
  let pending=[...events],cursor="0",owner=1;
  const log=[],states=[];
  const bridge={configuration:async()=>cfg,begin:async()=>{log.push("begin");return {exchange_id:"lease",installation_id:scope.installation_id};},
    readPending:async()=>pending,
    acknowledge:async(id,ids)=>{log.push("ack");pending=pending.filter(e=>!ids.includes(e.event_id));},
    readCursor:async()=>cursor,
    applyPage:async(id,after,page)=>{assert.equal(after,cursor);log.push("apply");cursor=page.at(-1).sequence_id;},
    end:async()=>{log.push("end");}};
  const fetchImpl=async(url,options)=>{
    const path=new URL(url).pathname;log.push(path);
    if(path==="/sync/allowance")return json({daily_limit:10,completed_today:0,reserved_today:1,remaining_today:9,session_permitted:true,day:"2026-10-26",reset_at:"2026-10-26T21:00:00Z"});
    if(path==="/sync/upload"){
      const sent=JSON.parse(options.body).events;
      return json({acknowledged_event_ids:sent.map(e=>e.event_id),receipts:sent.map((e,i)=>({event_id:e.event_id,sequence_id:String(i+1)}))});
    }
    if(path==="/sync/pull")return json({events:[{...localEvent(),event_id:crypto.randomUUID(),origin_installation_id:scope.installation_id,sequence_id:"9007199254740993"}],next_cursor:"9007199254740993",has_more:false});
    return json({accepted:true});
  };
  const controller=createSyncNowController({bridge,getUserKey:()=>owner,onState:(...state)=>states.push(state),fetchImpl});
  return {bridge,controller,fetchImpl,log,states,events,getPending:()=>pending,getCursor:()=>cursor,getOwner:()=>owner,setOwner:value=>{owner=value;}};
}

test("manual coordinator uploads, applies and confirms completion in order",async()=>{
  const h=harness();await h.controller.refresh();assert.equal(h.log.length,0);
  const result=await h.controller.run();
  assert.deepEqual(result,{ran:true,pushed:2,pulled:1});
  assert.deepEqual(h.log,["begin","/sync/allowance","/sync/upload","ack","/sync/pull","apply","/sync/complete","/sync/allowance","end"]);
  assert.equal(h.getPending().length,0);assert.equal(h.getCursor(),"9007199254740993");
  assert.deepEqual(h.states.at(-1),["completed",true]);
});

test("unconfigured release never starts an exchange or network request",async()=>{
  const h=harness();h.bridge.configuration=async()=>null;
  await h.controller.refresh();assert.deepEqual(await h.controller.run(),{ran:false,pushed:0,pulled:0});
  assert.deepEqual(h.log,[]);assert.deepEqual(h.states.at(-1),["ready",false]);
});

test("overlap is rejected and failed pull keeps upload receipts durable",async()=>{
  const h=harness();let release;h.bridge.configuration=()=>new Promise(resolve=>{release=()=>resolve(cfg);});
  const first=h.controller.run();await assert.rejects(()=>h.controller.run(),/already in progress/);release();await first;
  const broken=harness();broken.bridge.applyPage=async()=>{throw new Error("rolled back");};
  await assert.rejects(()=>broken.controller.run(),/did not finish/);
  assert.equal(broken.getPending().length,0);assert.equal(broken.getCursor(),"0");
  assert.ok(broken.log.includes("/sync/fail"));assert.equal(broken.log.at(-1),"end");
  assert.ok(!broken.log.includes("/sync/complete"));assert.deepEqual(broken.states.at(-1),["failed",true]);
});

test("logout during request prevents local acknowledgement and releases lease",async()=>{
  const h=harness();
  const c=createSyncNowController({bridge:h.bridge,getUserKey:h.getOwner,
    fetchImpl:async(url,options)=>{h.setOwner(null);c.invalidate();return h.fetchImpl(url,options);}});
  await assert.rejects(()=>c.run());
  assert.equal(h.getPending().length,2);assert.equal(h.getCursor(),"0");
  assert.equal(h.log.at(-1),"end");assert.ok(!h.log.includes("ack"));
});

test("header states keep busy and unavailable actions disabled with correct labels",()=>{
  const button={dataset:{},setAttribute(name,value){this[name]=value;}};
  const shell={querySelector:()=>button};
  setHeaderSyncStatus(shell,"ready",false);assert.equal(button.disabled,true);
  setHeaderSyncStatus(shell,"syncing",true);assert.equal(button.disabled,true);assert.equal(button["aria-busy"],"true");
  setHeaderSyncStatus(shell,"failed",true);assert.equal(button.disabled,false);assert.equal(button["aria-label"],"Retry Sync");
  setHeaderSyncStatus(shell,"completed",true);assert.equal(button["aria-label"],"Synced");
});

test("CORS preflight permits only pinned origins, routes, methods and headers",async()=>{
  const origin="http://tauri.localhost";
  const env={SYNC_ALLOWED_ORIGINS:JSON.stringify([origin])};
  const preflight=(from,method="POST",headers="authorization,content-type")=>new Request(cfg.uploadEndpoint,{method:"OPTIONS",headers:{origin:from,"access-control-request-method":method,"access-control-request-headers":headers}});
  const allowed=await worker.fetch(preflight(origin),env);assert.equal(allowed.status,204);
  assert.equal(allowed.headers.get("access-control-allow-origin"),origin);
  assert.equal(allowed.headers.get("access-control-allow-credentials"),null);
  for(const response of [await worker.fetch(preflight("https://attacker.test"),env),
    await worker.fetch(preflight(origin,"GET"),env),await worker.fetch(preflight(origin,"POST","x-workspace-id"),env),
    await worker.fetch(preflight(origin),{}),await worker.fetch(preflight("null"),{SYNC_ALLOWED_ORIGINS:'["null"]'}),
    await worker.fetch(preflight(origin),{SYNC_ALLOWED_ORIGINS:'["*"]'})])assert.equal(response.status,403);
  const unavailable=await worker.fetch(new Request(cfg.uploadEndpoint,{method:"POST",headers:{origin}}),env);
  assert.equal(unavailable.status,503);assert.equal(unavailable.headers.get("access-control-allow-origin"),origin);
});

test("terminal session API is scoped, idempotent and never reverses completed quota",async t=>{
  const db=await PGlite.create();
  try{
    for(const file of ["001_sync_workspaces.sql","002_sync_devices.sql","003_sync_sessions.sql","004_reserve_sync_session.sql","005_complete_sync_session.sql","006_fail_sync_session.sql","013_finish_authorized_sync_session.sql"]){
      await db.exec(await readFile(new URL("../"+file,import.meta.url),"utf8"));
    }
    await db.query("INSERT INTO sync_workspaces(id,name) VALUES ($1,'A'),($2,'B')",[scope.workspace_id,other]);
    await db.query("INSERT INTO sync_devices(installation_id,workspace_id) VALUES ($1,$2)",[scope.installation_id,scope.workspace_id]);
    const finish=createSessionFinisher("postgresql://test:fake@ep-example.eu-central-1.aws.neon.tech/neondb",()=>({query:async(q,p)=>(await db.query(q,p)).rows}));
    const config={SYNC_AUTH_VERIFIER:async()=>identity,SYNC_SESSION_CONTROL_ENABLED:"true",SYNC_SESSION_FINISHER:finish};
    const request=action=>new Request("https://sync.example.test/sync/"+action,{method:"POST",headers:{authorization:"Bearer test"}});
    await db.query("SELECT reserve_sync_session($1,$2)",[scope.installation_id,scope.session_id]);
    await t.test("completion retry reaches already completed session without recounting",async()=>{
      for(let i=0;i<2;i++){const r=await worker.fetch(request("complete"),config);assert.equal(r.status,200);assert.deepEqual(await r.json(),{accepted:true});}
      assert.deepEqual(await (await worker.fetch(request("fail"),config)).json(),{accepted:false});
      assert.equal((await db.query("SELECT count(*)::integer AS n FROM sync_sessions WHERE status='completed'")).rows[0].n,1);
    });
    await t.test("foreign scope cannot control a session",async()=>{
      await assert.rejects(()=>finish({...scope,workspace_id:other},"fail"),e=>e.code==="42501");
    });
    await t.test("expired completion fails the session without consuming quota",async()=>{
      await db.exec("UPDATE sync_sessions SET status='running',completed_at=NULL,started_at=now()-interval '16 minutes'");
      assert.deepEqual(await finish(scope,"complete"),{accepted:false});
      assert.equal((await db.query("SELECT status FROM sync_sessions")).rows[0].status,"failed");
    });
    await t.test("control flags stay closed and caller-controlled request scope is rejected",async()=>{
      assert.equal((await worker.fetch(request("complete"),{...config,SYNC_SESSION_CONTROL_ENABLED:undefined})).status,501);
      const injected=new Request(cfg.completeEndpoint,{method:"POST",headers:{authorization:"Bearer test"},body:JSON.stringify(scope)});
      assert.equal((await worker.fetch(injected,config)).status,400);
      const empty=new Request(cfg.completeEndpoint,{method:"POST",headers:{authorization:"Bearer test"},body:""});
      assert.equal((await worker.fetch(empty,config)).status,200);
    });
  }finally{await db.close();}
});

test("a manual click drains an initial backlog larger than one batch",async()=>{
  const h=harness();let pending=Array.from({length:602},localEvent);
  h.bridge.readPending=async()=>pending.slice(0,500);
  h.bridge.acknowledge=async(id,ids)=>{pending=pending.filter(e=>!ids.includes(e.event_id));};
  const c=createSyncNowController({bridge:h.bridge,getUserKey:()=>1,fetchImpl:h.fetchImpl});
  const result=await c.run();assert.equal(result.pushed,602);assert.equal(pending.length,0);
  assert.equal(h.log.filter(path=>path==="/sync/upload").length,2);
});

test("missing completion confirmation never reports Synced and still releases the lease",async()=>{
  const h=harness();const states=[];
  const c=createSyncNowController({bridge:h.bridge,getUserKey:()=>1,onState:(...state)=>states.push(state),
    fetchImpl:async(url,options)=>new URL(url).pathname==="/sync/complete"?json({accepted:false}):h.fetchImpl(url,options)});
  await assert.rejects(()=>c.run());assert.ok(!states.some(([state])=>state==="completed"));
  assert.equal(h.log.at(-1),"end");assert.ok(h.log.includes("/sync/fail"));
});

test("a provisioned token for another installation never touches network or data",async()=>{
  const h=harness();h.bridge.configuration=async()=>({...cfg,token:"e30."+Buffer.from(JSON.stringify({installation_id:other})).toString("base64url")+".test"});
  await assert.rejects(()=>h.controller.run());assert.deepEqual(h.log,["begin","end"]);
  assert.equal(h.getPending().length,2);assert.equal(h.getCursor(),"0");
});


test("broker availability refresh never requests credentials and each explicit exchange delivers after begin",async()=>{
  const h=harness();const {token,...metadata}=cfg;
  h.bridge.configuration=async()=>({...metadata,broker:true});
  h.bridge.credentials=async id=>{assert.equal(id,"lease");h.log.push("credentials");return {...cfg,broker:true};};
  await h.controller.refresh();assert.deepEqual(h.log,[]);
  await h.controller.run();
  assert.deepEqual(h.log.slice(0,3),["begin","credentials","/sync/allowance"]);
  assert.equal(h.getPending().length,0);
});
test("broker delivery failure, changed origin or logout cannot transfer business data",async()=>{
  for(const failure of ["reject","origin","logout"]){
    const h=harness();const {token,...metadata}=cfg;
    h.bridge.configuration=async()=>({...metadata,broker:true});
    h.bridge.credentials=async()=>{
      if(failure==="reject")throw new Error("native credentials unavailable");
      if(failure==="logout"){h.setOwner(null);h.controller.invalidate();return cfg;}
      return {...cfg,uploadEndpoint:"https://unapproved.example.test/sync/upload"};
    };
    await assert.rejects(()=>h.controller.run());
    assert.equal(h.getPending().length,2);assert.equal(h.getCursor(),"0");
    assert.deepEqual(h.log,["begin","end"]);
  }
});
