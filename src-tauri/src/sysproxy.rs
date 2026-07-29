//! System proxy set/restore with a dirty-exit backup file.
//! The previous proxy state is written to disk BEFORE we change anything;
//! if the app dies while connected, the next launch restores from that file.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AppError;

pub const BACKUP_FILE: &str = "proxy-backup.json";

pub fn backup_path(data_dir: &Path) -> PathBuf {
    data_dir.join(BACKUP_FILE)
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::fs;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::RegKey;

    const KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    const OVERRIDE: &str = "localhost;127.*;10.*;172.16.*;192.168.*;<local>";

    #[derive(Debug, Serialize, Deserialize)]
    struct ProxyBackup {
        proxy_enable: u32,
        proxy_server: Option<String>,
        proxy_override: Option<String>,
    }

    fn open_key() -> Result<RegKey, AppError> {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(KEY_PATH, KEY_READ | KEY_WRITE)
            .map_err(|e| AppError::Proxy(e.to_string()))
    }

    /// Point the system proxy at our local mixed inbound. Writes the backup
    /// first; an existing backup (unclean prior exit) is kept, not clobbered —
    /// it holds the true pre-PrivateSparta state.
    pub fn enable(local_port: u16, backup: &Path) -> Result<(), AppError> {
        let key = open_key()?;
        if !backup.exists() {
            let current = ProxyBackup {
                proxy_enable: key.get_value("ProxyEnable").unwrap_or(0u32),
                proxy_server: key.get_value("ProxyServer").ok(),
                proxy_override: key.get_value("ProxyOverride").ok(),
            };
            let json = serde_json::to_string(&current)
                .map_err(|e| AppError::Proxy(e.to_string()))?;
            fs::write(backup, json).map_err(|e| {
                AppError::Proxy(format!("couldn't save the previous proxy state: {e}"))
            })?;
        }
        key.set_value("ProxyEnable", &1u32)
            .map_err(|e| AppError::Proxy(e.to_string()))?;
        key.set_value("ProxyServer", &format!("127.0.0.1:{local_port}"))
            .map_err(|e| AppError::Proxy(e.to_string()))?;
        key.set_value("ProxyOverride", &OVERRIDE)
            .map_err(|e| AppError::Proxy(e.to_string()))?;
        refresh();
        Ok(())
    }

    /// Restore the pre-connect proxy state from the backup file. No backup
    /// file means nothing to do. Idempotent and safe to call from multiple
    /// teardown paths.
    pub fn restore(backup: &Path) -> Result<(), AppError> {
        let raw = match fs::read_to_string(backup) {
            Ok(raw) => raw,
            Err(_) => return Ok(()),
        };
        let saved: ProxyBackup =
            serde_json::from_str(&raw).map_err(|e| AppError::Proxy(e.to_string()))?;
        let key = open_key()?;
        key.set_value("ProxyEnable", &saved.proxy_enable)
            .map_err(|e| AppError::Proxy(e.to_string()))?;
        match &saved.proxy_server {
            Some(server) => key
                .set_value("ProxyServer", server)
                .map_err(|e| AppError::Proxy(e.to_string()))?,
            None => {
                let _ = key.delete_value("ProxyServer");
            }
        }
        match &saved.proxy_override {
            Some(ov) => key
                .set_value("ProxyOverride", ov)
                .map_err(|e| AppError::Proxy(e.to_string()))?,
            None => {
                let _ = key.delete_value("ProxyOverride");
            }
        }
        refresh();
        let _ = fs::remove_file(backup);
        Ok(())
    }

    fn refresh() {
        use windows_sys::Win32::Networking::WinInet::{
            InternetSetOptionW, INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED,
        };
        // SAFETY: documented calling convention for broadcasting a settings
        // change: null handle, null buffer, zero length.
        unsafe {
            InternetSetOptionW(
                std::ptr::null_mut(),
                INTERNET_OPTION_SETTINGS_CHANGED,
                std::ptr::null(),
                0,
            );
            InternetSetOptionW(
                std::ptr::null_mut(),
                INTERNET_OPTION_REFRESH,
                std::ptr::null(),
                0,
            );
        }
    }
}

/// macOS: drive `networksetup` per active network service.
/// UNVERIFIED — written from the documented interface, never run on a Mac.
#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use std::fs;
    use std::process::Command;

    #[derive(Debug, Serialize, Deserialize)]
    struct ServiceBackup {
        service: String,
        web_enabled: bool,
        secure_enabled: bool,
        web_server: Option<String>,
        secure_server: Option<String>,
    }

    fn network_services() -> Result<Vec<String>, AppError> {
        let output = Command::new("networksetup")
            .arg("-listallnetworkservices")
            .output()
            .map_err(|e| AppError::Proxy(e.to_string()))?;
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .skip(1) // header line
            .filter(|l| !l.starts_with('*')) // disabled services
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect())
    }

    fn read_state(service: &str, kind: &str) -> (bool, Option<String>) {
        let output = Command::new("networksetup")
            .arg(format!("-get{kind}proxy"))
            .arg(service)
            .output();
        let Ok(output) = output else {
            return (false, None);
        };
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        let field = |name: &str| -> Option<String> {
            text.lines()
                .find_map(|l| l.strip_prefix(&format!("{name}: ")))
                .map(|v| v.trim().to_string())
        };
        let enabled = field("Enabled").is_some_and(|v| v.eq_ignore_ascii_case("Yes"));
        let server = match (field("Server"), field("Port")) {
            (Some(host), Some(port)) if !host.is_empty() => Some(format!("{host}:{port}")),
            _ => None,
        };
        (enabled, server)
    }

    pub fn enable(local_port: u16, backup: &Path) -> Result<(), AppError> {
        let services = network_services()?;
        if !backup.exists() {
            let saved: Vec<ServiceBackup> = services
                .iter()
                .map(|service| {
                    let (web_enabled, web_server) = read_state(service, "web");
                    let (secure_enabled, secure_server) = read_state(service, "secureweb");
                    ServiceBackup {
                        service: service.clone(),
                        web_enabled,
                        secure_enabled,
                        web_server,
                        secure_server,
                    }
                })
                .collect();
            let json =
                serde_json::to_string(&saved).map_err(|e| AppError::Proxy(e.to_string()))?;
            fs::write(backup, json).map_err(|e| {
                AppError::Proxy(format!("couldn't save the previous proxy state: {e}"))
            })?;
        }
        let port = local_port.to_string();
        for service in &services {
            for kind in ["-setwebproxy", "-setsecurewebproxy"] {
                let _ = Command::new("networksetup")
                    .args([kind, service, "127.0.0.1", &port])
                    .status();
            }
        }
        Ok(())
    }

    pub fn restore(backup: &Path) -> Result<(), AppError> {
        let raw = match fs::read_to_string(backup) {
            Ok(raw) => raw,
            Err(_) => return Ok(()),
        };
        let saved: Vec<ServiceBackup> =
            serde_json::from_str(&raw).map_err(|e| AppError::Proxy(e.to_string()))?;
        for entry in saved {
            let restore_one = |set_state: &str, set_server: &str, enabled: bool,
                               server: &Option<String>| {
                match (enabled, server) {
                    (true, Some(addr)) => {
                        let (host, port) = addr.rsplit_once(':').unwrap_or((addr, "0"));
                        let _ = Command::new("networksetup")
                            .args([set_server, &entry.service, host, port])
                            .status();
                    }
                    _ => {
                        let _ = Command::new("networksetup")
                            .args([set_state, &entry.service, "off"])
                            .status();
                    }
                }
            };
            restore_one(
                "-setwebproxystate",
                "-setwebproxy",
                entry.web_enabled,
                &entry.web_server,
            );
            restore_one(
                "-setsecurewebproxystate",
                "-setsecurewebproxy",
                entry.secure_enabled,
                &entry.secure_server,
            );
        }
        let _ = fs::remove_file(backup);
        Ok(())
    }
}

/// Linux: GNOME/GSettings only. Other desktops have no common mechanism —
/// use Proxy Only or TUN mode there.
/// UNVERIFIED — written from the documented interface, never run on Linux.
#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::*;
    use std::fs;
    use std::process::Command;

    #[derive(Debug, Serialize, Deserialize)]
    struct GnomeBackup {
        mode: String,
        http_host: String,
        http_port: String,
        https_host: String,
        https_port: String,
    }

    fn get(schema: &str, key: &str) -> String {
        Command::new("gsettings")
            .args(["get", schema, key])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().trim_matches('\'').to_string())
            .unwrap_or_default()
    }

    fn set(schema: &str, key: &str, value: &str) {
        let _ = Command::new("gsettings")
            .args(["set", schema, key, value])
            .status();
    }

    fn gsettings_available() -> bool {
        Command::new("gsettings")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    }

    pub fn enable(local_port: u16, backup: &Path) -> Result<(), AppError> {
        if !gsettings_available() {
            return Err(AppError::Proxy(
                "System proxy needs GNOME settings here. Use Proxy Only or TUN mode."
                    .into(),
            ));
        }
        if !backup.exists() {
            let saved = GnomeBackup {
                mode: get("org.gnome.system.proxy", "mode"),
                http_host: get("org.gnome.system.proxy.http", "host"),
                http_port: get("org.gnome.system.proxy.http", "port"),
                https_host: get("org.gnome.system.proxy.https", "host"),
                https_port: get("org.gnome.system.proxy.https", "port"),
            };
            let json =
                serde_json::to_string(&saved).map_err(|e| AppError::Proxy(e.to_string()))?;
            fs::write(backup, json).map_err(|e| {
                AppError::Proxy(format!("couldn't save the previous proxy state: {e}"))
            })?;
        }
        let port = local_port.to_string();
        set("org.gnome.system.proxy.http", "host", "127.0.0.1");
        set("org.gnome.system.proxy.http", "port", &port);
        set("org.gnome.system.proxy.https", "host", "127.0.0.1");
        set("org.gnome.system.proxy.https", "port", &port);
        set("org.gnome.system.proxy", "mode", "manual");
        Ok(())
    }

    pub fn restore(backup: &Path) -> Result<(), AppError> {
        let raw = match fs::read_to_string(backup) {
            Ok(raw) => raw,
            Err(_) => return Ok(()),
        };
        let saved: GnomeBackup =
            serde_json::from_str(&raw).map_err(|e| AppError::Proxy(e.to_string()))?;
        set("org.gnome.system.proxy.http", "host", &saved.http_host);
        set("org.gnome.system.proxy.http", "port", &saved.http_port);
        set("org.gnome.system.proxy.https", "host", &saved.https_host);
        set("org.gnome.system.proxy.https", "port", &saved.https_port);
        set(
            "org.gnome.system.proxy",
            "mode",
            if saved.mode.is_empty() { "none" } else { &saved.mode },
        );
        let _ = fs::remove_file(backup);
        Ok(())
    }
}

pub use imp::{enable, restore};

/// Called once at launch: if a backup file exists, the last session died
/// without cleaning up. Put the user's proxy back before anything else.
pub fn restore_if_dirty(data_dir: &Path) {
    let backup = backup_path(data_dir);
    if backup.exists() {
        match restore(&backup) {
            Ok(()) => tracing::info!("restored system proxy after an unclean exit"),
            Err(err) => tracing::warn!("couldn't restore system proxy: {err}"),
        }
    }
}
