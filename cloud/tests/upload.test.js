import test from "node:test";
import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import {PGlite} from "@electric-sql/pglite";
import {createBatchUploader,validateUploadBatch,MAX_UPLOAD_BYTES} from "../db/upload.js";
import worker from "../worker.js";

const scope = {workspace_id:"11111111-1111-4111-8111-111111111111",
  installation_id:"22222222-2222-4222-8222-222222222222",session_id:"33333333-3333-4333-8333-333333333333"};
const other = "44444444-4444-4444-8444-444444444444";
const connection = "postgresql://test:fake@ep-example.eu-central-1.aws.neon.tech/neondb";
const event = () => ({event_id:crypto.randomUUID(),entity_type:"categories",entity_id:crypto.randomUUID(),
  operation:"upsert",payload:{origin_installation_id:scope.installation_id,category:{name:"Test"}}});
const identity = {...scope,authorized:true,session_authorized:true,
  session_workspace_id:scope.workspace_id,session_installation_id:scope.installation_id};
const request = (body,headers={}) => new Request("https://sync.example.test/sync/upload",{
  method:"POST",headers:{authorization:"Bearer test","content-type":"application/json",...headers},
  body:typeof body === "string" ? body : JSON.stringify(body),
});
const env = upload => ({SYNC_AUTH_VERIFIER:async () => identity,SYNC_SESSION_VALIDATOR:async () => true,
  SYNC_UPLOAD_ENABLED:"true",SYNC_BATCH_UPLOADER:upload});

test("atomic batch upload uses active scoped sessions in embedded PostgreSQL",async t => {
  const db = await PGlite.create();
  try {
    for (const file of ["001_sync_workspaces.sql","002_sync_devices.sql","003_sync_sessions.sql",
      "007_sync_events.sql","008_upload_sync_event.sql","011_upload_sync_batch.sql"]) {
      await db.exec(await readFile(new URL("../"+file,import.meta.url),"utf8"));
    }
    await db.query("INSERT INTO sync_workspaces(id,name) VALUES ($1,'A'),($2,'B')",[scope.workspace_id,other]);
    await db.query("INSERT INTO sync_devices(installation_id,workspace_id) VALUES ($1,$2)",[scope.installation_id,scope.workspace_id]);
    await db.query("INSERT INTO sync_sessions(id,installation_id) VALUES ($1,$2)",[scope.session_id,scope.installation_id]);
    const sql = {query:async (query,params,options) => {
      assert.ok(options.fetchOptions.signal instanceof AbortSignal);
      return (await db.query(query,params)).rows;
    }};
    const upload = createBatchUploader(connection,() => sql);
    const count = async () => (await db.query("SELECT count(*)::integer AS n FROM sync_events")).rows[0].n;
    const scenario = async (name,fn) => t.test(name,async () => {
      await db.exec("DELETE FROM sync_events; UPDATE sync_devices SET deactivated_at=NULL; UPDATE sync_sessions SET status='running',completed_at=NULL,started_at=now()-interval '1 minute'");
      await fn();
    });

    await scenario("committed receipts preserve request order and retries reuse sequence IDs",async () => {
      const events = [event(),event()];
      const first = await upload(scope,events);
      assert.deepEqual(first.acknowledged_event_ids,events.map(e => e.event_id));
      assert.ok(BigInt(first.receipts[0].sequence_id)<BigInt(first.receipts[1].sequence_id));
      assert.deepEqual(await upload(scope,events),first);
      assert.equal(await count(),2);
    });

    await scenario("a later conflict rolls back every newly inserted event in the batch",async () => {
      const existing = event();
      await upload(scope,[existing]);
      const fresh = event();
      await assert.rejects(() => upload(scope,[fresh,{...existing,payload:{changed:true}}]));
      assert.equal(await count(),1);
      assert.equal((await db.query("SELECT event_id FROM sync_events")).rows[0].event_id,existing.event_id);
    });

    await scenario("expired, future, failed, completed and deactivated sessions cannot write",async () => {
      for (const mutation of [
        "UPDATE sync_sessions SET started_at=now()-interval '16 minutes'",
        "UPDATE sync_sessions SET started_at=now()+interval '1 minute'",
        "UPDATE sync_sessions SET status='failed'",
        "UPDATE sync_sessions SET status='completed',completed_at=now()",
        "UPDATE sync_devices SET deactivated_at=now()",
      ]) {
        await db.exec(mutation);
        await assert.rejects(() => upload(scope,[event()]),error => error.code === "42501");
        assert.equal(await count(),0);
        await db.exec("UPDATE sync_devices SET deactivated_at=NULL; UPDATE sync_sessions SET status='running',completed_at=NULL,started_at=now()-interval '1 minute'");
      }
      for (const changed of [{...scope,workspace_id:other},{...scope,installation_id:other},{...scope,session_id:other}]) {
        await assert.rejects(() => upload(changed,[event()]));
      }
      assert.equal(await count(),0);
    });

    await scenario("SQL boundary rejects invalid envelopes and payload origin spoofing atomically",async () => {
      const rawUpload = events => sql.query("SELECT * FROM public.upload_sync_batch($1,$2,$3,$4)",
        [scope.workspace_id,scope.installation_id,scope.session_id,JSON.stringify(events)],{fetchOptions:{signal:AbortSignal.timeout(10000)}});
      for (const bad of [
        {...event(),entity_type:"users"}, {...event(),operation:"delete"}, {...event(),payload:[]},
        {...event(),payload:{origin_installation_id:other}}, {...event(),extra:"injected"},
        {...event(),operation:null}, {...event(),event_id:"not-a-uuid"},
      ]) {
        await assert.rejects(() => rawUpload([event(),bad]));
        assert.equal(await count(),0);
      }
      const repeated = event();
      await assert.rejects(() => rawUpload([repeated,{...repeated,event_id:repeated.event_id.toUpperCase()}]));
      await assert.rejects(() => rawUpload([]));
      await assert.rejects(() => rawUpload(Array.from({length:501},event)));
      assert.equal(await count(),0);
    });

    await scenario("API preflight success cannot authorize a revoked session at write time",async () => {
      const config = env(upload);
      config.SYNC_SESSION_VALIDATOR = async () => {
        await db.exec("UPDATE sync_sessions SET status='failed'");
        return true;
      };
      const response = await worker.fetch(request({events:[event()]}),config);
      assert.equal(response.status,403);
      assert.equal(await count(),0);
    });

    await scenario("HTTP response acknowledges only a committed batch and hides conflicts",async () => {
      const e = event();
      const response = await worker.fetch(request({events:[e]}),env(upload));
      assert.equal(response.status,200);
      assert.deepEqual((await response.json()).acknowledged_event_ids,[e.event_id]);
      const conflict = await worker.fetch(request({events:[event(),{...e,payload:{changed:true}}]}),env(upload));
      assert.equal(conflict.status,409);
      assert.deepEqual(await conflict.json(),{error:"upload_conflict"});
      assert.equal(await count(),1);
    });

    await scenario("PUBLIC cannot execute the batch routine",async () => {
      await db.exec("CREATE ROLE untrusted_upload_test");
      const result = await db.query("SELECT has_function_privilege('untrusted_upload_test','public.upload_sync_batch(uuid,uuid,uuid,jsonb)','EXECUTE') AS allowed");
      assert.equal(result.rows[0].allowed,false);
    });
  } finally {await db.close();}
});

test("upload stays disabled unless the exact server flag is explicitly set",async () => {
  for (const flag of [undefined,false,true,"false","TRUE"]) {
    let calls = 0;
    const config = {...env(async () => {calls++;}),SYNC_UPLOAD_ENABLED:flag};
    const response = await worker.fetch(request({events:[event()]}),config);
    assert.equal(response.status,501);
    assert.equal(calls,0);
  }
});

test("invalid request JSON, scopes, origins and unsupported types never reach upload",async () => {
  const e = event();
  let calls = 0;
  for (const body of ["{",[],{},null,{events:[]},{events:[e,e]},
    {events:[{...e,entity_type:"users"}]},{events:[{...e,payload:{origin_installation_id:other}}]},
    {events:[e],workspace_id:other},{events:[{...e,session_id:other}]}]) {
    const response = await worker.fetch(request(body),env(async () => {calls++;}));
    assert.equal(response.status,400);
  }
  assert.equal(calls,0);
});

test("body reads are bounded and unsupported media is rejected",async () => {
  let calls = 0;
  const upload = async () => {calls++;};
  const tooLarge = await worker.fetch(request(" ".repeat(MAX_UPLOAD_BYTES+1)),env(upload));
  assert.equal(tooLarge.status,413);
  const media = await worker.fetch(request({events:[event()]},{"content-type":"text/plain"}),env(upload));
  assert.equal(media.status,415);
  assert.equal(calls,0);
  assert.throws(() => validateUploadBatch([{...event(),payload:{huge:"x".repeat(MAX_UPLOAD_BYTES)}}],scope));
});

test("upload receipt validation preserves bigint precision and rejects malformed receipts",async () => {
  const e = event();
  for (const rows of [[],null,[{event_id:other,sequence_id:"1"}],
    [{event_id:e.event_id,sequence_id:9007199254740992}],
    [{event_id:e.event_id,sequence_id:"9223372036854775808"}]]) {
    const upload = createBatchUploader(connection,() => ({query:async () => rows}));
    await assert.rejects(() => upload(scope,[e]),/receipt/);
  }
  const upload = createBatchUploader(connection,() => ({query:async () => [{event_id:e.event_id,sequence_id:"9007199254740993"}]}));
  assert.equal((await upload(scope,[e])).receipts[0].sequence_id,"9007199254740993");
});

test("network failures return no acknowledgements or secret details",async () => {
  const response = await worker.fetch(request({events:[event()]}),env(async () => {throw new Error("private connection string");}));
  assert.equal(response.status,503);
  assert.deepEqual(await response.json(),{error:"upload_unavailable"});
});
