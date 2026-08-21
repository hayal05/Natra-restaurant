/**
 * Waiters: roster + receivables. A waiter's receivable is the sum of
 * their unsettled sales. This page intentionally uses a dense table so
 * a large roster stays scannable and every record remains visible.
 */

import * as api from "../api.js";
import { store, pushToast, withErrorToast } from "../state.js";
import { setHeaderActions } from "../components/header.js";
import { renderTable } from "../components/table.js";
import { openModal, closeModal } from "../components/modal.js";
import { formatMoney } from "../utils/currency.js";
import { firstError, isNonEmpty } from "../utils/validation.js";

export const title = "Waiters";

export async function render(container) {
  container.innerHTML = `
    <div class="page-heading">
      <div>
        <h1>Waiters</h1>
        <p class="page-subtitle">Roster, activity and unsettled sales in one place.</p>
      </div>
    </div>
    <div class="checkbox-row">
      <input type="checkbox" id="show-inactive" />
      <label for="show-inactive" style="font-size: var(--text-sm); color: var(--color-ink-soft);">Show inactive waiters</label>
    </div>
    <div class="card catalog-table">
      <div class="card-header">
        <span class="card-title">Waiter roster</span>
        <span class="badge badge-neutral" id="waiter-count">0 records</span>
      </div>
      <div id="waiter-table"></div>
    </div>
  `;

  const addBtn = document.createElement("button");
  addBtn.className = "btn btn-primary";
  addBtn.textContent = "Add waiter";
  addBtn.addEventListener("click", () => openAddWaiterModal(container));
  setHeaderActions(document, [addBtn]);

  container.querySelector("#show-inactive").addEventListener("change", () => loadWaiters(container));
  await loadWaiters(container);
}

async function loadWaiters(container) {
  const currency = store.getState().settings?.currency ?? "USD";
  const showInactive = container.querySelector("#show-inactive").checked;
  const table = container.querySelector("#waiter-table");

  let all, receivables;
  try {
    [all, receivables] = await Promise.all([
      withErrorToast(() => api.waiters.list(!showInactive)),
      withErrorToast(() => api.waiters.listReceivables()),
    ]);
  } catch {
    return;
  }

  const receivableMap = new Map(receivables.map((r) => [r.waiter.id, r.receivable]));
  container.querySelector("#waiter-count").textContent = `${all.length} record${all.length === 1 ? "" : "s"}`;

  renderTable(table, {
    columns: [
      { key: "full_name", label: "Waiter" },
      { key: "phone", label: "Phone", format: (w) => w.phone || "—" },
      {
        key: "status",
        label: "Status",
        format: (w) => {
          const badge = document.createElement("span");
          badge.className = `badge ${w.is_active ? "badge-sage" : "badge-neutral"}`;
          badge.textContent = w.is_active ? "Active" : "Inactive";
          return badge;
        },
      },
      {
        key: "receivable",
        label: "Receivable",
        numeric: true,
        format: (w) => formatMoney(receivableMap.get(w.id) ?? 0, currency),
      },
      {
        key: "actions",
        label: "Actions",
        format: (w) => {
          const actions = document.createElement("div");
          actions.className = "row-actions";
          const receivable = receivableMap.get(w.id) ?? 0;

          if (receivable > 0) {
            const settleBtn = document.createElement("button");
            settleBtn.className = "btn btn-secondary btn-sm";
            settleBtn.textContent = "Settle";
            settleBtn.addEventListener("click", async () => {
              try {
                await withErrorToast(() => api.waiters.settle(w.id));
                pushToast("Waiter settled.", "success");
                loadWaiters(container);
              } catch {}
            });
            actions.appendChild(settleBtn);
          }

          const toggleBtn = document.createElement("button");
          toggleBtn.className = "btn btn-ghost btn-sm";
          toggleBtn.textContent = w.is_active ? "Deactivate" : "Activate";
          toggleBtn.addEventListener("click", async () => {
            try {
              await withErrorToast(() => api.waiters.setActive(w.id, !w.is_active));
              loadWaiters(container);
            } catch {}
          });
          actions.appendChild(toggleBtn);
          return actions;
        },
      },
    ],
    rows: all,
    emptyMessage: "No waiters yet — add your first waiter to start taking sales.",
    getRowKey: (w) => w.id,
  });
}

function openAddWaiterModal(container) {
  const form = document.createElement("form");
  form.noValidate = true;
  form.innerHTML = `
    <div class="field">
      <label class="field-label" for="waiter-name">Full name</label>
      <input class="input" id="waiter-name" name="fullName" type="text" autofocus />
      <span class="field-error" id="waiter-name-error"></span>
    </div>
    <div class="field">
      <label class="field-label" for="waiter-phone">Phone (optional)</label>
      <input class="input" id="waiter-phone" name="phone" type="text" />
    </div>
  `;

  const cancelBtn = document.createElement("button");
  cancelBtn.className = "btn btn-secondary";
  cancelBtn.type = "button";
  cancelBtn.textContent = "Cancel";
  cancelBtn.addEventListener("click", closeModal);

  const saveBtn = document.createElement("button");
  saveBtn.className = "btn btn-primary";
  saveBtn.type = "button";
  saveBtn.textContent = "Add waiter";
  saveBtn.addEventListener("click", () => form.requestSubmit());

  openModal({ title: "New waiter", content: form, actions: [cancelBtn, saveBtn] });

  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    const fullName = form.fullName.value.trim();
    const phone = form.phone.value.trim();

    const error = firstError([[isNonEmpty(fullName), "Enter the waiter's name."]]);
    if (error) {
      form.querySelector("#waiter-name").classList.add("has-error");
      form.querySelector("#waiter-name-error").textContent = error;
      return;
    }

    saveBtn.disabled = true;
    try {
      await withErrorToast(() => api.waiters.create(fullName, phone || null));
      pushToast("Waiter added.", "success");
      closeModal();
      loadWaiters(container);
    } catch {
      saveBtn.disabled = false;
    }
  });
}
