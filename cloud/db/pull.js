import {createNeonClient} from "./neon-session.js";
import {uuid} from "../../src/js/sync/upload_protocol.js";
import {parseCursor,validateDownloadPage,MAX_DOWNLOAD_BYTES} from "../../src/js/sync/download_protocol.js";

export function createEventPuller(connectionString,createSql) {
  const sql=createNeonClient(connectionString,createSql);
  return async (scope,after,limit) => {
    parseCursor(after);
    if (!Number.isInteger(limit) || limit<1 || limit>500) throw new Error("Invalid pull limit");
    for (const field of ["workspace_id","installation_id","session_id"]) {
      if (typeof scope[field]!=="string" || !uuid.test(scope[field])) throw new Error("Invalid pull scope");
    }
    const rows=await sql.query("SELECT event_id,sequence_id::text AS sequence_id,origin_installation_id,entity_type,entity_id,operation,payload,is_lookahead FROM public.pull_authorized_sync_events($1::uuid,$2::uuid,$3::uuid,$4::bigint,$5::integer)",
      [scope.workspace_id,scope.installation_id,scope.session_id,after,limit],
      {fetchOptions:{signal:AbortSignal.timeout(10000)}});
    if (!Array.isArray(rows) || rows.length>limit+1) throw new Error("Invalid pull result");
    const events=[];
    let hasMore=false;
    for (let i=0;i<rows.length;i++) {
      const {is_lookahead,...event}=rows[i];
      if (typeof is_lookahead!=="boolean" || (is_lookahead && i!==rows.length-1)) throw new Error("Invalid pull boundary");
      if (is_lookahead) {hasMore=true;break;}
      events.push(event);
    }
    if (!events.length && hasMore) throw new Error("First cloud event exceeds page limit");
    const page={events,next_cursor:events.at(-1)?.sequence_id??after,has_more:hasMore};
    validateDownloadPage(page,after,limit);
    if (new TextEncoder().encode(JSON.stringify(page)).byteLength>MAX_DOWNLOAD_BYTES) throw new Error("Pull page too large");
    return page;
  };
}
