import {readAllowance} from "./allowance.js";
import {uploadPendingBatch} from "./cloud_upload.js";
import {downloadPage} from "./cloud_download.js";
import {finishSession} from "./session_control.js";

// Consistency check only; the server still verifies the JWT signature.
function bindTokenToInstallation(token,installationId) {
  const parts=token.split(".");
  if(parts.length!==3 || !/^[A-Za-z0-9_-]+$/.test(parts[1]))throw new Error("Invalid provisioned token");
  const encoded=parts[1].replace(/-/g,"+").replace(/_/g,"/");
  const binary=atob(encoded+"=".repeat((4-encoded.length%4)%4));
  const claims=JSON.parse(new TextDecoder("utf-8",{fatal:true}).decode(Uint8Array.from(binary,c=>c.charCodeAt(0))));
  if(typeof claims.installation_id!=="string" || typeof installationId!=="string" ||
    claims.installation_id.toLowerCase()!==installationId.toLowerCase())throw new Error("Token installation mismatch");
}

// A manual coordinator; constructing/refreshing it never starts network I/O.
export function createSyncNowController({bridge,getUserKey,onState=()=>{},onAllowance=()=>{},isOnline=()=>globalThis.navigator?.onLine!==false,fetchImpl=globalThis.fetch}) {
  let running=false,generation=0,activeAbort;
  const emit=(status,available)=>onState(status,available);
  const validConfig=config=>{
    if(!config)return false;
    const paths={uploadEndpoint:"upload",pullEndpoint:"pull",completeEndpoint:"complete",failEndpoint:"fail",allowanceEndpoint:"allowance"};
    const origins=new Set();
    for(const [key,action] of Object.entries(paths)) {
      const url=new URL(config[key]);
      if(url.protocol!=="https:" || url.username || url.password || url.search || url.hash || url.pathname!=="/sync/"+action) throw new Error("Invalid sync endpoint");
      origins.add(url.origin);
    }
    if(origins.size!==1 || (config.broker!==true && (typeof config.token!=="string" || !/^[\x21-\x7e]{1,8192}$/.test(config.token)))) throw new Error("Invalid sync configuration");
    return true;
  };
  const controller = {
    invalidate(){generation++;activeAbort?.abort();emit(isOnline()?(running?"syncing":"ready"):"offline",false);},
    async refresh(){
      if(running)return;
      if(!isOnline()){emit("offline",false);return;}
      const version=generation,owner=getUserKey();
      let available=false;
      try {available=!!owner && validConfig(await bridge.configuration());}catch{}
      if(!running && generation===version && getUserKey()===owner)emit("ready",available);
    },
    async run(){
      if(running)throw new Error("Synchronization is already in progress");
      if(!isOnline()){emit("offline",false);return {ran:false,pushed:0,pulled:0};}
      running=true;activeAbort=new AbortController();
      const version=generation,owner=getUserKey();
      let lease,config,completed=false,tokenBound=false;
      let pushed=0,pulled=0;
      const current=()=>{if(!owner || !isOnline() || activeAbort.signal.aborted || generation!==version || getUserKey()!==owner)throw new Error("Sync owner changed");};
      try {
        current();config=await bridge.configuration();
        if(!validConfig(config)){emit("ready",false);return {ran:false,pushed:0,pulled:0};}
        current();emit("syncing",true);
        lease=await bridge.begin();current();
        if(config.broker===true){
          const approvedOrigin=new URL(config.uploadEndpoint).origin;
          config=await bridge.credentials(lease.exchange_id);current();
          if(!validConfig(config) || typeof config.token!=="string" || !/^[\x21-\x7e]{1,8192}$/.test(config.token) ||
            new URL(config.uploadEndpoint).origin!==approvedOrigin)throw new Error("Invalid broker delivery");
        }
        bindTokenToInstallation(config.token,lease.installation_id);tokenBound=true;
        const allowance=await readAllowance({endpoint:config.allowanceEndpoint,token:config.token,fetchImpl,signal:activeAbort.signal});
        current();onAllowance(allowance);
        if(!allowance.session_permitted)throw new Error("No permitted daily sync session");
        // The backend lease fixes an initial rowid cutoff. Concurrent local
        // writes retain new IDs for the next manual cycle.
        for(let batch=0;;batch++) {
          current();const pending=await bridge.readPending(lease.exchange_id,500);
          if(!pending.length)break;
          if(batch>=100)throw new Error("Manual sync batch limit exceeded");
          const result=await uploadPendingBatch({endpoint:config.uploadEndpoint,token:config.token,
            installationId:lease.installation_id,fetchImpl,signal:activeAbort.signal,
            readPending:async()=>{current();return pending;},
            acknowledge:async ids=>{current();await bridge.acknowledge(lease.exchange_id,ids);}});
          if(result.uploaded<=0)throw new Error("Upload made no progress");
          pushed+=result.uploaded;
        }
        for(let page=0;;page++){
          if(page>=100)throw new Error("Manual sync page limit exceeded");
          current();let before;
          const result=await downloadPage({endpoint:config.pullEndpoint,token:config.token,fetchImpl,signal:activeAbort.signal,
            readCursor:async()=>{current();const cursor=await bridge.readCursor(lease.exchange_id);if(before===undefined)before=cursor;return cursor;},
            applyPage:async events=>{current();await bridge.applyPage(lease.exchange_id,before,events);}});
          pulled+=result.downloaded;
          if(!result.has_more)break;
        }
        current();await finishSession({endpoint:config.completeEndpoint,token:config.token,action:"complete",fetchImpl,signal:activeAbort.signal});
        completed=true;
        current();onAllowance(await readAllowance({endpoint:config.allowanceEndpoint,token:config.token,fetchImpl,signal:activeAbort.signal}));
        await bridge.end(lease.exchange_id);lease=null;current();emit("completed",true);
        return {ran:true,pushed,pulled};
      }catch{
        if(lease && config && tokenBound && !completed && isOnline()){try{await finishSession({endpoint:config.failEndpoint,token:config.token,action:"fail",fetchImpl});}catch{}}
        emit(isOnline()?"failed":"offline",isOnline() && !!owner && getUserKey()===owner && generation===version);
        throw new Error("Synchronization did not finish; retry when ready");
      }finally{
        if(lease){try{await bridge.end(lease.exchange_id);}catch{}}
        running=false;activeAbort=undefined;
        if(getUserKey()!==owner || generation!==version){
          emit(isOnline()?"ready":"offline",false);
          if(isOnline() && getUserKey())void controller.refresh();
        }
      }
    },
  };
  return controller;
}
