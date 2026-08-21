import { formatMoney } from "../utils/currency.js";
import { initials } from "../utils/formatting.js";

/**
 * @param {{
 *   waiter: { id: number, full_name: string, phone: string|null, is_active: boolean },
 *   receivable?: number,
 *   currency?: string,
 *   onSettle?: (waiterId: number) => void,
 *   onToggleActive?: (waiterId: number, isActive: boolean) => void,
 * }} opts
 */
export function createWaiterCard({ waiter, receivable, currency = "USD", onSettle, onToggleActive }) {
  const card = document.createElement("div");
  card.className = "entity-card";

  const avatar = document.createElement("div");
  avatar.className = "entity-avatar";
  avatar.textContent = initials(waiter.full_name);
  card.appendChild(avatar);

  const body = document.createElement("div");
  body.className = "entity-card-body";

  const name = document.createElement("div");
  name.className = "entity-card-name";
  name.textContent = waiter.full_name;
  body.appendChild(name);

  const meta = document.createElement("div");
  meta.className = "entity-card-meta";
  meta.textContent = waiter.phone || (waiter.is_active ? "Active" : "Inactive");
  body.appendChild(meta);

  card.appendChild(body);

  if (typeof receivable === "number") {
    const value = document.createElement("div");
    value.className = "entity-card-value";
    value.textContent = formatMoney(receivable, currency);
    if (receivable > 0) value.style.color = "var(--color-rust-dark)";
    card.appendChild(value);
  }

  if (onSettle && receivable > 0) {
    const settleBtn = document.createElement("button");
    settleBtn.className = "btn btn-secondary btn-sm";
    settleBtn.textContent = "Settle";
    settleBtn.addEventListener("click", () => onSettle(waiter.id));
    card.appendChild(settleBtn);
  }

  if (onToggleActive) {
    const toggleBtn = document.createElement("button");
    toggleBtn.className = "btn btn-ghost btn-sm";
    toggleBtn.textContent = waiter.is_active ? "Deactivate" : "Activate";
    toggleBtn.addEventListener("click", () => onToggleActive(waiter.id, !waiter.is_active));
    card.appendChild(toggleBtn);
  }

  return card;
}
