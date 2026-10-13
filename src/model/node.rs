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
    /// Encryption method for protocols that need one (Shadowsocks ciphers).
    /// `None` where the protocol fixes the cipher itself.
    #[serde(default)]
    pub cipher: Option<String>,
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
    /// Result of the real end-to-end connectivity test, when one was run.
    ///
    /// Kept apart from the latency field because the two disagree: on the
    /// bundled subscription the twelve lowest-TCP-latency servers all failed
    /// to carry traffic, so ordering by latency alone ranks broken nodes
    /// first.
    #[serde(default)]
    pub verified: Option<bool>,
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
            cipher: None,
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
            verified: None,
            is_favorite: false,
        }
    }

    /// Whether this node is known to carry traffic, unknown, or known to fail.
    pub fn verification_state(&self) -> VerificationState {
        match self.verified {
            Some(true) => VerificationState::Working,
            Some(false) => VerificationState::Failed,
            None => VerificationState::Unknown,
        }
    }

    /// Sort bucket that puts working servers first, then untested, then failed.
    ///
    /// Ordering by latency alone would rank servers that answer TCP but cannot
    /// carry traffic above servers that actually work, which is the failure
    /// measured on the bundled subscription.
    pub fn rank_bucket(&self) -> u8 {
        match self.verification_state() {
            VerificationState::Working => 0,
            VerificationState::Unknown => 1,
            VerificationState::Failed => 2,
        }
    }

    /// Latency used for ordering; untested nodes sort last.
    pub fn sort_latency(&self) -> u32 {
        self.latency_ms.unwrap_or(u32::MAX)
    }
}

/// Outcome of a real connectivity test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationState {
    /// Confirmed to carry traffic.
    Working,
    /// Never tested.
    Unknown,
    /// Tested and found unable to carry traffic.
    Failed,
}
