import { handleHealth } from "./api/health.js";
import { handleSyncRequest } from "./api/sync.js";

// Sync requests fail closed. The server-only Neon adapter can validate
// sessions when explicitly configured; financial endpoints remain disabled.
export default {
  async fetch(request, env) {
    const path = new URL(request.url).pathname;
    if (path.startsWith("/sync/")) return handleSyncRequest(request, env);
    return handleHealth(request);
  },
};
