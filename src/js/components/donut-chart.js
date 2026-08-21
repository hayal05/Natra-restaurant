/**
 * Donut chart — an SVG ring built from stacked stroke-dasharray segments
 * (no charting library), with a legend and a center total. Used for the
 * dashboard's sales mix, where seeing each item's share of the whole
 * matters more than comparing exact bar lengths.
 *
 * @param {HTMLElement} container
 * @param {{
 *   items: { label: string, value: number }[],
 *   formatValue?: (value: number) => string,
 *   centerLabel?: string,
 *   centerValue?: string,
 *   emptyMessage?: string,
 * }} opts
 */
const SEGMENT_TONES = ["navy", "sage", "rust", "amber", "navy-bright"];
const RADIUS = 15.9155; // circumference works out to a clean 100
const CIRCUMFERENCE = 2 * Math.PI * RADIUS;

export function renderDonutChart(
  container,
  { items, formatValue = (v) => String(v), centerLabel, centerValue, emptyMessage = "Nothing to show yet." }
) {
  container.innerHTML = "";

  const total = items.reduce((sum, i) => sum + Math.max(i.value, 0), 0);

  if (!items.length || total <= 0) {
    const empty = document.createElement("div");
    empty.className = "table-empty";
    empty.textContent = emptyMessage;
    container.appendChild(empty);
    return;
  }

  const wrap = document.createElement("div");
  wrap.className = "donut-chart";

  const figure = document.createElement("div");
  figure.className = "donut-chart-figure";

  const svgNS = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(svgNS, "svg");
  svg.setAttribute("viewBox", "0 0 36 36");

  const track = document.createElementNS(svgNS, "circle");
  track.setAttribute("cx", "18");
  track.setAttribute("cy", "18");
  track.setAttribute("r", String(RADIUS));
  track.setAttribute("class", "donut-chart-segment");
  track.style.stroke = "var(--color-paper-alt)";
  svg.appendChild(track);

  let offset = 0;
  items.forEach((item, i) => {
    const pct = Math.max(item.value, 0) / total;
    const dash = pct * CIRCUMFERENCE;
    const seg = document.createElementNS(svgNS, "circle");
    seg.setAttribute("cx", "18");
    seg.setAttribute("cy", "18");
    seg.setAttribute("r", String(RADIUS));
    seg.setAttribute("class", "donut-chart-segment");
    seg.style.stroke = `var(--color-${SEGMENT_TONES[i % SEGMENT_TONES.length]})`;
    seg.setAttribute("stroke-dasharray", `${dash} ${CIRCUMFERENCE - dash}`);
    seg.setAttribute("stroke-dashoffset", String(-offset));
    svg.appendChild(seg);
    offset += dash;
  });

  figure.appendChild(svg);

  if (centerValue) {
    const center = document.createElement("div");
    center.className = "donut-chart-center";
    center.innerHTML = `
      <div class="donut-chart-center-value">${centerValue}</div>
      ${centerLabel ? `<div class="donut-chart-center-label">${centerLabel}</div>` : ""}
    `;
    figure.appendChild(center);
  }

  wrap.appendChild(figure);

  const legend = document.createElement("div");
  legend.className = "donut-legend";
  items.forEach((item, i) => {
    const row = document.createElement("div");
    row.className = "donut-legend-item";
    const tone = SEGMENT_TONES[i % SEGMENT_TONES.length];
    row.innerHTML = `
      <span class="donut-legend-swatch" style="background: var(--color-${tone})"></span>
      <span class="donut-legend-label" title="${item.label}">${item.label}</span>
      <span class="donut-legend-value">${formatValue(item.value)}</span>
    `;
    legend.appendChild(row);
  });
  wrap.appendChild(legend);

  container.appendChild(wrap);
}
