//! Minimal client for sing-box's clash_api on 127.0.0.1.

use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct ClashEndpoint {
    pub port: u16,
    pub secret: String,
}

/// Delay-test target. cp.cloudflare.com responds with a tiny 204 and is
/// reachable from censored networks more reliably than gstatic.
pub const TEST_URL: &str = "http://cp.cloudflare.com/";
pub const DELAY_TIMEOUT_MS: u32 = 5000;

pub fn local_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap_or_default()
}

#[derive(Deserialize)]
struct DelayResponse {
    delay: u32,
}

/// One latency probe through the given outbound tag. None = unreachable.
pub async fn delay(
    client: &reqwest::Client,
    endpoint: &ClashEndpoint,
    tag: &str,
) -> Option<u32> {
    let url = format!("http://127.0.0.1:{}/proxies/{tag}/delay", endpoint.port);
    let response = client
        .get(url)
        .bearer_auth(&endpoint.secret)
        .query(&[
            ("timeout", DELAY_TIMEOUT_MS.to_string()),
            ("url", TEST_URL.to_string()),
        ])
        .timeout(std::time::Duration::from_millis(u64::from(DELAY_TIMEOUT_MS) + 2000))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json::<DelayResponse>().await.ok().map(|d| d.delay)
}

#[derive(Deserialize, Default, Clone, Copy)]
pub struct TrafficSample {
    pub up: u64,
    pub down: u64,
}
