import { handleHealth } from "./api/health.js";

// Cloudflare Workers adapter. Reuses the provider-neutral handler without
// starting an always-on backend or connecting to Neon.
export default {
  async fetch(request) {
    return handleHealth(request);
  },
};
