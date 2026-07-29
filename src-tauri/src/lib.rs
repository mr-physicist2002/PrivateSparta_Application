mod commands;
mod config_gen;
mod core;
mod error;
mod model;
mod parser;
mod store;
mod sysproxy;
mod tray;

use std::sync::Mutex;

use tauri::{Manager, RunEvent, WindowEvent};

use commands::AppState;
use core::supervisor::Supervisor;
use store::ConfigStore;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;

            // A leftover proxy backup means the last session died while
            // connected: put the user's proxy back before anything else.
            sysproxy::restore_if_dirty(&data_dir);

            let store = ConfigStore::load(data_dir.join("config.json"));

            #[cfg(windows)]
            let supervisor = Supervisor::new(core::job_object::JobObject::new()?);
            #[cfg(not(windows))]
            let supervisor = Supervisor::new();

            app.manage(AppState {
                store: Mutex::new(store),
                supervisor,
                pending_import: Mutex::new(None),
            });

            tray::setup(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window minimizes to tray; quitting is explicit.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::preview_clipboard_import,
            commands::commit_clipboard_import,
            commands::select_node,
            commands::delete_node,
            commands::set_mode,
            commands::connect,
            commands::disconnect,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build the app");

    app.run(|app_handle, event| {
        if let RunEvent::Exit = event {
            // Signal teardown and restore the proxy synchronously. The job
            // object kills the core even if we die before this runs.
            let state = app_handle.state::<AppState>();
            state.supervisor.disconnect(app_handle);
            if let Ok(dir) = app_handle.path().app_data_dir() {
                if let Err(err) = sysproxy::restore(&sysproxy::backup_path(&dir)) {
                    tracing::warn!("proxy restore on exit failed: {err}");
                }
            }
        }
    });
}
