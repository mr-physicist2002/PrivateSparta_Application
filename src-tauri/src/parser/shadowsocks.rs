use url::Url;
use uuid::Uuid;

use super::common::{fragment_name, host_port, lenient_b64, pct_decode, query_map};
use super::ParseError;
use crate::model::{Node, Protocol, ProtocolParams, TlsConfig, Transport};

/// SIP002: ss://BASE64URL(method:password)@host:port/?plugin=...#name
///         ss://method:password@host:port (percent-encoded, SS2022 style)
/// Legacy: ss://BASE64(method:password@host:port)#name
pub fn parse(uri: &str) -> Result<Node, ParseError> {
    let body = uri
        .strip_prefix("ss://")
        .ok_or_else(|| ParseError::new("not an ss link"))?;
    let before_fragment = body.split('#').next().unwrap_or(body);
    if before_fragment.contains('@') {
        parse_sip002(uri)
    } else {
        parse_legacy(body)
    }
}

fn parse_sip002(uri: &str) -> Result<Node, ParseError> {
    let url = Url::parse(uri).map_err(|_| ParseError::new("malformed ss link"))?;
    let userinfo = {
        let user = pct_decode(url.username())?;
        let pass = url.password().map(pct_decode).transpose()?;
        match pass {
            Some(pass) => format!("{user}:{pass}"),
            None => user,
        }
    };
    if userinfo.is_empty() {
        return Err(ParseError::new("ss link is missing credentials"));
    }
    // Base64 userinfo has no ':' before decoding; plain (SS2022) does.
    let (method, password) = if userinfo.contains(':') {
        split_method_password(&userinfo)?
    } else {
        let decoded = lenient_b64(&userinfo)
            .and_then(|b| String::from_utf8(b).ok())
            .ok_or_else(|| ParseError::new("ss credentials aren't valid base64"))?;
        split_method_password(&decoded)?
    };
    let (server, port) = host_port(&url, "ss")?;
    let query = query_map(&url);
    let (plugin, plugin_opts) = match query.get("plugin").filter(|p| !p.is_empty()) {
        Some(raw) => {
            let mut parts = raw.splitn(2, ';');
            let name = parts.next().unwrap_or("").to_string();
            let opts = parts.next().map(|s| s.to_string());
            (Some(name), opts)
        }
        None => (None, None),
    };

    Ok(Node {
        id: Uuid::new_v4(),
        name: fragment_name(&url, format!("{server}:{port}")),
        protocol: Protocol::Shadowsocks,
        server,
        port,
        params: ProtocolParams::Shadowsocks {
            method,
            password,
            plugin,
            plugin_opts,
        },
        transport: Transport::Tcp,
        tls: TlsConfig::None,
    })
}

fn parse_legacy(body: &str) -> Result<Node, ParseError> {
    let (encoded, fragment) = match body.split_once('#') {
        Some((e, f)) => (e, Some(f)),
        None => (body, None),
    };
    let decoded = lenient_b64(encoded)
        .and_then(|b| String::from_utf8(b).ok())
        .ok_or_else(|| ParseError::new("legacy ss link isn't valid base64"))?;
    // method:password@host:port — password may itself contain ':' or '@',
    // so split on the LAST '@'.
    let (creds, addr) = decoded
        .rsplit_once('@')
        .ok_or_else(|| ParseError::new("legacy ss link is missing the server"))?;
    let (method, password) = split_method_password(creds)?;
    let (server, port_str) = addr
        .rsplit_once(':')
        .ok_or_else(|| ParseError::new("legacy ss link is missing the port"))?;
    let port: u16 = port_str
        .parse()
        .map_err(|_| ParseError::new("legacy ss link has a bad port"))?;
    let server = server.trim_matches(['[', ']']).to_string();
    let name = fragment
        .and_then(|f| percent_encoding::percent_decode_str(f).decode_utf8().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{server}:{port}"));

    Ok(Node {
        id: Uuid::new_v4(),
        name,
        protocol: Protocol::Shadowsocks,
        server,
        port,
        params: ProtocolParams::Shadowsocks {
            method,
            password,
            plugin: None,
            plugin_opts: None,
        },
        transport: Transport::Tcp,
        tls: TlsConfig::None,
    })
}

fn split_method_password(input: &str) -> Result<(String, String), ParseError> {
    let (method, password) = input
        .split_once(':')
        .ok_or_else(|| ParseError::new("ss credentials are missing the cipher"))?;
    if method.is_empty() || password.is_empty() {
        return Err(ParseError::new("ss credentials are incomplete"));
    }
    Ok((method.to_string(), password.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
    use base64::Engine;

    #[test]
    fn parses_sip002_base64_userinfo() {
        let creds = URL_SAFE_NO_PAD.encode("aes-256-gcm:barfoo!");
        let uri = format!("ss://{creds}@ss.example.com:8388#SS%20node");
        let node = parse(&uri).expect("parse");
        assert_eq!(node.name, "SS node");
        match &node.params {
            ProtocolParams::Shadowsocks { method, password, .. } => {
                assert_eq!(method, "aes-256-gcm");
                assert_eq!(password, "barfoo!");
            }
            other => panic!("wrong params {other:?}"),
        }
    }

    #[test]
    fn parses_sip002_plain_ss2022() {
        let uri =
            "ss://2022-blake3-aes-256-gcm:cGFzc3dvcmRwYXNzd29yZA%3D%3D@h.example.com:8388#s22";
        let node = parse(uri).expect("parse");
        match &node.params {
            ProtocolParams::Shadowsocks { method, password, .. } => {
                assert_eq!(method, "2022-blake3-aes-256-gcm");
                assert_eq!(password, "cGFzc3dvcmRwYXNzd29yZA==");
            }
            other => panic!("wrong params {other:?}"),
        }
    }

    #[test]
    fn parses_sip002_plugin() {
        let creds = URL_SAFE_NO_PAD.encode("chacha20-ietf-poly1305:pw");
        let uri = format!(
            "ss://{creds}@h.example.com:443/?plugin=v2ray-plugin%3Btls%3Bhost%3Dcdn.example.com#p"
        );
        let node = parse(&uri).expect("parse");
        match &node.params {
            ProtocolParams::Shadowsocks { plugin, plugin_opts, .. } => {
                assert_eq!(plugin.as_deref(), Some("v2ray-plugin"));
                assert_eq!(plugin_opts.as_deref(), Some("tls;host=cdn.example.com"));
            }
            other => panic!("wrong params {other:?}"),
        }
    }

    #[test]
    fn parses_legacy_base64_whole_uri() {
        let inner = "aes-128-gcm:pass@word@legacy.example.com:8388";
        let uri = format!("ss://{}#Legacy", STANDARD.encode(inner));
        let node = parse(&uri).expect("parse");
        assert_eq!(node.server, "legacy.example.com");
        assert_eq!(node.port, 8388);
        match &node.params {
            ProtocolParams::Shadowsocks { method, password, .. } => {
                assert_eq!(method, "aes-128-gcm");
                assert_eq!(password, "pass@word");
            }
            other => panic!("wrong params {other:?}"),
        }
    }

    #[test]
    fn rejects_junk() {
        assert!(parse("ss://%%%").is_err());
        assert!(parse("ss://").is_err());
    }
}
