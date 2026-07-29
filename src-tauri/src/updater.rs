//! App update check against signed GitHub Releases.
//!
//! Explicitly user-initiated only — nothing here runs on a timer, because the
//! app makes zero network requests the user didn't ask for.

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

use crate::error::AppError;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub available: bool,
    pub version: Option<String>,
    pub notes: Option<String>,
}

pub async fn check(app: &AppHandle) -> Result<UpdateInfo, AppError> {
    let updater = app
        .updater()
        .map_err(|e| AppError::Core(format!("Couldn't check for updates: {e}")))?;
    match updater.check().await {
        Ok(Some(update)) => Ok(UpdateInfo {
            available: true,
            version: Some(update.version.clone()),
            notes: update.body.clone(),
        }),
        Ok(None) => Ok(UpdateInfo {
            available: false,
            version: None,
            notes: None,
        }),
        Err(err) => Err(AppError::Core(format!(
            "Couldn't reach the update server. {err}"
        ))),
    }
}

/// Download and install, then restart. Signature verification is enforced by
/// the plugin against the bundled public key.
pub async fn install(app: &AppHandle) -> Result<(), AppError> {
    let updater = app
        .updater()
        .map_err(|e| AppError::Core(format!("Couldn't check for updates: {e}")))?;
    let update = updater
        .check()
        .await
        .map_err(|e| AppError::Core(format!("Couldn't reach the update server. {e}")))?
        .ok_or_else(|| AppError::Core("Already up to date.".into()))?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| AppError::Core(format!("The update couldn't be installed. {e}")))?;
    app.restart();
}
