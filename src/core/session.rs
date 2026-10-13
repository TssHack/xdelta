//! Connection model: what to run, and the state machine that drives it.
//!
//! This module holds the *description* of a connection ([`ConnectionProfile`])
//! and the *lifecycle* rules ([`ConnectionState`]). It deliberately contains no
//! GTK or process code so the transitions stay unit-testable; the side effects
//! live in `core::process`, `net::sysproxy` and `net::tun`.

use crate::model::Node;
use serde::{Deserialize, Serialize};

/// Port Xray exposes its stats/API listener on, bound to loopback only.
pub const DEFAULT_API_PORT: u16 = 10085;

/// Local SOCKS5 and HTTP listener defaults.
pub const DEFAULT_SOCKS_PORT: u16 = 10808;
pub const DEFAULT_HTTP_PORT: u16 = 10809;

/// How traffic is captured from the operating system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionMode {
    /// Applications reach the local listeners through the desktop proxy
    /// settings. Needs no privileges; only cooperating apps are routed.
    SystemProxy,
    /// A layer-3 TUN device captures all traffic. Needs `CAP_NET_ADMIN` on the
    /// core binary, because creating the device is a privileged operation.
    Tun,
}

impl ConnectionMode {
    pub fn label(&self) -> &'static str {
        match self {
            Self::SystemProxy => "System Proxy",
            Self::Tun => "TUN",
        }
    }
}

/// How traffic is classified before being forwarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingStrategy {
    /// Send domestic and private destinations direct, everything else via proxy.
    Rule,
    /// Send everything through the proxy.
    Global,
    /// Send nothing through the proxy (useful for testing).
    Direct,
}

impl RoutingStrategy {
    pub fn index(&self) -> u32 {
        match self {
            Self::Rule => 0,
            Self::Global => 1,
            Self::Direct => 2,
        }
    }

    pub fn from_index(index: u32) -> Self {
        match index {
            1 => Self::Global,
            2 => Self::Direct,
            _ => Self::Rule,
        }
    }
}

/// Routing rule switches exposed on the Routing page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingOptions {
    pub strategy: RoutingStrategy,
    /// Keep private/link-local ranges off the proxy.
    pub bypass_lan: bool,
    /// Keep domestic (.ir / category-ir) destinations off the proxy.
    pub bypass_iran: bool,
    /// Drop known advertising and telemetry domains.
    pub block_ads: bool,
    /// Extra domains the user wants routed directly.
    pub direct_domains: Vec<String>,
}

impl Default for RoutingOptions {
    fn default() -> Self {
        Self {
            strategy: RoutingStrategy::Rule,
            bypass_lan: true,
            bypass_iran: true,
            block_ads: true,
            direct_domains: Vec::new(),
        }
    }
}

impl RoutingOptions {
    /// Splits the free-text entry into trimmed, non-empty domain tokens.
    ///
    /// Accepts comma- or newline-separated input so a user can paste a list.
    pub fn parse_direct_domains(input: &str) -> Vec<String> {
        input
            .split([',', '\n'])
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    }
}

/// Which network stack the TUN device uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TunStack {
    /// In-kernel stack: fastest and most compatible.
    System,
    /// Userspace stack: safer, works without extra kernel features.
    Gvisor,
    /// Let the core choose.
    Mixed,
}

impl TunStack {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Gvisor => "gvisor",
            Self::Mixed => "mixed",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::System => "System (kernel)",
            Self::Gvisor => "gVisor (userspace)",
            Self::Mixed => "Mixed",
        }
    }

    pub fn index(&self) -> u32 {
        match self {
            Self::System => 0,
            Self::Gvisor => 1,
            Self::Mixed => 2,
        }
    }

    pub fn from_index(index: u32) -> Self {
        match index {
            1 => Self::Gvisor,
            2 => Self::Mixed,
            _ => Self::System,
        }
    }
}

/// TUN device parameters exposed on the Settings page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TunOptions {
    pub stack: TunStack,
    /// Interface MTU. 1500 matches Ethernet; lower values can help on flaky links.
    pub mtu: u16,
    /// Install the default routes for the tunnel automatically.
    pub auto_route: bool,
    /// Refuse to fall back to a direct connection if the tunnel is not ready.
    /// Safer against leaks, but a misconfiguration can leave the machine offline.
    pub strict_route: bool,
    /// DNS servers pushed into the tunnel.
    pub dns_servers: Vec<String>,
}

impl Default for TunOptions {
    fn default() -> Self {
        Self {
            stack: TunStack::System,
            mtu: 1500,
            auto_route: true,
            strict_route: false,
            dns_servers: vec!["1.1.1.1".to_string(), "8.8.8.8".to_string()],
        }
    }
}

impl TunOptions {
    pub fn parse_dns(input: &str) -> Vec<String> {
        RoutingOptions::parse_direct_domains(input)
    }

    /// MTU values the kernel accepts on a TUN device.
    pub fn is_valid_mtu(mtu: u16) -> bool {
        (576..=9000).contains(&mtu)
    }
}

/// Everything needed to build a core configuration and bring a connection up.
///
/// Deliberately not `PartialEq`: profiles are compared by their generated
/// configuration in tests, and requiring equality here would force it onto the
/// whole node model for no benefit.
#[derive(Debug, Clone)]
pub struct ConnectionProfile {
    pub node: Node,
    pub mode: ConnectionMode,
    pub socks_port: u16,
    pub http_port: u16,
    pub api_port: u16,
    pub routing: RoutingOptions,
    pub tun: TunOptions,
}

impl ConnectionProfile {
    pub fn new(node: Node, mode: ConnectionMode) -> Self {
        Self {
            node,
            mode,
            socks_port: DEFAULT_SOCKS_PORT,
            http_port: DEFAULT_HTTP_PORT,
            api_port: DEFAULT_API_PORT,
            routing: RoutingOptions::default(),
            tun: TunOptions::default(),
        }
    }

    /// Rejects listener port combinations that cannot work.
    ///
    /// Two listeners on the same port, or a privileged port the app cannot
    /// bind as a normal user, would fail deep inside the core with an opaque
    /// error, so they are caught here with an actionable message.
    pub fn validate(&self) -> Result<(), String> {
        let ports = [
            ("SOCKS", self.socks_port),
            ("HTTP", self.http_port),
            ("API", self.api_port),
        ];

        for (name, port) in ports {
            if port == 0 {
                return Err(format!("{name} port must not be 0"));
            }
            if port < 1024 {
                return Err(format!(
                    "{name} port {port} is privileged (<1024); choose one above 1023"
                ));
            }
        }

        for (i, (name_a, port_a)) in ports.iter().enumerate() {
            for (name_b, port_b) in ports.iter().skip(i + 1) {
                if port_a == port_b {
                    return Err(format!(
                        "{name_a} and {name_b} ports are both {port_a}; they must differ"
                    ));
                }
            }
        }

        if !TunOptions::is_valid_mtu(self.tun.mtu) {
            return Err(format!(
                "MTU {} is outside the supported range (576-9000)",
                self.tun.mtu
            ));
        }

        if self.node.server.trim().is_empty() {
            return Err("selected node has no server address".to_string());
        }
        if self.node.secret.trim().is_empty() {
            return Err("selected node has no credential".to_string());
        }

        Ok(())
    }
}

/// Lifecycle state of a connection attempt.
///
/// The distinction between `Failed` and `Connected` matters: the UI previously
/// reported success even when the core never started, which made a broken
/// configuration look like a working tunnel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
    Failed(String),
}

impl ConnectionState {
    /// Whether a new connection may be started from this state.
    pub fn can_connect(&self) -> bool {
        matches!(self, Self::Disconnected | Self::Failed(_))
    }

    /// Whether an active connection may be torn down from this state.
    pub fn can_disconnect(&self) -> bool {
        matches!(self, Self::Connected | Self::Failed(_))
    }

    /// Whether the core is expected to be running.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Connected | Self::Connecting)
    }

    pub fn is_transitioning(&self) -> bool {
        matches!(self, Self::Connecting | Self::Disconnecting)
    }

    /// Short uppercase label for the dashboard status badge.
    pub fn badge_label(&self) -> &str {
        match self {
            Self::Disconnected => "DISCONNECTED",
            Self::Connecting => "CONNECTING",
            Self::Connected => "CONNECTED",
            Self::Disconnecting => "DISCONNECTING",
            Self::Failed(_) => "ERROR",
        }
    }

    /// Human-readable explanation for the status line.
    pub fn detail(&self) -> String {
        match self {
            Self::Disconnected => "Click power button to connect".to_string(),
            Self::Connecting => "Starting core...".to_string(),
            Self::Connected => "Active".to_string(),
            Self::Disconnecting => "Stopping core...".to_string(),
            Self::Failed(reason) => format!("Error: {reason}"),
        }
    }

    /// CSS class matching this state on the status badge.
    pub fn css_class(&self) -> &'static str {
        match self {
            Self::Connected => "status-connected",
            Self::Failed(_) => "status-error",
            _ => "status-disconnected",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Node, Protocol};

    fn node() -> Node {
        Node::new(
            "1".into(),
            "Test".into(),
            "example.com".into(),
            443,
            Protocol::Vless,
            "00000000-0000-0000-0000-000000000001".into(),
        )
    }

    fn profile() -> ConnectionProfile {
        ConnectionProfile::new(node(), ConnectionMode::SystemProxy)
    }

    #[test]
    fn default_profile_is_valid() {
        assert_eq!(profile().validate(), Ok(()));
    }

    /// The listeners must not collide, otherwise the core fails at startup with
    /// an unhelpful bind error.
    #[test]
    fn duplicate_ports_are_rejected() {
        let mut p = profile();
        p.http_port = p.socks_port;
        let err = p.validate().expect_err("duplicate ports must be rejected");
        assert!(
            err.contains("SOCKS"),
            "message should name the ports: {err}"
        );
    }

    /// Binding below 1024 requires privileges the app does not have.
    #[test]
    fn privileged_ports_are_rejected() {
        let mut p = profile();
        p.socks_port = 80;
        let err = p.validate().expect_err("privileged port must be rejected");
        assert!(err.contains("privileged"), "got: {err}");
    }

    #[test]
    fn zero_port_is_rejected() {
        let mut p = profile();
        p.api_port = 0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn out_of_range_mtu_is_rejected() {
        let mut p = profile();
        p.tun.mtu = 100;
        let err = p.validate().expect_err("tiny MTU must be rejected");
        assert!(err.contains("MTU"), "got: {err}");
    }

    #[test]
    fn node_without_server_or_secret_is_rejected() {
        let mut p = profile();
        p.node.server = "   ".into();
        assert!(p.validate().is_err());

        let mut p = profile();
        p.node.secret = String::new();
        assert!(p.validate().is_err());
    }

    #[test]
    fn mtu_bounds_are_inclusive() {
        assert!(TunOptions::is_valid_mtu(576));
        assert!(TunOptions::is_valid_mtu(1500));
        assert!(TunOptions::is_valid_mtu(9000));
        assert!(!TunOptions::is_valid_mtu(575));
        assert!(!TunOptions::is_valid_mtu(9001));
    }

    #[test]
    fn direct_domains_are_split_and_trimmed() {
        let parsed = RoutingOptions::parse_direct_domains(" a.com , b.com\n  c.com ,, ");
        assert_eq!(parsed, vec!["a.com", "b.com", "c.com"]);
    }

    #[test]
    fn empty_domain_input_yields_no_entries() {
        assert!(RoutingOptions::parse_direct_domains("   ,  \n ").is_empty());
    }

    #[test]
    fn routing_strategy_index_roundtrips() {
        for index in 0..3u32 {
            assert_eq!(RoutingStrategy::from_index(index).index(), index);
        }
    }

    #[test]
    fn tun_stack_index_roundtrips() {
        for index in 0..3u32 {
            assert_eq!(TunStack::from_index(index).index(), index);
        }
    }

    /// The whole point of the state machine: a failed start must never be
    /// reported as a working connection.
    #[test]
    fn failed_start_is_not_reported_as_connected() {
        let failed = ConnectionState::Failed("binary missing".into());
        assert!(!failed.is_active());
        assert_eq!(failed.badge_label(), "ERROR");
        assert_ne!(failed.badge_label(), "CONNECTED");
        assert!(failed.detail().contains("binary missing"));
    }

    #[test]
    fn connecting_and_connected_are_active() {
        assert!(ConnectionState::Connected.is_active());
        assert!(ConnectionState::Connecting.is_active());
        assert!(!ConnectionState::Disconnected.is_active());
        assert!(!ConnectionState::Disconnecting.is_active());
    }

    #[test]
    fn connection_can_be_retried_after_failure() {
        let failed = ConnectionState::Failed("nope".into());
        assert!(failed.can_connect(), "a failure must be retryable");
        assert!(failed.can_disconnect(), "a failure must be cleanable");
    }

    #[test]
    fn transitioning_states_block_new_actions() {
        assert!(ConnectionState::Connecting.is_transitioning());
        assert!(ConnectionState::Disconnecting.is_transitioning());
        assert!(!ConnectionState::Connecting.can_connect());
        assert!(!ConnectionState::Disconnecting.can_disconnect());
    }

    #[test]
    fn every_state_has_a_badge_and_css_class() {
        let states = [
            ConnectionState::Disconnected,
            ConnectionState::Connecting,
            ConnectionState::Connected,
            ConnectionState::Disconnecting,
            ConnectionState::Failed("x".into()),
        ];
        for state in states {
            assert!(!state.badge_label().is_empty());
            assert!(!state.detail().is_empty());
            assert!(state.css_class().starts_with("status-"));
        }
    }
}
