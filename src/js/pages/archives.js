/** Permanent offline transaction archive management. */
import * as api from "../api.js";
import { clearHeaderActions } from "../components/header.js";
import { pushToast, withErrorToast } from "../state.js";

export const title = "Archives";

export async function render(container) {
  clearHeaderActions(document);
  const parts = Object.fromEntries(new Intl.DateTimeFormat("en-US", {
    timeZone: "Africa/Addis_Ababa", year: "numeric", month: "2-digit", day: "2-digit",
  }).formatToParts(new Date()).filter(p => p.type !== "literal").map(p => [p.type, p.value]));
  const today = `${parts.year}-${parts.month}-${parts.day}`;
  container.innerHTML = `
    <div class="page-heading">
      <div><h1>Archives</h1>
      <p class="page-subtitle">Export Excel to Downloads. Permanent offline copies remain safely stored in AppData.</p></div>
    </div>
    <div class="card">
      <div class="card-header"><span class="card-title">Export transaction archive</span></div>
      <form id="archive-form" style="display:flex;flex-wrap:wrap;align-items:end;gap:var(--space-3)">
        <div class="field"><label class="field-label" for="archive-from">From (Ethiopian local date)</label>
          <input class="input" type="date" id="archive-from" required /></div>
        <div class="field"><label class="field-label" for="archive-through">Through</label>
          <input class="input" type="date" id="archive-through" required /></div>
        <button class="btn btn-primary" id="archive-export" type="submit">Export to Downloads</button>
      </form>
      <p id="archive-feedback" role="status" style="margin-top:var(--space-3);font-size:var(--text-sm)"></p>
    </div>
    <div class="card">
      <div class="card-header"><span class="card-title">Saved batches</span>
        <button class="btn btn-secondary" type="button" id="archive-refresh" data-no-header-promotion="true">Refresh</button></div>
      <p style="font-size:var(--text-sm);color:var(--color-ink-soft)">Daily archiving runs automatically while the app is open. Missing completed days are recovered at startup and hourly.</p>
      <p style="font-size:var(--text-sm);overflow-wrap:anywhere">Archive folder: <code id="archive-directory">Loading…</code></p>
      <div id="archive-list" style="margin-top:var(--space-3)" aria-live="polite">Loading saved batches…</div>
    </div>`;
  const from = container.querySelector("#archive-from");
  const through = container.querySelector("#archive-through");
  const form = container.querySelector("#archive-form");
  const exportBtn = container.querySelector("#archive-export");
  const refresh = container.querySelector("#archive-refresh");
  const feedback = container.querySelector("#archive-feedback");
  const list = container.querySelector("#archive-list");
  const directory = container.querySelector("#archive-directory");
  from.value = through.value = today;

  async function load() {
    refresh.disabled = true;
    list.textContent = "Loading saved batches…";
    try {
      const data = await withErrorToast(() => api.archives.list());
      directory.textContent = data.directory;
      list.replaceChildren();
      if (!data.entries.length) {
        list.textContent = "No saved archive batches yet.";
      } else {
        const table = document.createElement("table");
        table.className = "table";
        const head = document.createElement("thead");
        head.innerHTML = "<tr><th>Batch ID</th><th>File</th><th>Size</th><th>Integrity</th><th>Action</th></tr>";
        table.appendChild(head);
        const body = document.createElement("tbody");
        for (const entry of data.entries) {
          const tr = document.createElement("tr");
          for (const value of [entry.batch_id, entry.filename,
            (entry.size_bytes / 1024).toFixed(1) + " KB", entry.valid ? "Verified" : "Invalid"]) {
            const td = document.createElement("td");
            td.textContent = value;
            tr.appendChild(td);
          }
          const action = document.createElement("td");
          const button = document.createElement("button");
          button.className = "btn btn-secondary";
          button.type = "button";
          button.textContent = "Save to Downloads";
          button.disabled = !entry.valid;
          button.addEventListener("click", async () => {
            button.disabled = true;
            try {
              const saved = await withErrorToast(() => api.archives.download(entry.filename));
              pushToast("Saved to Downloads: " + saved.filename, "success");
              feedback.textContent = "Excel copy saved: " + saved.path;
            } catch { feedback.textContent = "Could not save the archive to Downloads."; }
            finally { button.disabled = !entry.valid; }
          });
          action.appendChild(button);
          tr.appendChild(action);
          body.appendChild(tr);
        }
        table.appendChild(body);
        const wrapper = document.createElement("div");
        wrapper.style.overflowX = "auto";
        wrapper.appendChild(table);
        list.appendChild(wrapper);
      }
    } catch {
      list.textContent = "Unable to load archive batches.";
    } finally { refresh.disabled = false; }
  }
  refresh.addEventListener("click", load);
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (!from.value || !through.value || from.value > through.value) {
      feedback.textContent = "Choose a valid date range.";
      return;
    }
    exportBtn.disabled = true;
    feedback.textContent = "Generating and validating workbook…";
    try {
      const result = await withErrorToast(() => api.archives.export(from.value, through.value));
      const filename = result.path.replace(/.*[\\\\/]/, "");
      const saved = await withErrorToast(() => api.archives.download(filename));
      feedback.textContent = "Excel saved to Downloads: " + saved.path;
      pushToast("Excel saved to Downloads.", "success");
      await load();
    } catch (error) {
      feedback.textContent = "Export failed. No existing batch was overwritten.";
    } finally { exportBtn.disabled = false; }
  });
  await load();
}
