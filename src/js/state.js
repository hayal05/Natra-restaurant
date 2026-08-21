/**
 * Small pub/sub store. No framework — just enough reactivity for
 * plain-JS pages to re-render when the bits they care about change.
 */

function createStore(initialState) {
  let state = initialState;
  const listeners = new Set();

  return {
    getState() {
      return state;
    },
    setState(patch) {
      state = { ...state, ...(typeof patch === "function" ? patch(state) : patch) };
      listeners.forEach((fn) => fn(state));
    },
    subscribe(fn) {
      listeners.add(fn);
      return () => listeners.delete(fn);
    },
  };
}

export const store = createStore({
  /** Current logged-in user, or null. See src-tauri/src/models/user.rs. */
  user: null,
  /** App settings row (restaurant_name, currency, ...), loaded after login. */
  settings: null,
  ui: {
    sidebarCollapsed: false,
    toasts: [],
  },
});

export function setUser(user) {
  store.setState({ user });
}

export function setSettings(settings) {
  store.setState({ settings });
}

export function toggleSidebar() {
  const { ui } = store.getState();
  store.setState({ ui: { ...ui, sidebarCollapsed: !ui.sidebarCollapsed } });
}

let toastId = 0;

/**
 * Push a toast notification. `variant`: 'success' | 'error' | 'info'.
 * Auto-dismisses after `duration` ms (default 4s); pass 0 to persist
 * until the user or caller dismisses it explicitly.
 */
export function pushToast(message, variant = "info", duration = 4000) {
  const id = ++toastId;
  const { ui } = store.getState();
  store.setState({ ui: { ...ui, toasts: [...ui.toasts, { id, message, variant }] } });

  if (duration > 0) {
    setTimeout(() => dismissToast(id), duration);
  }
  return id;
}

export function dismissToast(id) {
  const { ui } = store.getState();
  store.setState({ ui: { ...ui, toasts: ui.toasts.filter((t) => t.id !== id) } });
}

/** Convenience: run an async action, toast on failure with the backend's message. */
export async function withErrorToast(action) {
  try {
    return await action();
  } catch (err) {
    pushToast(typeof err === "string" ? err : "Something went wrong.", "error");
    throw err;
  }
}
