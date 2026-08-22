/**
 * Raw materials: catalog + purchase log. Purchases are independent
 * cost events, not linked to any specific dish (see
 * src-tauri/src/services/raw_material_service.rs) — they feed the
 * "Raw-Material Costs" term in the profit formula in aggregate.
 */

import * as api from "../api.js";
import { store, pushToast, withErrorToast } from "../state.js";
import { setHeaderActions } from "../components/header.js";
import { createStatCard } from "../components/stat-card.js";
import { renderTable } from "../components/table.js";
import { openModal, closeModal } from "../components/modal.js";
import { formatMoney } from "../utils/currency.js";
import { formatDate } from "../utils/dates.js";
import { firstError, isNonEmpty, isPositiveNumber } from "../utils/validation.js";

export const title = "Raw materials";

export async function render(container) {
  container.innerHTML = `
    <div class="page-heading">
      <div>
        <h1>Raw materials</h1>
        <p class="page-subtitle">Purchases here are tracked independently of sales — this is what cookable items really cost.</p>
      </div>
    </div>
    <div class="grid grid-cols-4" id="stat-row"></div>
    <div class="card">
      <div class="card-header"><span class="card-title">Materials</span></div>
      <div id="materials-table"></div>
    </div>
    <div class="card">
      <div class="card-header"><span class="card-title">Recent purchases</span></div>
      <div id="purchases-table"></div>
    </div>
  `;

  // The router moves this page's DOM out of `container` into the live outlet
  // once render() resolves, leaving `container` itself empty. Every element a
  // later callback (modal submit) needs must be captured here while
  // `container` still holds it - never re-queried from `container` afterwards.
  const els = {
    statRow: container.querySelector("#stat-row"),
    materialsTable: container.querySelector("#materials-table"),
    purchasesTable: container.querySelector("#purchases-table"),
  };

  const addBtn = document.createElement("button");
  addBtn.className = "btn btn-primary";
  addBtn.textContent = "Add material";
  addBtn.addEventListener("click", () => openMaterialModal(els));
  setHeaderActions(document, [addBtn]);

  await loadAll(els);
}

async function loadAll(els) {
  const currency = store.getState().settings?.currency ?? "USD";

  let materials, purchases, totalCost;
  try {
    [materials, purchases, totalCost] = await Promise.all([
      withErrorToast(() => api.rawMaterials.list(true)),
      withErrorToast(() => api.rawMaterials.listPurchases()),
      withErrorToast(() => api.rawMaterials.totalCost()),
    ]);
  } catch {
    return; // toast already shown
  }

  const statRow = els.statRow;
  statRow.innerHTML = "";
  statRow.appendChild(createStatCard({ label: "Active materials", value: String(materials.length) }));
  statRow.appendChild(createStatCard({ label: "Total raw-material cost", value: formatMoney(totalCost, currency), tone: "rust" }));
  statRow.appendChild(createStatCard({ label: "Purchases logged", value: String(purchases.length) }));

  const materialsById = new Map(materials.map((m) => [m.id, m]));

  renderTable(els.materialsTable, {
    columns: [
      { key: "name", label: "Name" },
      { key: "unit", label: "Unit" },
      {
        key: "actions",
        label: "",
        format: (m) => {
          const btn = document.createElement("button");
          btn.className = "btn btn-secondary btn-sm";
          btn.textContent = "Record purchase";
          btn.addEventListener("click", () => openPurchaseModal(els, m));
          return btn;
        },
      },
    ],
    rows: materials,
    emptyMessage: "No raw materials yet — add flour, chicken, oil, whatever your kitchen buys.",
    getRowKey: (m) => m.id,
  });

  renderTable(els.purchasesTable, {
    columns: [
      { key: "purchase_date", label: "Date", format: (p) => formatDate(p.purchase_date) },
      { key: "material", label: "Material", format: (p) => materialsById.get(p.raw_material_id)?.name ?? `#${p.raw_material_id}` },
      { key: "quantity", label: "Qty", numeric: true, format: (p) => `${p.quantity} ${materialsById.get(p.raw_material_id)?.unit ?? ""}` },
      { key: "unit_cost", label: "Unit cost", numeric: true, format: (p) => formatMoney(p.unit_cost, currency) },
      { key: "total_cost", label: "Total", numeric: true, format: (p) => formatMoney(p.total_cost, currency) },
      { key: "supplier", label: "Supplier", format: (p) => p.supplier || "—" },
    ],
    rows: purchases,
    emptyMessage: "No purchases logged yet.",
    getRowKey: (p) => p.id,
  });
}

function openMaterialModal(els) {
  const form = document.createElement("form");
  form.noValidate = true;
  form.innerHTML = `
    <div class="field">
      <label class="field-label" for="material-name">Name</label>
      <input class="input" id="material-name" name="name" type="text" placeholder="Chicken" autofocus />
      <span class="field-error" id="material-name-error"></span>
    </div>
    <div class="field">
      <label class="field-label" for="material-unit">Unit</label>
      <input class="input" id="material-unit" name="unit" type="text" placeholder="kg" />
    </div>
  `;

  const cancelBtn = document.createElement("button");
  cancelBtn.className = "btn btn-secondary";
  cancelBtn.type = "button";
  cancelBtn.textContent = "Cancel";
  cancelBtn.addEventListener("click", closeModal);

  const saveBtn = document.createElement("button");
  saveBtn.className = "btn btn-primary";
  saveBtn.type = "button";
  saveBtn.textContent = "Add material";
  saveBtn.addEventListener("click", () => form.requestSubmit());

  openModal({ title: "New raw material", content: form, actions: [cancelBtn, saveBtn] });

  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    const name = form.name.value.trim();
    const unit = form.unit.value.trim() || "unit";

    const error = firstError([[isNonEmpty(name), "Enter a material name."]]);
    if (error) {
      form.querySelector("#material-name").classList.add("has-error");
      form.querySelector("#material-name-error").textContent = error;
      return;
    }

    saveBtn.disabled = true;
    try {
      await withErrorToast(() => api.rawMaterials.create(name, unit));
      pushToast("Raw material added.", "success");
      closeModal();
      loadAll(els);
    } catch {
      saveBtn.disabled = false;
    }
  });
}

function openPurchaseModal(els, material) {
  const form = document.createElement("form");
  form.noValidate = true;
  form.innerHTML = `
    <p style="font-size: var(--text-sm); color: var(--color-ink-soft); margin-bottom: var(--space-4);">
      Recording a purchase of <strong>${escapeHtml(material.name)}</strong> (${escapeHtml(material.unit)}).
    </p>
    <div class="field">
      <label class="field-label" for="purchase-qty">Quantity</label>
      <input class="input input-mono" id="purchase-qty" type="number" step="0.01" min="0" placeholder="0.00" autofocus />
      <span class="field-error" id="purchase-qty-error"></span>
    </div>
    <div class="field">
      <label class="field-label" for="purchase-cost">Unit cost</label>
      <input class="input input-mono" id="purchase-cost" type="number" step="0.01" min="0" placeholder="0.00" />
      <span class="field-error" id="purchase-cost-error"></span>
    </div>
    <div class="field">
      <label class="field-label" for="purchase-supplier">Supplier (optional)</label>
      <input class="input" id="purchase-supplier" type="text" />
    </div>
    <div class="field">
      <label class="field-label" for="purchase-note">Note (optional)</label>
      <input class="input" id="purchase-note" type="text" />
    </div>
  `;

  const cancelBtn = document.createElement("button");
  cancelBtn.className = "btn btn-secondary";
  cancelBtn.type = "button";
  cancelBtn.textContent = "Cancel";
  cancelBtn.addEventListener("click", closeModal);

  const saveBtn = document.createElement("button");
  saveBtn.className = "btn btn-primary";
  saveBtn.type = "button";
  saveBtn.textContent = "Record purchase";
  saveBtn.addEventListener("click", () => form.requestSubmit());

  openModal({ title: "Record purchase", content: form, actions: [cancelBtn, saveBtn] });

  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    const quantity = Number(form.querySelector("#purchase-qty").value);
    const unitCost = Number(form.querySelector("#purchase-cost").value);
    const supplier = form.querySelector("#purchase-supplier").value.trim();
    const note = form.querySelector("#purchase-note").value.trim();

    const error = firstError([
      [isPositiveNumber(quantity), "Enter a quantity greater than zero."],
      [isPositiveNumber(unitCost) || unitCost === 0, "Enter a valid unit cost."],
    ]);
    if (error) {
      const fieldId = error.includes("quantity") ? "purchase-qty" : "purchase-cost";
      form.querySelector(`#${fieldId}`).classList.add("has-error");
      form.querySelector(`#${fieldId}-error`).textContent = error;
      return;
    }

    saveBtn.disabled = true;
    try {
      await withErrorToast(() =>
        api.rawMaterials.recordPurchase(material.id, quantity, unitCost, supplier || null, note || null)
      );
      pushToast("Purchase recorded.", "success");
      closeModal();
      loadAll(els);
    } catch {
      saveBtn.disabled = false;
    }
  });
}

function escapeHtml(text) {
  const div = document.createElement("div");
  div.textContent = text ?? "";
  return div.innerHTML;
}
