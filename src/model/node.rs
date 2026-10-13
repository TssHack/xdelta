use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    Vless,
    Vmess,
    Trojan,
    Shadowsocks,
    Hysteria2,
    Tuic,
}

impl Protocol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Vless => "VLESS",
            Self::Vmess => "VMess",
            Self::Trojan => "Trojan",
            Self::Shadowsocks => "SS",
            Self::Hysteria2 => "Hy2",
            Self::Tuic => "TUIC",
        }
    }

    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Vless => "badge-vless",
            Self::Vmess => "badge-vmess",
            Self::Trojan => "badge-trojan",
            Self::Shadowsocks => "badge-ss",
            Self::Hysteria2 => "badge-vless",
            Self::Tuic => "badge-vmess",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Transport {
    Tcp,
    Ws,
    Grpc,
    HttpUpgrade,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Security {
    None,
    Tls,
    Reality,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealityConfig {
    pub public_key: String,
    pub short_id: Option<String>,
    pub sni: String,
    pub spider_x: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsConfig {
    pub path: String,
    pub host: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrpcConfig {
    pub service_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: String,
    pub name: String,
    pub server: String,
    pub port: u16,
    pub protocol: Protocol,
    pub secret: String, // UUID, password, or key
    pub flow: Option<String>,
    pub security: Security,
    pub transport: Transport,
    pub sni: Option<String>,
    pub alpn: Option<Vec<String>>,
    pub reality: Option<RealityConfig>,
    pub ws: Option<WsConfig>,
    pub grpc: Option<GrpcConfig>,
    pub subscription_id: Option<String>,
    pub latency_ms: Option<u32>,
    pub is_favorite: bool,
}

impl Node {
    pub fn new(
        id: String,
        name: String,
        server: String,
        port: u16,
        protocol: Protocol,
        secret: String,
    ) -> Self {
        Self {
            id,
            name,
            server,
            port,
            protocol,
            secret,
            flow: None,
            security: Security::None,
            transport: Transport::Tcp,
            sni: None,
            alpn: None,
            reality: None,
            ws: None,
            grpc: None,
            subscription_id: None,
            latency_ms: None,
            is_favorite: false,
        }
    }
}
