import { importPKCS8, SignJWT } from "jose";
import { createNeonClient } from "../db/neon-session.js";

// Trusted issuer process only. Never import into the Worker or desktop app.
// The administrator must authorize this subject/workspace/device beforehand.
export async function createSessionTokenIssuer({connectionString, issuer, audience, kid, privateKeyPem}, createSql) {
  const url = new URL(issuer);
  if (url.protocol !== "https:" || url.username || url.password || url.search || url.hash ||
      typeof audience !== "string" || !audience.trim() || audience.length > 256 ||
      typeof kid !== "string" || !kid.trim() || kid.length > 128) {
    throw new Error("Invalid issuer configuration");
  }
  const key = await importPKCS8(privateKeyPem,"RS256");
  if (key.algorithm.modulusLength < 2048) throw new Error("Invalid signing key");
  const sql = createNeonClient(connectionString,createSql);
  return async ({workspace_id, installation_id, subject}) => {
    const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
    if (typeof workspace_id !== "string" || !uuid.test(workspace_id) ||
        typeof installation_id !== "string" || !uuid.test(installation_id) ||
        typeof subject !== "string" || !subject.trim() || subject.length > 256) {
      throw new Error("Invalid token scope");
    }
    workspace_id = workspace_id.toLowerCase();
    installation_id = installation_id.toLowerCase();
    const session_id = crypto.randomUUID();
    const options = () => ({fetchOptions:{signal:AbortSignal.timeout(10000)}});
    try {
      const rows = await sql.query(
        "SELECT * FROM public.reserve_authorized_sync_session($1::uuid,$2::uuid,$3::uuid)",
        [workspace_id,installation_id,session_id],options());
      if (!Array.isArray(rows) || rows.length !== 1 || rows[0]?.session_id !== session_id) {
        throw new Error("Invalid session reservation");
      }
      const iat = Math.floor(new Date(rows[0].started_at).getTime()/1000);
      const exp = Math.floor(new Date(rows[0].expires_at).getTime()/1000);
      if (!Number.isFinite(iat) || !Number.isFinite(exp) || exp <= iat || exp-iat > 900 ||
          exp <= Math.floor(Date.now()/1000)) throw new Error("Invalid session deadline");
      const token = await new SignJWT({workspace_id,installation_id,session_id,scope:"natra:sync"})
        .setProtectedHeader({alg:"RS256",kid,typ:"JWT"})
        .setIssuer(issuer).setAudience(audience).setSubject(subject)
        .setIssuedAt(iat).setExpirationTime(exp).setJti(crypto.randomUUID()).sign(key);
      return {token,session_id,expires_at:new Date(exp*1000).toISOString()};
    } catch {
      // Also covers an ambiguous reservation response. This generated session
      // ID belongs only to this attempt; never release another request's ID.
      try {
        await sql.query("SELECT public.fail_sync_session($1::uuid,$2::uuid)",
          [installation_id,session_id],options());
      } catch {
        // Network failure can prevent cleanup. Abandoned sessions expire after
        // 15 minutes; never return a token or silently retry the reservation.
      }
      throw new Error("Sync token issuance failed");
    }
  };
}
