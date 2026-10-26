import { withSyncCors } from "./api/cors.js";
import { handleHealth } from "./api/health.js";
import { handleSyncRequest } from "./api/sync.js";

// Sync requests fail closed. The server-only Neon adapter can validate
// sessions when explicitly configured; upload requires a separate explicit
// server flag. No live flag is configured by this source change.
export default {
  async fetch(request, env) {
    const path = new URL(request.url).pathname;
    if (path.startsWith("/sync/")) return withSyncCors(request, env, () => handleSyncRequest(request, env));
    return handleHealth(request);
  },
};
