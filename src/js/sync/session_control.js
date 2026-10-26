export async function finishSession({endpoint,token,action,fetchImpl=globalThis.fetch}) {
  const url=new URL(endpoint);
  if(!["complete","fail"].includes(action) || url.protocol!=="https:" || url.username || url.password || url.search || url.hash ||
    url.pathname!=="/sync/"+action || typeof token!=="string" || !/^[\x21-\x7e]{1,8192}$/.test(token)) throw new Error("Invalid session control configuration");
  const controller=new AbortController();
  const timer=setTimeout(()=>controller.abort(),15000);
  try {
    const response=await fetchImpl(url.href,{method:"POST",headers:{authorization:"Bearer "+token},
      signal:controller.signal,redirect:"error",credentials:"omit",cache:"no-store",referrerPolicy:"no-referrer"});
    if(response.status!==200 || !response.body ||
      (response.headers.get("content-type")??"").split(";")[0].trim().toLowerCase()!=="application/json") throw new Error("Invalid session response");
    const reader=response.body.getReader();
    const chunks=[];let length=0;
    try {
      for(;;){const {done,value}=await reader.read();if(done)break;length+=value.byteLength;
        if(length>1024){await reader.cancel();throw new Error("Invalid session response");}chunks.push(value);}
    } finally{reader.releaseLock();}
    const bytes=new Uint8Array(length);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.byteLength;}
    const result=JSON.parse(new TextDecoder("utf-8",{fatal:true}).decode(bytes));
    if(!result || Object.keys(result).join(",")!=="accepted" || typeof result.accepted!=="boolean" ||
      (action==="complete" && !result.accepted)) throw new Error("Session was not completed");
    return result.accepted;
  } catch {throw new Error("Cloud session confirmation did not finish");}
  finally {clearTimeout(timer);}
}
