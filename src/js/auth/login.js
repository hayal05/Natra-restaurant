/**
 * Credential login. Shown whenever there's no user in the store —
 * every app launch starts here (the backend issues no session token,
 * see src-tauri/src/services/auth_service.rs, so there's nothing to
 * silently restore).
 */

import * as api from "../api.js";
import { pushToast } from "../state.js";
import { firstError, isNonEmpty } from "../utils/validation.js";

export const title = "Log in";

export async function render(container) {
  const { completeLogin } = await import("../app.js");

  const card = document.createElement("div");
  card.className = "auth-card";
  card.innerHTML = `
    <div class="auth-card-brand">
      <img src="../assets/logo.svg" alt="" />
      <span class="sidebar-brand-name" style="color: var(--color-ink);">Restaurant Manager</span>
    </div>
    <h1 class="auth-card-heading">Log in</h1>
    <p class="auth-card-subheading">Use your restaurant account to continue.</p>
    <form novalidate>
      <div class="field">
        <label class="field-label" for="username">Username</label>
        <input class="input" id="username" name="username" type="text" autocomplete="username" autofocus />
        <span class="field-error" id="username-error"></span>
      </div>
      <div class="field">
        <label class="field-label" for="password">Password</label>
        <input class="input" id="password" name="password" type="password" autocomplete="current-password" />
        <span class="field-error" id="password-error"></span>
      </div>
      <button class="btn btn-primary" type="submit">Log in</button>
    </form>
  `;
  container.appendChild(card);

  const form = card.querySelector("form");
  const submitBtn = form.querySelector("button[type=submit]");

  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    clearErrors(form);

    const username = form.username.value.trim();
    const password = form.password.value;

    const error = firstError([
      [isNonEmpty(username), "Enter your username."],
      [isNonEmpty(password), "Enter your password."],
    ]);

    if (error) {
      const fieldId = error.includes("username") ? "username" : "password";
      const input = form.querySelector(`#${fieldId}`);
      const errorEl = form.querySelector(`#${fieldId}-error`);
      if (input) input.classList.add("has-error");
      if (errorEl) errorEl.textContent = error;
      return;
    }

    submitBtn.disabled = true;
    submitBtn.textContent = "Logging in…";
    try {
      const user = await api.auth.login(username, password);
      await completeLogin(user);
    } catch (err) {
      pushToast(typeof err === "string" ? err : "Couldn't log in.", "error");
      submitBtn.disabled = false;
      submitBtn.textContent = "Log in";
    }
  });
}

function clearErrors(form) {
  form.querySelectorAll(".input").forEach((el) => el.classList.remove("has-error"));
  form.querySelectorAll(".field-error").forEach((el) => (el.textContent = ""));
}
