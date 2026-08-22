import * as api from "./api.js";
import { registerRoutes, initRouter, navigate } from "./router.js";
import { store, setUser, setSettings, pushToast } from "./state.js";
import { renderNavItems, setActiveNavItem } from "./components/sidebar.js";
import { setHeaderTitle, clearHeaderActions } from "./components/header.js";
import { mountToastStack } from "./components/notification.js";

const appRoot = document.getElementById("app");

function buildLayouts() {
  appRoot.innerHTML = "";
  const publicScreen = document.createElement("div");
  publicScreen.className = "screen-centered"; publicScreen.style.display = "none";
  const publicOutlet = document.createElement("div"); publicOutlet.id = "public-outlet"; publicOutlet.style.width="100%"; publicOutlet.style.maxWidth="28rem"; publicScreen.appendChild(publicOutlet);
  const shell = document.createElement("div"); shell.className="app-shell"; shell.style.display="none";
  shell.innerHTML = `
    <aside class="sidebar"><div class="sidebar-brand"><img src="assets/logo.svg" alt="NATRA"/><span class="sidebar-brand-natra">NATRA</span><span class="sidebar-brand-name">Cashier</span></div><nav class="sidebar-nav" id="sidebar-nav"></nav><div class="sidebar-footer"><div style="display:flex;align-items:center;justify-content:space-between;gap:.5rem"><span id="current-user-label" style="color:var(--color-paper-ink-soft);font-size:var(--text-sm);white-space:nowrap;overflow:hidden;text-overflow:ellipsis"></span><button id="logout-btn" style="color:var(--color-navy-bright);font-size:var(--text-sm);font-weight:650">Log out</button></div><div style="margin-top:.45rem;color:#6f88a3;font-size:.68rem">NATRA Management</div></div></aside>
    <div class="main"><header class="header"><div class="header-title" id="header-title"></div><div class="header-actions" id="header-actions"></div></header><div class="content"><div class="content-inner" id="app-outlet"></div></div></div>`;
  appRoot.append(publicScreen,shell); mountToastStack(appRoot); renderNavItems(shell.querySelector("#sidebar-nav"));
  shell.querySelector("#logout-btn").addEventListener("click",()=>{setUser(null);navigate("/login")});
  return {publicScreen,publicOutlet,shell,appOutlet:shell.querySelector("#app-outlet")};
}

function promotePageActions(appOutlet, shell) {
  const headerActions = shell.querySelector("#header-actions");
  if (!headerActions) return;

  const existing = new Set(Array.from(headerActions.querySelectorAll("button")).map((b) => b.textContent.trim()));
  const candidates = appOutlet.querySelectorAll(".card > .card-header button, .card > .card-header > div button");

  candidates.forEach((button) => {
    // Some pages intentionally keep their actions inside the page (for
    // example Items, where Add item/Add category belong to their tables).
    if (button.dataset.noHeaderPromotion === "true") return;

    const label = button.textContent.trim();
    if (!label || existing.has(label)) {
      if (existing.has(label)) button.remove();
      return;
    }
    existing.add(label);
    headerActions.appendChild(button);
  });
}

async function bootstrap(){
  const {publicScreen,publicOutlet,shell,appOutlet}=buildLayouts();
  registerRoutes({
    "/setup":{loader:()=>import("./auth/initialization.js"),public:true,title:"Setup"},
    "/login":{loader:()=>import("./auth/login.js"),public:true,title:"Login"},
    "/dashboard":{loader:()=>import("./pages/dashboard.js"),title:"Dashboard"},
    "/pos":{loader:()=>import("./pages/pos.js"),title:"Point of sale"},
    "/waiters":{loader:()=>import("./pages/waiters.js"),title:"Waiters"},
    "/items":{loader:()=>import("./pages/items.js"),title:"Items"},
    "/raw-materials":{loader:()=>import("./pages/raw-materials.js"),title:"Raw materials"},
    "/expenses":{loader:()=>import("./pages/expenses.js"),title:"Expenses"},
    "/reports":{loader:()=>import("./pages/reports.js"),title:"Reports"},
    "/settings":{loader:()=>import("./pages/settings.js"),title:"Settings"}
  });

  const actionObserver = new MutationObserver(() => promotePageActions(appOutlet, shell));
  actionObserver.observe(appOutlet, { childList: true, subtree: true });

  initRouter((def)=>{
    if(def.public){
      clearHeaderActions(shell);
      shell.style.display="none";
      publicScreen.style.display="flex";
      return publicOutlet;
    }

    clearHeaderActions(shell);
    setHeaderTitle(shell, def.title ?? "");

    publicScreen.style.display="none";
    shell.style.display="grid";
    return appOutlet;
  },{onNavigate:({path,title})=>{
    setActiveNavItem(shell.querySelector("#sidebar-nav"),path);
    setHeaderTitle(shell,title);
    const userLabel=shell.querySelector("#current-user-label");
    const {user}=store.getState();
    if(userLabel)userLabel.textContent=user?user.full_name:"";
  }});
  store.subscribe(({settings})=>{if(settings)document.title=`${settings.restaurant_name} — NATRA`});
  let initialized=false;try{initialized=await api.auth.isInitialized()}catch(err){pushToast(typeof err==="string"?err:"Couldn't reach the local database.","error",0)}
  if(!initialized){navigate("/setup");return} navigate("/login");
}
export async function completeLogin(user){setUser(user);try{setSettings(await api.settings.get())}catch(err){pushToast(typeof err==="string"?err:"Couldn't load settings.","error")}navigate("/dashboard")}
bootstrap();
