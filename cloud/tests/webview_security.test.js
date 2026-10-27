import test from "node:test";
import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import {escapeHtml,safePhotoUrl} from "../../src/js/utils/html.js";
import {tablePrintHtml} from "../../src/js/utils/print.js";
import {waiterReceiptHtml} from "../../src/js/utils/waiter-receipt.js";
const root=new URL("../../",import.meta.url);

test("untrusted display text is escaped for text and quoted attribute contexts",()=>{
  const attack=`'"><img src=x onerror=alert(1)>&`;
  const escaped=escapeHtml(attack);
  assert.equal(escaped,"&#39;&quot;&gt;&lt;img src=x onerror=alert(1)&gt;&amp;");
  assert.equal(escapeHtml(null),"");
});
test("portraits accept bounded raster data and reject executable or remote URLs",()=>{
  assert.equal(safePhotoUrl("data:image/png;base64,aGVsbG8="),"data:image/png;base64,aGVsbG8=");
  for(const value of ["https://tracker.example.test/photo.jpg","file:///secret","javascript:alert(1)",
    "data:image/svg+xml;base64,PHN2Zz4=","blob:http://tauri.localhost/anything","data:image/png;base64,aGVsbG8=\n",
    "data:image/png;base64,"+"A".repeat(2*1024*1024)])assert.equal(safePhotoUrl(value),null);
});
test("printed receipt and purchase-table fields cannot create markup",()=>{
  const attack='<img src=x onerror="parent.pwned=1">';
  const table=tablePrintHtml(attack,attack,[attack],[[attack]]);
  assert.ok(!table.includes("<img"));assert.ok(table.includes("&lt;img"));
  const receipt=waiterReceiptHtml({result:{sale:{id:attack,waiter_id:attack,created_at:"2026-10-27 06:00:00",total_amount:30},
    items:[{item_name:attack,quantity:attack,unit_price:15,line_total:30}]},waiterName:attack,cashierName:attack,restaurantName:attack,currency:"ETB"});
  assert.ok(!receipt.includes("<img"));assert.ok(receipt.includes("ETB 30.00"));
});
test("every registered command is explicitly enrolled and granted only to the local main window",async()=>{
  const [lib,names,cap,config,build]=await Promise.all([
    readFile(new URL("src-tauri/src/lib.rs",root),"utf8"),readFile(new URL("src-tauri/ipc_command_names.rs",root),"utf8"),
    readFile(new URL("src-tauri/capabilities/main-local.json",root),"utf8"),readFile(new URL("src-tauri/tauri.conf.json",root),"utf8"),
    readFile(new URL("src-tauri/build.rs",root),"utf8")]);
  const registered=[...lib.matchAll(/commands::\w+::(\w+),/g)].map(m=>m[1]).sort();
  const enrolled=[...names.matchAll(/"(\w+)"/g)].map(m=>m[1]).sort();
  assert.equal(registered.length,new Set(registered).size);assert.deepEqual(enrolled,registered);
  assert.ok(build.includes("AppManifest::new().commands(APP_COMMANDS)"));
  const c=JSON.parse(cap),conf=JSON.parse(config);
  assert.equal(c.local,true);assert.equal(c.remote,undefined);assert.deepEqual(c.windows,["main"]);
  assert.deepEqual([...c.permissions].sort(),registered.map(v=>"allow-"+v.replaceAll("_","-")).sort());
  assert.deepEqual(conf.app.security.capabilities,["main-local"]);
  assert.equal(conf.app.windows.length,1);assert.equal(conf.app.windows[0].label,"main");assert.equal(conf.app.windows[0].create,false);
  assert.ok(lib.includes(".on_navigation(webview_security::allow_main_navigation)"));assert.ok(lib.includes("NewWindowResponse::Deny"));
});
test("CSP forbids executable inline/remote content and broad network grants",async()=>{
  const config=JSON.parse(await readFile(new URL("src-tauri/tauri.conf.json",root),"utf8"));
  const csp=config.app.security.csp;
  assert.equal(csp["script-src"],"'self'");assert.equal(csp["script-src-attr"],"'none'");
  for(const directive of ["object-src","base-uri","form-action","worker-src"])assert.equal(csp[directive],"'none'");
  assert.equal(csp["frame-src"],"'self'");assert.equal(csp["font-src"],"'self'");
  for(const source of csp["connect-src"].split(/\s+/)){
    assert.ok(["'self'","ipc:","http://ipc.localhost"].includes(source) || /^https:\/\/[^/\s*]+$/.test(source));
  }
  assert.equal(config.app.windows[0].devtools,false);
});
