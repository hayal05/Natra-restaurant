/** Dashboard: the "walk in and see how the day looks" page. */
import * as api from "../api.js";
import { refresh } from "../router.js";
import { store, withErrorToast, pushToast } from "../state.js";
import { clearHeaderActions } from "../components/header.js";
import { createStatCard } from "../components/stat-card.js";
import { renderTable } from "../components/table.js";
import { renderDonutChart } from "../components/donut-chart.js";
import { renderLineChart } from "../components/line-chart.js";
import { renderVerticalBarChart } from "../components/vertical-bar-chart.js";
import { formatMoney } from "../utils/currency.js";
import { formatDateShort, monthName } from "../utils/dates.js";
export const title = "Dashboard";

export async function render(container) {
  clearHeaderActions(document);
  container.innerHTML=`<div class="grid grid-cols-4" id="stat-row"></div><div class="grid grid-cols-2"><div class="card"><div class="card-header"><span class="card-title">Waiter receivables</span></div><div id="receivables-table"></div></div><div class="card"><div class="card-header"><span class="card-title">Sales mix — this month</span></div><div id="sales-mix-chart"></div></div></div><div class="card"><div class="card-header"><span class="card-title">Revenue vs. profit — last 14 days</span></div><div class="trend-split"><div class="trend-split-main" id="revenue-profit-chart"></div><div class="trend-split-aside"><span class="trend-split-aside-title">Cost vs. revenue — last 6 months</span><div class="trend-split-aside-body" id="cost-revenue-chart"></div></div></div></div><div class="card"><div class="card-header"><span class="card-title">Top products today</span></div><div id="top-products-table"></div></div>`;
  const currency=store.getState().settings?.currency??"USD"; let summary;
  try{summary=await withErrorToast(()=>api.dashboard.summary());}catch{return;}
  const statRow=container.querySelector("#stat-row");
  statRow.appendChild(createStatCard({label:"Today's sales",value:formatMoney(summary.today.sales,currency)}));
  statRow.appendChild(createStatCard({label:"Today's profit",value:formatMoney(summary.today.profit,currency),tone:summary.today.profit>=0?"sage":"rust"}));
  statRow.appendChild(createStatCard({label:"This month's profit",value:formatMoney(summary.this_month.profit,currency),tone:summary.this_month.profit>=0?"sage":"rust",sublabel:`on ${formatMoney(summary.this_month.sales,currency)} sales`}));
  statRow.appendChild(createStatCard({label:"Waiter receivables",value:formatMoney(summary.total_receivable,currency),tone:summary.total_receivable>0?"rust":"sage"}));
  renderTable(container.querySelector("#receivables-table"),{columns:[
    {key:"waiter",label:"Waiter",format:(r)=>waiterIdentity(r.waiter)},
    {key:"receivable",label:"Receivable",numeric:true,format:(r)=>formatMoney(r.receivable,currency)},
    {key:"settle",label:"Action",format:(r)=>{const button=document.createElement("button");button.className="btn btn-secondary btn-sm";button.textContent="Settle";button.disabled=!(r.receivable>0);button.addEventListener("click",async()=>{button.disabled=true;try{await withErrorToast(()=>api.waiters.settle(r.waiter.id));pushToast(`${r.waiter.full_name} settled.`,"success");await refresh();}catch{button.disabled=false;}});return button;}}
  ],rows:summary.waiter_receivables.slice().sort((a,b)=>b.receivable-a.receivable),emptyMessage:"No active waiters yet.",getRowKey:(r)=>r.waiter.id});
  renderDonutChart(container.querySelector("#sales-mix-chart"),{items:summary.sales_mix_this_month.map((m)=>({label:m.item_name,value:m.percentage_of_sales})),formatValue:(v)=>`${v.toFixed(1)}%`,centerValue:formatMoney(summary.this_month.sales,currency),centerLabel:"this month",emptyMessage:"No sales recorded this month yet."});
  renderLineChart(container.querySelector("#revenue-profit-chart"),{points:summary.revenue_profit_trend.map((d)=>({date:d.date,revenue:d.revenue,profit:d.profit})),formatValue:(v)=>formatMoney(v,currency),formatDate:formatDateShort,emptyMessage:"No sales recorded in the last 14 days."});
  renderVerticalBarChart(container.querySelector("#cost-revenue-chart"),{groups:summary.cost_revenue_by_month.map((m)=>({label:monthName(m.month).slice(0,3),values:[m.revenue,m.cost]})),series:[{label:"Revenue",tone:"navy"},{label:"Cost",tone:"rust"}],formatValue:(v)=>formatMoney(v,currency),emptyMessage:"No data yet."});
  renderTable(container.querySelector("#top-products-table"),{columns:[{key:"item_name",label:"Item"},{key:"quantity_sold",label:"Qty",numeric:true},{key:"total_sales",label:"Sales",numeric:true,format:(r)=>formatMoney(r.total_sales,currency)},{key:"total_cost",label:"Cost",numeric:true,format:(r)=>formatMoney(r.total_cost,currency)}],rows:summary.top_products_today,emptyMessage:"No sales recorded today yet.",getRowKey:(r)=>r.item_id});
}

function waiterIdentity(waiter){const wrap=document.createElement("div");wrap.className="waiter-identity";const avatar=document.createElement("div");avatar.className="waiter-avatar";if(waiter.profile_photo){const img=document.createElement("img");img.src=waiter.profile_photo;img.alt="";avatar.appendChild(img);}else{avatar.textContent=(waiter.full_name||"?").trim().charAt(0).toUpperCase();}const name=document.createElement("span");name.textContent=waiter.full_name;wrap.append(avatar,name);return wrap;}
