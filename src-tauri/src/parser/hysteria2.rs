use url::Url;
use uuid::Uuid;

use super::common::{fragment_name, get, host_port, pct_decode, query_map};
use super::ParseError;
use crate::model::{Node, Protocol, ProtocolParams, TlsConfig, Transport};

/// hysteria2://<auth>@<host>:<port>/?sni=&insecure=1&obfs=salamander&obfs-password=pw#name
/// (hy2:// is an accepted alias; QUIC-native, always TLS.)
pub fn parse(uri: &str) -> Result<Node, ParseError> {
    let normalized = if let Some(rest) = uri.strip_prefix("hy2://") {
        format!("hysteria2://{rest}")
    } else {
        uri.to_string()
    };
    let url =
        Url::parse(&normalized).map_err(|_| ParseError::new("malformed hysteria2 link"))?;
    let password = pct_decode(url.username())?;
    if password.is_empty() {
        return Err(ParseError::new("hysteria2 link is missing the password"));
    }
    let (server, port) = host_port(&url, "hysteria2")?;
    let query = query_map(&url);

    let obfs = get(&query, "obfs");
    let obfs_password = get(&query, "obfs-password");
    if obfs.is_some() && obfs_password.is_none() {
        return Err(ParseError::new("hysteria2 obfs needs obfs-password"));
    }

    let tls = TlsConfig::Tls {
        sni: get(&query, "sni").or_else(|| Some(server.clone())),
        alpn: vec!["h3".into()],
        fingerprint: None,
        insecure: matches!(get(&query, "insecure").as_deref(), Some("1") | Some("true")),
    };

    Ok(Node {
        id: Uuid::new_v4(),
        name: fragment_name(&url, format!("{server}:{port}")),
        protocol: Protocol::Hysteria2,
        server,
        port,
        params: ProtocolParams::Hysteria2 {
            password,
            obfs,
            obfs_password,
        },
        transport: Transport::Tcp,
        tls,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hysteria2_with_obfs() {
        let node = parse(
            "hysteria2://authpw@hy.example.com:8443/?sni=hy.example.com&obfs=salamander&obfs-password=obfspw&insecure=0#HY2%20FI",
        )
        .expect("parse");
        assert_eq!(node.name, "HY2 FI");
        match &node.params {
            ProtocolParams::Hysteria2 { password, obfs, obfs_password } => {
                assert_eq!(password, "authpw");
                assert_eq!(obfs.as_deref(), Some("salamander"));
                assert_eq!(obfs_password.as_deref(), Some("obfspw"));
            }
            other => panic!("wrong params {other:?}"),
        }
    }

    #[test]
    fn hy2_alias_works() {
        let node = parse("hy2://pw@h.example.com:443#alias").expect("parse");
        assert_eq!(node.protocol, Protocol::Hysteria2);
        assert_eq!(node.name, "alias");
    }

    #[test]
    fn alpn_defaults_to_h3() {
        let node = parse("hy2://pw@h.example.com:443").expect("parse");
        match &node.tls {
            TlsConfig::Tls { alpn, .. } => assert_eq!(alpn, &vec!["h3".to_string()]),
            other => panic!("expected tls, got {other:?}"),
        }
    }

    #[test]
    fn rejects_obfs_without_password() {
        assert!(parse("hy2://pw@h.example.com:443/?obfs=salamander").is_err());
    }

    #[test]
    fn rejects_missing_auth() {
        assert!(parse("hysteria2://@h.example.com:443").is_err());
    }
}
