import {parseCursor} from "../../src/js/sync/download_protocol.js";
const json=(status,body)=>new Response(JSON.stringify(body),{status,headers:{
  "content-type":"application/json; charset=utf-8","cache-control":"no-store",
}});
export async function handlePullRequest(request,scope,pull) {
  const query=new URL(request.url).searchParams;
  let after,limit;
  try {
    for (const key of query.keys()) if (!["after","limit"].includes(key) || query.getAll(key).length!==1) throw new Error("Invalid pull parameter");
    after=query.get("after")??"0";
    parseCursor(after);
    const raw=query.get("limit")??"100";
    if (!/^[1-9][0-9]{0,2}$/.test(raw)) throw new Error("Invalid limit");
    limit=Number(raw);
    if (limit>500) throw new Error("Invalid limit");
  } catch {return json(400,{error:"invalid_pull_parameters"});}
  try {return json(200,await pull(scope,after,limit));}
  catch(error) {
    if (error?.code==="42501") return json(403,{error:"sync_session_required"});
    return json(503,{error:"pull_unavailable"});
  }
}
