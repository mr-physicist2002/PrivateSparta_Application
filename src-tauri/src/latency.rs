//! Latency testing via the clash_api delay endpoint. Uses the running core
//! when connected; otherwise spawns an ephemeral core (no inbounds) with the
//! nodes under test as outbounds. Concurrency-limited and cancellable.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::clash::{self, ClashEndpoint};
use crate::commands::AppState;
use crate::config_gen::{self, GenInput};
use crate::core::supervisor;
use crate::error::AppError;
use crate::model::{LatencyResult, Node, TestProgress};

const CONCURRENCY: usize = 8;

#[derive(Default)]
pub struct LatencyState {
    /// Runtime-only results; never persisted.
    pub results: Mutex<HashMap<Uuid, u32>>,
    /// Bumping this cancels any in-flight run.
    generation: AtomicU64,
}

impl LatencyState {
    pub fn cancel(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    pub fn latency_of(&self, id: Uuid) -> Option<u32> {
        self.results
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&id)
            .copied()
    }
}

fn emit_progress(app: &AppHandle, running: bool, done: usize, total: usize) {
    let _ = app.emit("test-progress", TestProgress { running, done, total });
}

/// Test the given nodes. Runs in a spawned task; results stream to the UI as
/// `latency-result` events, progress as `test-progress`.
pub async fn run_tests(app: AppHandle, nodes: Vec<Node>) {
    let state = app.state::<AppState>();
    let generation = state.latency.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let total = nodes.len();
    if total == 0 {
        return;
    }
    emit_progress(&app, true, 0, total);

    // Prefer the connected core; otherwise bring up an ephemeral one.
    let (endpoint, mut ephemeral) = match state.supervisor.clash_endpoint() {
        Some(endpoint) => (endpoint, None),
        None => match spawn_ephemeral(&app, &nodes).await {
            Ok((endpoint, child)) => (endpoint, Some(child)),
            Err(err) => {
                tracing::warn!("latency test setup failed: {err}");
                emit_progress(&app, false, 0, total);
                return;
            }
        },
    };

    let client = clash::local_client();
    let semaphore = Arc::new(Semaphore::new(CONCURRENCY));
    let done = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::with_capacity(total);

    for node in nodes {
        let permit_source = Arc::clone(&semaphore);
        let client = client.clone();
        let endpoint = endpoint.clone();
        let app = app.clone();
        let done = Arc::clone(&done);
        handles.push(tauri::async_runtime::spawn(async move {
            let Ok(_permit) = permit_source.acquire().await else {
                return;
            };
            let state = app.state::<AppState>();
            if state.latency.generation.load(Ordering::SeqCst) != generation {
                return; // cancelled before this node started
            }
            let latency = clash::delay(&client, &endpoint, &node.tag()).await;
            if state.latency.generation.load(Ordering::SeqCst) != generation {
                return; // cancelled while probing
            }
            {
                let mut results = state
                    .latency
                    .results
                    .lock()
                    .unwrap_or_else(|p| p.into_inner());
                match latency {
                    Some(ms) => {
                        results.insert(node.id, ms);
                    }
                    None => {
                        results.remove(&node.id);
                    }
                }
            }
            let finished = done.fetch_add(1, Ordering::SeqCst) + 1;
            let _ = app.emit(
                "latency-result",
                LatencyResult {
                    node_id: node.id,
                    latency_ms: latency,
                },
            );
            emit_progress(&app, true, finished as usize, total);
        }));
    }

    for handle in handles {
        let _ = handle.await;
    }
    if let Some(child) = ephemeral.as_mut() {
        let _ = child.kill().await;
    }
    emit_progress(&app, false, done.load(Ordering::SeqCst) as usize, total);
}

async fn spawn_ephemeral(
    app: &AppHandle,
    nodes: &[Node],
) -> Result<(ClashEndpoint, tokio::process::Child), AppError> {
    let clash_port = supervisor::pick_free_port()?;
    let secret = supervisor::random_secret()?;
    // Any node can be "selected" for routing purposes; delay tests name their
    // outbound explicitly. Pick the first that generates.
    let selected_tag = nodes
        .iter()
        .find(|n| config_gen::outbound_for_node(n).is_ok())
        .map(|n| n.tag())
        .ok_or_else(|| AppError::Config("None of these servers can be tested.".into()))?;
    let config = config_gen::generate(&GenInput {
        nodes,
        selected_tag: &selected_tag,
        local_port: None,
        allow_lan: false,
        clash_port,
        clash_secret: &secret,
        log_level: "warn",
    })?;
    let dir = supervisor::data_dir(app)?;
    let path = dir.join("test-config.json");
    supervisor::write_and_check(&config, &path).await?;

    let ring = Arc::new(Mutex::new(std::collections::VecDeque::new()));
    let mut child = supervisor::spawn_core(&path, &ring)?;
    let state = app.state::<AppState>();
    state.supervisor.assign_to_job(&child);
    if !supervisor::wait_for_port(&mut child, clash_port).await {
        let _ = child.kill().await;
        return Err(AppError::Core("Couldn't start the test core.".into()));
    }
    Ok((
        ClashEndpoint {
            port: clash_port,
            secret,
        },
        child,
    ))
}
