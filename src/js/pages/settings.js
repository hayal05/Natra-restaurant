/**
 * Settings: restaurant name/currency, and optional Turso sync. Sync
 * credentials are write-only from the frontend's perspective — the
 * backend never sends turso_url/turso_auth_token back out (see
 * #[serde(skip_serializing)] on Settings) — so this page only ever
 * shows whether sync is on, never the stored credentials themselves.
 */

import * as api from "../api.js";
import { setSettings, pushToast, withErrorToast } from "../state.js";
import { clearHeaderActions } from "../components/header.js";
import { firstError, isNonEmpty } from "../utils/validation.js";
import { formatDateTime } from "../utils/dates.js";

export const title = "Settings";

export async function render(container) {
  clearHeaderActions(document);

  container.innerHTML = `
    <div class="page-heading">
      <div>
        <h1>Settings</h1>
        <p class="page-subtitle">General preferences and optional cloud sync.</p>
      </div>
    </div>
    <div class="card" style="max-width: 32rem;">
      <div class="card-header"><span class="card-title">General</span></div>
      <form id="general-form" novalidate>
        <div class="field">
          <label class="field-label" for="restaurant-name">Company name</label>
          <input class="input" id="restaurant-name" type="text" />
          <span class="field-hint">Shown in the middle of the top bar on every page.</span>
          <span class="field-error" id="restaurant-name-error"></span>
        </div>
        <div class="field">
          <label class="field-label" for="currency">Currency code</label>
          <input class="input input-mono" id="currency" type="text" placeholder="USD" maxlength="3" style="text-transform: uppercase;" />
          <span class="field-hint">3-letter ISO code, e.g. USD, EUR, ETB.</span>
          <span class="field-error" id="currency-error"></span>
        </div>
        <div class="form-actions">
          <button class="btn btn-primary" type="submit">Save changes</button>
        </div>
      </form>
    </div>
    <div class="card" style="max-width: 32rem;">
      <div class="card-header">
        <span class="card-title">Sync</span>
        <span class="badge" id="sync-badge"></span>
      </div>
      <p style="font-size: var(--text-sm); color: var(--color-ink-soft); margin-bottom: var(--space-4);">
        The app works fully offline on its local database. Turning on sync pushes
        changes to a Turso database so you can access the same data elsewhere.
      </p>
      <div id="sync-section"></div>
    </div>
    <div class="card" style="max-width: 32rem;">
      <div class="card-header">
        <span class="card-title">Backups</span>
        <span class="badge" id="backup-badge"></span>
      </div>
      <p style="font-size: var(--text-sm); color: var(--color-ink-soft); margin-bottom: var(--space-4);">
        Once a day the app writes a full Excel (.xlsx) copy of your sales, items,
        expenses, and raw materials to disk — no cloud account needed. The last
        30 days are kept.
      </p>
      <div id="backup-section"></div>
    </div>
  `;

  let settings;
  try {
    settings = await withErrorToast(() => api.settings.get());
  } catch {
    return; // toast already shown
  }

  // The router moves this page's DOM out of `container` into the live outlet
  // once render() resolves, leaving `container` itself empty. Every element a
  // later callback (form submit, sync toggle, backup action) needs must be
  // captured here while `container` still holds it - never re-queried from
  // `container` afterwards.
  const els = {
    nameInput: container.querySelector("#restaurant-name"),
    currencyInput: container.querySelector("#currency"),
    nameError: container.querySelector("#restaurant-name-error"),
    currencyError: container.querySelector("#currency-error"),
    generalForm: container.querySelector("#general-form"),
    syncBadge: container.querySelector("#sync-badge"),
    syncSection: container.querySelector("#sync-section"),
    backupBadge: container.querySelector("#backup-badge"),
    backupSection: container.querySelector("#backup-section"),
  };

  els.nameInput.value = settings.restaurant_name;
  els.currencyInput.value = settings.currency;

  els.generalForm.addEventListener("submit", async (e) => {
    e.preventDefault();
    els.nameInput.classList.remove("has-error");
    els.currencyInput.classList.remove("has-error");
    els.nameError.textContent = "";
    els.currencyError.textContent = "";

    const restaurantName = els.nameInput.value.trim();
    const currency = els.currencyInput.value.trim().toUpperCase();

    const error = firstError([
      [isNonEmpty(restaurantName), "Enter a company name."],
      [isNonEmpty(currency), "Enter a currency code."],
    ]);
    if (error) {
      const el = error.includes("currency") ? els.currencyInput : els.nameInput;
      const errEl = error.includes("currency") ? els.currencyError : els.nameError;
      el.classList.add("has-error");
      errEl.textContent = error;
      return;
    }

    const submitBtn = e.target.querySelector("button[type=submit]");
    submitBtn.disabled = true;
    try {
      const updated = await withErrorToast(() => api.settings.updateGeneral(restaurantName, currency));
      setSettings(updated);
      pushToast("Settings saved.", "success");
    } catch {
      /* toast already shown */
    } finally {
      submitBtn.disabled = false;
    }
  });

  renderSyncSection(els, settings);
  renderBackupSection(els);
}

function renderSyncSection(els, settings) {
  const badge = els.syncBadge;
  badge.className = "badge " + (settings.sync_enabled ? "badge-sage" : "badge-neutral");
  badge.textContent = settings.sync_enabled ? "Enabled" : "Disabled";

  const section = els.syncSection;

  if (settings.sync_enabled) {
    section.innerHTML = `<button class="btn btn-danger" id="disable-sync-btn">Disable sync</button>`;
    section.querySelector("#disable-sync-btn").addEventListener("click", async (e) => {
      e.target.disabled = true;
      try {
        const updated = await withErrorToast(() => api.settings.disableSync());
        setSettings(updated);
        pushToast("Sync disabled.", "success");
        renderSyncSection(els, updated);
      } catch {
        e.target.disabled = false;
      }
    });
    return;
  }

  section.innerHTML = `
    <form id="sync-form" novalidate>
      <div class="field">
        <label class="field-label" for="turso-url">Turso database URL</label>
        <input class="input input-mono" id="turso-url" type="text" placeholder="libsql://your-db.turso.io" />
        <span class="field-error" id="turso-url-error"></span>
      </div>
      <div class="field">
        <label class="field-label" for="turso-token">Auth token</label>
        <input class="input input-mono" id="turso-token" type="password" />
        <span class="field-error" id="turso-token-error"></span>
      </div>
      <div class="form-actions">
        <button class="btn btn-primary" type="submit">Enable sync</button>
      </div>
    </form>
  `;

  section.querySelector("#sync-form").addEventListener("submit", async (e) => {
    e.preventDefault();
    const urlInput = section.querySelector("#turso-url");
    const tokenInput = section.querySelector("#turso-token");
    urlInput.classList.remove("has-error");
    tokenInput.classList.remove("has-error");

    const tursoUrl = urlInput.value.trim();
    const tursoAuthToken = tokenInput.value.trim();

    const error = firstError([
      [isNonEmpty(tursoUrl), "Enter your Turso database URL."],
      [isNonEmpty(tursoAuthToken), "Enter your Turso auth token."],
    ]);
    if (error) {
      const el = error.includes("URL") ? urlInput : tokenInput;
      const errEl = error.includes("URL") ? section.querySelector("#turso-url-error") : section.querySelector("#turso-token-error");
      el.classList.add("has-error");
      errEl.textContent = error;
      return;
    }

    const submitBtn = e.target.querySelector("button[type=submit]");
    submitBtn.disabled = true;
    try {
      const updated = await withErrorToast(() => api.settings.enableSync(tursoUrl, tursoAuthToken));
      setSettings(updated);
      pushToast("Sync enabled.", "success");
      renderSyncSection(els, updated);
    } catch {
      submitBtn.disabled = false;
    }
  });
}

async function renderBackupSection(els) {
  const badge = els.backupBadge;
  const section = els.backupSection;

  let status;
  try {
    status = await withErrorToast(() => api.backups.status());
  } catch {
    return; // toast already shown
  }

  paintBackupSection(badge, section, status);
}

function paintBackupSection(badge, section, status) {
  badge.className = "badge " + (status.enabled ? "badge-sage" : "badge-neutral");
  badge.textContent = status.enabled ? "Enabled" : "Disabled";

  const lastRun = status.last_backup_at
    ? `Last backup: ${formatDateTime(status.last_backup_at)}`
    : "No backup has run yet.";
  const lastError = status.last_backup_error
    ? `<p class="field-error" style="margin-top: var(--space-2);">Last attempt failed: ${status.last_backup_error}</p>`
    : "";

  section.innerHTML = `
    <p style="font-size: var(--text-sm); margin-bottom: var(--space-1);">${lastRun}</p>
    <p style="font-size: var(--text-xs); color: var(--color-ink-soft); margin-bottom: var(--space-4);">
      Saved to: <span class="input-mono">${status.backup_dir}</span>
    </p>
    ${lastError}
    <div class="form-actions" style="gap: var(--space-2);">
      <button class="btn btn-primary" id="backup-now-btn">Backup now</button>
      <button class="btn ${status.enabled ? "btn-danger" : "btn-secondary"}" id="toggle-backup-btn">
        ${status.enabled ? "Disable backups" : "Enable backups"}
      </button>
    </div>
  `;

  section.querySelector("#backup-now-btn").addEventListener("click", async (e) => {
    e.target.disabled = true;
    try {
      const report = await withErrorToast(() => api.backups.now());
      if (report.error) {
        pushToast(`Backup failed: ${report.error}`, "error");
      } else if (report.ran) {
        pushToast("Backup complete.", "success");
      } else {
        pushToast("Backups are disabled.", "error");
      }
      const refreshed = await api.backups.status();
      paintBackupSection(badge, section, refreshed);
    } catch {
      /* toast already shown */
    } finally {
      e.target.disabled = false;
    }
  });

  section.querySelector("#toggle-backup-btn").addEventListener("click", async (e) => {
    e.target.disabled = true;
    try {
      await withErrorToast(() => api.backups.setEnabled(!status.enabled));
      pushToast(status.enabled ? "Backups disabled." : "Backups enabled.", "success");
      const refreshed = await api.backups.status();
      paintBackupSection(badge, section, refreshed);
    } catch {
      e.target.disabled = false;
    }
  });
}
