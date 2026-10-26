export function validateAllowance(value) {
  const keys="completed_today,daily_limit,day,remaining_today,reserved_today,reset_at,session_permitted";
  if(!value || Object.keys(value).sort().join(",")!==keys || value.daily_limit!==10 ||
    ![value.completed_today,value.reserved_today,value.remaining_today].every(n=>Number.isSafeInteger(n)&&n>=0) ||
    value.reserved_today>1 || value.remaining_today!==Math.max(0,10-value.completed_today-value.reserved_today) ||
    typeof value.session_permitted!=="boolean" ||
    (value.session_permitted && (value.completed_today>=10 || value.reserved_today!==1)) || typeof value.day!=="string" || !/^\d{4}-\d{2}-\d{2}$/.test(value.day) ||
    typeof value.reset_at!=="string" || !Number.isFinite(Date.parse(value.reset_at)))throw new Error("Invalid sync allowance");
  return value;
}
export async function readAllowance({endpoint,token,fetchImpl=globalThis.fetch}) {
  const url=new URL(endpoint);
  if(url.protocol!=="https:" || url.username || url.password || url.search || url.hash || url.pathname!=="/sync/allowance" || typeof token!=="string" || !/^[\x21-\x7e]{1,8192}$/.test(token))throw new Error("Invalid allowance endpoint");
  const controller=new AbortController(),timer=setTimeout(()=>controller.abort(),15000);
  try {
    const response=await fetchImpl(url.href,{headers:{authorization:"Bearer "+token},signal:controller.signal,
      redirect:"error",credentials:"omit",cache:"no-store",referrerPolicy:"no-referrer"});
    if(response.status!==200 || !response.body || (response.headers.get("content-type")??"").split(";")[0].trim().toLowerCase()!=="application/json")throw new Error("Allowance unavailable");
    const reader=response.body.getReader();const chunks=[];let length=0;
    try{for(;;){const {done,value}=await reader.read();if(done)break;length+=value.byteLength;
      if(length>2048){await reader.cancel();throw new Error("Allowance too large");}chunks.push(value);}}finally{reader.releaseLock();}
    const bytes=new Uint8Array(length);let offset=0;for(const c of chunks){bytes.set(c,offset);offset+=c.byteLength;}
    return validateAllowance(JSON.parse(new TextDecoder("utf-8",{fatal:true}).decode(bytes)));
  }catch{throw new Error("Sync allowance could not be confirmed");}finally{clearTimeout(timer);}
}
