# NATRA — Restaurant Management

A local-first restaurant management system for managing waiters, POS sales, items,
raw materials, expenses, and financial reports. Built with **Tauri** (Rust backend,
vanilla JS/HTML/CSS frontend) and a local SQLite database, with optional **Turso**
cloud synchronization.

## Status
🚧 In active development — built step by step. See commit history / issues for progress.

## Core Concepts
- **Waiters** are registered and selected during POS sales.
- **Items** are either:
  - **Ready-Made** — tracked with a registered purchase cost and selling price.
  - **Cookable** — tracked with a selling price only; costs come from Raw Materials.
- Every POS sale calculates quantities, sales value, and waiter receivables automatically.
- **Raw material purchases** are recorded as costs independently of sales.
- **Other expenses** are recorded separately.
- **Profit = Sales − Ready-Made Costs − Raw-Material Costs − Other Expenses**
- The **Dashboard** summarizes sales, costs, profit, waiter receivables, product
  performance, sales mix, and cash flow.
- **Reports** provide monthly revenue, expenses, costs, and cash-flow data.
- The system works fully **offline** using a local database, with optional Turso
  sync when enabled.
- First launch initializes the system (create administrator account), then
  credential-based login for all subsequent use.

## Tech Stack
- **Frontend:** Vanilla JS, HTML, CSS (no framework)
- **Backend:** Rust via Tauri commands
- **Database:** SQLite (local-first), optional Turso sync
- **CI/CD:** GitHub Actions (Windows build + release)

## Project Structure
See repository layout in the project docs / initial issue for the full folder structure.

> **Asset locations, and why there are two:** `tauri.conf.json` sets
> `frontendDist: "../src"`, so only files under `src/` ship inside the
> running app — `public/` is never bundled into the webview. The split:
> - `public/assets/icons/` — native OS icon files (`.ico`, taskbar/installer
>   PNGs) read directly by the Tauri CLI at *build time* via `bundle.icon`
>   in `tauri.conf.json`. Never referenced from JS/HTML.
> - `src/assets/` — anything the UI itself loads at runtime (the logo used
>   on the login/setup screens, the favicon). Referenced with relative
>   paths like `../assets/logo.svg`.
>
> If you regenerate the app icon, update `public/assets/icons/`. If you
> change the in-app logo, update `src/assets/` (and re-copy to
> `public/assets/icons/` too if the OS icon should match).

## Getting Started
```bash
npm install
npm run dev
```

## License
MIT
