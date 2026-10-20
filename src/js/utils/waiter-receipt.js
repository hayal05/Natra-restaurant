/**
 * Single-sale waiter receipt, rendered from committed checkout snapshots.
 * Print uses an isolated iframe so POS styling doesn't appear on paper.
 */
const escapeHtml = (value) => String(value ?? "").replace(/[&<>"']/g, (c) =>
  ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
const amount = (v) => Number(v ?? 0).toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 });

export function waiterReceiptHtml({ result, waiterName, cashierName, restaurantName, currency }) {
  const sale = result.sale;
  const items = result.items || [];
  // SQLite stores the sale time in UTC without an offset marker.
  const date = new Date(String(sale.created_at).replace(" ", "T") + "Z");
  const dateLabel = new Intl.DateTimeFormat("en-GB", {
    timeZone: "Africa/Addis_Ababa", day: "2-digit", month: "2-digit", year: "numeric",
  }).format(date);
  const timeLabel = new Intl.DateTimeFormat("en-US", {
    timeZone: "Africa/Addis_Ababa", hour: "2-digit", minute: "2-digit", hour12: true,
  }).format(date);
  const rows = items.map((item) => \`<tr>
    <td>\${escapeHtml(item.item_name)}</td><td class="num">\${item.quantity}</td>
    <td class="num">\${amount(item.unit_price)}</td><td class="num">\${amount(item.line_total)}</td>
  </tr>\`).join("");
  const unit = escapeHtml(currency || "ETB");
  const title = escapeHtml(restaurantName || "Restaurant");
  const total = amount(sale.total_amount);
  return \`<!doctype html><html><head><meta charset="utf-8"/><title>Waiter Receipt #\${sale.id}</title>
  <style>
    @page { size: 80mm auto; margin: 4mm; }
    * { box-sizing:border-box; }
    body { margin:0; color:#111; background:white; font:12px Arial,sans-serif; }
    .paper { max-width:76mm; margin:0 auto; padding:6px 3px; }
    .app { text-align:right; font-size:9px; color:#555; }
    .restaurant { text-align:center; font-size:18px; font-weight:800; margin:10px 0 4px; overflow-wrap:anywhere; }
    .title { text-align:center; font-size:12px; font-weight:bold; margin-bottom:10px; }
    .rule { border-top:1px solid #111; margin:8px 0; }
    .meta { display:flex; justify-content:space-between; gap:10px; margin:4px 0; }
    .meta span:last-child { text-align:right; overflow-wrap:anywhere; }
    table { width:100%; border-collapse:collapse; table-layout:fixed; }
    th,td { padding:6px 2px; border-bottom:1px dashed #bbb; overflow-wrap:anywhere; }
    th { font-size:10px; text-align:left; background:#eee; }
    th:first-child { width:38%; }
    .num { text-align:right; }
    .total { display:flex; justify-content:space-between; gap:8px; padding:6px 0; }
    .due { background:#eee; font-weight:bold; font-size:14px; padding:8px 4px; }
    .small { font-size:10px; line-height:1.5; }
    .signatures { display:flex; justify-content:space-between; gap:16px; margin-top:38px; text-align:center; font-size:10px; }
    .signatures div { flex:1; border-top:1px solid; padding-top:6px; }
    footer { text-align:center; margin-top:20px; font-style:italic; }
    @media screen { body { padding:16px; } .paper { border:1px solid #ddd; } }
  </style></head><body><main class="paper">
    <div class="app">Natra Restaurant POS</div>
    <div class="restaurant">\${title}</div>
    <div class="title">Waiter Outstanding Receipt</div>
    <div class="rule"></div>
    <div class="meta"><span>Transaction Ref</span><span>S-\${sale.id}</span></div>
    <div class="meta"><span>Date</span><span>\${dateLabel}</span></div>
    <div class="meta"><span>Time</span><span>\${timeLabel}</span></div>
    <div class="meta"><span>Waiter</span><span>\${escapeHtml(waiterName)}</span></div>
    <div class="meta"><span>Waiter ID</span><span>\${sale.waiter_id}</span></div>
    <div class="meta"><span>Cashier</span><span>\${escapeHtml(cashierName || "—")}</span></div>
    <div class="rule"></div>
    <strong>TRANSACTION ITEMS</strong>
    <table><thead><tr><th>Item</th><th class="num">Qty</th><th class="num">Unit</th><th class="num">Subtotal</th></tr></thead><tbody>\${rows}</tbody></table>
    <div class="total"><span>Grand total</span><strong>\${unit} \${total}</strong></div>
    <div class="total"><span>Amount settled</span><span>\${unit} 0.00</span></div>
    <div class="total due"><span>Outstanding</span><span>\${unit} \${total}</span></div>
    <div class="rule"></div>
    <div class="small">Outstanding amount for this transaction only. This receipt is not a payment confirmation.</div>
    <div class="signatures"><div>Waiter Signature</div><div>Authorized By</div></div>
    <footer>Thank you</footer>
  </main></body></html>\`;
}

export function printWaiterReceipt(html) {
  const frame = document.createElement("iframe");
  frame.style.cssText = "position:fixed;width:0;height:0;border:0;opacity:0";
  frame.setAttribute("title", "Waiter receipt print");
  document.body.appendChild(frame);
  frame.onload = () => {
    try { frame.contentWindow.focus(); frame.contentWindow.print(); }
    catch (err) { frame.remove(); throw err; }
  };
  frame.srcdoc = html;
  // Keep frame during the print dialog; clean it up after the dialog closes.
  frame.contentWindow?.addEventListener("afterprint", () => frame.remove(), { once:true });
}
