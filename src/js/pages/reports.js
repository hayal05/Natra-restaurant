/**
 * Reports: a daily report (revenue/cost/profit + products sold, split
 * ready-made vs cookable, with print support) on top, then the existing
 * monthly revenue/costs/cash-flow report underneath. All figures come
 * straight from report_service.rs / financial_service.rs — this page just
 * requests the date windows the backend computes against.
 */

import * as api from "../api.js";
import { store, withErrorToast } from "../state.js";
import { clearHeaderActions } from "../components/header.js";
import { createStatCard } from "../components/stat-card.js";
import { renderTable } from "../components/table.js";
import { renderBarChart } from "../components/bar-chart.js";
import { formatMoney } from "../utils/currency.js";
import { monthName, monthRange, dateRange, todayIso, formatDate } from "../utils/dates.js";

export const title = "Reports";

export async function render(container) {
  clearHeaderActions(document);

  const now = new Date();
  const state = { year: now.getFullYear(), month: now.getMonth() + 1, dailyDate: todayIso() };

  container.innerHTML = `
    <div class="page-heading">
      <div>
        <h1>Reports</h1>
        <p class="page-subtitle">Daily revenue and product sales, plus monthly costs and cash flow.</p>
      </div>
    </div>
    <div class="card" id="daily-report-card">
      <div class="card-header">
        <span class="card-title">Daily report</span>
        <div style="display:flex; align-items:center; gap:var(--space-2);">
          <input type="date" class="input" id="daily-date" style="width:auto;" />
          <button type="button" class="btn btn-secondary" id="daily-print-btn">Print</button>
        </div>
      </div>
      <div class="grid grid-cols-3" id="daily-stat-row"></div>
      <div class="report-product-group">
        <h3 class="report-group-title">Ready-made items</h3>
        <div id="daily-readymade-table"></div>
      </div>
      <div class="report-product-group">
        <h3 class="report-group-title">Cookable items</h3>
        <div id="daily-cookable-table"></div>
      </div>
    </div>
    <div style="display:flex; justify-content:flex-end; gap: var(--space-2); margin-bottom: var(--space-3);">
      <select class="select" id="month-select"></select>
      <select class="select" id="year-select"></select>
    </div>
    <div class="grid grid-cols-4" id="stat-row"></div>
    <div class="grid grid-cols-2">
      <div class="card">
        <div class="card-header"><span class="card-title">Product performance</span></div>
        <div id="performance-table"></div>
      </div>
      <div class="card">
        <div class="card-header"><span class="card-title">Sales mix</span></div>
        <div id="sales-mix-chart"></div>
      </div>
    </div>
    <div class="card">
      <div class="card-header"><span class="card-title" id="cash-flow-title"></span></div>
      <div id="cash-flow-chart"></div>
    </div>
  `;

  // The print sheet lives outside the router-managed container, as a direct
  // child of <body>, so the print stylesheet can hide #app (sidebar, header,
  // the rest of this page) while printing without also hiding this element -
  // display:none on an ancestor always wins over a child's own display rule.
  let dailyPrintSheet = document.getElementById("daily-print-sheet");
  if (!dailyPrintSheet) {
    dailyPrintSheet = document.createElement("div");
    dailyPrintSheet.id = "daily-print-sheet";
    document.body.appendChild(dailyPrintSheet);
  }

  const dailyDateInput = container.querySelector("#daily-date");
  dailyDateInput.value = state.dailyDate;

  const monthSelect = container.querySelector("#month-select");
  monthSelect.innerHTML = Array.from({ length: 12 }, (_, i) => i + 1)
    .map((m) => `<option value="${m}" ${m === state.month ? "selected" : ""}>${monthName(m)}</option>`)
    .join("");

  const yearSelect = container.querySelector("#year-select");
  const currentYear = now.getFullYear();
  yearSelect.innerHTML = [currentYear, currentYear - 1, currentYear - 2]
    .map((y) => `<option value="${y}">${y}</option>`)
    .join("");

  // The router moves this page's DOM out of `container` into the live outlet
  // once render() resolves, leaving `container` itself empty. Every element a
  // later callback (month/year/date change) needs must be captured here while
  // `container` still holds it - never re-queried from `container` afterwards.
  const els = {
    dailyDateInput,
    dailyPrintBtn: container.querySelector("#daily-print-btn"),
    dailyStatRow: container.querySelector("#daily-stat-row"),
    dailyReadyMadeTable: container.querySelector("#daily-readymade-table"),
    dailyCookableTable: container.querySelector("#daily-cookable-table"),
    dailyPrintSheet,
    statRow: container.querySelector("#stat-row"),
    performanceTable: container.querySelector("#performance-table"),
    salesMixChart: container.querySelector("#sales-mix-chart"),
    cashFlowTitle: container.querySelector("#cash-flow-title"),
    cashFlowChart: container.querySelector("#cash-flow-chart"),
  };

  dailyDateInput.addEventListener("change", () => {
    state.dailyDate = dailyDateInput.value || todayIso();
    loadDailyReport(els, state);
  });
  els.dailyPrintBtn.addEventListener("click", () => printDailyReport(els, state));

  monthSelect.addEventListener("change", () => {
    state.month = Number(monthSelect.value);
    loadReport(els, state);
  });
  yearSelect.addEventListener("change", () => {
    state.year = Number(yearSelect.value);
    loadReport(els, state);
  });

  await Promise.all([loadDailyReport(els, state), loadReport(els, state)]);
}

async function loadDailyReport(els, state) {
  const currency = store.getState().settings?.currency ?? "USD";
  const [from, to] = dateRange(state.dailyDate, state.dailyDate);

  let summary, performance;
  try {
    [summary, performance] = await Promise.all([
      withErrorToast(() => api.expenses.calculateProfit(from, to)),
      withErrorToast(() => api.reports.productPerformance(from, to)),
    ]);
  } catch {
    return; // toast already shown
  }

  // Stashed so the print button always prints exactly what's on screen.
  state.dailySummary = summary;
  state.dailyPerformance = performance;

  const totalCost = summary.ready_made_costs + summary.raw_material_costs;

  const statRow = els.dailyStatRow;
  statRow.innerHTML = "";
  statRow.appendChild(createStatCard({ label: "Total revenue", value: formatMoney(summary.sales, currency) }));
  statRow.appendChild(createStatCard({ label: "Total cost", value: formatMoney(totalCost, currency) }));
  statRow.appendChild(
    createStatCard({
      label: "Net profit",
      value: formatMoney(summary.profit, currency),
      tone: summary.profit >= 0 ? "sage" : "rust",
    })
  );

  const readyMade = performance.filter((p) => p.item_type === "ready_made");
  const cookable = performance.filter((p) => p.item_type === "cookable");

  renderProductTable(els.dailyReadyMadeTable, readyMade, currency, "No ready-made items sold on this day.");
  renderProductTable(els.dailyCookableTable, cookable, currency, "No cookable items sold on this day.");
}

function renderProductTable(host, rows, currency, emptyMessage) {
  renderTable(host, {
    columns: [
      { key: "item_name", label: "Item" },
      { key: "quantity_sold", label: "Qty", numeric: true },
      { key: "unit_price", label: "Unit price", numeric: true, format: (r) => formatMoney(r.total_sales / r.quantity_sold, currency) },
      { key: "unit_cost", label: "Unit cost", numeric: true, format: (r) => formatMoney(r.total_cost / r.quantity_sold, currency) },
      { key: "total_sales", label: "Total", numeric: true, format: (r) => formatMoney(r.total_sales, currency) },
    ],
    rows,
    emptyMessage,
    getRowKey: (r) => r.item_id,
  });
}

/**
 * Builds a clean, app-chrome-free version of the daily report into the
 * off-screen #daily-print-sheet and opens the browser print dialog. A
 * print stylesheet (ux-overrides.css) hides everything else in #app while
 * printing, so the sheet is all that ends up on paper / in "Save as PDF".
 */
function printDailyReport(els, state) {
  const { dailySummary: summary, dailyPerformance: performance } = state;
  if (!summary || !performance) return;

  const currency = store.getState().settings?.currency ?? "USD";
  const restaurantName = store.getState().settings?.restaurant_name ?? "";
  const totalCost = summary.ready_made_costs + summary.raw_material_costs;
  const readyMade = performance.filter((p) => p.item_type === "ready_made");
  const cookable = performance.filter((p) => p.item_type === "cookable");

  const sheet = els.dailyPrintSheet;
  sheet.innerHTML = "";

  const header = document.createElement("div");
  header.className = "print-sheet-header";
  header.innerHTML = `
    <h1>${escapeHtml(restaurantName)}</h1>
    <p>Daily report — ${escapeHtml(formatDate(state.dailyDate))}</p>
  `;
  sheet.appendChild(header);

  const statRow = document.createElement("div");
  statRow.className = "grid grid-cols-3";
  statRow.appendChild(createStatCard({ label: "Total revenue", value: formatMoney(summary.sales, currency) }));
  statRow.appendChild(createStatCard({ label: "Total cost", value: formatMoney(totalCost, currency) }));
  statRow.appendChild(
    createStatCard({
      label: "Net profit",
      value: formatMoney(summary.profit, currency),
      tone: summary.profit >= 0 ? "sage" : "rust",
    })
  );
  sheet.appendChild(statRow);

  const readyMadeSection = document.createElement("div");
  readyMadeSection.className = "report-product-group";
  readyMadeSection.innerHTML = `<h3 class="report-group-title">Ready-made items</h3>`;
  const readyMadeTable = document.createElement("div");
  readyMadeSection.appendChild(readyMadeTable);
  sheet.appendChild(readyMadeSection);
  renderProductTable(readyMadeTable, readyMade, currency, "No ready-made items sold on this day.");

  const cookableSection = document.createElement("div");
  cookableSection.className = "report-product-group";
  cookableSection.innerHTML = `<h3 class="report-group-title">Cookable items</h3>`;
  const cookableTable = document.createElement("div");
  cookableSection.appendChild(cookableTable);
  sheet.appendChild(cookableSection);
  renderProductTable(cookableTable, cookable, currency, "No cookable items sold on this day.");

  window.print();
}

function escapeHtml(value) {
  const div = document.createElement("div");
  div.textContent = value ?? "";
  return div.innerHTML;
}

async function loadReport(els, { year, month }) {
  const currency = store.getState().settings?.currency ?? "USD";
  const [from, to] = monthRange(year, month);

  let summary, performance, mix, cashFlow;
  try {
    [summary, performance, mix, cashFlow] = await Promise.all([
      withErrorToast(() => api.reports.monthly(year, month)),
      withErrorToast(() => api.reports.productPerformance(from, to)),
      withErrorToast(() => api.reports.salesMix(from, to)),
      withErrorToast(() => api.reports.cashFlowByYear(year)),
    ]);
  } catch {
    return; // toast already shown
  }

  const statRow = els.statRow;
  statRow.innerHTML = "";
  statRow.appendChild(createStatCard({ label: "Revenue", value: formatMoney(summary.sales, currency) }));
  statRow.appendChild(createStatCard({ label: "Ready-made costs", value: formatMoney(summary.ready_made_costs, currency) }));
  statRow.appendChild(createStatCard({ label: "Raw-material costs", value: formatMoney(summary.raw_material_costs, currency) }));
  statRow.appendChild(
    createStatCard({
      label: "Profit",
      value: formatMoney(summary.profit, currency),
      tone: summary.profit >= 0 ? "sage" : "rust",
      sublabel: `after ${formatMoney(summary.other_expenses, currency)} other expenses`,
    })
  );

  renderTable(els.performanceTable, {
    columns: [
      { key: "item_name", label: "Item" },
      { key: "quantity_sold", label: "Qty", numeric: true },
      { key: "total_sales", label: "Sales", numeric: true, format: (r) => formatMoney(r.total_sales, currency) },
      { key: "total_cost", label: "Cost", numeric: true, format: (r) => formatMoney(r.total_cost, currency) },
    ],
    rows: performance,
    emptyMessage: "No sales recorded for this month.",
    getRowKey: (r) => r.item_id,
  });

  renderBarChart(els.salesMixChart, {
    items: mix.map((m) => ({ label: m.item_name, value: m.percentage_of_sales, tone: "navy" })),
    formatValue: (v) => `${v.toFixed(1)}%`,
    emptyMessage: "No sales recorded for this month.",
  });

  els.cashFlowTitle.textContent = `Cash flow — ${year}`;
  renderBarChart(els.cashFlowChart, {
    items: cashFlow.map((m) => ({
      label: monthName(m.month),
      value: m.net,
      tone: m.net >= 0 ? "sage" : "rust",
    })),
    formatValue: (v) => formatMoney(v, currency),
    emptyMessage: "No activity recorded for this year.",
  });
}
