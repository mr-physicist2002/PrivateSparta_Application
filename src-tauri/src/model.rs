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
}

impl Protocol {
    pub fn label(self) -> &'static str {
        match self {
            Protocol::Vless => "VLESS",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "protocol", rename_all = "lowercase")]
pub enum ProtocolParams {
    Vless { uuid: String, flow: Option<String> },
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

/// What the WebView is allowed to see. No uuid, no keys, masked endpoint.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeView {
    pub id: Uuid,
    pub name: String,
    pub protocol: &'static str,
    pub endpoint: String,
}

impl NodeView {
    pub fn from_node(node: &Node) -> Self {
        NodeView {
            id: node.id,
            name: node.name.clone(),
            protocol: node.protocol.label(),
            endpoint: mask_endpoint(&node.server, node.port),
        }
    }
}

/// "de1.example.com:443" -> "de1.•••.com:443"; "203.0.113.7:443" -> "203.•••.7:443".
/// Single-label hosts keep only the first character.
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
}
