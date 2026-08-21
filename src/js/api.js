/**
 * Thin wrapper around every Tauri command exposed by src-tauri/src/lib.rs.
 *
 * Two casing rules apply, inherited straight from Tauri + serde defaults
 * used on the Rust side (no `rename_all` anywhere there):
 *   - top-level command argument names are camelCase (Tauri's default
 *     argument-name conversion), e.g. `full_name` -> `fullName`.
 *   - fields *inside* a struct argument (NewItem, CheckoutRequest, ...)
 *     keep the Rust field's own casing, which here is snake_case, e.g.
 *     `{ item_id, quantity }`.
 * Response payloads are plain `serde` output, so they come back
 * snake_case too (`full_name`, `total_amount`, ...) — the frontend uses
 * those keys as-is rather than translating them.
 *
 * Every command can reject; failures arrive as a plain string message
 * (see src-tauri/src/errors.rs) — callers can show it directly.
 */

function invoke(cmd, args) {
  if (!window.__TAURI__) {
    return Promise.reject("Tauri bridge is not available in this context.");
  }
  return window.__TAURI__.core.invoke(cmd, args);
}

export const auth = {
  isInitialized: () => invoke("is_initialized"),
  initializeAdmin: (username, password, fullName) =>
    invoke("initialize_admin", { username, password, fullName }),
  login: (username, password) => invoke("login", { username, password }),
};

export const waiters = {
  create: (fullName, phone) => invoke("create_waiter", { fullName, phone: phone ?? null }),
  list: (onlyActive) => invoke("list_waiters", { onlyActive }),
  setActive: (waiterId, isActive) => invoke("set_waiter_active", { waiterId, isActive }),
  listReceivables: () => invoke("list_waiter_receivables"),
  settle: (waiterId) => invoke("settle_waiter", { waiterId }),
};

export const items = {
  createCategory: (name, description) => invoke("create_category", { name, description: description ?? null }),
  listCategories: () => invoke("list_categories"),
  /** newItem: { category_id, name, item_type: 'ready_made'|'cookable', purchase_cost, selling_price } */
  create: (newItem) => invoke("create_item", { newItem }),
  /** filter: { only_active, item_type, category_id } */
  list: (filter) => invoke("list_items", { filter }),
  setActive: (itemId, isActive) => invoke("set_item_active", { itemId, isActive }),
  updatePricing: (itemId, sellingPrice, purchaseCost) =>
    invoke("update_item_pricing", { itemId, sellingPrice, purchaseCost: purchaseCost ?? null }),
};

export const rawMaterials = {
  create: (name, unit) => invoke("create_raw_material", { name, unit }),
  list: (onlyActive) => invoke("list_raw_materials", { onlyActive }),
  recordPurchase: (rawMaterialId, quantity, unitCost, supplier, note) =>
    invoke("record_purchase", {
      rawMaterialId,
      quantity,
      unitCost,
      supplier: supplier ?? null,
      note: note ?? null,
    }),
  listPurchases: (rawMaterialId) => invoke("list_purchases", { rawMaterialId: rawMaterialId ?? null }),
  totalCost: (from, to) => invoke("raw_material_total_cost", { from: from ?? null, to: to ?? null }),
};

export const expenses = {
  create: (category, description, amount, expenseDate, note) =>
    invoke("create_expense", {
      category,
      description: description ?? null,
      amount,
      expenseDate: expenseDate ?? null,
      note: note ?? null,
    }),
  list: (from, to) => invoke("list_expenses", { from: from ?? null, to: to ?? null }),
  calculateProfit: (from, to) => invoke("calculate_profit", { from: from ?? null, to: to ?? null }),
};

export const pos = {
  /** req: { waiter_id, user_id, payment_method: 'cash'|'card'|'other', lines: [{item_id, quantity}], note } */
  checkout: (req) => invoke("checkout", { req }),
  listSales: (waiterId, limit) => invoke("list_sales", { waiterId: waiterId ?? null, limit }),
};

export const reports = {
  monthly: (year, month) => invoke("monthly_report", { year, month }),
  productPerformance: (from, to) => invoke("product_performance", { from: from ?? null, to: to ?? null }),
  salesMix: (from, to) => invoke("sales_mix", { from: from ?? null, to: to ?? null }),
  cashFlowByYear: (year) => invoke("cash_flow_by_year", { year }),
};

export const dashboard = {
  summary: () => invoke("dashboard_summary"),
};

export const settings = {
  get: () => invoke("get_settings"),
  updateGeneral: (restaurantName, currency) =>
    invoke("update_general_settings", { restaurantName, currency }),
  enableSync: (tursoUrl, tursoAuthToken) => invoke("enable_sync", { tursoUrl, tursoAuthToken }),
  disableSync: () => invoke("disable_sync"),
  syncNow: () => invoke("sync_now"),
  syncQueueStatus: () => invoke("sync_queue_status"),
};

export const backups = {
  /** { enabled, backup_dir, last_backup_at, last_backup_path, last_backup_error } */
  status: () => invoke("backup_status"),
  /** Writes a fresh .xlsx immediately, ignoring the daily cadence. */
  now: () => invoke("backup_now"),
  setEnabled: (enabled) => invoke("set_backup_enabled", { enabled }),
};
