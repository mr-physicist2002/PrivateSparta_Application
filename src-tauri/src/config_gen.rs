use serde_json::{json, Map, Value};

use crate::error::AppError;
use crate::model::{Node, ProtocolParams, TlsConfig, Transport};

pub struct GenInput<'a> {
    pub node: &'a Node,
    pub local_port: u16,
    pub clash_port: u16,
    pub clash_secret: &'a str,
}

/// Build a complete sing-box config for one node. Generated fresh on every
/// connect; never persisted beyond the runtime file handed to the sidecar.
pub fn generate(input: &GenInput<'_>) -> Result<Value, AppError> {
    let outbound = vless_outbound(input.node)?;
    Ok(json!({
        "log": { "level": "warn", "timestamp": true },
        "experimental": {
            "clash_api": {
                "external_controller": format!("127.0.0.1:{}", input.clash_port),
                "secret": input.clash_secret
            }
        },
        "inbounds": [{
            "type": "mixed",
            "tag": "mixed-in",
            "listen": "127.0.0.1",
            "listen_port": input.local_port
        }],
        "outbounds": [
            outbound,
            { "type": "direct", "tag": "direct" }
        ],
        "route": { "final": "proxy", "auto_detect_interface": true }
    }))
}

fn vless_outbound(node: &Node) -> Result<Value, AppError> {
    let ProtocolParams::Vless { uuid, flow } = &node.params;

    let mut ob = Map::new();
    ob.insert("type".into(), json!("vless"));
    ob.insert("tag".into(), json!("proxy"));
    ob.insert("server".into(), json!(node.server));
    ob.insert("server_port".into(), json!(node.port));
    ob.insert("uuid".into(), json!(uuid));
    if let Some(flow) = flow {
        ob.insert("flow".into(), json!(flow));
    }
    if let Some(tls) = tls_value(&node.tls) {
        ob.insert("tls".into(), tls);
    }
    if let Some(transport) = transport_value(&node.transport)? {
        ob.insert("transport".into(), transport);
    }
    Ok(Value::Object(ob))
}

fn tls_value(tls: &TlsConfig) -> Option<Value> {
    match tls {
        TlsConfig::None => None,
        TlsConfig::Tls {
            sni,
            alpn,
            fingerprint,
            insecure,
        } => {
            let mut v = Map::new();
            v.insert("enabled".into(), json!(true));
            if let Some(sni) = sni {
                v.insert("server_name".into(), json!(sni));
            }
            if *insecure {
                v.insert("insecure".into(), json!(true));
            }
            if !alpn.is_empty() {
                v.insert("alpn".into(), json!(alpn));
            }
            if let Some(fp) = fingerprint {
                v.insert("utls".into(), json!({ "enabled": true, "fingerprint": fp }));
            }
            Some(Value::Object(v))
        }
        TlsConfig::Reality {
            sni,
            fingerprint,
            public_key,
            short_id,
            spider_x: _,
        } => {
            let mut reality = Map::new();
            reality.insert("enabled".into(), json!(true));
            reality.insert("public_key".into(), json!(public_key));
            if let Some(sid) = short_id {
                reality.insert("short_id".into(), json!(sid));
            }
            let mut v = Map::new();
            v.insert("enabled".into(), json!(true));
            if let Some(sni) = sni {
                v.insert("server_name".into(), json!(sni));
            }
            v.insert(
                "utls".into(),
                json!({ "enabled": true, "fingerprint": fingerprint }),
            );
            v.insert("reality".into(), Value::Object(reality));
            Some(Value::Object(v))
        }
    }
}

fn transport_value(transport: &Transport) -> Result<Option<Value>, AppError> {
    Ok(match transport {
        Transport::Tcp => None,
        Transport::Ws { path, host } => {
            let mut v = Map::new();
            v.insert("type".into(), json!("ws"));
            v.insert("path".into(), json!(path));
            if let Some(host) = host {
                v.insert("headers".into(), json!({ "Host": host }));
            }
            Some(Value::Object(v))
        }
        Transport::Grpc { service_name } => Some(json!({
            "type": "grpc",
            "service_name": service_name
        })),
        Transport::HttpUpgrade { path, host } => {
            let mut v = Map::new();
            v.insert("type".into(), json!("httpupgrade"));
            v.insert("path".into(), json!(path));
            if let Some(host) = host {
                v.insert("host".into(), json!(host));
            }
            Some(Value::Object(v))
        }
        Transport::H2 { path, host } => {
            let mut v = Map::new();
            v.insert("type".into(), json!("http"));
            v.insert("path".into(), json!(path));
            if let Some(host) = host {
                v.insert("host".into(), json!([host]));
            }
            Some(Value::Object(v))
        }
        Transport::Xhttp { .. } => {
            return Err(AppError::Config(
                "This server uses the xhttp transport, which the tunnel core doesn't support yet. Pick another server.".into(),
            ))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Node, Protocol};
    use uuid::Uuid;

    fn reality_node() -> Node {
        Node {
            id: Uuid::new_v4(),
            name: "test".into(),
            protocol: Protocol::Vless,
            server: "203.0.113.7".into(),
            port: 443,
            params: ProtocolParams::Vless {
                uuid: "2f9a4b7c-1d2e-4f5a-8b9c-0d1e2f3a4b5c".into(),
                flow: Some("xtls-rprx-vision".into()),
            },
            transport: Transport::Tcp,
            tls: TlsConfig::Reality {
                sni: Some("www.microsoft.com".into()),
                fingerprint: "chrome".into(),
                public_key: "pubkey123".into(),
                short_id: Some("6ba85179".into()),
                spider_x: None,
            },
        }
    }

    #[test]
    fn generates_reality_outbound() {
        let node = reality_node();
        let cfg = generate(&GenInput {
            node: &node,
            local_port: 2080,
            clash_port: 9911,
            clash_secret: "s3cret",
        })
        .expect("generate");

        let ob = &cfg["outbounds"][0];
        assert_eq!(ob["type"], "vless");
        assert_eq!(ob["flow"], "xtls-rprx-vision");
        assert_eq!(ob["tls"]["reality"]["public_key"], "pubkey123");
        assert_eq!(ob["tls"]["reality"]["short_id"], "6ba85179");
        assert_eq!(ob["tls"]["utls"]["fingerprint"], "chrome");
        assert_eq!(ob["tls"]["server_name"], "www.microsoft.com");
        assert!(ob.get("transport").is_none());
    }

    #[test]
    fn inbound_binds_loopback_only() {
        let node = reality_node();
        let cfg = generate(&GenInput {
            node: &node,
            local_port: 2080,
            clash_port: 9911,
            clash_secret: "s",
        })
        .expect("generate");
        assert_eq!(cfg["inbounds"][0]["listen"], "127.0.0.1");
        assert_eq!(
            cfg["experimental"]["clash_api"]["external_controller"],
            "127.0.0.1:9911"
        );
    }

    #[test]
    fn ws_transport_carries_host_header() {
        let mut node = reality_node();
        node.transport = Transport::Ws {
            path: "/ws".into(),
            host: Some("cdn.example.com".into()),
        };
        node.tls = TlsConfig::Tls {
            sni: Some("cdn.example.com".into()),
            alpn: vec![],
            fingerprint: None,
            insecure: false,
        };
        let cfg = generate(&GenInput {
            node: &node,
            local_port: 2080,
            clash_port: 9911,
            clash_secret: "s",
        })
        .expect("generate");
        let ob = &cfg["outbounds"][0];
        assert_eq!(ob["transport"]["type"], "ws");
        assert_eq!(ob["transport"]["headers"]["Host"], "cdn.example.com");
    }

    #[test]
    fn xhttp_is_a_clean_error() {
        let mut node = reality_node();
        node.transport = Transport::Xhttp {
            path: "/".into(),
            host: None,
        };
        node.tls = TlsConfig::None;
        let err = generate(&GenInput {
            node: &node,
            local_port: 2080,
            clash_port: 9911,
            clash_secret: "s",
        });
        assert!(err.is_err());
    }
}
