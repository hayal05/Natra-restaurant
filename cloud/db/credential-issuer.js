import {createNeonClient} from "./neon-session.js";
import {createReservedTokenSigner} from "../auth/issue-token.js";
import {localUserId} from "../auth/credential.js";
import {uuid} from "../../src/js/sync/upload_protocol.js";

export async function createCredentialIssuer(config,createSql) {
  const sign=await createReservedTokenSigner(config);
  const sql=createNeonClient(config.connectionString,createSql);
  const options=()=>({fetchOptions:{signal:AbortSignal.timeout(10000)}});
  return async ({digest,installation_id,local_user_id})=>{
    if(typeof digest!=="string" || digest.length!==64 || !/^[0-9a-f]{64}$/.test(digest) || typeof installation_id!=="string" || installation_id.length!==36 || !uuid.test(installation_id))
      throw new Error("Invalid credential scope");
    localUserId(local_user_id);
    installation_id=installation_id.toLowerCase();
    const session_id=crypto.randomUUID();
    try{
      // Credential verification and scope derivation happen atomically with
      // reservation, under the same device lock as credential revocation.
      const rows=await sql.query("SELECT * FROM public.reserve_credential_sync_session($1::text,$2::uuid,$3::bigint,$4::uuid)",
        [digest,installation_id,local_user_id,session_id],options());
      const row=rows?.[0];
      if(!Array.isArray(rows) || rows.length!==1 || row?.session_id!==session_id ||
         row.installation_id!==installation_id || row.local_user_id!==local_user_id || !uuid.test(row.workspace_id??"") ||
         !uuid.test(row.credential_id??"") || typeof row.subject!=="string" || !row.subject.trim())throw new Error("Invalid reservation");
      const result=await sign(row,row);
      return {...result,credential_id:row.credential_id,workspace_id:row.workspace_id,installation_id,local_user_id,subject:row.subject};
    }catch(error){
      // Never retry an ambiguous reservation or release another attempt's ID.
      try{await sql.query("SELECT public.fail_sync_session($1::uuid,$2::uuid)",[installation_id,session_id],options());}catch{}
      const failure=new Error("Credential session issuance unavailable");
      if(error?.code==="42501")failure.code="42501";
      if(["P0001","23505"].includes(error?.code))failure.code="session_conflict";
      throw failure;
    }
  };
}
