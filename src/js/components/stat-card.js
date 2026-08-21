/**
 * The app's signature component: a stat card styled like a torn
 * receipt (see .ticket-card in components.css). Used for dashboard
 * summary numbers, report totals, POS running totals — anywhere a
 * single headline figure needs to read at a glance.
 *
 * @param {{ label: string, value: string, sublabel?: string, tone?: 'sage'|'rust' }} opts
 * @returns {HTMLElement}
 */
export function createStatCard({ label, value, sublabel, tone }) {
  const card = document.createElement("div");
  card.className = "ticket-card";

  const labelEl = document.createElement("div");
  labelEl.className = "ticket-card-label";
  labelEl.textContent = label;
  card.appendChild(labelEl);

  const valueEl = document.createElement("div");
  valueEl.className = "ticket-card-value" + (tone ? ` tone-${tone}` : "");
  valueEl.textContent = value;
  card.appendChild(valueEl);

  if (sublabel) {
    const subEl = document.createElement("div");
    subEl.className = "ticket-card-sublabel";
    subEl.textContent = sublabel;
    card.appendChild(subEl);
  }

  return card;
}
