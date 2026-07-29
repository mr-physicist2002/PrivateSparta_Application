//! Routing rule-set resolution and optional background refresh.
//! Bundled .srs files ship in the app's resource dir so the first connect
//! never depends on a download; refreshed copies land in app-data and
//! shadow the bundled ones.

use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use crate::core::supervisor::data_dir;

pub const RULE_SETS: [(&str, &str); 3] = [
    (
        "geosite-ir",
        "https://raw.githubusercontent.com/Chocolate4U/Iran-sing-box-rules/rule-set/geosite-ir.srs",
    ),
    (
        "geoip-ir",
        "https://raw.githubusercontent.com/Chocolate4U/Iran-sing-box-rules/rule-set/geoip-ir.srs",
    ),
    (
        "geosite-category-ads-all",
        "https://raw.githubusercontent.com/SagerNet/sing-geosite/rule-set/geosite-category-ads-all.srs",
    ),
];

#[derive(Debug, Clone)]
pub struct RulePaths {
    pub geosite_ir: PathBuf,
    pub geoip_ir: PathBuf,
    pub ads: PathBuf,
}

/// Resolve rule-set paths: an app-data refresh shadows the bundled copy.
/// None if the bundled files are missing entirely (broken install) — the
/// caller then connects without split routing rather than failing.
pub fn resolve(app: &AppHandle) -> Option<RulePaths> {
    let resource_dir = app.path().resource_dir().ok()?.join("rulesets");
    let refreshed_dir = data_dir(app).ok()?.join("rulesets");
    let pick = |name: &str| -> Option<PathBuf> {
        let file = format!("{name}.srs");
        let refreshed = refreshed_dir.join(&file);
        if refreshed.exists() {
            return Some(refreshed);
        }
        let bundled = resource_dir.join(&file);
        bundled.exists().then_some(bundled)
    };
    Some(RulePaths {
        geosite_ir: pick("geosite-ir")?,
        geoip_ir: pick("geoip-ir")?,
        ads: pick("geosite-category-ads-all")?,
    })
}

/// Refresh rule-sets into app-data. Only ever called when the user has
/// enabled auto-refresh (default off: the app makes no unrequested requests).
/// Best-effort: failures leave the previous files in place.
pub async fn refresh(app: &AppHandle, via_local_proxy: Option<u16>) {
    let Ok(dir) = data_dir(app).map(|d| d.join("rulesets")) else {
        return;
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let mut builder = reqwest::Client::builder()
        .user_agent(format!("PrivateSparta/{}", app.package_info().version))
        .timeout(std::time::Duration::from_secs(60));
    builder = match via_local_proxy {
        Some(port) => {
            let Ok(proxy) = reqwest::Proxy::all(format!("http://127.0.0.1:{port}")) else {
                return;
            };
            builder.proxy(proxy)
        }
        None => builder.no_proxy(),
    };
    let Ok(client) = builder.build() else { return };

    for (name, url) in RULE_SETS {
        let target = dir.join(format!("{name}.srs"));
        match client.get(url).send().await {
            Ok(response) if response.status().is_success() => {
                if let Ok(bytes) = response.bytes().await {
                    // .srs binary magic: "SRS" — don't overwrite with an
                    // error page served as 200.
                    if bytes.len() > 16 && bytes.starts_with(b"SRS") {
                        let tmp = target.with_extension("srs.tmp");
                        if std::fs::write(&tmp, &bytes).is_ok() {
                            let _ = std::fs::rename(&tmp, &target);
                            tracing::info!("refreshed rule-set {name}");
                        }
                    } else {
                        tracing::warn!("rule-set {name} refresh returned junk; kept old");
                    }
                }
            }
            _ => tracing::info!("rule-set {name} refresh unavailable"),
        }
    }
}
