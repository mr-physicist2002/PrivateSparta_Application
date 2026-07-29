use serde_json::{json, Map, Value};

use crate::error::AppError;
use crate::model::{Node, ProtocolParams, TlsConfig, Transport};

pub struct GenInput<'a> {
    /// Every known node becomes a tagged outbound so the clash API can
    /// latency-test any of them on the running instance.
    pub nodes: &'a [Node],
    /// Tag of the node traffic actually routes through.
    pub selected_tag: &'a str,
    /// None = no local inbound (ephemeral latency-test instance).
    pub local_port: Option<u16>,
    pub allow_lan: bool,
    pub clash_port: u16,
    pub clash_secret: &'a str,
    pub log_level: &'a str,
}

/// Build a complete sing-box config. Generated fresh on every connect; never
/// persisted beyond the runtime file handed to the sidecar.
pub fn generate(input: &GenInput<'_>) -> Result<Value, AppError> {
    let mut outbounds = Vec::new();
    let mut endpoints = Vec::new();
    for node in input.nodes {
        match outbound_for_node(node) {
            Ok(OutboundValue::Outbound(v)) => outbounds.push(v),
            Ok(OutboundValue::Endpoint(v)) => endpoints.push(v),
            Err(err) => {
                // Only the selected node MUST generate; others are skipped so
                // one bad node can't block connecting through a good one.
                if node.tag() == input.selected_tag {
                    return Err(err);
                }
                tracing::debug!("skipping node in config: {err}");
            }
        }
    }
    if !outbounds.iter().any(|o| o["tag"] == input.selected_tag)
        && !endpoints.iter().any(|e| e["tag"] == input.selected_tag)
    {
        return Err(AppError::Config("The selected server is unavailable.".into()));
    }
    outbounds.push(json!({ "type": "direct", "tag": "direct" }));

    let mut inbounds = Vec::new();
    if let Some(port) = input.local_port {
        inbounds.push(json!({
            "type": "mixed",
            "tag": "mixed-in",
            "listen": if input.allow_lan { "0.0.0.0" } else { "127.0.0.1" },
            "listen_port": port
        }));
    }

    let mut root = Map::new();
    root.insert(
        "log".into(),
        json!({ "level": input.log_level, "timestamp": true }),
    );
    root.insert(
        "experimental".into(),
        json!({
            "clash_api": {
                "external_controller": format!("127.0.0.1:{}", input.clash_port),
                "secret": input.clash_secret
            }
        }),
    );
    root.insert("inbounds".into(), Value::Array(inbounds));
    root.insert("outbounds".into(), Value::Array(outbounds));
    if !endpoints.is_empty() {
        root.insert("endpoints".into(), Value::Array(endpoints));
    }
    root.insert(
        "route".into(),
        json!({ "final": input.selected_tag, "auto_detect_interface": true }),
    );
    Ok(Value::Object(root))
}

pub enum OutboundValue {
    Outbound(Value),
    /// WireGuard lives under `endpoints` since sing-box 1.11.
    Endpoint(Value),
}

pub fn outbound_for_node(node: &Node) -> Result<OutboundValue, AppError> {
    let tag = node.tag();
    let mut ob = Map::new();
    ob.insert("tag".into(), json!(tag));
    ob.insert("server".into(), json!(node.server));
    ob.insert("server_port".into(), json!(node.port));

    match &node.params {
        ProtocolParams::Vless { uuid, flow } => {
            ob.insert("type".into(), json!("vless"));
            ob.insert("uuid".into(), json!(uuid));
            if let Some(flow) = flow {
                ob.insert("flow".into(), json!(flow));
            }
        }
        ProtocolParams::Vmess {
            uuid,
            alter_id,
            security,
        } => {
            ob.insert("type".into(), json!("vmess"));
            ob.insert("uuid".into(), json!(uuid));
            ob.insert("alter_id".into(), json!(alter_id));
            ob.insert("security".into(), json!(security));
        }
        ProtocolParams::Trojan { password } => {
            ob.insert("type".into(), json!("trojan"));
            ob.insert("password".into(), json!(password));
        }
        ProtocolParams::Shadowsocks {
            method,
            password,
            plugin,
            plugin_opts,
        } => {
            ob.insert("type".into(), json!("shadowsocks"));
            ob.insert("method".into(), json!(method));
            ob.insert("password".into(), json!(password));
            if let Some(plugin) = plugin {
                ob.insert("plugin".into(), json!(plugin));
                if let Some(opts) = plugin_opts {
                    ob.insert("plugin_opts".into(), json!(opts));
                }
            }
        }
        ProtocolParams::Hysteria2 {
            password,
            obfs,
            obfs_password,
        } => {
            ob.insert("type".into(), json!("hysteria2"));
            ob.insert("password".into(), json!(password));
            if let (Some(obfs), Some(obfs_pw)) = (obfs, obfs_password) {
                ob.insert(
                    "obfs".into(),
                    json!({ "type": obfs, "password": obfs_pw }),
                );
            }
        }
        ProtocolParams::Tuic {
            uuid,
            password,
            congestion_control,
            udp_relay_mode,
        } => {
            ob.insert("type".into(), json!("tuic"));
            ob.insert("uuid".into(), json!(uuid));
            ob.insert("password".into(), json!(password));
            if let Some(cc) = congestion_control {
                ob.insert("congestion_control".into(), json!(cc));
            }
            if let Some(mode) = udp_relay_mode {
                ob.insert("udp_relay_mode".into(), json!(mode));
            }
        }
        ProtocolParams::Wireguard {
            private_key,
            peer_public_key,
            preshared_key,
            addresses,
            reserved,
            mtu,
        } => {
            let mut peer = Map::new();
            peer.insert("address".into(), json!(node.server));
            peer.insert("port".into(), json!(node.port));
            peer.insert("public_key".into(), json!(peer_public_key));
            peer.insert("allowed_ips".into(), json!(["0.0.0.0/0", "::/0"]));
            if let Some(psk) = preshared_key {
                peer.insert("pre_shared_key".into(), json!(psk));
            }
            if let Some(reserved) = reserved {
                peer.insert("reserved".into(), json!(reserved));
            }
            let mut ep = Map::new();
            ep.insert("type".into(), json!("wireguard"));
            ep.insert("tag".into(), json!(tag));
            ep.insert("address".into(), json!(addresses));
            ep.insert("private_key".into(), json!(private_key));
            ep.insert("peers".into(), json!([Value::Object(peer)]));
            if let Some(mtu) = mtu {
                ep.insert("mtu".into(), json!(mtu));
            }
            return Ok(OutboundValue::Endpoint(Value::Object(ep)));
        }
    }

    if let Some(tls) = tls_value(&node.tls) {
        ob.insert("tls".into(), tls);
    }
    if let Some(transport) = transport_value(&node.transport)? {
        ob.insert("transport".into(), transport);
    }
    Ok(OutboundValue::Outbound(Value::Object(ob)))
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
    use crate::model::Protocol;
    use uuid::Uuid;

    fn base_node(protocol: Protocol, params: ProtocolParams) -> Node {
        Node {
            id: Uuid::new_v4(),
            name: "test".into(),
            protocol,
            server: "203.0.113.7".into(),
            port: 443,
            params,
            transport: Transport::Tcp,
            tls: TlsConfig::None,
        }
    }

    fn reality_node() -> Node {
        let mut node = base_node(
            Protocol::Vless,
            ProtocolParams::Vless {
                uuid: "2f9a4b7c-1d2e-4f5a-8b9c-0d1e2f3a4b5c".into(),
                flow: Some("xtls-rprx-vision".into()),
            },
        );
        node.tls = TlsConfig::Reality {
            sni: Some("www.microsoft.com".into()),
            fingerprint: "chrome".into(),
            public_key: "pubkey123".into(),
            short_id: Some("6ba85179".into()),
            spider_x: None,
        };
        node
    }

    fn gen(nodes: &[Node], selected: &str) -> Value {
        generate(&GenInput {
            nodes,
            selected_tag: selected,
            local_port: Some(2080),
            allow_lan: false,
            clash_port: 9911,
            clash_secret: "s3cret",
            log_level: "warn",
        })
        .expect("generate")
    }

    #[test]
    fn generates_reality_outbound_and_routes_to_it() {
        let node = reality_node();
        let tag = node.tag();
        let cfg = gen(std::slice::from_ref(&node), &tag);
        let ob = &cfg["outbounds"][0];
        assert_eq!(ob["type"], "vless");
        assert_eq!(ob["tag"], tag.as_str());
        assert_eq!(ob["tls"]["reality"]["public_key"], "pubkey123");
        assert_eq!(cfg["route"]["final"], tag.as_str());
    }

    #[test]
    fn all_nodes_become_outbounds() {
        let nodes = vec![
            reality_node(),
            base_node(
                Protocol::Trojan,
                ProtocolParams::Trojan { password: "pw".into() },
            ),
            base_node(
                Protocol::Shadowsocks,
                ProtocolParams::Shadowsocks {
                    method: "aes-256-gcm".into(),
                    password: "pw".into(),
                    plugin: None,
                    plugin_opts: None,
                },
            ),
        ];
        let cfg = gen(&nodes, &nodes[1].tag());
        // 3 nodes + direct
        let obs = cfg["outbounds"].as_array().expect("array");
        assert_eq!(obs.len(), 4);
    }

    #[test]
    fn hysteria2_gets_obfs_object() {
        let node = base_node(
            Protocol::Hysteria2,
            ProtocolParams::Hysteria2 {
                password: "pw".into(),
                obfs: Some("salamander".into()),
                obfs_password: Some("opw".into()),
            },
        );
        let cfg = gen(std::slice::from_ref(&node), &node.tag());
        assert_eq!(cfg["outbounds"][0]["obfs"]["type"], "salamander");
    }

    #[test]
    fn wireguard_becomes_endpoint() {
        let node = base_node(
            Protocol::Wireguard,
            ProtocolParams::Wireguard {
                private_key: "priv".into(),
                peer_public_key: "pub".into(),
                preshared_key: None,
                addresses: vec!["10.0.0.2/32".into()],
                reserved: Some(vec![1, 2, 3]),
                mtu: Some(1420),
            },
        );
        let cfg = gen(std::slice::from_ref(&node), &node.tag());
        let ep = &cfg["endpoints"][0];
        assert_eq!(ep["type"], "wireguard");
        assert_eq!(ep["peers"][0]["public_key"], "pub");
        assert_eq!(ep["peers"][0]["port"], 443);
        assert_eq!(cfg["route"]["final"], node.tag().as_str());
    }

    #[test]
    fn inbound_binds_loopback_unless_lan_allowed() {
        let node = reality_node();
        let cfg = gen(std::slice::from_ref(&node), &node.tag());
        assert_eq!(cfg["inbounds"][0]["listen"], "127.0.0.1");
        let cfg_lan = generate(&GenInput {
            nodes: std::slice::from_ref(&node),
            selected_tag: &node.tag(),
            local_port: Some(2080),
            allow_lan: true,
            clash_port: 9911,
            clash_secret: "s",
            log_level: "warn",
        })
        .expect("generate");
        assert_eq!(cfg_lan["inbounds"][0]["listen"], "0.0.0.0");
    }

    #[test]
    fn ephemeral_config_has_no_inbounds() {
        let node = reality_node();
        let cfg = generate(&GenInput {
            nodes: std::slice::from_ref(&node),
            selected_tag: &node.tag(),
            local_port: None,
            allow_lan: false,
            clash_port: 9911,
            clash_secret: "s",
            log_level: "warn",
        })
        .expect("generate");
        assert_eq!(cfg["inbounds"].as_array().map(|a| a.len()), Some(0));
    }

    #[test]
    fn bad_nonselected_node_is_skipped_selected_errors() {
        let mut xhttp = reality_node();
        xhttp.transport = Transport::Xhttp { path: "/".into(), host: None };
        xhttp.tls = TlsConfig::None;
        let good = base_node(
            Protocol::Trojan,
            ProtocolParams::Trojan { password: "pw".into() },
        );
        let nodes = vec![xhttp.clone(), good.clone()];
        // selected = good: xhttp silently skipped
        let cfg = gen(&nodes, &good.tag());
        assert_eq!(cfg["outbounds"].as_array().map(|a| a.len()), Some(2));
        // selected = xhttp: hard error
        assert!(generate(&GenInput {
            nodes: &nodes,
            selected_tag: &xhttp.tag(),
            local_port: Some(2080),
            allow_lan: false,
            clash_port: 9911,
            clash_secret: "s",
            log_level: "warn",
        })
        .is_err());
    }
}
