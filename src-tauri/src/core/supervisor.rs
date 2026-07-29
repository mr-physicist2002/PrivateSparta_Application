//! Owns the sing-box lifecycle: spawn, health check, crash restarts (max 3,
//! exponential backoff), teardown, and the 1 Hz traffic pump while connected.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use futures_util::StreamExt;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::clash::{self, ClashEndpoint};
use crate::config_gen::{self, GenInput};
use crate::error::{redact, AppError};
use crate::model::{ConnState, ConnectionEvent, Node, ProxyMode, TrafficEvent};
use crate::sysproxy;

#[cfg(windows)]
use super::job_object::JobObject;

const MAX_RESTARTS: u32 = 3;
const HEALTH_TIMEOUT: Duration = Duration::from_secs(6);
const HEALTH_POLL: Duration = Duration::from_millis(150);
#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn can_connect(state: ConnState) -> bool {
    matches!(state, ConnState::Disconnected | ConnState::Error)
}

pub fn can_disconnect(state: ConnState) -> bool {
    matches!(state, ConnState::Connected | ConnState::Connecting)
}

pub struct ConnectRequest {
    pub nodes: Vec<Node>,
    pub selected: Node,
    pub mode: ProxyMode,
    pub local_port: u16,
    pub allow_lan: bool,
    pub log_level: String,
    pub rules: Option<crate::rules::RulePaths>,
    pub ad_block: bool,
}

pub struct Supervisor {
    inner: Arc<Mutex<Inner>>,
    #[cfg(windows)]
    job: Arc<JobObject>,
}

struct Inner {
    state: ConnState,
    node_id: Option<Uuid>,
    /// Bumped on every connect; a finished task only mutates state if its
    /// generation is still current, so a stale task can't clobber a new run.
    generation: u64,
    kill_tx: Option<oneshot::Sender<()>>,
    /// clash_api endpoint of the RUNNING core; used by the latency tester.
    run_info: Option<ClashEndpoint>,
}

fn lock(inner: &Mutex<Inner>) -> MutexGuard<'_, Inner> {
    inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn emit(app: &AppHandle, event: ConnectionEvent) {
    if let Err(err) = app.emit("connection-state", &event) {
        tracing::warn!("couldn't emit connection-state: {err}");
    }
}

impl Supervisor {
    #[cfg(windows)]
    pub fn new(job: JobObject) -> Self {
        Supervisor {
            inner: Arc::new(Mutex::new(Inner {
                state: ConnState::Disconnected,
                node_id: None,
                generation: 0,
                kill_tx: None,
                run_info: None,
            })),
            job: Arc::new(job),
        }
    }

    #[cfg(not(windows))]
    pub fn new() -> Self {
        Supervisor {
            inner: Arc::new(Mutex::new(Inner {
                state: ConnState::Disconnected,
                node_id: None,
                generation: 0,
                kill_tx: None,
                run_info: None,
            })),
        }
    }

    pub fn snapshot(&self) -> (ConnState, Option<Uuid>) {
        let inner = lock(&self.inner);
        (inner.state, inner.node_id)
    }

    /// clash endpoint of the running core, if connected.
    pub fn clash_endpoint(&self) -> Option<ClashEndpoint> {
        lock(&self.inner).run_info.clone()
    }

    #[cfg(windows)]
    pub fn assign_to_job(&self, child: &Child) {
        if let Some(handle) = child.raw_handle() {
            if let Err(err) = self.job.assign(handle as _) {
                tracing::warn!("couldn't assign process to job object: {err}");
            }
        }
    }

    #[cfg(not(windows))]
    pub fn assign_to_job(&self, _child: &Child) {}

    pub fn connect(&self, app: &AppHandle, request: ConnectRequest) -> Result<(), AppError> {
        let (kill_tx, kill_rx) = oneshot::channel();
        let generation;
        {
            let mut inner = lock(&self.inner);
            if !can_connect(inner.state) {
                return Err(AppError::Busy);
            }
            inner.generation += 1;
            generation = inner.generation;
            inner.state = ConnState::Connecting;
            inner.node_id = Some(request.selected.id);
            inner.kill_tx = Some(kill_tx);
            inner.run_info = None;
        }
        emit(
            app,
            ConnectionEvent::new(ConnState::Connecting, Some(request.selected.id)),
        );

        let inner = Arc::clone(&self.inner);
        #[cfg(windows)]
        let job = Arc::clone(&self.job);
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            run_connection(RunCtx {
                app,
                inner,
                #[cfg(windows)]
                job,
                generation,
                request,
                kill_rx,
            })
            .await;
        });
        Ok(())
    }

    /// Signal the running task to tear down. Returns immediately; the task
    /// emits `disconnecting` → `disconnected` as it unwinds.
    pub fn disconnect(&self, app: &AppHandle) {
        let (kill_tx, node_id) = {
            let mut inner = lock(&self.inner);
            if !can_disconnect(inner.state) {
                return;
            }
            inner.state = ConnState::Disconnecting;
            inner.run_info = None;
            (inner.kill_tx.take(), inner.node_id)
        };
        emit(app, ConnectionEvent::new(ConnState::Disconnecting, node_id));
        if let Some(tx) = kill_tx {
            let _ = tx.send(());
        }
    }
}

struct RunCtx {
    app: AppHandle,
    inner: Arc<Mutex<Inner>>,
    #[cfg(windows)]
    job: Arc<JobObject>,
    generation: u64,
    request: ConnectRequest,
    kill_rx: oneshot::Receiver<()>,
}

enum Health {
    Up,
    Down,
    Killed,
}

async fn run_connection(ctx: RunCtx) {
    let RunCtx {
        app,
        inner,
        #[cfg(windows)]
        job,
        generation,
        request,
        mut kill_rx,
    } = ctx;
    let node_id = request.selected.id;

    let prep = match prepare(&app, &request).await {
        Ok(prep) => prep,
        Err(err) => {
            finish_error(&app, &inner, generation, node_id, err.to_string());
            return;
        }
    };

    let stderr_ring: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
    let totals = Arc::new(TrafficTotals::default());
    let session_start = tokio::time::Instant::now();
    let mut proxy_set = false;
    let mut attempt: u32 = 0;

    loop {
        let logs = {
            use tauri::Manager as _;
            Arc::clone(&app.state::<crate::commands::AppState>().logs)
        };
        let mut child = match spawn_core(&prep.config_path, &stderr_ring, logs) {
            Ok(child) => child,
            Err(err) => {
                teardown_proxy(&app, proxy_set);
                finish_error(&app, &inner, generation, node_id, err.to_string());
                return;
            }
        };
        #[cfg(windows)]
        if let Some(handle) = child.raw_handle() {
            if let Err(err) = job.assign(handle as _) {
                tracing::warn!("couldn't assign core to job object: {err}");
            }
        }

        match health_check(&mut child, prep.endpoint.port, &mut kill_rx).await {
            Health::Killed => {
                let _ = child.kill().await;
                finish_disconnect(&app, &inner, generation, node_id, proxy_set);
                return;
            }
            Health::Down => {
                let _ = child.kill().await;
                if attempt < MAX_RESTARTS {
                    attempt += 1;
                    tokio::time::sleep(backoff(attempt)).await;
                    continue;
                }
                teardown_proxy(&app, proxy_set);
                let detail = last_core_error(&stderr_ring);
                finish_error(
                    &app,
                    &inner,
                    generation,
                    node_id,
                    format!("Couldn't start the tunnel.{detail} Check the server details or pick another server."),
                );
                return;
            }
            Health::Up => {}
        }

        if request.mode == ProxyMode::SystemProxy && !proxy_set {
            match enable_proxy(&app, request.local_port) {
                Ok(()) => proxy_set = true,
                Err(err) => {
                    let _ = child.kill().await;
                    finish_error(&app, &inner, generation, node_id, err.to_string());
                    return;
                }
            }
        }

        let still_current = {
            let mut guard = lock(&inner);
            if guard.generation == generation {
                guard.state = ConnState::Connected;
                guard.run_info = Some(prep.endpoint.clone());
                true
            } else {
                false
            }
        };
        if !still_current {
            let _ = child.kill().await;
            return;
        }
        emit(&app, ConnectionEvent::new(ConnState::Connected, Some(node_id)));

        let stats_task = tauri::async_runtime::spawn(stats_pump(
            app.clone(),
            prep.endpoint.clone(),
            Arc::clone(&totals),
            session_start,
        ));

        tokio::select! {
            _ = &mut kill_rx => {
                stats_task.abort();
                let _ = child.kill().await;
                finish_disconnect(&app, &inner, generation, node_id, proxy_set);
                return;
            }
            status = child.wait() => {
                stats_task.abort();
                let code = status.ok().and_then(|s| s.code());
                tracing::warn!("core exited unexpectedly (code {code:?})");
                if attempt < MAX_RESTARTS {
                    attempt += 1;
                    let still_current = {
                        let mut guard = lock(&inner);
                        if guard.generation == generation {
                            guard.state = ConnState::Connecting;
                            guard.run_info = None;
                            true
                        } else {
                            false
                        }
                    };
                    if !still_current {
                        return;
                    }
                    emit(&app, ConnectionEvent::new(ConnState::Connecting, Some(node_id)));
                    tokio::time::sleep(backoff(attempt)).await;
                    continue;
                }
                teardown_proxy(&app, proxy_set);
                let detail = last_core_error(&stderr_ring);
                finish_error(
                    &app,
                    &inner,
                    generation,
                    node_id,
                    format!("The tunnel keeps stopping.{detail} Try another server."),
                );
                return;
            }
        }
    }
}

#[derive(Default)]
struct TrafficTotals {
    up: AtomicU64,
    down: AtomicU64,
}

/// Reads the clash /traffic chunked stream (one JSON object per second) and
/// re-emits it as a `traffic` event — the 1 Hz push the UI listens to.
async fn stats_pump(
    app: AppHandle,
    endpoint: ClashEndpoint,
    totals: Arc<TrafficTotals>,
    session_start: tokio::time::Instant,
) {
    let client = clash::local_client();
    let url = format!("http://127.0.0.1:{}/traffic", endpoint.port);
    let response = match client.get(&url).bearer_auth(&endpoint.secret).send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => {
            tracing::warn!("traffic stream rejected: {}", r.status());
            return;
        }
        Err(err) => {
            tracing::warn!("traffic stream failed: {err}");
            return;
        }
    };
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else { break };
        buffer.extend_from_slice(&chunk);
        while let Some(pos) = buffer.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = buffer.drain(..=pos).collect();
            if let Ok(sample) = serde_json::from_slice::<clash::TrafficSample>(&line) {
                let up_total = totals.up.fetch_add(sample.up, Ordering::Relaxed) + sample.up;
                let down_total =
                    totals.down.fetch_add(sample.down, Ordering::Relaxed) + sample.down;
                let event = TrafficEvent {
                    up_bps: sample.up,
                    down_bps: sample.down,
                    up_total,
                    down_total,
                    seconds: session_start.elapsed().as_secs(),
                };
                if let Err(err) = app.emit("traffic", event) {
                    tracing::warn!("couldn't emit traffic: {err}");
                }
            }
        }
    }
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(500u64.saturating_mul(1 << attempt.min(4)))
}

struct Prepared {
    config_path: PathBuf,
    endpoint: ClashEndpoint,
}

async fn prepare(app: &AppHandle, request: &ConnectRequest) -> Result<Prepared, AppError> {
    let data_dir = data_dir(app)?;
    let clash_port = pick_free_port()?;
    let secret = random_secret()?;
    let selected_tag = request.selected.tag();
    let config = config_gen::generate(&GenInput {
        nodes: &request.nodes,
        selected_tag: &selected_tag,
        local_port: Some(request.local_port),
        allow_lan: request.allow_lan,
        clash_port,
        clash_secret: &secret,
        log_level: &request.log_level,
        mode: request.mode,
        rules: request.rules.as_ref(),
        ad_block: request.ad_block,
        full_dns: true,
    })?;
    let config_path = data_dir.join("run-config.json");
    write_and_check(&config, &config_path).await?;
    Ok(Prepared {
        config_path,
        endpoint: ClashEndpoint {
            port: clash_port,
            secret,
        },
    })
}

/// Write a generated config and validate it with `sing-box check`.
pub async fn write_and_check(
    config: &serde_json::Value,
    path: &PathBuf,
) -> Result<(), AppError> {
    let json = serde_json::to_vec_pretty(config).map_err(|e| AppError::Config(e.to_string()))?;
    std::fs::write(path, json).map_err(|e| AppError::Config(e.to_string()))?;
    let bin = sidecar_path()?;
    let mut cmd = Command::new(&bin);
    cmd.arg("check").arg("-c").arg(path);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let output = cmd
        .output()
        .await
        .map_err(|e| AppError::Core(format!("Couldn't run the tunnel core: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let line = stderr.lines().last().unwrap_or("unknown error");
        return Err(AppError::Config(format!(
            "The core rejected this server's config: {}",
            redact(line)
        )));
    }
    Ok(())
}

pub fn spawn_core(
    config_path: &PathBuf,
    stderr_ring: &Arc<Mutex<VecDeque<String>>>,
    logs: Arc<crate::logs::LogBuffer>,
) -> Result<Child, AppError> {
    let bin = sidecar_path()?;
    let mut cmd = Command::new(&bin);
    cmd.arg("run")
        .arg("-c")
        .arg(config_path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let mut child = cmd
        .spawn()
        .map_err(|e| AppError::Core(format!("Couldn't start the tunnel core: {e}")))?;

    if let Some(stdout) = child.stdout.take() {
        let logs = Arc::clone(&logs);
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let clean = redact(&line);
                tracing::info!(target: "core", "{clean}");
                logs.push(clean);
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let ring = Arc::clone(stderr_ring);
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let clean = redact(&line);
                tracing::warn!(target: "core", "{clean}");
                logs.push(clean.clone());
                let mut ring = ring.lock().unwrap_or_else(|p| p.into_inner());
                if ring.len() >= 12 {
                    ring.pop_front();
                }
                ring.push_back(clean);
            }
        });
    }
    Ok(child)
}

/// Wait for the clash_api port to accept connections, watching for early
/// process death and user cancellation the whole time.
async fn health_check(
    child: &mut Child,
    clash_port: u16,
    kill_rx: &mut oneshot::Receiver<()>,
) -> Health {
    let deadline = tokio::time::Instant::now() + HEALTH_TIMEOUT;
    loop {
        if let Ok(Some(_)) = child.try_wait() {
            return Health::Down;
        }
        tokio::select! {
            _ = &mut *kill_rx => return Health::Killed,
            connected = TcpStream::connect(("127.0.0.1", clash_port)) => {
                if connected.is_ok() {
                    return Health::Up;
                }
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Health::Down;
        }
        tokio::time::sleep(HEALTH_POLL).await;
    }
}

/// Wait (outside the supervisor) for a spawned core's clash port. Used by the
/// latency tester's ephemeral instance.
pub async fn wait_for_port(child: &mut Child, clash_port: u16) -> bool {
    let deadline = tokio::time::Instant::now() + HEALTH_TIMEOUT;
    loop {
        if let Ok(Some(_)) = child.try_wait() {
            return false;
        }
        if TcpStream::connect(("127.0.0.1", clash_port)).await.is_ok() {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(HEALTH_POLL).await;
    }
}

fn last_core_error(ring: &Arc<Mutex<VecDeque<String>>>) -> String {
    let ring = ring.lock().unwrap_or_else(|p| p.into_inner());
    match ring
        .iter()
        .rev()
        .find(|l| l.contains("ERROR") || l.contains("FATAL"))
    {
        Some(line) => format!(" ({})", line.trim()),
        None => String::new(),
    }
}

fn set_state_if_current(inner: &Mutex<Inner>, generation: u64, state: ConnState) -> bool {
    let mut inner = lock(inner);
    if inner.generation != generation {
        return false;
    }
    inner.state = state;
    inner.run_info = None;
    true
}

fn finish_disconnect(
    app: &AppHandle,
    inner: &Mutex<Inner>,
    generation: u64,
    node_id: Uuid,
    proxy_set: bool,
) {
    teardown_proxy(app, proxy_set);
    if set_state_if_current(inner, generation, ConnState::Disconnected) {
        lock(inner).kill_tx = None;
        emit(app, ConnectionEvent::new(ConnState::Disconnected, Some(node_id)));
    }
}

fn finish_error(
    app: &AppHandle,
    inner: &Mutex<Inner>,
    generation: u64,
    node_id: Uuid,
    message: String,
) {
    let message = redact(&message);
    if set_state_if_current(inner, generation, ConnState::Error) {
        lock(inner).kill_tx = None;
        emit(app, ConnectionEvent::error(Some(node_id), message));
    }
}

fn enable_proxy(app: &AppHandle, local_port: u16) -> Result<(), AppError> {
    let data_dir = data_dir(app)?;
    sysproxy::enable(local_port, &sysproxy::backup_path(&data_dir))
}

fn teardown_proxy(app: &AppHandle, proxy_set: bool) {
    if !proxy_set {
        return;
    }
    match data_dir(app) {
        Ok(dir) => {
            if let Err(err) = sysproxy::restore(&sysproxy::backup_path(&dir)) {
                tracing::warn!("couldn't restore system proxy: {err}");
            }
        }
        Err(err) => tracing::warn!("couldn't resolve data dir for proxy restore: {err}"),
    }
}

pub fn data_dir(app: &AppHandle) -> Result<PathBuf, AppError> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Store(e.to_string()))?;
    std::fs::create_dir_all(&dir).map_err(|e| AppError::Store(e.to_string()))?;
    Ok(dir)
}

/// The bundled sidecar sits next to our executable (Tauri strips the target
/// triple suffix at bundle/dev time).
pub fn sidecar_path() -> Result<PathBuf, AppError> {
    let exe = std::env::current_exe()
        .map_err(|e| AppError::Core(format!("couldn't locate the app: {e}")))?;
    let dir = exe
        .parent()
        .ok_or_else(|| AppError::Core("couldn't locate the app folder".into()))?;
    let name = if cfg!(windows) { "sing-box.exe" } else { "sing-box" };
    let path = dir.join(name);
    if !path.exists() {
        return Err(AppError::Core(
            "The tunnel core is missing. Reinstall PrivateSparta.".into(),
        ));
    }
    Ok(path)
}

pub fn pick_free_port() -> Result<u16, AppError> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| AppError::Core(format!("no free local port: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| AppError::Core(format!("no free local port: {e}")))?
        .port();
    Ok(port)
}

pub fn random_secret() -> Result<String, AppError> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|e| AppError::Core(format!("no randomness source: {e}")))?;
    let mut hex = String::with_capacity(64);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(hex, "{b:02x}");
    }
    Ok(hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The full transition table for the connection state machine.
    #[test]
    fn connect_allowed_only_when_idle_or_errored() {
        assert!(can_connect(ConnState::Disconnected));
        assert!(can_connect(ConnState::Error));
        assert!(!can_connect(ConnState::Connecting));
        assert!(!can_connect(ConnState::Connected));
        assert!(!can_connect(ConnState::Disconnecting));
    }

    #[test]
    fn disconnect_allowed_only_when_up_or_coming_up() {
        assert!(can_disconnect(ConnState::Connected));
        assert!(can_disconnect(ConnState::Connecting));
        assert!(!can_disconnect(ConnState::Disconnected));
        assert!(!can_disconnect(ConnState::Disconnecting));
        assert!(!can_disconnect(ConnState::Error));
    }

    #[test]
    fn stale_generation_cannot_mutate_state() {
        let inner = Mutex::new(Inner {
            state: ConnState::Connected,
            node_id: None,
            generation: 5,
            kill_tx: None,
            run_info: None,
        });
        assert!(!set_state_if_current(&inner, 4, ConnState::Error));
        assert_eq!(lock(&inner).state, ConnState::Connected);
        assert!(set_state_if_current(&inner, 5, ConnState::Disconnected));
        assert_eq!(lock(&inner).state, ConnState::Disconnected);
    }

    #[test]
    fn finishing_clears_run_info() {
        let inner = Mutex::new(Inner {
            state: ConnState::Connected,
            node_id: None,
            generation: 1,
            kill_tx: None,
            run_info: Some(ClashEndpoint {
                port: 1234,
                secret: "s".into(),
            }),
        });
        assert!(set_state_if_current(&inner, 1, ConnState::Disconnected));
        assert!(lock(&inner).run_info.is_none());
    }

    #[test]
    fn backoff_grows_and_caps() {
        assert!(backoff(1) < backoff(2));
        assert!(backoff(2) < backoff(3));
        assert_eq!(backoff(10), backoff(4));
    }

    #[test]
    fn state_serializes_kebab_case() {
        let event = ConnectionEvent::new(ConnState::Connecting, None);
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("\"connecting\""));
        assert!(json.contains("\"nodeId\":null"));
    }
}
