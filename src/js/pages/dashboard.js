/**
 * Dashboard: the "walk in and see how the day looks" page. Pulls a
 * single aggregated payload from dashboard_summary (see
 * src-tauri/src/services/dashboard_service.rs) rather than firing off
 * half a dozen separate commands.
 */

import * as api from "../api.js";
import { store, withErrorToast } from "../state.js";
import { clearHeaderActions } from "../components/header.js";
import { createStatCard } from "../components/stat-card.js";
import { createWaiterCard } from "../components/waiter-card.js";
import { renderTable } from "../components/table.js";
import { renderDonutChart } from "../components/donut-chart.js";
import { renderLineChart } from "../components/line-chart.js";
import { renderVerticalBarChart } from "../components/vertical-bar-chart.js";
import { formatMoney } from "../utils/currency.js";
import { formatDateShort, monthName } from "../utils/dates.js";

export const title = "Dashboard";

export async function render(container) {
  clearHeaderActions(document);

  container.innerHTML = `
    <div class="page-heading">
      <div>
        <h1>Dashboard</h1>
        <p class="page-subtitle">Today's performance and what's still outstanding.</p>
      </div>
    </div>
    <div class="grid grid-cols-4" id="stat-row"></div>
    <div class="grid grid-cols-2">
      <div class="card">
        <div class="card-header"><span class="card-title">Waiter receivables</span></div>
        <div class="grid" id="receivables-list" style="gap: var(--space-3);"></div>
      </div>
      <div class="card">
        <div class="card-header"><span class="card-title">Sales mix — this month</span></div>
        <div id="sales-mix-chart"></div>
      </div>
    </div>
    <div class="card">
      <div class="card-header"><span class="card-title">Revenue vs. profit — last 14 days</span></div>
      <div class="trend-split">
        <div class="trend-split-main" id="revenue-profit-chart"></div>
        <div class="trend-split-aside">
          <span class="trend-split-aside-title">Cost vs. revenue — last 6 months</span>
          <div class="trend-split-aside-body" id="cost-revenue-chart"></div>
        </div>
      </div>
    </div>
    <div class="card">
      <div class="card-header"><span class="card-title">Top products today</span></div>
      <div id="top-products-table"></div>
    </div>
  `;

  const currency = store.getState().settings?.currency ?? "USD";

  let summary;
  try {
    summary = await withErrorToast(() => api.dashboard.summary());
  } catch {
    return; // toast already shown
  }

  const statRow = container.querySelector("#stat-row");
  statRow.appendChild(createStatCard({
    label: "Today's sales",
    value: formatMoney(summary.today.sales, currency),
  }));
  statRow.appendChild(createStatCard({
    label: "Today's profit",
    value: formatMoney(summary.today.profit, currency),
    tone: summary.today.profit >= 0 ? "sage" : "rust",
  }));
  statRow.appendChild(createStatCard({
    label: "This month's profit",
    value: formatMoney(summary.this_month.profit, currency),
    tone: summary.this_month.profit >= 0 ? "sage" : "rust",
    sublabel: `on ${formatMoney(summary.this_month.sales, currency)} sales`,
  }));
  statRow.appendChild(createStatCard({
    label: "Waiter receivables",
    value: formatMoney(summary.total_receivable, currency),
    tone: summary.total_receivable > 0 ? "rust" : "sage",
  }));

  const receivablesList = container.querySelector("#receivables-list");
  if (!summary.waiter_receivables.length) {
    receivablesList.innerHTML = `<div class="empty-state"><div class="empty-state-title">No active waiters yet</div></div>`;
  } else {
    summary.waiter_receivables
      .slice()
      .sort((a, b) => b.receivable - a.receivable)
      .forEach(({ waiter, receivable }) => {
        receivablesList.appendChild(
          createWaiterCard({
            waiter,
            receivable,
            currency,
            onSettle: async (waiterId) => {
              try {
                await withErrorToast(() => api.waiters.settle(waiterId));
                render(container);
              } catch {
                /* toast already shown */
              }
            },
          })
        );
      });
  }

  renderDonutChart(container.querySelector("#sales-mix-chart"), {
    items: summary.sales_mix_this_month.map((m) => ({
      label: m.item_name,
      value: m.percentage_of_sales,
    })),
    formatValue: (v) => `${v.toFixed(1)}%`,
    centerValue: formatMoney(summary.this_month.sales, currency),
    centerLabel: "this month",
    emptyMessage: "No sales recorded this month yet.",
  });

  renderLineChart(container.querySelector("#revenue-profit-chart"), {
    points: summary.revenue_profit_trend.map((d) => ({
      date: d.date,
      revenue: d.revenue,
      profit: d.profit,
    })),
    formatValue: (v) => formatMoney(v, currency),
    formatDate: formatDateShort,
    emptyMessage: "No sales recorded in the last 14 days.",
  });

  renderVerticalBarChart(container.querySelector("#cost-revenue-chart"), {
    groups: summary.cost_revenue_by_month.map((m) => ({
      label: monthName(m.month).slice(0, 3),
      values: [m.revenue, m.cost],
    })),
    series: [
      { label: "Revenue", tone: "navy" },
      { label: "Cost", tone: "rust" },
    ],
    formatValue: (v) => formatMoney(v, currency),
    emptyMessage: "No data yet.",
  });

  renderTable(container.querySelector("#top-products-table"), {
    columns: [
      { key: "item_name", label: "Item" },
      { key: "quantity_sold", label: "Qty", numeric: true },
      { key: "total_sales", label: "Sales", numeric: true, format: (r) => formatMoney(r.total_sales, currency) },
      { key: "total_cost", label: "Cost", numeric: true, format: (r) => formatMoney(r.total_cost, currency) },
    ],
    rows: summary.top_products_today,
    emptyMessage: "No sales recorded today yet.",
    getRowKey: (r) => r.item_id,
  });
}
