use serde_json::Value;
use uuid::Uuid;

use super::common::lenient_b64;
use super::ParseError;
use crate::model::{Node, Protocol, ProtocolParams, TlsConfig, Transport};

/// vmess://BASE64(v2rayN JSON): {"v":"2","ps":name,"add":host,"port":p,"id":uuid,
/// "aid":n,"scy":cipher,"net":transport,"host":h,"path":p,"tls":"tls","sni":s,...}
/// Fields arrive as strings or numbers depending on the exporting panel.
pub fn parse(uri: &str) -> Result<Node, ParseError> {
    let body = uri
        .strip_prefix("vmess://")
        .ok_or_else(|| ParseError::new("not a vmess link"))?;
    let bytes =
        lenient_b64(body).ok_or_else(|| ParseError::new("vmess link isn't valid base64"))?;
    let json: Value = serde_json::from_slice(&bytes)
        .map_err(|_| ParseError::new("vmess link doesn't contain valid JSON"))?;

    let str_field = |key: &str| -> Option<String> {
        match json.get(key) {
            Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
            Some(Value::Number(n)) => Some(n.to_string()),
            _ => None,
        }
    };

    let server =
        str_field("add").ok_or_else(|| ParseError::new("vmess link is missing the server"))?;
    let port: u16 = str_field("port")
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| ParseError::new("vmess link has a bad port"))?;
    let id = str_field("id").ok_or_else(|| ParseError::new("vmess link is missing the id"))?;
    let uuid = Uuid::parse_str(&id)
        .map_err(|_| ParseError::new("vmess id isn't a valid UUID"))?
        .to_string();
    let alter_id: u32 = str_field("aid").and_then(|a| a.parse().ok()).unwrap_or(0);
    let security = str_field("scy").unwrap_or_else(|| "auto".into());

    let net = str_field("net").unwrap_or_else(|| "tcp".into());
    let path = str_field("path").unwrap_or_else(|| "/".into());
    let host = str_field("host");
    let transport = match net.as_str() {
        "tcp" | "none" | "" => Transport::Tcp,
        "ws" => Transport::Ws { path, host },
        "grpc" => Transport::Grpc {
            // v2rayN puts the gRPC service name in `path`
            service_name: if path == "/" { String::new() } else { path },
        },
        "httpupgrade" => Transport::HttpUpgrade { path, host },
        "xhttp" | "splithttp" => Transport::Xhttp { path, host },
        "h2" | "http" => Transport::H2 { path, host },
        other => return Err(ParseError::new(format!("unsupported transport \"{other}\""))),
    };

    let tls = match str_field("tls").as_deref() {
        Some("tls") => TlsConfig::Tls {
            sni: str_field("sni").or_else(|| str_field("host")).or_else(|| Some(server.clone())),
            alpn: str_field("alpn")
                .map(|a| a.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default(),
            fingerprint: str_field("fp"),
            insecure: false,
        },
        _ => TlsConfig::None,
    };

    let name = str_field("ps").unwrap_or_else(|| format!("{server}:{port}"));

    Ok(Node {
        id: Uuid::new_v4(),
        name,
        protocol: Protocol::Vmess,
        server,
        port,
        params: ProtocolParams::Vmess {
            uuid,
            alter_id,
            security,
        },
        transport,
        tls,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    fn encode(json: &str) -> String {
        format!("vmess://{}", STANDARD.encode(json))
    }

    #[test]
    fn parses_v2rayn_ws_tls() {
        let uri = encode(
            r#"{"v":"2","ps":"US West","add":"us1.example.com","port":"443","id":"2f9a4b7c-1d2e-4f5a-8b9c-0d1e2f3a4b5c","aid":"0","scy":"auto","net":"ws","type":"none","host":"us1.example.com","path":"/vmws","tls":"tls","sni":"us1.example.com"}"#,
        );
        let node = parse(&uri).expect("should parse");
        assert_eq!(node.name, "US West");
        assert_eq!(node.port, 443);
        match &node.params {
            ProtocolParams::Vmess { alter_id, security, .. } => {
                assert_eq!(*alter_id, 0);
                assert_eq!(security, "auto");
            }
            other => panic!("wrong params {other:?}"),
        }
        assert!(matches!(node.transport, Transport::Ws { .. }));
        assert!(matches!(node.tls, TlsConfig::Tls { .. }));
    }

    #[test]
    fn accepts_numeric_port_and_aid() {
        let uri = encode(
            r#"{"v":2,"ps":"n","add":"h.example.com","port":8443,"id":"2f9a4b7c-1d2e-4f5a-8b9c-0d1e2f3a4b5c","aid":64,"net":"tcp"}"#,
        );
        let node = parse(&uri).expect("should parse");
        assert_eq!(node.port, 8443);
        match &node.params {
            ProtocolParams::Vmess { alter_id, .. } => assert_eq!(*alter_id, 64),
            other => panic!("wrong params {other:?}"),
        }
        assert_eq!(node.tls, TlsConfig::None);
    }

    #[test]
    fn grpc_service_name_comes_from_path() {
        let uri = encode(
            r#"{"ps":"g","add":"h.example.com","port":"443","id":"2f9a4b7c-1d2e-4f5a-8b9c-0d1e2f3a4b5c","net":"grpc","path":"svc","tls":"tls"}"#,
        );
        let node = parse(&uri).expect("should parse");
        assert_eq!(
            node.transport,
            Transport::Grpc {
                service_name: "svc".into()
            }
        );
    }

    #[test]
    fn rejects_garbage_base64() {
        assert!(parse("vmess://!!!not-base64!!!").is_err());
    }

    #[test]
    fn rejects_non_json_payload() {
        let uri = format!("vmess://{}", STANDARD.encode("hello world"));
        assert!(parse(&uri).is_err());
    }
}
