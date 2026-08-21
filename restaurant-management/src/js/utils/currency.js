/** Formats a number as currency using the settings-store currency code. */
export function formatMoney(amount, currency = "USD") {
  try {
    return new Intl.NumberFormat(undefined, { style: "currency", currency }).format(amount ?? 0);
  } catch {
    return (amount ?? 0).toFixed(2);
  }
}

/** Plain 2-decimal number, no currency symbol — for input fields, exports. */
export function formatNumber(amount) {
  return (amount ?? 0).toFixed(2);
}
