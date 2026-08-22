/**
 * Minimal hash router. Each route lazily imports a page module that
 * exports `render(container)` (sync or async) and an optional `title`.
 *
 * Routes are guarded by `store.getState().user`:
 *   - `public: true` routes (login, first-run setup) redirect signed-in
 *     users away to '/dashboard'.
 *   - all other routes redirect signed-out users to '/login'.
 */

import { store } from "./state.js";

const routes = new Map();
let getContainer = null; // (def) => HTMLElement — lets the caller pick public vs. app-shell layout
let onNavigate = null; // optional callback, e.g. to update the header title / active nav item
let navigationVersion = 0;

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
  const version = ++navigationVersion;
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
    // Do not let an old, slower navigation overwrite the current page.
    if (version !== navigationVersion || currentPath() !== path) return;
    renderPlaceholder(outlet, `Couldn't load this page: ${err}`);
    if (onNavigate) onNavigate({ path, title: def.title ?? "" });
    return;
  }

  // A slower previous navigation may finish after the user has already
  // selected another sidebar page. Its title must never win the header.
  if (version !== navigationVersion || currentPath() !== path) return;

  if (typeof mod.render !== "function") {
    renderPlaceholder(outlet, "This page is still being built.");
  } else {
    await mod.render(outlet);
  }

  // Page rendering can be asynchronous (database reads). Check again before
  // updating navigation chrome so a previous page cannot leave its title in
  // the top bar after the user has moved to another page.
  if (version !== navigationVersion || currentPath() !== path) return;

  if (onNavigate) onNavigate({ path, title: def.title ?? mod.title ?? "" });
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
