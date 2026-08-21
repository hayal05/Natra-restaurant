/**
 * Renders a responsive data table into `container`.
 *
 * Tables stay horizontally scrollable on narrow windows while the page itself
 * remains vertically scrollable. This keeps long records usable without
 * hiding columns or forcing card layouts.
 */
export function renderTable(container, { columns, rows, emptyMessage = "Nothing here yet.", getRowKey }) {
  container.innerHTML = "";

  if (!rows.length) {
    const empty = document.createElement("div");
    empty.className = "table-empty";
    empty.textContent = emptyMessage;
    container.appendChild(empty);
    return;
  }

  const wrap = document.createElement("div");
  wrap.className = "table-wrap";

  const table = document.createElement("table");
  table.className = "data-table";

  const thead = document.createElement("thead");
  const headRow = document.createElement("tr");
  columns.forEach((col) => {
    const th = document.createElement("th");
    th.textContent = col.label;
    if (col.numeric) th.classList.add("col-numeric");
    headRow.appendChild(th);
  });
  thead.appendChild(headRow);
  table.appendChild(thead);

  const tbody = document.createElement("tbody");
  rows.forEach((row) => {
    const tr = document.createElement("tr");
    if (getRowKey) tr.dataset.key = getRowKey(row);
    columns.forEach((col) => {
      const td = document.createElement("td");
      if (col.numeric) td.classList.add("col-numeric");
      const value = col.format ? col.format(row) : row[col.key];
      if (value instanceof Node) {
        td.appendChild(value);
      } else {
        td.textContent = value ?? "";
      }
      tr.appendChild(td);
    });
    tbody.appendChild(tr);
  });
  table.appendChild(tbody);

  wrap.appendChild(table);
  container.appendChild(wrap);
}
