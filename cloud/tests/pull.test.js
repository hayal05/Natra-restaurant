import test from "node:test";
import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import {PGlite} from "@electric-sql/pglite";
import {createEventPuller} from "../db/pull.js";
import {downloadPage} from "../../src/js/sync/cloud_download.js";
import {validateDownloadPage,MAX_DOWNLOAD_BYTES} from "../../src/js/sync/download_protocol.js";
import worker from "../worker.js";
const scope={workspace_id:"11111111-1111-4111-8111-111111111111",installation_id:"22222222-2222-4222-8222-222222222222",session_id:"33333333-3333-4333-8333-333333333333"};
const other="44444444-4444-4444-8444-444444444444";
const connection="postgresql://test:fake@ep-example.eu-central-1.aws.neon.tech/neondb";
const identity={...scope,authorized:true,session_authorized:true,session_workspace_id:scope.workspace_id,session_installation_id:scope.installation_id};
const env=pull=>({SYNC_AUTH_VERIFIER:async()=>identity,SYNC_SESSION_VALIDATOR:async()=>true,SYNC_PULL_ENABLED:"true",SYNC_EVENT_PULLER:pull});
const req=query=>new Request("https://sync.example.test/sync/pull"+query,{headers:{authorization:"Bearer test"}});
const event=(sequence="1")=>({sequence_id:sequence,event_id:crypto.randomUUID(),origin_installation_id:scope.installation_id,
  entity_type:"categories",entity_id:crypto.randomUUID(),operation:"upsert",payload:{name:"Test",origin_installation_id:scope.installation_id}});
const json=page=>new Response(JSON.stringify(page),{status:200,headers:{"content-type":"application/json"}});
const clientConfig=overrides=>({endpoint:"https://sync.example.test/sync/pull",token:"test",...overrides});

test("authorized incremental pull runs actual PostgreSQL scope and pagination rules",async t=>{
  const db=await PGlite.create();
  try {
    for(const file of ["001_sync_workspaces.sql","002_sync_devices.sql","003_sync_sessions.sql","007_sync_events.sql","008_upload_sync_event.sql","012_pull_authorized_sync_events.sql"]) {
      await db.exec(await readFile(new URL("../"+file,import.meta.url),"utf8"));
    }
    await db.query("INSERT INTO sync_workspaces(id,name) VALUES ($1,'A'),($2,'B')",[scope.workspace_id,other]);
    await db.query("INSERT INTO sync_devices(installation_id,workspace_id) VALUES ($1,$2),($3,$3)",[scope.installation_id,scope.workspace_id,other]);
    await db.query("INSERT INTO sync_sessions(id,installation_id) VALUES ($1,$2)",[scope.session_id,scope.installation_id]);
    const sql={query:async(query,params,options)=>{
      assert.ok(options.fetchOptions.signal instanceof AbortSignal);
      return (await db.query(query,params)).rows;
    }};
    const pull=createEventPuller(connection,()=>sql);
    const add=async(payload={name:"Test"},workspace=scope.workspace_id,installation=scope.installation_id)=>{
      const e=event();
      const result=await db.query("SELECT upload_sync_event($1,$2,$3,'categories',$4,'upsert',$5::jsonb)::text AS sequence_id",
        [workspace,installation,e.event_id,e.entity_id,JSON.stringify(payload)]);
      return {...e,payload,sequence_id:result.rows[0].sequence_id,origin_installation_id:installation};
    };
    const scenario=async(name,fn)=>t.test(name,async()=>{
      await db.exec("DELETE FROM sync_events; UPDATE sync_devices SET deactivated_at=NULL; UPDATE sync_sessions SET status='running',completed_at=NULL,started_at=now()-interval '1 minute'");
      await fn();
    });

    await scenario("ordered pages return only the authorized workspace and do not skip lookahead",async()=>{
      const first=await add();
      await add({name:"Foreign"},other,other);
      const second=await add();
      const third=await add();
      const page=await pull(scope,"0",2);
      assert.deepEqual(page.events.map(e=>e.event_id),[first.event_id,second.event_id]);
      assert.equal(page.next_cursor,second.sequence_id);
      assert.equal(page.has_more,true);
      const tail=await pull(scope,page.next_cursor,2);
      assert.deepEqual(tail.events.map(e=>e.event_id),[third.event_id]);
      assert.equal(tail.has_more,false);
      assert.deepEqual(await pull(scope,tail.next_cursor,2),{events:[],next_cursor:third.sequence_id,has_more:false});
      // Own events remain in the ordered stream. SQLite's outbox-echo guard
      // skips their financial effect while recording the receipt/cursor.
      assert.equal(page.events[0].origin_installation_id,scope.installation_id);
    });

    await scenario("revocation, expiry and foreign scopes fail inside the read transaction",async()=>{
      await add();
      for(const mutation of ["UPDATE sync_sessions SET status='failed'",
        "UPDATE sync_sessions SET status='completed',completed_at=now()",
        "UPDATE sync_sessions SET started_at=now()-interval '16 minutes'",
        "UPDATE sync_sessions SET started_at=now()+interval '1 minute'",
        "UPDATE sync_devices SET deactivated_at=now() WHERE installation_id='"+scope.installation_id+"'"]) {
        await db.exec(mutation);
        await assert.rejects(()=>pull(scope,"0",10),error=>error.code==="42501");
        await db.exec("UPDATE sync_devices SET deactivated_at=NULL; UPDATE sync_sessions SET status='running',completed_at=NULL,started_at=now()-interval '1 minute'");
      }
      for(const changed of [{...scope,workspace_id:other},{...scope,installation_id:other},{...scope,session_id:other}]) {
        await assert.rejects(()=>pull(changed,"0",10));
      }
      const config=env(pull);
      config.SYNC_SESSION_VALIDATOR=async()=>{await db.exec("UPDATE sync_sessions SET status='failed'");return true;};
      assert.equal((await worker.fetch(req(""),config)).status,403);
    });

    await scenario("large PostgreSQL sequences remain exact across API and desktop callbacks",async()=>{
      await db.exec("ALTER TABLE sync_events ALTER COLUMN sequence_id RESTART WITH 9007199254740993");
      const e=await add();
      let cursor="0";
      const result=await downloadPage(clientConfig({readCursor:async()=>cursor,limit:1,
        fetchImpl:(url,options)=>worker.fetch(new Request(url,options),env(pull)),
        applyPage:async events=>{assert.equal(events[0].sequence_id,"9007199254740993");cursor=events.at(-1).sequence_id;}}));
      assert.equal(cursor,e.sequence_id);
      assert.deepEqual(result,{downloaded:1,next_cursor:e.sequence_id,has_more:false});
    });

    await scenario("byte-bounded pages retain the next event instead of advancing past it",async()=>{
      const records=[];
      for(let i=0;i<3;i++) records.push(await add({text:"x".repeat(800000)}));
      const page=await pull(scope,"0",500);
      assert.equal(page.events.length,2);
      assert.equal(page.has_more,true);
      const last=await pull(scope,page.next_cursor,500);
      assert.deepEqual(last.events.map(e=>e.event_id),[records[2].event_id]);
    });

    await scenario("PUBLIC has no pull execution grant",async()=>{
      await db.exec("CREATE ROLE untrusted_pull_test");
      const r=await db.query("SELECT has_function_privilege('untrusted_pull_test','public.pull_authorized_sync_events(uuid,uuid,uuid,bigint,integer)','EXECUTE') AS allowed");
      assert.equal(r.rows[0].allowed,false);
    });
  } finally {await db.close();}
});

test("pull API rejects injected scopes, duplicate parameters and imprecise cursors",async()=>{
  let calls=0;
  for(const query of ["?after=-1","?after=01","?after=9007199254740993.0","?after=9223372036854775808",
    "?limit=0","?limit=501","?after=0&after=1","?workspace_id="+other,"?after=1e3"]) {
    const response=await worker.fetch(req(query),env(async()=>{calls++;}));
    assert.equal(response.status,400);
  }
  assert.equal(calls,0);
  for(const flag of [undefined,true,false,"false","TRUE"]) {
    const response=await worker.fetch(req(""),{...env(async()=>{calls++;}),SYNC_PULL_ENABLED:flag});
    assert.equal(response.status,501);
  }
  assert.equal(calls,0);
});

test("malformed and unordered pages cannot reach local page application",async()=>{
  const first=event("10"),second=event("11");
  const base={events:[first,second],next_cursor:"11",has_more:false};
  const invalid=[{...base,next_cursor:"12"},{...base,events:[second,first]},
    {...base,events:[first,{...second,event_id:first.event_id}]},
    {...base,events:[first,{...second,sequence_id:11}]},
    {...base,events:[],has_more:true,next_cursor:"0"},
    {...base,events:[{...first,origin_installation_id:"bad"}],next_cursor:"10"},
    {...base,events:[{...first,payload:{origin_installation_id:other}}],next_cursor:"10"}];
  let applied=0;
  for(const page of invalid) {
    await assert.rejects(()=>downloadPage(clientConfig({readCursor:async()=>"0",applyPage:async()=>{applied++;},fetchImpl:async()=>json(page)})),/retain the durable cursor/);
  }
  assert.equal(applied,0);
  assert.throws(()=>validateDownloadPage(base,"10",100));
});

test("failed local page apply retries from the same durable cursor",async()=>{
  const e=event("7");
  let cursor="0",attempts=0;
  const urls=[];
  const config=clientConfig({readCursor:async()=>cursor,
    fetchImpl:async(url,options)=>{
      urls.push(url);assert.equal(options.redirect,"error");assert.equal(options.credentials,"omit");
      return json({events:[e],next_cursor:"7",has_more:false});
    },applyPage:async()=>{if(++attempts===1)throw new Error("rolled back");cursor="7";}});
  await assert.rejects(()=>downloadPage(config));
  assert.equal(cursor,"0");
  assert.deepEqual(await downloadPage(config),{downloaded:1,next_cursor:"7",has_more:false});
  assert.equal(urls[0],urls[1]);
  assert.equal(cursor,"7");
});

test("empty pages preserve the cursor and oversized/errors never apply",async()=>{
  let applied=0;
  const base=clientConfig({readCursor:async()=>"9007199254740993",applyPage:async()=>{applied++;}});
  assert.deepEqual(await downloadPage({...base,fetchImpl:async()=>json({events:[],next_cursor:"9007199254740993",has_more:false})}),
    {downloaded:0,next_cursor:"9007199254740993",has_more:false});
  for(const fetchImpl of [async()=>new Response(" ".repeat(MAX_DOWNLOAD_BYTES+1),{headers:{"content-type":"application/json"}}),
    async()=>{throw new Error("private connection details");},async()=>new Response("private server details",{status:503})]) {
    await assert.rejects(()=>downloadPage({...base,fetchImpl}),error=>{assert.doesNotMatch(error.message,/private/);return true;});
  }
  assert.equal(applied,0);
  await assert.rejects(()=>downloadPage({...base,endpoint:"http://sync.example.test/sync/pull"}));
  await assert.rejects(()=>downloadPage({...base,readCursor:async()=>9007199254740992}));
  const html=await readFile(new URL("../../src/index.html",import.meta.url),"utf8");
  assert.doesNotMatch(html,/cloud_download\.js/);
});

test("a successful callback without a durable cursor commit is rejected",async()=>{
  const e=event("2");
  await assert.rejects(()=>downloadPage(clientConfig({readCursor:async()=>"0",applyPage:async()=>{},
    fetchImpl:async()=>json({events:[e],next_cursor:"2",has_more:false})})),/retain the durable cursor/);
});
