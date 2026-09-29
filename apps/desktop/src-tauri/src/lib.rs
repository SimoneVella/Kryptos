//! Kryptos app shell, shared by desktop (macOS/Windows/Linux) and mobile (Android/iOS).

#[cfg(desktop)]
mod bridge;
mod browser;
mod commands;
#[cfg(target_os = "android")]
mod android;
mod settings;
mod state;
mod sync;
mod watchdog;

#[cfg(not(target_os = "android"))]
use tauri::Emitter;
use tauri::Manager;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(sync::Staged::default())
        .setup(|app| {
            // Android shares the state (and its watchdog) with the autofill side.
            #[cfg(target_os = "android")]
            {
                android::set_app(app.handle().clone());
                app.manage(android::shared_state(app.path().app_data_dir()?));
            }
            #[cfg(not(target_os = "android"))]
            {
                #[cfg(mobile)]
                kryptos_core::paths::set_data_dir(app.path().app_data_dir()?);
                let state = AppState::new(kryptos_core::paths::vault_path(), settings::Settings::path());
                let handle = app.handle().clone();
                watchdog::start(state.clone(), move |reason| {
                    let _ = handle.emit("vault-locked", reason);
                });
                app.manage(state);
            }
            #[cfg(desktop)]
            {
                bridge::start(app.handle().clone());
                // Keep browser manifests pointing at this copy of the app (it may have moved).
                if app.state::<AppState>().settings().browser_integration {
                    let _ = browser::register();
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Minimizing to the dock is not "closing"; but losing the window entirely locks.
            if let tauri::WindowEvent::Destroyed = event {
                window.state::<AppState>().lock();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::status,
            commands::create_vault,
            commands::unlock,
            commands::lock,
            commands::touch,
            commands::list_entries,
            commands::get_entry,
            commands::add_entry,
            commands::update_entry,
            commands::delete_entry,
            commands::copy_field,
            commands::copy_text,
            commands::security_report,
            commands::generate_password,
            commands::change_master_password,
            commands::get_settings,
            commands::update_settings,
            commands::import_csv,
            commands::export_backup,
            commands::connect_browser,
            commands::disconnect_browser,
            commands::autofill_status,
            commands::open_autofill_settings,
            commands::biometric_status,
            commands::biometric_enable,
            commands::biometric_disable,
            commands::biometric_unlock,
            sync::sync_stage_file,
            sync::sync_stage_bytes,
            sync::sync_merge,
            sync::sync_cancel,
            sync::sync_qr_frames,
            sync::share_backup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Kryptos");
}
