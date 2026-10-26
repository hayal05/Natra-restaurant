const routes={"/sync/upload":"POST","/sync/pull":"GET","/sync/complete":"POST","/sync/fail":"POST","/sync/allowance":"GET"};
export async function withSyncCors(request,env={},handle) {
  const origin=request.headers.get("origin");
  if(!origin) return handle();
  let allowed=[];
  try {allowed=JSON.parse(env.SYNC_ALLOWED_ORIGINS??"[]");} catch {}
  const permitted=Array.isArray(allowed) && allowed.every(value=>typeof value==="string" && value!=="*") &&
    origin!=="null" && allowed.includes(origin);
  const deny=()=>new Response(JSON.stringify({error:"origin_not_allowed"}),{status:403,headers:{
    "content-type":"application/json; charset=utf-8","cache-control":"no-store","vary":"Origin",
  }});
  if(!permitted) return deny();
  const method=routes[new URL(request.url).pathname];
  if(request.method==="OPTIONS") {
    const requested=request.headers.get("access-control-request-method");
    const names=(request.headers.get("access-control-request-headers")??"").split(",").map(value=>value.trim().toLowerCase()).filter(Boolean);
    if(!method || requested!==method || names.some(name=>!["authorization","content-type"].includes(name))) return deny();
    return new Response(null,{status:204,headers:{"access-control-allow-origin":origin,
      "access-control-allow-methods":method,"access-control-allow-headers":"authorization, content-type",
      "vary":"Origin, Access-Control-Request-Method, Access-Control-Request-Headers","cache-control":"no-store"}});
  }
  const response=await handle();
  const headers=new Headers(response.headers);
  headers.set("access-control-allow-origin",origin);
  headers.set("vary","Origin");
  return new Response(response.body,{status:response.status,headers});
}
