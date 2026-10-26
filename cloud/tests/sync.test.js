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
    SYNC_SESSION_VALIDATOR: async () => true,
    SYNC_AUTH_VERIFIER: async () => ({
      authorized: true,workspace_id:"11111111-1111-4111-8111-111111111111",installation_id:"22222222-2222-4222-8222-222222222222",session_authorized:true,session_id:"33333333-3333-4333-8333-333333333333",session_workspace_id:"11111111-1111-4111-8111-111111111111",session_installation_id:"22222222-2222-4222-8222-222222222222",
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

test("misconfigured verifier and oversized bearer tokens fail closed", async () => {
  const configuredWrong = await worker.fetch(req("Bearer token"), {
    SYNC_AUTH_VERIFIER: "not-a-function",
  });
  assert.equal(configuredWrong.status,503);
  const response = await worker.fetch(req("Bearer " + "x".repeat(9000)), {
    SYNC_AUTH_VERIFIER: async () => {
      throw new Error("oversized token should never be verified");
    },
  });
  assert.equal(response.status,401);
});

test("verified workspace identity alone cannot access sync without an authorized session", async () => {
  const base = { authorized:true,workspace_id:"11111111-1111-4111-8111-111111111111",installation_id:"22222222-2222-4222-8222-222222222222" };
  for (const identity of [
    base,
    { ...base,session_authorized:false,session_id:"33333333-3333-4333-8333-333333333333" },
    { ...base,session_authorized:true,session_id:"not-a-uuid" },
  ]) {
    const response = await worker.fetch(req("Bearer token"),{
      SYNC_AUTH_VERIFIER: async () => identity,
    });
    assert.equal(response.status,403);
    assert.deepEqual(await response.json(),{error:"sync_session_required"});
  }
});

test("sync session must match authorized workspace and installation", async () => {
  const base = {
    authorized:true,workspace_id:"11111111-1111-4111-8111-111111111111",installation_id:"22222222-2222-4222-8222-222222222222",
    session_authorized:true,session_id:"33333333-3333-4333-8333-333333333333",
    session_workspace_id:"11111111-1111-4111-8111-111111111111",session_installation_id:"22222222-2222-4222-8222-222222222222",
  };
  for (const identity of [
    {...base,session_workspace_id:"44444444-4444-4444-8444-444444444444"},
    {...base,session_installation_id:"44444444-4444-4444-8444-444444444444"},
    {...base,session_workspace_id:undefined},
  ]) {
    const response = await worker.fetch(req("Bearer token"), {
      SYNC_AUTH_VERIFIER: async () => identity,
    });
    assert.equal(response.status,403);
    assert.deepEqual(await response.json(), {error:"sync_session_required"});
  }
});

const verifiedIdentity = {
  authorized: true,
  workspace_id: "11111111-1111-4111-8111-111111111111",
  installation_id: "22222222-2222-4222-8222-222222222222",
  session_authorized: true,
  session_id: "33333333-3333-4333-8333-333333333333",
  session_workspace_id: "11111111-1111-4111-8111-111111111111",
  session_installation_id: "22222222-2222-4222-8222-222222222222",
};

test("server session validation is mandatory and fails closed", async () => {
  for (const validator of [undefined, "not-a-function"]) {
    const response = await worker.fetch(req("Bearer token"), {
      SYNC_AUTH_VERIFIER: async () => verifiedIdentity,
      SYNC_SESSION_VALIDATOR: validator,
    });
    assert.equal(response.status, 503);
    assert.deepEqual(await response.json(), {error:"sync_session_validation_not_configured"});
  }
  for (const result of [false, undefined, null, 1, "true", {authorized:true}]) {
    const response = await worker.fetch(req("Bearer token"), {
      SYNC_AUTH_VERIFIER: async () => verifiedIdentity,
      SYNC_SESSION_VALIDATOR: async () => result,
    });
    assert.equal(response.status, 403);
    assert.deepEqual(await response.json(), {error:"sync_session_required"});
  }
});

test("database validation failures expose no database details", async () => {
  const response = await worker.fetch(req("Bearer token"), {
    SYNC_AUTH_VERIFIER: async () => verifiedIdentity,
    SYNC_SESSION_VALIDATOR: async () => {throw new Error("private connection details");},
  });
  assert.equal(response.status, 503);
  assert.deepEqual(await response.json(), {error:"sync_session_validation_unavailable"});
});

test("upload and pull validators receive only authenticated scope", async () => {
  for (const path of ["upload", "pull"]) {
    let calls = 0;
    const request = new Request("https://sync.example.test/sync/" + path + "?workspace_id=untrusted", {
      method: path === "upload" ? "POST" : "GET",
      headers: {authorization:"Bearer token", "x-session-id":"untrusted"},
      ...(path === "upload" ? {body:JSON.stringify({session_id:"untrusted"})} : {}),
    });
    const response = await worker.fetch(request, {
      SYNC_AUTH_VERIFIER: async () => verifiedIdentity,
      SYNC_SESSION_VALIDATOR: async (scope) => {
        calls++;
        assert.deepEqual(scope, {
          workspace_id:verifiedIdentity.workspace_id,
          installation_id:verifiedIdentity.installation_id,
          session_id:verifiedIdentity.session_id,
        });
        assert.equal(Object.isFrozen(scope), true);
        return true;
      },
    });
    assert.equal(calls, 1);
    assert.equal(response.status, 501);
  }
});

test("unauthenticated or mismatched callers never reach session validation", async () => {
  for (const identity of [null, {...verifiedIdentity, session_installation_id:"44444444-4444-4444-8444-444444444444"}]) {
    let calls = 0;
    const response = await worker.fetch(req("Bearer token"), {
      SYNC_AUTH_VERIFIER: async () => identity,
      SYNC_SESSION_VALIDATOR: async () => {calls++; return true;},
    });
    assert.equal(response.status, 403);
    assert.equal(calls, 0);
  }
});

// Neon transport is mocked: tests never use credentials or a live database.
const {createNeonSessionValidator, VALIDATE_SESSION_SQL} = await import("../db/neon-session.js");
const neonTestUrl = "postgresql://test:fake@ep-example.eu-central-1.aws.neon.tech/neondb";
const sessionScope = {
  workspace_id:verifiedIdentity.workspace_id,
  installation_id:verifiedIdentity.installation_id,
  session_id:verifiedIdentity.session_id,
};

test("Neon validator binds verified IDs and bounds network wait", async () => {
  let calls = 0;
  const validate = createNeonSessionValidator(neonTestUrl, (connectionString) => {
    assert.equal(connectionString, neonTestUrl);
    return {query:async (query, params, options) => {
      calls++;
      assert.equal(query, VALIDATE_SESSION_SQL);
      assert.deepEqual(params, Object.values(sessionScope));
      assert.ok(options.fetchOptions.signal instanceof AbortSignal);
      return [sessionScope];
    }};
  });
  assert.equal(calls, 0);
  assert.equal(await validate(sessionScope), true);
  assert.equal(calls, 1);
});

test("Neon validator denies absent, ambiguous, malformed and mismatched results", async () => {
  for (const rows of [[], null, {}, [sessionScope,sessionScope], [{}],
    [{...sessionScope,installation_id:"44444444-4444-4444-8444-444444444444"}],
    [{...sessionScope,workspace_id:"44444444-4444-4444-8444-444444444444"}],
    [{...sessionScope,session_id:"44444444-4444-4444-8444-444444444444"}]]) {
    const validate = createNeonSessionValidator(neonTestUrl, () => ({query:async () => rows}));
    assert.equal(await validate(sessionScope), false);
  }
});

test("Neon query errors reach the API's fail-closed error handler", async () => {
  const validate = createNeonSessionValidator(neonTestUrl, () => ({query:async () => {
    throw new Error("private database details");
  }}));
  const response = await worker.fetch(req("Bearer token"), {
    SYNC_AUTH_VERIFIER:async () => verifiedIdentity,
    SYNC_SESSION_VALIDATOR:validate,
  });
  assert.equal(response.status,503);
  assert.deepEqual(await response.json(),{error:"sync_session_validation_unavailable"});
});

test("invalid Neon settings are rejected before transport is constructed", async () => {
  for (const url of ["bad", "https://example.com", "postgresql://u:p@localhost/db",
    "postgresql://u:p@neon.tech.attacker.test/db", "postgresql://ep-example.aws.neon.tech/db",
    "postgresql://u:p@ep-example.aws.neon.tech/", "postgresql://u:p@ep-example.aws.neon.tech:80/db"]) {
    assert.throws(() => createNeonSessionValidator(url, () => {
      assert.fail("invalid settings must not construct transport");
    }));
    const response = await worker.fetch(req("Bearer token"), {
      SYNC_AUTH_VERIFIER:async () => verifiedIdentity,
      NEON_DATABASE_URL:url,
    });
    assert.equal(response.status,503);
    assert.deepEqual(await response.json(),{error:"sync_session_validation_not_configured"});
  }
});

test("Neon validation query preserves canonical scope and lifecycle checks", async () => {
  const {readFile} = await import("node:fs/promises");
  const source = await readFile(new URL("../queries/validate_sync_session.sql",import.meta.url),"utf8");
  const normalize = sql => sql.replace(/\r\n/g,"\n").trim();
  for (const text of [source, source.replace(/\r?\n/g,"\r\n")]) {
    assert.equal(normalize(VALIDATE_SESSION_SQL),normalize(text.slice(text.indexOf("SELECT "))));
  }
});

test("configured API uses the real Neon driver with mocked HTTP transport", async () => {
  const {neonConfig} = await import("@neondatabase/serverless");
  const originalFetch = neonConfig.fetchFunction;
  let calls = 0;
  neonConfig.fetchFunction = async (url, options) => {
    calls++;
    assert.equal(new URL(url).protocol,"https:");
    const body = JSON.parse(options.body);
    assert.equal(body.query,VALIDATE_SESSION_SQL);
    assert.deepEqual(body.params,Object.values(sessionScope));
    assert.ok(options.signal instanceof AbortSignal);
    return new Response(JSON.stringify({
      fields:Object.keys(sessionScope).map(name => ({name,dataTypeID:2950})),
      rows:[Object.values(sessionScope)],
      rowCount:1,command:"SELECT",rowAsArray:true,
    }),{status:200,headers:{"content-type":"application/json"}});
  };
  try {
    const response = await worker.fetch(req("Bearer token"),{
      SYNC_AUTH_VERIFIER:async () => verifiedIdentity,
      NEON_DATABASE_URL:neonTestUrl,
    });
    assert.equal(response.status,501);
    assert.deepEqual(await response.json(),{error:"sync_endpoint_not_implemented"});
    assert.equal(calls,1);
  } finally {
    neonConfig.fetchFunction = originalFetch;
  }
});
