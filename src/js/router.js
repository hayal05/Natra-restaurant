/**
 * Minimal hash router. Each route lazily imports a page module that
 * exports `render(container)` (sync or async) and an optional `title`.
 */

import { store } from "./state.js";

const routes = new Map();
let getContainer = null;
let onNavigate = null;
let navigationVersion = 0;

export function registerRoutes(routeMap) {
  Object.entries(routeMap).forEach(([path, def]) => routes.set(path, def));
}

export function initRouter(containerFn, { onNavigate: navigateCb } = {}) {
  getContainer = containerFn;
  onNavigate = navigateCb ?? null;
  window.addEventListener("hashchange", resolve);
}

export function navigate(path) {
  if (currentPath() === path) resolve();
  else window.location.hash = path;
}

function currentPath() {
  const hash = window.location.hash.replace(/^#/, "");
  return hash || "/";
}

function isCurrent(version, path) {
  return version === navigationVersion && currentPath() === path;
}

async function resolve() {
  const path = currentPath();
  const version = ++navigationVersion;
  const def = routes.get(path) ?? routes.get("/not-found");
  const { user } = store.getState();

  if (!def) return;
  if (def.public && user) { navigate("/dashboard"); return; }
  if (!def.public && !user) { navigate("/login"); return; }
  if (!getContainer) return;

  const outlet = getContainer(def);
  if (!outlet) return;

  // Render into a detached staging element. Page renders perform async DB
  // reads; if the user navigates away during those reads, the old page must
  // never be allowed to mutate the live outlet after the new page is active.
  const staging = document.createElement("div");
  staging.className = "route-staging";

  let mod;
  try {
    mod = await def.loader();
  } catch (err) {
    if (!isCurrent(version, path)) return;
    renderPlaceholder(staging, `Couldn't load this page: ${err}`);
    outlet.replaceChildren(...staging.childNodes);
    if (onNavigate) onNavigate({ path, title: def.title ?? "" });
    return;
  }

  if (!isCurrent(version, path)) return;

  if (typeof mod.render !== "function") {
    renderPlaceholder(staging, "This page is still being built.");
  } else {
    try {
      await mod.render(staging);
    } catch (err) {
      if (!isCurrent(version, path)) return;
      staging.replaceChildren();
      renderPlaceholder(staging, `Couldn't render this page: ${err}`);
    }
  }

  // The page may have awaited one or more database calls. Commit only when
  // the route is still the one the user selected.
  if (!isCurrent(version, path)) return;

  outlet.replaceChildren(...staging.childNodes);
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

export function refresh() { resolve(); }
export { currentPath };
