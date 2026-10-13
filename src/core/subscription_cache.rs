//! Subscription cache and the bundled default subscription.
//!
//! Two concerns live here:
//!
//! * **The default subscription.** On a fresh install the application should
//!   come up with a usable server list instead of placeholder data, so a known
//!   public subscription is fetched automatically the first time.
//! * **Caching.** The default list is large and fetching it on every launch
//!   would make startup depend on the network. The parsed nodes are written to
//!   disk and reloaded until they go stale, so an offline launch still works.
//!
//! The cache stores the parsed nodes rather than the raw payload: parsing is the
//! part that benefits from being done once, and it lets a partially parseable
//! payload keep the entries that did work.

use crate::model::Node;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The subscription shipped as the default on a fresh install.
pub const DEFAULT_SUBSCRIPTION_URL: &str =
    "https://raw.githubusercontent.com/barry-far/V2ray-config/main/Splitted-By-Protocol/ss.txt";

/// Display name for the bundled subscription.
pub const DEFAULT_SUBSCRIPTION_NAME: &str = "Public Shadowsocks List";

/// How long cached nodes stay usable before a refresh is attempted.
pub const CACHE_TTL_HOURS: i64 = 12;

/// Cached nodes plus the metadata needed to decide on a refresh.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionCache {
    pub url: String,
    pub fetched_at: DateTime<Utc>,
    pub nodes: Vec<Node>,
}

impl SubscriptionCache {
    /// Whether the cache is old enough to warrant a refetch.
    pub fn is_stale(&self, now: DateTime<Utc>) -> bool {
        now - self.fetched_at > Duration::hours(CACHE_TTL_HOURS)
    }

    /// Whether the cache holds anything usable.
    pub fn is_usable(&self) -> bool {
        !self.nodes.is_empty()
    }
}

/// Directory for cached subscriptions, created on demand.
fn cache_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;

    let dir = base.join("xdelta").join("subscriptions");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("Could not create cache directory {:?}: {}", dir, e);
        return None;
    }

    Some(dir)
}

/// Turns a subscription URL into a stable, filesystem-safe cache file name.
///
/// The name is derived from the URL so different subscriptions cannot collide,
/// and sanitized because URLs contain characters a path cannot.
pub fn cache_file_name(url: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in url.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}.json")
}

/// Reads a cached subscription, if one exists and parses.
pub fn load_cache(url: &str) -> Option<SubscriptionCache> {
    let path = cache_dir()?.join(cache_file_name(url));
    let raw = std::fs::read_to_string(&path).ok()?;

    match serde_json::from_str::<SubscriptionCache>(&raw) {
        Ok(cache) => Some(cache),
        Err(e) => {
            log::warn!("Ignoring unreadable subscription cache {:?}: {}", path, e);
            None
        }
    }
}

/// Writes a subscription to the cache. Failures are logged, never fatal.
pub fn save_cache(url: &str, nodes: &[Node]) {
    let Some(dir) = cache_dir() else {
        return;
    };
    let path = dir.join(cache_file_name(url));

    let cache = SubscriptionCache {
        url: url.to_string(),
        fetched_at: Utc::now(),
        nodes: nodes.to_vec(),
    };

    match serde_json::to_string(&cache) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, json) {
                log::warn!("Could not write subscription cache {:?}: {}", path, e);
            }
        }
        Err(e) => log::warn!("Could not serialize subscription cache: {}", e),
    }
}

/// Decides whether a refresh should be attempted.
///
/// A stale-but-populated cache is still returned so the UI has something to
/// show immediately; the caller refreshes in the background.
pub fn cached_or_stale(url: &str, now: DateTime<Utc>) -> Option<SubscriptionCache> {
    load_cache(url).filter(|c| c.is_usable() && !c.is_stale(now))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Protocol;

    #[test]
    fn cache_file_names_are_stable_and_distinct() {
        let a = cache_file_name(DEFAULT_SUBSCRIPTION_URL);
        let b = cache_file_name(DEFAULT_SUBSCRIPTION_URL);
        let c = cache_file_name("https://example.com/other.txt");

        assert_eq!(a, b, "same URL must map to the same cache file");
        assert_ne!(a, c, "different URLs must not collide");
    }

    #[test]
    fn cache_file_names_are_filesystem_safe() {
        for url in [
            DEFAULT_SUBSCRIPTION_URL,
            "https://example.com/a?b=c&d=e#frag",
            "http://user:pass@host/path",
        ] {
            let name = cache_file_name(url);
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-'),
                "cache name {name} contains unsafe characters"
            );
            assert!(name.ends_with(".json"));
        }
    }

    fn node(id: &str) -> Node {
        Node::new(
            id.into(),
            "test".into(),
            "1.2.3.4".into(),
            8388,
            Protocol::Shadowsocks,
            "pw".into(),
        )
    }

    #[test]
    fn fresh_cache_is_not_stale() {
        let cache = SubscriptionCache {
            url: DEFAULT_SUBSCRIPTION_URL.into(),
            fetched_at: Utc::now(),
            nodes: vec![node("a")],
        };
        assert!(!cache.is_stale(Utc::now()));
    }

    #[test]
    fn old_cache_is_stale() {
        let cache = SubscriptionCache {
            url: DEFAULT_SUBSCRIPTION_URL.into(),
            fetched_at: Utc::now() - Duration::hours(CACHE_TTL_HOURS + 1),
            nodes: vec![node("a")],
        };
        assert!(cache.is_stale(Utc::now()));
    }

    /// An empty cache must never be served as valid; otherwise a failed fetch
    /// would leave the user with an empty list and no retry.
    #[test]
    fn empty_cache_is_not_usable() {
        let cache = SubscriptionCache {
            url: DEFAULT_SUBSCRIPTION_URL.into(),
            fetched_at: Utc::now(),
            nodes: Vec::new(),
        };
        assert!(!cache.is_usable());
    }

    #[test]
    fn cache_survives_a_serialization_round_trip() {
        let cache = SubscriptionCache {
            url: DEFAULT_SUBSCRIPTION_URL.into(),
            fetched_at: Utc::now(),
            nodes: vec![node("a"), node("b")],
        };

        let json = serde_json::to_string(&cache).expect("serialize");
        let restored: SubscriptionCache = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.nodes.len(), 2);
        assert_eq!(restored.url, cache.url);
    }

    /// The default subscription must be the Shadowsocks list the project ships
    /// with, not an arbitrary URL.
    #[test]
    fn default_url_points_at_the_bundled_list() {
        assert!(DEFAULT_SUBSCRIPTION_URL.starts_with("https://"));
        assert!(DEFAULT_SUBSCRIPTION_URL.ends_with("ss.txt"));
    }

    #[test]
    fn ttl_is_long_enough_to_avoid_refetching_on_every_launch() {
        assert!(CACHE_TTL_HOURS >= 1);
    }
}
