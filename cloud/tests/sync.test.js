import test from "node:test";
import assert from "node:assert/strict";
import worker from "../worker.js";

const req = (token) => new Request("https://sync.example.test/sync/upload", {
  method: "POST",
  headers: token ? { authorization: token } : {},
});

test("sync route fails closed without an authentication verifier", async () => {
  const response = await worker.fetch(req("Bearer token"), {});
  assert.equal(response.status, 503);
  assert.deepEqual(await response.json(), { error: "sync_not_configured" });
  assert.equal(response.headers.get("cache-control"), "no-store");
});

test("sync route requires bearer credentials", async () => {
  const env = { SYNC_AUTH_VERIFIER: async () => ({ authorized: true, workspace_id: "11111111-1111-4111-8111-111111111111", installation_id: "22222222-2222-4222-8222-222222222222" }) };
  for (const request of [req(),req("Basic token"),req("Bearer "),req("Bearer two words")]) {
    const response = await worker.fetch(request,env);
    assert.equal(response.status,401);
  }
});

test("invalid credentials never become authorized", async () => {
  const response = await worker.fetch(req("Bearer bad"),{
    SYNC_AUTH_VERIFIER: async () => { throw new Error("not authenticated"); },
  });
  assert.equal(response.status,401);
  assert.deepEqual(await response.json(), { error: "unauthorized" });
});

test("workspace and installation must both be authorized", async () => {
  for (const identity of [
    { authorized: true, workspace_id: "11111111-1111-4111-8111-111111111111" },
    { authorized: true, installation_id: "22222222-2222-4222-8222-222222222222" },
    { authorized: false, workspace_id: "11111111-1111-4111-8111-111111111111", installation_id: "22222222-2222-4222-8222-222222222222" },
  ]) {
    const response = await worker.fetch(req("Bearer token"),{
      SYNC_AUTH_VERIFIER: async () => identity,
    });
    assert.equal(response.status,403);
  }
});

test("even authenticated clients cannot upload before activation", async () => {
  const response = await worker.fetch(req("Bearer token"),{
    SYNC_AUTH_VERIFIER: async () => ({
      authorized: true,workspace_id:"11111111-1111-4111-8111-111111111111",installation_id:"22222222-2222-4222-8222-222222222222",
    }),
  });
  assert.equal(response.status,501);
  assert.deepEqual(await response.json(), { error:"sync_endpoint_not_implemented" });
});

test("only documented routes and methods are exposed", async () => {
  const unknown = await worker.fetch(new Request("https://sync.example.test/sync/unknown"));
  assert.equal(unknown.status,404);
  const method = await worker.fetch(new Request("https://sync.example.test/sync/upload"));
  assert.equal(method.status,405);
  assert.equal(method.headers.get("allow"),"POST");
  const pullMethod = await worker.fetch(new Request("https://sync.example.test/sync/pull",{
    method:"POST"
  }));
  assert.equal(pullMethod.status,405);
  assert.equal(pullMethod.headers.get("allow"),"GET");
});

test("malformed verified workspace and installation claims are rejected", async () => {
  for (const identity of [
    { authorized:true, workspace_id:"w", installation_id:"22222222-2222-4222-8222-222222222222" },
    { authorized:true, workspace_id:"11111111-1111-4111-8111-111111111111", installation_id:"i" },
    { authorized:true, workspace_id:"11111111-1111-4111-8111-111111111111", installation_id:42 },
  ]) {
    const response = await worker.fetch(req("Bearer token"), {
      SYNC_AUTH_VERIFIER: async () => identity,
    });
    assert.equal(response.status,403);
  }
});
