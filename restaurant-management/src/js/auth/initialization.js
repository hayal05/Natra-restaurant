/**
 * First-run screen. Shown when `is_initialized` is false (see app.js
 * bootstrap). Creates the one admin account the whole system runs on
 * (src-tauri/src/services/auth_service.rs enforces there's only ever
 * one init) and logs straight in on success.
 */

import * as api from "../api.js";
import { pushToast } from "../state.js";
import { firstError, isNonEmpty, minLength } from "../utils/validation.js";

export const title = "Set up";

export async function render(container) {
  const { completeLogin } = await import("../app.js");

  const card = document.createElement("div");
  card.className = "auth-card";
  card.innerHTML = `
    <div class="auth-card-brand">
      <img src="../assets/logo.svg" alt="" />
      <span class="sidebar-brand-name" style="color: var(--color-ink);">Restaurant Manager</span>
    </div>
    <h1 class="auth-card-heading">Create the admin account</h1>
    <p class="auth-card-subheading">This is the one account that runs the whole system. You can add staff logins later.</p>
    <form novalidate>
      <div class="field">
        <label class="field-label" for="full-name">Full name</label>
        <input class="input" id="full-name" name="fullName" type="text" placeholder="Alex Bekele" autocomplete="name" />
        <span class="field-error" id="full-name-error"></span>
      </div>
      <div class="field">
        <label class="field-label" for="username">Username</label>
        <input class="input" id="username" name="username" type="text" placeholder="alex" autocomplete="username" />
        <span class="field-error" id="username-error"></span>
      </div>
      <div class="field">
        <label class="field-label" for="password">Password</label>
        <input class="input" id="password" name="password" type="password" placeholder="At least 8 characters" autocomplete="new-password" />
        <span class="field-error" id="password-error"></span>
      </div>
      <div class="field">
        <label class="field-label" for="confirm-password">Confirm password</label>
        <input class="input" id="confirm-password" name="confirmPassword" type="password" autocomplete="new-password" />
        <span class="field-error" id="confirm-password-error"></span>
      </div>
      <button class="btn btn-primary" type="submit">Create account</button>
    </form>
  `;
  container.appendChild(card);

  const form = card.querySelector("form");
  const submitBtn = form.querySelector("button[type=submit]");

  clearErrors(form);

  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    clearErrors(form);

    const fullName = form.fullName.value.trim();
    const username = form.username.value.trim();
    const password = form.password.value;
    const confirmPassword = form.confirmPassword.value;

    const error = firstError([
      [isNonEmpty(fullName), "Enter a name."],
      [isNonEmpty(username), "Choose a username."],
      [minLength(password, 8), "Use at least 8 characters."],
      [password === confirmPassword, "Passwords don't match."],
    ]);

    if (error) {
      showFieldError(form, fieldForError(error), error);
      return;
    }

    submitBtn.disabled = true;
    submitBtn.textContent = "Creating…";
    try {
      const user = await api.auth.initializeAdmin(username, password, fullName);
      await completeLogin(user);
    } catch (err) {
      pushToast(typeof err === "string" ? err : "Couldn't create the admin account.", "error");
      submitBtn.disabled = false;
      submitBtn.textContent = "Create account";
    }
  });
}

function fieldForError(message) {
  if (message.includes("name")) return "full-name";
  if (message.includes("username")) return "username";
  if (message.includes("characters")) return "password";
  if (message.includes("match")) return "confirm-password";
  return "full-name";
}

function showFieldError(form, fieldId, message) {
  const input = form.querySelector(`#${fieldId}`);
  const errorEl = form.querySelector(`#${fieldId}-error`);
  if (input) input.classList.add("has-error");
  if (errorEl) errorEl.textContent = message;
}

function clearErrors(form) {
  form.querySelectorAll(".input").forEach((el) => el.classList.remove("has-error"));
  form.querySelectorAll(".field-error").forEach((el) => (el.textContent = ""));
}
