pub mod common;
mod hysteria2;
mod shadowsocks;
pub mod subscription;
mod trojan;
mod tuic;
mod vless;
mod vmess;
mod wireguard;

use crate::model::Node;

/// A parse failure never carries the raw URI — only the scheme and a reason.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{reason}")]
pub struct ParseError {
    pub reason: String,
}

impl ParseError {
    pub fn new(reason: impl Into<String>) -> Self {
        ParseError {
            reason: reason.into(),
        }
    }
}

/// Parse one share URI, any supported scheme.
pub fn parse_uri(uri: &str) -> Result<Node, ParseError> {
    let trimmed = uri.trim();
    if trimmed.is_empty() {
        return Err(ParseError::new("empty line"));
    }
    let scheme = trimmed
        .split("://")
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    match scheme.as_str() {
        "vless" => vless::parse(trimmed),
        "vmess" => vmess::parse(trimmed),
        "trojan" => trojan::parse(trimmed),
        "ss" => shadowsocks::parse(trimmed),
        "hysteria2" | "hy2" => hysteria2::parse(trimmed),
        "tuic" => tuic::parse(trimmed),
        "wireguard" | "wg" => wireguard::parse(trimmed),
        "" => Err(ParseError::new("not a server link")),
        other => Err(ParseError::new(format!(
            "{other}:// links aren't supported"
        ))),
    }
}

pub struct ParsedBatch {
    pub nodes: Vec<Node>,
    pub skipped: usize,
    /// First credential-safe parser reason, used to explain an all-failed
    /// clipboard import without ever returning the raw link to the WebView.
    pub first_error: Option<String>,
}

/// Parse clipboard/subscription-style text: URIs separated by newlines, or a
/// single base64 blob wrapping such a list. Malformed lines are counted,
/// never fatal.
pub fn parse_text(text: &str) -> ParsedBatch {
    let trimmed = text.trim();
    // A pasted base64 subscription payload has no scheme; try decoding it.
    if !trimmed.contains("://") {
        if let Some(decoded) = common::lenient_b64(trimmed)
            .and_then(|b| String::from_utf8(b).ok())
            .filter(|s| s.contains("://"))
        {
            return parse_lines(&decoded);
        }
    }
    parse_lines(trimmed)
}

fn parse_lines(text: &str) -> ParsedBatch {
    let mut nodes = Vec::new();
    let mut skipped = 0usize;
    let mut first_error = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match parse_uri(line) {
            Ok(node) => nodes.push(node),
            Err(err) => {
                skipped += 1;
                if first_error.is_none() {
                    first_error = Some(err.reason.clone());
                }
                tracing::debug!("skipped line: {}", err.reason);
            }
        }
    }
    ParsedBatch {
        nodes,
        skipped,
        first_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    #[test]
    fn all_schemes_dispatch() {
        let uris = [
            "vless://2f9a4b7c-1d2e-4f5a-8b9c-0d1e2f3a4b5c@a.example.com:443?security=tls#1",
            "trojan://pw@b.example.com:443#2",
            "hy2://pw@c.example.com:443#3",
            "tuic://2f9a4b7c-1d2e-4f5a-8b9c-0d1e2f3a4b5c:pw@d.example.com:443#4",
        ];
        for uri in uris {
            assert!(parse_uri(uri).is_ok(), "failed: {uri}");
        }
    }

    #[test]
    fn unknown_scheme_is_skipped_not_fatal() {
        let batch = parse_text("socks5://user@host:1080#name\n");
        assert_eq!(batch.nodes.len(), 0);
        assert_eq!(batch.skipped, 1);
        assert_eq!(
            batch.first_error.as_deref(),
            Some("socks5:// links aren't supported")
        );
    }

    #[test]
    fn garbage_never_panics() {
        for garbage in [
            "",
            "vless://",
            "vmess://",
            "ss://",
            "tuic://",
            "wireguard://",
            "vless://@:",
            "http://example.com",
            "just some words",
            "vless://\u{0}\u{ff}",
            "://missing",
        ] {
            let _ = parse_text(garbage);
        }
    }

    #[test]
    fn base64_blob_of_uris_is_decoded() {
        let list = "trojan://pw@x.example.com:443#a\nhy2://pw@y.example.com:443#b\n";
        let blob = STANDARD.encode(list);
        let batch = parse_text(&blob);
        assert_eq!(batch.nodes.len(), 2);
        assert_eq!(batch.skipped, 0);
        assert_eq!(batch.first_error, None);
    }

    #[test]
    fn mixed_batch_counts_correctly() {
        let text = "\
vless://2f9a4b7c-1d2e-4f5a-8b9c-0d1e2f3a4b5c@example.com:443?security=tls#ok

not-a-link
trojan://pw@other.net:8443#ok2
";
        let batch = parse_text(text);
        assert_eq!(batch.nodes.len(), 2);
        assert_eq!(batch.skipped, 1);
        assert_eq!(
            batch.first_error.as_deref(),
            Some("not-a-link:// links aren't supported")
        );
    }

    #[test]
    fn all_failed_import_keeps_the_first_safe_reason() {
        let text = "vless://2f9a4b7c-1d2e-4f5a-8b9c-0d1e2f3a4b5c@example.com:443?security=reality&pbk=key&type=xhttp\nnot-a-link";
        let batch = parse_text(text);
        assert!(batch.nodes.is_empty());
        assert_eq!(batch.skipped, 2);
        assert_eq!(
            batch.first_error.as_deref(),
            Some("xhttp transport isn't supported by the bundled tunnel core")
        );
        assert!(!batch.first_error.unwrap().contains("2f9a4b7c"));
    }
}
