import { importPKCS8, SignJWT } from "jose";
import { createNeonClient } from "../db/neon-session.js";

// Trusted issuer only: CLI or separately deployed broker. Never import into
// the sync/data Worker or desktop app.
// The administrator must authorize this subject/workspace/device beforehand.
export async function createReservedTokenSigner({issuer, audience, kid, privateKeyPem}) {
  const url = new URL(issuer);
  if (url.protocol !== "https:" || url.username || url.password || url.search || url.hash ||
      typeof audience !== "string" || !audience.trim() || audience.length > 256 ||
      typeof kid !== "string" || !kid.trim() || kid.length > 128) {
    throw new Error("Invalid issuer configuration");
  }
  const key = await importPKCS8(privateKeyPem,"RS256");
  if (key.algorithm.modulusLength < 2048) throw new Error("Invalid signing key");
  return async (scope,reservation) => {
    const {workspace_id,installation_id,subject,credential_id,local_user_id} = scope;
    const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
    if (typeof workspace_id !== "string" || !uuid.test(workspace_id) ||
        typeof installation_id !== "string" || !uuid.test(installation_id) ||
        typeof subject !== "string" || !subject.trim() || subject.length > 256) {
      throw new Error("Invalid token scope");
    }
    const {session_id,started_at,expires_at} = reservation;
    if (typeof session_id!=="string" || !uuid.test(session_id)) throw new Error("Invalid session reservation");
    const iat = Math.floor(new Date(started_at).getTime()/1000);
    const exp = Math.floor(new Date(expires_at).getTime()/1000);
    if (!Number.isFinite(iat) || !Number.isFinite(exp) || exp<=iat || exp-iat>900 ||
        exp<=Math.floor(Date.now()/1000)) throw new Error("Invalid session deadline");
    const claims={workspace_id:workspace_id.toLowerCase(),installation_id:installation_id.toLowerCase(),session_id,scope:"natra:sync"};
    if (credential_id!==undefined || local_user_id!==undefined) {
      if(typeof credential_id!=="string" || !uuid.test(credential_id) || typeof local_user_id!=="string" ||
         !/^[1-9][0-9]{0,18}$/.test(local_user_id) || BigInt(local_user_id)>9223372036854775807n) throw new Error("Invalid credential scope");
      Object.assign(claims,{credential_id,local_user_id});
    }
    const token=await new SignJWT(claims).setProtectedHeader({alg:"RS256",kid,typ:"JWT"})
      .setIssuer(issuer).setAudience(audience).setSubject(subject).setIssuedAt(iat).setExpirationTime(exp)
      .setJti(crypto.randomUUID()).sign(key);
    return {token,session_id,expires_at:new Date(exp*1000).toISOString()};
  };
}

export async function createSessionTokenIssuer(config,createSql) {
  const sign=await createReservedTokenSigner(config);
  const sql=createNeonClient(config.connectionString,createSql);
  return async scope=>{
    let {workspace_id,installation_id,subject}=scope;
    const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
    if (typeof workspace_id !== "string" || !uuid.test(workspace_id) ||
        typeof installation_id !== "string" || !uuid.test(installation_id) ||
        typeof subject !== "string" || !subject.trim() || subject.length > 256) {
      throw new Error("Invalid token scope");
    }
    workspace_id = workspace_id.toLowerCase();
    installation_id = installation_id.toLowerCase();
    const session_id=crypto.randomUUID();
    const options=()=>({fetchOptions:{signal:AbortSignal.timeout(10000)}});
    try {
      const rows=await sql.query("SELECT * FROM public.reserve_authorized_sync_session($1::uuid,$2::uuid,$3::uuid)",
        [workspace_id,installation_id,session_id],options());
      if(!Array.isArray(rows) || rows.length!==1 || rows[0]?.session_id!==session_id) throw new Error("Invalid reservation");
      return await sign({workspace_id,installation_id,subject},rows[0]);
    }catch{
      try{await sql.query("SELECT public.fail_sync_session($1::uuid,$2::uuid)",[installation_id,session_id],options());}catch{}
      throw new Error("Sync token issuance failed");
    }
  };
}
