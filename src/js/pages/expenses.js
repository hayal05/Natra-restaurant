/**
 * Other operating expenses (rent, utilities, salaries...) — the last
 * independent cost stream in the profit formula, see
 * src-tauri/src/services/financial_service.rs.
 */

import * as api from "../api.js";
import { store, pushToast, withErrorToast } from "../state.js";
import { setHeaderActions } from "../components/header.js";
import { createStatCard } from "../components/stat-card.js";
import { renderTable } from "../components/table.js";
import { openModal, closeModal } from "../components/modal.js";
import { formatMoney } from "../utils/currency.js";
import { formatDate, todayIso } from "../utils/dates.js";
import { firstError, isNonEmpty, isPositiveNumber } from "../utils/validation.js";

export const title = "Expenses";

export async function render(container) {
  container.innerHTML = `
    <div class="page-heading">
      <div>
        <h1>Expenses</h1>
        <p class="page-subtitle">Rent, utilities, salaries — anything that isn't a sale or a raw-material purchase.</p>
      </div>
    </div>
    <div class="grid grid-cols-4" id="stat-row"></div>
    <div class="card">
      <div class="card-header"><span class="card-title">All expenses</span></div>
      <div id="expenses-table"></div>
    </div>
  `;

  // The router moves this page's DOM out of `container` into the live outlet
  // once render() resolves, leaving `container` itself empty. Every element a
  // later callback (modal submit) needs must be captured here while
  // `container` still holds it - never re-queried from `container` afterwards.
  const els = {
    statRow: container.querySelector("#stat-row"),
    expensesTable: container.querySelector("#expenses-table"),
  };

  const addBtn = document.createElement("button");
  addBtn.className = "btn btn-primary";
  addBtn.textContent = "Add expense";
  addBtn.addEventListener("click", () => openExpenseModal(els));
  setHeaderActions(document, [addBtn]);

  await loadExpenses(els);
}

async function loadExpenses(els) {
  const currency = store.getState().settings?.currency ?? "USD";

  let expenses;
  try {
    expenses = await withErrorToast(() => api.expenses.list());
  } catch {
    return; // toast already shown
  }

  const total = expenses.reduce((sum, e) => sum + e.amount, 0);
  const byCategory = new Map();
  expenses.forEach((e) => byCategory.set(e.category, (byCategory.get(e.category) ?? 0) + e.amount));
  const topCategory = [...byCategory.entries()].sort((a, b) => b[1] - a[1])[0];

  const statRow = els.statRow;
  statRow.innerHTML = "";
  statRow.appendChild(createStatCard({ label: "Total expenses", value: formatMoney(total, currency), tone: "rust" }));
  statRow.appendChild(createStatCard({ label: "Entries logged", value: String(expenses.length) }));
  statRow.appendChild(
    createStatCard({
      label: "Largest category",
      value: topCategory ? topCategory[0] : "—",
      sublabel: topCategory ? formatMoney(topCategory[1], currency) : undefined,
    })
  );

  renderTable(els.expensesTable, {
    columns: [
      { key: "expense_date", label: "Date", format: (e) => formatDate(e.expense_date) },
      { key: "category", label: "Category" },
      { key: "description", label: "Description", format: (e) => e.description || "—" },
      { key: "amount", label: "Amount", numeric: true, format: (e) => formatMoney(e.amount, currency) },
    ],
    rows: expenses,
    emptyMessage: "No expenses logged yet.",
    getRowKey: (e) => e.id,
  });
}

function openExpenseModal(els) {
  const form = document.createElement("form");
  form.noValidate = true;
  form.innerHTML = `
    <div class="field">
      <label class="field-label" for="expense-category">Category</label>
      <input class="input" id="expense-category" name="category" type="text" placeholder="Utilities" autofocus />
      <span class="field-error" id="expense-category-error"></span>
    </div>
    <div class="field">
      <label class="field-label" for="expense-amount">Amount</label>
      <input class="input input-mono" id="expense-amount" name="amount" type="number" step="0.01" min="0" placeholder="0.00" />
      <span class="field-error" id="expense-amount-error"></span>
    </div>
    <div class="field">
      <label class="field-label" for="expense-date">Date</label>
      <input class="input" id="expense-date" name="expenseDate" type="date" value="${todayIso()}" />
    </div>
    <div class="field">
      <label class="field-label" for="expense-description">Description (optional)</label>
      <input class="input" id="expense-description" name="description" type="text" />
    </div>
  `;

  const cancelBtn = document.createElement("button");
  cancelBtn.className = "btn btn-secondary";
  cancelBtn.type = "button";
  cancelBtn.textContent = "Cancel";
  cancelBtn.addEventListener("click", closeModal);

  const saveBtn = document.createElement("button");
  saveBtn.className = "btn btn-primary";
  saveBtn.type = "button";
  saveBtn.textContent = "Add expense";
  saveBtn.addEventListener("click", () => form.requestSubmit());

  openModal({ title: "New expense", content: form, actions: [cancelBtn, saveBtn] });

  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    const category = form.category.value.trim();
    const amount = Number(form.amount.value);
    const description = form.description.value.trim();
    const expenseDate = form.expenseDate.value ? `${form.expenseDate.value} 00:00:00` : null;

    const error = firstError([
      [isNonEmpty(category), "Enter a category."],
      [isPositiveNumber(amount), "Enter an amount greater than zero."],
    ]);
    if (error) {
      const fieldId = error.includes("category") ? "expense-category" : "expense-amount";
      form.querySelector(`#${fieldId}`).classList.add("has-error");
      form.querySelector(`#${fieldId}-error`).textContent = error;
      return;
    }

    saveBtn.disabled = true;
    try {
      await withErrorToast(() => api.expenses.create(category, description || null, amount, expenseDate, null));
      pushToast("Expense added.", "success");
      closeModal();
      loadExpenses(els);
    } catch {
      saveBtn.disabled = false;
    }
  });
}
