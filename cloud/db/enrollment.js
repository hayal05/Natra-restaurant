import { createNeonClient } from "./neon-session.js";

// Trusted administrator composition only: no public enrollment route exists.
// The caller must authorize workspace administration before using this helper.
export function createDeviceEnroller(connectionString, createSql) {
  const sql = createNeonClient(connectionString, createSql);
  return async ({workspace_id, installation_id}) => {
    const rows = await sql.query(
      "SELECT public.enroll_sync_device($1::uuid, $2::uuid) AS installation_id",
      [workspace_id, installation_id],
      {fetchOptions:{signal:AbortSignal.timeout(10000)}});
    if (!Array.isArray(rows) || rows.length !== 1 ||
        typeof rows[0]?.installation_id !== "string" ||
        rows[0].installation_id.toLowerCase() !== installation_id.toLowerCase()) {
      throw new Error("Device enrollment failed");
    }
    return rows[0].installation_id;
  };
}
