/**
 * Point of sale. A waiter is picked once per sale, items are tapped
 * into a cart, and "Complete sale" calls the atomic checkout command
 * (src-tauri/src/services/sales_service.rs) — either the whole sale
 * writes or none of it does.
 */

import * as api from "../api.js";
import { store, pushToast, withErrorToast } from "../state.js";
import { clearHeaderActions } from "../components/header.js";
import { createProductCard } from "../components/product-card.js";
import { formatMoney } from "../utils/currency.js";
import { humanizeEnum } from "../utils/formatting.js";

export const title = "Point of sale";

/** cart: Map<itemId, { item, quantity }> — module-local, resets on page leave/reload. */
let cart = new Map();

export async function render(container) {
  clearHeaderActions(document);
  cart = new Map();

  container.innerHTML = `
    <div class="page-heading">
      <div>
        <h1>Point of sale</h1>
        <p class="page-subtitle">Pick a waiter, tap items to add them, then complete the sale.</p>
      </div>
    </div>
    <div class="grid" style="grid-template-columns: 1fr 22rem; align-items: start;">
      <div class="card">
        <div class="card-header">
          <span class="card-title">Items</span>
          <div class="field" style="flex-direction: row; align-items: center; gap: var(--space-2); margin: 0;">
            <select class="select" id="type-filter">
              <option value="">All types</option>
              <option value="ready_made">Ready-made</option>
              <option value="cookable">Cookable</option>
            </select>
          </div>
        </div>
        <div class="grid grid-cols-3" id="item-grid"></div>
      </div>
      <div class="card" style="position: sticky; top: 0;">
        <div class="card-header"><span class="card-title">Current sale</span></div>
        <div class="field">
          <label class="field-label" for="waiter-select">Waiter</label>
          <select class="select" id="waiter-select"></select>
        </div>
        <div class="field">
          <label class="field-label" for="payment-select">Payment method</label>
          <select class="select" id="payment-select">
            <option value="cash">Cash</option>
            <option value="card">Card</option>
            <option value="other">Other</option>
          </select>
        </div>
        <div id="cart-lines" style="margin-top: var(--space-4); display: flex; flex-direction: column; gap: var(--space-2);"></div>
        <div style="display: flex; justify-content: space-between; align-items: baseline; margin-top: var(--space-4); padding-top: var(--space-4); border-top: 1px solid var(--color-border);">
          <span class="field-label">Total</span>
          <span class="ticket-card-value" id="cart-total" style="margin: 0; font-size: var(--text-lg);">${formatMoney(0)}</span>
        </div>
        <button class="btn btn-primary" id="checkout-btn" style="width: 100%; margin-top: var(--space-4);" disabled>
          Complete sale
        </button>
      </div>
    </div>
  `;

  const currency = store.getState().settings?.currency ?? "USD";
  const itemGrid = container.querySelector("#item-grid");
  const waiterSelect = container.querySelector("#waiter-select");
  const typeFilter = container.querySelector("#type-filter");

  let allItems = [];
  try {
    const [items, waiters] = await Promise.all([
      withErrorToast(() => api.items.list({ only_active: true, item_type: null, category_id: null })),
      withErrorToast(() => api.waiters.list(true)),
    ]);
    allItems = items;

    if (!waiters.length) {
      waiterSelect.innerHTML = `<option value="">No active waiters — add one first</option>`;
    } else {
      waiterSelect.innerHTML = waiters
        .map((w) => `<option value="${w.id}">${escapeHtml(w.full_name)}</option>`)
        .join("");
    }
  } catch {
    return; // toast already shown
  }

  function renderItemGrid() {
    itemGrid.innerHTML = "";
    const filterType = typeFilter.value;
    const filtered = filterType ? allItems.filter((i) => i.type === filterType) : allItems;

    if (!filtered.length) {
      const empty = document.createElement("div");
      empty.className = "empty-state";
      empty.innerHTML = `<div class="empty-state-title">No items yet</div><p>Add items from the Items page first.</p>`;
      itemGrid.appendChild(empty);
      return;
    }

    filtered.forEach((item) => {
      itemGrid.appendChild(
        createProductCard({
          item,
          currency,
          mode: "pos",
          onSelect: (itemId) => addToCart(item),
        })
      );
    });
  }

  function addToCart(item) {
    const existing = cart.get(item.id);
    cart.set(item.id, { item, quantity: (existing?.quantity ?? 0) + 1 });
    renderCart();
  }

  function changeQuantity(itemId, delta) {
    const line = cart.get(itemId);
    if (!line) return;
    const nextQty = line.quantity + delta;
    if (nextQty <= 0) {
      cart.delete(itemId);
    } else {
      cart.set(itemId, { ...line, quantity: nextQty });
    }
    renderCart();
  }

  function renderCart() {
    const cartLines = container.querySelector("#cart-lines");
    const totalEl = container.querySelector("#cart-total");
    const checkoutBtn = container.querySelector("#checkout-btn");

    cartLines.innerHTML = "";
    if (cart.size === 0) {
      cartLines.innerHTML = `<p style="color: var(--color-ink-soft); font-size: var(--text-sm);">No items yet — tap something on the left.</p>`;
    }

    let total = 0;
    cart.forEach(({ item, quantity }) => {
      const lineTotal = item.selling_price * quantity;
      total += lineTotal;

      const row = document.createElement("div");
      row.style.cssText = "display:flex; align-items:center; gap: var(--space-2);";
      row.innerHTML = `
        <div style="flex:1; min-width:0;">
          <div style="font-size: var(--text-sm); font-weight:500;">${escapeHtml(item.name)}</div>
          <div style="font-size: var(--text-xs); color: var(--color-ink-soft);">${humanizeEnum(item.type)} · ${formatMoney(item.selling_price, currency)}</div>
        </div>
        <button class="btn btn-ghost btn-sm" data-action="dec">−</button>
        <span class="input-mono" style="width:1.5rem; text-align:center; font-size: var(--text-sm);">${quantity}</span>
        <button class="btn btn-ghost btn-sm" data-action="inc">+</button>
        <span class="entity-card-value" style="width:4.5rem; text-align:right;">${formatMoney(lineTotal, currency)}</span>
      `;
      row.querySelector('[data-action="dec"]').addEventListener("click", () => changeQuantity(item.id, -1));
      row.querySelector('[data-action="inc"]').addEventListener("click", () => changeQuantity(item.id, 1));
      cartLines.appendChild(row);
    });

    totalEl.textContent = formatMoney(total, currency);
    checkoutBtn.disabled = cart.size === 0;
  }

  typeFilter.addEventListener("change", renderItemGrid);
  renderItemGrid();
  renderCart();

  container.querySelector("#checkout-btn").addEventListener("click", async () => {
    const waiterId = Number(waiterSelect.value);
    if (!waiterId) {
      pushToast("Select a waiter before completing the sale.", "error");
      return;
    }

    const checkoutBtn = container.querySelector("#checkout-btn");
    checkoutBtn.disabled = true;
    checkoutBtn.textContent = "Completing…";

    const req = {
      waiter_id: waiterId,
      user_id: store.getState().user?.id ?? null,
      payment_method: container.querySelector("#payment-select").value,
      lines: Array.from(cart.values()).map(({ item, quantity }) => ({ item_id: item.id, quantity })),
      note: null,
    };

    try {
      const result = await api.pos.checkout(req);
      pushToast(`Sale completed — ${formatMoney(result.sale.total_amount, currency)}.`, "success");
      cart = new Map();
      renderCart();
    } catch (err) {
      pushToast(typeof err === "string" ? err : "Couldn't complete the sale.", "error");
    } finally {
      checkoutBtn.disabled = cart.size === 0;
      checkoutBtn.textContent = "Complete sale";
    }
  });
}

function escapeHtml(text) {
  const div = document.createElement("div");
  div.textContent = text ?? "";
  return div.innerHTML;
}
