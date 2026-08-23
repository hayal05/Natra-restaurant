function invoke(cmd, args) {
  if (!window.__TAURI__) return Promise.reject("Tauri bridge is not available in this context.");
  return window.__TAURI__.core.invoke(cmd, args);
}

export const auth = {
  isInitialized: () => invoke("is_initialized"),
  initializeAdmin: (username, password, fullName) => invoke("initialize_admin", { username, password, fullName }),
  login: (username, password) => invoke("login", { username, password }),
};

export const waiters = {
  create: (fullName, phone, profilePhoto) => invoke("create_waiter", { fullName, phone: phone ?? null, profilePhoto: profilePhoto ?? null }),
  list: (onlyActive) => invoke("list_waiters", { onlyActive }),
  setActive: (waiterId, isActive) => invoke("set_waiter_active", { waiterId, isActive }),
  listReceivables: () => invoke("list_waiter_receivables"),
  settle: (waiterId) => invoke("settle_waiter", { waiterId }),
};

export const items = {
  createCategory: (name, description) => invoke("create_category", { name, description: description ?? null }),
  listCategories: () => invoke("list_categories"),
  create: (newItem) => invoke("create_item", { newItem }),
  list: (filter) => invoke("list_items", { filter }),
  setActive: (itemId, isActive) => invoke("set_item_active", { itemId, isActive }),
  updatePricing: (itemId, sellingPrice, purchaseCost) => invoke("update_item_pricing", { itemId, sellingPrice, purchaseCost: purchaseCost ?? null }),
};

export const rawMaterials = {
  create: (name, unit) => invoke("create_raw_material", { name, unit }),
  list: (onlyActive) => invoke("list_raw_materials", { onlyActive }),
  recordPurchase: (rawMaterialId, quantity, unitCost, supplier, note) => invoke("record_purchase", { rawMaterialId, quantity, unitCost, supplier: supplier ?? null, note: note ?? null }),
  listPurchases: (rawMaterialId) => invoke("list_purchases", { rawMaterialId: rawMaterialId ?? null }),
  totalCost: (from, to) => invoke("raw_material_total_cost", { from: from ?? null, to: to ?? null }),
};

export const expenses = {
  create: (category, description, amount, expenseDate, note) => invoke("create_expense", { category, description: description ?? null, amount, expenseDate: expenseDate ?? null, note: note ?? null }),
  list: (from, to) => invoke("list_expenses", { from: from ?? null, to: to ?? null }),
  calculateProfit: (from, to) => invoke("calculate_profit", { from: from ?? null, to: to ?? null }),
};

export const pos = {
  checkout: (req) => invoke("checkout", { req }),
  listSales: (waiterId, limit) => invoke("list_sales", { waiterId: waiterId ?? null, limit }),
  reverseSale: (saleId) => invoke("reverse_sale", { saleId }),
};

export const reports = {
  monthly: (year, month) => invoke("monthly_report", { year, month }),
  productPerformance: (from, to) => invoke("product_performance", { from: from ?? null, to: to ?? null }),
  salesMix: (from, to) => invoke("sales_mix", { from: from ?? null, to: to ?? null }),
  cashFlowByYear: (year) => invoke("cash_flow_by_year", { year }),
};

export const dashboard = { summary: () => invoke("dashboard_summary") };
export const settings = {
  get: () => invoke("get_settings"),
  updateGeneral: (restaurantName, currency) => invoke("update_general_settings", { restaurantName, currency }),
  enableSync: (tursoUrl, tursoAuthToken) => invoke("enable_sync", { tursoUrl, tursoAuthToken }),
  disableSync: () => invoke("disable_sync"),
  syncNow: () => invoke("sync_now"),
  syncQueueStatus: () => invoke("sync_queue_status"),
};
export const backups = {
  status: () => invoke("backup_status"),
  now: () => invoke("backup_now"),
  setEnabled: (enabled) => invoke("set_backup_enabled", { enabled }),
};
