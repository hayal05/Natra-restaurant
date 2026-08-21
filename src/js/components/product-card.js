import { formatMoney } from "../utils/currency.js";
import { humanizeEnum } from "../utils/formatting.js";

/**
 * @param {{
 *   item: { id: number, name: string, type: 'ready_made'|'cookable', purchase_cost: number|null, selling_price: number, is_active: boolean },
 *   currency?: string,
 *   mode?: 'catalog'|'pos',
 *   onSelect?: (itemId: number) => void,   // pos mode: add to cart
 *   onEdit?: (itemId: number) => void,     // catalog mode
 *   onToggleActive?: (itemId: number, isActive: boolean) => void,
 * }} opts
 */
export function createProductCard({ item, currency = "USD", mode = "catalog", onSelect, onEdit, onToggleActive }) {
  const card = document.createElement("div");
  card.className = "entity-card";
  if (mode === "pos") {
    card.style.cursor = "pointer";
    card.addEventListener("click", () => onSelect && onSelect(item.id));
  }

  const avatar = document.createElement("div");
  avatar.className = "entity-avatar";
  avatar.textContent = item.name[0]?.toUpperCase() ?? "?";
  card.appendChild(avatar);

  const body = document.createElement("div");
  body.className = "entity-card-body";

  const name = document.createElement("div");
  name.className = "entity-card-name";
  name.textContent = item.name;
  body.appendChild(name);

  const meta = document.createElement("div");
  meta.className = "entity-card-meta";
  meta.textContent = humanizeEnum(item.type);
  body.appendChild(meta);

  card.appendChild(body);

  const price = document.createElement("div");
  price.className = "entity-card-value";
  price.textContent = formatMoney(item.selling_price, currency);
  card.appendChild(price);

  if (mode === "catalog") {
    if (onEdit) {
      const editBtn = document.createElement("button");
      editBtn.className = "btn btn-secondary btn-sm";
      editBtn.textContent = "Edit";
      editBtn.addEventListener("click", () => onEdit(item.id));
      card.appendChild(editBtn);
    }
    if (onToggleActive) {
      const toggleBtn = document.createElement("button");
      toggleBtn.className = "btn btn-ghost btn-sm";
      toggleBtn.textContent = item.is_active ? "Deactivate" : "Activate";
      toggleBtn.addEventListener("click", () => onToggleActive(item.id, !item.is_active));
      card.appendChild(toggleBtn);
    }
  }

  return card;
}
