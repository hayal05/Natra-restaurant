/**
 * Mounts a fixed toast stack into `root` that stays in sync with
 * `store.getState().ui.toasts` (see state.js). Call once, at app
 * bootstrap — pages just call `pushToast(...)` from state.js and this
 * renders it.
 */

import { store, dismissToast } from "../state.js";

export function mountToastStack(root) {
  const stack = document.createElement("div");
  stack.className = "toast-stack";
  root.appendChild(stack);

  const render = ({ ui }) => {
    stack.innerHTML = "";
    ui.toasts.forEach((toast) => {
      const el = document.createElement("div");
      el.className = `toast toast-${toast.variant}`;

      const message = document.createElement("span");
      message.textContent = toast.message;
      el.appendChild(message);

      const dismiss = document.createElement("button");
      dismiss.className = "toast-dismiss";
      dismiss.setAttribute("aria-label", "Dismiss");
      dismiss.textContent = "\u00d7";
      dismiss.addEventListener("click", () => dismissToast(toast.id));
      el.appendChild(dismiss);

      stack.appendChild(el);
    });
  };

  render(store.getState());
  store.subscribe(render);

  return stack;
}
