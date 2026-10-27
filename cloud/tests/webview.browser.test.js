// Real Edge/Chromium renderer tests. No Tauri process or live account is used.
import test from "node:test";
import assert from "node:assert/strict";
import {createServer} from "node:http";
import {readFile} from "node:fs/promises";
import {fileURLToPath} from "node:url";
import path from "node:path";
import {chromium} from "playwright-core";
const project=new URL("../../",import.meta.url),src=path.resolve(fileURLToPath(new URL("src/",project)));
const config=JSON.parse(await readFile(new URL("src-tauri/tauri.conf.json",project),"utf8"));
const policy={...config.app.security.csp};
// Tauri can append a style nonce; dedicated attr/element rules must still allow
// the app's existing inline CSS without enabling inline JavaScript.
policy["style-src"]+=" 'nonce-dGVzdG5vbmNl'";
const csp=Object.entries(policy).map(([k,v])=>k+" "+v).join("; ");

test("browser enforces the WebView policy while rendering app data and print frames",async t=>{
  const server=createServer(async(req,res)=>{
    res.setHeader("Content-Security-Policy",csp);
    const url=new URL(req.url,"http://localhost");
    if(url.pathname==="/harness"){
      res.setHeader("content-type","text/html; charset=utf-8");
      res.end('<!doctype html><html><head><title>Security fixture</title></head><body><div id="styled" style="width:113px">style</div><div id="test-root"></div></body></html>');return;
    }
    try{
      const target=path.resolve(src,"."+decodeURIComponent(url.pathname));
      if(!target.startsWith(src+path.sep))throw new Error("invalid path");
      const data=await readFile(target);
      res.setHeader("content-type",({".js":"text/javascript",".css":"text/css",".html":"text/html",".svg":"image/svg+xml",".png":"image/png"})[path.extname(target)]??"application/octet-stream");
      res.end(data);
    }catch{res.statusCode=404;res.end("Not found");}
  });
  await new Promise(resolve=>server.listen(0,"127.0.0.1",resolve));
  const origin="http://127.0.0.1:"+server.address().port;
  let browser;
  try{
    browser=await chromium.launch(process.env.NATRA_TEST_BROWSER_EXECUTABLE?
      {executablePath:process.env.NATRA_TEST_BROWSER_EXECUTABLE,headless:true}:
      process.platform==="win32"?{channel:"msedge",headless:true}:{headless:true});
    const context=await browser.newContext();const outbound=[];
    await context.route("**/*",route=>{
      const url=route.request().url();
      if(url.startsWith(origin+"/"))return route.continue();
      outbound.push(url);return route.abort(); // Never contact external test domains, even on policy failure.
    });
    const page=await context.newPage();
    const reset=()=>page.goto(origin+"/harness");
    await t.test("malicious chart labels are text rather than DOM, independently of CSP",async()=>{
      await reset();
      const result=await page.evaluate(async()=>{
        const {renderDonutChart}=await import("/js/components/donut-chart.js");
        const {renderVerticalBarChart}=await import("/js/components/vertical-bar-chart.js");
        const root=document.querySelector("#test-root"),a=document.createElement("div"),b=document.createElement("div");root.append(a,b);
        const attack='\"><img src=x onerror="window.pwned=1">';
        renderDonutChart(a,{items:[{label:attack,value:1}],centerLabel:attack,centerValue:attack,formatValue:()=>attack});
        renderVerticalBarChart(b,{groups:[{label:attack,values:[1]}],series:[{label:attack,tone:'sage);background:url(https://bad.invalid)'}]});
        return {count:root.querySelectorAll("img").length,labels:[...root.querySelectorAll(".donut-legend-label,.vbar-chart-legend-item")].map(e=>e.textContent),attack,pwned:!!window.pwned};
      });
      assert.equal(result.count,0);assert.equal(result.pwned,false);assert.ok(result.labels.every(v=>v.includes(result.attack)));
    });
    await t.test("CSP blocks script elements, event attributes and remote connections while keeping inline CSS",async()=>{
      await reset();
      const result=await page.evaluate(async()=>{
        const violations=[];document.addEventListener("securitypolicyviolation",e=>violations.push(e.effectiveDirective));
        window.pwned=false;
        const script=document.createElement("script");script.textContent="window.pwned=true";document.body.append(script);
        const button=document.createElement("button");button.setAttribute("onclick","window.pwned=true");document.body.append(button);button.click();
        await fetch("https://unapproved.invalid/never-contact").catch(()=>{});
        await new Promise(resolve=>setTimeout(resolve,100));
        return {pwned:window.pwned,violations,width:getComputedStyle(document.querySelector("#styled")).width};
      });
      assert.equal(result.pwned,false);assert.equal(result.width,"113px");
      assert.ok(result.violations.some(v=>v.startsWith("script-src")));assert.ok(result.violations.includes("connect-src"));
    });
    await t.test("backup error/path strings remain text in the actual settings page",async()=>{
      await reset();
      await page.evaluate(()=>{
        window.__TAURI__={core:{invoke:async cmd=>{
          if(cmd==="get_settings")return {restaurant_name:"Test",currency:"ETB",sync_enabled:false};
          if(cmd==="backup_status")return {enabled:true,backup_dir:'\"><img src=x onerror="window.pwned=1">',last_backup_at:"bad<date>",last_backup_error:'<img src=x onerror="window.pwned=1">'};
          throw new Error("Unexpected mock command "+cmd);
        }}};
      });
      await page.evaluate(async()=>{await (await import("/js/pages/settings.js")).render(document.querySelector("#test-root"));});
      await page.waitForFunction(()=>document.querySelector("#backup-section")?.textContent.includes("Last attempt failed"));
      assert.equal(await page.locator("#backup-section img").count(),0);
      assert.ok((await page.locator("#backup-section").textContent()).includes("<img"));
    });
    await t.test("portraits allow offline raster images but never remote or SVG URLs",async()=>{
      await reset();
      const result=await page.evaluate(async()=>{
        const {setPortrait}=await import("/js/utils/html.js");const host=document.querySelector("#test-root");
        setPortrait(host,"https://tracker.invalid/photo.png","Waiter");const remote=host.querySelectorAll("img").length;
        setPortrait(host,"data:image/svg+xml;base64,PHN2Zz4=","Waiter");const svg=host.querySelectorAll("img").length;
        const canvas=document.createElement("canvas");canvas.width=canvas.height=1;
        setPortrait(host,canvas.toDataURL("image/png"),"Waiter");await host.querySelector("img").decode();
        return {remote,svg,raster:host.querySelectorAll("img").length};
      });
      assert.deepEqual(result,{remote:0,svg:0,raster:1});
    });
    await t.test("print frames disable child script execution and keep receipt totals/text",async()=>{
      await reset();
      const result=await page.evaluate(async()=>{
        const {createPrintFrame}=await import("/js/utils/print.js"),{waiterReceiptHtml}=await import("/js/utils/waiter-receipt.js");
        const attack='<img src=x onerror="parent.pwned=true">';
        const html=waiterReceiptHtml({result:{sale:{id:1,waiter_id:1,created_at:"2026-10-27 06:00:00",total_amount:30},items:[{item_name:attack,quantity:2,unit_price:15,line_total:30}]},waiterName:attack,restaurantName:attack,currency:"ETB"});
        const frame=createPrintFrame(html);await new Promise(resolve=>{frame.onload=resolve;document.body.append(frame);});
        const result={sandbox:frame.getAttribute("sandbox"),text:frame.contentDocument.body.textContent,images:frame.contentDocument.querySelectorAll("img").length};frame.remove();
        window.pwned=false;
        const malicious=createPrintFrame('<html><head></head><body><script>parent.pwned=true</script></body></html>');
        await new Promise(resolve=>{malicious.onload=resolve;document.body.append(malicious);});
        result.executed=window.pwned;malicious.remove();return result;
      });
      assert.equal(result.sandbox,"allow-same-origin allow-modals");assert.equal(result.images,0);assert.equal(result.executed,false);
      assert.ok(result.text.includes("ETB 30.00"));assert.ok(result.text.includes("<img"));
    });
    await t.test("the bundled application boots to login under the policy",async()=>{
      await page.addInitScript(()=>{window.__TAURI__={core:{invoke:async cmd=>{
        if(cmd==="logout")return;
        if(cmd==="is_initialized")return true;
        if(cmd==="cloud_sync_configuration")return null;
        throw new Error("Unexpected startup mock "+cmd);
      }}};});
      await page.goto(origin+"/index.html");await page.getByRole("heading",{name:"Welcome back"}).waitFor();
      assert.equal(await page.locator("#header-sync-btn").isDisabled(),true);
    });
    await t.test("staff login lands on POS, hides admin actions and rejects direct admin routes",async()=>{
      await page.evaluate(()=>{
        window.__TAURI__.core.invoke=async cmd=>{
          if(cmd==="logout")return;
          if(cmd==="cloud_sync_configuration")return null;
          if(cmd==="login")return {id:2,full_name:"Test Cashier",username:"cashier",role:"staff",is_active:true};
          if(cmd==="get_settings"){
            if(window.expireSession)throw "authentication error: your local session has ended; please sign in again";
            return {restaurant_name:"Test Restaurant",currency:"ETB",sync_enabled:false};
          }
          if(cmd==="list_items" || cmd==="list_waiters")return [];
          if(cmd==="list_sales")return [{id:1,waiter_id:1,total_quantity:2,payment_method:"cash",total_amount:30,is_settled:false,is_reversed:false,created_at:new Date().toISOString()}];
          throw new Error("Unexpected staff command "+cmd);
        };
      });
      await page.locator("#username").fill("cashier");await page.locator("#password").fill("test-secret");
      await page.getByRole("button",{name:"Log in",exact:true}).click();
      await page.waitForURL(url=>url.hash==="#/pos");await page.locator("#checkout-btn").waitFor();
      assert.equal(await page.locator("#sidebar-nav .nav-item").count(),1);
      assert.equal(await page.getByRole("button",{name:"Reverse",exact:true}).count(),0);
      await page.evaluate(()=>{location.hash="/settings";});
      await page.waitForURL(url=>url.hash==="#/pos");assert.equal(await page.locator("#backup-section").count(),0);
    });
    await t.test("native expiry clears the active screen and returns to login",async()=>{
      await page.evaluate(async()=>{
        const content=document.createElement("div");content.textContent="Private operation";
        (await import("/js/components/modal.js")).openModal({title:"Pending operation",content});
        window.expireSession=true;
        await (await import("/js/api.js")).settings.get().catch(()=>{});
      });
      await page.getByRole("heading",{name:"Welcome back"}).waitFor();
      assert.equal(await page.evaluate(async()=>(await import("/js/state.js")).store.getState().user),null);
      assert.equal(await page.locator(".modal-backdrop").count(),0);
      assert.equal(await page.locator("#app-outlet > *").count(),0);
      assert.equal(await page.locator("#header-sync-btn").isDisabled(),true);
    });
    assert.deepEqual(outbound,[]);
    await context.close();
  }finally{await browser?.close();await new Promise(resolve=>server.close(resolve));}
});
