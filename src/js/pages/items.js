/**
 * Items catalog. Ready-made items carry a purchase cost here directly;
 * cookable items don't — their cost is tracked separately via Raw
 * Materials (see src-tauri/src/models/item.rs for the enforced rule).
 */

import * as api from "../api.js";
import { store, pushToast, withErrorToast } from "../state.js";
import { setHeaderActions } from "../components/header.js";
import { createProductCard } from "../components/product-card.js";
import { openModal, closeModal } from "../components/modal.js";
import { firstError, isNonEmpty, isNonNegativeNumber } from "../utils/validation.js";

export const title = "Items";

export async function render(container) {
  container.innerHTML = `
    <div class="page-heading">
      <div>
        <h1>Items</h1>
        <p class="page-subtitle">Ready-made items carry their own cost. Cookable items are priced only — cost comes from raw materials.</p>
      </div>
    </div>
    <div class="field" style="flex-direction: row; align-items: center; gap: var(--space-2);">
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
    <div class="grid grid-cols-3" id="item-grid"></div>
  `;

  let categories = [];
  try {
    categories = await withErrorToast(() => api.items.listCategories());
  } catch {
    /* non-fatal — the add form falls back to no category */
  }

  const addBtn = document.createElement("button");
  addBtn.className = "btn btn-primary";
  addBtn.textContent = "Add item";
  addBtn.addEventListener("click", () => openItemModal(container, categories));
  setHeaderActions(document, [addBtn]);

  container.querySelector("#type-filter").addEventListener("change", () => loadItems(container));
  container.querySelector("#show-inactive").addEventListener("change", () => loadItems(container));

  await loadItems(container);
}

async function loadItems(container) {
  const currency = store.getState().settings?.currency ?? "USD";
  const showInactive = container.querySelector("#show-inactive").checked;
  const itemType = container.querySelector("#type-filter").value || undefined;
  const grid = container.querySelector("#item-grid");

  let items;
  try {
    items = await withErrorToast(() =>
      api.items.list({ only_active: !showInactive, item_type: itemType ?? null, category_id: null })
    );
  } catch {
    return; // toast already shown
  }

  grid.innerHTML = "";
  if (!items.length) {
    grid.innerHTML = `<div class="empty-state"><div class="empty-state-title">No items yet</div><p>Add your first menu item to start selling.</p></div>`;
    return;
  }

  items.forEach((item) => {
    grid.appendChild(
      createProductCard({
        item,
        currency,
        mode: "catalog",
        onEdit: (itemId) => openPricingModal(container, item),
        onToggleActive: async (itemId, isActive) => {
          try {
            await withErrorToast(() => api.items.setActive(itemId, isActive));
            loadItems(container);
          } catch {
            /* toast already shown */
          }
        },
      })
    );
  });
}

function openItemModal(container, categories) {
  const form = document.createElement("form");
  form.noValidate = true;
  form.innerHTML = `
    <div class="field">
      <label class="field-label" for="item-name">Name</label>
      <input class="input" id="item-name" name="name" type="text" autofocus />
      <span class="field-error" id="item-name-error"></span>
    </div>
    <div class="field">
      <label class="field-label" for="item-category">Category (optional)</label>
      <select class="select" id="item-category" name="categoryId">
        <option value="">No category</option>
        ${categories.map((c) => `<option value="${c.id}">${escapeHtml(c.name)}</option>`).join("")}
      </select>
    </div>
    <div class="field">
      <label class="field-label">Type</label>
      <div class="checkbox-row" style="gap: var(--space-4);">
        <label style="display:flex; align-items:center; gap: var(--space-1); font-size: var(--text-sm);">
          <input type="radio" name="itemType" value="ready_made" checked /> Ready-made
        </label>
        <label style="display:flex; align-items:center; gap: var(--space-1); font-size: var(--text-sm);">
          <input type="radio" name="itemType" value="cookable" /> Cookable
        </label>
      </div>
      <span class="field-hint">Ready-made: cost tracked here. Cookable: cost comes from raw materials.</span>
    </div>
    <div class="field" id="cost-field">
      <label class="field-label" for="item-cost">Purchase cost</label>
      <input class="input input-mono" id="item-cost" name="purchaseCost" type="number" step="0.01" min="0" />
      <span class="field-error" id="item-cost-error"></span>
    </div>
    <div class="field">
      <label class="field-label" for="item-price">Selling price</label>
      <input class="input input-mono" id="item-price" name="sellingPrice" type="number" step="0.01" min="0" />
      <span class="field-error" id="item-price-error"></span>
    </div>
  `;

  const costField = form.querySelector("#cost-field");
  const costInput = form.querySelector("#item-cost");
  form.querySelectorAll('input[name="itemType"]').forEach((radio) => {
    radio.addEventListener("change", () => {
      const isCookable = form.itemType.value === "cookable";
      costField.style.display = isCookable ? "none" : "flex";
      if (isCookable) costInput.value = "";
    });
  });

  const cancelBtn = document.createElement("button");
  cancelBtn.className = "btn btn-secondary";
  cancelBtn.type = "button";
  cancelBtn.textContent = "Cancel";
  cancelBtn.addEventListener("click", closeModal);

  const saveBtn = document.createElement("button");
  saveBtn.className = "btn btn-primary";
  saveBtn.type = "button";
  saveBtn.textContent = "Add item";
  saveBtn.addEventListener("click", () => form.requestSubmit());

  openModal({ title: "New item", content: form, actions: [cancelBtn, saveBtn] });

  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    clearFormErrors(form);

    const name = form.name.value.trim();
    const itemType = form.itemType.value;
    const sellingPrice = Number(form.sellingPrice.value);
    const purchaseCostRaw = form.purchaseCost.value;
    const purchaseCost = itemType === "ready_made" ? Number(purchaseCostRaw) : null;

    const error = firstError([
      [isNonEmpty(name), "Enter an item name."],
      [isNonNegativeNumber(sellingPrice), "Enter a valid selling price."],
      [itemType !== "ready_made" || (purchaseCostRaw !== "" && isNonNegativeNumber(purchaseCost)), "Ready-made items need a purchase cost."],
    ]);
    if (error) {
      const fieldId = error.includes("selling") ? "item-price" : error.includes("cost") ? "item-cost" : "item-name";
      showFieldError(form, fieldId, error);
      return;
    }

    const categoryId = form.categoryId.value ? Number(form.categoryId.value) : null;

    saveBtn.disabled = true;
    try {
      await withErrorToast(() =>
        api.items.create({
          category_id: categoryId,
          name,
          item_type: itemType,
          purchase_cost: purchaseCost,
          selling_price: sellingPrice,
        })
      );
      pushToast("Item added.", "success");
      closeModal();
      loadItems(container);
    } catch {
      saveBtn.disabled = false;
    }
  });
}

function openPricingModal(container, item) {
  const isCookable = item.type === "cookable";
  const form = document.createElement("form");
  form.noValidate = true;
  form.innerHTML = `
    <p style="font-size: var(--text-sm); color: var(--color-ink-soft); margin-bottom: var(--space-4);">
      ${escapeHtml(item.name)} — pricing only. To change the name, category, or type, deactivate this item and create a new one.
    </p>
    <div class="field">
      <label class="field-label" for="edit-price">Selling price</label>
      <input class="input input-mono" id="edit-price" type="number" step="0.01" min="0" value="${item.selling_price}" />
      <span class="field-error" id="edit-price-error"></span>
    </div>
    ${
      isCookable
        ? ""
        : `<div class="field">
            <label class="field-label" for="edit-cost">Purchase cost</label>
            <input class="input input-mono" id="edit-cost" type="number" step="0.01" min="0" value="${item.purchase_cost ?? ""}" />
            <span class="field-error" id="edit-cost-error"></span>
          </div>`
    }
  `;

  const cancelBtn = document.createElement("button");
  cancelBtn.className = "btn btn-secondary";
  cancelBtn.type = "button";
  cancelBtn.textContent = "Cancel";
  cancelBtn.addEventListener("click", closeModal);

  const saveBtn = document.createElement("button");
  saveBtn.className = "btn btn-primary";
  saveBtn.type = "button";
  saveBtn.textContent = "Save";
  saveBtn.addEventListener("click", () => form.requestSubmit());

  openModal({ title: "Edit pricing", content: form, actions: [cancelBtn, saveBtn] });

  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    clearFormErrors(form);

    const sellingPrice = Number(form.querySelector("#edit-price").value);
    const purchaseCost = isCookable ? null : Number(form.querySelector("#edit-cost").value);

    if (!isNonNegativeNumber(sellingPrice)) {
      showFieldError(form, "edit-price", "Enter a valid selling price.");
      return;
    }
    if (!isCookable && !isNonNegativeNumber(purchaseCost)) {
      showFieldError(form, "edit-cost", "Enter a valid purchase cost.");
      return;
    }

    saveBtn.disabled = true;
    try {
      await withErrorToast(() => api.items.updatePricing(item.id, sellingPrice, purchaseCost));
      pushToast("Pricing updated.", "success");
      closeModal();
      loadItems(container);
    } catch {
      saveBtn.disabled = false;
    }
  });
}

function showFieldError(form, fieldId, message) {
  const input = form.querySelector(`#${fieldId}`);
  const errorEl = form.querySelector(`#${fieldId}-error`);
  if (input) input.classList.add("has-error");
  if (errorEl) errorEl.textContent = message;
}

function clearFormErrors(form) {
  form.querySelectorAll(".input, .select").forEach((el) => el.classList.remove("has-error"));
  form.querySelectorAll(".field-error").forEach((el) => (el.textContent = ""));
}

function escapeHtml(text) {
  const div = document.createElement("div");
  div.textContent = text ?? "";
  return div.innerHTML;
}
