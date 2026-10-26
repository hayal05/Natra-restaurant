import {uuid, validateUploadBatch} from "./upload_protocol.js";
export const MAX_DOWNLOAD_BYTES = 2097152;
export function parseCursor(value) {
  if (typeof value !== "string" || !/^(0|[1-9][0-9]{0,18})$/.test(value) || BigInt(value)>9223372036854775807n) {
    throw new Error("Invalid cloud pull cursor");
  }
  return BigInt(value);
}
export function validateDownloadPage(page, after, limit) {
  let previous = parseCursor(after);
  if (!Number.isInteger(limit) || limit<1 || limit>500 || !page || typeof page!=="object" ||
      Object.keys(page).sort().join(",")!=="events,has_more,next_cursor" ||
      !Array.isArray(page.events) || page.events.length>limit || typeof page.has_more!=="boolean" ||
      (!page.events.length && page.has_more)) throw new Error("Invalid download page");
  const seen = new Set();
  for (const event of page.events) {
    if (!event || Object.keys(event).sort().join(",")!=="entity_id,entity_type,event_id,operation,origin_installation_id,payload,sequence_id" ||
        typeof event.origin_installation_id!=="string" || !uuid.test(event.origin_installation_id)) throw new Error("Invalid download event");
    const next = parseCursor(event.sequence_id);
    if (next<=previous || typeof event.event_id!=="string" || seen.has(event.event_id.toLowerCase())) throw new Error("Unordered or duplicate download event");
    seen.add(event.event_id.toLowerCase());
    const {sequence_id,origin_installation_id,...envelope} = event;
    validateUploadBatch([envelope],{installation_id:origin_installation_id});
    previous=next;
  }
  if (parseCursor(page.next_cursor)!==previous) throw new Error("Download cursor does not match page");
}
