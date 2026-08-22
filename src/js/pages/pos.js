/**
 * Point of sale. Items are staged in a client-side Current sale cart only.
 * Nothing is recorded as a sale until Complete sale is pressed.
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
          <span class="card-title">Items</span>
          <div class="pos-item-filters">
            <label class="pos-search" aria-label="Search items"><span aria-hidden="true">⌕</span><input class="input" id="item-search" type="search" placeholder="Search name, category or price…" autocomplete="off" /></label>
            <select class="select" id="type-filter" aria-label="Filter item type"><option value="">All types</option><option value="ready_made">Ready-made</option><option value="cookable">Cookable</option></select>
          </div>
        </div>
        <div id="item-table" class="pos-items-table"></div>
      </div>
      <div class="card pos-sale-panel">
        <div class="card-header"><span class="card-title">Current sale</span><span id="cart-count" class="pos-cart-count">0 items</span></div>
        <div class="field"><label class="field-label" for="waiter-select">Waiter</label><select class="select" id="waiter-select"></select></div>
        <div class="field"><label class="field-label" for="payment-select">Payment method</label><select class="select" id="payment-select"><option value="cash">Cash</option><option value="card">Card</option><option value="other">Other</option></select></div>
        <div class="pos-cart-heading">Selected items</div>
        <div id="cart-lines"></div>
        <div class="pos-total"><span class="field-label">Total</span><span class="ticket-card-value" id="cart-total">${formatMoney(0)}</span></div>
        <button class="btn btn-primary" id="checkout-btn" style="width:100%;margin-top:var(--space-4)" disabled>Complete sale</button>
      </div>
    </div>
    <div class="card pos-daily-history"><div class="card-header"><span class="card-title">Daily sales history</span><span class="pos-history-total" id="daily-history-total"></span></div><div id="daily-sales-table"></div></div>`;

  const currency = store.getState().settings?.currency ?? "USD";
  const itemTable = container.querySelector("#item-table");
  const cartLines = container.querySelector("#cart-lines");
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
    allItems = Array.isArray(items) ? items : [];
    allWaiters = Array.isArray(waiters) ? waiters : [];
    waiterSelect.innerHTML = allWaiters.length
      ? allWaiters.map((w) => `<option value="${w.id}">${escapeHtml(w.full_name)}</option>`).join("")
      : `<option value="">No active waiters — add one first</option>`;
  } catch { return; }

  const itemId = (item) => String(item.id);
  const categoryName = (item) => item.category_name || item.category || item.categoryName || "—";
  const itemType = (item) => item.type || item.item_type || item.itemType || "";
  const itemPrice = (item) => Number(item.selling_price ?? item.sellingPrice ?? 0);

  function renderItemTable() {
    const filterType = typeFilter.value;
    const query = searchInput.value.trim().toLocaleLowerCase();
    const filtered = allItems.filter((item) => {
      if (filterType && itemType(item) !== filterType) return false;
      if (!query) return true;
      return [item.name, categoryName(item), itemPrice(item)].some((value) => String(value ?? "").toLocaleLowerCase().includes(query));
    });

    renderTable(itemTable, {
      columns: [
        { key: "name", label: "Item" },
        { key: "category", label: "Category", format: (item) => categoryName(item) },
        { key: "selling_price", label: "Unit price", numeric: true, format: (item) => formatMoney(itemPrice(item), currency) },
        { key: "action", label: "Action", format: (item) => {
          const button = document.createElement("button");
          button.className = "btn btn-primary btn-sm";
          button.type = "button";
          button.dataset.posAction = "add";
          button.dataset.cartItemId = itemId(item);
          const quantity = cart.get(itemId(item))?.quantity || 0;
          button.textContent = quantity ? `Add +1 (${quantity})` : "Add";
          return button;
        }},
      ],
      rows: filtered,
      emptyMessage: query ? "No items match your search." : "No active items match this filter.",
      getRowKey: (item) => itemId(item),
    });
  }

  function calculateLineTotal(line) {
    return itemPrice(line.item) * line.quantity;
  }

  function renderCart() {
    const rows = Array.from(cart.values());
    const totalQuantity = rows.reduce((sum, line) => sum + line.quantity, 0);
    const saleTotal = rows.reduce((sum, line) => sum + calculateLineTotal(line), 0);

    container.querySelector("#cart-count").textContent = `${totalQuantity} item${totalQuantity === 1 ? "" : "s"}`;
    container.querySelector("#cart-total").textContent = formatMoney(saleTotal, currency);
    container.querySelector("#checkout-btn").disabled = rows.length === 0;

    renderTable(cartLines, {
      columns: [
        { key: "item", label: "Item", format: (line) => { const el = document.createElement("strong"); el.textContent = line.item.name || "Unnamed item"; return el; } },
        { key: "price", label: "Price", numeric: true, format: (line) => formatMoney(itemPrice(line.item), currency) },
        { key: "quantity", label: "Qty", numeric: true, format: (line) => {
          const wrap = document.createElement("div");
          wrap.className = "pos-qty-controls";
          const dec = document.createElement("button");
          dec.type = "button"; dec.className = "btn btn-ghost btn-sm";
          dec.dataset.posAction = "decrease"; dec.dataset.cartItemId = itemId(line.item); dec.textContent = "−";
          const input = document.createElement("input");
          input.type = "number"; input.min = "1"; input.step = "1"; input.value = String(line.quantity);
          input.className = "input input-mono pos-qty-input";
          input.dataset.posAction = "quantity"; input.dataset.cartItemId = itemId(line.item);
          const inc = document.createElement("button");
          inc.type = "button"; inc.className = "btn btn-ghost btn-sm";
          inc.dataset.posAction = "increase"; inc.dataset.cartItemId = itemId(line.item); inc.textContent = "+";
          wrap.append(dec, input, inc);
          return wrap;
        }},
        { key: "total", label: "Line total", numeric: true, format: (line) => formatMoney(calculateLineTotal(line), currency) },
      ],
      rows,
      emptyMessage: "No items yet — click Add beside an item.",
      getRowKey: (line) => itemId(line.item),
    });
  }

  function addToCart(item) {
    const id = itemId(item);
    const current = cart.get(id);
    cart.set(id, { item, quantity: (current?.quantity || 0) + 1 });
    renderItemTable();
    renderCart();
  }

  function setQuantity(id, quantity) {
    const key = String(id);
    const line = cart.get(key);
    if (!line) return;
    const next = Math.floor(Number(quantity));
    if (!Number.isFinite(next) || next <= 0) cart.delete(key);
    else cart.set(key, { ...line, quantity: next });
    renderItemTable();
    renderCart();
  }

  function changeQuantity(id, delta) {
    const key = String(id);
    const line = cart.get(key);
    if (!line) return;
    setQuantity(key, line.quantity + delta);
  }

  // Use one delegated handler on the stable POS container. The table itself is
  // re-rendered after every cart change, so handlers must not be attached to
  // individual table rows/buttons.
  container.addEventListener("click", (event) => {
    const button = event.target.closest("button[data-pos-action][data-cart-item-id]");
    if (!button || !container.contains(button)) return;
    const action = button.dataset.posAction;
    const id = button.dataset.cartItemId;
    event.preventDefault();
    if (action === "add") {
      const item = allItems.find((candidate) => itemId(candidate) === id);
      if (item) addToCart(item);
    } else if (action === "increase") {
      changeQuantity(id, 1);
    } else if (action === "decrease") {
      changeQuantity(id, -1);
    }
  });

  container.addEventListener("change", (event) => {
    const input = event.target.closest("input[data-pos-action="quantity"]");
    if (!input) return;
    event.preventDefault();
    setQuantity(input.dataset.cartItemId, input.value);
  });

  container.addEventListener("keydown", (event) => {
    if (event.target.matches("input[data-pos-action=quantity]") && event.key === "Enter") {
      event.preventDefault();
      event.target.blur();
    }
  });

  function localDateKey(date = new Date()) {
    const y = date.getFullYear(); const m = String(date.getMonth() + 1).padStart(2, "0"); const d = String(date.getDate()).padStart(2, "0");
    return `${y}-${m}-${d}`;
  }

  async function renderDailyHistory() {
    const table = container.querySelector("#daily-sales-table");
    try {
      const sales = await withErrorToast(() => api.pos.listSales(null, 200));
      const todaySales = sales.filter((sale) => String(sale.created_at || "").slice(0, 10) === localDateKey());
      const waiterById = new Map(allWaiters.map((w) => [w.id, w.full_name]));
      const total = todaySales.reduce((sum, sale) => sum + Number(sale.total_amount || 0), 0);
      container.querySelector("#daily-history-total").textContent = `${todaySales.length} sale${todaySales.length === 1 ? "" : "s"} · ${formatMoney(total, currency)}`;
      renderTable(table, {
        columns: [
          { key: "created_at", label: "Time", format: (sale) => { const value = String(sale.created_at || ""); return value.includes("T") ? value.split("T")[1].slice(0, 5) : value.slice(11, 16) || "—"; } },
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
    if (!cart.size) return pushToast("Add at least one item before completing the sale.", "error");

    const lines = Array.from(cart.values()).map(({ item, quantity }) => ({
      item_id: item.id,
      quantity: Math.floor(quantity),
    })).filter((line) => line.quantity > 0);
    if (!lines.length) return pushToast("Add at least one item before completing the sale.", "error");

    const button = container.querySelector("#checkout-btn");
    button.disabled = true;
    button.textContent = "Completing…";
    try {
      // This is the ONLY place that calls the backend checkout command.
      // Adding/changing cart items never creates a sale.
      const result = await api.pos.checkout({
        waiter_id: waiterId,
        user_id: store.getState().user?.id ?? null,
        payment_method: container.querySelector("#payment-select").value,
        lines,
        note: null,
      });
      const completedTotal = Number(result?.sale?.total_amount ?? lines.reduce((sum, line) => {
        const item = allItems.find((candidate) => itemId(candidate) === String(line.item_id));
        return sum + itemPrice(item || {}) * line.quantity;
      }, 0));
      pushToast(`Sale completed — ${formatMoney(completedTotal, currency)}.`, "success");
      cart = new Map();
      renderItemTable();
      renderCart();
      await renderDailyHistory();
    } catch (err) {
      pushToast(typeof err === "string" ? err : (err?.message || "Couldn't complete the sale."), "error");
      renderCart();
    } finally {
      button.textContent = "Complete sale";
      button.disabled = cart.size === 0;
    }
  });
}

function escapeHtml(text) {
  const div = document.createElement("div");
  div.textContent = text ?? "";
  return div.innerHTML;
}
