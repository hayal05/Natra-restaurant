import { createRemoteJWKSet, jwtVerify } from "jose";

// Public verification keys only; signing keys never belong in this service.
export function createJwtAuthVerifier({issuer, audience, jwksUrl}, resolveKeys) {
  const issuerUrl = new URL(issuer);
  const keysUrl = new URL(jwksUrl);
  for (const url of [issuerUrl, keysUrl]) {
    if (url.protocol !== "https:" || url.username || url.password || url.hash || url.search) {
      throw new Error("Invalid sync authentication configuration");
    }
  }
  if (typeof audience !== "string" || !audience.trim()) {
    throw new Error("Invalid sync authentication audience");
  }
  // The key URL comes exclusively from server configuration, never a JWT
  // header. Retain this resolver across requests for caching/key rotation.
  const keys = resolveKeys ?? createRemoteJWKSet(keysUrl, {
    timeoutDuration: 5000,
    cooldownDuration: 30000,
    cacheMaxAge: 600000,
  });
  return async token => {
    const {payload} = await jwtVerify(token, keys, {
      algorithms: ["RS256"],
      issuer,
      audience,
      requiredClaims: ["exp", "sub", "workspace_id", "installation_id", "session_id", "scope"],
    });
    if (typeof payload.sub !== "string" || !payload.sub.trim() ||
        typeof payload.scope !== "string" || !payload.scope.split(/\s+/).includes("natra:sync")) {
      throw new Error("Sync permission required");
    }
    // UUID validation and current database authorization follow in the API.
    // Only the trusted issuer may grant workspace/installation/session claims.
    return {
      authorized: true,
      workspace_id: payload.workspace_id,
      installation_id: payload.installation_id,
      session_authorized: true,
      session_id: payload.session_id,
      session_workspace_id: payload.workspace_id,
      session_installation_id: payload.installation_id,
    };
  };
}

let cached;
export function configuredAuthVerifier(env) {
  if (env.SYNC_AUTH_VERIFIER !== undefined) return env.SYNC_AUTH_VERIFIER;
  const config = {
    issuer: env.SYNC_JWT_ISSUER,
    audience: env.SYNC_JWT_AUDIENCE,
    jwksUrl: env.SYNC_JWKS_URL,
  };
  const key = JSON.stringify(config);
  if (cached?.key !== key) {
    cached = {key, verify:createJwtAuthVerifier(config)};
  }
  return cached.verify;
}
