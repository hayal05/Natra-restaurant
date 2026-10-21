import * as api from "../api.js";
import { store, withErrorToast } from "../state.js";
import { navigate } from "../router.js";
import { clearHeaderActions } from "../components/header.js";
import { renderTable } from "../components/table.js";
import { formatMoney } from "../utils/currency.js";

export const title = "Raw Material Purchase History";
const eatDate = value => {
  const utc = new Date(String(value).replace(" ","T")+"Z");
  return Number.isNaN(utc.getTime()) ? String(value).slice(0,10) :
    new Intl.DateTimeFormat("en-CA",{timeZone:"Africa/Addis_Ababa",year:"numeric",month:"2-digit",day:"2-digit"}).format(utc);
};
const quoteCsv = v => '"'+String(v??"").replace(/"/g,'""')+'"';
const csvValue = records => [
  ["Purchase date (EAT)","Material","Quantity","Unit","Unit cost","Total cost","Supplier","Notes"],
  ...records.map(r=>[r.localDate,r.material,r.quantity,r.unit,r.unit_cost,r.total_cost,r.supplier||"",r.note||""])
].map(row=>row.map(quoteCsv).join(",")).join("\r\n");

export async function render(container) {
  clearHeaderActions(document);
  container.innerHTML = `
  <div class="page-heading"><div><h1>Purchase History</h1><p class="page-subtitle">Browse raw material purchases by month, date and material.</p></div></div>
  <div class="card">
    <div class="card-header"><span class="card-title">Purchase ledger</span>
      <div style="display:flex;flex-wrap:wrap;gap:8px">
        <button type="button" class="btn btn-secondary btn-sm" id="materials-back" data-no-header-promotion="true">Materials</button>
        <button type="button" class="btn btn-secondary btn-sm" id="closing-forward" data-no-header-promotion="true">Month-End Closing</button>
      </div>
    </div>
    <div style="display:flex;gap:12px;flex-wrap:wrap;margin-bottom:16px">
      <label class="field">Month<input type="month" class="input" id="purchase-month"/></label>
      <label class="field">From<input type="date" class="input" id="purchase-from"/></label>
      <label class="field">To<input type="date" class="input" id="purchase-to"/></label>
      <label class="field">Material<select class="select" id="purchase-material"><option value="">All materials</option></select></label>
      <label class="field">Search<input class="input" id="purchase-search" placeholder="Material or supplier"/></label>
    </div>
    <p id="purchase-summary" style="margin-bottom:12px"></p>
    <div id="purchase-history-table"></div>
    <div style="display:flex;flex-wrap:wrap;gap:8px;margin-top:16px">
      <button class="btn btn-secondary" id="purchase-export">Export CSV</button>
      <button class="btn btn-secondary" id="purchase-print">Print</button>
      <button class="btn btn-secondary" id="purchase-clear">Clear filters</button>
    </div>
  </div>`;
  const el = id=>container.querySelector("#"+id);
  el("materials-back").onclick=()=>navigate("/raw-materials");
  el("closing-forward").onclick=()=>navigate("/raw-materials/closing");
  const [materials,purchases]=await Promise.all([
    withErrorToast(()=>api.rawMaterials.list(false)),
    withErrorToast(()=>api.rawMaterials.listPurchases())
  ]);
  const currency=store.getState().settings?.currency??"ETB";
  const byId=new Map(materials.map(m=>[String(m.id),m]));
  const records=purchases.map(p=>{
    const m=byId.get(String(p.raw_material_id));
    return {...p,localDate:eatDate(p.purchase_date),material:m?.name??("#"+p.raw_material_id),unit:m?.unit??""};
  });
  for(const m of materials){
    const option=document.createElement("option");option.value=String(m.id);option.textContent=m.name;
    el("purchase-material").append(option);
  }
  const current=new Intl.DateTimeFormat("en-CA",{timeZone:"Africa/Addis_Ababa",year:"numeric",month:"2-digit"}).format(new Date());
  el("purchase-month").value=current;
  let visible=[];
  const refresh=()=>{
    const month=el("purchase-month").value,from=el("purchase-from").value,to=el("purchase-to").value;
    const material=el("purchase-material").value,query=el("purchase-search").value.toLowerCase().trim();
    visible=records.filter(r=>(!month||r.localDate.startsWith(month))&&(!from||r.localDate>=from)&&(!to||r.localDate<=to)&&(!material||String(r.raw_material_id)===material)&&(!query||[r.material,r.supplier,r.note].some(v=>String(v??"").toLowerCase().includes(query))));
    const total=visible.reduce((n,r)=>n+Number(r.total_cost),0);
    el("purchase-summary").textContent=`${visible.length} purchase(s) · ${formatMoney(total,currency)} total`;
    renderTable(el("purchase-history-table"),{
      columns:[
        {key:"localDate",label:"Date (EAT)"},
        {key:"material",label:"Material"},
        {key:"quantity",label:"Quantity",numeric:true,format:r=>`${r.quantity} ${r.unit}`},
        {key:"unit_cost",label:"Unit cost",numeric:true,format:r=>formatMoney(r.unit_cost,currency)},
        {key:"total_cost",label:"Total",numeric:true,format:r=>formatMoney(r.total_cost,currency)},
        {key:"supplier",label:"Supplier",format:r=>r.supplier||"—"},
        {key:"note",label:"Notes",format:r=>r.note||"—"},
      ],rows:visible,emptyMessage:"No purchases match the selected filters.",getRowKey:r=>r.id
    });
  };
  for(const id of ["purchase-month","purchase-from","purchase-to","purchase-material","purchase-search"])
    el(id).addEventListener("input",refresh);
  el("purchase-clear").onclick=()=>{for(const id of ["purchase-month","purchase-from","purchase-to","purchase-search"])el(id).value="";el("purchase-material").value="";refresh();};
  el("purchase-export").onclick=()=>{
    const blob=new Blob(["\uFEFF",csvValue(visible)],{type:"text/csv;charset=utf-8"});
    const url=URL.createObjectURL(blob);const a=document.createElement("a");a.href=url;a.download=`raw-material-purchases-${el("purchase-month").value||"all"}.csv`;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);
  };
  el("purchase-print").onclick=()=>{
    const win=window.open("","_blank");
    if(!win){window.print();return;}
    const h=win.document.createElement("h2");h.textContent="Raw Material Purchase History";
    const p=win.document.createElement("p");p.textContent=el("purchase-summary").textContent;
    const table=win.document.createElement("table");table.style.cssText="border-collapse:collapse;width:100%;font-family:Arial;font-size:12px";
    const headings=["Date (EAT)","Material","Quantity","Unit cost","Total","Supplier","Notes"];
    for(const row of [headings,...visible.map(r=>[r.localDate,r.material,r.quantity+" "+r.unit,formatMoney(r.unit_cost,currency),formatMoney(r.total_cost,currency),r.supplier||"",r.note||""])]){
      const tr=table.insertRow();for(const value of row){const td=document.createElement(row===headings?"th":"td");td.textContent=value;td.style.cssText="border:1px solid #ccc;padding:6px;text-align:left";tr.append(td);}
    }
    win.document.body.append(h,p,table);win.print();
  };
  refresh();
}
