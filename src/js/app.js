import * as api from "./api.js";
import { registerRoutes, initRouter, navigate } from "./router.js";
import { store, setUser, setSettings, pushToast } from "./state.js";
import { renderNavItems, setActiveNavItem } from "./components/sidebar.js";
import { mountToastStack } from "./components/notification.js";

const appRoot = document.getElementById("app");

function buildLayouts() {
  appRoot.innerHTML = "";
  const publicScreen = document.createElement("div");
  publicScreen.className = "screen-centered"; publicScreen.style.display = "none";
  const publicOutlet = document.createElement("div"); publicOutlet.id = "public-outlet"; publicOutlet.style.width="100%"; publicOutlet.style.maxWidth="28rem"; publicScreen.appendChild(publicOutlet);
  const shell = document.createElement("div"); shell.className="app-shell"; shell.style.display="none";
  shell.innerHTML = `
    <aside class="sidebar"><div class="sidebar-brand"><img src="assets/logo.svg" alt="NATRA"/><span class="sidebar-brand-name">NATRA</span></div><nav class="sidebar-nav" id="sidebar-nav"></nav><div class="sidebar-footer"><div style="display:flex;align-items:center;justify-content:space-between;gap:.5rem"><span id="current-user-label" style="color:var(--color-paper-ink-soft);font-size:var(--text-sm);white-space:nowrap;overflow:hidden;text-overflow:ellipsis"></span><button id="logout-btn" style="color:var(--color-navy-bright);font-size:var(--text-sm);font-weight:650">Log out</button></div><div style="margin-top:.45rem;color:#6f88a3;font-size:.68rem">NATRA Management</div></div></aside>
    <div class="main"><header class="header"><div class="header-actions" id="header-actions"></div></header><div class="content"><div class="content-inner" id="app-outlet"></div></div></div>`;
  appRoot.append(publicScreen,shell); mountToastStack(appRoot); renderNavItems(shell.querySelector("#sidebar-nav"));
  shell.querySelector("#logout-btn").addEventListener("click",()=>{setUser(null);navigate("/login")});
  return {publicScreen,publicOutlet,shell,appOutlet:shell.querySelector("#app-outlet")};
}

async function bootstrap(){
  const {publicScreen,publicOutlet,shell,appOutlet}=buildLayouts();
  registerRoutes({
    "/setup":{loader:()=>import("./auth/initialization.js"),public:true}, "/login":{loader:()=>import("./auth/login.js"),public:true},
    "/dashboard":{loader:()=>import("./pages/dashboard.js")}, "/pos":{loader:()=>import("./pages/pos.js")}, "/waiters":{loader:()=>import("./pages/waiters.js")}, "/items":{loader:()=>import("./pages/items.js")},
    "/raw-materials":{loader:()=>import("./pages/raw-materials.js")}, "/expenses":{loader:()=>import("./pages/expenses.js")}, "/reports":{loader:()=>import("./pages/reports.js")}, "/settings":{loader:()=>import("./pages/settings.js")}
  });
  initRouter((def)=>{if(def.public){shell.style.display="none";publicScreen.style.display="flex";return publicOutlet}publicScreen.style.display="none";shell.style.display="grid";return appOutlet},{onNavigate:({path})=>{setActiveNavItem(shell.querySelector("#sidebar-nav"),path);const userLabel=shell.querySelector("#current-user-label");const {user}=store.getState();if(userLabel)userLabel.textContent=user?user.full_name:"";}});
  store.subscribe(({settings})=>{if(settings)document.title=`${settings.restaurant_name} — NATRA`});
  let initialized=false;try{initialized=await api.auth.isInitialized()}catch(err){pushToast(typeof err==="string"?err:"Couldn't reach the local database.","error",0)}
  if(!initialized){navigate("/setup");return} navigate("/login");
}
export async function completeLogin(user){setUser(user);try{setSettings(await api.settings.get())}catch(err){pushToast(typeof err==="string"?err:"Couldn't load settings.","error")}navigate("/dashboard")}
bootstrap();
