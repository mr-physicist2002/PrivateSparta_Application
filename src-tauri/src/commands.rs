use std::sync::{Mutex, MutexGuard};

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_clipboard_manager::ClipboardExt;
use uuid::Uuid;

use crate::core::supervisor::{can_connect, ConnectRequest, Supervisor};
use crate::error::AppError;
use crate::latency::{self, LatencyState};
use crate::model::{
    mask_url, ConnState, ConnectionEvent, Node, NodeView, Subscription, SubscriptionView,
    UpdateInterval,
};
use crate::store::{ConfigStore, Settings};
use crate::subs;
use crate::uri_export::export_uri;

pub struct AppState {
    pub store: Mutex<ConfigStore>,
    pub supervisor: Supervisor,
    pub pending_import: Mutex<Option<Vec<Node>>>,
    pub latency: LatencyState,
    pub window_state: Mutex<crate::model::WindowState>,
    pub logs: std::sync::Arc<crate::logs::LogBuffer>,
}

impl AppState {
    pub fn lock_store(&self) -> MutexGuard<'_, ConfigStore> {
        self.store.lock().unwrap_or_else(|p| p.into_inner())
    }
}

fn lock<'a, T>(m: &'a Mutex<T>) -> MutexGuard<'a, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    connection: ConnectionEvent,
    manual_nodes: Vec<NodeView>,
    subscriptions: Vec<SubscriptionView>,
    selected_node_id: Option<Uuid>,
    settings: Settings,
    version: String,
    elevated: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    nodes: Vec<NodeView>,
    skipped: usize,
}

fn node_view(state: &AppState, store: &ConfigStore, node: &Node) -> NodeView {
    NodeView::from_node(
        node,
        state.latency.latency_of(node.id),
        store.config.is_favorite(node.id),
    )
}

fn sub_view(state: &AppState, store: &ConfigStore, sub: &Subscription) -> SubscriptionView {
    SubscriptionView {
        id: sub.id,
        name: sub.name.clone(),
        url_masked: mask_url(&sub.url),
        node_count: sub.nodes.len(),
        user_info: sub.user_info,
        auto_update: sub.auto_update,
        last_updated: sub.last_updated.map(|t| t.timestamp()),
        last_error: sub.last_error.clone(),
        nodes: sub.nodes.iter().map(|n| node_view(state, store, n)).collect(),
    }
}

#[tauri::command]
pub fn get_snapshot(app: AppHandle, state: State<'_, AppState>) -> AppSnapshot {
    let (conn_state, node_id) = state.supervisor.snapshot();
    let store = state.lock_store();
    AppSnapshot {
        connection: ConnectionEvent::new(conn_state, node_id),
        manual_nodes: store
            .config
            .manual_nodes
            .iter()
            .map(|n| node_view(&state, &store, n))
            .collect(),
        subscriptions: store
            .config
            .subscriptions
            .iter()
            .map(|s| sub_view(&state, &store, s))
            .collect(),
        selected_node_id: store.config.last_selected,
        settings: store.config.settings.clone(),
        version: app.package_info().version.to_string(),
        elevated: crate::elevation::is_elevated(),
    }
}

// ---------- import ----------

#[tauri::command]
pub fn preview_clipboard_import(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ImportPreview, AppError> {
    let text = app.clipboard().read_text().map_err(|_| AppError::Clipboard)?;
    let batch = crate::parser::parse_text(&text);
    let store = state.lock_store();
    let views = batch
        .nodes
        .iter()
        .map(|n| node_view(&state, &store, n))
        .collect();
    drop(store);
    *lock(&state.pending_import) = if batch.nodes.is_empty() {
        None
    } else {
        Some(batch.nodes)
    };
    Ok(ImportPreview {
        nodes: views,
        skipped: batch.skipped,
    })
}

#[tauri::command]
pub fn commit_clipboard_import(state: State<'_, AppState>) -> Result<(), AppError> {
    let pending = lock(&state.pending_import)
        .take()
        .ok_or_else(|| AppError::Parse("Nothing to import. Preview first.".into()))?;
    let mut store = state.lock_store();
    for node in pending {
        let duplicate = store
            .config
            .all_nodes()
            .any(|existing| existing.same_endpoint(&node));
        if !duplicate {
            store.config.manual_nodes.push(node);
        }
    }
    if store.config.last_selected.is_none() {
        store.config.last_selected = store.config.manual_nodes.first().map(|n| n.id);
    }
    store.save()
}

// ---------- subscriptions ----------

#[tauri::command]
pub async fn add_subscription(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    url: String,
) -> Result<(), AppError> {
    let trimmed = url.trim().to_string();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err(AppError::Parse("Subscription links start with http:// or https://.".into()));
    }
    let sub_id = {
        let mut store = state.lock_store();
        if store.config.subscriptions.iter().any(|s| s.url == trimmed) {
            return Err(AppError::Parse("That subscription is already added.".into()));
        }
        let sub = Subscription {
            id: Uuid::new_v4(),
            name: if name.trim().is_empty() {
                mask_url(&trimmed)
            } else {
                name.trim().to_string()
            },
            url: trimmed,
            nodes: Vec::new(),
            user_info: None,
            auto_update: UpdateInterval::Off,
            last_updated: None,
            last_error: None,
        };
        let id = sub.id;
        store.config.subscriptions.push(sub);
        store.save()?;
        id
    };
    // First fetch; failure keeps the subscription with last_error set.
    let result = subs::update_subscription(&app, sub_id).await;
    let mut store = state.lock_store();
    if store.config.last_selected.is_none() {
        store.config.last_selected = store
            .config
            .subscriptions
            .iter()
            .find(|s| s.id == sub_id)
            .and_then(|s| s.nodes.first())
            .map(|n| n.id);
        store.save()?;
    }
    drop(store);
    result.map(|_| ())
}

#[tauri::command]
pub async fn update_subscription(app: AppHandle, sub_id: Uuid) -> Result<usize, AppError> {
    subs::update_subscription(&app, sub_id).await
}

#[tauri::command]
pub fn delete_subscription(state: State<'_, AppState>, sub_id: Uuid) -> Result<(), AppError> {
    ensure_idle(&state)?;
    let mut store = state.lock_store();
    let removed_ids: Vec<Uuid> = store
        .config
        .subscriptions
        .iter()
        .filter(|s| s.id == sub_id)
        .flat_map(|s| s.nodes.iter().map(|n| n.id))
        .collect();
    store.config.subscriptions.retain(|s| s.id != sub_id);
    store.config.favorites.retain(|id| !removed_ids.contains(id));
    if store
        .config
        .last_selected
        .is_some_and(|id| removed_ids.contains(&id))
    {
        store.config.last_selected = None;
    }
    store.save()
}

#[tauri::command]
pub fn set_sub_auto_update(
    state: State<'_, AppState>,
    sub_id: Uuid,
    interval: UpdateInterval,
) -> Result<(), AppError> {
    let mut store = state.lock_store();
    let sub = store
        .config
        .subscriptions
        .iter_mut()
        .find(|s| s.id == sub_id)
        .ok_or_else(|| AppError::Config("That subscription no longer exists.".into()))?;
    sub.auto_update = interval;
    store.save()
}

/// Explicit reveal action from the Subscriptions screen (masked by default).
#[tauri::command]
pub fn reveal_subscription_url(
    state: State<'_, AppState>,
    sub_id: Uuid,
) -> Result<String, AppError> {
    let store = state.lock_store();
    store
        .config
        .subscriptions
        .iter()
        .find(|s| s.id == sub_id)
        .map(|s| s.url.clone())
        .ok_or_else(|| AppError::Config("That subscription no longer exists.".into()))
}

// ---------- nodes ----------

#[tauri::command]
pub fn select_node(state: State<'_, AppState>, node_id: Uuid) -> Result<(), AppError> {
    ensure_idle(&state)?;
    let mut store = state.lock_store();
    if store.config.find_node(node_id).is_none() {
        return Err(AppError::Parse("That server no longer exists.".into()));
    }
    store.config.last_selected = Some(node_id);
    store.save()
}

#[tauri::command]
pub fn delete_node(state: State<'_, AppState>, node_id: Uuid) -> Result<(), AppError> {
    ensure_idle(&state)?;
    let mut store = state.lock_store();
    store.config.manual_nodes.retain(|n| n.id != node_id);
    store.config.favorites.retain(|id| *id != node_id);
    if store.config.last_selected == Some(node_id) {
        store.config.last_selected = None;
    }
    store.save()
}

#[tauri::command]
pub fn toggle_favorite(state: State<'_, AppState>, node_id: Uuid) -> Result<bool, AppError> {
    let mut store = state.lock_store();
    let now_favorite = if store.config.favorites.contains(&node_id) {
        store.config.favorites.retain(|id| *id != node_id);
        false
    } else {
        store.config.favorites.push(node_id);
        true
    };
    store.save()?;
    Ok(now_favorite)
}

/// Rebuilds the share URI and puts it on the clipboard — in Rust, so the
/// credential-bearing link never crosses IPC.
#[tauri::command]
pub fn copy_node_link(
    app: AppHandle,
    state: State<'_, AppState>,
    node_id: Uuid,
) -> Result<(), AppError> {
    let uri = {
        let store = state.lock_store();
        let node = store
            .config
            .find_node(node_id)
            .ok_or_else(|| AppError::Parse("That server no longer exists.".into()))?;
        export_uri(node)
    };
    app.clipboard()
        .write_text(uri)
        .map_err(|_| AppError::Clipboard)
}

// ---------- latency ----------

#[tauri::command]
pub fn test_nodes(
    app: AppHandle,
    state: State<'_, AppState>,
    node_ids: Option<Vec<Uuid>>,
) -> Result<(), AppError> {
    let store = state.lock_store();
    let nodes: Vec<Node> = match &node_ids {
        Some(ids) => store
            .config
            .all_nodes()
            .filter(|n| ids.contains(&n.id))
            .cloned()
            .collect(),
        None => store.config.all_nodes().cloned().collect(),
    };
    drop(store);
    if nodes.is_empty() {
        return Err(AppError::Parse("No servers to test.".into()));
    }
    tauri::async_runtime::spawn(latency::run_tests(app, nodes));
    Ok(())
}

#[tauri::command]
pub fn cancel_test(state: State<'_, AppState>) {
    state.latency.cancel();
}

// ---------- settings ----------

#[tauri::command]
pub fn set_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<(), AppError> {
    let autostart_changed = {
        let mut store = state.lock_store();
        let changed = store.config.settings.autostart != settings.autostart;
        store.config.settings = settings.clone();
        store.save()?;
        changed
    };
    if autostart_changed {
        let manager = app.autolaunch();
        let result = if settings.autostart {
            manager.enable()
        } else {
            manager.disable()
        };
        if let Err(err) = result {
            return Err(AppError::Store(format!("couldn't change launch at login: {err}")));
        }
    }
    Ok(())
}

// ---------- logs / elevation ----------

#[tauri::command]
pub fn get_logs(state: State<'_, AppState>) -> Vec<crate::logs::LogLine> {
    state.logs.all()
}

#[tauri::command]
pub fn clear_logs(state: State<'_, AppState>) {
    state.logs.clear();
}

#[tauri::command]
pub fn copy_logs(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    app.clipboard()
        .write_text(state.logs.joined())
        .map_err(|_| AppError::Clipboard)
}

#[tauri::command]
pub fn relaunch_elevated(app: AppHandle) -> Result<(), AppError> {
    if crate::elevation::relaunch_elevated() {
        app.exit(0);
        Ok(())
    } else {
        Err(AppError::Core(
            "Couldn't restart with administrator rights.".into(),
        ))
    }
}

// ---------- updates ----------

#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
) -> Result<crate::updater::UpdateInfo, AppError> {
    crate::updater::check(&app).await
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), AppError> {
    crate::updater::install(&app).await
}

// ---------- connection ----------

#[tauri::command]
pub fn connect(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    let request = {
        let store = state.lock_store();
        let selected_id = store.config.last_selected.ok_or(AppError::NoNodeSelected)?;
        let selected = store
            .config
            .find_node(selected_id)
            .cloned()
            .ok_or(AppError::NoNodeSelected)?;
        let settings = &store.config.settings;
        if settings.mode == crate::model::ProxyMode::Tun && !crate::elevation::is_elevated() {
            return Err(AppError::Core(
                "TUN mode needs administrator rights. Restart as administrator, or switch modes.".into(),
            ));
        }
        let rules = if settings.rules_enabled {
            let resolved = crate::rules::resolve(&app);
            if resolved.is_none() {
                tracing::warn!("rule-sets missing; connecting without split routing");
            }
            resolved
        } else {
            None
        };
        ConnectRequest {
            nodes: store.config.all_nodes().cloned().collect(),
            selected,
            mode: settings.mode,
            local_port: settings.local_port,
            allow_lan: settings.allow_lan,
            log_level: settings.log_level.clone(),
            rules,
            ad_block: settings.ad_block,
        }
    };
    state.supervisor.connect(&app, request)
}

#[tauri::command]
pub fn disconnect(app: AppHandle, state: State<'_, AppState>) {
    state.supervisor.disconnect(&app);
}

fn ensure_idle(state: &State<'_, AppState>) -> Result<(), AppError> {
    let (conn_state, _) = state.supervisor.snapshot();
    if can_connect(conn_state) {
        Ok(())
    } else {
        Err(AppError::Busy)
    }
}

/// Shared by the tray: connect if idle, disconnect if up.
pub fn toggle_connection(app: &AppHandle) {
    let state = app.state::<AppState>();
    let (conn_state, _) = state.supervisor.snapshot();
    match conn_state {
        ConnState::Connected | ConnState::Connecting => {
            state.supervisor.disconnect(app);
        }
        ConnState::Disconnected | ConnState::Error => {
            if let Err(err) = connect(app.clone(), app.state::<AppState>()) {
                tracing::warn!("tray connect failed: {err}");
            }
        }
        ConnState::Disconnecting => {}
    }
}
