//! Rebuild share URIs from stored nodes (for "copy link"). The inverse of the
//! parsers; the resulting URI carries credentials, so it is written to the
//! clipboard in Rust and never returned across IPC.

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde_json::json;

use crate::model::{Node, ProtocolParams, TlsConfig, Transport};

fn enc(input: &str) -> String {
    utf8_percent_encode(input, NON_ALPHANUMERIC).to_string()
}

fn push_transport(query: &mut Vec<(String, String)>, transport: &Transport) {
    let mut add = |k: &str, v: &str| query.push((k.into(), v.into()));
    match transport {
        Transport::Tcp => add("type", "tcp"),
        Transport::Ws { path, host } => {
            add("type", "ws");
            add("path", path);
            if let Some(host) = host {
                add("host", host);
            }
        }
        Transport::Grpc { service_name } => {
            add("type", "grpc");
            add("serviceName", service_name);
        }
        Transport::HttpUpgrade { path, host } => {
            add("type", "httpupgrade");
            add("path", path);
            if let Some(host) = host {
                add("host", host);
            }
        }
        Transport::Xhttp { path, host, mode } => {
            add("type", "xhttp");
            add("path", path);
            add("mode", mode);
            if let Some(host) = host {
                add("host", host);
            }
        }
        Transport::H2 { path, host } => {
            add("type", "h2");
            add("path", path);
            if let Some(host) = host {
                add("host", host);
            }
        }
    }
}

fn push_tls(query: &mut Vec<(String, String)>, tls: &TlsConfig) {
    let mut add = |k: &str, v: String| query.push((k.into(), v));
    match tls {
        TlsConfig::None => add("security", "none".into()),
        TlsConfig::Tls {
            sni,
            alpn,
            fingerprint,
            insecure,
        } => {
            add("security", "tls".into());
            if let Some(sni) = sni {
                add("sni", sni.clone());
            }
            if !alpn.is_empty() {
                add("alpn", alpn.join(","));
            }
            if let Some(fp) = fingerprint {
                add("fp", fp.clone());
            }
            if *insecure {
                add("allowInsecure", "1".into());
            }
        }
        TlsConfig::Reality {
            sni,
            fingerprint,
            public_key,
            short_id,
            spider_x,
        } => {
            add("security", "reality".into());
            if let Some(sni) = sni {
                add("sni", sni.clone());
            }
            add("fp", fingerprint.clone());
            add("pbk", public_key.clone());
            if let Some(sid) = short_id {
                add("sid", sid.clone());
            }
            if let Some(spx) = spider_x {
                add("spx", spx.clone());
            }
        }
    }
}

fn query_string(query: &[(String, String)]) -> String {
    query
        .iter()
        .map(|(k, v)| format!("{k}={}", enc(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn host_for_uri(server: &str) -> String {
    if server.contains(':') {
        format!("[{server}]")
    } else {
        server.to_string()
    }
}

pub fn export_uri(node: &Node) -> String {
    let host = host_for_uri(&node.server);
    let port = node.port;
    let name = enc(&node.name);
    match &node.params {
        ProtocolParams::Vless { uuid, flow } => {
            let mut query = Vec::new();
            if let Some(flow) = flow {
                query.push(("flow".into(), flow.clone()));
            }
            push_tls(&mut query, &node.tls);
            push_transport(&mut query, &node.transport);
            format!("vless://{uuid}@{host}:{port}?{}#{name}", query_string(&query))
        }
        ProtocolParams::Vmess {
            uuid,
            alter_id,
            security,
        } => {
            let (net, path, ws_host) = match &node.transport {
                Transport::Tcp => ("tcp", String::new(), None),
                Transport::Ws { path, host } => ("ws", path.clone(), host.clone()),
                Transport::Grpc { service_name } => ("grpc", service_name.clone(), None),
                Transport::HttpUpgrade { path, host } => {
                    ("httpupgrade", path.clone(), host.clone())
                }
                Transport::Xhttp { path, host, .. } => ("xhttp", path.clone(), host.clone()),
                Transport::H2 { path, host } => ("h2", path.clone(), host.clone()),
            };
            let (tls, sni) = match &node.tls {
                TlsConfig::Tls { sni, .. } => ("tls", sni.clone()),
                _ => ("", None),
            };
            let body = json!({
                "v": "2",
                "ps": node.name,
                "add": node.server,
                "port": port.to_string(),
                "id": uuid,
                "aid": alter_id.to_string(),
                "scy": security,
                "net": net,
                "type": "none",
                "host": ws_host.unwrap_or_default(),
                "path": path,
                "tls": tls,
                "sni": sni.unwrap_or_default(),
            });
            format!("vmess://{}", STANDARD.encode(body.to_string()))
        }
        ProtocolParams::Trojan { password } => {
            let mut query = Vec::new();
            push_tls(&mut query, &node.tls);
            push_transport(&mut query, &node.transport);
            format!(
                "trojan://{}@{host}:{port}?{}#{name}",
                enc(password),
                query_string(&query)
            )
        }
        ProtocolParams::Shadowsocks {
            method,
            password,
            plugin,
            plugin_opts,
        } => {
            let creds = URL_SAFE_NO_PAD.encode(format!("{method}:{password}"));
            let mut uri = format!("ss://{creds}@{host}:{port}");
            if let Some(plugin) = plugin {
                let mut value = plugin.clone();
                if let Some(opts) = plugin_opts {
                    value = format!("{value};{opts}");
                }
                uri.push_str(&format!("/?plugin={}", enc(&value)));
            }
            format!("{uri}#{name}")
        }
        ProtocolParams::Hysteria2 {
            password,
            obfs,
            obfs_password,
        } => {
            let mut query = Vec::new();
            if let TlsConfig::Tls { sni, insecure, .. } = &node.tls {
                if let Some(sni) = sni {
                    query.push(("sni".into(), sni.clone()));
                }
                if *insecure {
                    query.push(("insecure".into(), "1".into()));
                }
            }
            if let (Some(obfs), Some(opw)) = (obfs, obfs_password) {
                query.push(("obfs".into(), obfs.clone()));
                query.push(("obfs-password".into(), opw.clone()));
            }
            format!(
                "hysteria2://{}@{host}:{port}/?{}#{name}",
                enc(password),
                query_string(&query)
            )
        }
        ProtocolParams::Tuic {
            uuid,
            password,
            congestion_control,
            udp_relay_mode,
        } => {
            let mut query = Vec::new();
            if let Some(cc) = congestion_control {
                query.push(("congestion_control".into(), cc.clone()));
            }
            if let Some(mode) = udp_relay_mode {
                query.push(("udp_relay_mode".into(), mode.clone()));
            }
            if let TlsConfig::Tls { sni, alpn, .. } = &node.tls {
                if let Some(sni) = sni {
                    query.push(("sni".into(), sni.clone()));
                }
                if !alpn.is_empty() {
                    query.push(("alpn".into(), alpn.join(",")));
                }
            }
            format!(
                "tuic://{uuid}:{}@{host}:{port}?{}#{name}",
                enc(password),
                query_string(&query)
            )
        }
        ProtocolParams::Wireguard {
            private_key,
            peer_public_key,
            preshared_key,
            addresses,
            reserved,
            mtu,
        } => {
            let mut query = vec![
                ("address".into(), addresses.join(",")),
                ("publickey".into(), peer_public_key.clone()),
            ];
            if let Some(psk) = preshared_key {
                query.push(("presharedkey".into(), psk.clone()));
            }
            if let Some(reserved) = reserved {
                query.push((
                    "reserved".into(),
                    reserved
                        .iter()
                        .map(|b| b.to_string())
                        .collect::<Vec<_>>()
                        .join(","),
                ));
            }
            if let Some(mtu) = mtu {
                query.push(("mtu".into(), mtu.to_string()));
            }
            format!(
                "wireguard://{}@{host}:{port}?{}#{name}",
                enc(private_key),
                query_string(&query)
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_uri;

    /// Round-trip: parse → export → parse again must preserve identity.
    #[test]
    fn roundtrips_every_scheme() {
        let uris = [
            "vless://2f9a4b7c-1d2e-4f5a-8b9c-0d1e2f3a4b5c@a.example.com:443?security=reality&pbk=k123&sid=ab&fp=chrome&sni=x.com&flow=xtls-rprx-vision&type=tcp#Reality",
            "trojan://p%40ss@b.example.com:443?security=tls&sni=b.example.com&type=ws&path=%2Ftj#Troj",
            "hysteria2://pw@c.example.com:8443/?sni=c.example.com&obfs=salamander&obfs-password=opw#Hy2",
            "tuic://2f9a4b7c-1d2e-4f5a-8b9c-0d1e2f3a4b5c:pw@d.example.com:443?congestion_control=bbr&sni=d.example.com#Tuic",
            "wireguard://cHJpdg%3D%3D@e.example.com:51820?address=10.0.0.2%2F32&publickey=pub&mtu=1420#Wg",
        ];
        for uri in uris {
            let original = parse_uri(uri).expect("parse original");
            let exported = export_uri(&original);
            let reparsed = parse_uri(&exported)
                .unwrap_or_else(|e| panic!("re-parse failed for {exported}: {e}"));
            assert!(
                original.same_endpoint(&reparsed),
                "identity lost: {uri} -> {exported}"
            );
            assert_eq!(original.name, reparsed.name);
            assert_eq!(original.tls, reparsed.tls, "tls differs for {uri}");
        }
    }

    #[test]
    fn ss_roundtrips_including_plugin() {
        let creds = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode("chacha20-ietf-poly1305:pw");
        let uri = format!("ss://{creds}@f.example.com:8388/?plugin=obfs-local%3Bobfs%3Dhttp#SS");
        let original = parse_uri(&uri).expect("parse");
        let reparsed = parse_uri(&export_uri(&original)).expect("re-parse");
        assert!(original.same_endpoint(&reparsed));
    }

    #[test]
    fn vmess_roundtrips() {
        let body = serde_json::json!({
            "v": "2", "ps": "VM", "add": "g.example.com", "port": "443",
            "id": "2f9a4b7c-1d2e-4f5a-8b9c-0d1e2f3a4b5c", "aid": "0", "scy": "auto",
            "net": "ws", "host": "g.example.com", "path": "/vm", "tls": "tls",
            "sni": "g.example.com"
        });
        let uri = format!("vmess://{}", STANDARD.encode(body.to_string()));
        let original = parse_uri(&uri).expect("parse");
        let reparsed = parse_uri(&export_uri(&original)).expect("re-parse");
        assert!(original.same_endpoint(&reparsed));
        assert_eq!(original.name, reparsed.name);
    }
}
