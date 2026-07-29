use url::Url;
use uuid::Uuid;

use super::common::{
    build_tls, build_transport, fragment_name, host_port, pct_decode, query_map,
};
use super::ParseError;
use crate::model::{Node, Protocol, ProtocolParams};

/// trojan://<password>@<host>:<port>?<query>#<name> — TLS by default.
pub fn parse(uri: &str) -> Result<Node, ParseError> {
    let url = Url::parse(uri).map_err(|_| ParseError::new("malformed trojan link"))?;
    let password = pct_decode(url.username())?;
    if password.is_empty() {
        return Err(ParseError::new("trojan link is missing the password"));
    }
    let (server, port) = host_port(&url, "trojan")?;
    let query = query_map(&url);
    let transport = build_transport(&query)?;
    let tls = build_tls(&query, &server, true)?;

    Ok(Node {
        id: Uuid::new_v4(),
        name: fragment_name(&url, format!("{server}:{port}")),
        protocol: Protocol::Trojan,
        server,
        port,
        params: ProtocolParams::Trojan { password },
        transport,
        tls,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{TlsConfig, Transport};

    #[test]
    fn parses_basic_trojan_with_default_tls() {
        let node = parse("trojan://s3cretpw@tr.example.com:443#Trojan%20DE").expect("parse");
        assert_eq!(node.name, "Trojan DE");
        match &node.params {
            ProtocolParams::Trojan { password } => assert_eq!(password, "s3cretpw"),
            other => panic!("wrong params {other:?}"),
        }
        match &node.tls {
            TlsConfig::Tls { sni, .. } => assert_eq!(sni.as_deref(), Some("tr.example.com")),
            other => panic!("expected tls, got {other:?}"),
        }
    }

    #[test]
    fn parses_ws_transport_and_sni() {
        let node = parse(
            "trojan://pw@tr.example.com:443?type=ws&path=%2Ftj&sni=cdn.example.org&allowInsecure=1#t",
        )
        .expect("parse");
        assert!(matches!(node.transport, Transport::Ws { .. }));
        match &node.tls {
            TlsConfig::Tls { sni, insecure, .. } => {
                assert_eq!(sni.as_deref(), Some("cdn.example.org"));
                assert!(*insecure);
            }
            other => panic!("expected tls, got {other:?}"),
        }
    }

    #[test]
    fn percent_encoded_password_is_decoded() {
        let node = parse("trojan://p%40ss%3Aword@h.example.com:443#x").expect("parse");
        match &node.params {
            ProtocolParams::Trojan { password } => assert_eq!(password, "p@ss:word"),
            other => panic!("wrong params {other:?}"),
        }
    }

    #[test]
    fn rejects_missing_password() {
        assert!(parse("trojan://@h.example.com:443").is_err());
    }
}
