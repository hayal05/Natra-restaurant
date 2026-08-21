/**
 * Sidebar nav items. The shell itself (brand, footer, collapse
 * behavior) is built once in app.js — this module owns just the
 * navigation list, so the set of pages lives in exactly one place.
 */

export const NAV_ITEMS = [
  { path: "/dashboard", label: "Dashboard" },
  { path: "/pos", label: "Point of sale" },
  { path: "/waiters", label: "Waiters" },
  { path: "/items", label: "Items" },
  { path: "/raw-materials", label: "Raw materials" },
  { path: "/expenses", label: "Expenses" },
  { path: "/reports", label: "Reports" },
  { path: "/settings", label: "Settings" },
];

/** Builds the <nav> contents into `navEl`. Call `setActive` on navigation. */
export function renderNavItems(navEl) {
  navEl.innerHTML = "";
  NAV_ITEMS.forEach((item) => {
    const link = document.createElement("a");
    link.href = `#${item.path}`;
    link.className = "nav-item";
    link.dataset.path = item.path;
    link.innerHTML = `<span class="nav-item-label">${item.label}</span>`;
    navEl.appendChild(link);
  });
}

export function setActiveNavItem(navEl, path) {
  navEl.querySelectorAll(".nav-item").forEach((el) => {
    el.classList.toggle("is-active", el.dataset.path === path);
  });
}
