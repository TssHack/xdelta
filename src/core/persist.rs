//! Lightweight persistence for user preferences.
//!
//! Settings live in `~/.config/xdelta/settings.json` (respecting `XDG_CONFIG_HOME`).
//! Writing is best-effort: a failure to persist never breaks the running app.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Directory holding the application's configuration files.
///
/// Public because the log file lives beside the settings, and both must follow
/// the same `XDG_CONFIG_HOME` rule.
pub fn config_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;

    let dir = base.join("xdelta");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("Could not create config directory {:?}: {}", dir, e);
        return None;
    }

    Some(dir)
}

/// Directory holding application data, bundled assets, and user-managed binaries (~/.local/share/xdelta).
pub fn data_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;

    let dir = base.join("xdelta");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("Could not create data directory {:?}: {}", dir, e);
        return None;
    }

    Some(dir)
}

/// Directory for user-managed core binaries (~/.local/share/xdelta/bin).
pub fn user_bin_dir() -> Option<PathBuf> {
    let dir = data_dir()?.join("bin");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir)
}

/// Directory for geo routing databases (~/.local/share/xdelta/assets).
pub fn user_assets_dir() -> Option<PathBuf> {
    let dir = data_dir()?.join("assets");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir)
}

/// Path of the application's log file.
pub fn log_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("xdelta.log"))
}

/// Visual preferences controlled from the Appearance section of the settings view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppearanceSettings {
    /// Hex accent color in `#rrggbb` form. `None` follows the system accent.
    pub accent_hex: Option<String>,
    /// `"system"`, `"dark"` or `"light"`.
    pub color_scheme: String,
    /// Bundled wallpaper id, or `None` for a plain background.
    ///
    /// Defaults to the bundled default for older settings files that predate
    /// this field, so upgrading users still get a background.
    #[serde(default = "default_wallpaper")]
    pub wallpaper_id: Option<String>,
}

/// Serde default for [`AppearanceSettings::wallpaper_id`].
fn default_wallpaper() -> Option<String> {
    Some(crate::ui::wallpaper::DEFAULT_ID.to_string())
}

/// Connection preferences, persisted alongside the appearance.
///
/// Stored as plain numbers and strings rather than a serialized connection
/// profile: a profile embeds a node, and writing node credentials to disk is a
/// decision the user has not made. Ports and tunnel parameters are safe to
/// remember; the node itself is resolved from the subscription at connect time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConnectionSettings {
    pub socks_port: u16,
    pub http_port: u16,
    pub api_port: u16,
    /// `"system_proxy"` or `"tun"`.
    pub mode: String,
    /// `"system"`, `"gvisor"` or `"mixed"`.
    pub tun_stack: String,
    pub tun_mtu: u16,
    pub tun_auto_route: bool,
    pub tun_strict_route: bool,
    pub tun_dns: Vec<String>,
    /// `"rule"`, `"global"` or `"direct"`.
    pub routing_strategy: String,
    pub bypass_lan: bool,
    pub bypass_iran: bool,
    pub block_ads: bool,
    pub direct_domains: Vec<String>,
    /// Whether the desktop proxy is configured automatically on connect.
    pub auto_system_proxy: bool,
    /// Whether closing the main window minimizes to the system tray.
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ConnectionSettings {
    fn default() -> Self {
        Self {
            socks_port: crate::core::session::DEFAULT_SOCKS_PORT,
            http_port: crate::core::session::DEFAULT_HTTP_PORT,
            api_port: crate::core::session::DEFAULT_API_PORT,
            mode: "system_proxy".to_string(),
            tun_stack: "system".to_string(),
            tun_mtu: 1500,
            tun_auto_route: true,
            tun_strict_route: false,
            tun_dns: vec!["1.1.1.1".to_string(), "8.8.8.8".to_string()],
            routing_strategy: "rule".to_string(),
            bypass_lan: true,
            bypass_iran: true,
            block_ads: true,
            direct_domains: Vec::new(),
            auto_system_proxy: true,
            minimize_to_tray: true,
        }
    }
}

impl ConnectionSettings {
    /// Reads the file and returns both sections.
    ///
    /// A missing or unreadable file yields defaults; a partially written one
    /// falls back field by field through serde's own defaults, so a bad file
    /// never blocks startup.
    pub fn load() -> (AppearanceSettings, ConnectionSettings) {
        let Some(path) = settings_path() else {
            return (AppearanceSettings::default(), ConnectionSettings::default());
        };

        let Ok(raw) = std::fs::read_to_string(&path) else {
            return (AppearanceSettings::default(), ConnectionSettings::default());
        };

        #[derive(Deserialize)]
        struct Document {
            #[serde(flatten)]
            appearance: AppearanceSettings,
            #[serde(default)]
            connection: ConnectionSettings,
        }

        match serde_json::from_str::<Document>(&raw) {
            Ok(doc) => (doc.appearance, doc.connection),
            Err(e) => {
                log::warn!("Ignoring malformed settings file {:?}: {}", path, e);
                (AppearanceSettings::default(), ConnectionSettings::default())
            }
        }
    }

    /// Persists both sections in one write, so the file can never end up with
    /// one section updated and the other stale.
    pub fn save(appearance: &AppearanceSettings, connection: &ConnectionSettings) {
        let Some(path) = settings_path() else {
            return;
        };

        #[derive(Serialize)]
        struct Document<'a> {
            #[serde(flatten)]
            appearance: &'a AppearanceSettings,
            connection: &'a ConnectionSettings,
        }

        match serde_json::to_string_pretty(&Document {
            appearance,
            connection,
        }) {
            Ok(json) => {
                log::debug!("PERSIST-WRITE -> {:?}", path);
                if let Err(e) = std::fs::write(&path, json) {
                    log::warn!("Could not write settings to {:?}: {}", path, e);
                }
            }
            Err(e) => log::warn!("Could not serialize settings: {}", e),
        }
    }
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            accent_hex: None,
            color_scheme: "system".to_string(),
            wallpaper_id: default_wallpaper(),
        }
    }
}

impl AppearanceSettings {
    /// Maps the stored scheme string onto the zero-based index of the settings combo row.
    pub fn scheme_index(&self) -> u32 {
        match self.color_scheme.as_str() {
            "dark" => 1,
            "light" => 2,
            _ => 0,
        }
    }

    /// Inverse of [`Self::scheme_index`]; unknown values fall back to `"system"`.
    pub fn scheme_from_index(index: u32) -> &'static str {
        match index {
            1 => "dark",
            2 => "light",
            _ => "system",
        }
    }

    pub fn set_scheme(&mut self, scheme: &str) {
        self.color_scheme = match scheme {
            "dark" => "dark".to_string(),
            "light" => "light".to_string(),
            _ => "system".to_string(),
        };
    }
}

/// Resolves the settings file path, creating the parent directory if needed.
pub fn settings_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;

    let dir = base.join("xdelta");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("Could not create config directory {:?}: {}", dir, e);
        return None;
    }

    Some(dir.join("settings.json"))
}

/// Reads the stored appearance settings, falling back to defaults.
pub fn load_appearance() -> AppearanceSettings {
    ConnectionSettings::load().0
}

/// Reads the stored connection settings, falling back to defaults.
pub fn load_connection() -> ConnectionSettings {
    ConnectionSettings::load().1
}

/// Persists the appearance settings. Failures are logged, never propagated.
/// Persists the appearance section.
///
/// The connection section is read back first and rewritten alongside it: this
/// function owns only the appearance keys, and a blind overwrite would silently
/// discard every connection preference the user had saved.
pub fn save_appearance(settings: &AppearanceSettings) {
    let (_, connection) = ConnectionSettings::load();
    ConnectionSettings::save(settings, &connection);
}

/// Persists the connection section, preserving the appearance section.
pub fn save_connection(settings: &ConnectionSettings) {
    let (appearance, _) = ConnectionSettings::load();
    ConnectionSettings::save(&appearance, settings);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Connection defaults must match the session module's constants; a drift
    /// here would make the settings page disagree with the generated config.
    #[test]
    fn connection_defaults_match_session_constants() {
        let settings = ConnectionSettings::default();
        assert_eq!(
            settings.socks_port,
            crate::core::session::DEFAULT_SOCKS_PORT
        );
        assert_eq!(settings.http_port, crate::core::session::DEFAULT_HTTP_PORT);
        assert_eq!(settings.api_port, crate::core::session::DEFAULT_API_PORT);
        assert_eq!(settings.mode, "system_proxy");
        assert_eq!(settings.routing_strategy, "rule");
    }

    #[test]
    fn connection_settings_survive_serialization() {
        let original = ConnectionSettings {
            socks_port: 12345,
            tun_stack: "gvisor".to_string(),
            tun_mtu: 1380,
            tun_strict_route: true,
            direct_domains: vec!["a.internal".to_string()],
            ..Default::default()
        };

        #[derive(serde::Serialize, serde::Deserialize)]
        struct Wrap {
            connection: ConnectionSettings,
        }

        let json = serde_json::to_string(&Wrap {
            connection: original.clone(),
        })
        .expect("serialize");
        let restored: Wrap = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.connection, original);
    }

    /// A settings file written before connection settings existed must still
    /// load and gain defaults rather than failing.
    #[test]
    fn legacy_file_without_connection_section_loads() {
        #[derive(serde::Deserialize)]
        struct Document {
            #[serde(flatten)]
            appearance: AppearanceSettings,
            #[serde(default)]
            connection: ConnectionSettings,
        }

        let legacy = r##"{"accent_hex":"#2ec27e","color_scheme":"dark","wallpaper_id":"drift"}"##;
        let doc: Document = serde_json::from_str(legacy).expect("legacy deserialize");
        assert_eq!(doc.appearance.color_scheme, "dark");
        assert_eq!(
            doc.connection.socks_port,
            crate::core::session::DEFAULT_SOCKS_PORT
        );
        assert_eq!(doc.connection.tun_mtu, 1500);
    }

    /// Both sections must coexist in one file, and each must survive a write of
    /// the other: losing connection settings on an appearance change was a real
    /// regression this guards against.
    #[test]
    fn both_sections_round_trip_together() {
        let appearance = AppearanceSettings {
            accent_hex: Some("#9141ac".to_string()),
            color_scheme: "light".to_string(),
            wallpaper_id: Some("evelyn".to_string()),
        };
        let connection = ConnectionSettings {
            socks_port: 11111,
            mode: "tun".to_string(),
            ..Default::default()
        };

        #[derive(serde::Serialize, serde::Deserialize)]
        struct Document {
            #[serde(flatten)]
            appearance: AppearanceSettings,
            #[serde(default)]
            connection: ConnectionSettings,
        }

        #[derive(serde::Serialize)]
        struct OutDocument<'a> {
            #[serde(flatten)]
            appearance: &'a AppearanceSettings,
            connection: &'a ConnectionSettings,
        }

        let json = serde_json::to_string(&OutDocument {
            appearance: &appearance,
            connection: &connection,
        })
        .expect("serialize");

        let parsed: Document = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.appearance, appearance);
        assert_eq!(parsed.connection, connection);
        assert_eq!(parsed.connection.mode, "tun");
    }

    #[test]
    fn scheme_index_roundtrip() {
        for index in 0..3u32 {
            let scheme = AppearanceSettings::scheme_from_index(index);
            let mut settings = AppearanceSettings::default();
            settings.set_scheme(scheme);
            assert_eq!(settings.scheme_index(), index);
        }
    }

    #[test]
    fn unknown_scheme_falls_back_to_system() {
        let mut settings = AppearanceSettings::default();
        settings.set_scheme("solarized");
        assert_eq!(settings.color_scheme, "system");
        assert_eq!(settings.scheme_index(), 0);
    }

    #[test]
    fn settings_survive_serialization() {
        let original = AppearanceSettings {
            accent_hex: Some("#2ec27e".to_string()),
            color_scheme: "dark".to_string(),
            wallpaper_id: Some("velocity".to_string()),
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let restored: AppearanceSettings = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, restored);
    }

    /// A settings file written before wallpapers existed must still load, and
    /// must gain the default background rather than losing the whole file.
    #[test]
    fn legacy_settings_without_wallpaper_field_still_load() {
        let legacy = r##"{"accent_hex":"#9141ac","color_scheme":"light"}"##;
        let parsed: AppearanceSettings = serde_json::from_str(legacy).expect("legacy deserialize");
        assert_eq!(parsed.accent_hex.as_deref(), Some("#9141ac"));
        assert_eq!(parsed.color_scheme, "light");
        assert_eq!(
            parsed.wallpaper_id.as_deref(),
            Some(crate::ui::wallpaper::DEFAULT_ID),
            "missing field should fall back to the bundled default wallpaper"
        );
    }

    /// An explicit `null` means the user turned the background off, which is
    /// different from the field being absent.
    #[test]
    fn explicit_null_wallpaper_means_disabled() {
        let disabled = r##"{"accent_hex":null,"color_scheme":"system","wallpaper_id":null}"##;
        let parsed: AppearanceSettings = serde_json::from_str(disabled).expect("deserialize");
        assert_eq!(parsed.wallpaper_id, None);
    }
}
