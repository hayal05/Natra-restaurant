export function initials(fullName) {
  return (fullName || "")
    .trim()
    .split(/\s+/)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? "")
    .join("");
}

export function capitalize(text) {
  if (!text) return "";
  return text[0].toUpperCase() + text.slice(1);
}

/** e.g. pluralize(1, 'item') -> '1 item', pluralize(3, 'item') -> '3 items' */
export function pluralize(count, singular, plural) {
  const word = count === 1 ? singular : plural ?? `${singular}s`;
  return `${count} ${word}`;
}

/** 'ready_made' -> 'Ready made', 'cookable' -> 'Cookable' */
export function humanizeEnum(value) {
  if (!value) return "";
  return capitalize(value.replace(/_/g, " "));
}
