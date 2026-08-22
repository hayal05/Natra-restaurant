/**
 * Reports: pick a month, see revenue/costs/profit for it, plus product
 * performance and sales mix for that month, and a full year of cash
 * flow underneath. All figures come straight from report_service.rs —
 * this page just requests the same date windows the backend computes.
 */

import * as api from "../api.js";
import { store, withErrorToast } from "../state.js";
import { clearHeaderActions } from "../components/header.js";
import { createStatCard } from "../components/stat-card.js";
import { renderTable } from "../components/table.js";
import { renderBarChart } from "../components/bar-chart.js";
import { formatMoney } from "../utils/currency.js";
import { monthName, monthRange } from "../utils/dates.js";

export const title = "Reports";

export async function render(container) {
  clearHeaderActions(document);

  const now = new Date();
  const state = { year: now.getFullYear(), month: now.getMonth() + 1 };

  container.innerHTML = `
    <div class="page-heading">
      <div>
        <h1>Reports</h1>
        <p class="page-subtitle">Monthly revenue, costs, and cash flow.</p>
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
  // later callback (month/year change) needs must be captured here while
  // `container` still holds it - never re-queried from `container` afterwards.
  const els = {
    statRow: container.querySelector("#stat-row"),
    performanceTable: container.querySelector("#performance-table"),
    salesMixChart: container.querySelector("#sales-mix-chart"),
    cashFlowTitle: container.querySelector("#cash-flow-title"),
    cashFlowChart: container.querySelector("#cash-flow-chart"),
  };

  monthSelect.addEventListener("change", () => {
    state.month = Number(monthSelect.value);
    loadReport(els, state);
  });
  yearSelect.addEventListener("change", () => {
    state.year = Number(yearSelect.value);
    loadReport(els, state);
  });

  await loadReport(els, state);
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
