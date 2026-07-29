mod vless;

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

/// Parse one share URI. Phase 1 supports vless:// (incl. REALITY); the other
/// schemes land in Phase 2 behind this same entry point.
pub fn parse_uri(uri: &str) -> Result<Node, ParseError> {
    let trimmed = uri.trim();
    if trimmed.is_empty() {
        return Err(ParseError::new("empty line"));
    }
    let scheme = trimmed.split("://").next().unwrap_or("").to_ascii_lowercase();
    match scheme.as_str() {
        "vless" => vless::parse(trimmed),
        "" => Err(ParseError::new("not a server link")),
        other => Err(ParseError::new(format!(
            "{other}:// links aren't supported yet"
        ))),
    }
}

pub struct ParsedBatch {
    pub nodes: Vec<Node>,
    pub skipped: usize,
}

/// Parse clipboard-style text: one or more URIs separated by newlines.
/// Malformed lines are counted, never fatal.
pub fn parse_text(text: &str) -> ParsedBatch {
    let mut nodes = Vec::new();
    let mut skipped = 0usize;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match parse_uri(line) {
            Ok(node) => nodes.push(node),
            Err(err) => {
                skipped += 1;
                tracing::debug!("skipped clipboard line: {}", err.reason);
            }
        }
    }
    ParsedBatch { nodes, skipped }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_scheme_is_skipped_not_fatal() {
        let batch = parse_text("trojan://secret@host:443#name\n");
        assert_eq!(batch.nodes.len(), 0);
        assert_eq!(batch.skipped, 1);
    }

    #[test]
    fn garbage_never_panics() {
        for garbage in [
            "",
            "vless://",
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
    fn mixed_batch_counts_correctly() {
        let text = "\
vless://2f9a4b7c-1d2e-4f5a-8b9c-0d1e2f3a4b5c@example.com:443?security=tls#ok

not-a-link
vless://2f9a4b7c-1d2e-4f5a-8b9c-0d1e2f3a4b5c@other.net:8443?security=none#ok2
";
        let batch = parse_text(text);
        assert_eq!(batch.nodes.len(), 2);
        assert_eq!(batch.skipped, 1);
    }
}
