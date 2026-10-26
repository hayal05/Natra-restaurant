import test from "node:test";
import assert from "node:assert/strict";
import worker from "../worker.js";

const req = (token) => new Request("https://sync.example.test/sync/events", {
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
  const env = { SYNC_AUTH_VERIFIER: async () => ({ authorized: true, workspace_id: "w", installation_id: "i" }) };
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
    { authorized: true, workspace_id: "w" },
    { authorized: true, installation_id: "i" },
    { authorized: false, workspace_id: "w", installation_id: "i" },
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
      authorized: true,workspace_id:"w",installation_id:"i",
    }),
  });
  assert.equal(response.status,501);
  assert.deepEqual(await response.json(), { error:"sync_endpoint_not_implemented" });
});
