//! Lightweight persistence for user preferences.
//!
//! Settings live in `~/.config/xdelta/settings.json` (respecting `XDG_CONFIG_HOME`).
//! Writing is best-effort: a failure to persist never breaks the running app.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
    let Some(path) = settings_path() else {
        return AppearanceSettings::default();
    };

    let Ok(raw) = std::fs::read_to_string(&path) else {
        return AppearanceSettings::default();
    };

    match serde_json::from_str::<AppearanceSettings>(&raw) {
        Ok(settings) => settings,
        Err(e) => {
            log::warn!("Ignoring malformed settings file {:?}: {}", path, e);
            AppearanceSettings::default()
        }
    }
}

/// Persists the appearance settings. Failures are logged, never propagated.
pub fn save_appearance(settings: &AppearanceSettings) {
    let Some(path) = settings_path() else {
        return;
    };

    match serde_json::to_string_pretty(settings) {
        Ok(json) => {
            log::debug!("PERSIST-WRITE {:?} -> {:?}", settings, path);
            if let Err(e) = std::fs::write(&path, json) {
                log::warn!("Could not write settings to {:?}: {}", path, e);
            }
        }
        Err(e) => log::warn!("Could not serialize settings: {}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
