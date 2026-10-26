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
  // No implicit authentication from an installation ID, workspace ID,
  // session cookie, or client-controlled headers.
  if (!env.SYNC_AUTH_VERIFIER) return json(503, "sync_not_configured");
  const token = request.headers.get("authorization");
  if (!token || !/^Bearer [^\s]+$/.test(token)) {
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
  if (!identity || !identity.workspace_id || !identity.installation_id ||
      identity.authorized !== true) {
    return json(403, "forbidden");
  }
  // Deliberately do not route to Neon until authorized session, scope,
  // and per-installation quota checks are integrated and tested.
  return json(501, "sync_endpoint_not_implemented");
}
