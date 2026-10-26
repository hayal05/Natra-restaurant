// Task 35: secure-by-default entry point for the future sync API.
// Authentication and workspace authorization must be verified before any
// Neon query is connected. This handler never processes financial events.
const headers = {
  "content-type": "application/json; charset=utf-8",
  "cache-control": "no-store",
};
const json = (status, error) =>
  new Response(JSON.stringify({ error }), { status, headers });

export async function handleSyncRequest(request, env = {}) {
  const path = new URL(request.url).pathname;
  if (!path.startsWith("/sync/")) return json(404, "not_found");
  // Reserve only the intended upload/pull routes. Unknown routes may
  // never inherit authorization and accidentally reach a future handler.
  const supported = {
    "/sync/upload": "POST",
    "/sync/pull": "GET",
  };
  if (!Object.hasOwn(supported, path)) return json(404, "not_found");
  if (request.method !== supported[path]) {
    return new Response(JSON.stringify({ error: "method_not_allowed" }), {
      status: 405,
      headers: { ...headers, allow: supported[path] },
    });
  }
  // No implicit authentication from an installation ID, workspace ID,
  // session cookie, or client-controlled headers.
  if (typeof env.SYNC_AUTH_VERIFIER !== "function") return json(503, "sync_not_configured");
  const token = request.headers.get("authorization");
  if (!token || token.length > 8199 || !/^Bearer [^\s]+$/.test(token)) {
    return json(401, "unauthorized");
  }
  // A deployment-supplied verifier must authenticate the caller and bind
  // workspace/installation permissions. It must not merely parse a JWT.
  let identity;
  try {
    identity = await env.SYNC_AUTH_VERIFIER(token.slice(7));
  } catch {
    return json(401, "unauthorized");
  }
  // Cloud IDs are UUIDs, never arbitrary client-defined identifiers.
  const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
  if (!identity || identity.authorized !== true ||
      typeof identity.workspace_id !== "string" ||
      typeof identity.installation_id !== "string" ||
      !uuid.test(identity.workspace_id) || !uuid.test(identity.installation_id)) {
    return json(403, "forbidden");
  }
  // Require a verified sync session bound to the same authorized
  // installation and workspace. Do not trust client-supplied session IDs.
  if (identity.session_authorized !== true ||
      typeof identity.session_id !== "string" ||
      !uuid.test(identity.session_id)) {
    return json(403, "sync_session_required");
  }
  // Deliberately do not route to Neon until authorized session, scope,
  // and per-installation quota checks are integrated and tested.
  return json(501, "sync_endpoint_not_implemented");
}
