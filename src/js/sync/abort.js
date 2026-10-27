// Forward exchange cancellation without replacing each request's own timeout.
export function forwardAbort(signal,controller) {
  const abort=()=>controller.abort();
  if(signal){
    if(typeof signal.aborted!=="boolean" || typeof signal.addEventListener!=="function" || typeof signal.removeEventListener!=="function")throw new Error("Invalid cancellation signal");
    if(signal.aborted)abort();else signal.addEventListener("abort",abort,{once:true});
  }
  return ()=>signal?.removeEventListener("abort",abort);
}
