use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppError;
use crate::model::{Node, ProxyMode, Subscription, WindowState};

pub const CURRENT_SCHEMA: u32 = 3;

fn default_local_port() -> u16 {
    12334
}
fn default_log_level() -> String {
    "warn".into()
}
fn default_mode() -> ProxyMode {
    ProxyMode::SystemProxy
}
fn default_true() -> bool {
    true
}
fn default_language() -> String {
    "en".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_mode")]
    pub mode: ProxyMode,
    #[serde(default = "default_local_port")]
    pub local_port: u16,
    #[serde(default)]
    pub allow_lan: bool,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default)]
    pub autostart: bool,
    #[serde(default)]
    pub start_minimized: bool,
    #[serde(default)]
    pub auto_connect: bool,
    /// Split routing (Iran direct, private direct) — on by default per brief.
    #[serde(default = "default_true")]
    pub rules_enabled: bool,
    #[serde(default = "default_true")]
    pub ad_block: bool,
    /// Background .srs refresh — off by default: the app makes zero network
    /// requests the user didn't initiate.
    #[serde(default)]
    pub ruleset_auto_update: bool,
    #[serde(default = "default_language")]
    pub language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            mode: default_mode(),
            local_port: default_local_port(),
            allow_lan: false,
            log_level: default_log_level(),
            autostart: false,
            start_minimized: false,
            auto_connect: false,
            rules_enabled: true,
            ad_block: true,
            ruleset_auto_update: false,
            language: default_language(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub schema_version: u32,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub subscriptions: Vec<Subscription>,
    #[serde(default)]
    pub manual_nodes: Vec<Node>,
    #[serde(default)]
    pub favorites: Vec<Uuid>,
    #[serde(default)]
    pub last_selected: Option<Uuid>,
    #[serde(default)]
    pub window: WindowState,
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            schema_version: CURRENT_SCHEMA,
            settings: Settings::default(),
            subscriptions: Vec::new(),
            manual_nodes: Vec::new(),
            favorites: Vec::new(),
            last_selected: None,
            window: WindowState::default(),
        }
    }
}

impl AppConfig {
    pub fn all_nodes(&self) -> impl Iterator<Item = &Node> {
        self.manual_nodes
            .iter()
            .chain(self.subscriptions.iter().flat_map(|s| s.nodes.iter()))
    }

    pub fn find_node(&self, id: Uuid) -> Option<&Node> {
        self.all_nodes().find(|n| n.id == id)
    }

    pub fn is_favorite(&self, id: Uuid) -> bool {
        self.favorites.contains(&id)
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

/// Versioned migration. Unknown future versions refuse to load rather than
/// silently drop data.
fn migrate(mut value: serde_json::Value) -> Result<AppConfig, String> {
    let version = value
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .ok_or("missing schema_version")? as u32;
    if version > CURRENT_SCHEMA {
        return Err(format!("config written by a newer version (schema {version})"));
    }
    if version < 1 {
        return Err(format!("no migration path from schema {version}"));
    }
    // v1 -> v2: subscriptions/favorites/window and the settings additions are
    // new fields with serde defaults, and Settings moved to camelCase keys —
    // rename what v1 wrote in snake_case so nothing is silently dropped.
    if version == 1 {
        if let Some(settings) = value.get_mut("settings").and_then(|s| s.as_object_mut()) {
            if let Some(port) = settings.remove("local_port") {
                settings.insert("localPort".into(), port);
            }
        }
    }
    // v2 -> v3: routing/ad-block/language settings, all serde-defaulted.
    if let Some(v) = value.get_mut("schema_version") {
        *v = serde_json::json!(CURRENT_SCHEMA);
    }
    serde_json::from_value::<AppConfig>(value).map_err(|e| e.to_string())
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
        assert_eq!(store.config.settings.local_port, 12334);
    }

    #[test]
    fn save_and_reload_roundtrip() {
        let dir = std::env::temp_dir().join(format!("ps-test-{}", Uuid::new_v4()));
        let path = dir.join("config.json");
        let mut store = ConfigStore::load(path.clone());
        store.config.settings.local_port = 3131;
        store.config.favorites.push(Uuid::new_v4());
        store.save().expect("save");
        let reloaded = ConfigStore::load(path);
        assert_eq!(reloaded.config.settings.local_port, 3131);
        assert_eq!(reloaded.config.favorites.len(), 1);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn migrates_v1_config() {
        // Exactly what Phase 1 wrote to disk: snake_case settings keys.
        let v1 = serde_json::json!({
            "schema_version": 1,
            "settings": { "mode": "system-proxy", "local_port": 2081 },
            "manual_nodes": [],
            "last_selected": null
        });
        let config = migrate(v1).expect("migrate");
        assert_eq!(config.schema_version, CURRENT_SCHEMA);
        assert_eq!(config.settings.local_port, 2081);
        assert!(config.subscriptions.is_empty());
        assert!(!config.settings.allow_lan);
        assert_eq!(config.settings.log_level, "warn");
    }

    #[test]
    fn migrates_v2_config_keeping_settings() {
        let v2 = serde_json::json!({
            "schema_version": 2,
            "settings": { "mode": "proxy-only", "localPort": 2099, "allowLan": true },
            "subscriptions": [],
            "manual_nodes": [],
            "favorites": []
        });
        let config = migrate(v2).expect("migrate");
        assert_eq!(config.schema_version, CURRENT_SCHEMA);
        assert_eq!(config.settings.local_port, 2099);
        assert!(config.settings.allow_lan);
        // v3 defaults
        assert!(config.settings.rules_enabled);
        assert!(config.settings.ad_block);
        assert!(!config.settings.ruleset_auto_update);
        assert_eq!(config.settings.language, "en");
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
