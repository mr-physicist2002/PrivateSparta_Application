use url::Url;
use uuid::Uuid;

use super::common::{fragment_name, get, host_port, pct_decode, query_map};
use super::ParseError;
use crate::model::{Node, Protocol, ProtocolParams, TlsConfig, Transport};

/// TUIC v5: tuic://<uuid>:<password>@<host>:<port>?congestion_control=bbr
///          &udp_relay_mode=native&alpn=h3&sni=&allow_insecure=1#name
pub fn parse(uri: &str) -> Result<Node, ParseError> {
    let url = Url::parse(uri).map_err(|_| ParseError::new("malformed tuic link"))?;
    let user_uuid = {
        let raw = pct_decode(url.username())?;
        if raw.is_empty() {
            return Err(ParseError::new("tuic link is missing the uuid"));
        }
        Uuid::parse_str(&raw)
            .map_err(|_| ParseError::new("tuic uuid isn't a valid UUID"))?
            .to_string()
    };
    let password = url
        .password()
        .map(pct_decode)
        .transpose()?
        .ok_or_else(|| ParseError::new("tuic link is missing the password"))?;
    let (server, port) = host_port(&url, "tuic")?;
    let query = query_map(&url);

    let tls = TlsConfig::Tls {
        sni: get(&query, "sni").or_else(|| Some(server.clone())),
        alpn: get(&query, "alpn")
            .map(|a| a.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_else(|| vec!["h3".into()]),
        fingerprint: None,
        insecure: matches!(
            get(&query, "allow_insecure")
                .or_else(|| get(&query, "allowinsecure"))
                .as_deref(),
            Some("1") | Some("true")
        ),
    };

    Ok(Node {
        id: Uuid::new_v4(),
        name: fragment_name(&url, format!("{server}:{port}")),
        protocol: Protocol::Tuic,
        server,
        port,
        params: ProtocolParams::Tuic {
            uuid: user_uuid,
            password,
            congestion_control: get(&query, "congestion_control"),
            udp_relay_mode: get(&query, "udp_relay_mode"),
        },
        transport: Transport::Tcp,
        tls,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "2f9a4b7c-1d2e-4f5a-8b9c-0d1e2f3a4b5c";

    #[test]
    fn parses_tuic_v5_full() {
        let uri = format!(
            "tuic://{UUID}:tuicpass@tu.example.com:443?congestion_control=bbr&udp_relay_mode=native&alpn=h3&sni=tu.example.com#TUIC%20NL"
        );
        let node = parse(&uri).expect("parse");
        assert_eq!(node.name, "TUIC NL");
        match &node.params {
            ProtocolParams::Tuic { uuid, password, congestion_control, udp_relay_mode } => {
                assert_eq!(uuid, UUID);
                assert_eq!(password, "tuicpass");
                assert_eq!(congestion_control.as_deref(), Some("bbr"));
                assert_eq!(udp_relay_mode.as_deref(), Some("native"));
            }
            other => panic!("wrong params {other:?}"),
        }
    }

    #[test]
    fn rejects_missing_password() {
        assert!(parse(&format!("tuic://{UUID}@h.example.com:443")).is_err());
    }

    #[test]
    fn rejects_bad_uuid() {
        assert!(parse("tuic://nope:pw@h.example.com:443").is_err());
    }
}
