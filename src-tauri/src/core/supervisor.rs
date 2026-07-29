//! Owns the sing-box lifecycle: spawn, health check, crash restarts (max 3,
//! exponential backoff), and teardown. All state changes flow through here and
//! are pushed to the WebView as `connection-state` events.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::config_gen::{self, GenInput};
use crate::error::{redact, AppError};
use crate::model::{ConnState, ConnectionEvent, Node, ProxyMode};
use crate::sysproxy;

#[cfg(windows)]
use super::job_object::JobObject;

const MAX_RESTARTS: u32 = 3;
const HEALTH_TIMEOUT: Duration = Duration::from_secs(6);
const HEALTH_POLL: Duration = Duration::from_millis(150);
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn can_connect(state: ConnState) -> bool {
    matches!(state, ConnState::Disconnected | ConnState::Error)
}

pub fn can_disconnect(state: ConnState) -> bool {
    matches!(state, ConnState::Connected | ConnState::Connecting)
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
            })),
        }
    }

    pub fn snapshot(&self) -> (ConnState, Option<Uuid>) {
        let inner = lock(&self.inner);
        (inner.state, inner.node_id)
    }

    pub fn connect(
        &self,
        app: &AppHandle,
        node: Node,
        mode: ProxyMode,
        local_port: u16,
    ) -> Result<(), AppError> {
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
            inner.node_id = Some(node.id);
            inner.kill_tx = Some(kill_tx);
        }
        emit(app, ConnectionEvent::new(ConnState::Connecting, Some(node.id)));

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
                node,
                mode,
                local_port,
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
    node: Node,
    mode: ProxyMode,
    local_port: u16,
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
        node,
        mode,
        local_port,
        mut kill_rx,
    } = ctx;
    let node_id = node.id;

    let (config_path, clash_port) = match prepare(&app, &node, local_port).await {
        Ok(prep) => prep,
        Err(err) => {
            finish_error(&app, &inner, generation, node_id, err.to_string());
            return;
        }
    };

    let stderr_ring: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
    let mut proxy_set = false;
    let mut attempt: u32 = 0;

    loop {
        let mut child = match spawn_core(&config_path, &stderr_ring) {
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

        match health_check(&mut child, clash_port, &mut kill_rx).await {
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

        if mode == ProxyMode::SystemProxy && !proxy_set {
            match enable_proxy(&app, local_port) {
                Ok(()) => proxy_set = true,
                Err(err) => {
                    let _ = child.kill().await;
                    finish_error(&app, &inner, generation, node_id, err.to_string());
                    return;
                }
            }
        }

        if !set_state_if_current(&inner, generation, ConnState::Connected) {
            let _ = child.kill().await;
            return;
        }
        emit(&app, ConnectionEvent::new(ConnState::Connected, Some(node_id)));

        tokio::select! {
            _ = &mut kill_rx => {
                let _ = child.kill().await;
                finish_disconnect(&app, &inner, generation, node_id, proxy_set);
                return;
            }
            status = child.wait() => {
                let code = status.ok().and_then(|s| s.code());
                tracing::warn!("core exited unexpectedly (code {code:?})");
                if attempt < MAX_RESTARTS {
                    attempt += 1;
                    if set_state_if_current(&inner, generation, ConnState::Connecting) {
                        emit(&app, ConnectionEvent::new(ConnState::Connecting, Some(node_id)));
                    } else {
                        return;
                    }
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

fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(500u64.saturating_mul(1 << attempt.min(4)))
}

async fn prepare(
    app: &AppHandle,
    node: &Node,
    local_port: u16,
) -> Result<(PathBuf, u16), AppError> {
    let data_dir = data_dir(app)?;
    let clash_port = pick_free_port()?;
    let secret = random_secret()?;
    let config = config_gen::generate(&GenInput {
        node,
        local_port,
        clash_port,
        clash_secret: &secret,
    })?;
    let config_path = data_dir.join("run-config.json");
    let json = serde_json::to_vec_pretty(&config)
        .map_err(|e| AppError::Config(e.to_string()))?;
    std::fs::write(&config_path, json).map_err(|e| AppError::Config(e.to_string()))?;

    // Validate before spawning; a rejected config is a clean, early error.
    let bin = sidecar_path()?;
    let mut cmd = Command::new(&bin);
    cmd.arg("check").arg("-c").arg(&config_path);
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
    Ok((config_path, clash_port))
}

fn spawn_core(
    config_path: &PathBuf,
    stderr_ring: &Arc<Mutex<VecDeque<String>>>,
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
        tauri::async_runtime::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::info!(target: "core", "{}", redact(&line));
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

fn last_core_error(ring: &Arc<Mutex<VecDeque<String>>>) -> String {
    let ring = ring.lock().unwrap_or_else(|p| p.into_inner());
    match ring.iter().rev().find(|l| l.contains("ERROR") || l.contains("FATAL")) {
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
fn sidecar_path() -> Result<PathBuf, AppError> {
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

fn pick_free_port() -> Result<u16, AppError> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|e| AppError::Core(format!("no free local port: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| AppError::Core(format!("no free local port: {e}")))?
        .port();
    Ok(port)
}

fn random_secret() -> Result<String, AppError> {
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
        });
        assert!(!set_state_if_current(&inner, 4, ConnState::Error));
        assert_eq!(lock(&inner).state, ConnState::Connected);
        assert!(set_state_if_current(&inner, 5, ConnState::Disconnected));
        assert_eq!(lock(&inner).state, ConnState::Disconnected);
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
