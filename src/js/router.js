/**
 * Minimal hash router. Each route lazily imports a page module that
 * exports `render(container)` (sync or async) and an optional `title`.
 *
 * Routes are guarded by `store.getState().user`:
 *   - `public: true`  routes (login, first-run setup) redirect signed-in
 *     users away to '/dashboard'.
 *   - all other routes redirect signed-out users to '/login'.
 */

import { store } from "./state.js";

const routes = new Map();
let getContainer = null; // (def) => HTMLElement — lets the caller pick public vs. app-shell layout
let onNavigate = null; // optional callback, e.g. to update the header title / active nav item

export function registerRoutes(routeMap) {
  Object.entries(routeMap).forEach(([path, def]) => routes.set(path, def));
}

/**
 * @param {(def: {public?: boolean, title?: string}) => HTMLElement} containerFn
 *   Called on every navigation to get the element the page should render
 *   into. Lets the caller show a bare centered layout for public routes
 *   (login/setup) and the full sidebar+header shell for everything else.
 */
export function initRouter(containerFn, { onNavigate: navigateCb } = {}) {
  getContainer = containerFn;
  onNavigate = navigateCb ?? null;
  window.addEventListener("hashchange", resolve);
}

export function navigate(path) {
  if (currentPath() === path) {
    resolve();
  } else {
    window.location.hash = path;
  }
}

function currentPath() {
  const hash = window.location.hash.replace(/^#/, "");
  return hash || "/";
}

async function resolve() {
  const path = currentPath();
  const def = routes.get(path) ?? routes.get("/not-found");
  const { user } = store.getState();

  if (!def) return;

  if (def.public && user) {
    navigate("/dashboard");
    return;
  }
  if (!def.public && !user) {
    navigate("/login");
    return;
  }

  if (!getContainer) return;
  const outlet = getContainer(def);
  if (!outlet) return;

  outlet.innerHTML = "";
  let mod;
  try {
    mod = await def.loader();
  } catch (err) {
    renderPlaceholder(outlet, `Couldn't load this page: ${err}`);
    return;
  }

  if (typeof mod.render !== "function") {
    renderPlaceholder(outlet, "This page is still being built.");
  } else {
    await mod.render(outlet);
  }

  if (onNavigate) onNavigate({ path, title: mod.title ?? def.title ?? "" });
}

function renderPlaceholder(outlet, message) {
  const p = document.createElement("p");
  p.style.color = "var(--color-ink-soft)";
  p.style.fontFamily = "var(--font-mono)";
  p.style.fontSize = "var(--text-sm)";
  p.textContent = message;
  outlet.appendChild(p);
}

/** Re-run the guard/resolve step, e.g. right after login sets `user`. */
export function refresh() {
  resolve();
}

export { currentPath };
