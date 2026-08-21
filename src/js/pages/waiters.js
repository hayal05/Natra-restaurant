/**
 * Waiters: roster + receivables. A waiter's receivable is the sum of
 * their unsettled sales (see src-tauri/src/services/waiter_service.rs)
 * — "Settle" clears it once the business has reconciled with them.
 */

import * as api from "../api.js";
import { store, pushToast, withErrorToast } from "../state.js";
import { setHeaderActions } from "../components/header.js";
import { createWaiterCard } from "../components/waiter-card.js";
import { openModal, closeModal } from "../components/modal.js";
import { firstError, isNonEmpty } from "../utils/validation.js";

export const title = "Waiters";

export async function render(container) {
  container.innerHTML = `
    <div class="page-heading">
      <div>
        <h1>Waiters</h1>
        <p class="page-subtitle">Everyone who can be assigned a sale, and what they still owe.</p>
      </div>
    </div>
    <div class="checkbox-row">
      <input type="checkbox" id="show-inactive" />
      <label for="show-inactive" style="font-size: var(--text-sm); color: var(--color-ink-soft);">Show inactive waiters</label>
    </div>
    <div class="grid grid-cols-3" id="waiter-grid"></div>
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
  const grid = container.querySelector("#waiter-grid");

  let all, receivables;
  try {
    [all, receivables] = await Promise.all([
      withErrorToast(() => api.waiters.list(!showInactive)),
      withErrorToast(() => api.waiters.listReceivables()),
    ]);
  } catch {
    return; // toast already shown
  }

  const receivableMap = new Map(receivables.map((r) => [r.waiter.id, r.receivable]));

  grid.innerHTML = "";
  if (!all.length) {
    grid.innerHTML = `<div class="empty-state"><div class="empty-state-title">No waiters yet</div><p>Add your first waiter to start taking sales.</p></div>`;
    return;
  }

  all.forEach((waiter) => {
    grid.appendChild(
      createWaiterCard({
        waiter,
        receivable: receivableMap.get(waiter.id) ?? 0,
        currency,
        onSettle: async (waiterId) => {
          try {
            await withErrorToast(() => api.waiters.settle(waiterId));
            pushToast("Waiter settled.", "success");
            loadWaiters(container);
          } catch {
            /* toast already shown */
          }
        },
        onToggleActive: async (waiterId, isActive) => {
          try {
            await withErrorToast(() => api.waiters.setActive(waiterId, isActive));
            loadWaiters(container);
          } catch {
            /* toast already shown */
          }
        },
      })
    );
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
