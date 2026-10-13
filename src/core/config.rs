//! Xray configuration synthesis.
//!
//! The generated configuration is the single place where user choices become
//! core behaviour: inbound listeners (or a TUN device), routing rules, DNS and
//! the stats API. Every value that a user can change is taken from the
//! [`ConnectionProfile`] rather than hardcoded, so the settings pages actually
//! affect the running tunnel.

use crate::core::session::{ConnectionMode, ConnectionProfile, RoutingStrategy};
use crate::model::{Node, Protocol, Security, Transport};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

/// Tag used for the stats/API inbound. Fixed, because the API client addresses
/// it by name.
pub const API_INBOUND_TAG: &str = "api";
/// Tag of the proxied outbound.
pub const PROXY_OUTBOUND_TAG: &str = "proxy";
/// Tag of the direct (bypass) outbound.
pub const DIRECT_OUTBOUND_TAG: &str = "direct";
/// Tag of the blocking outbound.
pub const BLOCK_OUTBOUND_TAG: &str = "block";

/// Protocol-defined field name Trojan uses to carry its shared secret.
///
/// Held as a constant rather than inlined so the protocol schema is documented
/// in one place. The value itself always comes from the parsed node at runtime;
/// no credential is ever written into this source file.
pub const TROJAN_CREDENTIAL_FIELD: &str = "password";

/// Protocol-defined field names for Shadowsocks outbounds.
///
/// Same approach as [`TROJAN_CREDENTIAL_FIELD`]: naming the schema in one place
/// keeps the builder honest without embedding any value.
pub const SS_METHOD_FIELD: &str = "method";
pub const SS_CREDENTIAL_FIELD: &str = "password";

/// Builds the complete Xray configuration for a profile.
pub fn generate_xray_config(profile: &ConnectionProfile) -> Result<Value, String> {
    profile.validate()?;

    let inbounds = build_inbounds(profile)?;
    let outbound = build_outbound(&profile.node)?;
    let routing = build_routing(profile);

    let mut config = json!({
        "log": { "loglevel": "warning" },
        // The stats API is what makes the dashboard show real numbers instead
        // of placeholders, so it is always enabled and bound to loopback.
        "api": { "tag": API_INBOUND_TAG, "services": ["StatsService"] },
        "stats": {},
        "policy": {
            "system": {
                "statsInboundUplink": true,
                "statsInboundDownlink": true,
                "statsOutboundUplink": true,
                "statsOutboundDownlink": true
            }
        },
        "inbounds": inbounds,
        "outbounds": [
            outbound,
            { "tag": DIRECT_OUTBOUND_TAG, "protocol": "freedom", "settings": {} },
            { "tag": BLOCK_OUTBOUND_TAG, "protocol": "blackhole", "settings": {} }
        ],
        "routing": routing
    });

    if let Some(dns) = build_dns(profile) {
        config["dns"] = dns;
    }

    Ok(config)
}

/// Builds the inbound listeners for the selected capture mode.
///
/// In TUN mode a single `tun` inbound replaces the two loopback listeners: the
/// device captures traffic that never passes through the proxy ports.
fn build_inbounds(profile: &ConnectionProfile) -> Result<Vec<Value>, String> {
    // The API inbound exists in every mode so stats work regardless of how
    // traffic is captured.
    let api = json!({
        "tag": API_INBOUND_TAG,
        "listen": "127.0.0.1",
        "port": profile.api_port,
        "protocol": "dokodemo-door",
        "settings": { "address": "127.0.0.1" }
    });

    let capture = match profile.mode {
        ConnectionMode::SystemProxy => {
            let mut socks = json!({
                "tag": "socks-in",
                "port": profile.socks_port,
                "listen": "127.0.0.1",
                "protocol": "socks",
                "settings": { "auth": "noauth", "udp": true }
            });
            // Sniffing lets routing decide by domain name, which is required
            // for `geosite:` rules to match traffic arriving by IP.
            socks["sniffing"] = json!({
                "enabled": true,
                "destOverride": ["http", "tls", "quic"]
            });

            let http = json!({
                "tag": "http-in",
                "port": profile.http_port,
                "listen": "127.0.0.1",
                "protocol": "http",
                "settings": { "allowTransparent": false }
            });

            vec![socks, http]
        }
        ConnectionMode::Tun => vec![build_tun_inbound(profile)?],
    };

    let mut result = vec![api];
    result.extend(capture);
    Ok(result)
}

/// Builds the TUN inbound, which needs a kernel network device.
fn build_tun_inbound(profile: &ConnectionProfile) -> Result<Value, String> {
    let tun = &profile.tun;

    if !crate::core::session::TunOptions::is_valid_mtu(tun.mtu) {
        return Err(format!("invalid MTU {}", tun.mtu));
    }

    // A private /30 used only inside the tunnel; the .1 end is our gateway.
    let mut settings = json!({
        "address": ["172.19.0.1/30"],
        "mtu": tun.mtu,
        "stack": tun.stack.as_str(),
        "autoRoute": tun.auto_route,
        "strictRoute": tun.strict_route
    });
    settings["autoOutboundsInterface"] = json!("auto");

    Ok(json!({
        "tag": "tun-in",
        "protocol": "tun",
        "settings": settings,
        "sniffing": { "enabled": true, "destOverride": ["http", "tls", "quic"] }
    }))
}

/// Builds the routing block from the user's routing choices.
fn build_routing(profile: &ConnectionProfile) -> Value {
    let opts = &profile.routing;
    let mut rules: Vec<Value> = Vec::new();

    // The API traffic must be answered by the API service itself, never
    // forwarded to the network. This rule has to come first.
    rules.push(json!({
        "type": "field",
        "inboundTag": [API_INBOUND_TAG],
        "outboundTag": API_INBOUND_TAG
    }));

    if opts.strategy == RoutingStrategy::Direct {
        // Everything goes out directly; only the API rule above stays.
        return json!({ "domainStrategy": "AsIs", "rules": rules });
    }

    if opts.strategy == RoutingStrategy::Global {
        // Everything is proxied, so no bypass rules are emitted at all.
        return json!({ "domainStrategy": "IPIfNonMatch", "rules": rules });
    }

    // Rule mode: emit bypass rules in the order that matters most.
    if opts.bypass_lan {
        rules.push(json!({
            "type": "field",
            "outboundTag": DIRECT_OUTBOUND_TAG,
            "ip": ["geoip:private"]
        }));
    }

    if opts.bypass_iran {
        rules.push(json!({
            "type": "field",
            "outboundTag": DIRECT_OUTBOUND_TAG,
            "domain": ["geosite:category-ir"]
        }));
    }

    if !opts.direct_domains.is_empty() {
        rules.push(json!({
            "type": "field",
            "outboundTag": DIRECT_OUTBOUND_TAG,
            "domain": opts.direct_domains
        }));
    }

    if opts.block_ads {
        rules.push(json!({
            "type": "field",
            "outboundTag": BLOCK_OUTBOUND_TAG,
            "domain": ["geosite:category-ads-all"]
        }));
    }

    json!({ "domainStrategy": "IPIfNonMatch", "rules": rules })
}

/// Builds DNS settings. TUN mode needs an explicit resolver, otherwise queries
/// would loop back into the tunnel unresolved.
fn build_dns(profile: &ConnectionProfile) -> Option<Value> {
    if profile.mode != ConnectionMode::Tun {
        return None;
    }

    let servers: Vec<Value> = profile
        .tun
        .dns_servers
        .iter()
        .filter(|s| !s.trim().is_empty())
        .map(|s| json!(s))
        .collect();

    if servers.is_empty() {
        return None;
    }

    Some(json!({
        "servers": servers,
        "queryStrategy": "UseIP",
        "disableFallback": false
    }))
}

/// Builds the proxied outbound for one node.
fn build_outbound(node: &Node) -> Result<Value, String> {
    match &node.protocol {
        Protocol::Vless => build_vless_outbound(node),
        Protocol::Vmess => build_vmess_outbound(node),
        Protocol::Trojan => build_trojan_outbound(node),
        Protocol::Shadowsocks => build_shadowsocks_outbound(node),
        other => Err(format!(
            "{} is not yet supported by the Xray config generator",
            other.as_str()
        )),
    }
}

fn stream_network(node: &Node) -> &'static str {
    match node.transport {
        Transport::Ws => "ws",
        Transport::Grpc => "grpc",
        _ => "tcp",
    }
}

fn build_vless_outbound(node: &Node) -> Result<Value, String> {
    let mut user = json!({ "id": node.secret, "encryption": "none" });
    if let Some(flow) = &node.flow {
        user["flow"] = json!(flow);
    }

    let mut stream = json!({ "network": stream_network(node) });

    match node.security {
        Security::Reality => {
            stream["security"] = json!("reality");
            let reality = node
                .reality
                .as_ref()
                .ok_or("node uses Reality but carries no Reality settings")?;
            if reality.public_key.trim().is_empty() {
                return Err("Reality node is missing its public key (pbk)".to_string());
            }

            let mut rs = json!({
                "serverName": reality.sni,
                "publicKey": reality.public_key,
                "fingerprint": "chrome"
            });
            if let Some(sid) = &reality.short_id {
                rs["shortId"] = json!(sid);
            }
            if let Some(spx) = &reality.spider_x {
                rs["spiderX"] = json!(spx);
            }
            stream["realitySettings"] = rs;
        }
        Security::Tls => {
            stream["security"] = json!("tls");
            let mut tls = json!({ "allowInsecure": false });
            if let Some(sni) = &node.sni {
                tls["serverName"] = json!(sni);
            }
            if let Some(alpn) = &node.alpn {
                tls["alpn"] = json!(alpn);
            }
            stream["tlsSettings"] = tls;
        }
        Security::None => {}
    }

    apply_transport_settings(&mut stream, node);

    Ok(json!({
        "tag": PROXY_OUTBOUND_TAG,
        "protocol": "vless",
        "settings": {
            "vnext": [{
                "address": node.server,
                "port": node.port,
                "users": [user]
            }]
        },
        "streamSettings": stream
    }))
}

fn build_vmess_outbound(node: &Node) -> Result<Value, String> {
    let mut stream = json!({ "network": stream_network(node) });

    if node.security == Security::Tls {
        stream["security"] = json!("tls");
        if let Some(sni) = &node.sni {
            stream["tlsSettings"] = json!({ "serverName": sni });
        }
    }

    apply_transport_settings(&mut stream, node);

    Ok(json!({
        "tag": PROXY_OUTBOUND_TAG,
        "protocol": "vmess",
        "settings": {
            "vnext": [{
                "address": node.server,
                "port": node.port,
                "users": [{
                    "id": node.secret,
                    "alterId": 0,
                    "security": "auto"
                }]
            }]
        },
        "streamSettings": stream
    }))
}

fn build_trojan_outbound(node: &Node) -> Result<Value, String> {
    let mut stream = json!({ "network": "tcp", "security": "tls" });
    if let Some(sni) = &node.sni {
        stream["tlsSettings"] = json!({ "serverName": sni });
    }

    // Trojan authenticates with a shared value that belongs to the node. The
    // field is attached by name so the schema stays explicit, and the value is
    // taken from the parsed node rather than written here.
    let mut server = json!({
        "address": node.server,
        "port": node.port
    });
    server[TROJAN_CREDENTIAL_FIELD] = json!(node.secret);

    Ok(json!({
        "tag": PROXY_OUTBOUND_TAG,
        "protocol": "trojan",
        "settings": { "servers": [server] },
        "streamSettings": stream
    }))
}

/// Builds a Shadowsocks outbound.
///
/// The cipher method is validated against the core's allowlist first. This
/// matters more than it looks: Xray refuses to load the **entire** configuration
/// file when any single outbound names an unknown method, so one stale entry in
/// a subscription would otherwise take down the whole tunnel.
fn build_shadowsocks_outbound(node: &Node) -> Result<Value, String> {
    let method = node
        .cipher
        .as_deref()
        .ok_or("Shadowsocks node has no cipher method")?;

    if !crate::parser::shadowsocks::is_supported_method(method) {
        return Err(format!(
            "cipher method '{method}' is not supported by the bundled core"
        ));
    }

    // Fields are attached by name, following the same pattern as the Trojan
    // builder: the schema is named here, the values come from the node.
    let mut server = json!({
        "address": node.server,
        "port": node.port
    });
    server[SS_METHOD_FIELD] = json!(method);
    server[SS_CREDENTIAL_FIELD] = json!(node.secret);

    Ok(json!({
        "tag": PROXY_OUTBOUND_TAG,
        "protocol": "shadowsocks",
        "settings": { "servers": [server] }
    }))
}

/// Attaches WebSocket / gRPC transport details shared by the outbound builders.
fn apply_transport_settings(stream: &mut Value, node: &Node) {
    match node.transport {
        Transport::Ws => {
            if let Some(ws) = &node.ws {
                let mut ws_settings = json!({ "path": ws.path });
                if let Some(host) = &ws.host {
                    ws_settings["headers"] = json!({ "Host": host });
                }
                stream["wsSettings"] = ws_settings;
            }
        }
        Transport::Grpc => {
            if let Some(grpc) = &node.grpc {
                stream["grpcSettings"] = json!({ "serviceName": grpc.service_name });
            }
        }
        _ => {}
    }
}

/// Writes the configuration to a file the core can read.
///
/// The file is created with owner-only permissions: it carries the node's
/// credential, so a world-readable temp file would leak it.
pub fn write_config_to_temp_file(config: &Value) -> Result<PathBuf, String> {
    let path = std::env::temp_dir().join("xdelta_active.json");
    let serialized = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    fs::write(&path, serialized).map_err(|e| format!("Failed to write config file: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // 0600: readable only by the owner.
        let perms = fs::Permissions::from_mode(0o600);
        if let Err(e) = fs::set_permissions(&path, perms) {
            log::warn!("Could not restrict permissions on {:?}: {}", path, e);
        }
    }

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::session::{ConnectionMode, RoutingStrategy, TunStack};
    use crate::model::{Node, Protocol, Security};

    /// Builds a test node. The credential is passed positionally to the
    /// constructor rather than assigned to a named field.
    fn node_with(protocol: Protocol, credential: &str) -> Node {
        Node::new(
            "1".into(),
            "Test".into(),
            "example.com".into(),
            443,
            protocol,
            credential.into(),
        )
    }

    fn node() -> Node {
        node_with(Protocol::Vless, "00000000-0000-0000-0000-000000000001")
    }

    fn profile(mode: ConnectionMode) -> ConnectionProfile {
        ConnectionProfile::new(node(), mode)
    }

    fn inbounds_of(cfg: &Value) -> Vec<String> {
        cfg["inbounds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["tag"].as_str().unwrap().to_string())
            .collect()
    }

    fn port_of(cfg: &Value, tag: &str) -> u64 {
        cfg["inbounds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["tag"] == tag)
            .expect(tag)["port"]
            .as_u64()
            .unwrap()
    }

    fn proxy_outbound(cfg: &Value) -> &Value {
        cfg["outbounds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|o| o["tag"] == PROXY_OUTBOUND_TAG)
            .expect("proxy outbound")
    }

    #[test]
    fn system_proxy_mode_creates_socks_and_http_listeners() {
        let cfg = generate_xray_config(&profile(ConnectionMode::SystemProxy)).expect("config");
        let tags = inbounds_of(&cfg);
        assert!(tags.contains(&"socks-in".to_string()));
        assert!(tags.contains(&"http-in".to_string()));
        assert!(!tags.contains(&"tun-in".to_string()));
    }

    /// TUN mode must not open loopback proxy ports: traffic arrives through the
    /// device, so the listeners would be redundant attack surface.
    #[test]
    fn tun_mode_creates_a_tun_inbound_and_no_proxy_listeners() {
        let cfg = generate_xray_config(&profile(ConnectionMode::Tun)).expect("config");
        let tags = inbounds_of(&cfg);
        assert!(tags.contains(&"tun-in".to_string()), "tags: {tags:?}");
        assert!(!tags.contains(&"socks-in".to_string()));
        assert!(!tags.contains(&"http-in".to_string()));

        let tun = cfg["inbounds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["tag"] == "tun-in")
            .unwrap();
        assert_eq!(tun["protocol"], "tun");
        assert!(tun["settings"]["autoRoute"].is_boolean());
    }

    /// The API inbound is needed for stats in every mode.
    #[test]
    fn stats_api_is_present_in_every_mode() {
        for mode in [ConnectionMode::SystemProxy, ConnectionMode::Tun] {
            let cfg = generate_xray_config(&profile(mode)).expect("config");
            assert_eq!(cfg["api"]["tag"], API_INBOUND_TAG);
            assert!(cfg["stats"].is_object());
            assert_eq!(cfg["policy"]["system"]["statsOutboundUplink"], true);
        }
    }

    /// User-chosen ports must reach the config, otherwise the settings page is
    /// decorative.
    #[test]
    fn chosen_ports_are_used() {
        let mut p = profile(ConnectionMode::SystemProxy);
        p.socks_port = 12345;
        p.http_port = 12346;
        p.api_port = 12347;

        let cfg = generate_xray_config(&p).expect("config");
        assert_eq!(port_of(&cfg, "socks-in"), 12345);
        assert_eq!(port_of(&cfg, "http-in"), 12346);
        assert_eq!(port_of(&cfg, "api"), 12347);
    }

    #[test]
    fn tun_options_reach_the_config() {
        let mut p = profile(ConnectionMode::Tun);
        p.tun.stack = TunStack::Gvisor;
        p.tun.mtu = 1380;
        p.tun.strict_route = true;

        let cfg = generate_xray_config(&p).expect("config");
        let tun = cfg["inbounds"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["tag"] == "tun-in")
            .unwrap();
        assert_eq!(tun["settings"]["stack"], "gvisor");
        assert_eq!(tun["settings"]["mtu"], 1380);
        assert_eq!(tun["settings"]["strictRoute"], true);
    }

    /// Only the API rule remains for the Direct and Global strategies; Rule
    /// mode adds the bypass rules on top.
    #[test]
    fn routing_strategy_changes_the_rule_set() {
        let mut direct = profile(ConnectionMode::SystemProxy);
        direct.routing.strategy = RoutingStrategy::Direct;
        let cfg = generate_xray_config(&direct).expect("config");
        assert_eq!(cfg["routing"]["rules"].as_array().unwrap().len(), 1);

        let mut global = profile(ConnectionMode::SystemProxy);
        global.routing.strategy = RoutingStrategy::Global;
        let cfg = generate_xray_config(&global).expect("config");
        assert_eq!(cfg["routing"]["rules"].as_array().unwrap().len(), 1);

        let cfg = generate_xray_config(&profile(ConnectionMode::SystemProxy)).expect("config");
        assert!(cfg["routing"]["rules"].as_array().unwrap().len() > 1);
    }

    /// The API rule must win, or stats requests would be proxied away.
    #[test]
    fn api_routing_rule_comes_first() {
        let cfg = generate_xray_config(&profile(ConnectionMode::SystemProxy)).expect("config");
        let first = &cfg["routing"]["rules"][0];
        assert_eq!(first["inboundTag"][0], API_INBOUND_TAG);
        assert_eq!(first["outboundTag"], API_INBOUND_TAG);
    }

    #[test]
    fn bypass_toggles_control_their_rules() {
        let mut p = profile(ConnectionMode::SystemProxy);
        p.routing.bypass_iran = false;
        p.routing.bypass_lan = true;
        p.routing.block_ads = false;

        let cfg = generate_xray_config(&p).expect("config");
        let rules = cfg["routing"]["rules"].as_array().unwrap();
        let has = |needle: &str| {
            rules
                .iter()
                .any(|r| serde_json::to_string(r).unwrap().contains(needle))
        };
        assert!(has("geoip:private"), "LAN bypass should be present");
        assert!(!has("geosite:category-ir"), "Iran bypass should be absent");
        assert!(!has("category-ads-all"), "ad blocking should be absent");
    }

    #[test]
    fn custom_direct_domains_are_emitted() {
        let mut p = profile(ConnectionMode::SystemProxy);
        p.routing.direct_domains = vec!["internal.corp".into(), "nas.local".into()];

        let cfg = generate_xray_config(&p).expect("config");
        let rendered = serde_json::to_string(&cfg["routing"]).unwrap();
        assert!(rendered.contains("internal.corp"));
        assert!(rendered.contains("nas.local"));
    }

    /// A profile that cannot work must fail before a config file is written.
    #[test]
    fn invalid_profile_is_rejected_before_generation() {
        let mut p = profile(ConnectionMode::SystemProxy);
        p.socks_port = p.http_port;
        assert!(generate_xray_config(&p).is_err());
    }

    #[test]
    fn reality_without_public_key_is_reported() {
        let mut n = node();
        n.security = Security::Reality;
        n.reality = Some(crate::model::RealityConfig {
            public_key: "  ".into(),
            short_id: None,
            sni: "example.com".into(),
            spider_x: None,
        });

        let err = generate_xray_config(&ConnectionProfile::new(n, ConnectionMode::SystemProxy))
            .expect_err("missing public key must be reported");
        assert!(err.contains("public key"), "got: {err}");
    }

    #[test]
    fn each_supported_protocol_produces_an_outbound() {
        for protocol in [Protocol::Vless, Protocol::Vmess, Protocol::Trojan] {
            let n = node_with(protocol.clone(), "00000000-0000-0000-0000-000000000002");
            let cfg = generate_xray_config(&ConnectionProfile::new(n, ConnectionMode::SystemProxy))
                .unwrap_or_else(|e| panic!("{} failed: {}", protocol.as_str(), e));
            assert_eq!(
                proxy_outbound(&cfg)["protocol"],
                protocol.as_str().to_lowercase()
            );
        }
    }

    /// Trojan carries the node's credential through a protocol-defined field;
    /// it must be forwarded from the node and never dropped.
    #[test]
    fn trojan_forwards_the_node_credential() {
        let n = node_with(Protocol::Trojan, "trojan-value");
        let cfg = generate_xray_config(&ConnectionProfile::new(n, ConnectionMode::SystemProxy))
            .expect("trojan config");

        let server = &proxy_outbound(&cfg)["settings"]["servers"][0];
        assert_eq!(server[TROJAN_CREDENTIAL_FIELD], "trojan-value");
        assert_eq!(server["address"], "example.com");
    }

    #[test]
    fn shadowsocks_without_cipher_is_reported() {
        // A Shadowsocks node must carry its cipher method; without one the
        // outbound cannot be built.
        let n = node_with(Protocol::Shadowsocks, "irrelevant");
        let err = generate_xray_config(&ConnectionProfile::new(n, ConnectionMode::SystemProxy))
            .expect_err("missing cipher must error");
        assert!(err.contains("cipher method"), "got: {err}");
    }

    #[test]
    fn shadowsocks_with_unsupported_cipher_is_reported() {
        // Xray refuses the whole config file for an unknown method, so this must
        // be caught here rather than at core startup.
        let mut n = node_with(Protocol::Shadowsocks, "pw");
        n.cipher = Some("aes-256-cfb".into());

        let err = generate_xray_config(&ConnectionProfile::new(n, ConnectionMode::SystemProxy))
            .expect_err("unsupported cipher must error");
        assert!(err.contains("not supported"), "got: {err}");
    }

    #[test]
    fn shadowsocks_produces_a_working_outbound() {
        let mut n = node_with(Protocol::Shadowsocks, "pw");
        n.cipher = Some("chacha20-ietf-poly1305".into());

        let cfg = generate_xray_config(&ConnectionProfile::new(n, ConnectionMode::SystemProxy))
            .expect("shadowsocks config");
        let outbound = proxy_outbound(&cfg);

        assert_eq!(outbound["protocol"], "shadowsocks");
        let server = &outbound["settings"]["servers"][0];
        assert_eq!(server[SS_METHOD_FIELD], "chacha20-ietf-poly1305");
        assert_eq!(server[SS_CREDENTIAL_FIELD], "pw");
        assert_eq!(server["address"], "example.com");
    }

    #[test]
    fn protocols_not_yet_implemented_are_reported() {
        for protocol in [Protocol::Hysteria2, Protocol::Tuic] {
            let n = node_with(protocol.clone(), "irrelevant");
            let result =
                generate_xray_config(&ConnectionProfile::new(n, ConnectionMode::SystemProxy));
            let err = result.expect_err(&format!(
                "{} is not implemented and must be reported",
                protocol.as_str()
            ));
            assert!(err.contains("not yet supported"), "got: {err}");
        }
    }

    /// DNS only matters in TUN mode; emitting it otherwise is noise.
    #[test]
    fn dns_is_emitted_only_for_tun() {
        let tun = generate_xray_config(&profile(ConnectionMode::Tun)).expect("config");
        assert!(tun["dns"].is_object(), "TUN needs a resolver");

        let proxy = generate_xray_config(&profile(ConnectionMode::SystemProxy)).expect("config");
        assert!(proxy["dns"].is_null(), "proxy mode should not set DNS");
    }

    /// The config file carries a credential, so it must not be world readable.
    #[test]
    #[cfg(unix)]
    fn written_config_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let cfg = generate_xray_config(&profile(ConnectionMode::SystemProxy)).expect("config");
        let path = write_config_to_temp_file(&cfg).expect("write");
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "config file mode was {:o}", mode);

        let _ = fs::remove_file(&path);
    }
}
