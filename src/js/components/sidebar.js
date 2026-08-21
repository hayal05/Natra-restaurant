export const NAV_ITEMS = [
  { path: "/dashboard", label: "Dashboard", icon: "grid" },
  { path: "/pos", label: "Point of sale", icon: "cart" },
  { path: "/waiters", label: "Waiters", icon: "users" },
  { path: "/items", label: "Items", icon: "box" },
  { path: "/raw-materials", label: "Raw materials", icon: "layers" },
  { path: "/expenses", label: "Expenses", icon: "wallet" },
  { path: "/reports", label: "Reports", icon: "chart" },
  { path: "/settings", label: "Settings", icon: "settings" },
];

const ICONS = {
  grid: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/></svg>',
  cart: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M3 4h2l2.1 10.2a2 2 0 0 0 2 1.6h7.8a2 2 0 0 0 1.9-1.4L20.5 8H6"/><circle cx="9" cy="20" r="1.2"/><circle cx="18" cy="20" r="1.2"/></svg>',
  users: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><circle cx="9" cy="8" r="3"/><path d="M3 20c0-3.3 2.7-5 6-5s6 1.7 6 5"/><path d="M16 5.2a3 3 0 0 1 0 5.6M17 15c2.4.2 4 1.7 4 4"/></svg>',
  box: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="m12 3 8 4.2v9.6L12 21l-8-4.2V7.2L12 3Z"/><path d="m4.3 7.3 7.7 4 7.7-4M12 11.3V21"/></svg>',
  layers: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="m12 3 9 5-9 5-9-5 9-5Z"/><path d="m3 12 9 5 9-5M3 16l9 5 9-5"/></svg>',
  wallet: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M4 6.5A2.5 2.5 0 0 1 6.5 4H20v16H6.5A2.5 2.5 0 0 1 4 17.5v-11Z"/><path d="M4 7h16M16 13h4"/><circle cx="16" cy="13" r=".7" fill="currentColor"/></svg>',
  chart: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M4 19V5M4 19h17"/><path d="m7 15 3-4 3 2 5-6"/></svg>',
  settings: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M12 3v3M12 18v3M3 12h3M18 12h3M5.6 5.6l2.1 2.1M16.3 16.3l2.1 2.1M18.4 5.6l-2.1 2.1M7.7 16.3l-2.1 2.1"/><circle cx="12" cy="12" r="4"/></svg>'
};

export function renderNavItems(navEl) {
  navEl.innerHTML = "";
  NAV_ITEMS.forEach((item) => {
    const link = document.createElement("a");
    link.href = `#${item.path}`;
    link.className = "nav-item";
    link.dataset.path = item.path;
    link.setAttribute("aria-label", item.label);
    link.innerHTML = `<span class="nav-item-icon" aria-hidden="true">${ICONS[item.icon]}</span><span class="nav-item-label">${item.label}</span>`;
    navEl.appendChild(link);
  });
}

export function setActiveNavItem(navEl, path) {
  navEl.querySelectorAll(".nav-item").forEach((el) => {
    el.classList.toggle("is-active", el.dataset.path === path);
  });
}
