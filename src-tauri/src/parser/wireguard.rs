use url::Url;
use uuid::Uuid;

use super::common::{fragment_name, get, host_port, pct_decode, query_map};
use super::ParseError;
use crate::model::{Node, Protocol, ProtocolParams, TlsConfig, Transport};

/// wireguard://<privatekey>@<host>:<port>?address=10.0.0.2/32,fd00::2/128
///            &publickey=...&presharedkey=...&mtu=1420&reserved=1,2,3#name
pub fn parse(uri: &str) -> Result<Node, ParseError> {
    let url = Url::parse(uri).map_err(|_| ParseError::new("malformed wireguard link"))?;
    let private_key = pct_decode(url.username())?;
    if private_key.is_empty() {
        return Err(ParseError::new("wireguard link is missing the private key"));
    }
    let (server, port) = host_port(&url, "wireguard")?;
    let query = query_map(&url);

    let peer_public_key = get(&query, "publickey")
        .ok_or_else(|| ParseError::new("wireguard link is missing publickey"))?;
    let addresses: Vec<String> = get(&query, "address")
        .map(|a| a.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();
    if addresses.is_empty() {
        return Err(ParseError::new("wireguard link is missing address"));
    }
    let reserved = match get(&query, "reserved") {
        Some(raw) => {
            let parsed: Result<Vec<u8>, _> =
                raw.split(',').map(|s| s.trim().parse::<u8>()).collect();
            Some(parsed.map_err(|_| ParseError::new("wireguard reserved must be bytes"))?)
        }
        None => None,
    };
    let mtu = match get(&query, "mtu") {
        Some(raw) => Some(
            raw.parse::<u16>()
                .map_err(|_| ParseError::new("wireguard mtu isn't a number"))?,
        ),
        None => None,
    };

    Ok(Node {
        id: Uuid::new_v4(),
        name: fragment_name(&url, format!("{server}:{port}")),
        protocol: Protocol::Wireguard,
        server,
        port,
        params: ProtocolParams::Wireguard {
            private_key,
            peer_public_key,
            preshared_key: get(&query, "presharedkey"),
            addresses,
            reserved,
            mtu,
        },
        transport: Transport::Tcp,
        tls: TlsConfig::None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wireguard_full() {
        let uri = "wireguard://cHJpdmF0ZWtleWJhc2U2NGVuY29kZWQ%3D@wg.example.com:51820?address=10.66.66.2%2F32,fd42%3A%3A2%2F128&publickey=cHVibGlja2V5&presharedkey=cHNr&mtu=1420&reserved=78,12,199#WG%20US";
        let node = parse(uri).expect("parse");
        assert_eq!(node.name, "WG US");
        assert_eq!(node.port, 51820);
        match &node.params {
            ProtocolParams::Wireguard {
                private_key,
                peer_public_key,
                preshared_key,
                addresses,
                reserved,
                mtu,
            } => {
                assert_eq!(private_key, "cHJpdmF0ZWtleWJhc2U2NGVuY29kZWQ=");
                assert_eq!(peer_public_key, "cHVibGlja2V5");
                assert_eq!(preshared_key.as_deref(), Some("cHNr"));
                assert_eq!(addresses, &vec!["10.66.66.2/32".to_string(), "fd42::2/128".to_string()]);
                assert_eq!(reserved.as_deref(), Some(&[78u8, 12, 199][..]));
                assert_eq!(*mtu, Some(1420));
            }
            other => panic!("wrong params {other:?}"),
        }
    }

    #[test]
    fn rejects_missing_publickey() {
        assert!(parse("wireguard://priv@h.example.com:51820?address=10.0.0.2/32").is_err());
    }

    #[test]
    fn rejects_missing_address() {
        assert!(parse("wireguard://priv@h.example.com:51820?publickey=pub").is_err());
    }
}
