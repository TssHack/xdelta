use crate::model::{GrpcConfig, Node, Protocol, RealityConfig, Security, Transport, WsConfig};
use base64::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use url::Url;
use uuid::Uuid;

#[derive(Deserialize)]
struct VmessJson {
    pub add: Option<String>,
    pub port: Option<serde_json::Value>,
    pub id: Option<String>,
    pub net: Option<String>,
    pub host: Option<String>,
    pub path: Option<String>,
    pub tls: Option<String>,
    pub ps: Option<String>,
}

pub fn parse_node_uri(uri_str: &str) -> Result<Node, String> {
    let trimmed = uri_str.trim();

    if trimmed.starts_with("vless://") {
        parse_vless_uri(trimmed)
    } else if trimmed.starts_with("vmess://") {
        parse_vmess_uri(trimmed)
    } else if trimmed.starts_with("trojan://") {
        parse_trojan_uri(trimmed)
    } else {
        Err(format!("Unsupported protocol in URI: {}", trimmed))
    }
}

fn parse_vless_uri(uri_str: &str) -> Result<Node, String> {
    let url = Url::parse(uri_str).map_err(|e| format!("Invalid URL: {}", e))?;
    let secret = url.username().to_string();
    let server = url.host_str().ok_or("Missing host")?.to_string();
    let port = url.port().ok_or("Missing port")?;

    let name = url
        .fragment()
        .map(|f| urlencoding::decode(f).unwrap_or(f.into()).to_string())
        .unwrap_or_else(|| format!("{}:{}", server, port));

    let query_pairs: HashMap<String, String> = url.query_pairs().into_owned().collect();

    let mut node = Node::new(
        Uuid::new_v4().to_string(),
        name,
        server,
        port,
        Protocol::Vless,
        secret,
    );

    node.flow = query_pairs.get("flow").cloned();

    // Security & Reality
    let sec_str = query_pairs
        .get("security")
        .map(|s| s.as_str())
        .unwrap_or("none");
    match sec_str {
        "reality" => {
            node.security = Security::Reality;
            let sni = query_pairs.get("sni").cloned().unwrap_or_default();
            let pbk = query_pairs.get("pbk").cloned().unwrap_or_default();
            let sid = query_pairs.get("sid").cloned();
            let spx = query_pairs.get("spx").cloned();
            node.sni = Some(sni.clone());
            node.reality = Some(RealityConfig {
                public_key: pbk,
                short_id: sid,
                sni,
                spider_x: spx,
            });
        }
        "tls" => {
            node.security = Security::Tls;
            node.sni = query_pairs.get("sni").cloned();
        }
        _ => node.security = Security::None,
    }

    // Transport
    let net_type = query_pairs.get("type").map(|s| s.as_str()).unwrap_or("tcp");
    match net_type {
        "ws" => {
            node.transport = Transport::Ws;
            let path = query_pairs
                .get("path")
                .cloned()
                .unwrap_or_else(|| "/".into());
            let host = query_pairs.get("host").cloned();
            node.ws = Some(WsConfig { path, host });
        }
        "grpc" => {
            node.transport = Transport::Grpc;
            let service_name = query_pairs.get("serviceName").cloned().unwrap_or_default();
            node.grpc = Some(GrpcConfig { service_name });
        }
        _ => node.transport = Transport::Tcp,
    }

    Ok(node)
}

fn parse_vmess_uri(uri_str: &str) -> Result<Node, String> {
    let b64_payload = uri_str
        .strip_prefix("vmess://")
        .ok_or("Invalid vmess prefix")?;
    let decoded_bytes = BASE64_STANDARD
        .decode(b64_payload)
        .or_else(|_| BASE64_URL_SAFE.decode(b64_payload))
        .or_else(|_| BASE64_STANDARD_NO_PAD.decode(b64_payload))
        .map_err(|e| format!("Base64 decode failed: {}", e))?;

    let vmess: VmessJson = serde_json::from_slice(&decoded_bytes)
        .map_err(|e| format!("Failed to parse VMess JSON: {}", e))?;

    let server = vmess.add.ok_or("Missing vmess server address")?;
    let secret = vmess.id.ok_or("Missing vmess uuid")?;
    let port: u16 = match vmess.port {
        Some(serde_json::Value::Number(n)) => n.as_u64().unwrap_or(443) as u16,
        Some(serde_json::Value::String(s)) => s.parse().unwrap_or(443),
        _ => 443,
    };

    let name = vmess.ps.unwrap_or_else(|| format!("{}:{}", server, port));
    let mut node = Node::new(
        Uuid::new_v4().to_string(),
        name,
        server,
        port,
        Protocol::Vmess,
        secret,
    );

    if let Some(tls) = vmess.tls {
        if tls.eq_ignore_ascii_case("tls") {
            node.security = Security::Tls;
        }
    }

    match vmess.net.as_deref() {
        Some("ws") => {
            node.transport = Transport::Ws;
            node.ws = Some(WsConfig {
                path: vmess.path.unwrap_or_else(|| "/".into()),
                host: vmess.host,
            });
        }
        Some("grpc") => {
            node.transport = Transport::Grpc;
            node.grpc = Some(GrpcConfig {
                service_name: vmess.path.unwrap_or_default(),
            });
        }
        _ => node.transport = Transport::Tcp,
    }

    Ok(node)
}

fn parse_trojan_uri(uri_str: &str) -> Result<Node, String> {
    let url = Url::parse(uri_str).map_err(|e| format!("Invalid URL: {}", e))?;
    let secret = url.username().to_string();
    let server = url.host_str().ok_or("Missing host")?.to_string();
    let port = url.port().ok_or("Missing port")?;

    let name = url
        .fragment()
        .map(|f| urlencoding::decode(f).unwrap_or(f.into()).to_string())
        .unwrap_or_else(|| format!("{}:{}", server, port));

    let query_pairs: HashMap<String, String> = url.query_pairs().into_owned().collect();

    let mut node = Node::new(
        Uuid::new_v4().to_string(),
        name,
        server,
        port,
        Protocol::Trojan,
        secret,
    );
    node.security = Security::Tls;
    node.sni = query_pairs.get("sni").cloned();

    Ok(node)
}

// Simple fallback urlencoding module in case urlencoding crate is not directly in Cargo.toml
mod urlencoding {
    pub fn decode(s: &str) -> Result<String, ()> {
        Ok(percent_encoding::percent_decode_str(s)
            .decode_utf8_lossy()
            .to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_vless_reality() {
        let uri = "vless://a1b2c3d4-e5f6-7a8b-9c0d-1e2f3a4b5c6d@198.51.100.1:443?security=reality&sni=gateway.icloud.com&pbk=xYz1234567890abcdefghijklmnopqrstuvwxyzAB&sid=1234abcd&type=tcp&flow=xtls-rprx-vision#Frankfurt-Reality";
        let node = parse_node_uri(uri).expect("Should parse vless reality");
        assert_eq!(node.protocol, Protocol::Vless);
        assert_eq!(node.server, "198.51.100.1");
        assert_eq!(node.port, 443);
        assert_eq!(node.name, "Frankfurt-Reality");
        assert_eq!(node.security, Security::Reality);
        assert_eq!(node.flow.as_deref(), Some("xtls-rprx-vision"));
        let reality = node.reality.expect("Reality config should be present");
        assert_eq!(reality.sni, "gateway.icloud.com");
        assert_eq!(reality.short_id.as_deref(), Some("1234abcd"));
    }

    #[test]
    fn test_parse_trojan() {
        let uri = "trojan://password123@my-trojan.com:443?sni=my-trojan.com#Trojan-Fast";
        let node = parse_node_uri(uri).expect("Should parse trojan");
        assert_eq!(node.protocol, Protocol::Trojan);
        assert_eq!(node.server, "my-trojan.com");
        assert_eq!(node.port, 443);
        assert_eq!(node.secret, "password123");
        assert_eq!(node.name, "Trojan-Fast");
    }
}
