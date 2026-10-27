// Separate trusted issuer deployment, never imported by the sync/data Worker.
import {credentialDigest,credentialPattern,localUserId} from "./auth/credential.js";
import {createCredentialIssuer} from "./db/credential-issuer.js";
import {uuid} from "../src/js/sync/upload_protocol.js";
const json=(status,body)=>new Response(JSON.stringify(body),{status,headers:{
  "content-type":"application/json; charset=utf-8","cache-control":"no-store","pragma":"no-cache"}});
let cached;
async function configuredIssuer(env){
  if(env.SYNC_CREDENTIAL_ISSUER!==undefined)return env.SYNC_CREDENTIAL_ISSUER; // Trusted composition/tests only.
  const config={connectionString:env.NEON_DATABASE_URL,issuer:env.SYNC_JWT_ISSUER,audience:env.SYNC_JWT_AUDIENCE,
    kid:env.SYNC_SIGNING_KEY_ID,privateKeyPem:env.SYNC_BROKER_SIGNING_KEY};
  // Cache by configured values without serializing secrets into a loggable key.
  if(!cached || Object.keys(config).some(key=>config[key]!==cached.config[key]))
    cached={config,issue:await createCredentialIssuer(config)};
  return cached.issue;
}
async function readScope(request){
  if(!request.body)throw new Error("missing body");
  const reader=request.body.getReader();
  try{
    const chunks=[];let length=0,reads=0;
    for(;;){const {done,value}=await reader.read();if(done)break;
      length+=value.byteLength;if(++reads>64 || length>2048){await reader.cancel();throw new Error("body too large");}chunks.push(value);}
    const bytes=new Uint8Array(length);let offset=0;
    for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.byteLength;}
    const scope=JSON.parse(new TextDecoder("utf-8",{fatal:true}).decode(bytes));
    if(!scope || Array.isArray(scope) || typeof scope!=="object" || Object.keys(scope).length!==2 ||
       typeof scope.installation_id!=="string" || scope.installation_id.length!==36 || !uuid.test(scope.installation_id))throw new Error("invalid scope");
    localUserId(scope.local_user_id);
    return {installation_id:scope.installation_id.toLowerCase(),local_user_id:scope.local_user_id};
  }finally{reader.releaseLock();}
}
export default {async fetch(request,env={}){
  const url=new URL(request.url);
  if(url.pathname!=="/session" || url.search)return json(404,{error:"not_found"});
  if(request.method!=="POST")return json(405,{error:"method_not_allowed"});
  if(url.protocol!=="https:" || request.headers.has("origin"))return json(403,{error:"native_client_required"});
  if(env.SYNC_BROKER_ENABLED!=="true")return json(503,{error:"broker_not_configured"});
  // Both global and per-credential throttles are mandatory, fail closed.
  const limiter=env.SYNC_BROKER_RATE_LIMITER;
  if(typeof limiter?.limit!=="function")return json(503,{error:"broker_not_configured"});
  try{if((await limiter.limit({key:"broker:global"}))?.success!==true)return json(429,{error:"rate_limited"});}
  catch{return json(503,{error:"broker_unavailable"});}
  const authorization=request.headers.get("authorization")??"";
  if(authorization.length!==61 || !authorization.startsWith("Bearer ") || !credentialPattern.test(authorization.slice(7)))return json(401,{error:"unauthorized"});
  const digest=await credentialDigest(authorization.slice(7));
  try{if((await limiter.limit({key:"broker:credential:"+digest}))?.success!==true)return json(429,{error:"rate_limited"});}
  catch{return json(503,{error:"broker_unavailable"});}
  if((request.headers.get("content-type")??"").split(";")[0].trim().toLowerCase()!=="application/json" ||
     ![null,"identity"].includes(request.headers.get("content-encoding")))return json(415,{error:"unsupported_media_type"});
  let scope;try{scope=await readScope(request);}catch{return json(400,{error:"invalid_request"});}
  let issue;try{issue=await configuredIssuer(env);}catch{return json(503,{error:"broker_not_configured"});}
  if(typeof issue!=="function")return json(503,{error:"broker_not_configured"});
  try{return json(200,await issue({...scope,digest}));}
  catch(error){return json(error?.code==="42501"?401:error?.code==="session_conflict"?409:503,{error:"session_unavailable"});}
}};
