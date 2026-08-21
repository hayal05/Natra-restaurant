import * as api from "../api.js";
import { store, pushToast, withErrorToast } from "../state.js";
import { setHeaderActions } from "../components/header.js";
import { renderTable } from "../components/table.js";
import { openModal, closeModal } from "../components/modal.js";
import { formatMoney } from "../utils/currency.js";
import { firstError, isNonEmpty, isNonNegativeNumber } from "../utils/validation.js";

export const title = "Items";

export async function render(container) {
  container.innerHTML = `
    <div class="page-heading">
      <div>
        <h1>Items</h1>
        <p class="page-subtitle">Menu items, custom categories, pricing and availability.</p>
      </div>
    </div>
    <div class="field" style="flex-direction: row; align-items: center; gap: var(--space-2); flex-wrap: wrap;">
      <select class="select" id="type-filter" style="width: 12rem;">
        <option value="">All types</option>
        <option value="ready_made">Ready-made</option>
        <option value="cookable">Cookable</option>
      </select>
      <div class="checkbox-row">
        <input type="checkbox" id="show-inactive" />
        <label for="show-inactive" style="font-size: var(--text-sm); color: var(--color-ink-soft);">Show inactive</label>
      </div>
    </div>

    <div class="card catalog-table">
      <div class="card-header">
        <span class="card-title">Menu items</span>
        <div style="display:flex;align-items:center;gap:.6rem;flex-wrap:wrap;">
          <span class="badge badge-neutral" id="item-count">0 records</span>
          <button class="btn btn-secondary btn-sm" id="manage-categories">Manage categories</button>
          <button class="btn btn-primary btn-sm" id="add-item">Add item</button>
        </div>
      </div>
      <div id="item-table"></div>
    </div>

    <div class="card catalog-table">
      <div class="card-header">
        <div>
          <span class="card-title">Categories</span>
          <div class="page-subtitle">Create custom categories for your restaurant menu.</div>
        </div>
        <button class="btn btn-secondary btn-sm" id="add-category">Add category</button>
      </div>
      <div id="category-table"></div>
    </div>
  `;

  let categories = await loadCategories(container);

  container.querySelector("#add-item").addEventListener("click", () => openItemModal(container, categories));
  container.querySelector("#add-category").addEventListener("click", () => openCategoryModal(container));
  container.querySelector("#manage-categories").addEventListener("click", () => {
    container.querySelector("#category-table").scrollIntoView({ behavior: "smooth", block: "start" });
  });
  container.querySelector("#type-filter").addEventListener("change", () => loadItems(container, categories));
  container.querySelector("#show-inactive").addEventListener("change", () => loadItems(container, categories));

  const addBtn = document.createElement("button");
  addBtn.className = "btn btn-primary";
  addBtn.textContent = "Add item";
  addBtn.addEventListener("click", () => openItemModal(container, categories));
  setHeaderActions(document, [addBtn]);

  await loadItems(container, categories);
}

async function loadCategories(container) {
  try {
    const categories = await withErrorToast(() => api.items.listCategories());
    renderCategoryTable(container, categories);
    return categories;
  } catch {
    renderCategoryTable(container, []);
    return [];
  }
}

function renderCategoryTable(container, categories) {
  renderTable(container.querySelector("#category-table"), {
    columns: [
      { key: "name", label: "Category" },
      { key: "description", label: "Description", format: (c) => c.description || "—" },
      {
        key: "actions",
        label: "Actions",
        format: (category) => {
          const button = document.createElement("button");
          button.className = "btn btn-secondary btn-sm";
          button.textContent = "Add item here";
          button.addEventListener("click", () => openItemModal(container, categories, category.id));
          return button;
        },
      },
    ],
    rows: categories,
    emptyMessage: "No categories yet — create your first custom category.",
    getRowKey: (category) => category.id,
  });
}

async function loadItems(container, categories = []) {
  const currency = store.getState().settings?.currency ?? "USD";
  const showInactive = container.querySelector("#show-inactive").checked;
  const itemType = container.querySelector("#type-filter").value || undefined;
  const categoryMap = new Map(categories.map((c) => [c.id, c.name]));
  let items;
  try {
    items = await withErrorToast(() => api.items.list({ only_active: !showInactive, item_type: itemType ?? null, category_id: null }));
  } catch { return; }

  container.querySelector("#item-count").textContent = `${items.length} record${items.length === 1 ? "" : "s"}`;
  renderTable(container.querySelector("#item-table"), {
    columns: [
      { key: "name", label: "Item" },
      { key: "type", label: "Type", format: (item) => { const b = document.createElement("span"); const cookable = item.type === "cookable"; b.className = `badge ${cookable ? "badge-navy" : "badge-neutral"}`; b.textContent = cookable ? "Cookable" : "Ready-made"; return b; } },
      { key: "category", label: "Category", format: (item) => categoryMap.get(item.category_id) || "—" },
      { key: "purchase_cost", label: "Unit cost", numeric: true, format: (item) => item.purchase_cost == null ? "Raw materials" : formatMoney(item.purchase_cost, currency) },
      { key: "selling_price", label: "Selling price", numeric: true, format: (item) => formatMoney(item.selling_price, currency) },
      { key: "margin", label: "Margin", numeric: true, format: (item) => item.purchase_cost == null ? "—" : formatMoney(item.selling_price - item.purchase_cost, currency) },
      { key: "status", label: "Status", format: (item) => { const b = document.createElement("span"); b.className = `badge ${item.is_active ? "badge-sage" : "badge-neutral"}`; b.textContent = item.is_active ? "Active" : "Inactive"; return b; } },
      { key: "actions", label: "Actions", format: (item) => { const actions = document.createElement("div"); actions.className = "row-actions"; const edit = document.createElement("button"); edit.className = "btn btn-secondary btn-sm"; edit.textContent = "Edit pricing"; edit.addEventListener("click", () => openPricingModal(container, item)); actions.appendChild(edit); const toggle = document.createElement("button"); toggle.className = "btn btn-ghost btn-sm"; toggle.textContent = item.is_active ? "Deactivate" : "Activate"; toggle.addEventListener("click", async () => { try { await withErrorToast(() => api.items.setActive(item.id, !item.is_active)); loadItems(container, categories); } catch {} }); actions.appendChild(toggle); return actions; } },
    ],
    rows: items,
    emptyMessage: "No items yet — add your first menu item to start selling.",
    getRowKey: (item) => item.id,
  });
}

function openCategoryModal(container) {
  const form = document.createElement("form");
  form.noValidate = true;
  form.innerHTML = `
    <div class="field"><label class="field-label" for="category-name">Category name</label><input class="input" id="category-name" name="name" type="text" autofocus /><span class="field-error" id="category-name-error"></span></div>
    <div class="field"><label class="field-label" for="category-description">Description (optional)</label><textarea class="input" id="category-description" name="description" rows="3" style="resize:vertical;"></textarea></div>
  `;
  const cancel = document.createElement("button"); cancel.className = "btn btn-secondary"; cancel.type = "button"; cancel.textContent = "Cancel"; cancel.addEventListener("click", closeModal);
  const save = document.createElement("button"); save.className = "btn btn-primary"; save.type = "button"; save.textContent = "Create category"; save.addEventListener("click", () => form.requestSubmit());
  openModal({ title: "New category", content: form, actions: [cancel, save] });
  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    const name = form.name.value.trim();
    if (!isNonEmpty(name)) { form.querySelector("#category-name").classList.add("has-error"); form.querySelector("#category-name-error").textContent = "Enter a category name."; return; }
    save.disabled = true;
    try {
      await withErrorToast(() => api.items.createCategory(name, form.description.value.trim() || null));
      pushToast("Category created.", "success");
      closeModal();
      const categories = await loadCategories(container);
      await loadItems(container, categories);
    } catch { save.disabled = false; }
  });
}

function openItemModal(container, categories, selectedCategoryId = null) {
  const form = document.createElement("form"); form.noValidate = true;
  form.innerHTML = `
    <div class="field"><label class="field-label" for="item-name">Name</label><input class="input" id="item-name" name="name" type="text" autofocus /><span class="field-error" id="item-name-error"></span></div>
    <div class="field"><label class="field-label" for="item-category">Category</label><div style="display:flex;gap:.5rem;align-items:center;"><select class="select" id="item-category" name="categoryId" style="flex:1;"><option value="">No category</option>${categories.map((c) => `<option value="${c.id}" ${Number(selectedCategoryId) === Number(c.id) ? "selected" : ""}>${escapeHtml(c.name)}</option>`).join("")}</select><button class="btn btn-secondary btn-sm" type="button" id="quick-category">New</button></div><span class="field-hint">Create a custom category here without leaving item creation.</span></div>
    <div class="field"><label class="field-label">Type</label><div class="checkbox-row" style="gap:var(--space-4);"><label style="display:flex;align-items:center;gap:var(--space-1);font-size:var(--text-sm);"><input type="radio" name="itemType" value="ready_made" checked /> Ready-made</label><label style="display:flex;align-items:center;gap:var(--space-1);font-size:var(--text-sm);"><input type="radio" name="itemType" value="cookable" /> Cookable</label></div><span class="field-hint">Ready-made: cost tracked here. Cookable: cost comes from raw materials.</span></div>
    <div class="field" id="cost-field"><label class="field-label" for="item-cost">Purchase cost</label><input class="input input-mono" id="item-cost" name="purchaseCost" type="number" step="0.01" min="0" /><span class="field-error" id="item-cost-error"></span></div>
    <div class="field"><label class="field-label" for="item-price">Selling price</label><input class="input input-mono" id="item-price" name="sellingPrice" type="number" step="0.01" min="0" /><span class="field-error" id="item-price-error"></span></div>
  `;
  form.querySelector("#quick-category").addEventListener("click", () => openCategoryModal(container));
  const costField = form.querySelector("#cost-field"); const costInput = form.querySelector("#item-cost");
  form.querySelectorAll('input[name="itemType"]').forEach((radio) => radio.addEventListener("change", () => { const cookable = form.itemType.value === "cookable"; costField.style.display = cookable ? "none" : "flex"; if (cookable) costInput.value = ""; }));
  const cancel = document.createElement("button"); cancel.className = "btn btn-secondary"; cancel.type = "button"; cancel.textContent = "Cancel"; cancel.addEventListener("click", closeModal);
  const save = document.createElement("button"); save.className = "btn btn-primary"; save.type = "button"; save.textContent = "Add item"; save.addEventListener("click", () => form.requestSubmit());
  openModal({ title: "New item", content: form, actions: [cancel, save] });
  form.addEventListener("submit", async (e) => {
    e.preventDefault(); clearFormErrors(form);
    const name = form.name.value.trim(); const itemType = form.itemType.value; const sellingPrice = Number(form.sellingPrice.value); const purchaseCostRaw = form.purchaseCost.value; const purchaseCost = itemType === "ready_made" ? Number(purchaseCostRaw) : null;
    const error = firstError([[isNonEmpty(name), "Enter an item name."],[isNonNegativeNumber(sellingPrice), "Enter a valid selling price."],[itemType !== "ready_made" || (purchaseCostRaw !== "" && isNonNegativeNumber(purchaseCost)), "Ready-made items need a purchase cost."]]);
    if (error) { showFieldError(form, error.includes("selling") ? "item-price" : error.includes("cost") ? "item-cost" : "item-name", error); return; }
    save.disabled = true;
    try { await withErrorToast(() => api.items.create({ category_id: form.categoryId.value ? Number(form.categoryId.value) : null, name, item_type: itemType, purchase_cost: purchaseCost, selling_price: sellingPrice })); pushToast("Item added.", "success"); closeModal(); const freshCategories = await loadCategories(container); loadItems(container, freshCategories); } catch { save.disabled = false; }
  });
}

function openPricingModal(container, item) {
  const cookable = item.type === "cookable"; const form = document.createElement("form"); form.noValidate = true;
  form.innerHTML = `<p style="font-size:var(--text-sm);color:var(--color-ink-soft);margin-bottom:var(--space-4);">${escapeHtml(item.name)} — pricing only.</p><div class="field"><label class="field-label" for="edit-price">Selling price</label><input class="input input-mono" id="edit-price" type="number" step="0.01" min="0" value="${item.selling_price}" /><span class="field-error" id="edit-price-error"></span></div>${cookable ? "" : `<div class="field"><label class="field-label" for="edit-cost">Purchase cost</label><input class="input input-mono" id="edit-cost" type="number" step="0.01" min="0" value="${item.purchase_cost ?? ""}" /><span class="field-error" id="edit-cost-error"></span></div>`}`;
  const cancel = document.createElement("button"); cancel.className = "btn btn-secondary"; cancel.type = "button"; cancel.textContent = "Cancel"; cancel.addEventListener("click", closeModal);
  const save = document.createElement("button"); save.className = "btn btn-primary"; save.type = "button"; save.textContent = "Save"; save.addEventListener("click", () => form.requestSubmit());
  openModal({ title: "Edit pricing", content: form, actions: [cancel, save] });
  form.addEventListener("submit", async (e) => { e.preventDefault(); const sellingPrice = Number(form.querySelector("#edit-price").value); const purchaseCost = cookable ? null : Number(form.querySelector("#edit-cost").value); if (!isNonNegativeNumber(sellingPrice)) { showFieldError(form,"edit-price","Enter a valid selling price."); return; } if (!cookable && !isNonNegativeNumber(purchaseCost)) { showFieldError(form,"edit-cost","Enter a valid purchase cost."); return; } save.disabled=true; try { await withErrorToast(() => api.items.updatePricing(item.id,sellingPrice,purchaseCost)); pushToast("Pricing updated.","success"); closeModal(); loadItems(container); } catch { save.disabled=false; } });
}

function showFieldError(form, fieldId, message) { const input=form.querySelector(`#${fieldId}`); const errorEl=form.querySelector(`#${fieldId}-error`); if(input) input.classList.add("has-error"); if(errorEl) errorEl.textContent=message; }
function clearFormErrors(form) { form.querySelectorAll(".input,.select").forEach((el)=>el.classList.remove("has-error")); form.querySelectorAll(".field-error").forEach((el)=>(el.textContent="")); }
function escapeHtml(text) { const div=document.createElement("div"); div.textContent=text ?? ""; return div.innerHTML; }
