import { neon } from "@neondatabase/serverless";

// Keep synchronized with queries/validate_sync_session.sql (checked in tests).
export const VALIDATE_SESSION_SQL = `SELECT s.id AS session_id,
       d.workspace_id,
       d.installation_id
FROM sync_sessions AS s
JOIN sync_devices AS d ON d.installation_id = s.installation_id
WHERE d.workspace_id = $1::uuid
  AND d.installation_id = $2::uuid
  AND s.id = $3::uuid
  AND d.deactivated_at IS NULL
  AND s.status = 'running'
  AND s.completed_at IS NULL
  AND s.started_at <= statement_timestamp()
  AND s.started_at >= statement_timestamp() - interval '15 minutes'
FOR SHARE OF d, s;`;

// Server-only adapter. Construction does not contact Neon; only an already
// authenticated request may invoke the returned validator. Never send this
// connection string or this module to the desktop/browser application.
export function createNeonSessionValidator(connectionString, createSql = neon) {
  const url = new URL(connectionString);
  if (!["postgres:", "postgresql:"].includes(url.protocol) ||
      !url.hostname.endsWith(".neon.tech") || !url.username || !url.password ||
      url.pathname.length < 2 || (url.port && url.port !== "5432")) {
    throw new Error("Invalid Neon configuration");
  }
  const sql = createSql(connectionString);
  return async ({workspace_id, installation_id, session_id}) => {
    const rows = await sql.query(VALIDATE_SESSION_SQL,
      [workspace_id, installation_id, session_id],
      {fetchOptions:{signal:AbortSignal.timeout(10000)}});
    // Deny unexpected or ambiguous results, including a mismatched scope.
    if (!Array.isArray(rows) || rows.length !== 1) return false;
    const row = rows[0];
    return row?.workspace_id?.toLowerCase() === workspace_id.toLowerCase() &&
      row?.installation_id?.toLowerCase() === installation_id.toLowerCase() &&
      row?.session_id?.toLowerCase() === session_id.toLowerCase();
  };
}

// This is a validation-only query. Its locks end with the HTTP transaction.
// Future financial operations MUST revalidate in their own transaction; a
// successful preflight must never authorize a later write by itself.
