use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppError;
use crate::model::{Node, ProxyMode};

pub const CURRENT_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub mode: ProxyMode,
    pub local_port: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            mode: ProxyMode::SystemProxy,
            local_port: 2080,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub schema_version: u32,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub manual_nodes: Vec<Node>,
    #[serde(default)]
    pub last_selected: Option<Uuid>,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            schema_version: CURRENT_SCHEMA,
            settings: Settings::default(),
            manual_nodes: Vec::new(),
            last_selected: None,
        }
    }
}

pub struct ConfigStore {
    path: PathBuf,
    pub config: AppConfig,
}

impl ConfigStore {
    /// Load from disk. A missing file yields defaults; a corrupt file is set
    /// aside (renamed .bad) rather than deleted, and defaults are used.
    pub fn load(path: PathBuf) -> Self {
        let config = match fs::read_to_string(&path) {
            Ok(raw) => match serde_json::from_str::<serde_json::Value>(&raw) {
                Ok(value) => match migrate(value) {
                    Ok(config) => config,
                    Err(err) => {
                        tracing::warn!("config migration failed: {err}; starting fresh");
                        quarantine(&path);
                        AppConfig::default()
                    }
                },
                Err(err) => {
                    tracing::warn!("config unreadable: {err}; starting fresh");
                    quarantine(&path);
                    AppConfig::default()
                }
            },
            Err(_) => AppConfig::default(),
        };
        ConfigStore { path, config }
    }

    /// Atomic save: write a temp file in the same directory, then replace.
    pub fn save(&self) -> Result<(), AppError> {
        let json = serde_json::to_string_pretty(&self.config)
            .map_err(|e| AppError::Store(e.to_string()))?;
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|e| AppError::Store(e.to_string()))?;
        }
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, json).map_err(|e| AppError::Store(e.to_string()))?;
        replace_file(&tmp, &self.path).map_err(|e| AppError::Store(e.to_string()))?;
        Ok(())
    }
}

fn quarantine(path: &Path) {
    let bad = path.with_extension("json.bad");
    let _ = fs::rename(path, bad);
}

#[cfg(windows)]
fn replace_file(tmp: &Path, dest: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let to_wide = |p: &Path| -> Vec<u16> {
        p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
    };
    let src = to_wide(tmp);
    let dst = to_wide(dest);
    // SAFETY: both strings are valid, NUL-terminated wide strings.
    let ok = unsafe {
        MoveFileExW(
            src.as_ptr(),
            dst.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(tmp: &Path, dest: &Path) -> std::io::Result<()> {
    std::fs::rename(tmp, dest)
}

/// Versioned migration. New schema versions add a step here; unknown future
/// versions refuse to load rather than silently drop data.
fn migrate(value: serde_json::Value) -> Result<AppConfig, String> {
    let version = value
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .ok_or("missing schema_version")? as u32;
    match version {
        CURRENT_SCHEMA => {
            serde_json::from_value::<AppConfig>(value).map_err(|e| e.to_string())
        }
        v if v > CURRENT_SCHEMA => Err(format!(
            "config written by a newer version (schema {v})"
        )),
        v => Err(format!("no migration path from schema {v}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_yields_defaults() {
        let store = ConfigStore::load(PathBuf::from(
            "Z:\\definitely\\not\\real\\config.json",
        ));
        assert_eq!(store.config.schema_version, CURRENT_SCHEMA);
        assert!(store.config.manual_nodes.is_empty());
    }

    #[test]
    fn save_and_reload_roundtrip() {
        let dir = std::env::temp_dir().join(format!("ps-test-{}", Uuid::new_v4()));
        let path = dir.join("config.json");
        let mut store = ConfigStore::load(path.clone());
        store.config.settings.local_port = 3131;
        store.save().expect("save");
        let reloaded = ConfigStore::load(path);
        assert_eq!(reloaded.config.settings.local_port, 3131);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn newer_schema_refuses_to_load() {
        let value = serde_json::json!({ "schema_version": 99 });
        assert!(migrate(value).is_err());
    }

    #[test]
    fn corrupt_json_yields_defaults() {
        let dir = std::env::temp_dir().join(format!("ps-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("config.json");
        fs::write(&path, "{ not valid json").expect("write");
        let store = ConfigStore::load(path.clone());
        assert_eq!(store.config.schema_version, CURRENT_SCHEMA);
        assert!(path.with_extension("json.bad").exists());
        let _ = fs::remove_dir_all(dir);
    }
}
