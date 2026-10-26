import {forwardAbort} from "./abort.js";
// Staged transport only. Task 38 binds the existing atomic SQLite page apply
// helper and durable cursor reader. Loading this module starts no request.
import {parseCursor,validateDownloadPage,MAX_DOWNLOAD_BYTES} from "./download_protocol.js";

export async function downloadPage({endpoint,token,readCursor,applyPage,limit=100,fetchImpl=globalThis.fetch,signal}) {
  const url=new URL(endpoint);
  if (url.protocol!=="https:" || url.username || url.password || url.search || url.hash || url.pathname!=="/sync/pull" ||
      typeof token!=="string" || !/^[\x21-\x7e]{1,8192}$/.test(token) ||
      typeof readCursor!=="function" || typeof applyPage!=="function" || typeof fetchImpl!=="function" ||
      !Number.isInteger(limit) || limit<1 || limit>500) throw new Error("Invalid cloud download configuration");
  const after=await readCursor();
  parseCursor(after);
  url.searchParams.set("after",after);
  url.searchParams.set("limit",String(limit));
  const controller=new AbortController();
  const detach=forwardAbort(signal,controller);
  const timer=setTimeout(()=>controller.abort(),30000);
  try {
    if(controller.signal.aborted)throw new Error("Sync request cancelled");
    const response=await fetchImpl(url.href,{method:"GET",headers:{authorization:"Bearer "+token},
      signal:controller.signal,redirect:"error",credentials:"omit",cache:"no-store",referrerPolicy:"no-referrer"});
    if (response.status!==200 || !response.body ||
        (response.headers.get("content-type")??"").split(";")[0].trim().toLowerCase()!=="application/json") {
      throw new Error("Invalid download response");
    }
    const reader=response.body.getReader();
    const chunks=[];
    let length=0;
    try {
      for (;;) {
        const {done,value}=await reader.read();
        if (done) break;
        length+=value.byteLength;
        if (length>MAX_DOWNLOAD_BYTES) {await reader.cancel();throw new Error("Download too large");}
        chunks.push(value);
      }
    } finally {reader.releaseLock();}
    const bytes=new Uint8Array(length);
    let offset=0;
    for (const chunk of chunks) {bytes.set(chunk,offset);offset+=chunk.byteLength;}
    const page=JSON.parse(new TextDecoder("utf-8",{fatal:true}).decode(bytes));
    validateDownloadPage(page,after,limit);
    if (page.events.length) {
      // No independent cursor setter: the existing SQLite helper must commit
      // business rows, receipts and the cursor in ONE transaction.
      await applyPage(page.events);
      if (await readCursor()!==page.next_cursor) throw new Error("Cursor was not durably committed");
    }
    return {downloaded:page.events.length,next_cursor:page.next_cursor,has_more:page.has_more};
  } catch {
    throw new Error("Cloud download did not finish; retain the durable cursor for retry");
  } finally {clearTimeout(timer);detach();}
}
