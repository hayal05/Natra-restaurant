/**
 * Point of sale. Items are staged in a client-side Current sale cart only.
 * Nothing is recorded as a sale until Complete sale is pressed.
 */
import * as api from "../api.js";
import { store, pushToast, withErrorToast } from "../state.js";
import { clearHeaderActions } from "../components/header.js";
import { renderTable } from "../components/table.js";
import { openModal, closeModal } from "../components/modal.js";
import { formatMoney, formatNumber } from "../utils/currency.js";
import { humanizeEnum } from "../utils/formatting.js";
import { todayIso, daysAgoIso, dateRange, formatDateTime } from "../utils/dates.js";

// Sales can only be reversed within this many hours of being made — must
// match sales_service::REVERSAL_WINDOW_HOURS on the backend, which is the
// real enforcement point; this is only used to gray out the button early.
const REVERSAL_WINDOW_HOURS = 24;

// sale.created_at is stored by SQLite as a UTC "YYYY-MM-DD HH:MM:SS" string
// with no timezone marker, so it must be parsed as UTC explicitly here.
function saleAgeHours(sale) {
  const raw = String(sale.created_at || "").replace(" ", "T");
  const parsed = new Date(raw.endsWith("Z") ? raw : `${raw}Z`);
  if (Number.isNaN(parsed.getTime())) return Infinity;
  return (Date.now() - parsed.getTime()) / 3_600_000;
}

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
            <label class="pos-search" aria-label="Search items"><span aria-hidden="true">⌕</span><input class="input" id="item-search" type="search" placeholder="Search name, category or price..." autocomplete="off" /></label>
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
    <div class="card pos-daily-history">
      <div class="card-header"><span class="card-title">Sales history</span><span class="pos-history-total" id="daily-history-total"></span></div>
      <div class="pos-history-filters" style="display:flex;gap:var(--space-2);align-items:flex-end;flex-wrap:wrap;margin-bottom:var(--space-3);">
        <div class="field" style="margin-top:0"><label class="field-label" for="history-from">From</label><input class="input" type="date" id="history-from" /></div>
        <div class="field" style="margin-top:0"><label class="field-label" for="history-to">To</label><input class="input" type="date" id="history-to" /></div>
        <div style="display:flex;gap:var(--space-2)">
          <button class="btn btn-ghost btn-sm" type="button" id="history-today">Today</button>
          <button class="btn btn-ghost btn-sm" type="button" id="history-7d">Last 7 days</button>
          <button class="btn btn-ghost btn-sm" type="button" id="history-30d">Last 30 days</button>
        </div>
      </div>
      <div id="daily-sales-table"></div>
    </div>`;

  const currency = store.getState().settings?.currency ?? "USD";
  const itemTable = container.querySelector("#item-table");
  const cartLines = container.querySelector("#cart-lines");
  const waiterSelect = container.querySelector("#waiter-select");
  const typeFilter = container.querySelector("#type-filter");
  const searchInput = container.querySelector("#item-search");
  const paymentSelect = container.querySelector("#payment-select");
  const cartCountEl = container.querySelector("#cart-count");
  const cartTotalEl = container.querySelector("#cart-total");
  const checkoutBtn = container.querySelector("#checkout-btn");
  const dailyHistoryTable = container.querySelector("#daily-sales-table");
  const dailyHistoryTotalEl = container.querySelector("#daily-history-total");
  const historyFromInput = container.querySelector("#history-from");
  const historyToInput = container.querySelector("#history-to");
  const historyTodayBtn = container.querySelector("#history-today");
  const history7dBtn = container.querySelector("#history-7d");
  const history30dBtn = container.querySelector("#history-30d");
  // The router renders each page into a detached staging element and only
  // moves its children into the live outlet once render() resolves. `container`
  // itself stays behind, empty, after that move - so every element we'll need
  // later (cart totals, the checkout button, daily history) must be captured
  // here, up front, while `container` still holds them. Re-querying `container`
  // from inside a later callback (a click handler, a re-render) would silently
  // hit the emptied-out node and return null.
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
      : `<option value="">No active waiters - add one first</option>`;
  } catch {
    return;
  }

  const itemId = (item) => String(item.id);
  const categoryName = (item) => item.category_name || item.category || item.categoryName || "-";
  const itemType = (item) => item.type || item.item_type || item.itemType || "";
  const itemPrice = (item) => Number(item.selling_price ?? item.sellingPrice ?? 0);
  const lineTotal = (line) => itemPrice(line.item) * line.quantity;

  function syncAddButton(button) {
    const quantity = cart.get(String(button.dataset.itemId))?.quantity || 0;
    button.textContent = quantity ? `Add + (${quantity})` : "Add";
  }

  function bindAddButtons() {
    itemTable.querySelectorAll("button.pos-add-item").forEach((button) => {
      button.onclick = (event) => {
        event.preventDefault();
        event.stopPropagation();
        const item = allItems.find((candidate) => itemId(candidate) === String(button.dataset.itemId));
        if (!item) {
          pushToast("This item is no longer available.", "error");
          return;
        }
        addToCart(item, button);
      };
    });
  }

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
          button.className = "btn btn-primary btn-sm pos-add-item";
          button.type = "button";
          button.dataset.itemId = itemId(item);
          syncAddButton(button);
          return button;
        }},
      ],
      rows: filtered,
      emptyMessage: query ? "No items match your search." : "No active items match this filter.",
      getRowKey: (item) => itemId(item),
    });

    bindAddButtons();
  }

  function renderCart() {
    const rows = Array.from(cart.values());
    const totalQuantity = rows.reduce((sum, line) => sum + line.quantity, 0);
    const saleTotal = rows.reduce((sum, line) => sum + lineTotal(line), 0);

    cartCountEl.textContent = `${totalQuantity} item${totalQuantity === 1 ? "" : "s"}`;
    cartTotalEl.textContent = formatMoney(saleTotal, currency);
    checkoutBtn.disabled = rows.length === 0;

    cartLines.innerHTML = "";
    if (!rows.length) {
      const empty = document.createElement("div");
      empty.className = "table-empty";
      empty.textContent = "No items yet - click Add beside an item.";
      cartLines.appendChild(empty);
      return;
    }

    const wrap = document.createElement("div");
    wrap.className = "table-wrap";
    const table = document.createElement("table");
    table.className = "data-table";
    table.innerHTML = `<thead><tr><th>Item</th><th class="col-numeric">Price</th><th class="col-numeric">Qty</th><th class="col-numeric">Line total</th></tr></thead>`;
    const tbody = document.createElement("tbody");

    rows.forEach((line) => {
      const tr = document.createElement("tr");
      const itemCell = document.createElement("td");
      const itemName = document.createElement("strong");
      itemName.textContent = line.item.name || "Unnamed item";
      itemCell.appendChild(itemName);

      const priceCell = document.createElement("td");
      priceCell.className = "col-numeric";
      priceCell.textContent = formatNumber(itemPrice(line.item));

      const qtyCell = document.createElement("td");
      qtyCell.className = "col-numeric";
      const controls = document.createElement("div");
      controls.className = "pos-qty-controls";

      const dec = document.createElement("button");
      dec.type = "button";
      dec.className = "btn btn-ghost pos-qty-step";
      dec.textContent = "-";
      dec.setAttribute("aria-label", "Decrease quantity");
      dec.addEventListener("click", () => changeQuantity(line.item, -1));

      const input = document.createElement("input");
      input.type = "number";
      input.min = "1";
      input.step = "1";
      input.value = String(line.quantity);
      input.className = "input input-mono pos-qty-input";
      input.addEventListener("change", () => setQuantity(line.item, input.value));

      const inc = document.createElement("button");
      inc.type = "button";
      inc.className = "btn btn-ghost pos-qty-step";
      inc.textContent = "+";
      inc.setAttribute("aria-label", "Increase quantity");
      inc.addEventListener("click", () => changeQuantity(line.item, 1));

      controls.append(dec, input, inc);
      qtyCell.appendChild(controls);

      const totalCell = document.createElement("td");
      totalCell.className = "col-numeric";
      totalCell.textContent = formatNumber(lineTotal(line));

      tr.append(itemCell, priceCell, qtyCell, totalCell);
      tbody.appendChild(tr);
    });

    table.appendChild(tbody);
    wrap.appendChild(table);
    cartLines.appendChild(wrap);
  }

  function addToCart(item, sourceButton = null) {
    const id = itemId(item);
    const current = cart.get(id);
    const nextQuantity = (current?.quantity || 0) + 1;
    cart.set(id, { item, quantity: nextQuantity });

    // The cart is updated before any item-table repaint. Current sale is the source of truth.
    renderCart();
    if (sourceButton && itemTable.contains(sourceButton)) syncAddButton(sourceButton);
  }

  function setQuantity(item, quantity) {
    const id = itemId(item);
    const line = cart.get(id);
    if (!line) return;
    const next = Math.floor(Number(quantity));
    if (!Number.isFinite(next) || next <= 0) cart.delete(id);
    else cart.set(id, { ...line, quantity: next });
    renderCart();
    const button = itemTable.querySelector(`button.pos-add-item[data-item-id="${CSS.escape(id)}"]`);
    if (button) syncAddButton(button);
  }

  function changeQuantity(item, delta) {
    const id = itemId(item);
    const line = cart.get(id);
    if (line) setQuantity(item, line.quantity + delta);
  }

  // History is browsable across the trailing month; the date pickers are
  // clamped to that window so "select a date or a range" always lands on
  // data that's actually kept around.
  const HISTORY_WINDOW_DAYS = 30;
  const historyMinDate = daysAgoIso(HISTORY_WINDOW_DAYS);
  const historyMaxDate = todayIso();
  [historyFromInput, historyToInput].forEach((input) => {
    input.min = historyMinDate;
    input.max = historyMaxDate;
    input.value = historyMaxDate;
  });

  function setHistoryRange(fromDate, toDate) {
    historyFromInput.value = fromDate;
    historyToInput.value = toDate;
    renderDailyHistory();
  }

  async function renderDailyHistory() {
    const table = dailyHistoryTable;
    let fromDate = historyFromInput.value || historyMaxDate;
    let toDate = historyToInput.value || historyMaxDate;
    // Keep the range sane if the user picks an end before the start.
    if (fromDate > toDate) [fromDate, toDate] = [toDate, fromDate];
    const [from, to] = dateRange(fromDate, toDate);
    const isSingleDay = fromDate === toDate;

    try {
      const sales = await withErrorToast(() => api.pos.listSales(null, from, to, null));
      const waiterById = new Map(allWaiters.map((w) => [w.id, w.full_name]));
      // Reversed sales stay visible in the history list (with a "Reversed"
      // badge) for audit purposes, but shouldn't count toward the period's
      // total, matching how the backend excludes them from every revenue figure.
      const activeSales = sales.filter((sale) => !sale.is_reversed);
      const total = activeSales.reduce((sum, sale) => sum + Number(sale.total_amount || 0), 0);
      dailyHistoryTotalEl.textContent = `${activeSales.length} sale${activeSales.length === 1 ? "" : "s"} - ${formatMoney(total, currency)}`;
      renderTable(table, {
        columns: [
          { key: "created_at", label: isSingleDay ? "Time" : "Date & time", format: (sale) => {
            if (isSingleDay) { const value = String(sale.created_at || ""); return value.includes("T") ? value.split("T")[1].slice(0, 5) : value.slice(11, 16) || "-"; }
            return formatDateTime(sale.created_at);
          } },
          { key: "waiter_id", label: "Waiter", format: (sale) => waiterById.get(sale.waiter_id) || `Waiter #${sale.waiter_id}` },
          { key: "total_quantity", label: "Qty", numeric: true },
          { key: "payment_method", label: "Payment", format: (sale) => humanizeEnum(sale.payment_method) },
          { key: "total_amount", label: "Total", numeric: true, format: (sale) => formatMoney(sale.total_amount, currency) },
          { key: "is_settled", label: "Status", format: (sale) => {
            if (sale.is_reversed) { const badge = document.createElement("span"); badge.className = "badge badge-rust"; badge.textContent = "Reversed"; return badge; }
            const badge = document.createElement("span"); badge.className = `badge ${sale.is_settled ? "badge-sage" : "badge-amber"}`; badge.textContent = sale.is_settled ? "Settled" : "Open"; return badge;
          } },
          { key: "actions", label: "", format: (sale) => {
            if (sale.is_reversed) return "";
            const btn = document.createElement("button");
            btn.className = "btn btn-ghost btn-sm";
            btn.textContent = "Reverse";
            const withinWindow = saleAgeHours(sale) < REVERSAL_WINDOW_HOURS;
            if (!withinWindow) { btn.disabled = true; btn.title = `Sales can only be reversed within ${REVERSAL_WINDOW_HOURS} hours.`; }
            btn.addEventListener("click", () => confirmReverseSale(sale));
            return btn;
          } },
        ], rows: sales, emptyMessage: isSingleDay ? "No sales recorded for this day." : "No sales recorded for this period.", getRowKey: (sale) => sale.id,
      });
    } catch {
      dailyHistoryTotalEl.textContent = "";
    }
  }

  function confirmReverseSale(sale) {
    const body = document.createElement("div");
    body.innerHTML = `<p style="font-size:var(--text-sm);color:var(--color-ink-soft);">This voids the sale of <strong>${formatMoney(sale.total_amount, currency)}</strong> and removes it from revenue, cost, and waiter-receivable totals. The record stays in history, marked as reversed. This can't be undone.</p>`;
    const cancel = document.createElement("button");
    cancel.className = "btn btn-secondary"; cancel.type = "button"; cancel.textContent = "Cancel";
    cancel.addEventListener("click", closeModal);
    const confirmBtn = document.createElement("button");
    confirmBtn.className = "btn btn-danger"; confirmBtn.type = "button"; confirmBtn.textContent = "Reverse sale";
    confirmBtn.addEventListener("click", async () => {
      confirmBtn.disabled = true;
      try {
        await withErrorToast(() => api.pos.reverseSale(sale.id));
        pushToast("Sale reversed.", "success");
        closeModal();
        await renderDailyHistory();
      } catch {
        confirmBtn.disabled = false;
      }
    });
    openModal({ title: "Reverse this sale?", content: body, actions: [cancel, confirmBtn] });
  }

  typeFilter.addEventListener("change", renderItemTable);
  searchInput.addEventListener("input", renderItemTable);
  historyFromInput.addEventListener("change", renderDailyHistory);
  historyToInput.addEventListener("change", renderDailyHistory);
  historyTodayBtn.addEventListener("click", () => setHistoryRange(historyMaxDate, historyMaxDate));
  history7dBtn.addEventListener("click", () => setHistoryRange(daysAgoIso(6), historyMaxDate));
  history30dBtn.addEventListener("click", () => setHistoryRange(historyMinDate, historyMaxDate));
  renderItemTable();
  renderCart();
  await renderDailyHistory();

  checkoutBtn.addEventListener("click", async () => {
    const waiterId = Number(waiterSelect.value);
    if (!waiterId) return pushToast("Select a waiter before completing the sale.", "error");
    if (!cart.size) return pushToast("Add at least one item before completing the sale.", "error");
    const lines = Array.from(cart.values()).map(({ item, quantity }) => ({ item_id: item.id, quantity: Math.floor(quantity) })).filter((line) => line.quantity > 0);
    const button = checkoutBtn;
    button.disabled = true;
    button.textContent = "Completing...";
    try {
      const result = await api.pos.checkout({ waiter_id: waiterId, user_id: store.getState().user?.id ?? null, payment_method: paymentSelect.value, lines, note: null });
      const completedTotal = Number(result?.sale?.total_amount ?? lines.reduce((sum, line) => {
        const item = allItems.find((candidate) => itemId(candidate) === String(line.item_id));
        return sum + itemPrice(item || {}) * line.quantity;
      }, 0));
      pushToast(`Sale completed - ${formatMoney(completedTotal, currency)}.`, "success");
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
