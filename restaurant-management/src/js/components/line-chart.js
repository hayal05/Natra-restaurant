/**
 * Two-series line chart — plain SVG polylines on a scaled grid, no
 * charting library. Built for the dashboard's revenue-vs-profit trend,
 * but generic enough for any { date, seriesA, seriesB } series.
 *
 * @param {HTMLElement} container
 * @param {{
 *   points: { date: string, revenue: number, profit: number }[],
 *   formatValue?: (value: number) => string,
 *   formatDate?: (date: string) => string,
 *   emptyMessage?: string,
 * }} opts
 */
const WIDTH = 640;
const HEIGHT = 220;
const PADDING = { top: 12, right: 12, bottom: 24, left: 12 };

export function renderLineChart(
  container,
  { points, formatValue = (v) => String(v), formatDate = (d) => d, emptyMessage = "Nothing to show yet." }
) {
  container.innerHTML = "";

  if (!points.length) {
    const empty = document.createElement("div");
    empty.className = "table-empty";
    empty.textContent = emptyMessage;
    container.appendChild(empty);
    return;
  }

  const wrap = document.createElement("div");
  wrap.className = "line-chart";

  const legend = document.createElement("div");
  legend.className = "line-chart-legend";
  legend.innerHTML = `
    <span class="line-chart-legend-item">
      <span class="line-chart-legend-swatch" style="background: var(--color-navy)"></span> Revenue
    </span>
    <span class="line-chart-legend-item">
      <span class="line-chart-legend-swatch" style="background: var(--color-sage)"></span> Profit
    </span>
  `;
  wrap.appendChild(legend);

  const plotW = WIDTH - PADDING.left - PADDING.right;
  const plotH = HEIGHT - PADDING.top - PADDING.bottom;

  const allValues = points.flatMap((p) => [p.revenue, p.profit]);
  const rawMax = Math.max(...allValues, 0);
  const rawMin = Math.min(...allValues, 0);
  const max = rawMax === rawMin ? rawMax + 1 : rawMax;
  const min = rawMin;

  const xFor = (i) => PADDING.left + (points.length === 1 ? plotW / 2 : (i / (points.length - 1)) * plotW);
  const yFor = (v) => PADDING.top + plotH - ((v - min) / (max - min)) * plotH;
  const zeroY = yFor(0);

  const svgNS = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(svgNS, "svg");
  svg.setAttribute("viewBox", `0 0 ${WIDTH} ${HEIGHT}`);
  svg.setAttribute("class", "line-chart-figure");

  // Gridlines: zero line + top line
  [PADDING.top, zeroY].forEach((y) => {
    const line = document.createElementNS(svgNS, "line");
    line.setAttribute("x1", String(PADDING.left));
    line.setAttribute("x2", String(WIDTH - PADDING.right));
    line.setAttribute("y1", String(y));
    line.setAttribute("y2", String(y));
    line.setAttribute("class", "line-chart-gridline");
    svg.appendChild(line);
  });

  const revenuePoints = points.map((p, i) => [xFor(i), yFor(p.revenue)]);
  const profitPoints = points.map((p, i) => [xFor(i), yFor(p.profit)]);

  // Filled area under the revenue line, down to the zero line.
  const areaPath =
    `M ${revenuePoints[0][0]} ${zeroY} ` +
    revenuePoints.map(([x, y]) => `L ${x} ${y}`).join(" ") +
    ` L ${revenuePoints[revenuePoints.length - 1][0]} ${zeroY} Z`;
  const area = document.createElementNS(svgNS, "path");
  area.setAttribute("d", areaPath);
  area.setAttribute("class", "line-chart-area-revenue");
  svg.appendChild(area);

  const makeLine = (pts, cls) => {
    const path = document.createElementNS(svgNS, "path");
    path.setAttribute("d", pts.map(([x, y], i) => `${i === 0 ? "M" : "L"} ${x} ${y}`).join(" "));
    path.setAttribute("class", cls);
    svg.appendChild(path);
  };
  makeLine(revenuePoints, "line-chart-path-revenue");
  makeLine(profitPoints, "line-chart-path-profit");

  const makeDots = (pts, cls, values) => {
    pts.forEach(([x, y], i) => {
      const dot = document.createElementNS(svgNS, "circle");
      dot.setAttribute("cx", String(x));
      dot.setAttribute("cy", String(y));
      dot.setAttribute("r", "2.5");
      dot.setAttribute("class", cls);
      const title = document.createElementNS(svgNS, "title");
      title.textContent = `${formatDate(points[i].date)}: ${formatValue(values[i])}`;
      dot.appendChild(title);
      svg.appendChild(dot);
    });
  };
  makeDots(revenuePoints, "line-chart-dot-revenue", points.map((p) => p.revenue));
  makeDots(profitPoints, "line-chart-dot-profit", points.map((p) => p.profit));

  // X-axis labels: first, middle, last date to keep it uncluttered.
  const labelIndexes = points.length > 2 ? [0, Math.floor((points.length - 1) / 2), points.length - 1] : points.map((_, i) => i);
  labelIndexes.forEach((i) => {
    const label = document.createElementNS(svgNS, "text");
    label.setAttribute("x", String(xFor(i)));
    label.setAttribute("y", String(HEIGHT - 6));
    label.setAttribute("text-anchor", i === 0 ? "start" : i === points.length - 1 ? "end" : "middle");
    label.setAttribute("class", "line-chart-axis-label");
    label.textContent = formatDate(points[i].date);
    svg.appendChild(label);
  });

  wrap.appendChild(svg);
  container.appendChild(wrap);
}
