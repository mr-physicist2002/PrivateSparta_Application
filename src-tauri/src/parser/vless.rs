use url::Url;
use uuid::Uuid;

use super::common::{
    build_tls, build_transport, fragment_name, host_port, pct_decode, query_map,
    reality_transport_ok,
};
use super::ParseError;
use crate::model::{Node, Protocol, ProtocolParams};

/// vless://<uuid>@<host>:<port>?<query>#<name>
/// REALITY params: security=reality, pbk, sid, fp, sni, flow, spx.
pub fn parse(uri: &str) -> Result<Node, ParseError> {
    let url = Url::parse(uri).map_err(|_| ParseError::new("malformed vless link"))?;

    let user_uuid = {
        let raw = url.username();
        if raw.is_empty() {
            return Err(ParseError::new("vless link is missing the user id"));
        }
        let decoded = pct_decode(raw)?;
        Uuid::parse_str(&decoded)
            .map_err(|_| ParseError::new("vless user id isn't a valid UUID"))?
            .to_string()
    };

    let (server, port) = host_port(&url, "vless")?;
    let query = query_map(&url);
    let transport = build_transport(&query)?;
    let tls = build_tls(&query, &server, false)?;
    if !reality_transport_ok(&tls, &transport) {
        return Err(ParseError::new(
            "REALITY only works with tcp, grpc, h2, or xhttp transports",
        ));
    }

    Ok(Node {
        id: Uuid::new_v4(),
        name: fragment_name(&url, format!("{server}:{port}")),
        protocol: Protocol::Vless,
        server,
        port,
        params: ProtocolParams::Vless {
            uuid: user_uuid,
            flow: query.get("flow").filter(|v| !v.is_empty()).cloned(),
        },
        transport,
        tls,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{TlsConfig, Transport};

    const UUID: &str = "2f9a4b7c-1d2e-4f5a-8b9c-0d1e2f3a4b5c";

    #[test]
    fn parses_reality_with_all_params() {
        let uri = format!(
            "vless://{UUID}@203.0.113.7:443?security=reality&encryption=none&pbk=SbVKOEMjK0sIlbwg4akyBg5mL5KZwwB-ed4eEE7YnRc&sid=6ba85179&spx=%2F&fp=chrome&sni=www.microsoft.com&flow=xtls-rprx-vision&type=tcp#DE%20Frankfurt%201"
        );
        let node = parse(&uri).expect("should parse");
        assert_eq!(node.name, "DE Frankfurt 1");
        assert_eq!(node.server, "203.0.113.7");
        assert_eq!(node.port, 443);
        match &node.params {
            ProtocolParams::Vless { uuid, flow } => {
                assert_eq!(uuid, UUID);
                assert_eq!(flow.as_deref(), Some("xtls-rprx-vision"));
            }
            other => panic!("wrong params {other:?}"),
        }
        match &node.tls {
            TlsConfig::Reality {
                sni,
                fingerprint,
                public_key,
                short_id,
                spider_x,
            } => {
                assert_eq!(sni.as_deref(), Some("www.microsoft.com"));
                assert_eq!(fingerprint, "chrome");
                assert_eq!(public_key, "SbVKOEMjK0sIlbwg4akyBg5mL5KZwwB-ed4eEE7YnRc");
                assert_eq!(short_id.as_deref(), Some("6ba85179"));
                assert_eq!(spider_x.as_deref(), Some("/"));
            }
            other => panic!("expected reality, got {other:?}"),
        }
        assert_eq!(node.transport, Transport::Tcp);
    }

    #[test]
    fn parses_ws_tls() {
        let uri = format!(
            "vless://{UUID}@cdn.example.com:443?security=tls&type=ws&path=%2Fws&host=cdn.example.com&sni=cdn.example.com&alpn=h2,http%2F1.1#WS%20node"
        );
        let node = parse(&uri).expect("should parse");
        match &node.transport {
            Transport::Ws { path, host } => {
                assert_eq!(path, "/ws");
                assert_eq!(host.as_deref(), Some("cdn.example.com"));
            }
            other => panic!("expected ws, got {other:?}"),
        }
        match &node.tls {
            TlsConfig::Tls { sni, alpn, .. } => {
                assert_eq!(sni.as_deref(), Some("cdn.example.com"));
                assert_eq!(alpn, &vec!["h2".to_string(), "http/1.1".to_string()]);
            }
            other => panic!("expected tls, got {other:?}"),
        }
    }

    #[test]
    fn parses_grpc_reality() {
        let uri = format!(
            "vless://{UUID}@198.51.100.3:8443?security=reality&pbk=key123&fp=firefox&sni=cloudflare.com&type=grpc&serviceName=grpcsvc#gRPC"
        );
        let node = parse(&uri).expect("should parse");
        assert_eq!(
            node.transport,
            Transport::Grpc {
                service_name: "grpcsvc".into()
            }
        );
    }

    #[test]
    fn parses_ipv6_host() {
        let uri = format!("vless://{UUID}@[2001:db8::1]:443?security=none#v6");
        let node = parse(&uri).expect("should parse");
        assert_eq!(node.server, "2001:db8::1");
    }

    #[test]
    fn defaults_sni_to_server_for_plain_tls() {
        let uri = format!("vless://{UUID}@example.com:443?security=tls");
        let node = parse(&uri).expect("should parse");
        match &node.tls {
            TlsConfig::Tls { sni, .. } => assert_eq!(sni.as_deref(), Some("example.com")),
            other => panic!("expected tls, got {other:?}"),
        }
    }

    #[test]
    fn rejects_missing_uuid() {
        assert!(parse("vless://@example.com:443").is_err());
    }

    #[test]
    fn rejects_non_uuid_user() {
        assert!(parse("vless://not-a-uuid@example.com:443").is_err());
    }

    #[test]
    fn rejects_missing_port() {
        assert!(parse(&format!("vless://{UUID}@example.com")).is_err());
    }

    #[test]
    fn rejects_reality_without_pbk() {
        let uri = format!("vless://{UUID}@example.com:443?security=reality");
        assert!(parse(&uri).is_err());
    }

    #[test]
    fn rejects_reality_over_ws() {
        let uri = format!("vless://{UUID}@example.com:443?security=reality&pbk=k&type=ws");
        assert!(parse(&uri).is_err());
    }

    #[test]
    fn parses_reality_xhttp_with_mode() {
        let uri = format!(
            "vless://{UUID}@example.com:443?security=reality&pbk=key&type=xhttp&path=%2F&mode=auto"
        );
        let node = parse(&uri).expect("xhttp should parse for the Xray fallback");
        assert_eq!(
            node.transport,
            Transport::Xhttp {
                path: "/".into(),
                host: None,
                mode: "auto".into(),
            }
        );
        assert!(matches!(node.tls, TlsConfig::Reality { .. }));
    }

    #[test]
    fn error_messages_never_contain_the_uuid() {
        let uri = format!("vless://{UUID}@example.com:443?security=reality");
        let err = parse(&uri).expect_err("should fail");
        assert!(!err.reason.contains(UUID));
    }
}
