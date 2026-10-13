//! Shadowsocks URI parsing.
//!
//! Two encodings are in circulation and both appear in real subscriptions, so
//! both are supported:
//!
//! * `ss://base64(method:password)@host:port#tag`  — SIP002 / modern clients
//! * `ss://base64(method:password@host:port)#tag`  — the legacy whole-payload form
//!
//! Subscriptions in the wild are dirty: base64 padding is often wrong, a few
//! entries are truncated mid-string, and the fragment carries emoji and
//! advertising text. Every one of those cases is handled explicitly rather than
//! assumed away, because a single unparseable line used to be able to abort the
//! whole import.

use crate::model::{Node, Protocol};
use base64::prelude::*;

/// Cipher methods the bundled Xray core accepts.
///
/// Deliberately an allowlist: an unknown method makes Xray refuse to load the
/// **entire** configuration file, so one bad entry would take down the whole
/// tunnel. Filtering here keeps that failure local to the unusable entry.
///
/// Verified against the installed core. Notably absent: `aes-256-cfb` and
/// `rc4-md5`, which are common in old subscriptions but rejected by Xray.
pub const SUPPORTED_METHODS: &[&str] = &[
    "chacha20-ietf-poly1305",
    "chacha20-poly1305",
    "xchacha20-ietf-poly1305",
    "aes-128-gcm",
    "aes-192-gcm",
    "aes-256-gcm",
    "2022-blake3-aes-128-gcm",
    "2022-blake3-aes-256-gcm",
    "2022-blake3-chacha20-poly1305",
    "none",
];

/// Whether the installed core can use this cipher method.
pub fn is_supported_method(method: &str) -> bool {
    SUPPORTED_METHODS.contains(&method.trim().to_ascii_lowercase().as_str())
}

/// Decodes base64 that may be standard or URL-safe, padded or not.
///
/// Real subscriptions mix all four combinations, and the `base64` crate's strict
/// decoders reject the unpadded and URL-safe variants, so each is tried.
fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = input
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();

    for decoder in [
        &BASE64_STANDARD,
        &BASE64_STANDARD_NO_PAD,
        &BASE64_URL_SAFE,
        &BASE64_URL_SAFE_NO_PAD,
    ] {
        if let Ok(bytes) = decoder.decode(&cleaned) {
            return Ok(bytes);
        }
    }

    Err("base64 payload could not be decoded".to_string())
}

/// Strips the noise that subscription authors put in the fragment.
///
/// Fragments carry emoji flags, channel adverts and zero-width joiners. Keeping
/// the raw text makes the server list unreadable, so only a sensible label is
/// retained.
pub fn clean_display_name(raw: &str) -> String {
    let decoded = percent_encoding::percent_decode_str(raw)
        .decode_utf8_lossy()
        .to_string();

    let mut out = String::with_capacity(decoded.len());
    let mut last_was_space = false;

    for ch in decoded.chars() {
        // Drop emoji and pictographs.
        let is_pictograph = matches!(
            ch as u32,
            0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0xFE0F | 0x200D | 0x2190..=0x21FF
        );
        if is_pictograph {
            continue;
        }

        let ch = if ch == '+' { ' ' } else { ch };
        if ch.is_whitespace() {
            if !last_was_space && !out.is_empty() {
                out.push(' ');
                last_was_space = true;
            }
        } else {
            out.push(ch);
            last_was_space = false;
        }
    }

    let trimmed = out.trim().trim_matches('|').trim().to_string();

    // Drop a fragment that is nothing but advertising for a Telegram channel.
    if trimmed.is_empty() || trimmed.starts_with('@') || trimmed.starts_with("t.me/") {
        return String::new();
    }

    trimmed
}

/// Parses a `ss://` URI into a node.
pub fn parse_shadowsocks_uri(uri_str: &str) -> Result<Node, String> {
    let body = uri_str
        .trim()
        .strip_prefix("ss://")
        .ok_or("not a shadowsocks URI")?;

    let (before_fragment, fragment) = match body.split_once('#') {
        Some((b, f)) => (b, Some(f)),
        None => (body, None),
    };

    // Query parameters (plugin settings) are not supported by this client; drop
    // them so they cannot corrupt the host/port split.
    let before_query = before_fragment.split('?').next().unwrap_or(before_fragment);

    let (method, credential, host, port) =
        if let Some((encoded, hostport)) = before_query.rsplit_once('@') {
            // SIP002: the userinfo half is base64(method:password).
            let decoded = decode_base64(encoded)?;
            let text = String::from_utf8(decoded)
                .map_err(|_| "credentials were not valid UTF-8".to_string())?;
            let (method, credential) = text
                .split_once(':')
                .ok_or("credentials are missing the method:password separator")?;

            let (host, port) = split_host_port(hostport)?;
            (method.to_string(), credential.to_string(), host, port)
        } else {
            // Legacy: the whole payload is base64(method:password@host:port).
            let decoded = decode_base64(before_query)?;
            let text = String::from_utf8(decoded)
                .map_err(|_| "payload was not valid UTF-8".to_string())?;
            let (credentials, hostport) = text
                .rsplit_once('@')
                .ok_or("payload is missing the host section")?;
            let (method, credential) = credentials
                .split_once(':')
                .ok_or("credentials are missing the method:password separator")?;

            let (host, port) = split_host_port(hostport)?;
            (method.to_string(), credential.to_string(), host, port)
        };

    let method = method.trim().to_ascii_lowercase();
    if method.is_empty() {
        return Err("cipher method is empty".to_string());
    }
    if !is_supported_method(&method) {
        return Err(format!(
            "cipher method '{method}' is not supported by the bundled core"
        ));
    }

    let name = fragment
        .map(clean_display_name)
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| format!("{}:{}", host, port));

    let mut node = Node::new(
        uuid::Uuid::new_v4().to_string(),
        name,
        host,
        port,
        Protocol::Shadowsocks,
        credential,
    );
    node.cipher = Some(method);
    node.security = crate::model::Security::None;

    Ok(node)
}

/// Splits `host:port`, handling bracketed IPv6 literals.
fn split_host_port(input: &str) -> Result<(String, u16), String> {
    let input = input.trim();

    // IPv6 in brackets: [::1]:8388
    let (host, port_str) = if let Some(rest) = input.strip_prefix('[') {
        let (inside, after) = rest.split_once(']').ok_or("unterminated IPv6 address")?;
        let port = after
            .strip_prefix(':')
            .ok_or("IPv6 address is missing its port")?;
        (inside.to_string(), port)
    } else {
        let (h, p) = input
            .rsplit_once(':')
            .ok_or("endpoint is missing its port")?;
        (h.trim().to_string(), p.trim())
    };

    if host.is_empty() {
        return Err("host is empty".to_string());
    }

    let port: u16 = port_str
        .trim()
        .parse()
        .map_err(|_| format!("invalid port '{port_str}'"))?;

    if port == 0 {
        return Err("port must not be 0".to_string());
    }

    Ok((host, port))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The common case: SIP002 form with the credentials base64 encoded.
    #[test]
    fn parses_sip002_form() {
        // base64("chacha20-ietf-poly1305:hunter2")
        let cred = BASE64_STANDARD.encode("chacha20-ietf-poly1305:hunter2");
        let uri = format!("ss://{cred}@198.98.53.130:443#United+States");

        let node = parse_shadowsocks_uri(&uri).expect("parse");
        assert_eq!(node.server, "198.98.53.130");
        assert_eq!(node.port, 443);
        assert_eq!(node.protocol, Protocol::Shadowsocks);
        assert_eq!(node.cipher.as_deref(), Some("chacha20-ietf-poly1305"));
        assert_eq!(node.secret, "hunter2");
        assert_eq!(node.name, "United States");
    }

    /// The legacy layout encodes the whole `method:pass@host:port` payload.
    #[test]
    fn parses_legacy_form() {
        let payload = BASE64_STANDARD.encode("aes-256-gcm:secret123@example.com:8388");
        let uri = format!("ss://{payload}#Legacy");

        let node = parse_shadowsocks_uri(&uri).expect("parse");
        assert_eq!(node.server, "example.com");
        assert_eq!(node.port, 8388);
        assert_eq!(node.cipher.as_deref(), Some("aes-256-gcm"));
        assert_eq!(node.secret, "secret123");
        assert_eq!(node.name, "Legacy");
    }

    /// Padding is frequently wrong in real lists; the unpadded decoder must save it.
    #[test]
    fn accepts_unpadded_base64() {
        let cred = BASE64_STANDARD
            .encode("aes-128-gcm:pw")
            .trim_end_matches('=')
            .to_string();
        let uri = format!("ss://{cred}@1.2.3.4:8388#n");
        assert!(parse_shadowsocks_uri(&uri).is_ok());
    }

    /// URL-safe alphabets appear in the wild as well.
    #[test]
    fn accepts_url_safe_base64() {
        let payload = "chacha20-ietf-poly1305:pa??ss/word+x";
        let cred = BASE64_URL_SAFE.encode(payload).replace('=', "");
        let uri = format!("ss://{cred}@1.2.3.4:8388#n");

        let node = parse_shadowsocks_uri(&uri).expect("parse urlsafe");
        assert!(node.secret.contains("ss/word"), "got {:?}", node.secret);
    }

    /// A password containing a colon must not truncate the credential.
    #[test]
    fn password_may_contain_a_colon() {
        let cred = BASE64_STANDARD.encode("aes-256-gcm:pa:ss:word");
        let uri = format!("ss://{cred}@1.2.3.4:8388#n");

        let node = parse_shadowsocks_uri(&uri).expect("parse");
        assert_eq!(node.secret, "pa:ss:word");
    }

    /// Xray rejects the whole config file for an unknown cipher, so the entry
    /// must be rejected here instead of poisoning the tunnel later.
    #[test]
    fn unsupported_method_is_rejected() {
        let cred = BASE64_STANDARD.encode("aes-256-cfb:pw");
        let uri = format!("ss://{cred}@1.2.3.4:8388#n");

        let err = parse_shadowsocks_uri(&uri).expect_err("cfb must be rejected");
        assert!(err.contains("not supported"), "got: {err}");
    }

    #[test]
    fn method_matching_is_case_insensitive() {
        assert!(is_supported_method("CHACHA20-IETF-POLY1305"));
        assert!(is_supported_method("  aes-256-gcm  "));
        assert!(!is_supported_method("rc4-md5"));
    }

    #[test]
    fn every_allowlisted_method_is_lowercase() {
        for method in SUPPORTED_METHODS {
            assert_eq!(
                *method,
                method.to_ascii_lowercase(),
                "allowlist entry {method} must be lowercase to match comparisons"
            );
        }
    }

    /// Broken entries must fail individually, never panic.
    #[test]
    fn malformed_inputs_are_rejected_without_panicking() {
        let cases: Vec<String> = vec![
            "ss://".into(),
            "ss://@@@".into(),
            "ss://not-base64!!@1.2.3.4:8388#x".into(),
            "ss://YWVzLTI1Ni1nY206cHc=@nohost".into(),
            "ss://YWVzLTI1Ni1nY206cHc=@:8388".into(),
            "ss://YWVzLTI1Ni1nY206cHc=@1.2.3.4:0".into(),
            "ss://YWVzLTI1Ni1nY206cHc=@1.2.3.4:notaport".into(),
            format!(
                "ss://{}@1.2.3.4:99999",
                BASE64_STANDARD.encode("chacha20-ietf-poly1305:pw")
            ),
        ];

        for case in &cases {
            assert!(
                parse_shadowsocks_uri(case).is_err(),
                "expected rejection for {case}"
            );
        }
    }

    #[test]
    fn non_shadowsocks_uri_is_rejected() {
        assert!(parse_shadowsocks_uri("vless://x@1.2.3.4:443").is_err());
    }

    /// IPv6 endpoints are bracketed and must not be split at the wrong colon.
    #[test]
    fn parses_bracketed_ipv6() {
        let cred = BASE64_STANDARD.encode("aes-256-gcm:pw");
        let uri = format!("ss://{cred}@[2001:db8::1]:8388#v6");

        let node = parse_shadowsocks_uri(&uri).expect("parse ipv6");
        assert_eq!(node.server, "2001:db8::1");
        assert_eq!(node.port, 8388);
    }

    /// Plugin query parameters are not supported; they must not break parsing.
    #[test]
    fn plugin_query_string_is_ignored() {
        let cred = BASE64_STANDARD.encode("aes-256-gcm:pw");
        let uri = format!("ss://{cred}@1.2.3.4:8388?plugin=obfs-local%3Bobfs%3Dhttp#n");

        let node = parse_shadowsocks_uri(&uri).expect("parse with plugin param");
        assert_eq!(node.server, "1.2.3.4");
        assert_eq!(node.port, 8388);
    }

    #[test]
    fn display_name_is_cleaned_of_noise() {
        assert_eq!(clean_display_name("United+States"), "United States");
        assert_eq!(clean_display_name("NL%20Server"), "NL Server");
        assert_eq!(
            clean_display_name("🇳🇱 Join Telegram:@Farah_VPN 🇳🇱"),
            "Join Telegram:@Farah_VPN"
        );
        // Pure advertising collapses to nothing so a host:port label is used.
        assert_eq!(clean_display_name("@ICV2RAY"), "");
        assert_eq!(
            clean_display_name("⚡️ KURDNET | کورد نت | @vpn_kurdnet"),
            "KURDNET | کورد نت | @vpn_kurdnet"
        );
    }

    /// When the fragment carries no usable label, the address is used instead of
    /// an empty title.
    #[test]
    fn name_falls_back_to_address() {
        let cred = BASE64_STANDARD.encode("aes-256-gcm:pw");
        let uri = format!("ss://{cred}@1.2.3.4:8388");
        let node = parse_shadowsocks_uri(&uri).expect("parse");
        assert_eq!(node.name, "1.2.3.4:8388");
    }

    #[test]
    fn fresh_node_has_no_verification_and_no_latency() {
        let cred = BASE64_STANDARD.encode("aes-256-gcm:pw");
        let uri = format!("ss://{cred}@1.2.3.4:8388#n");
        let node = parse_shadowsocks_uri(&uri).expect("parse");

        assert_eq!(node.verified, None);
        assert_eq!(node.latency_ms, None);
        assert_eq!(
            node.verification_state(),
            crate::model::VerificationState::Unknown
        );
    }
}
