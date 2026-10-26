import { handleHealth } from "./api/health.js";
import { handleSyncRequest } from "./api/sync.js";

// Sync requests are fail-closed. No database transport is enabled.
export default {
  async fetch(request, env) {
    const path = new URL(request.url).pathname;
    if (path.startsWith("/sync/")) return handleSyncRequest(request, env);
    return handleHealth(request);
  },
};
