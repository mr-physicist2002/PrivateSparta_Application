use serde::Serializer;

/// All user-facing failures. Messages follow the copy rules: say what failed
/// and what to do, never embed credentials or raw URIs.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Couldn't read the clipboard.")]
    Clipboard,
    #[error("{0}")]
    Parse(String),
    #[error("No server selected. Import one first.")]
    NoNodeSelected,
    #[error("{0}")]
    Config(String),
    #[error("{0}")]
    Core(String),
    #[error("Couldn't change the system proxy. {0}")]
    Proxy(String),
    #[error("Couldn't save settings. {0}")]
    Store(String),
    #[error("Disconnect before doing that.")]
    Busy,
}

/// Tauri commands serialize errors for the WebView; redact defensively even
/// though messages are constructed without credentials.
impl serde::Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&redact(&self.to_string()))
    }
}

const SECRET_QUERY_KEYS: [&str; 7] = [
    "pbk",
    "sid",
    "password",
    "pass",
    "key",
    "secret",
    "private_key",
];

/// Strip credential-shaped content from arbitrary text before it reaches a
/// log line or an error message: RFC-4122 UUIDs and known secret query params.
pub fn redact(input: &str) -> String {
    redact_query_secrets(&redact_uuids(input))
}

fn redact_uuids(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut skip_until = 0usize;
    for (i, ch) in input.char_indices() {
        if i < skip_until {
            continue;
        }
        if let Some(len) = uuid_len_at(bytes, i) {
            out.push_str("[redacted]");
            skip_until = i + len;
        } else {
            out.push(ch);
        }
    }
    out
}

/// Length of an RFC-4122-shaped substring (8-4-4-4-12 hex) starting at `i`.
fn uuid_len_at(b: &[u8], i: usize) -> Option<usize> {
    const GROUPS: [usize; 5] = [8, 4, 4, 4, 12];
    let mut pos = i;
    for (gi, &group) in GROUPS.iter().enumerate() {
        for _ in 0..group {
            if pos >= b.len() || !b[pos].is_ascii_hexdigit() {
                return None;
            }
            pos += 1;
        }
        if gi < GROUPS.len() - 1 {
            if pos >= b.len() || b[pos] != b'-' {
                return None;
            }
            pos += 1;
        }
    }
    Some(pos - i)
}

fn redact_query_secrets(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    'outer: while !rest.is_empty() {
        for key in SECRET_QUERY_KEYS {
            let pattern = format!("{key}=");
            if let Some(pos) = rest.find(&pattern) {
                // Only treat as a query param when preceded by ? & or start.
                let preceded_ok = pos == 0
                    || matches!(rest.as_bytes().get(pos - 1), Some(b'?' | b'&' | b' '));
                if preceded_ok {
                    let value_start = pos + pattern.len();
                    let value_end = rest[value_start..]
                        .find(['&', '#', ' '])
                        .map(|o| value_start + o)
                        .unwrap_or(rest.len());
                    out.push_str(&rest[..value_start]);
                    out.push_str("[redacted]");
                    rest = &rest[value_end..];
                    continue 'outer;
                }
            }
        }
        out.push_str(rest);
        break;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_uuid() {
        let s = "user 2f9a4b7c-1d2e-4f5a-8b9c-0d1e2f3a4b5c failed";
        assert_eq!(redact(s), "user [redacted] failed");
    }

    #[test]
    fn redacts_uuid_inside_uri() {
        let s = "vless://2f9a4b7c-1d2e-4f5a-8b9c-0d1e2f3a4b5c@host:443";
        assert_eq!(redact(s), "vless://[redacted]@host:443");
    }

    #[test]
    fn redacts_reality_public_key_param() {
        let s = "vless://x@h:1?security=reality&pbk=AbC123xyz&sni=example.com";
        assert_eq!(
            redact(s),
            "vless://x@h:1?security=reality&pbk=[redacted]&sni=example.com"
        );
    }

    #[test]
    fn leaves_ordinary_text_alone() {
        let s = "Couldn't reach the subscription server.";
        assert_eq!(redact(s), s);
    }

    #[test]
    fn handles_non_ascii() {
        let s = "سرور 2f9a4b7c-1d2e-4f5a-8b9c-0d1e2f3a4b5c قطع شد";
        assert_eq!(redact(s), "سرور [redacted] قطع شد");
    }
}
