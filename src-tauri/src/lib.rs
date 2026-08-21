//! NATRA Restaurant Management — Tauri backend entry point.
//!
//! Responsibilities at startup:
//! 1. Resolve where the local SQLite file lives (per-OS app data dir).
//! 2. Open it and run any pending migrations (see `database::migrations`).
//! 3. Register the connection as managed Tauri state so every command
//!    handler in `commands/` can access it.

pub mod backup;
pub mod commands;
pub mod database;
pub mod errors;
pub mod models;
pub mod services;
pub mod sync;

use database::Db;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve app data directory");

            std::fs::create_dir_all(&app_data_dir)
                .expect("failed to create app data directory");

            let db_path = app_data_dir.join("natra.db");

            let db = Db::init(&db_path).expect("failed to initialize local database");

            app.manage(db);

            // Sync is a no-op every tick until Settings > sync_enabled
            // is turned on (see sync::manager::run_once), so it's safe
            // to always start the loop rather than gating it here.
            sync::start_background(app.handle().clone());

            // Same pattern: a no-op every tick until Settings >
            // backup_enabled is turned on (it's on by default).
            backup::start_background(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::auth::is_initialized,
            commands::auth::initialize_admin,
            commands::auth::login,
            commands::waiters::create_waiter,
            commands::waiters::list_waiters,
            commands::waiters::set_waiter_active,
            commands::waiters::list_waiter_receivables,
            commands::waiters::settle_waiter,
            commands::items::create_category,
            commands::items::list_categories,
            commands::items::create_item,
            commands::items::list_items,
            commands::items::set_item_active,
            commands::items::update_item_pricing,
            commands::raw_materials::create_raw_material,
            commands::raw_materials::list_raw_materials,
            commands::raw_materials::record_purchase,
            commands::raw_materials::list_purchases,
            commands::raw_materials::raw_material_total_cost,
            commands::expenses::create_expense,
            commands::expenses::list_expenses,
            commands::expenses::calculate_profit,
            commands::pos::checkout,
            commands::pos::list_sales,
            commands::reports::monthly_report,
            commands::reports::product_performance,
            commands::reports::sales_mix,
            commands::reports::cash_flow_by_year,
            commands::dashboard::dashboard_summary,
            commands::settings::get_settings,
            commands::settings::update_general_settings,
            commands::settings::enable_sync,
            commands::settings::disable_sync,
            commands::settings::sync_now,
            commands::settings::sync_queue_status,
            commands::backup::backup_status,
            commands::backup::backup_now,
            commands::backup::set_backup_enabled,
        ])
        .run(tauri::generate_context!())
        .expect("error while running NATRA");
}
