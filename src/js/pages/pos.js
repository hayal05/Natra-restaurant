/**
 * Point of sale. A waiter is picked once per sale, items are added from a
 * dense table, and "Complete sale" calls the atomic checkout command.
 */
import * as api from "../api.js";
import { store, pushToast, withErrorToast } from "../state.js";
import { clearHeaderActions } from "../components/header.js";
import { renderTable } from "../components/table.js";
import { formatMoney } from "../utils/currency.js";
import { humanizeEnum } from "../utils/formatting.js";

export const title = "Point of sale";
let cart = new Map();

export async function render(container) {
  clearHeaderActions(document);
  cart = new Map();
  container.innerHTML = `
    <div class="pos-layout">
      <div class="card pos-items-card">
        <div class="card-header pos-items-toolbar">
          <div>
            <span class="card-title">Items</span>
            <div class="pos-items-hint">Search by product name, category or price</div>
          </div>
          <div class="pos-item-filters">
            <label class="pos-search" aria-label="Search items">
              <span aria-hidden="true">⌕</span>
              <input class="input" id="item-search" type="search" placeholder="Search name, category or price…" autocomplete="off" />
            </label>
            <select class="select" id="type-filter" aria-label="Filter item type">
              <option value="">All types</option><option value="ready_made">Ready-made</option><option value="cookable">Cookable</option>
            </select>
          </div>
        </div>
        <div id="item-table" class="pos-items-table"></div>
      </div>
      <div class="card pos-sale-panel">
        <div class="card-header"><span class="card-title">Current sale</span></div>
        <div class="field"><label class="field-label" for="waiter-select">Waiter</label><select class="select" id="waiter-select"></select></div>
        <div class="field"><label class="field-label" for="payment-select">Payment method</label><select class="select" id="payment-select"><option value="cash">Cash</option><option value="card">Card</option><option value="other">Other</option></select></div>
        <div class="pos-cart-heading">Sale lines</div><div id="cart-lines"></div>
        <div class="pos-total"><span class="field-label">Total</span><span class="ticket-card-value" id="cart-total">${formatMoney(0)}</span></div>
        <button class="btn btn-primary" id="checkout-btn" style="width:100%;margin-top:var(--space-4)" disabled>Complete sale</button>
      </div>
    </div>
    <div class="card pos-daily-history">
      <div class="card-header">
        <span class="card-title">Daily sales history</span>
        <span class="pos-history-total" id="daily-history-total"></span>
      </div>
      <div id="daily-sales-table"></div>
    </div>`;

  const currency = store.getState().settings?.currency ?? "USD";
  const itemTable = container.querySelector("#item-table");
  const waiterSelect = container.querySelector("#waiter-select");
  const typeFilter = container.querySelector("#type-filter");
  const searchInput = container.querySelector("#item-search");
  let allItems = [];
  let allWaiters = [];

  try {
    const [items, waiters] = await Promise.all([
      withErrorToast(() => api.items.list({ only_active: true, item_type: null, category_id: null })),
      withErrorToast(() => api.waiters.list(true)),
    ]);
    allItems = items;
    allWaiters = waiters;
    waiterSelect.innerHTML = waiters.length
      ? waiters.map((w) => `<option value="${w.id}">${escapeHtml(w.full_name)}</option>`).join("")
      : `<option value="">No active waiters — add one first</option>`;
  } catch { return; }

  const categoryName = (item) => item.category_name || item.category || item.categoryName || "—";

  function renderItemTable() {
    const filterType = typeFilter.value;
    const query = searchInput.value.trim().toLocaleLowerCase();
    const filtered = allItems.filter((item) => {
      if (filterType && item.type !== filterType) return false;
      if (!query) return true;
      const searchable = [item.name, categoryName(item), item.selling_price].map((value) => String(value ?? "").toLocaleLowerCase());
      return searchable.some((value) => value.includes(query));
    });

    renderTable(itemTable, {
      columns: [
        { key: "name", label: "Item" },
        { key: "category", label: "Category", format: (item) => categoryName(item) },
        { key: "selling_price", label: "Unit price", numeric: true, format: (item) => formatMoney(item.selling_price, currency) },
        { key: "action", label: "Action", format: (item) => {
          const button = document.createElement("button");
          button.className = "btn btn-primary btn-sm";
          button.type = "button";
          button.dataset.cartItemId = String(item.id);
          button.textContent = cart.has(item.id) ? `Add +1 (${cart.get(item.id).quantity})` : "Add";
          return button;
        }},
      ],
      rows: filtered,
      emptyMessage: query ? "No items match your search." : "No active items match this filter.",
      getRowKey: (item) => item.id,
    });
  }

  function addToCart(item) {
    const existing = cart.get(item.id);
    const quantity = (existing?.quantity ?? 0) + 1;
    cart.set(item.id, { item, quantity });
    renderItemTable();
    renderCart();
  }

  function changeQuantity(itemId, delta) {
    const line = cart.get(itemId);
    if (!line) return;
    const next = line.quantity + delta;
    if (next <= 0) cart.delete(itemId);
    else cart.set(itemId, { ...line, quantity: next });
    renderItemTable();
    renderCart();
  }

  function renderCart() {
    const rows = Array.from(cart.values());
    let total = 0;
    rows.forEach(({ item, quantity }) => { total += Number(item.selling_price || 0) * quantity; });

    renderTable(container.querySelector("#cart-lines"), {
      columns: [
        { key: "item", label: "Item", format: (line) => {
          const d = document.createElement("div");
          d.innerHTML = `<strong>${escapeHtml(line.item.name)}</strong><div style="font-size:.72rem;color:var(--color-ink-soft)">${escapeHtml(categoryName(line.item))} · ${humanizeEnum(line.item.type)}</div>`;
          return d;
        }},
        { key: "quantity", label: "Qty", numeric: true, format: (line) => {
          const d = document.createElement("div");
          d.className = "pos-qty-controls";
          const dec = document.createElement("button");
          dec.className = "btn btn-ghost btn-sm";
          dec.type = "button";
          dec.textContent = "−";
          dec.title = "Decrease quantity";
          dec.addEventListener("click", () => changeQuantity(line.item.id, -1));
          const q = document.createElement("span");
          q.className = "input-mono";
          q.textContent = String(line.quantity);
          q.setAttribute("aria-label", `Quantity ${line.quantity}`);
          const inc = document.createElement("button");
          inc.className = "btn btn-ghost btn-sm";
          inc.type = "button";
          inc.textContent = "+";
          inc.title = "Increase quantity";
          inc.addEventListener("click", () => changeQuantity(line.item.id, 1));
          d.append(dec, q, inc);
          return d;
        }},
        { key: "total", label: "Total", numeric: true, format: (line) => formatMoney(Number(line.item.selling_price || 0) * line.quantity, currency) },
      ],
      rows,
      emptyMessage: "No items yet — add something from the Items table.",
      getRowKey: (line) => line.item.id,
    });
    container.querySelector("#cart-total").textContent = formatMoney(total, currency);
    container.querySelector("#checkout-btn").disabled = cart.size === 0;
  }

  // Use one delegated click handler for the item table. The table is rebuilt
  // whenever the cart/search/filter changes, so delegation prevents the Add
  // action from becoming detached from the current table DOM.
  itemTable.addEventListener("click", (event) => {
    const button = event.target.closest("button[data-cart-item-id]");
    if (!button || !itemTable.contains(button)) return;
    const itemId = button.dataset.cartItemId;
    const item = allItems.find((candidate) => String(candidate.id) === itemId);
    if (item) addToCart(item);
  });

  function localDateKey(date = new Date()) {
    const y = date.getFullYear(); const m = String(date.getMonth() + 1).padStart(2, "0"); const d = String(date.getDate()).padStart(2, "0"); return `${y}-${m}-${d}`;
  }

  async function renderDailyHistory() {
    const table = container.querySelector("#daily-sales-table");
    try {
      const sales = await withErrorToast(() => api.pos.listSales(null, 200));
      const today = localDateKey(); const todaySales = sales.filter((sale) => String(sale.created_at || "").slice(0, 10) === today);
      const waiterById = new Map(allWaiters.map((w) => [w.id, w.full_name]));
      const total = todaySales.reduce((sum, sale) => sum + Number(sale.total_amount || 0), 0);
      container.querySelector("#daily-history-total").textContent = `${todaySales.length} sale${todaySales.length === 1 ? "" : "s"} · ${formatMoney(total, currency)}`;
      renderTable(table, {
        columns: [
          { key: "created_at", label: "Time", format: (sale) => { const value = String(sale.created_at || ""); const time = value.includes("T") ? value.split("T")[1].slice(0, 5) : value.slice(11, 16); return time || "—"; } },
          { key: "waiter_id", label: "Waiter", format: (sale) => waiterById.get(sale.waiter_id) || `Waiter #${sale.waiter_id}` },
          { key: "total_quantity", label: "Qty", numeric: true },
          { key: "payment_method", label: "Payment", format: (sale) => humanizeEnum(sale.payment_method) },
          { key: "total_amount", label: "Total", numeric: true, format: (sale) => formatMoney(sale.total_amount, currency) },
          { key: "is_settled", label: "Status", format: (sale) => { const badge = document.createElement("span"); badge.className = `badge ${sale.is_settled ? "badge-sage" : "badge-amber"}`; badge.textContent = sale.is_settled ? "Settled" : "Open"; return badge; } },
        ], rows: todaySales, emptyMessage: "No sales recorded today yet.", getRowKey: (sale) => sale.id,
      });
    } catch { container.querySelector("#daily-history-total").textContent = ""; }
  }

  typeFilter.addEventListener("change", renderItemTable);
  searchInput.addEventListener("input", renderItemTable);
  renderItemTable();
  renderCart();
  await renderDailyHistory();

  container.querySelector("#checkout-btn").addEventListener("click", async () => {
    const waiterId = Number(waiterSelect.value);
    if (!waiterId) return pushToast("Select a waiter before completing the sale.", "error");
    const button = container.querySelector("#checkout-btn");
    button.disabled = true;
    button.textContent = "Completing…";
    try {
      const result = await api.pos.checkout({
        waiter_id: waiterId,
        user_id: store.getState().user?.id ?? null,
        payment_method: container.querySelector("#payment-select").value,
        lines: Array.from(cart.values()).map(({ item, quantity }) => ({ item_id: item.id, quantity })),
        note: null,
      });
      pushToast(`Sale completed — ${formatMoney(result.sale.total_amount, currency)}.`, "success");
      cart = new Map();
      renderItemTable();
      renderCart();
      await renderDailyHistory();
    } catch (err) {
      pushToast(typeof err === "string" ? err : "Couldn't complete the sale.", "error");
    } finally {
      button.disabled = cart.size === 0;
      button.textContent = "Complete sale";
    }
  });
}

function escapeHtml(text) {
  const div = document.createElement("div");
  div.textContent = text ?? "";
  return div.innerHTML;
}
