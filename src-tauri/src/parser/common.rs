use std::collections::HashMap;

use base64::engine::general_purpose::{
    STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD,
};
use base64::Engine;
use percent_encoding::percent_decode_str;
use url::Url;

use super::ParseError;
use crate::model::{TlsConfig, Transport};

/// Decode base64 accepting standard/url-safe alphabets with or without
/// padding, tolerating surrounding whitespace. Share links are sloppy.
pub fn lenient_b64(input: &str) -> Option<Vec<u8>> {
    let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();
    for engine in [&STANDARD, &STANDARD_NO_PAD, &URL_SAFE, &URL_SAFE_NO_PAD] {
        if let Ok(bytes) = engine.decode(&cleaned) {
            return Some(bytes);
        }
    }
    None
}

pub fn pct_decode(input: &str) -> Result<String, ParseError> {
    percent_decode_str(input)
        .decode_utf8()
        .map(|s| s.into_owned())
        .map_err(|_| ParseError::new("link contains invalid text"))
}

pub fn query_map(url: &Url) -> HashMap<String, String> {
    url.query_pairs()
        .map(|(k, v)| (k.to_ascii_lowercase(), v.into_owned()))
        .collect()
}

pub fn get(query: &HashMap<String, String>, key: &str) -> Option<String> {
    query.get(key).filter(|v| !v.is_empty()).cloned()
}

pub fn host_port(url: &Url, scheme: &str) -> Result<(String, u16), ParseError> {
    let server = url
        .host_str()
        .ok_or_else(|| ParseError::new(format!("{scheme} link is missing the server address")))?
        .trim_matches(['[', ']'])
        .to_string();
    let port = url
        .port()
        .ok_or_else(|| ParseError::new(format!("{scheme} link is missing the port")))?;
    Ok((server, port))
}

pub fn fragment_name(url: &Url, fallback: String) -> String {
    url.fragment()
        .and_then(|f| percent_decode_str(f).decode_utf8().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback)
}

pub fn build_transport(query: &HashMap<String, String>) -> Result<Transport, ParseError> {
    let kind = get(query, "type")
        .unwrap_or_else(|| "tcp".into())
        .to_ascii_lowercase();
    let path = get(query, "path").unwrap_or_else(|| "/".into());
    let host = get(query, "host");
    Ok(match kind.as_str() {
        "tcp" | "raw" | "none" => Transport::Tcp,
        "ws" => Transport::Ws { path, host },
        "grpc" => Transport::Grpc {
            service_name: get(query, "servicename").unwrap_or_default(),
        },
        "httpupgrade" => Transport::HttpUpgrade { path, host },
        "xhttp" | "splithttp" => Transport::Xhttp { path, host },
        "h2" | "http" => Transport::H2 { path, host },
        other => {
            return Err(ParseError::new(format!(
                "unsupported transport \"{other}\""
            )))
        }
    })
}

/// `default_tls`: trojan-style links are TLS unless stated otherwise.
pub fn build_tls(
    query: &HashMap<String, String>,
    server: &str,
    default_tls: bool,
) -> Result<TlsConfig, ParseError> {
    let security = get(query, "security")
        .unwrap_or_else(|| if default_tls { "tls".into() } else { "none".into() })
        .to_ascii_lowercase();
    Ok(match security.as_str() {
        "none" => TlsConfig::None,
        "tls" => TlsConfig::Tls {
            sni: get(query, "sni")
                .or_else(|| get(query, "peer"))
                .or_else(|| Some(server.to_string())),
            alpn: get(query, "alpn")
                .map(|a| a.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_default(),
            fingerprint: get(query, "fp"),
            insecure: matches!(
                get(query, "allowinsecure")
                    .or_else(|| get(query, "insecure"))
                    .as_deref(),
                Some("1") | Some("true")
            ),
        },
        "reality" => TlsConfig::Reality {
            sni: get(query, "sni"),
            fingerprint: get(query, "fp").unwrap_or_else(|| "chrome".into()),
            public_key: get(query, "pbk")
                .ok_or_else(|| ParseError::new("REALITY link is missing pbk"))?,
            short_id: get(query, "sid"),
            spider_x: get(query, "spx"),
        },
        other => {
            return Err(ParseError::new(format!(
                "unsupported security \"{other}\""
            )))
        }
    })
}

pub fn reality_transport_ok(tls: &TlsConfig, transport: &Transport) -> bool {
    !matches!(tls, TlsConfig::Reality { .. })
        || matches!(
            transport,
            Transport::Tcp | Transport::Grpc { .. } | Transport::H2 { .. }
        )
}
