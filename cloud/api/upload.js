import { MAX_UPLOAD_BYTES, validateUploadBatch } from "../db/upload.js";
const json = (status, body) => new Response(JSON.stringify(body),{status,headers:{
  "content-type":"application/json; charset=utf-8","cache-control":"no-store",
}});

export async function handleUploadRequest(request, scope, upload) {
  if ((request.headers.get("content-type")??"").split(";")[0].trim().toLowerCase() !== "application/json" ||
      ![null,"identity"].includes(request.headers.get("content-encoding"))) {
    return json(415,{error:"unsupported_media_type"});
  }
  if (!request.body) return json(400,{error:"invalid_upload_batch"});
  const reader = request.body.getReader();
  let events;
  try {
    const chunks = [];
    let length = 0;
    for (;;) {
      const {done,value} = await reader.read();
      if (done) break;
      length += value.byteLength;
      if (length > MAX_UPLOAD_BYTES) {
        await reader.cancel();
        return json(413,{error:"upload_too_large"});
      }
      chunks.push(value);
    }
    const bytes = new Uint8Array(length);
    let offset = 0;
    for (const chunk of chunks) {bytes.set(chunk,offset);offset+=chunk.byteLength;}
    const body = JSON.parse(new TextDecoder("utf-8",{fatal:true}).decode(bytes));
    if (!body || typeof body !== "object" || Array.isArray(body) ||
        Object.keys(body).length !== 1 || !Object.hasOwn(body,"events")) {
      return json(400,{error:"invalid_upload_batch"});
    }
    events = body.events;
    validateUploadBatch(events,scope);
  } catch {
    return json(400,{error:"invalid_upload_batch"});
  } finally {
    reader.releaseLock();
  }
  try {
    // Receipt is returned only after the database statement commits.
    return json(200,await upload(scope,events));
  } catch (error) {
    if (error?.code === "42501") return json(403,{error:"sync_session_required"});
    if (["22023","22P02","23502","23514"].includes(error?.code)) return json(400,{error:"invalid_upload_batch"});
    if (["P0001","23505"].includes(error?.code)) return json(409,{error:"upload_conflict"});
    // A timeout may follow a committed batch. Retry must retain event IDs.
    return json(503,{error:"upload_unavailable"});
  }
}
