import { createBatchUploader } from "../db/upload.js";
import { handleUploadRequest } from "./upload.js";
import { configuredAuthVerifier } from "../auth/jwt.js";
import { createNeonSessionValidator } from "../db/neon-session.js";

// Task 35: secure-by-default entry point for the future sync API.
// Authentication and workspace authorization precede every Neon query.
// Upload remains disabled unless explicitly enabled by server configuration.
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
  let verifyAuth;
  try {
    verifyAuth = configuredAuthVerifier(env);
  } catch {
    return json(503, "sync_not_configured");
  }
  if (typeof verifyAuth !== "function") return json(503, "sync_not_configured");
  const token = request.headers.get("authorization");
  if (!token || token.length > 8199 || !/^Bearer [^\s]+$/.test(token)) {
    return json(401, "unauthorized");
  }
  // A deployment-supplied verifier must authenticate the caller and bind
  // workspace/installation permissions. It must not merely parse a JWT.
  let identity;
  try {
    identity = await verifyAuth(token.slice(7));
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
      !uuid.test(identity.session_id) ||
      identity.session_workspace_id !== identity.workspace_id ||
      identity.session_installation_id !== identity.installation_id) {
    return json(403, "sync_session_required");
  }
  // Authentication claims alone cannot establish that a session is still
  // running. The server adapter must revalidate against current database
  // state using queries/validate_sync_session.sql and these verified IDs.
  // Future operations must revalidate inside their own database transaction.
  let validateSession = env.SYNC_SESSION_VALIDATOR;
  // Explicit invalid overrides fail closed instead of falling back silently.
  if (validateSession === undefined && env.NEON_DATABASE_URL) {
    try {
      validateSession = createNeonSessionValidator(env.NEON_DATABASE_URL);
    } catch {
      return json(503, "sync_session_validation_not_configured");
    }
  }
  if (typeof validateSession !== "function") {
    return json(503, "sync_session_validation_not_configured");
  }
  const scope = Object.freeze({workspace_id:identity.workspace_id,
    installation_id:identity.installation_id,session_id:identity.session_id});
  let sessionValid;
  try {
    sessionValid = await validateSession(scope);
  } catch {
    return json(503, "sync_session_validation_unavailable");
  }
  if (sessionValid !== true) return json(403, "sync_session_required");
  // The flag is intentionally absent from deployment configuration. Setting
  // it in a live environment requires the user's separate approval.
  if (path === "/sync/upload" && env.SYNC_UPLOAD_ENABLED === "true") {
    let upload = env.SYNC_BATCH_UPLOADER;
    if (upload === undefined) {
      try { upload = createBatchUploader(env.NEON_DATABASE_URL); }
      catch { return json(503,"upload_not_configured"); }
    }
    if (typeof upload !== "function") return json(503,"upload_not_configured");
    return handleUploadRequest(request,scope,upload);
  }
  // Pull remains staged for Task 37. No live flag is configured here.
  return json(501, "sync_endpoint_not_implemented");
}
