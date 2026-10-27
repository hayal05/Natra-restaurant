import { createNeonClient } from "./neon-session.js";

import {uuid, validateUploadBatch} from "../../src/js/sync/upload_protocol.js";
export {MAX_UPLOAD_BYTES, validateUploadBatch} from "../../src/js/sync/upload_protocol.js";

export function createBatchUploader(connectionString, createSql) {
  const sql = createNeonClient(connectionString,createSql);
  return async (scope, events) => {
    // These identifiers come from verified authentication, never request JSON.
    for (const name of ["workspace_id","installation_id","session_id"]) {
      if (typeof scope[name] !== "string" || !uuid.test(scope[name])) throw new Error("Invalid upload scope");
    }
    const serialized = validateUploadBatch(events,scope);
    // One statement is one database transaction: no partial acknowledgements.
    const rows = await sql.query("SELECT event_id,sequence_id::text AS sequence_id FROM public.upload_sync_batch($1::uuid,$2::uuid,$3::uuid,$4::jsonb)",
      [scope.workspace_id,scope.installation_id,scope.session_id,serialized],
      {fetchOptions:{signal:AbortSignal.timeout(10000)}});
    if (!Array.isArray(rows) || rows.length !== events.length) throw new Error("Invalid upload receipt");
    const receipts = rows.map((row,index) => {
      if (typeof row?.event_id !== "string" || row.event_id !== events[index].event_id.toLowerCase() ||
          typeof row.sequence_id !== "string" || !/^[1-9][0-9]{0,18}$/.test(row.sequence_id) ||
          BigInt(row.sequence_id) > 9223372036854775807n) throw new Error("Invalid upload receipt");
      return {event_id:row.event_id,sequence_id:row.sequence_id};
    });
    return {acknowledged_event_ids:receipts.map(row => row.event_id),receipts};
  };
}
