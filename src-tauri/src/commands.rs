use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use uuid::Uuid;

use crate::core::supervisor::{can_connect, Supervisor};
use crate::error::AppError;
use crate::model::{ConnState, ConnectionEvent, Node, NodeView, ProxyMode};
use crate::parser;
use crate::store::ConfigStore;

pub struct AppState {
    pub store: Mutex<ConfigStore>,
    pub supervisor: Supervisor,
    pub pending_import: Mutex<Option<Vec<Node>>>,
}

fn lock<'a, T>(m: &'a Mutex<T>) -> std::sync::MutexGuard<'a, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    connection: ConnectionEvent,
    nodes: Vec<NodeView>,
    selected_node_id: Option<Uuid>,
    mode: ProxyMode,
    local_port: u16,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    nodes: Vec<NodeView>,
    skipped: usize,
}

#[tauri::command]
pub fn get_snapshot(state: State<'_, AppState>) -> AppSnapshot {
    let (conn_state, node_id) = state.supervisor.snapshot();
    let store = lock(&state.store);
    AppSnapshot {
        connection: ConnectionEvent::new(conn_state, node_id),
        nodes: store.config.manual_nodes.iter().map(NodeView::from_node).collect(),
        selected_node_id: store.config.last_selected,
        mode: store.config.settings.mode,
        local_port: store.config.settings.local_port,
    }
}

#[tauri::command]
pub fn preview_clipboard_import(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ImportPreview, AppError> {
    let text = app.clipboard().read_text().map_err(|_| AppError::Clipboard)?;
    let batch = parser::parse_text(&text);
    let views = batch.nodes.iter().map(NodeView::from_node).collect();
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
pub fn commit_clipboard_import(
    state: State<'_, AppState>,
) -> Result<Vec<NodeView>, AppError> {
    let pending = lock(&state.pending_import)
        .take()
        .ok_or_else(|| AppError::Parse("Nothing to import. Preview first.".into()))?;
    let mut store = lock(&state.store);
    for node in pending {
        let duplicate = store.config.manual_nodes.iter().any(|existing| {
            existing.server == node.server
                && existing.port == node.port
                && existing.params == node.params
                && existing.transport == node.transport
        });
        if !duplicate {
            store.config.manual_nodes.push(node);
        }
    }
    if store.config.last_selected.is_none() {
        store.config.last_selected = store.config.manual_nodes.first().map(|n| n.id);
    }
    store.save()?;
    Ok(store.config.manual_nodes.iter().map(NodeView::from_node).collect())
}

#[tauri::command]
pub fn select_node(state: State<'_, AppState>, node_id: Uuid) -> Result<(), AppError> {
    ensure_idle(&state)?;
    let mut store = lock(&state.store);
    if !store.config.manual_nodes.iter().any(|n| n.id == node_id) {
        return Err(AppError::Parse("That server no longer exists.".into()));
    }
    store.config.last_selected = Some(node_id);
    store.save()
}

#[tauri::command]
pub fn delete_node(state: State<'_, AppState>, node_id: Uuid) -> Result<(), AppError> {
    ensure_idle(&state)?;
    let mut store = lock(&state.store);
    store.config.manual_nodes.retain(|n| n.id != node_id);
    if store.config.last_selected == Some(node_id) {
        store.config.last_selected = None;
    }
    store.save()
}

#[tauri::command]
pub fn set_mode(state: State<'_, AppState>, mode: ProxyMode) -> Result<(), AppError> {
    ensure_idle(&state)?;
    let mut store = lock(&state.store);
    store.config.settings.mode = mode;
    store.save()
}

#[tauri::command]
pub fn connect(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    let (node, mode, local_port) = {
        let store = lock(&state.store);
        let selected = store.config.last_selected.ok_or(AppError::NoNodeSelected)?;
        let node = store
            .config
            .manual_nodes
            .iter()
            .find(|n| n.id == selected)
            .cloned()
            .ok_or(AppError::NoNodeSelected)?;
        (node, store.config.settings.mode, store.config.settings.local_port)
    };
    state.supervisor.connect(&app, node, mode, local_port)
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
