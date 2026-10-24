/**
 * Provider-neutral health-check handler for the future serverless sync API.
 * The hosting adapter will route GET /health here in a later task.
 * No database connection or credentials are needed.
 */
export function handleHealth(request) {
  const url = new URL(request.url);
  if (url.pathname !== "/health") {
    return new Response(JSON.stringify({ error: "not_found" }), {
      status: 404,
      headers: { "content-type": "application/json; charset=utf-8", "cache-control": "no-store" },
    });
  }
  if (request.method !== "GET") {
    return new Response(JSON.stringify({ error: "method_not_allowed" }), {
      status: 405,
      headers: {
        "content-type": "application/json; charset=utf-8",
        "cache-control": "no-store",
        allow: "GET",
      },
    });
  }
  return new Response(JSON.stringify({ status: "ok", service: "natra-sync-api" }), {
    status: 200,
    headers: { "content-type": "application/json; charset=utf-8", "cache-control": "no-store" },
  });
}
