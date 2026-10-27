import {createNeonClient} from "./neon-session.js";
import {uuid} from "../../src/js/sync/upload_protocol.js";
import {validateAllowance} from "../../src/js/sync/allowance.js";
export function createAllowanceReader(connectionString,createSql){
  const sql=createNeonClient(connectionString,createSql);
  return async scope=>{
    for(const key of ["workspace_id","installation_id","session_id"])if(typeof scope[key]!=="string"||!uuid.test(scope[key]))throw new Error("Invalid allowance scope");
    const rows=await sql.query(`SELECT daily_limit,completed_today,reserved_today,remaining_today,session_permitted,day,to_char(reset_at AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS"Z"') AS reset_at FROM public.authorized_sync_allowance($1::uuid,$2::uuid,$3::uuid)`,
      [scope.workspace_id,scope.installation_id,scope.session_id],{fetchOptions:{signal:AbortSignal.timeout(10000)}});
    if(!Array.isArray(rows)||rows.length!==1)throw new Error("Invalid allowance result");
    return validateAllowance(rows[0]);
  };
}
