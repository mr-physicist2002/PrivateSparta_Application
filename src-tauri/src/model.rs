use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProxyMode {
    SystemProxy,
    ProxyOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Vless,
    Vmess,
    Trojan,
    Shadowsocks,
    Hysteria2,
    Tuic,
    Wireguard,
}

impl Protocol {
    pub fn label(self) -> &'static str {
        match self {
            Protocol::Vless => "VLESS",
            Protocol::Vmess => "VMess",
            Protocol::Trojan => "Trojan",
            Protocol::Shadowsocks => "SS",
            Protocol::Hysteria2 => "HY2",
            Protocol::Tuic => "TUIC",
            Protocol::Wireguard => "WG",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "protocol", rename_all = "lowercase")]
pub enum ProtocolParams {
    Vless {
        uuid: String,
        flow: Option<String>,
    },
    Vmess {
        uuid: String,
        alter_id: u32,
        security: String,
    },
    Trojan {
        password: String,
    },
    Shadowsocks {
        method: String,
        password: String,
        plugin: Option<String>,
        plugin_opts: Option<String>,
    },
    Hysteria2 {
        password: String,
        obfs: Option<String>,
        obfs_password: Option<String>,
    },
    Tuic {
        uuid: String,
        password: String,
        congestion_control: Option<String>,
        udp_relay_mode: Option<String>,
    },
    Wireguard {
        private_key: String,
        peer_public_key: String,
        preshared_key: Option<String>,
        addresses: Vec<String>,
        reserved: Option<Vec<u8>>,
        mtu: Option<u16>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Transport {
    Tcp,
    Ws {
        path: String,
        host: Option<String>,
    },
    Grpc {
        service_name: String,
    },
    #[serde(rename = "httpupgrade")]
    HttpUpgrade {
        path: String,
        host: Option<String>,
    },
    Xhttp {
        path: String,
        host: Option<String>,
    },
    H2 {
        path: String,
        host: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "security", rename_all = "lowercase")]
pub enum TlsConfig {
    None,
    Tls {
        sni: Option<String>,
        alpn: Vec<String>,
        fingerprint: Option<String>,
        insecure: bool,
    },
    Reality {
        sni: Option<String>,
        fingerprint: String,
        public_key: String,
        short_id: Option<String>,
        spider_x: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub id: Uuid,
    pub name: String,
    pub protocol: Protocol,
    pub server: String,
    pub port: u16,
    pub params: ProtocolParams,
    pub transport: Transport,
    pub tls: TlsConfig,
}

impl Node {
    /// Identity for dedup: same endpoint + credentials + transport.
    pub fn same_endpoint(&self, other: &Node) -> bool {
        self.server == other.server
            && self.port == other.port
            && self.params == other.params
            && self.transport == other.transport
    }

    /// The outbound tag this node gets in generated configs.
    pub fn tag(&self) -> String {
        self.id.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateInterval {
    #[serde(rename = "off")]
    Off,
    #[serde(rename = "6h")]
    H6,
    #[serde(rename = "12h")]
    H12,
    #[serde(rename = "24h")]
    H24,
}

impl UpdateInterval {
    pub fn as_hours(self) -> Option<i64> {
        match self {
            UpdateInterval::Off => None,
            UpdateInterval::H6 => Some(6),
            UpdateInterval::H12 => Some(12),
            UpdateInterval::H24 => Some(24),
        }
    }
}

/// Parsed `subscription-userinfo` response header. Bytes; expire is unix secs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubUserInfo {
    pub upload: u64,
    pub download: u64,
    pub total: u64,
    pub expire: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    pub nodes: Vec<Node>,
    pub user_info: Option<SubUserInfo>,
    pub auto_update: UpdateInterval,
    pub last_updated: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WindowState {
    pub width: u32,
    pub height: u32,
    pub x: Option<i32>,
    pub y: Option<i32>,
}

impl Default for WindowState {
    fn default() -> Self {
        WindowState {
            width: 960,
            height: 640,
            x: None,
            y: None,
        }
    }
}

/// What the WebView is allowed to see. No credentials, masked endpoint.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeView {
    pub id: Uuid,
    pub name: String,
    pub protocol: &'static str,
    pub endpoint: String,
    pub latency_ms: Option<u32>,
    pub favorite: bool,
}

impl NodeView {
    pub fn from_node(node: &Node, latency_ms: Option<u32>, favorite: bool) -> Self {
        NodeView {
            id: node.id,
            name: node.name.clone(),
            protocol: node.protocol.label(),
            endpoint: mask_endpoint(&node.server, node.port),
            latency_ms,
            favorite,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionView {
    pub id: Uuid,
    pub name: String,
    pub url_masked: String,
    pub node_count: usize,
    pub user_info: Option<SubUserInfo>,
    pub auto_update: UpdateInterval,
    pub last_updated: Option<i64>,
    pub last_error: Option<String>,
    pub nodes: Vec<NodeView>,
}

/// "https://panel.example.com/sub/token123" -> "https://panel.example.com/•••"
pub fn mask_url(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(parsed) => {
            let host = parsed.host_str().unwrap_or("•••");
            let has_path = parsed.path().len() > 1 || parsed.query().is_some();
            if has_path {
                format!("{}://{}/•••", parsed.scheme(), host)
            } else {
                format!("{}://{}", parsed.scheme(), host)
            }
        }
        Err(_) => "•••".into(),
    }
}

/// "de1.example.com:443" -> "de1.•••.com:443"; "203.0.113.7:443" -> "203.•••.7:443".
pub fn mask_endpoint(server: &str, port: u16) -> String {
    let parts: Vec<&str> = server.split('.').collect();
    let masked = if parts.len() >= 3 {
        let first = parts[0];
        let last = parts[parts.len() - 1];
        format!("{first}.•••.{last}")
    } else if parts.len() == 2 {
        format!("{}.•••", parts[0])
    } else {
        let head: String = server.chars().take(1).collect();
        format!("{head}•••")
    };
    format!("{masked}:{port}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConnState {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionEvent {
    pub state: ConnState,
    pub node_id: Option<Uuid>,
    pub message: Option<String>,
}

impl ConnectionEvent {
    pub fn new(state: ConnState, node_id: Option<Uuid>) -> Self {
        ConnectionEvent {
            state,
            node_id,
            message: None,
        }
    }

    pub fn error(node_id: Option<Uuid>, message: String) -> Self {
        ConnectionEvent {
            state: ConnState::Error,
            node_id,
            message: Some(message),
        }
    }
}

/// Pushed at 1 Hz while connected.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficEvent {
    pub up_bps: u64,
    pub down_bps: u64,
    pub up_total: u64,
    pub down_total: u64,
    pub seconds: u64,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LatencyResult {
    pub node_id: Uuid,
    pub latency_ms: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestProgress {
    pub running: bool,
    pub done: usize,
    pub total: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_multi_label_domain() {
        assert_eq!(mask_endpoint("de1.cdn.example.com", 443), "de1.•••.com:443");
    }

    #[test]
    fn masks_two_label_domain() {
        assert_eq!(mask_endpoint("example.com", 8443), "example.•••:8443");
    }

    #[test]
    fn masks_bare_host() {
        assert_eq!(mask_endpoint("myserver", 443), "m•••:443");
    }

    #[test]
    fn masks_ipv4_like_a_domain() {
        assert_eq!(mask_endpoint("203.0.113.7", 443), "203.•••.7:443");
    }

    #[test]
    fn masks_subscription_url_path() {
        assert_eq!(
            mask_url("https://panel.example.com/sub/secret-token?x=1"),
            "https://panel.example.com/•••"
        );
        assert_eq!(mask_url("https://example.com"), "https://example.com");
        assert_eq!(mask_url("not a url"), "•••");
    }

    #[test]
    fn update_interval_serializes_compactly() {
        assert_eq!(
            serde_json::to_string(&UpdateInterval::H6).expect("ser"),
            "\"6h\""
        );
        assert_eq!(
            serde_json::from_str::<UpdateInterval>("\"off\"").expect("de"),
            UpdateInterval::Off
        );
    }
}
