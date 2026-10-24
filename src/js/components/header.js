/**
 * Header bar. `setHeaderTitle` is called by the router's onNavigate
 * callback; `setHeaderActions` lets a page drop a button (e.g. "New
 * sale", "Add waiter") into the top-right slot for as long as it's
 * the active page.
 */

export function setHeaderTitle(headerEl, title) {
  const el = headerEl.querySelector("#header-title");
  if (!el) return;
  el.textContent = title || "";
  el.classList.toggle("header-natra-title", title === "NATRA");
}

/** @param {HTMLElement[]} nodes */
export function setHeaderActions(headerEl, nodes = []) {
  const actions = headerEl.querySelector("#header-actions");
  if (!actions) return;
  actions.innerHTML = "";
  nodes.forEach((node) => actions.appendChild(node));
}

export function clearHeaderActions(headerEl) {
  setHeaderActions(headerEl, []);
}

/**
 * Shared header sync status. The button remains unavailable until a real
 * synchronization handler is attached in a later task.
 * @param {HTMLElement} headerEl
 * @param {"ready"|"syncing"|"failed"|"completed"} status
 * @param {boolean} available
 */
export function setHeaderSyncStatus(headerEl, status = "ready", available = false) {
  const button = headerEl.querySelector("#header-sync-btn");
  if (!button) return;
  const labels = {
    ready: "↻ Sync Now",
    syncing: "↻ Syncing…",
    failed: "↻ Retry Sync",
    completed: "✓ Synced",
  };
  const safeStatus = Object.hasOwn(labels, status) ? status : "ready";
  button.textContent = labels[safeStatus];
  button.dataset.syncStatus = safeStatus;
  button.disabled = !available || safeStatus === "syncing";
  button.setAttribute("aria-busy", String(safeStatus === "syncing"));
  button.title = !available
    ? "Cloud synchronization is not configured yet"
    : safeStatus === "syncing"
      ? "Synchronization is in progress"
      : safeStatus === "failed"
        ? "Synchronization failed; retry when ready"
        : safeStatus === "completed"
          ? "Last synchronization completed"
          : "Synchronize new changes";
}
