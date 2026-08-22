import * as api from "../api.js";
import { store, pushToast, withErrorToast } from "../state.js";
import { setHeaderActions } from "../components/header.js";
import { renderTable } from "../components/table.js";
import { openModal, closeModal } from "../components/modal.js";
import { formatMoney } from "../utils/currency.js";
import { firstError, isNonEmpty } from "../utils/validation.js";

export const title = "Waiters";

export async function render(container) {
  container.innerHTML = `
    <div class="checkbox-row"><input type="checkbox" id="show-inactive" /><label for="show-inactive" style="font-size:var(--text-sm);color:var(--color-ink-soft);">Show inactive waiters</label></div>
    <div class="card catalog-table">
      <div class="card-header"><span class="card-title">Waiter roster</span><span class="badge badge-neutral" id="waiter-count">0 records</span></div>
      <div id="waiter-table"></div>
    </div>`;

  const addBtn = document.createElement("button");
  addBtn.className = "btn btn-primary";
  addBtn.textContent = "Add waiter";
  addBtn.addEventListener("click", () => openAddWaiterModal(container));
  setHeaderActions(document, [addBtn]);
  container.querySelector("#show-inactive").addEventListener("change", () => loadWaiters(container));
  await loadWaiters(container);
}

async function loadWaiters(container) {
  const currency = store.getState().settings?.currency ?? "USD";
  const showInactive = container.querySelector("#show-inactive").checked;
  let all, receivables;
  try {
    [all, receivables] = await Promise.all([
      withErrorToast(() => api.waiters.list(!showInactive)),
      withErrorToast(() => api.waiters.listReceivables()),
    ]);
  } catch { return; }
  const receivableMap = new Map(receivables.map((r) => [r.waiter.id, r.receivable]));
  container.querySelector("#waiter-count").textContent = `${all.length} record${all.length === 1 ? "" : "s"}`;
  renderTable(container.querySelector("#waiter-table"), {
    columns: [
      { key: "full_name", label: "Waiter", format: (w) => waiterIdentity(w) },
      { key: "phone", label: "Phone", format: (w) => w.phone || "—" },
      { key: "status", label: "Status", format: (w) => { const b=document.createElement("span"); b.className=`badge ${w.is_active?"badge-sage":"badge-neutral"}`; b.textContent=w.is_active?"Active":"Inactive"; return b; } },
      { key: "receivable", label: "Receivable", numeric: true, format: (w) => formatMoney(receivableMap.get(w.id) ?? 0, currency) },
      { key: "actions", label: "Actions", format: (w) => {
        const actions=document.createElement("div"); actions.className="row-actions"; const receivable=receivableMap.get(w.id)??0;
        if(receivable>0){ const b=document.createElement("button"); b.className="btn btn-secondary btn-sm"; b.textContent="Settle"; b.addEventListener("click",async()=>{try{await withErrorToast(()=>api.waiters.settle(w.id));pushToast("Waiter settled.","success");loadWaiters(container);}catch{}}); actions.appendChild(b); }
        const toggle=document.createElement("button"); toggle.className="btn btn-ghost btn-sm"; toggle.textContent=w.is_active?"Deactivate":"Activate"; toggle.addEventListener("click",async()=>{try{await withErrorToast(()=>api.waiters.setActive(w.id,!w.is_active));loadWaiters(container);}catch{}}); actions.appendChild(toggle); return actions;
      } },
    ], rows: all, emptyMessage: "No waiters yet — add your first waiter to start taking sales.", getRowKey: (w) => w.id,
  });
}

function waiterIdentity(waiter) {
  const wrap=document.createElement("div"); wrap.className="waiter-identity";
  const avatar=document.createElement("div"); avatar.className="waiter-avatar";
  if(waiter.profile_photo){ avatar.innerHTML=`<img src="${escapeAttr(waiter.profile_photo)}" alt="" />`; } else { avatar.textContent=(waiter.full_name||"?").trim().charAt(0).toUpperCase(); }
  const name=document.createElement("span"); name.textContent=waiter.full_name; wrap.append(avatar,name); return wrap;
}

function openAddWaiterModal(container) {
  const form=document.createElement("form"); form.noValidate=true;
  form.innerHTML=`
    <div class="field"><label class="field-label" for="waiter-photo">Profile photo</label><input class="input" id="waiter-photo" type="file" accept="image/jpeg,image/png,image/webp" /><div id="waiter-photo-preview" class="waiter-photo-preview"><span>No photo selected</span></div><small class="field-help">JPG, PNG or WebP. The image is resized locally before saving.</small></div>
    <div class="field"><label class="field-label" for="waiter-name">Full name</label><input class="input" id="waiter-name" name="fullName" type="text" autofocus /><span class="field-error" id="waiter-name-error"></span></div>
    <div class="field"><label class="field-label" for="waiter-phone">Phone (optional)</label><input class="input" id="waiter-phone" name="phone" type="text" /></div>`;
  let profilePhoto=null;
  const photoInput=form.querySelector("#waiter-photo"); const preview=form.querySelector("#waiter-photo-preview");
  photoInput.addEventListener("change",async()=>{ const file=photoInput.files?.[0]; if(!file)return; if(!file.type.startsWith("image/")){pushToast("Please select an image file.","error");photoInput.value="";return;} try{profilePhoto=await prepareProfilePhoto(file); preview.innerHTML=`<img src="${escapeAttr(profilePhoto)}" alt="Profile preview" />`;}catch{pushToast("Couldn't process that image.","error");photoInput.value="";} });
  const cancel=document.createElement("button"); cancel.className="btn btn-secondary"; cancel.type="button"; cancel.textContent="Cancel"; cancel.addEventListener("click",closeModal);
  const save=document.createElement("button"); save.className="btn btn-primary"; save.type="button"; save.textContent="Add waiter"; save.addEventListener("click",()=>form.requestSubmit());
  openModal({title:"New waiter",content:form,actions:[cancel,save]});
  form.addEventListener("submit",async(e)=>{e.preventDefault();const fullName=form.fullName.value.trim();const phone=form.phone.value.trim();const error=firstError([[isNonEmpty(fullName),"Enter the waiter's name."]]);if(error){form.querySelector("#waiter-name").classList.add("has-error");form.querySelector("#waiter-name-error").textContent=error;return;}save.disabled=true;try{await withErrorToast(()=>api.waiters.create(fullName,phone||null,profilePhoto));pushToast("Waiter added.","success");closeModal();loadWaiters(container);}catch{save.disabled=false;}});
}

function prepareProfilePhoto(file){
  return new Promise((resolve,reject)=>{const reader=new FileReader();reader.onerror=reject;reader.onload=()=>{const img=new Image();img.onerror=reject;img.onload=()=>{const max=320;const scale=Math.min(1,max/Math.max(img.width,img.height));const canvas=document.createElement("canvas");canvas.width=Math.max(1,Math.round(img.width*scale));canvas.height=Math.max(1,Math.round(img.height*scale));canvas.getContext("2d").drawImage(img,0,0,canvas.width,canvas.height);resolve(canvas.toDataURL("image/jpeg",0.82));};img.src=reader.result;};reader.readAsDataURL(file);});
}
function escapeAttr(value){const div=document.createElement("div");div.textContent=value??"";return div.innerHTML.replace(/"/g,"&quot;");}
