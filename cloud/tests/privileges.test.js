import test from "node:test";
import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import {PGlite} from "@electric-sql/pglite";

const workspace="11111111-1111-4111-8111-111111111111";
const installation="22222222-2222-4222-8222-222222222222";
const session="33333333-3333-4333-8333-333333333333";
const eventId="44444444-4444-4444-8444-444444444444";
const signatures=["reserve_sync_session(uuid,uuid)","complete_sync_session(uuid,uuid)",
  "fail_sync_session(uuid,uuid)","upload_sync_event(uuid,uuid,uuid,text,text,text,jsonb)",
  "enroll_sync_device(uuid,uuid)","reserve_authorized_sync_session(uuid,uuid,uuid)",
  "upload_sync_batch(uuid,uuid,uuid,jsonb)","pull_authorized_sync_events(uuid,uuid,uuid,bigint,integer)",
  "finish_authorized_sync_session(uuid,uuid,uuid,text)","sync_day_bounds(timestamptz)",
  "authorized_sync_allowance(uuid,uuid,uuid)"];
const denied=error=>error.code==="42501";

test("sync privilege hardening blocks ambient execution and temporary-table shadowing",async t=>{
  const db=await PGlite.create();
  const asRole=async(role,fn)=>{
    // Role names below are fixed test literals, never request input.
    await db.exec(`SET ROLE ${role}`);
    try{return await fn();}finally{await db.exec("RESET ROLE; RESET search_path");}
  };
  try{
    const files=["001_sync_workspaces.sql","002_sync_devices.sql","003_sync_sessions.sql",
      "004_reserve_sync_session.sql","005_complete_sync_session.sql","006_fail_sync_session.sql",
      "007_sync_events.sql","008_upload_sync_event.sql","009_enroll_sync_device.sql",
      "010_reserve_authorized_sync_session.sql","011_upload_sync_batch.sql",
      "012_pull_authorized_sync_events.sql","013_finish_authorized_sync_session.sql","014_authorized_sync_allowance.sql"];
    for(const file of files)await db.exec(await readFile(new URL("../"+file,import.meta.url),"utf8"));
    await db.exec("CREATE ROLE sync_untrusted_test; CREATE ROLE sync_issuer_test; CREATE ROLE sync_runtime_test; CREATE ROLE sync_execute_only_test; GRANT USAGE ON SCHEMA public TO sync_untrusted_test,sync_issuer_test,sync_runtime_test,sync_execute_only_test");
    const hardening=await readFile(new URL("../015_harden_sync_function_privileges.sql",import.meta.url),"utf8");
    await t.test("upgrade revokes inherited PUBLIC access while preserving explicit role grants",async()=>{
      assert.equal((await db.query("SELECT has_function_privilege('sync_untrusted_test','public.complete_sync_session(uuid,uuid)','EXECUTE') AS allowed")).rows[0].allowed,true);
      await db.exec("GRANT EXECUTE ON FUNCTION public.complete_sync_session(uuid,uuid) TO sync_runtime_test");
      await db.exec(hardening);
      await db.exec(hardening); // Safe repeat; does not recreate routines or data.
      assert.equal((await db.query("SELECT has_function_privilege('sync_runtime_test','public.complete_sync_session(uuid,uuid)','EXECUTE') AS allowed")).rows[0].allowed,true);
      for(const signature of signatures){
        const row=(await db.query("SELECT p.prosecdef,p.proconfig,has_function_privilege('sync_untrusted_test',p.oid,'EXECUTE') AS allowed FROM pg_proc p WHERE p.oid=$1::regprocedure",["public."+signature])).rows[0];
        assert.equal(row.allowed,false,signature);
        assert.equal(row.prosecdef,false,signature);
        assert.ok(row.proconfig.includes("search_path=pg_catalog, public, pg_temp"),signature);
      }
    });
    await db.query("INSERT INTO public.sync_workspaces(id,name) VALUES ($1,'Privilege test')",[workspace]);
    await db.query("INSERT INTO public.sync_devices(installation_id,workspace_id) VALUES ($1,$2)",[installation,workspace]);
    await t.test("an untrusted SQL role cannot call old helpers or scoped entry points",async()=>{
      await asRole("sync_untrusted_test",async()=>{
        for(const query of [
          "SELECT public.reserve_sync_session(NULL,NULL)","SELECT public.complete_sync_session(NULL,NULL)",
          "SELECT public.fail_sync_session(NULL,NULL)","SELECT public.upload_sync_event(NULL,NULL,NULL,NULL,NULL,NULL,NULL)",
          "SELECT public.enroll_sync_device(NULL,NULL)","SELECT public.reserve_authorized_sync_session(NULL,NULL,NULL)",
          "SELECT public.upload_sync_batch(NULL,NULL,NULL,NULL)","SELECT public.pull_authorized_sync_events(NULL,NULL,NULL,0,1)",
          "SELECT public.finish_authorized_sync_session(NULL,NULL,NULL,'complete')",
          "SELECT public.authorized_sync_allowance(NULL,NULL,NULL)","SELECT * FROM public.sync_day_bounds(now())",
        ])await assert.rejects(()=>db.query(query),denied,query);
      });
    });
    await t.test("invoker mode does not elevate a role granted only function execution",async()=>{
      await db.exec("GRANT EXECUTE ON FUNCTION public.reserve_authorized_sync_session(uuid,uuid,uuid) TO sync_execute_only_test");
      await asRole("sync_execute_only_test",()=>assert.rejects(()=>db.query("SELECT * FROM public.reserve_authorized_sync_session($1,$2,$3)",[workspace,installation,session]),denied));
      assert.equal((await db.query("SELECT count(*)::int AS n FROM public.sync_sessions")).rows[0].n,0);
    });
    await t.test("explicit trusted issuer privileges reserve without enrollment access",async()=>{
      await db.exec(`GRANT SELECT,UPDATE ON public.sync_devices TO sync_issuer_test;
        GRANT SELECT,INSERT,UPDATE ON public.sync_sessions TO sync_issuer_test;
        GRANT EXECUTE ON FUNCTION public.reserve_authorized_sync_session(uuid,uuid,uuid),public.reserve_sync_session(uuid,uuid) TO sync_issuer_test`);
      await asRole("sync_issuer_test",async()=>{
        const rows=(await db.query("SELECT * FROM public.reserve_authorized_sync_session($1,$2,$3)",[workspace,installation,session])).rows;
        assert.equal(rows[0].session_id,session);
        await assert.rejects(()=>db.query("SELECT public.enroll_sync_device($1,$2)",[workspace,installation]),denied);
      });
    });
    await t.test("trusted runtime uses public ledger and sessions despite hostile caller search path",async()=>{
      await db.exec(`GRANT SELECT,UPDATE ON public.sync_workspaces,public.sync_devices,public.sync_sessions TO sync_runtime_test;
        GRANT SELECT,INSERT ON public.sync_events TO sync_runtime_test;
        GRANT USAGE ON SEQUENCE public.sync_events_sequence_id_seq TO sync_runtime_test;
        GRANT EXECUTE ON FUNCTION public.upload_sync_batch(uuid,uuid,uuid,jsonb),public.upload_sync_event(uuid,uuid,uuid,text,text,text,jsonb),
          public.pull_authorized_sync_events(uuid,uuid,uuid,bigint,integer),public.finish_authorized_sync_session(uuid,uuid,uuid,text),
          public.complete_sync_session(uuid,uuid),public.fail_sync_session(uuid,uuid),public.authorized_sync_allowance(uuid,uuid,uuid),
          public.sync_day_bounds(timestamptz) TO sync_runtime_test;
        CREATE TEMP TABLE sync_sessions (LIKE public.sync_sessions INCLUDING ALL);
        CREATE TEMP TABLE sync_events (LIKE public.sync_events INCLUDING ALL);
        CREATE TEMP TABLE sync_devices (LIKE public.sync_devices INCLUDING ALL);
        CREATE TEMP TABLE sync_workspaces (LIKE public.sync_workspaces INCLUDING ALL);
        GRANT SELECT,INSERT,UPDATE ON pg_temp.sync_sessions,pg_temp.sync_events,pg_temp.sync_devices,pg_temp.sync_workspaces TO sync_runtime_test`);
      // Empty temporary device/workspace tables would reject the upload if
      // legacy SQL resolved them ahead of the real public relations.
      await asRole("sync_runtime_test",async()=>{
        await db.exec("SET search_path=pg_temp,public,pg_catalog");
        const events=[{event_id:eventId,entity_type:"categories",entity_id:installation+":1",operation:"upsert",payload:{name:"Tea",origin_installation_id:installation}}];
        for(let i=0;i<2;i++){
          const receipt=(await db.query("SELECT * FROM public.upload_sync_batch($1,$2,$3,$4::jsonb)",[workspace,installation,session,JSON.stringify(events)])).rows;
          assert.equal(receipt.length,1);assert.equal(receipt[0].event_id,eventId);
        }
        const page=(await db.query("SELECT * FROM public.pull_authorized_sync_events($1,$2,$3,0,10)",[workspace,installation,session])).rows;
        assert.equal(page.length,1);assert.equal(page[0].event_id,eventId);
        for(let i=0;i<2;i++)assert.equal((await db.query("SELECT public.finish_authorized_sync_session($1,$2,$3,'complete') AS ok",[workspace,installation,session])).rows[0].ok,true);
        const allowance=(await db.query("SELECT * FROM public.authorized_sync_allowance($1,$2,$3)",[workspace,installation,session])).rows[0];
        assert.equal(allowance.completed_today,1);assert.equal(allowance.session_permitted,false);
        await assert.rejects(()=>db.query("SELECT public.reserve_sync_session($1,$2)",[installation,crypto.randomUUID()]),denied);
      });
      assert.equal((await db.query("SELECT status FROM public.sync_sessions WHERE id=$1",[session])).rows[0].status,"completed");
      assert.equal((await db.query("SELECT count(*)::int AS n FROM public.sync_events")).rows[0].n,1);
      for(const table of ["sync_sessions","sync_events","sync_devices","sync_workspaces"]){
        assert.equal((await db.query(`SELECT count(*)::int AS n FROM pg_temp.${table}`)).rows[0].n,0);
      }
    });
  }finally{await db.close();}
});
