// Shared pure upload envelope validation; no server credentials or database code.
export const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const types = new Set(["categories","items","waiters","raw_materials","raw_material_purchases",
  "expenses","sales","sale_reversals","waiter_settlements","material_month_closings"]);
const object = value => value !== null && typeof value === "object" && !Array.isArray(value);
export const MAX_UPLOAD_BYTES = 1048576;

export function validateUploadBatch(events, scope) {
  if (!Array.isArray(events) || events.length < 1 || events.length > 500) throw new Error("Invalid upload batch");
  const seen = new Set();
  for (const event of events) {
    if (!object(event) || Object.keys(event).sort().join(",") !== "entity_id,entity_type,event_id,operation,payload" ||
        typeof event.event_id !== "string" || !uuid.test(event.event_id) ||
        !types.has(event.entity_type) || typeof event.entity_id !== "string" ||
        !event.entity_id.trim() || event.entity_id.length > 200 || event.operation !== "upsert" ||
        !object(event.payload)) throw new Error("Invalid upload event");
    const id = event.event_id.toLowerCase();
    if (seen.has(id)) throw new Error("Duplicate upload event");
    seen.add(id);
    if (Object.hasOwn(event.payload,"origin_installation_id") &&
        (typeof event.payload.origin_installation_id !== "string" ||
         event.payload.origin_installation_id.toLowerCase() !== scope.installation_id.toLowerCase())) {
      throw new Error("Invalid upload origin");
    }
  }
  const serialized = JSON.stringify(events);
  if (new TextEncoder().encode(serialized).byteLength > MAX_UPLOAD_BYTES) throw new Error("Upload too large");
  return serialized;
}

