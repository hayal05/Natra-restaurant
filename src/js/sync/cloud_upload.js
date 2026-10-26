import {forwardAbort} from "./abort.js";
// Staged WebView fetch transport. Task 38 will provide the existing durable
// outbox read/ack callbacks and connect this to an explicit Sync Now action.
// The manual coordinator imports this module; loading it starts no request.
import {uuid, MAX_UPLOAD_BYTES, validateUploadBatch} from "./upload_protocol.js";
const MAX_REPLY_BYTES = 262144;

function prepareBatch(snapshot, installationId) {
  if (!Array.isArray(snapshot) || snapshot.length > 500) throw new Error("Invalid pending upload snapshot");
  let body = '{"events":[';
  let bytes = new TextEncoder().encode(body+"]}").byteLength;
  let count = 0;
  for (const event of snapshot) {
    const serialized = JSON.stringify(event);
    const addition = new TextEncoder().encode(serialized).byteLength + (count ? 1 : 0);
    if (bytes+addition > MAX_UPLOAD_BYTES) {
      if (!count) throw new Error("First pending upload event exceeds batch limit");
      break;
    }
    body += (count ? "," : "")+serialized;
    bytes += addition;
    count++;
  }
  body += "]}";
  const events = JSON.parse(body).events;
  if (count) validateUploadBatch(events,{installation_id:installationId});
  return {body, events};
}

export function validateUploadReceipt(reply, submitted) {
  if (!reply || typeof reply !== "object" ||
      Object.keys(reply).sort().join(",") !== "acknowledged_event_ids,receipts" ||
      !Array.isArray(reply.acknowledged_event_ids) || !Array.isArray(reply.receipts) ||
      reply.acknowledged_event_ids.length !== submitted.length || reply.receipts.length !== submitted.length) {
    throw new Error("Invalid cloud upload receipt");
  }
  const sequences = new Set();
  for (let i=0;i<submitted.length;i++) {
    const receipt = reply.receipts[i];
    if (reply.acknowledged_event_ids[i] !== submitted[i].toLowerCase() || !receipt ||
        Object.keys(receipt).sort().join(",") !== "event_id,sequence_id" ||
        receipt.event_id !== reply.acknowledged_event_ids[i] ||
        typeof receipt.sequence_id !== "string" || !/^[1-9][0-9]{0,18}$/.test(receipt.sequence_id) ||
        BigInt(receipt.sequence_id)>9223372036854775807n || sequences.has(receipt.sequence_id)) {
      throw new Error("Invalid cloud upload receipt");
    }
    sequences.add(receipt.sequence_id);
  }
}

async function readReply(response) {
  if (!response.body || (response.headers.get("content-type")??"").split(";")[0].trim().toLowerCase() !== "application/json") {
    throw new Error("Invalid cloud upload response");
  }
  const reader = response.body.getReader();
  const chunks = [];
  let length = 0;
  try {
    for (;;) {
      const {done,value} = await reader.read();
      if (done) break;
      length += value.byteLength;
      if (length>MAX_REPLY_BYTES) {await reader.cancel();throw new Error("Cloud upload response too large");}
      chunks.push(value);
    }
  } finally {reader.releaseLock();}
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {bytes.set(chunk,offset);offset+=chunk.byteLength;}
  return JSON.parse(new TextDecoder("utf-8",{fatal:true}).decode(bytes));
}

export async function uploadPendingBatch({endpoint,token,installationId,readPending,acknowledge,fetchImpl=globalThis.fetch,signal}) {
  const url = new URL(endpoint);
  if (url.protocol !== "https:" || url.username || url.password || url.search || url.hash || url.pathname !== "/sync/upload" ||
      typeof token !== "string" || !/^[\x21-\x7e]{1,8192}$/.test(token) ||
      typeof installationId !== "string" || !uuid.test(installationId) ||
      typeof readPending !== "function" || typeof acknowledge !== "function" || typeof fetchImpl !== "function") {
    throw new Error("Invalid cloud upload configuration");
  }
  // Snapshot IDs/payload before awaiting network I/O. A later local write is
  // a separate pending event and must never be acknowledged by this request.
  const snapshot = JSON.parse(JSON.stringify(await readPending(500)));
  const {body,events} = prepareBatch(snapshot,installationId);
  if (!events.length) return {uploaded:0};
  const submitted = events.map(event => event.event_id);
  const controller = new AbortController();
  const detach=forwardAbort(signal,controller);
  const timer = setTimeout(() => controller.abort(),30000);
  try {
    if(controller.signal.aborted)throw new Error("Sync request cancelled");
    const response = await fetchImpl(url.href,{
      method:"POST",headers:{authorization:"Bearer "+token,"content-type":"application/json"},
      body,signal:controller.signal,redirect:"error",credentials:"omit",cache:"no-store",referrerPolicy:"no-referrer",
    });
    if (response.status !== 200) throw new Error("Cloud upload was rejected");
    validateUploadReceipt(await readReply(response),submitted);
    // Callback must use the existing atomic SQLite acknowledge_uploads helper.
    // No local queue state changes before a fully validated committed receipt.
    await acknowledge(submitted);
    return {uploaded:submitted.length};
  } catch {
    // Never surface fetch errors containing URLs, credentials or server text.
    // Retain original outbox IDs; a timed-out server may already have committed.
    throw new Error("Cloud upload did not finish; retain pending events for retry");
  } finally {clearTimeout(timer);detach();}
}
