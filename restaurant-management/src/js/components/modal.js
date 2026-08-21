/**
 * Imperative modal. Usage:
 *
 *   const close = openModal({
 *     title: "New waiter",
 *     content: formEl,
 *     actions: [cancelBtn, saveBtn],
 *   });
 *   // later, e.g. after a successful save:
 *   close();
 *
 * Only one modal is shown at a time — opening a new one closes the last.
 */

let activeBackdrop = null;

export function openModal({ title, content, actions = [] }) {
  closeModal();

  const backdrop = document.createElement("div");
  backdrop.className = "modal-backdrop";

  const modal = document.createElement("div");
  modal.className = "modal";
  modal.setAttribute("role", "dialog");
  modal.setAttribute("aria-modal", "true");

  const header = document.createElement("div");
  header.className = "modal-header";

  const titleEl = document.createElement("h3");
  titleEl.textContent = title || "";
  header.appendChild(titleEl);

  const closeBtn = document.createElement("button");
  closeBtn.className = "modal-close";
  closeBtn.setAttribute("aria-label", "Close");
  closeBtn.textContent = "\u00d7";
  closeBtn.addEventListener("click", closeModal);
  header.appendChild(closeBtn);

  modal.appendChild(header);

  const body = document.createElement("div");
  body.className = "modal-body";
  if (content) body.appendChild(content);
  modal.appendChild(body);

  if (actions.length) {
    const actionsRow = document.createElement("div");
    actionsRow.className = "modal-actions";
    actions.forEach((node) => actionsRow.appendChild(node));
    modal.appendChild(actionsRow);
  }

  backdrop.appendChild(modal);
  backdrop.addEventListener("click", (e) => {
    if (e.target === backdrop) closeModal();
  });

  const onKeydown = (e) => {
    if (e.key === "Escape") closeModal();
  };
  document.addEventListener("keydown", onKeydown);
  backdrop._onKeydown = onKeydown;

  document.body.appendChild(backdrop);
  activeBackdrop = backdrop;

  return closeModal;
}

export function closeModal() {
  if (!activeBackdrop) return;
  document.removeEventListener("keydown", activeBackdrop._onKeydown);
  activeBackdrop.remove();
  activeBackdrop = null;
}
