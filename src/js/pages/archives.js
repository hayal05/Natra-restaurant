/** Permanent offline transaction archive management. */
import * as api from "../api.js";
import { clearHeaderActions } from "../components/header.js";
import { pushToast, withErrorToast } from "../state.js";

export const title = "Archives";

export async function render(container) {
  clearHeaderActions(document);
  const parts = Object.fromEntries(new Intl.DateTimeFormat("en-US", {
    timeZone: "Africa/Addis_Ababa", year: "numeric", month: "2-digit", day: "2-digit",
  }).formatToParts(new Date()).filter(p => p.type !== "literal").map(p => [p.type, p.value]));
  const today = `${parts.year}-${parts.month}-${parts.day}`;
  container.innerHTML = `
    <div class="page-heading">
      <div><h1>Archives</h1>
      <p class="page-subtitle">Export Excel to Downloads. Permanent offline copies remain safely stored in AppData.</p></div>
    </div>
    <div class="card">
      <div class="card-header"><span class="card-title">Export transaction archive</span></div>
      <form id="archive-form" style="display:flex;flex-wrap:wrap;align-items:end;gap:var(--space-3)">
        <div class="field"><label class="field-label" for="archive-from">From (Ethiopian local date)</label>
          <input class="input" type="date" id="archive-from" required /></div>
        <div class="field"><label class="field-label" for="archive-through">Through</label>
          <input class="input" type="date" id="archive-through" required /></div>
        <button class="btn btn-primary" id="archive-export" type="submit">Export to Downloads</button>
      </form>
      <p id="archive-feedback" role="status" style="margin-top:var(--space-3);font-size:var(--text-sm)"></p>
    </div>
    <div class="card">
      <div class="card-header"><span class="card-title">Archive library</span><button class="btn btn-secondary" id="archive-refresh" type="button" data-no-header-promotion="true">Refresh</button></div>
      <div id="archive-summary" style="display:flex;flex-wrap:wrap;gap:12px;margin-bottom:16px"></div>
      <div style="display:flex;gap:12px;flex-wrap:wrap;margin-bottom:12px"><input class="input" type="search" id="archive-search" placeholder="Search date or batch…" style="flex:1;min-width:180px"/><select id="archive-filter" class="select"><option value="all">All archives</option><option value="valid">Verified</option><option value="invalid">Needs attention</option></select></div>
      <div id="archive-list" aria-live="polite">Loading saved batches…</div>
      <div style="display:flex;justify-content:space-between;align-items:center;margin-top:12px"><span id="archive-page-info"></span><div style="display:flex;gap:8px"><button class="btn btn-secondary" id="archive-prev" type="button">Previous</button><button class="btn btn-secondary" id="archive-next" type="button">Next</button></div></div>
      <details style="margin-top:20px"><summary>Storage location</summary><p>Permanent archives remain stored locally; Downloads contains copies.</p><code id="archive-directory" style="overflow-wrap:anywhere">Loading…</code></details>
    </div>`;
  const from = container.querySelector("#archive-from");
  const through = container.querySelector("#archive-through");
  const form = container.querySelector("#archive-form");
  const exportBtn = container.querySelector("#archive-export");
  const refresh = container.querySelector("#archive-refresh");
  const feedback = container.querySelector("#archive-feedback");
  const list = container.querySelector("#archive-list");
  const directory = container.querySelector("#archive-directory");
  from.value = through.value = today;


  const search = container.querySelector("#archive-search"), filter = container.querySelector("#archive-filter");
  const summary = container.querySelector("#archive-summary"), pageInfo = container.querySelector("#archive-page-info");
  const prev = container.querySelector("#archive-prev"), next = container.querySelector("#archive-next");
  const state = { rows: [], page: 0 }, pageSize = 10;
  const pretty = d => {
    const date = new Date(d.slice(0,4)+"-"+d.slice(4,6)+"-"+d.slice(6,8)+"T12:00:00Z");
    return Number.isNaN(date.getTime()) ? d : new Intl.DateTimeFormat("en-GB",{day:"numeric",month:"short",year:"numeric",timeZone:"UTC"}).format(date);
  };
  const period = id => {
    const m = /^EAT-(\d{8})-(\d{8})$/.exec(id);
    return m ? (m[1]===m[2] ? pretty(m[1]) : pretty(m[1])+" – "+pretty(m[2])) : id;
  };
  function renderList() {
    const query=search.value.trim().toLowerCase();
    const rows=state.rows.filter(e => (filter.value==="all" || (filter.value==="valid")===e.valid) && (!query || (e.filename+" "+e.batch_id+" "+period(e.batch_id)).toLowerCase().includes(query)));
    const pages=Math.max(1,Math.ceil(rows.length/pageSize));
    state.page=Math.max(0,Math.min(state.page,pages-1));
    list.replaceChildren();
    if (!rows.length) {
      const p=document.createElement("p");
      p.textContent=state.rows.length ? "No matching archives." : "No archives yet. Export a date range to create your first workbook.";
      list.appendChild(p);
    } else {
      const table=document.createElement("table");table.className="table";
      const head=document.createElement("thead");const hr=document.createElement("tr");
      ["Period","Batch","Size","Status",""].forEach(t=>{const th=document.createElement("th");th.textContent=t;hr.appendChild(th);});head.appendChild(hr);table.appendChild(head);
      const body=document.createElement("tbody");
      rows.slice(state.page*pageSize,(state.page+1)*pageSize).forEach(entry=>{
        const tr=document.createElement("tr");
        [period(entry.batch_id),entry.batch_id,(entry.size_bytes/1024).toFixed(1)+" KB"].forEach((v,i)=>{const td=document.createElement("td");td.textContent=v;if(i===0)td.style.fontWeight="600";if(i===1){td.style.fontSize="var(--text-sm)";td.title=entry.filename;}tr.appendChild(td);});
        const status=document.createElement("td"),badge=document.createElement("span");
        badge.className="badge "+(entry.valid?"badge-sage":"badge-neutral");badge.textContent=entry.valid?"Verified":"Needs attention";status.appendChild(badge);tr.appendChild(status);
        const action=document.createElement("td"),button=document.createElement("button");
        button.type="button";button.className="btn btn-secondary";button.textContent="Download";button.disabled=!entry.valid;
        button.addEventListener("click",async()=>{button.disabled=true;try{const saved=await withErrorToast(()=>api.archives.download(entry.filename));feedback.textContent="Excel copy saved: "+saved.path;pushToast("Saved to Downloads: "+saved.filename,"success");}catch{feedback.textContent="Could not save this archive to Downloads.";}finally{button.disabled=!entry.valid;}});
        action.appendChild(button);tr.appendChild(action);body.appendChild(tr);
      });
      table.appendChild(body);const wrap=document.createElement("div");wrap.style.overflowX="auto";wrap.appendChild(table);list.appendChild(wrap);
    }
    pageInfo.textContent=rows.length ? "Showing "+(state.page*pageSize+1)+"–"+Math.min((state.page+1)*pageSize,rows.length)+" of "+rows.length : "0 files";
    prev.disabled=state.page===0;next.disabled=state.page>=pages-1;
  }
  search.addEventListener("input",()=>{state.page=0;renderList();});
  filter.addEventListener("change",()=>{state.page=0;renderList();});
  prev.addEventListener("click",()=>{state.page--;renderList();});
  next.addEventListener("click",()=>{state.page++;renderList();});
  async function load() {
    refresh.disabled=true;list.textContent="Loading saved batches…";
    try {
      const data=await withErrorToast(()=>api.archives.list());directory.textContent=data.directory;state.rows=data.entries;state.page=0;
      const valid=data.entries.filter(e=>e.valid).length;summary.replaceChildren();
      [[data.entries.length,"Total batches"],[valid,"Verified"],[data.entries.length-valid,"Needs attention"]].forEach(([value,label])=>{
        const tile=document.createElement("div");tile.style.cssText="padding:12px 16px;border:1px solid var(--color-border,#ddd);border-radius:10px;flex:1;min-width:120px";
        const number=document.createElement("strong");number.style.cssText="display:block;font-size:1.4rem";number.textContent=String(value);
        const caption=document.createElement("small");caption.textContent=label;tile.append(number,caption);summary.appendChild(tile);
      });renderList();
    } catch { list.textContent="Unable to load archive batches."; }
    finally {refresh.disabled=false;}
  }
  refresh.addEventListener("click", load);
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (!from.value || !through.value || from.value > through.value) {
      feedback.textContent = "Choose a valid date range.";
      return;
    }
    exportBtn.disabled = true;
    feedback.textContent = "Generating and validating workbook…";
    try {
      const result = await withErrorToast(() => api.archives.export(from.value, through.value));
      const filename = result.path.replace(/.*[\\\\/]/, "");
      const saved = await withErrorToast(() => api.archives.download(filename));
      feedback.textContent = "Excel saved to Downloads: " + saved.path;
      pushToast("Excel saved to Downloads.", "success");
      await load();
    } catch (error) {
      feedback.textContent = "Export failed. No existing batch was overwritten.";
    } finally { exportBtn.disabled = false; }
  });
  await load();
}
