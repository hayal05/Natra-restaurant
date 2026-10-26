import test from "node:test";
import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import {PGlite} from "@electric-sql/pglite";
import {createDeviceEnroller} from "../db/enrollment.js";
import {createNeonSessionValidator} from "../db/neon-session.js";

const connection = "postgresql://test:fake@ep-example.eu-central-1.aws.neon.tech/neondb";
const workspace = "11111111-1111-4111-8111-111111111111";
const otherWorkspace = "44444444-4444-4444-8444-444444444444";
const installation = "22222222-2222-4222-8222-222222222222";
const session = "33333333-3333-4333-8333-333333333333";

test("trusted device enrollment and session SQL run against embedded PostgreSQL", async t => {
  const db = await PGlite.create();
  try {
    for (const file of ["001_sync_workspaces.sql","002_sync_devices.sql","003_sync_sessions.sql","009_enroll_sync_device.sql"]) {
      await db.exec(await readFile(new URL("../"+file,import.meta.url),"utf8"));
    }
    await db.query("INSERT INTO sync_workspaces(id,name) VALUES ($1,'A'),($2,'B')",[workspace,otherWorkspace]);
    let calls = 0;
    const createSql = () => ({query:async (sql, params, options) => {
      calls++;
      assert.ok(options.fetchOptions.signal instanceof AbortSignal);
      return (await db.query(sql,params)).rows;
    }});
    const enroll = createDeviceEnroller(connection,createSql);

    await t.test("enrollment is idempotent and preserves registration time", async () => {
      assert.equal(calls,0);
      assert.equal(await enroll({workspace_id:workspace,installation_id:installation}),installation);
      const before = (await db.query("SELECT * FROM sync_devices")).rows;
      assert.equal(await enroll({workspace_id:workspace,installation_id:installation}),installation);
      assert.deepEqual((await db.query("SELECT * FROM sync_devices")).rows,before);
    });

    await t.test("an installation cannot be reassigned to another workspace", async () => {
      await assert.rejects(() => enroll({workspace_id:otherWorkspace,installation_id:installation}),/another workspace/);
      assert.equal((await db.query("SELECT workspace_id FROM sync_devices")).rows[0].workspace_id,workspace);
    });

    await t.test("invalid enrollment leaves no device record", async () => {
      for (const scope of [
        {workspace_id:workspace,installation_id:null},
        {workspace_id:null,installation_id:session},
        {workspace_id:session,installation_id:session},
        {workspace_id:workspace,installation_id:"not-a-uuid"},
      ]) await assert.rejects(() => enroll(scope));
      assert.equal((await db.query("SELECT count(*)::integer AS n FROM sync_devices")).rows[0].n,1);
    });

    await t.test("deactivated devices require separate reactivation approval", async () => {
      await db.query("UPDATE sync_devices SET deactivated_at=now() WHERE installation_id=$1",[installation]);
      await assert.rejects(() => enroll({workspace_id:workspace,installation_id:installation}),/deactivated/);
      assert.ok((await db.query("SELECT deactivated_at FROM sync_devices")).rows[0].deactivated_at);
      await db.query("UPDATE sync_devices SET deactivated_at=NULL WHERE installation_id=$1",[installation]);
    });

    await t.test("PUBLIC has no enrollment function execution grant", async () => {
      await db.exec("CREATE ROLE untrusted_enrollment_test");
      const result = await db.query("SELECT has_function_privilege('untrusted_enrollment_test','public.enroll_sync_device(uuid,uuid)','EXECUTE') AS allowed");
      assert.equal(result.rows[0].allowed,false);
    });

    await t.test("session adapter enforces scope and lifecycle in PostgreSQL", async () => {
      await db.query("INSERT INTO sync_sessions(id,installation_id,started_at) VALUES ($1,$2,now()-interval '1 minute')",[session,installation]);
      const validate = createNeonSessionValidator(connection,createSql);
      const scope = {workspace_id:workspace,installation_id:installation,session_id:session};
      assert.equal(await validate(scope),true);
      for (const changed of [
        {...scope,workspace_id:otherWorkspace},
        {...scope,installation_id:otherWorkspace},
        {...scope,session_id:otherWorkspace},
      ]) assert.equal(await validate(changed),false);
      for (const mutation of [
        "UPDATE sync_devices SET deactivated_at=now()",
        "UPDATE sync_sessions SET status='failed'",
        "UPDATE sync_sessions SET status='completed',completed_at=now()",
        "UPDATE sync_sessions SET started_at=now()-interval '16 minutes'",
        "UPDATE sync_sessions SET started_at=now()+interval '1 minute'",
      ]) {
        await db.exec(mutation);
        assert.equal(await validate(scope),false,mutation);
        await db.exec("UPDATE sync_devices SET deactivated_at=NULL; UPDATE sync_sessions SET status='running',completed_at=NULL,started_at=now()-interval '1 minute'");
      }
    });
  } finally {
    await db.close();
  }
});

test("enrollment adapter rejects ambiguous or mismatched transport results", async () => {
  for (const rows of [[],null,[{},{}],[{}],[{installation_id:workspace}]]) {
    const enroll = createDeviceEnroller(connection,() => ({query:async () => rows}));
    await assert.rejects(() => enroll({workspace_id:workspace,installation_id:installation}),/enrollment failed/);
  }
});
