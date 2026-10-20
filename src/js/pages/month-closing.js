import * as api from "../api.js";
import { store, pushToast } from "../state.js";
import { navigate } from "../router.js";
import { openModal, closeModal } from "../components/modal.js";
import { formatMoney } from "../utils/currency.js";

export const title = "Raw Material Month-End Closing";
const monthKey = (year,month) => `${year}-${String(month).padStart(2,"0")}-01`;
const eatToday = () => {
  const parts = new Intl.DateTimeFormat("en-CA",{timeZone:"Africa/Addis_Ababa",year:"numeric",month:"2-digit",day:"2-digit"}).formatToParts(new Date());
  const get = name => parts.find(p => p.type === name)?.value;
  return { year:Number(get("year")),month:Number(get("month")) };
};
const safeText = value => String(value ?? "").replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));

export async function render(container) {
  const now=eatToday();
  const lastMonth=new Date(Date.UTC(now.year,now.month-2,1));
  const initial=`${lastMonth.getUTCFullYear()}-${String(lastMonth.getUTCMonth()+1).padStart(2,"0")}`;
  container.innerHTML=`
    <div class="page-heading"><div><h1>Month-End Closing</h1><p class="page-subtitle">Enter unused raw materials at month end. Their quantities and values carry forward automatically.</p></div></div>
    <div class="card"><div class="card-header"><span class="card-title">Closing period</span><button type="button" class="btn btn-secondary btn-sm" id="back-materials">Materials & purchases</button></div>
      <div class="field" style="max-width:230px"><label class="field-label" for="closing-period">Month</label><input class="input" type="month" id="closing-period" value="${initial}"/></div>
      <p id="closing-status" style="margin:12px 0"></p>
      <div class="table-wrap"><table class="data-table" style="width:100%"><thead><tr><th>Raw material</th><th>This month purchased</th><th>Remaining</th><th>Carried forward</th></tr></thead><tbody id="closing-rows"></tbody></table></div>
      <p style="font-size:var(--text-sm);color:var(--color-ink-soft);margin-top:12px">Remaining is the physical stock left, including any stock carried from the previous month. Cost valuation is calculated automatically.</p>
      <div style="display:flex;justify-content:flex-end;flex-wrap:wrap;gap:10px;margin-top:20px">
        <button class="btn btn-secondary" id="save-closing" type="button">Save Draft</button>
        <button class="btn btn-primary" id="finalize-closing" type="button">Close Month</button>
        <button class="btn btn-secondary" id="reopen-closing" type="button" hidden>Reopen for Correction</button>
      </div>
    </div>`;
  const period=container.querySelector("#closing-period");
  const rows=container.querySelector("#closing-rows");
  const status=container.querySelector("#closing-status");
  const save=container.querySelector("#save-closing");
  const close=container.querySelector("#finalize-closing");
  const reopen=container.querySelector("#reopen-closing");
  const currency=store.getState().settings?.currency || "ETB";
  let current=null, busy=false;
  container.querySelector("#back-materials").addEventListener("click",()=>navigate("/raw-materials"));
  const selectedMonth=()=>`${period.value}-01`;
  const controls=()=>{const closed=current?.status==="closed"; save.disabled=busy||closed||!current;close.disabled=busy||closed||!current;reopen.hidden=!closed;reopen.disabled=busy;};
  async function load() {
    busy=true;current=null;rows.replaceChildren();status.textContent="Loading closing records...";controls();
    try {
      const view=await api.monthClosing.get(selectedMonth());current=view;
      const entries=new Map(view.remaining.map(([id,qty])=>[id,qty]));
      status.textContent=view.status==="closed"?"Finalized — read only":view.status==="draft"?"Saved draft — not finalized":"New closing — not finalized";
      if(!view.materials.length){const tr=document.createElement("tr");const td=document.createElement("td");td.colSpan=4;td.textContent="No raw materials yet.";tr.append(td);rows.append(tr);}
      for(const m of view.materials){
        const tr=document.createElement("tr");const name=document.createElement("td");name.textContent=m.name;
        const purchased=document.createElement("td");purchased.textContent=`${m.purchased_quantity} ${m.unit}`;
        const remaining=document.createElement("td");const input=document.createElement("input");input.type="number";input.min="0";input.step="any";input.className="input";input.style.maxWidth="135px";input.dataset.materialId=String(m.raw_material_id);input.setAttribute("aria-label",`Remaining ${m.name} (${m.unit})`);
        const saved=entries.get(m.raw_material_id);input.value=saved==null?"":String(saved);input.disabled=view.status==="closed";remaining.append(input);
        const carry=document.createElement("td");carry.textContent=saved==null?"—":`${saved} ${m.unit}`;
        input.addEventListener("input",()=>{carry.textContent=input.value.trim()? `${input.value} ${m.unit}`:"—";});
        tr.append(name,purchased,remaining,carry);rows.append(tr);
      }
    } catch(err){status.textContent=String(err);pushToast(String(err),"error");}
    finally{busy=false;controls();}
  }
  function values() {
    if(!current||!current.materials.length)throw Error("No raw materials to close.");
    return current.materials.map(m=>{
      const input=Array.from(rows.querySelectorAll("input")).find(x=>Number(x.dataset.materialId)===m.raw_material_id);
      const raw=input?.value.trim()||"";
      if(raw==="")throw Error(`Enter remaining quantity for ${m.name}`);
      const value=Number(raw);
      if(!Number.isFinite(value)||value<0||value>m.available_quantity+0.000001)throw Error(`Invalid remaining quantity for ${m.name}; available: ${m.available_quantity} ${m.unit}`);
      return [m.raw_material_id,value];
    });
  }
  save.addEventListener("click",async()=>{
    try{const data=values();busy=true;controls();await api.monthClosing.saveDraft(selectedMonth(),data);pushToast("Draft saved.","success");await load();}
    catch(err){pushToast(String(err),"error");busy=false;controls();}
  });
  close.addEventListener("click",()=>{
    let data;try{data=values();}catch(err){pushToast(String(err),"error");return;}
    const note=document.createElement("p");note.textContent="Finalize this month? The closing will become read-only. To correct it later, you must reopen it with a reason.";
    const cancel=document.createElement("button");cancel.className="btn btn-secondary";cancel.textContent="Cancel";cancel.onclick=closeModal;
    const confirm=document.createElement("button");confirm.className="btn btn-primary";confirm.textContent="Close Month";
    confirm.onclick=async()=>{confirm.disabled=true;busy=true;controls();try{await api.monthClosing.saveDraft(selectedMonth(),data);await api.monthClosing.close(selectedMonth());closeModal();pushToast("Month finalized. Remaining stock carries forward.","success");await load();}catch(err){pushToast(String(err),"error");confirm.disabled=false;busy=false;controls();}};
    openModal({title:"Confirm Month-End Closing",content:note,actions:[cancel,confirm]});
  });
  reopen.addEventListener("click",()=>{
    const wrap=document.createElement("div");const p=document.createElement("p");p.textContent="Reopening requires a correction reason and is allowed only for the latest closed month.";const field=document.createElement("textarea");field.className="input";field.rows=3;field.placeholder="Reason for correction";field.style.width="100%";wrap.append(p,field);
    const cancel=document.createElement("button");cancel.className="btn btn-secondary";cancel.textContent="Cancel";cancel.onclick=closeModal;
    const confirm=document.createElement("button");confirm.className="btn btn-primary";confirm.textContent="Reopen";
    confirm.onclick=async()=>{confirm.disabled=true;try{await api.monthClosing.reopen(selectedMonth(),field.value);closeModal();pushToast("Month reopened for correction.","success");await load();}catch(err){pushToast(String(err),"error");confirm.disabled=false;}};
    openModal({title:"Reopen Month",content:wrap,actions:[cancel,confirm]});
  });
  period.addEventListener("change",load);
  await load();
}
