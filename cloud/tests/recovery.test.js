import test from "node:test";
import assert from "node:assert/strict";
import {createSyncNowController} from "../../src/js/sync/sync_now.js";
import {uploadPendingBatch} from "../../src/js/sync/cloud_upload.js";
import {downloadPage} from "../../src/js/sync/cloud_download.js";
import {finishSession} from "../../src/js/sync/session_control.js";
import {readAllowance} from "../../src/js/sync/allowance.js";
const installation="22222222-2222-4222-8222-222222222222";
const token="e30."+Buffer.from(JSON.stringify({installation_id:installation})).toString("base64url")+".test";
const config={token};for(const [key,path]of Object.entries({uploadEndpoint:"upload",pullEndpoint:"pull",completeEndpoint:"complete",failEndpoint:"fail",allowanceEndpoint:"allowance"}))config[key]="https://sync.example.test/sync/"+path;
const allowance={daily_limit:10,completed_today:0,reserved_today:1,remaining_today:9,session_permitted:true,day:"2026-10-26",reset_at:"2026-10-26T21:00:00Z"};
const event={event_id:"11111111-1111-4111-8111-111111111111",entity_type:"categories",entity_id:"local:recover",operation:"upsert",payload:{name:"Recover"}};
const json=body=>new Response(JSON.stringify(body),{headers:{"content-type":"application/json"}});

test("offline availability and reconnection do not automatically start an exchange",async()=>{
  let online=false,network=0,begins=0;const states=[];
  const controller=createSyncNowController({getUserKey:()=>1,isOnline:()=>online,onState:(...state)=>states.push(state),
    bridge:{configuration:async()=>config,begin:async()=>{begins++;}},fetchImpl:async()=>{network++;}});
  await controller.refresh();assert.deepEqual(await controller.run(),{ran:false,pushed:0,pulled:0});
  assert.deepEqual(states.at(-1),["offline",false]);assert.equal(begins,0);assert.equal(network,0);
  online=true;await controller.refresh();assert.deepEqual(states.at(-1),["ready",true]);assert.equal(network,0);assert.equal(begins,0);
});

test("disconnect cancels an in-flight upload and preserves pending data for manual retry",async()=>{
  let online=true,pending=[event],cursor="0",ended=0,network=0;
  let started;const uploading=new Promise(resolve=>{started=resolve;});
  const states=[];
  const bridge={configuration:async()=>config,begin:async()=>({exchange_id:"lease",installation_id:installation}),
    readPending:async()=>pending,acknowledge:async(id,ids)=>{pending=pending.filter(e=>!ids.includes(e.event_id));},
    readCursor:async()=>cursor,applyPage:async()=>{throw new Error("unexpected apply");},end:async()=>{ended++;}};
  const controller=createSyncNowController({bridge,getUserKey:()=>1,isOnline:()=>online,onState:(...state)=>states.push(state),fetchImpl:async(url,options)=>{
    network++;
    if(url.endsWith("allowance"))return json(allowance);
    if(url.endsWith("upload")){started();return new Promise((resolve,reject)=>options.signal.addEventListener("abort",()=>reject(new Error("cancelled private URL")),{once:true}));}
    throw new Error("offline must not contact other endpoints");
  }});
  const running=controller.run();await uploading;online=false;controller.invalidate();
  await assert.rejects(()=>running,/did not finish/);
  assert.equal(pending.length,1);assert.equal(cursor,"0");assert.equal(ended,1);assert.equal(network,2);
  assert.deepEqual(states.at(-1),["offline",false]);
  online=true;await controller.refresh();assert.equal(network,2);
  // Separate explicit retry, using unchanged IDs and an authorized session.
  const bodies=[];
  const retry=createSyncNowController({bridge,getUserKey:()=>1,isOnline:()=>true,fetchImpl:async(url,options)=>{
    const path=new URL(url).pathname;
    if(path.endsWith("allowance"))return json(allowance);
    if(path.endsWith("upload")){bodies.push(options.body);return json({acknowledged_event_ids:[event.event_id],receipts:[{event_id:event.event_id,sequence_id:"1"}]});}
    if(path.endsWith("pull"))return json({events:[],next_cursor:"0",has_more:false});
    return json({accepted:true});
  }});
  assert.deepEqual(await retry.run(),{ran:true,pushed:1,pulled:0});
  assert.equal(JSON.parse(bodies[0]).events[0].event_id,event.event_id);assert.equal(pending.length,0);assert.equal(ended,2);
});

test("already cancelled requests perform no network I/O and never acknowledge",async()=>{
  const controller=new AbortController();controller.abort();let calls=0,acks=0;
  const fetchImpl=async()=>{calls++;};
  await assert.rejects(()=>uploadPendingBatch({endpoint:config.uploadEndpoint,token,installationId:installation,
    readPending:async()=>[event],acknowledge:async()=>{acks++;},fetchImpl,signal:controller.signal}));
  await assert.rejects(()=>downloadPage({endpoint:config.pullEndpoint,token,readCursor:async()=>"0",applyPage:async()=>{acks++;},fetchImpl,signal:controller.signal}));
  await assert.rejects(()=>finishSession({endpoint:config.completeEndpoint,token,action:"complete",fetchImpl,signal:controller.signal}));
  await assert.rejects(()=>readAllowance({endpoint:config.allowanceEndpoint,token,fetchImpl,signal:controller.signal}));
  assert.equal(calls,0);assert.equal(acks,0);
});
