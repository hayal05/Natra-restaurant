include!("ipc_command_names.rs");
fn main() {
    tauri_build::try_build(tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(APP_COMMANDS)))
        .expect("Tauri command permission generation failed");
}
