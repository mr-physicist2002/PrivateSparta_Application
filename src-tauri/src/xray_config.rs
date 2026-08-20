//! Xray configuration for XHTTP nodes. All other transports stay on sing-box;
//! this fallback exists only because sing-box does not implement XHTTP.

use serde_json::{json, Map, Value};

use crate::error::AppError;
use crate::model::{Node, ProtocolParams, ProxyMode, TlsConfig, Transport};

pub struct GenInput<'a> {
    pub node: &'a Node,
    pub local_port: u16,
    pub allow_lan: bool,
    pub log_level: &'a str,
    pub mode: ProxyMode,
    pub rules_enabled: bool,
    pub ad_block: bool,
}

pub fn generate(input: &GenInput<'_>) -> Result<Value, AppError> {
    if input.mode == ProxyMode::Tun {
        return Err(AppError::Config(
            "XHTTP currently works in System proxy and Proxy only modes. Switch mode before connecting to this server."
                .into(),
        ));
    }

    let outbound = outbound_for_node(input.node)?;
    let mut outbounds = vec![
        outbound,
        json!({ "tag": "direct", "protocol": "freedom" }),
    ];
    if input.ad_block {
        outbounds.push(json!({ "tag": "block", "protocol": "blackhole" }));
    }

    let mut routing_rules = Vec::new();
    if input.rules_enabled {
        routing_rules.push(json!({
            "type": "field",
            "ip": ["geoip:private", "geoip:ir"],
            "outboundTag": "direct"
        }));
        if input.ad_block {
            routing_rules.push(json!({
                "type": "field",
                "domain": ["geosite:category-ads-all"],
                "outboundTag": "block"
            }));
        }
    }

    Ok(json!({
        "log": {
            "loglevel": xray_log_level(input.log_level)
        },
        "inbounds": [{
            "tag": "mixed-in",
            "listen": if input.allow_lan { "0.0.0.0" } else { "127.0.0.1" },
            "port": input.local_port,
            "protocol": "mixed",
            "settings": { "udp": true }
        }],
        "outbounds": outbounds,
        "routing": {
            "domainStrategy": "IPIfNonMatch",
            "rules": routing_rules
        }
    }))
}

fn outbound_for_node(node: &Node) -> Result<Value, AppError> {
    let Transport::Xhttp { path, host, mode } = &node.transport else {
        return Err(AppError::Config(
            "The Xray fallback was requested for a non-XHTTP server.".into(),
        ));
    };

    let mut outbound = Map::new();
    outbound.insert("tag".into(), json!(node.tag()));
    match &node.params {
        ProtocolParams::Vless { uuid, flow } => {
            outbound.insert("protocol".into(), json!("vless"));
            let mut user = Map::new();
            user.insert("id".into(), json!(uuid));
            user.insert("encryption".into(), json!("none"));
            if let Some(flow) = flow.as_ref().filter(|flow| !flow.is_empty()) {
                user.insert("flow".into(), json!(flow));
            }
            outbound.insert(
                "settings".into(),
                json!({
                    "vnext": [{
                        "address": node.server,
                        "port": node.port,
                        "users": [Value::Object(user)]
                    }]
                }),
            );
        }
        ProtocolParams::Vmess {
            uuid,
            alter_id,
            security,
        } => {
            outbound.insert("protocol".into(), json!("vmess"));
            outbound.insert(
                "settings".into(),
                json!({
                    "vnext": [{
                        "address": node.server,
                        "port": node.port,
                        "users": [{
                            "id": uuid,
                            "alterId": alter_id,
                            "security": security
                        }]
                    }]
                }),
            );
        }
        ProtocolParams::Trojan { password } => {
            outbound.insert("protocol".into(), json!("trojan"));
            outbound.insert(
                "settings".into(),
                json!({
                    "servers": [{
                        "address": node.server,
                        "port": node.port,
                        "password": password
                    }]
                }),
            );
        }
        _ => {
            return Err(AppError::Config(
                "XHTTP is supported for VLESS, VMess, and Trojan servers.".into(),
            ))
        }
    }

    let mut xhttp = Map::new();
    xhttp.insert("path".into(), json!(path));
    xhttp.insert(
        "mode".into(),
        json!(if mode.trim().is_empty() { "auto" } else { mode }),
    );
    if let Some(host) = host.as_ref().filter(|host| !host.is_empty()) {
        xhttp.insert("host".into(), json!(host));
    }

    let mut stream = Map::new();
    stream.insert("network".into(), json!("xhttp"));
    stream.insert("xhttpSettings".into(), Value::Object(xhttp));
    add_security(&mut stream, &node.tls);
    outbound.insert("streamSettings".into(), Value::Object(stream));
    Ok(Value::Object(outbound))
}

fn add_security(stream: &mut Map<String, Value>, tls: &TlsConfig) {
    match tls {
        TlsConfig::None => {
            stream.insert("security".into(), json!("none"));
        }
        TlsConfig::Tls {
            sni,
            alpn,
            fingerprint,
            insecure,
        } => {
            stream.insert("security".into(), json!("tls"));
            let mut settings = Map::new();
            if let Some(sni) = sni {
                settings.insert("serverName".into(), json!(sni));
            }
            if !alpn.is_empty() {
                settings.insert("alpn".into(), json!(alpn));
            }
            if let Some(fingerprint) = fingerprint {
                settings.insert("fingerprint".into(), json!(fingerprint));
            }
            if *insecure {
                settings.insert("allowInsecure".into(), json!(true));
            }
            stream.insert("tlsSettings".into(), Value::Object(settings));
        }
        TlsConfig::Reality {
            sni,
            fingerprint,
            public_key,
            short_id,
            spider_x,
        } => {
            stream.insert("security".into(), json!("reality"));
            let mut settings = Map::new();
            if let Some(sni) = sni {
                settings.insert("serverName".into(), json!(sni));
            }
            settings.insert("fingerprint".into(), json!(fingerprint));
            settings.insert("publicKey".into(), json!(public_key));
            if let Some(short_id) = short_id {
                settings.insert("shortId".into(), json!(short_id));
            }
            if let Some(spider_x) = spider_x {
                settings.insert("spiderX".into(), json!(spider_x));
            }
            stream.insert("realitySettings".into(), Value::Object(settings));
        }
    }
}

fn xray_log_level(level: &str) -> &'static str {
    match level.to_ascii_lowercase().as_str() {
        "trace" | "debug" => "debug",
        "info" => "info",
        "error" => "error",
        "off" => "none",
        _ => "warning",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Protocol, ProtocolParams};
    use uuid::Uuid;

    fn xhttp_node() -> Node {
        Node {
            id: Uuid::new_v4(),
            name: "XHTTP".into(),
            protocol: Protocol::Vless,
            server: "example.com".into(),
            port: 443,
            params: ProtocolParams::Vless {
                uuid: Uuid::new_v4().to_string(),
                flow: None,
            },
            transport: Transport::Xhttp {
                path: "/api".into(),
                host: Some("cdn.example.com".into()),
                mode: "auto".into(),
            },
            tls: TlsConfig::Reality {
                sni: Some("www.example.com".into()),
                fingerprint: "chrome".into(),
                public_key: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
                short_id: Some("abcd".into()),
                spider_x: None,
            },
        }
    }

    #[test]
    fn generates_vless_reality_xhttp() {
        let node = xhttp_node();
        let config = generate(&GenInput {
            node: &node,
            local_port: 12334,
            allow_lan: false,
            log_level: "warn",
            mode: ProxyMode::SystemProxy,
            rules_enabled: true,
            ad_block: true,
        })
        .expect("generate");

        let outbound = &config["outbounds"][0];
        assert_eq!(outbound["protocol"], "vless");
        assert_eq!(outbound["streamSettings"]["network"], "xhttp");
        assert_eq!(outbound["streamSettings"]["xhttpSettings"]["mode"], "auto");
        assert_eq!(
            outbound["streamSettings"]["realitySettings"]["publicKey"],
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        );
        assert_eq!(config["inbounds"][0]["port"], 12334);
        assert_eq!(config["log"]["loglevel"], "warning");
    }

    #[test]
    fn tun_mode_fails_before_writing_an_unverified_config() {
        let node = xhttp_node();
        let error = generate(&GenInput {
            node: &node,
            local_port: 12334,
            allow_lan: false,
            log_level: "warn",
            mode: ProxyMode::Tun,
            rules_enabled: false,
            ad_block: false,
        })
        .expect_err("TUN is not enabled for the Xray fallback yet");
        assert!(error.to_string().contains("System proxy"));
    }

    #[test]
    #[ignore = "needs a verified official Xray sidecar and geodata"]
    fn generated_config_passes_xray_validation() {
        let binary = std::env::var_os("PRIVATESPARTA_XRAY_BIN")
            .expect("set PRIVATESPARTA_XRAY_BIN to the verified Xray executable");
        let assets = std::env::var_os("PRIVATESPARTA_XRAY_ASSETS")
            .expect("set PRIVATESPARTA_XRAY_ASSETS to the verified geodata directory");
        let node = xhttp_node();
        let config = generate(&GenInput {
            node: &node,
            local_port: 12335,
            allow_lan: false,
            log_level: "warn",
            mode: ProxyMode::SystemProxy,
            rules_enabled: true,
            ad_block: true,
        })
        .expect("generate");
        let path = std::env::temp_dir().join(format!(
            "privatesparta-xray-validation-{}.json",
            Uuid::new_v4()
        ));
        std::fs::write(&path, serde_json::to_vec_pretty(&config).expect("serialize"))
            .expect("write validation config");
        let output = std::process::Command::new(binary)
            .arg("run")
            .arg("-test")
            .arg("-config")
            .arg(&path)
            .env("XRAY_LOCATION_ASSET", assets)
            .output()
            .expect("run Xray validator");
        let _ = std::fs::remove_file(path);
        assert!(
            output.status.success(),
            "Xray rejected generated config: {} {}",
            crate::error::redact(&String::from_utf8_lossy(&output.stdout)),
            crate::error::redact(&String::from_utf8_lossy(&output.stderr)),
        );
    }
}
