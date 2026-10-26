import {createNeonClient} from "./neon-session.js";
import {uuid} from "../../src/js/sync/upload_protocol.js";
export function createSessionFinisher(connectionString,createSql) {
  const sql=createNeonClient(connectionString,createSql);
  return async(scope,action)=>{
    for(const name of ["workspace_id","installation_id","session_id"]) {
      if(typeof scope[name]!=="string" || !uuid.test(scope[name])) throw new Error("Invalid session scope");
    }
    if(!["complete","fail"].includes(action)) throw new Error("Invalid session action");
    const rows=await sql.query("SELECT public.finish_authorized_sync_session($1::uuid,$2::uuid,$3::uuid,$4::text) AS accepted",
      [scope.workspace_id,scope.installation_id,scope.session_id,action],
      {fetchOptions:{signal:AbortSignal.timeout(10000)}});
    if(!Array.isArray(rows) || rows.length!==1 || typeof rows[0]?.accepted!=="boolean") throw new Error("Invalid session result");
    return {accepted:rows[0].accepted};
  };
}
