//! End-to-end smoke test against a real subscription.
//!
//! Ignored by default and reads the subscription URL from the environment, so
//! no credential ever lives in the repository:
//!
//!   $env:PRIVATESPARTA_TEST_SUB = "https://…"
//!   cargo test --lib live -- --ignored --nocapture
//!
//! It exercises the real path: fetch → parse → generate config →
//! `sing-box check` → spawn core → clash delay probes → HTTP through the
//! local inbound, confirming the exit IP actually changed.

#![cfg(test)]

use std::path::PathBuf;
use std::time::Duration;

use crate::clash::{self, ClashEndpoint};
use crate::config_gen::{self, GenInput};
use crate::model::ProxyMode;
use crate::parser::subscription::parse_payload;

const TRACE_URL: &str = "https://www.cloudflare.com/cdn-cgi/trace";

fn core_binary() -> PathBuf {
    let name = if cfg!(windows) {
        "sing-box-x86_64-pc-windows-msvc.exe"
    } else {
        "sing-box"
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join(name)
}

/// Pull `ip=` out of a Cloudflare trace response.
fn trace_ip(body: &str) -> Option<String> {
    body.lines()
        .find_map(|l| l.strip_prefix("ip="))
        .map(|s| s.trim().to_string())
}

#[tokio::test]
#[ignore = "needs PRIVATESPARTA_TEST_SUB and live network"]
async fn live_subscription_end_to_end() {
    let url = match std::env::var("PRIVATESPARTA_TEST_SUB") {
        Ok(url) if !url.is_empty() => url,
        _ => {
            eprintln!("PRIVATESPARTA_TEST_SUB not set; skipping");
            return;
        }
    };

    // 1. Fetch exactly the way the app does.
    let client = reqwest::Client::builder()
        .user_agent(format!("PrivateSparta/{}", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()
        .expect("client");
    let response = client.get(&url).send().await.expect("subscription fetch");
    assert!(response.status().is_success(), "status {}", response.status());
    let user_info = response
        .headers()
        .get("subscription-userinfo")
        .and_then(|v| v.to_str().ok())
        .and_then(crate::parser::subscription::parse_userinfo);
    println!("subscription-userinfo: {user_info:?}");
    let body = response.text().await.expect("body");

    // 2. Parse.
    let batch = parse_payload(&body);
    println!("parsed {} nodes, {} skipped", batch.nodes.len(), batch.skipped);
    assert!(!batch.nodes.is_empty(), "no nodes parsed");
    assert_eq!(batch.skipped, 0, "some nodes failed to parse");
    for node in &batch.nodes {
        println!(
            "  {} [{}] {}",
            node.name,
            node.protocol.label(),
            crate::model::mask_endpoint(&node.server, node.port)
        );
    }

    // 3. Generate a config for the first node and validate it with the core.
    let selected = batch.nodes[0].clone();
    let selected_tag = selected.tag();
    let local_port = crate::core::supervisor::pick_free_port().expect("port");
    let clash_port = crate::core::supervisor::pick_free_port().expect("port");
    let secret = crate::core::supervisor::random_secret().expect("secret");
    let config = config_gen::generate(&GenInput {
        nodes: &batch.nodes,
        selected_tag: &selected_tag,
        local_port: Some(local_port),
        allow_lan: false,
        clash_port,
        clash_secret: &secret,
        log_level: "warn",
        mode: ProxyMode::ProxyOnly,
        rules: None,
        ad_block: false,
        full_dns: true,
    })
    .expect("generate config");

    let config_path = std::env::temp_dir().join("privatesparta-live-test.json");
    std::fs::write(
        &config_path,
        serde_json::to_vec_pretty(&config).expect("serialize"),
    )
    .expect("write config");

    let bin = core_binary();
    assert!(bin.exists(), "core binary missing at {bin:?}");
    let check_started = std::time::Instant::now();
    let check = tokio::process::Command::new(&bin)
        .arg("check")
        .arg("-c")
        .arg(&config_path)
        .output()
        .await
        .expect("run check");
    println!(
        "PERF config generate + check: {} ms",
        check_started.elapsed().as_millis()
    );
    assert!(
        check.status.success(),
        "sing-box check rejected the generated config: {}",
        String::from_utf8_lossy(&check.stderr)
    );
    println!("sing-box check: ok ({} outbounds)", batch.nodes.len() + 1);

    // Baseline exit IP before the tunnel is up.
    let direct = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(20))
        .build()
        .expect("client");
    let direct_ip = match direct.get(TRACE_URL).send().await {
        Ok(r) => trace_ip(&r.text().await.unwrap_or_default()),
        Err(err) => {
            eprintln!("baseline trace unavailable ({err}); continuing");
            None
        }
    };
    println!("direct exit ip: {direct_ip:?}");

    // 4. Run the core, timing the whole connect path the way a click would.
    let connect_started = std::time::Instant::now();
    let mut child = tokio::process::Command::new(&bin)
        .arg("run")
        .arg("-c")
        .arg(&config_path)
        .kill_on_drop(true)
        .spawn()
        .expect("spawn core");
    let up = crate::core::supervisor::wait_for_port(&mut child, clash_port).await;
    let tunnel_up_ms = connect_started.elapsed().as_millis();
    assert!(up, "core never opened the clash port");
    println!("PERF connect→tunnel up: {tunnel_up_ms} ms (clash port {clash_port})");

    // 5. Latency-probe every node through the clash API.
    let endpoint = ClashEndpoint {
        port: clash_port,
        secret,
    };
    let probe_client = clash::local_client();
    let mut reachable = 0usize;
    for node in &batch.nodes {
        let delay = clash::delay(&probe_client, &endpoint, &node.tag()).await;
        match delay {
            Some(ms) => {
                reachable += 1;
                println!("  {:>6} ms  {}", ms, node.name);
            }
            None => println!("       —  {}", node.name),
        }
    }
    println!("{reachable}/{} nodes reachable", batch.nodes.len());

    // 6. Real traffic through the local mixed inbound.
    let tunneled = reqwest::Client::builder()
        .proxy(
            reqwest::Proxy::all(format!("http://127.0.0.1:{local_port}")).expect("proxy"),
        )
        .timeout(Duration::from_secs(30))
        .build()
        .expect("client");
    let first_byte = std::time::Instant::now();
    let through = tunneled
        .get(TRACE_URL)
        .send()
        .await
        .expect("request through the tunnel");
    println!(
        "PERF first request through tunnel: {} ms",
        first_byte.elapsed().as_millis()
    );
    assert!(through.status().is_success(), "status {}", through.status());
    let tunnel_ip = trace_ip(&through.text().await.expect("body"));
    println!("tunneled exit ip: {tunnel_ip:?}");
    assert!(tunnel_ip.is_some(), "no exit ip through the tunnel");
    if let (Some(direct), Some(tunnel)) = (&direct_ip, &tunnel_ip) {
        assert_ne!(direct, tunnel, "traffic did not leave through the tunnel");
    }
    assert!(reachable > 0, "no node answered a delay probe");

    let _ = child.kill().await;
    let _ = std::fs::remove_file(&config_path);
    println!("end-to-end: ok");
}
