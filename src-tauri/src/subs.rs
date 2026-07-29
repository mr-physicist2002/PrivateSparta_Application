//! Subscription fetching and updating. Fetches go direct when disconnected
//! and through the tunnel when connected, so updates work under censorship.

use chrono::Utc;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;

use crate::commands::AppState;
use crate::error::AppError;
use crate::model::{ConnState, Node, SubUserInfo};
use crate::parser::subscription::{parse_payload, parse_userinfo};

const FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub struct FetchResult {
    pub nodes: Vec<Node>,
    pub skipped: usize,
    pub user_info: Option<SubUserInfo>,
}

pub async fn fetch(
    app_version: &str,
    url: &str,
    via_local_proxy: Option<u16>,
) -> Result<FetchResult, AppError> {
    let mut builder = reqwest::Client::builder()
        .user_agent(format!("PrivateSparta/{app_version}"))
        .timeout(FETCH_TIMEOUT)
        .gzip(true);
    builder = match via_local_proxy {
        Some(port) => builder.proxy(
            reqwest::Proxy::all(format!("http://127.0.0.1:{port}"))
                .map_err(|e| AppError::Config(e.to_string()))?,
        ),
        None => builder.no_proxy(),
    };
    let client = builder.build().map_err(|e| AppError::Config(e.to_string()))?;

    let response = client.get(url).send().await.map_err(|_| {
        AppError::Config(
            "Couldn't reach the subscription server. Check your connection or try updating through the tunnel."
                .into(),
        )
    })?;
    if !response.status().is_success() {
        return Err(AppError::Config(format!(
            "The subscription server answered {}. Check the link.",
            response.status().as_u16()
        )));
    }
    let user_info = response
        .headers()
        .get("subscription-userinfo")
        .and_then(|v| v.to_str().ok())
        .and_then(parse_userinfo);
    let body = response
        .text()
        .await
        .map_err(|_| AppError::Config("The subscription response couldn't be read.".into()))?;
    let batch = parse_payload(&body);
    if batch.nodes.is_empty() {
        return Err(AppError::Config(
            "No servers found in the subscription. The link may be wrong or expired.".into(),
        ));
    }
    Ok(FetchResult {
        nodes: batch.nodes,
        skipped: batch.skipped,
        user_info,
    })
}

/// Fetch and apply one subscription update. Existing node ids are preserved
/// when the endpoint is unchanged so selection and favorites survive updates.
pub async fn update_subscription(app: &AppHandle, sub_id: Uuid) -> Result<usize, AppError> {
    let state = app.state::<AppState>();
    let (url, via_proxy, version) = {
        let store = state.lock_store();
        let sub = store
            .config
            .subscriptions
            .iter()
            .find(|s| s.id == sub_id)
            .ok_or_else(|| AppError::Config("That subscription no longer exists.".into()))?;
        let (conn_state, _) = state.supervisor.snapshot();
        let via_proxy = if conn_state == ConnState::Connected {
            Some(store.config.settings.local_port)
        } else {
            None
        };
        (
            sub.url.clone(),
            via_proxy,
            app.package_info().version.to_string(),
        )
    };

    let outcome = fetch(&version, &url, via_proxy).await;
    let mut store = state.lock_store();
    let sub = store
        .config
        .subscriptions
        .iter_mut()
        .find(|s| s.id == sub_id)
        .ok_or_else(|| AppError::Config("That subscription no longer exists.".into()))?;
    match outcome {
        Ok(mut result) => {
            if result.skipped > 0 {
                tracing::info!("subscription update skipped {} unparseable entries", result.skipped);
            }
            for new_node in result.nodes.iter_mut() {
                if let Some(old) = sub.nodes.iter().find(|o| o.same_endpoint(new_node)) {
                    new_node.id = old.id;
                }
            }
            let count = result.nodes.len();
            sub.nodes = result.nodes;
            if result.user_info.is_some() {
                sub.user_info = result.user_info;
            }
            sub.last_updated = Some(Utc::now());
            sub.last_error = None;
            store.save()?;
            drop(store);
            let _ = app.emit("subs-changed", ());
            Ok(count)
        }
        Err(err) => {
            sub.last_error = Some(err.to_string());
            store.save()?;
            drop(store);
            let _ = app.emit("subs-changed", ());
            Err(err)
        }
    }
}

/// Background scheduler: every 10 minutes, update any subscription whose
/// auto-update interval has elapsed.
pub fn start_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(600));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            let due: Vec<Uuid> = {
                let state = app.state::<AppState>();
                let store = state.lock_store();
                let now = Utc::now();
                store
                    .config
                    .subscriptions
                    .iter()
                    .filter(|sub| {
                        sub.auto_update.as_hours().is_some_and(|hours| {
                            sub.last_updated.is_none_or(|last| {
                                now.signed_duration_since(last).num_hours() >= hours
                            })
                        })
                    })
                    .map(|sub| sub.id)
                    .collect()
            };
            for sub_id in due {
                if let Err(err) = update_subscription(&app, sub_id).await {
                    tracing::info!("auto-update failed: {err}");
                }
            }
        }
    });
}
