mod clash;
mod commands;
mod config_gen;
mod core;
mod elevation;
mod error;
mod latency;
#[cfg(test)]
mod live_test;
mod logs;
mod model;
mod parser;
mod rules;
mod store;
mod subs;
mod sysproxy;
mod tray;
mod updater;
mod uri_export;

use std::sync::Mutex;

use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;

use commands::AppState;
use core::supervisor::Supervisor;
use latency::LatencyState;
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
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;

            // A leftover proxy backup means the last session died while
            // connected: put the user's proxy back before anything else.
            sysproxy::restore_if_dirty(&data_dir);

            let store = ConfigStore::load(data_dir.join("config.json"));
            let window_state = store.config.window;
            let start_minimized = store.config.settings.start_minimized;
            let auto_connect =
                store.config.settings.auto_connect && store.config.last_selected.is_some();

            #[cfg(windows)]
            let supervisor = Supervisor::new(core::job_object::JobObject::new()?);
            #[cfg(not(windows))]
            let supervisor = Supervisor::new();

            let ruleset_auto_update = {
                // Read before the store moves into managed state.
                let s = &store.config.settings;
                s.ruleset_auto_update
            };

            app.manage(AppState {
                store: Mutex::new(store),
                supervisor,
                pending_import: Mutex::new(None),
                latency: LatencyState::default(),
                window_state: Mutex::new(window_state),
                logs: std::sync::Arc::new(logs::LogBuffer::default()),
            });

            logs::start_flusher(app.handle().clone());

            if ruleset_auto_update {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    // Daily, plus once shortly after launch.
                    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(86_400));
                    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                    loop {
                        ticker.tick().await;
                        let via_proxy = {
                            let state = handle.state::<AppState>();
                            let (conn, _) = state.supervisor.snapshot();
                            if conn == model::ConnState::Connected {
                                Some(state.lock_store().config.settings.local_port)
                            } else {
                                None
                            }
                        };
                        rules::refresh(&handle, via_proxy).await;
                    }
                });
            }

            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_size(tauri::PhysicalSize::new(
                    window_state.width.max(840),
                    window_state.height.max(560),
                ));
                if let (Some(x), Some(y)) = (window_state.x, window_state.y) {
                    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
                }
                if start_minimized {
                    let _ = window.hide();
                }
            }

            tray::setup(app.handle())?;
            subs::start_scheduler(app.handle().clone());

            if auto_connect {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    // Give the WebView a moment so state events aren't lost.
                    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                    let state = handle.state::<AppState>();
                    if let Err(err) = commands::connect(handle.clone(), state) {
                        tracing::warn!("auto-connect failed: {err}");
                    }
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Closing the window minimizes to tray; quitting is explicit.
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    let state = window.app_handle().state::<AppState>();
                    let mut ws = state
                        .window_state
                        .lock()
                        .unwrap_or_else(|p| p.into_inner());
                    ws.width = size.width;
                    ws.height = size.height;
                }
            }
            WindowEvent::Moved(position) => {
                let state = window.app_handle().state::<AppState>();
                let mut ws = state
                    .window_state
                    .lock()
                    .unwrap_or_else(|p| p.into_inner());
                ws.x = Some(position.x);
                ws.y = Some(position.y);
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::preview_clipboard_import,
            commands::commit_clipboard_import,
            commands::add_subscription,
            commands::update_subscription,
            commands::delete_subscription,
            commands::set_sub_auto_update,
            commands::reveal_subscription_url,
            commands::select_node,
            commands::delete_node,
            commands::toggle_favorite,
            commands::copy_node_link,
            commands::test_nodes,
            commands::cancel_test,
            commands::set_settings,
            commands::connect,
            commands::disconnect,
            commands::get_logs,
            commands::clear_logs,
            commands::copy_logs,
            commands::relaunch_elevated,
            commands::check_for_update,
            commands::install_update,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build the app");

    app.run(|app_handle, event| {
        if let RunEvent::Exit = event {
            let state = app_handle.state::<AppState>();
            // Persist window geometry gathered during the session.
            {
                let ws = *state
                    .window_state
                    .lock()
                    .unwrap_or_else(|p| p.into_inner());
                let mut store = state.lock_store();
                store.config.window = ws;
                if let Err(err) = store.save() {
                    tracing::warn!("couldn't save window state: {err}");
                }
            }
            // Signal teardown and restore the proxy synchronously. The job
            // object kills the core even if we die before this runs.
            state.supervisor.disconnect(app_handle);
            if let Ok(dir) = app_handle.path().app_data_dir() {
                if let Err(err) = sysproxy::restore(&sysproxy::backup_path(&dir)) {
                    tracing::warn!("proxy restore on exit failed: {err}");
                }
            }
        }
    });
}
