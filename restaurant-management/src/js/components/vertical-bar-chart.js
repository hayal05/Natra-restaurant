/**
 * Grouped vertical bar chart — a cluster of bars per group (e.g. one
 * month), each cluster holding one bar per series (e.g. revenue, cost).
 * No charting library, just divs sized by percentage of the tallest
 * value across the whole chart so every cluster shares one scale.
 *
 * @param {HTMLElement} container
 * @param {{
 *   groups: { label: string, values: number[] }[],
 *   series: { label: string, tone: 'sage'|'rust'|'navy'|'amber' }[],
 *   formatValue?: (value: number) => string,
 *   emptyMessage?: string,
 * }} opts
 */
export function renderVerticalBarChart(container, { groups, series, formatValue = (v) => String(v), emptyMessage = "Nothing to show yet." }) {
  container.innerHTML = "";

  if (!groups.length) {
    const empty = document.createElement("div");
    empty.className = "table-empty";
    empty.textContent = emptyMessage;
    container.appendChild(empty);
    return;
  }

  const wrap = document.createElement("div");
  wrap.className = "vbar-chart";

  const legend = document.createElement("div");
  legend.className = "vbar-chart-legend";
  legend.innerHTML = series
    .map(
      (s) => `
        <span class="vbar-chart-legend-item">
          <span class="vbar-chart-legend-swatch" style="background: var(--color-${s.tone})"></span> ${s.label}
        </span>
      `
    )
    .join("");
  wrap.appendChild(legend);

  const maxAbs = Math.max(...groups.flatMap((g) => g.values.map((v) => Math.abs(v))), 0.01);

  const groupsEl = document.createElement("div");
  groupsEl.className = "vbar-chart-groups";

  groups.forEach((group) => {
    const groupEl = document.createElement("div");
    groupEl.className = "vbar-chart-group";

    const bars = document.createElement("div");
    bars.className = "vbar-chart-group-bars";

    group.values.forEach((value, i) => {
      const pct = Math.min(100, (Math.abs(value) / maxAbs) * 100);
      const track = document.createElement("div");
      track.className = "vbar-chart-track";
      track.title = `${series[i]?.label ?? ""}: ${formatValue(value)}`;
      const bar = document.createElement("div");
      bar.className = "vbar-chart-bar" + (series[i]?.tone ? ` tone-${series[i].tone}` : "");
      bar.style.height = `${pct}%`;
      track.appendChild(bar);
      bars.appendChild(track);
    });
    groupEl.appendChild(bars);

    const label = document.createElement("span");
    label.className = "vbar-chart-group-label";
    label.textContent = group.label;
    groupEl.appendChild(label);

    groupsEl.appendChild(groupEl);
  });

  wrap.appendChild(groupsEl);
  container.appendChild(wrap);
}
