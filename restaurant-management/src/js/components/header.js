/**
 * Header bar. `setHeaderTitle` is called by the router's onNavigate
 * callback; `setHeaderActions` lets a page drop a button (e.g. "New
 * sale", "Add waiter") into the top-right slot for as long as it's
 * the active page.
 */

export function setHeaderTitle(headerEl, title) {
  const el = headerEl.querySelector("#header-title");
  if (el) el.textContent = title || "";
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
