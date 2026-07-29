//! Subscription payload parsing: base64 URI list, plain URI list,
//! sing-box JSON, and Clash/Clash.Meta YAML — plus `subscription-userinfo`.

use serde_json::Value as Json;
use serde_yaml::Value as Yaml;
use uuid::Uuid;

use super::{parse_text, ParsedBatch};
use crate::model::{Node, Protocol, ProtocolParams, SubUserInfo, TlsConfig, Transport};

/// Detect the payload format and parse it. Never panics; unparseable entries
/// are counted as skipped.
pub fn parse_payload(body: &str) -> ParsedBatch {
    let trimmed = body.trim();
    if trimmed.starts_with('{') {
        return parse_singbox_json(trimmed);
    }
    if looks_like_clash(trimmed) {
        return parse_clash_yaml(trimmed);
    }
    // parse_text handles both plain and base64-wrapped URI lists.
    parse_text(trimmed)
}

/// `subscription-userinfo: upload=123; download=456; total=789; expire=1720000000`
pub fn parse_userinfo(header: &str) -> Option<SubUserInfo> {
    let mut upload = None;
    let mut download = None;
    let mut total = None;
    let mut expire = None;
    for part in header.split(';') {
        let (key, value) = part.split_once('=')?;
        let value = value.trim();
        match key.trim().to_ascii_lowercase().as_str() {
            "upload" => upload = value.parse::<u64>().ok(),
            "download" => download = value.parse::<u64>().ok(),
            "total" => total = value.parse::<u64>().ok(),
            "expire" => expire = value.parse::<i64>().ok().filter(|e| *e > 0),
            _ => {}
        }
    }
    Some(SubUserInfo {
        upload: upload?,
        download: download?,
        total: total?,
        expire,
    })
}

fn looks_like_clash(body: &str) -> bool {
    body.lines()
        .any(|l| l.trim_start().starts_with("proxies:"))
}

// ---------- sing-box JSON ----------

fn parse_singbox_json(body: &str) -> ParsedBatch {
    let mut nodes = Vec::new();
    let mut skipped = 0usize;
    let Ok(json) = serde_json::from_str::<Json>(body) else {
        return ParsedBatch { nodes, skipped: 1 };
    };
    let Some(outbounds) = json.get("outbounds").and_then(|o| o.as_array()) else {
        return ParsedBatch { nodes, skipped: 1 };
    };
    for outbound in outbounds {
        match singbox_outbound_to_node(outbound) {
            Some(node) => nodes.push(node),
            None => {
                let kind = outbound.get("type").and_then(|t| t.as_str()).unwrap_or("?");
                // Infrastructure outbounds aren't nodes and aren't failures.
                if !matches!(kind, "direct" | "block" | "dns" | "selector" | "urltest") {
                    skipped += 1;
                }
            }
        }
    }
    ParsedBatch { nodes, skipped }
}

fn singbox_outbound_to_node(ob: &Json) -> Option<Node> {
    let s = |key: &str| ob.get(key).and_then(|v| v.as_str()).map(|v| v.to_string());
    let kind = s("type")?;
    let server = s("server")?;
    let port = ob.get("server_port").and_then(|p| p.as_u64())? as u16;
    let name = s("tag").unwrap_or_else(|| format!("{server}:{port}"));

    let params = match kind.as_str() {
        "vless" => ProtocolParams::Vless {
            uuid: s("uuid")?,
            flow: s("flow"),
        },
        "vmess" => ProtocolParams::Vmess {
            uuid: s("uuid")?,
            alter_id: ob.get("alter_id").and_then(|a| a.as_u64()).unwrap_or(0) as u32,
            security: s("security").unwrap_or_else(|| "auto".into()),
        },
        "trojan" => ProtocolParams::Trojan { password: s("password")? },
        "shadowsocks" => ProtocolParams::Shadowsocks {
            method: s("method")?,
            password: s("password")?,
            plugin: s("plugin"),
            plugin_opts: s("plugin_opts"),
        },
        "hysteria2" => ProtocolParams::Hysteria2 {
            password: s("password")?,
            obfs: ob.pointer("/obfs/type").and_then(|v| v.as_str()).map(String::from),
            obfs_password: ob
                .pointer("/obfs/password")
                .and_then(|v| v.as_str())
                .map(String::from),
        },
        "tuic" => ProtocolParams::Tuic {
            uuid: s("uuid")?,
            password: s("password")?,
            congestion_control: s("congestion_control"),
            udp_relay_mode: s("udp_relay_mode"),
        },
        _ => return None,
    };
    let protocol = match kind.as_str() {
        "vless" => Protocol::Vless,
        "vmess" => Protocol::Vmess,
        "trojan" => Protocol::Trojan,
        "shadowsocks" => Protocol::Shadowsocks,
        "hysteria2" => Protocol::Hysteria2,
        "tuic" => Protocol::Tuic,
        _ => return None,
    };

    let transport = match ob.get("transport") {
        None => Transport::Tcp,
        Some(tr) => {
            let ts = |key: &str| tr.get(key).and_then(|v| v.as_str()).map(String::from);
            let path = ts("path").unwrap_or_else(|| "/".into());
            match tr.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                "ws" => Transport::Ws {
                    path,
                    host: tr
                        .pointer("/headers/Host")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                },
                "grpc" => Transport::Grpc {
                    service_name: ts("service_name").unwrap_or_default(),
                },
                "httpupgrade" => Transport::HttpUpgrade { path, host: ts("host") },
                "http" => Transport::H2 {
                    path,
                    host: tr
                        .get("host")
                        .and_then(|h| h.as_array())
                        .and_then(|a| a.first())
                        .and_then(|v| v.as_str())
                        .map(String::from),
                },
                _ => Transport::Tcp,
            }
        }
    };

    let tls = match ob.get("tls") {
        Some(tls) if tls.get("enabled").and_then(|e| e.as_bool()).unwrap_or(false) => {
            let sni = tls.get("server_name").and_then(|v| v.as_str()).map(String::from);
            let fingerprint = tls
                .pointer("/utls/fingerprint")
                .and_then(|v| v.as_str())
                .map(String::from);
            match tls.get("reality") {
                Some(re) if re.get("enabled").and_then(|e| e.as_bool()).unwrap_or(false) => {
                    TlsConfig::Reality {
                        sni,
                        fingerprint: fingerprint.unwrap_or_else(|| "chrome".into()),
                        public_key: re
                            .get("public_key")
                            .and_then(|v| v.as_str())
                            .map(String::from)?,
                        short_id: re.get("short_id").and_then(|v| v.as_str()).map(String::from),
                        spider_x: None,
                    }
                }
                _ => TlsConfig::Tls {
                    sni,
                    alpn: tls
                        .get("alpn")
                        .and_then(|a| a.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str())
                                .map(String::from)
                                .collect()
                        })
                        .unwrap_or_default(),
                    fingerprint,
                    insecure: tls.get("insecure").and_then(|v| v.as_bool()).unwrap_or(false),
                },
            }
        }
        _ => TlsConfig::None,
    };

    Some(Node {
        id: Uuid::new_v4(),
        name,
        protocol,
        server,
        port,
        params,
        transport,
        tls,
    })
}

// ---------- Clash / Clash.Meta YAML ----------

fn parse_clash_yaml(body: &str) -> ParsedBatch {
    let mut nodes = Vec::new();
    let mut skipped = 0usize;
    let Ok(yaml) = serde_yaml::from_str::<Yaml>(body) else {
        return ParsedBatch { nodes, skipped: 1 };
    };
    let Some(proxies) = yaml.get("proxies").and_then(|p| p.as_sequence()) else {
        return ParsedBatch { nodes, skipped: 1 };
    };
    for proxy in proxies {
        match clash_proxy_to_node(proxy) {
            Some(node) => nodes.push(node),
            None => skipped += 1,
        }
    }
    ParsedBatch { nodes, skipped }
}

fn yaml_str(value: &Yaml, key: &str) -> Option<String> {
    match value.get(key) {
        Some(Yaml::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Yaml::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

fn yaml_bool(value: &Yaml, key: &str) -> bool {
    value.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}

fn clash_proxy_to_node(proxy: &Yaml) -> Option<Node> {
    let kind = yaml_str(proxy, "type")?;
    let server = yaml_str(proxy, "server")?;
    let port: u16 = yaml_str(proxy, "port")?.parse().ok()?;
    let name = yaml_str(proxy, "name").unwrap_or_else(|| format!("{server}:{port}"));

    let (protocol, params) = match kind.as_str() {
        "vless" => (
            Protocol::Vless,
            ProtocolParams::Vless {
                uuid: yaml_str(proxy, "uuid")?,
                flow: yaml_str(proxy, "flow"),
            },
        ),
        "vmess" => (
            Protocol::Vmess,
            ProtocolParams::Vmess {
                uuid: yaml_str(proxy, "uuid")?,
                alter_id: yaml_str(proxy, "alterId")
                    .and_then(|a| a.parse().ok())
                    .unwrap_or(0),
                security: yaml_str(proxy, "cipher").unwrap_or_else(|| "auto".into()),
            },
        ),
        "trojan" => (
            Protocol::Trojan,
            ProtocolParams::Trojan {
                password: yaml_str(proxy, "password")?,
            },
        ),
        "ss" => (
            Protocol::Shadowsocks,
            ProtocolParams::Shadowsocks {
                method: yaml_str(proxy, "cipher")?,
                password: yaml_str(proxy, "password")?,
                plugin: yaml_str(proxy, "plugin"),
                plugin_opts: None,
            },
        ),
        "hysteria2" => (
            Protocol::Hysteria2,
            ProtocolParams::Hysteria2 {
                password: yaml_str(proxy, "password")?,
                obfs: yaml_str(proxy, "obfs"),
                obfs_password: yaml_str(proxy, "obfs-password"),
            },
        ),
        "tuic" => (
            Protocol::Tuic,
            ProtocolParams::Tuic {
                uuid: yaml_str(proxy, "uuid")?,
                password: yaml_str(proxy, "password")?,
                congestion_control: yaml_str(proxy, "congestion-controller"),
                udp_relay_mode: yaml_str(proxy, "udp-relay-mode"),
            },
        ),
        _ => return None,
    };

    let network = yaml_str(proxy, "network").unwrap_or_else(|| "tcp".into());
    let transport = match network.as_str() {
        "ws" => Transport::Ws {
            path: proxy
                .get("ws-opts")
                .and_then(|o| o.get("path"))
                .and_then(|v| v.as_str())
                .unwrap_or("/")
                .to_string(),
            host: proxy
                .get("ws-opts")
                .and_then(|o| o.get("headers"))
                .and_then(|h| h.get("Host"))
                .and_then(|v| v.as_str())
                .map(String::from),
        },
        "grpc" => Transport::Grpc {
            service_name: proxy
                .get("grpc-opts")
                .and_then(|o| o.get("grpc-service-name"))
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
        },
        _ => Transport::Tcp,
    };

    // trojan/hysteria2/tuic are TLS-implied in Clash configs.
    let tls_implied = matches!(kind.as_str(), "trojan" | "hysteria2" | "tuic");
    let tls = if yaml_bool(proxy, "tls") || tls_implied {
        let sni = yaml_str(proxy, "servername")
            .or_else(|| yaml_str(proxy, "sni"))
            .or_else(|| Some(server.clone()));
        let fingerprint = yaml_str(proxy, "client-fingerprint");
        match proxy.get("reality-opts") {
            Some(re) => TlsConfig::Reality {
                sni,
                fingerprint: fingerprint.unwrap_or_else(|| "chrome".into()),
                public_key: re
                    .get("public-key")
                    .and_then(|v| v.as_str())
                    .map(String::from)?,
                short_id: match re.get("short-id") {
                    Some(Yaml::String(s)) => Some(s.clone()),
                    Some(Yaml::Number(n)) => Some(n.to_string()),
                    _ => None,
                },
                spider_x: None,
            },
            None => TlsConfig::Tls {
                sni,
                alpn: proxy
                    .get("alpn")
                    .and_then(|a| a.as_sequence())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str())
                            .map(String::from)
                            .collect()
                    })
                    .unwrap_or_default(),
                fingerprint,
                insecure: yaml_bool(proxy, "skip-cert-verify"),
            },
        }
    } else {
        TlsConfig::None
    };

    Some(Node {
        id: Uuid::new_v4(),
        name,
        protocol,
        server,
        port,
        params,
        transport,
        tls,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    #[test]
    fn parses_plain_uri_list() {
        let body = "trojan://pw@a.example.com:443#a\nhy2://pw@b.example.com:443#b\n";
        let batch = parse_payload(body);
        assert_eq!(batch.nodes.len(), 2);
    }

    #[test]
    fn parses_base64_uri_list() {
        let body = STANDARD.encode("trojan://pw@a.example.com:443#a\n");
        let batch = parse_payload(&body);
        assert_eq!(batch.nodes.len(), 1);
    }

    #[test]
    fn parses_singbox_json_payload() {
        let body = r#"{
            "outbounds": [
                {"type": "vless", "tag": "DE 1", "server": "1.2.3.4", "server_port": 443,
                 "uuid": "2f9a4b7c-1d2e-4f5a-8b9c-0d1e2f3a4b5c", "flow": "xtls-rprx-vision",
                 "tls": {"enabled": true, "server_name": "www.microsoft.com",
                         "utls": {"enabled": true, "fingerprint": "chrome"},
                         "reality": {"enabled": true, "public_key": "pbk123", "short_id": "abcd"}}},
                {"type": "shadowsocks", "tag": "SS 1", "server": "5.6.7.8", "server_port": 8388,
                 "method": "aes-256-gcm", "password": "pw"},
                {"type": "direct", "tag": "direct"}
            ]
        }"#;
        let batch = parse_payload(body);
        assert_eq!(batch.nodes.len(), 2);
        assert_eq!(batch.skipped, 0);
        assert_eq!(batch.nodes[0].name, "DE 1");
        assert!(matches!(batch.nodes[0].tls, TlsConfig::Reality { .. }));
    }

    #[test]
    fn parses_clash_meta_yaml() {
        let body = r#"
port: 7890
proxies:
  - name: "Reality DE"
    type: vless
    server: 9.9.9.9
    port: 443
    uuid: 2f9a4b7c-1d2e-4f5a-8b9c-0d1e2f3a4b5c
    network: tcp
    tls: true
    flow: xtls-rprx-vision
    servername: www.microsoft.com
    client-fingerprint: chrome
    reality-opts:
      public-key: pbk456
      short-id: "1234"
  - name: VMess WS
    type: vmess
    server: 8.8.8.8
    port: 443
    uuid: 2f9a4b7c-1d2e-4f5a-8b9c-0d1e2f3a4b5c
    alterId: 0
    cipher: auto
    network: ws
    tls: true
    servername: cdn.example.com
    ws-opts:
      path: /vm
      headers:
        Host: cdn.example.com
  - name: Unsupported
    type: snell
    server: 7.7.7.7
    port: 443
"#;
        let batch = parse_payload(body);
        assert_eq!(batch.nodes.len(), 2);
        assert_eq!(batch.skipped, 1);
        match &batch.nodes[0].tls {
            TlsConfig::Reality { public_key, short_id, .. } => {
                assert_eq!(public_key, "pbk456");
                assert_eq!(short_id.as_deref(), Some("1234"));
            }
            other => panic!("expected reality, got {other:?}"),
        }
        match &batch.nodes[1].transport {
            Transport::Ws { path, host } => {
                assert_eq!(path, "/vm");
                assert_eq!(host.as_deref(), Some("cdn.example.com"));
            }
            other => panic!("expected ws, got {other:?}"),
        }
    }

    #[test]
    fn parses_userinfo_header() {
        let info =
            parse_userinfo("upload=1073741824; download=5368709120; total=107374182400; expire=1735689600")
                .expect("parse");
        assert_eq!(info.upload, 1_073_741_824);
        assert_eq!(info.download, 5_368_709_120);
        assert_eq!(info.total, 107_374_182_400);
        assert_eq!(info.expire, Some(1_735_689_600));
    }

    #[test]
    fn userinfo_zero_expire_means_none() {
        let info = parse_userinfo("upload=1; download=2; total=3; expire=0").expect("parse");
        assert_eq!(info.expire, None);
    }

    #[test]
    fn malformed_payloads_never_panic() {
        for body in ["", "{", "{}", "proxies: 12", "%%%", "proxies:\n  - 5"] {
            let _ = parse_payload(body);
        }
    }
}
