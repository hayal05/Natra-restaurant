export function isNonEmpty(value) {
  return typeof value === "string" && value.trim().length > 0;
}

export function isPositiveNumber(value) {
  return typeof value === "number" && Number.isFinite(value) && value > 0;
}

export function isNonNegativeNumber(value) {
  return typeof value === "number" && Number.isFinite(value) && value >= 0;
}

export function minLength(value, n) {
  return typeof value === "string" && value.trim().length >= n;
}

/**
 * Runs `checks` (an array of [condition, message]) in order and returns
 * the first failing message, or null if every check passes. Keeps form
 * validation blocks linear instead of nested if/else chains.
 */
export function firstError(checks) {
  for (const [ok, message] of checks) {
    if (!ok) return message;
  }
  return null;
}
