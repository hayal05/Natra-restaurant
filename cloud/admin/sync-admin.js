import { readFile } from "node:fs/promises";
import { pathToFileURL } from "node:url";
import { createDeviceEnroller } from "../db/enrollment.js";
import { createSessionTokenIssuer } from "../auth/issue-token.js";

export const usage = "Usage: node cloud/admin/sync-admin.js enroll WORKSPACE_UUID INSTALLATION_UUID\n       node cloud/admin/sync-admin.js issue-token WORKSPACE_UUID INSTALLATION_UUID SUBJECT\n";

// Host/OS access plus provisioning credentials establish the administrator
// boundary. This command is not exposed through an HTTP or desktop route.
export function parseAdminCommand(args) {
  const [action,workspace_id,installation_id,subject] = args;
  const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
  if (!(["enroll","issue-token"].includes(action)) ||
      args.length !== (action === "enroll" ? 3 : 4) ||
      !uuid.test(workspace_id ?? "") || !uuid.test(installation_id ?? "") ||
      (action === "issue-token" && (!subject?.trim() || subject.length > 256))) {
    throw new Error("Invalid administration command");
  }
  return {action,workspace_id,installation_id,subject};
}

export async function runAdminCommand(args,env) {
  const {action,...scope} = parseAdminCommand(args);
  if (action === "enroll") {
    const enroll = createDeviceEnroller(env.NEON_DATABASE_URL);
    return {installation_id:await enroll(scope)};
  }
  // Read a restricted server key file; never accept private keys in arguments.
  const privateKeyPem = await readFile(env.SYNC_SIGNING_KEY_FILE,"utf8");
  const issue = await createSessionTokenIssuer({
    connectionString:env.NEON_DATABASE_URL,
    issuer:env.SYNC_JWT_ISSUER,audience:env.SYNC_JWT_AUDIENCE,
    kid:env.SYNC_SIGNING_KEY_ID,privateKeyPem,
  });
  return issue(scope);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  if (process.argv.length === 3 && process.argv[2] === "--help") {
    process.stdout.write(usage);
  } else {
    try {
      process.stdout.write(JSON.stringify(await runAdminCommand(process.argv.slice(2),process.env))+"\n");
    } catch {
      // Underlying exceptions may include connection strings or key contents.
      process.stderr.write("Sync administration failed. Check arguments, server configuration and device/session eligibility.\n");
      process.exitCode = 1;
    }
  }
}
