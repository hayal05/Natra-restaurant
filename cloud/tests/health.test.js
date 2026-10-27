import test from "node:test";
import assert from "node:assert/strict";
import { handleHealth } from "../api/health.js";

const request = (path, method = "GET") =>
  new Request(`https://sync.example.test${path}`, { method });

test("GET /health returns a no-cache JSON response", async () => {
  const response = handleHealth(request("/health"));
  assert.equal(response.status, 200);
  assert.equal(response.headers.get("cache-control"), "no-store");
  assert.match(response.headers.get("content-type"), /application\/json/);
  assert.deepEqual(await response.json(), { status: "ok", service: "natra-sync-api" });
});

test("unsupported method returns 405 with allowed method", async () => {
  const response = handleHealth(request("/health", "POST"));
  assert.equal(response.status, 405);
  assert.equal(response.headers.get("allow"), "GET");
  assert.deepEqual(await response.json(), { error: "method_not_allowed" });
});

test("unknown path returns 404", async () => {
  const response = handleHealth(request("/other"));
  assert.equal(response.status, 404);
  assert.deepEqual(await response.json(), { error: "not_found" });
});
