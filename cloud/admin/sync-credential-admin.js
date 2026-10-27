import {pathToFileURL} from "node:url";
import {createNeonClient} from "../db/neon-session.js";
import {newSyncCredential,credentialDigest,localUserId} from "../auth/credential.js";
import {uuid} from "../../src/js/sync/upload_protocol.js";
export const usage="Usage: node cloud/admin/sync-credential-admin.js create WORKSPACE_UUID INSTALLATION_UUID LOCAL_USER_ID SUBJECT\n       node cloud/admin/sync-credential-admin.js revoke WORKSPACE_UUID INSTALLATION_UUID CREDENTIAL_UUID\n";
export function parseCredentialCommand(args){
  const [action,workspace_id,installation_id,value,subject]=args;
  if(!["create","revoke"].includes(action) || args.length!==(action==="create"?5:4) ||
    typeof workspace_id!=="string" || workspace_id.length!==36 || !uuid.test(workspace_id) ||
    typeof installation_id!=="string" || installation_id.length!==36 || !uuid.test(installation_id))throw new Error("Invalid credential command");
  if(action==="create"){
    localUserId(value);
    if(typeof subject!=="string" || !subject.trim() || subject.length>256)throw new Error("Invalid subject");
    return {action,workspace_id,installation_id,local_user_id:value,subject};
  }
  if(typeof value!=="string" || value.length!==36 || !uuid.test(value))throw new Error("Invalid credential identity");
  return {action,workspace_id,installation_id,credential_id:value};
}
// Restricted issuer/administrator host only; no HTTP endpoint or desktop IPC
// can create or choose these mappings. Successful stdout contains a secret.
export async function runCredentialCommand(args,env,createSql){
  const scope=parseCredentialCommand(args),sql=createNeonClient(env.NEON_DATABASE_URL,createSql);
  const options={fetchOptions:{signal:AbortSignal.timeout(10000)}};
  if(scope.action==="revoke"){
    const rows=await sql.query("SELECT public.revoke_sync_credential($1::uuid,$2::uuid,$3::uuid) AS revoked",
      [scope.workspace_id,scope.installation_id,scope.credential_id],options);
    if(!Array.isArray(rows) || rows.length!==1 || rows[0]?.revoked!==true)throw new Error("Revocation failed");
    return {credential_id:scope.credential_id,revoked:true};
  }
  const credential_id=crypto.randomUUID(),credential=newSyncCredential(),digest=await credentialDigest(credential);
  const rows=await sql.query("SELECT public.register_sync_credential($1::uuid,$2::uuid,$3::uuid,$4::bigint,$5::text,$6::text) AS credential_id",
    [credential_id,scope.workspace_id,scope.installation_id,scope.local_user_id,scope.subject,digest],options);
  if(!Array.isArray(rows) || rows.length!==1 || rows[0]?.credential_id!==credential_id)throw new Error("Registration failed");
  return {credential_id,workspace_id:scope.workspace_id,installation_id:scope.installation_id,
    local_user_id:scope.local_user_id,subject:scope.subject,credential};
}
if(process.argv[1] && import.meta.url===pathToFileURL(process.argv[1]).href){
  if(process.argv.length===3 && process.argv[2]==="--help")process.stdout.write(usage);
  else try{process.stdout.write(JSON.stringify(await runCredentialCommand(process.argv.slice(2),process.env))+"\n");}
  catch{process.stderr.write("Sync credential administration failed. Review scope, configuration and existing grants.\n");process.exitCode=1;}
}
