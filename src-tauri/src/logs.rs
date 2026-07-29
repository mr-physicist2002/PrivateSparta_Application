//! Ring buffer for core log lines, pushed to the UI in 1 Hz batches.
//! Every line is credential-redacted before it enters the buffer.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

const CAPACITY: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub seq: u64,
    pub level: LogLevel,
    pub text: String,
}

/// sing-box lines look like: `+0000 2026-07-29 12:00:00 ERROR [...] message`
fn detect_level(line: &str) -> LogLevel {
    if line.contains("ERROR") || line.contains("FATAL") || line.contains("PANIC") {
        LogLevel::Error
    } else if line.contains("WARN") {
        LogLevel::Warn
    } else if line.contains("DEBUG") || line.contains("TRACE") {
        LogLevel::Debug
    } else {
        LogLevel::Info
    }
}

#[derive(Default)]
pub struct LogBuffer {
    seq: AtomicU64,
    lines: Mutex<VecDeque<LogLine>>,
    pending: Mutex<Vec<LogLine>>,
}

impl LogBuffer {
    /// `text` must already be redacted by the caller.
    pub fn push(&self, text: String) {
        let line = LogLine {
            seq: self.seq.fetch_add(1, Ordering::Relaxed),
            level: detect_level(&text),
            text,
        };
        {
            let mut lines = self.lines.lock().unwrap_or_else(|p| p.into_inner());
            if lines.len() >= CAPACITY {
                lines.pop_front();
            }
            lines.push_back(line.clone());
        }
        self.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(line);
    }

    pub fn all(&self) -> Vec<LogLine> {
        self.lines
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .cloned()
            .collect()
    }

    pub fn clear(&self) {
        self.lines
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        self.pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
    }

    pub fn joined(&self) -> String {
        let lines = self.lines.lock().unwrap_or_else(|p| p.into_inner());
        lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn take_pending(&self) -> Vec<LogLine> {
        std::mem::take(&mut *self.pending.lock().unwrap_or_else(|p| p.into_inner()))
    }
}

/// 1 Hz flusher — the only path that emits log events to the WebView.
pub fn start_flusher(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            ticker.tick().await;
            let state = tauri::Manager::state::<crate::commands::AppState>(&app);
            let batch = state.logs.take_pending();
            if !batch.is_empty() {
                let _ = app.emit("log-batch", &batch);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_singbox_levels() {
        assert_eq!(detect_level("+0330 2026-07-29 ERROR bad handshake"), LogLevel::Error);
        assert_eq!(detect_level("WARN something odd"), LogLevel::Warn);
        assert_eq!(detect_level("DEBUG dialing"), LogLevel::Debug);
        assert_eq!(detect_level("INFO started"), LogLevel::Info);
        assert_eq!(detect_level("plain text"), LogLevel::Info);
    }

    #[test]
    fn ring_caps_at_capacity() {
        let buffer = LogBuffer::default();
        for i in 0..(CAPACITY + 50) {
            buffer.push(format!("line {i}"));
        }
        let all = buffer.all();
        assert_eq!(all.len(), CAPACITY);
        assert_eq!(all[0].text, "line 50");
    }

    #[test]
    fn pending_drains_once() {
        let buffer = LogBuffer::default();
        buffer.push("a".into());
        buffer.push("b".into());
        assert_eq!(buffer.take_pending().len(), 2);
        assert!(buffer.take_pending().is_empty());
        assert_eq!(buffer.all().len(), 2);
    }
}
