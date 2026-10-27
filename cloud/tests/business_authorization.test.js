import test from "node:test";
import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import {canVisit,landingFor} from "../../src/js/auth/access.js";
import {store,setUser} from "../../src/js/state.js";
const root=new URL("../../",import.meta.url);
const posCommands=new Set(["checkout","list_sales","list_waiters","list_categories","list_items","get_settings"]);

test("every business IPC callback checks native authority before running business/file work",async()=>{
  const lib=await readFile(new URL("src-tauri/src/lib.rs",root),"utf8");
  const registrations=[...lib.matchAll(/commands::(\w+)::(\w+),/g)].map(m=>({module:m[1],name:m[2]}));
  const business=registrations.filter(c=>!["auth","cloud_sync"].includes(c.module));
  assert.equal(business.length,48);
  for(const {module,name} of business){
    const source=await readFile(new URL(`src-tauri/src/commands/${module}.rs`,root),"utf8");
    const fn=source.slice(source.indexOf(`pub fn ${name}(`)).split(/\n(?:pub |#\[tauri::command\])/)[0];
    assert.ok(fn.includes("bridge: State<CloudBridge>"),name);
    const guard=`let _authorization = bridge.authorize_business(&conn, BusinessPermission::${posCommands.has(name)?"Pos":"Administrator"})?;`;
    assert.ok(fn.includes(guard),name);
    // Only locking may precede authorization: file paths/queries/exports come later.
    const body=fn.slice(fn.indexOf("{")+1,fn.indexOf(guard)).trim();
    assert.match(body,/^let (?:mut )?conn = db.lock\(\);$/,name);
    assert.equal((fn.match(/db\.lock\(\)/g)??[]).length,1,name+" must not re-lock SQLite");
  }
});
test("legacy direct sync cannot enable or run around the approved closed lease path",async()=>{
  const source=await readFile(new URL("src-tauri/src/commands/settings.rs",root),"utf8");
  assert.ok(!source.includes("settings_service::enable_sync("));
  assert.ok(!source.includes("sync::run_once("));
  assert.ok(source.includes("legacy synchronization cannot be enabled"));
});
test("staff UI stays on POS while administrator routes remain available",()=>{
  const staff={role:"staff"},admin={role:"admin"};
  assert.equal(landingFor(staff),"/pos");assert.equal(landingFor(admin),"/dashboard");
  assert.ok(canVisit(staff,"/pos"));
  for(const path of ["/dashboard","/settings","/reports","/archives","/items","/waiters","/expenses","/raw-materials/closing"]){
    assert.equal(canVisit(staff,path),false);assert.equal(canVisit(admin,path),true);
  }
  assert.equal(canVisit(null,"/pos"),false);assert.equal(canVisit({role:"unknown"},"/pos"),false);
});
test("native session expiry is surfaced once and stale failures cannot clear a new login",async()=>{
  const previous=globalThis.window;const events=[];
  const ended="authentication error: your local session has ended; please sign in again";
  let reject;
  globalThis.window={__TAURI__:{core:{invoke:()=>new Promise((_,r)=>{reject=r;})}},dispatchEvent:e=>events.push(e.type)};
  try{
    const {settings}=await import("../../src/js/api.js");
    setUser({id:1,role:"admin"});const current=settings.get();reject(ended);
    await assert.rejects(current,e=>e===ended);assert.deepEqual(events,["native-session-ended"]);
    const old=settings.get();setUser({id:1,role:"admin"});reject(ended);
    await assert.rejects(old,e=>e===ended);assert.equal(events.length,1);
    globalThis.window.__TAURI__.core.invoke=async()=>{throw "permission denied: administrator access is required";};
    await assert.rejects(settings.get());assert.equal(events.length,1);
  }finally{setUser(null);globalThis.window=previous;}
});
