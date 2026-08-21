/**
 * Point of sale. A waiter is picked once per sale, items are added from a
 * dense table, and "Complete sale" calls the atomic checkout command
 * (src-tauri/src/services/sales_service.rs) — either the whole sale writes
 * or none of it does.
 */

import * as api from "../api.js";
import { store, pushToast, withErrorToast } from "../state.js";
import { clearHeaderActions } from "../components/header.js";
import { renderTable } from "../components/table.js";
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
        <p class="page-subtitle">Select a waiter, add items from the table, then complete the sale.</p>
      </div>
    </div>
    <div class="pos-layout">
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
        <div id="item-table"></div>
      </div>

      <div class="card pos-sale-panel">
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
        <div class="pos-cart-heading">Sale lines</div>
        <div id="cart-lines"></div>
        <div class="pos-total">
          <span class="field-label">Total</span>
          <span class="ticket-card-value" id="cart-total">${formatMoney(0)}</span>
        </div>
        <button class="btn btn-primary" id="checkout-btn" style="width: 100%; margin-top: var(--space-4);" disabled>
          Complete sale
        </button>
      </div>
    </div>
  `;

  const currency = store.getState().settings?.currency ?? "USD";
  const itemTable = container.querySelector("#item-table");
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

  function renderItemTable() {
    const filterType = typeFilter.value;
    const filtered = filterType ? allItems.filter((i) => i.type === filterType) : allItems;

    renderTable(itemTable, {
      columns: [
        { key: "name", label: "Item" },
        {
          key: "type",
          label: "Type",
          format: (item) => {
            const badge = document.createElement("span");
            const cookable = item.type === "cookable";
            badge.className = `badge ${cookable ? "badge-navy" : "badge-neutral"}`;
            badge.textContent = cookable ? "Cookable" : "Ready-made";
            return badge;
          },
        },
        {
          key: "selling_price",
          label: "Unit price",
          numeric: true,
          format: (item) => formatMoney(item.selling_price, currency),
        },
        {
          key: "action",
          label: "Action",
          format: (item) => {
            const button = document.createElement("button");
            button.className = "btn btn-primary btn-sm";
            button.textContent = cart.has(item.id) ? "Add +1" : "Add";
            button.addEventListener("click", () => addToCart(item));
            return button;
          },
        },
      ],
      rows: filtered,
      emptyMessage: "No active items match this filter.",
      getRowKey: (item) => item.id,
    });
  }

  function addToCart(item) {
    const existing = cart.get(item.id);
    cart.set(item.id, { item, quantity: (existing?.quantity ?? 0) + 1 });
    renderItemTable();
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
    renderItemTable();
    renderCart();
  }

  function renderCart() {
    const cartLines = container.querySelector("#cart-lines");
    const totalEl = container.querySelector("#cart-total");
    const checkoutBtn = container.querySelector("#checkout-btn");

    const rows = Array.from(cart.values());
    let total = 0;
    rows.forEach(({ item, quantity }) => {
      total += item.selling_price * quantity;
    });

    renderTable(cartLines, {
      columns: [
        {
          key: "item",
          label: "Item",
          format: (line) => {
            const wrap = document.createElement("div");
            const name = document.createElement("div");
            name.style.fontWeight = "650";
            name.textContent = line.item.name;
            const meta = document.createElement("div");
            meta.style.cssText = "font-size:.72rem;color:var(--color-ink-soft);margin-top:2px;";
            meta.textContent = `${humanizeEnum(line.item.type)} · ${formatMoney(line.item.selling_price, currency)}`;
            wrap.append(name, meta);
            return wrap;
          },
        },
        {
          key: "quantity",
          label: "Qty",
          numeric: true,
          format: (line) => {
            const controls = document.createElement("div");
            controls.className = "pos-qty-controls";

            const dec = document.createElement("button");
            dec.className = "btn btn-ghost btn-sm";
            dec.textContent = "−";
            dec.title = "Decrease quantity";
            dec.addEventListener("click", () => changeQuantity(line.item.id, -1));

            const qty = document.createElement("span");
            qty.className = "input-mono";
            qty.textContent = String(line.quantity);

            const inc = document.createElement("button");
            inc.className = "btn btn-ghost btn-sm";
            inc.textContent = "+";
            inc.title = "Increase quantity";
            inc.addEventListener("click", () => changeQuantity(line.item.id, 1));

            controls.append(dec, qty, inc);
            return controls;
          },
        },
        {
          key: "total",
          label: "Total",
          numeric: true,
          format: (line) => formatMoney(line.item.selling_price * line.quantity, currency),
        },
      ],
      rows,
      emptyMessage: "No items yet — add something from the Items table.",
      getRowKey: (line) => line.item.id,
    });

    totalEl.textContent = formatMoney(total, currency);
    checkoutBtn.disabled = cart.size === 0;
  }

  typeFilter.addEventListener("change", renderItemTable);
  renderItemTable();
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
      renderItemTable();
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
