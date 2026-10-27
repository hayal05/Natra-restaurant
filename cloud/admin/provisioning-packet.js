import {pathToFileURL} from "node:url";
import {credentialPattern,localUserId} from "../auth/credential.js";
import {uuid} from "../../src/js/sync/upload_protocol.js";
const grantKeys=["credential_id","workspace_id","installation_id","local_user_id","subject","credential"];
const approvalKeys=["credential_id","workspace_id","installation_id","local_user_id","subject","broker_endpoint","sync_origin","issuer","audience"];
const bytes=value=>new TextEncoder().encode(value).byteLength;
function exact(value,keys){
  if(!value || typeof value!=="object" || Array.isArray(value) || Object.keys(value).sort().join()!==[...keys].sort().join())throw new Error("Invalid provisioning input");
}
function id(value){if(typeof value!=="string" || value.length!==36 || !uuid.test(value))throw new Error("Invalid identity");return value.toLowerCase();}
function text(value){if(typeof value!=="string" || !value.trim() || bytes(value)>256 || /[\u0000-\u001f\u007f-\u009f]/.test(value))throw new Error("Invalid approval");return value;}
function https(value){
  if(typeof value!=="string" || value.length>512 || /[^\x21-\x7e]/.test(value))throw new Error("Invalid endpoint");
  const url=new URL(value);
  if(url.protocol!=="https:" || url.username || url.password || value.includes("?") || value.includes("#") || (url.port && url.port!=="443"))throw new Error("Invalid endpoint");
  return url;
}
// Trusted issuer/operator host only. Approval must be independently verified;
// validating JSON is not enrollment approval. No database or network access.
export function buildProvisioningPacket(grant,approval){
  exact(grant,grantKeys);exact(approval,approvalKeys);
  const scope={};
  for(const name of ["credential_id","workspace_id","installation_id"]){
    scope[name]=id(grant[name]);if(scope[name]!==id(approval[name]))throw new Error("Approval mismatch");
  }
  scope.local_user_id=localUserId(grant.local_user_id);
  scope.subject=text(grant.subject);
  if(scope.local_user_id!==localUserId(approval.local_user_id) || scope.subject!==text(approval.subject))throw new Error("Approval mismatch");
  if(typeof grant.credential!=="string" || grant.credential.length!==54 || !credentialPattern.test(grant.credential))throw new Error("Invalid credential");
  const broker=https(approval.broker_endpoint),origin=https(approval.sync_origin);
  https(approval.issuer);text(approval.audience);
  if(broker.pathname!=="/session" || origin.pathname!=="/")throw new Error("Invalid endpoint path");
  const packet={version:1,...scope,credential:grant.credential,broker_endpoint:approval.broker_endpoint,
    sync_origin:approval.sync_origin,issuer:approval.issuer,audience:approval.audience};
  if(bytes(JSON.stringify(packet)+"\n")>2560)throw new Error("Provisioning packet too large");
  return packet;
}
export async function readProvisioningInput(stream){
  if(stream.isTTY)throw new Error("Piped input required");
  const parts=[];let size=0;
  for await(const part of stream){size+=part.length;if(size>8192)throw new Error("Provisioning input too large");parts.push(part);}
  const input=JSON.parse(new TextDecoder("utf-8",{fatal:true}).decode(Buffer.concat(parts)));exact(input,["grant","approval"]);
  return buildProvisioningPacket(input.grant,input.approval);
}
if(process.argv[1] && import.meta.url===pathToFileURL(process.argv[1]).href){
  if(process.argv.length===3 && process.argv[2]==="--help")process.stdout.write("Usage: node cloud/admin/provisioning-packet.js < approved-input.json\nSuccessful stdout contains a secret provisioning packet. Trusted operator host only.\n");
  else try{
    if(process.argv.length!==2)throw new Error("No command arguments accepted");
    process.stdout.write(JSON.stringify(await readProvisioningInput(process.stdin))+"\n");
  }catch{process.stderr.write("Provisioning packet preparation failed. Review approved scope and metadata.\n");process.exitCode=1;}
}
