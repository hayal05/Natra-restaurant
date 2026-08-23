/** Today's date as 'YYYY-MM-DD', in the local timezone. */
export function todayIso() {
  const d = new Date();
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** 'YYYY-MM-DD' or 'YYYY-MM-DD HH:MM:SS' -> a short human date, e.g. 'Aug 20, 2026'. */
export function formatDate(sqlDate) {
  if (!sqlDate) return "";
  const isoish = sqlDate.includes("T") ? sqlDate : sqlDate.replace(" ", "T");
  const d = new Date(isoish);
  if (Number.isNaN(d.getTime())) return sqlDate;
  return d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
}

/** Same as formatDate but includes the time, e.g. 'Aug 20, 2026, 3:45 PM'. */
export function formatDateTime(sqlDate) {
  if (!sqlDate) return "";
  const isoish = sqlDate.includes("T") ? sqlDate : sqlDate.replace(" ", "T");
  const d = new Date(isoish);
  if (Number.isNaN(d.getTime())) return sqlDate;
  return d.toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

/** 'YYYY-MM-DD' -> a compact axis label, e.g. 'Aug 20'. */
export function formatDateShort(sqlDate) {
  if (!sqlDate) return "";
  const isoish = sqlDate.includes("T") ? sqlDate : sqlDate.replace(" ", "T");
  const d = new Date(isoish);
  if (Number.isNaN(d.getTime())) return sqlDate;
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

const MONTH_NAMES = [
  "January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December",
];

export function monthName(month) {
  return MONTH_NAMES[month - 1] ?? "";
}

function pad(n) {
  return String(n).padStart(2, "0");
}

/**
 * Inclusive ['YYYY-MM-DD 00:00:00', 'YYYY-MM-DD 23:59:59'] range for a
 * calendar month — mirrors the Rust backend's report_service::month_range
 * so client-side requests (e.g. product performance for a chosen month)
 * line up exactly with what `monthly_report` computed.
 */
export function monthRange(year, month) {
  const start = `${year}-${pad(month)}-01`;
  const lastDay = new Date(year, month, 0).getDate(); // day 0 of next month = last day of this one
  const end = `${year}-${pad(month)}-${pad(lastDay)}`;
  return [`${start} 00:00:00`, `${end} 23:59:59`];
}

/**
 * Inclusive ['YYYY-MM-DD 00:00:00', 'YYYY-MM-DD 23:59:59'] range for two
 * 'YYYY-MM-DD' date-input values — same boundary convention as
 * `monthRange`, just for an arbitrary from/to pair (a single day when
 * `fromDate === toDate`).
 */
export function dateRange(fromDate, toDate) {
  return [`${fromDate} 00:00:00`, `${toDate} 23:59:59`];
}

/** 'YYYY-MM-DD' -> the same date `days` earlier, as 'YYYY-MM-DD', in local time. */
export function daysAgoIso(days, from = new Date()) {
  const d = new Date(from.getFullYear(), from.getMonth(), from.getDate() - days);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}
