/**
 * Simple horizontal bar list — no charting library, just divs sized by
 * percentage (see .chart-bar-* in charts.css). Used for sales mix and
 * cash flow, where a quick relative comparison matters more than a
 * precise plotted chart.
 *
 * @param {HTMLElement} container
 * @param {{
 *   items: { label: string, value: number, tone?: 'sage'|'rust'|'navy' }[],
 *   formatValue?: (value: number) => string,
 *   emptyMessage?: string,
 * }} opts
 */
export function renderBarChart(container, { items, formatValue = (v) => String(v), emptyMessage = "Nothing to show yet." }) {
  container.innerHTML = "";

  if (!items.length) {
    const empty = document.createElement("div");
    empty.className = "table-empty";
    empty.textContent = emptyMessage;
    container.appendChild(empty);
    return;
  }

  const maxAbs = Math.max(...items.map((i) => Math.abs(i.value)), 0.01);

  const wrap = document.createElement("div");
  wrap.className = "chart-bars";

  items.forEach((item) => {
    const row = document.createElement("div");
    row.className = "chart-bar-row";

    const label = document.createElement("span");
    label.className = "chart-bar-label";
    label.textContent = item.label;
    label.title = item.label;
    row.appendChild(label);

    const track = document.createElement("div");
    track.className = "chart-bar-track";
    const fill = document.createElement("div");
    fill.className = "chart-bar-fill" + (item.tone ? ` tone-${item.tone}` : "");
    const pct = Math.min(100, (Math.abs(item.value) / maxAbs) * 100);
    fill.style.width = `${pct}%`;
    track.appendChild(fill);
    row.appendChild(track);

    const value = document.createElement("span");
    value.className = "chart-bar-value";
    value.textContent = formatValue(item.value);
    row.appendChild(value);

    wrap.appendChild(row);
  });

  container.appendChild(wrap);
}
