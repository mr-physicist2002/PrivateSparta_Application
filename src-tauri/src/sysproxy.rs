//! Windows system proxy set/restore with a dirty-exit backup file.
//! The previous proxy state is written to disk BEFORE we change anything;
//! if the app dies while connected, the next launch restores from that file.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AppError;

pub const BACKUP_FILE: &str = "proxy-backup.json";

pub fn backup_path(data_dir: &Path) -> PathBuf {
    data_dir.join(BACKUP_FILE)
}

#[derive(Debug, Serialize, Deserialize)]
struct ProxyBackup {
    proxy_enable: u32,
    proxy_server: Option<String>,
    proxy_override: Option<String>,
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::fs;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::RegKey;

    const KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    const OVERRIDE: &str = "localhost;127.*;10.*;172.16.*;192.168.*;<local>";

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

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub fn enable(_local_port: u16, _backup: &Path) -> Result<(), AppError> {
        Err(AppError::Proxy(
            "system proxy isn't implemented on this platform yet".into(),
        ))
    }

    pub fn restore(_backup: &Path) -> Result<(), AppError> {
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
