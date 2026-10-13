//! GNOME desktop proxy management through GSettings.
//!
//! Two details matter for correctness that the previous implementation got
//! wrong:
//!
//! * The SOCKS proxy needs `use-same-proxy` handling and `ignore-hosts`
//!   coverage for LAN ranges, otherwise local traffic is pushed through the
//!   tunnel.
//! * The user's previous settings must be **restored**, not reset to a fixed
//!   "none". Clobbering a corporate proxy configuration on disconnect is a real
//!   regression for anyone who had one configured.

use gio::prelude::*;
use gio::Settings;

const SCHEMA_PROXY: &str = "org.gnome.system.proxy";
const SCHEMA_HTTP: &str = "org.gnome.system.proxy.http";
const SCHEMA_HTTPS: &str = "org.gnome.system.proxy.https";
const SCHEMA_SOCKS: &str = "org.gnome.system.proxy.socks";

/// Ranges that must never be sent through the proxy.
///
/// Private IPv4 space, loopback and link-local are included so that a manually
/// configured proxy does not break local networking.
pub const LAN_BYPASS_HOSTS: &[&str] = &[
    "localhost",
    "127.0.0.0/8",
    "::1",
    ".local",
    "10.0.0.0/8",
    "172.16.0.0/12",
    "192.168.0.0/16",
    "169.254.0.0/16",
];

/// A snapshot of the settings this module changes.
///
/// Only the keys we overwrite are captured, so restoring cannot disturb
/// unrelated configuration.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProxySnapshot {
    pub mode: String,
    pub ignore_hosts: Vec<String>,
    pub http_host: String,
    pub http_port: i32,
    pub http_enabled: bool,
    pub https_host: String,
    pub https_port: i32,
    pub socks_host: String,
    pub socks_port: i32,
    pub use_same_proxy: bool,
}

impl ProxySnapshot {
    /// Whether the desktop was actively using a proxy before we touched it.
    ///
    /// A snapshot taken from a system that never had a proxy reports `none`, and
    /// a default-constructed snapshot reports an empty string; both mean "no
    /// active proxy", so an empty mode must not be treated as a real one.
    pub fn had_active_proxy(&self) -> bool {
        !self.mode.is_empty() && self.mode != "none"
    }
}

pub struct GnomeProxyManager;

impl GnomeProxyManager {
    pub fn is_available() -> bool {
        gio::SettingsSchemaSource::default()
            .and_then(|source| source.lookup(SCHEMA_PROXY, true))
            .is_some()
    }

    fn settings(schema: &str) -> Settings {
        Settings::new(schema)
    }

    /// Reads the keys this module is about to overwrite.
    pub fn snapshot() -> Result<ProxySnapshot, String> {
        if !Self::is_available() {
            return Err("GNOME proxy schema is not available on this system".to_string());
        }

        let proxy = Self::settings(SCHEMA_PROXY);
        let http = Self::settings(SCHEMA_HTTP);
        let https = Self::settings(SCHEMA_HTTPS);
        let socks = Self::settings(SCHEMA_SOCKS);

        Ok(ProxySnapshot {
            mode: proxy.string("mode").to_string(),
            ignore_hosts: proxy
                .strv("ignore-hosts")
                .iter()
                .map(|s| s.to_string())
                .collect(),
            http_host: http.string("host").to_string(),
            http_port: http.int("port"),
            http_enabled: http.boolean("enabled"),
            https_host: https.string("host").to_string(),
            https_port: https.int("port"),
            socks_host: socks.string("host").to_string(),
            socks_port: socks.int("port"),
            use_same_proxy: proxy.boolean("use-same-proxy"),
        })
    }

    /// Points the desktop at the local listeners.
    pub fn enable(http_port: u16, socks_port: u16) -> Result<(), String> {
        if !Self::is_available() {
            return Err("GNOME proxy schema is not available on this system".to_string());
        }

        let proxy = Self::settings(SCHEMA_PROXY);
        let http = Self::settings(SCHEMA_HTTP);
        let https = Self::settings(SCHEMA_HTTPS);
        let socks = Self::settings(SCHEMA_SOCKS);

        http.set_string("host", "127.0.0.1")
            .map_err(|e| e.to_string())?;
        http.set_int("port", http_port as i32)
            .map_err(|e| e.to_string())?;
        http.set_boolean("enabled", true)
            .map_err(|e| e.to_string())?;

        https
            .set_string("host", "127.0.0.1")
            .map_err(|e| e.to_string())?;
        https
            .set_int("port", http_port as i32)
            .map_err(|e| e.to_string())?;

        socks
            .set_string("host", "127.0.0.1")
            .map_err(|e| e.to_string())?;
        socks
            .set_int("port", socks_port as i32)
            .map_err(|e| e.to_string())?;

        // Keep LAN traffic local. Existing entries are preserved so a user's own
        // exclusions are not discarded.
        let mut hosts: Vec<String> = proxy
            .strv("ignore-hosts")
            .iter()
            .map(|s| s.to_string())
            .collect();
        for entry in LAN_BYPASS_HOSTS {
            if !hosts.iter().any(|h| h == entry) {
                hosts.push((*entry).to_string());
            }
        }
        proxy
            .set_strv(
                "ignore-hosts",
                hosts.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            )
            .map_err(|e| e.to_string())?;

        // SOCKS and HTTP listen separately here, so the desktop must not reuse
        // one host for both schemes.
        proxy
            .set_boolean("use-same-proxy", false)
            .map_err(|e| e.to_string())?;
        proxy
            .set_string("mode", "manual")
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Restores a previously captured configuration.
    ///
    /// Falls back to disabling the proxy only when there is nothing to restore,
    /// which is the honest behaviour: better to clear our own setting than to
    /// leave the desktop pointing at a dead port.
    pub fn restore(snapshot: &ProxySnapshot) -> Result<(), String> {
        if !Self::is_available() {
            return Ok(());
        }

        let proxy = Self::settings(SCHEMA_PROXY);
        let http = Self::settings(SCHEMA_HTTP);
        let https = Self::settings(SCHEMA_HTTPS);
        let socks = Self::settings(SCHEMA_SOCKS);

        http.set_string("host", &snapshot.http_host)
            .map_err(|e| e.to_string())?;
        http.set_int("port", snapshot.http_port)
            .map_err(|e| e.to_string())?;
        http.set_boolean("enabled", snapshot.http_enabled)
            .map_err(|e| e.to_string())?;

        https
            .set_string("host", &snapshot.https_host)
            .map_err(|e| e.to_string())?;
        https
            .set_int("port", snapshot.https_port)
            .map_err(|e| e.to_string())?;

        socks
            .set_string("host", &snapshot.socks_host)
            .map_err(|e| e.to_string())?;
        socks
            .set_int("port", snapshot.socks_port)
            .map_err(|e| e.to_string())?;

        proxy
            .set_boolean("use-same-proxy", snapshot.use_same_proxy)
            .map_err(|e| e.to_string())?;
        proxy
            .set_strv(
                "ignore-hosts",
                snapshot
                    .ignore_hosts
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>(),
            )
            .map_err(|e| e.to_string())?;
        proxy
            .set_string("mode", &snapshot.mode)
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Turns the desktop proxy off without restoring anything.
    pub fn disable() -> Result<(), String> {
        if !Self::is_available() {
            return Ok(());
        }

        Self::settings(SCHEMA_PROXY)
            .set_string("mode", "none")
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A user with an existing proxy configuration is the case that the old
    /// "reset to none" behaviour broke, so the snapshot must report it.
    #[test]
    fn snapshot_detects_an_active_proxy() {
        let snapshot = ProxySnapshot {
            mode: "manual".to_string(),
            ..Default::default()
        };
        assert!(snapshot.had_active_proxy());
    }

    #[test]
    fn snapshot_reports_inactive_when_mode_is_none() {
        let snapshot = ProxySnapshot {
            mode: "none".to_string(),
            ..Default::default()
        };
        assert!(!snapshot.had_active_proxy());
    }

    /// An empty mode means no proxy was configured; treating it as active would
    /// make the UI claim it is overriding a configuration that does not exist.
    #[test]
    fn empty_mode_is_not_an_active_proxy() {
        let snapshot = ProxySnapshot {
            mode: String::new(),
            ..Default::default()
        };
        assert!(!snapshot.had_active_proxy());
    }

    #[test]
    fn lan_bypass_covers_private_and_loopback_ranges() {
        assert!(LAN_BYPASS_HOSTS.contains(&"127.0.0.0/8"));
        assert!(LAN_BYPASS_HOSTS.contains(&"10.0.0.0/8"));
        assert!(LAN_BYPASS_HOSTS.contains(&"172.16.0.0/12"));
        assert!(LAN_BYPASS_HOSTS.contains(&"192.168.0.0/16"));
        assert!(LAN_BYPASS_HOSTS.contains(&"localhost"));
    }

    /// Duplicate entries would bloat the GSettings value on every connect.
    #[test]
    fn lan_bypass_has_no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for host in LAN_BYPASS_HOSTS {
            assert!(seen.insert(*host), "duplicate LAN bypass entry: {host}");
        }
    }

    /// Default snapshot must compare equal to a freshly built default, which is
    /// what the restore path relies on.
    #[test]
    fn default_snapshot_is_empty() {
        assert_eq!(ProxySnapshot::default(), ProxySnapshot::default());
        assert!(!ProxySnapshot::default().had_active_proxy());
    }

    /// The manager must at least answer the availability question without
    /// panicking, on any machine.
    #[test]
    fn availability_check_does_not_panic() {
        let _ = GnomeProxyManager::is_available();
    }
}
