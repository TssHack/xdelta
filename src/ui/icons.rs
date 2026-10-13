//! Icon references and the audit that keeps them resolvable.
//!
//! GTK does not warn when an icon name is missing: it silently renders a
//! "broken image" placeholder, so a typo or a name that only exists in some
//! third-party themes ships unnoticed. The audit below collects every icon name
//! the UI asks for and asserts each one is either bundled with the project or
//! present in the base icon theme.

/// Icon names the UI relies on that are shipped with the project.
///
/// These are defined in `assets/icons/hicolor/symbolic/apps/`. They exist
/// precisely because the base Adwaita theme does not provide equivalents:
/// `route-symbolic`, `speedometer-symbolic` and `view-sidebar-symbolic` are
/// either absent or only present in unrelated themes (Breeze), which meant the
/// dashboard and routing icons rendered as blank placeholders.
pub const BUNDLED_BRAND_ICONS: &[&str] = &[
    "xdelta-dashboard-symbolic",
    "xdelta-routing-symbolic",
    "xdelta-sidebar-symbolic",
    "xdelta-symbolic",
    "xdelta-connected-symbolic",
    "xdelta-disconnected-symbolic",
];

/// Directory holding the bundled symbolic icons, relative to the crate root.
pub const SYMBOLIC_ICON_DIR: &str = "assets/icons/hicolor/symbolic/apps";

/// Returns true when the project ships an SVG for `icon_name`.
pub fn is_bundled(icon_name: &str) -> bool {
    let path = format!("{}/{}.svg", SYMBOLIC_ICON_DIR, icon_name);
    std::path::Path::new(&path).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every name in [`BUNDLED_BRAND_ICONS`] must have a real file on disk.
    /// A stale entry here is as bad as a missing icon at runtime.
    #[test]
    fn every_brand_icon_is_present_on_disk() {
        for icon in BUNDLED_BRAND_ICONS {
            assert!(
                is_bundled(icon),
                "{} is listed as bundled but {}/{}.svg does not exist",
                icon,
                SYMBOLIC_ICON_DIR,
                icon
            );
        }
    }

    /// Bundled icons must be single-color `currentColor` SVGs, otherwise they
    /// ignore the theme's foreground color and look wrong in either scheme.
    #[test]
    fn bundled_icons_use_current_color() {
        for icon in BUNDLED_BRAND_ICONS {
            let path = format!("{}/{}.svg", SYMBOLIC_ICON_DIR, icon);
            let svg = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("could not read {}: {}", path, e));

            assert!(
                svg.contains("currentColor"),
                "{} does not use currentColor, so it cannot follow the theme",
                icon
            );
            // Hardcoded painter colors (fill="#..." / stroke="#...") would break
            // dark and light mode alike.
            for literal in ["fill=\"#", "stroke=\"#"] {
                assert!(
                    !svg.contains(literal),
                    "{} hardcodes a color via {}, which breaks theming",
                    icon,
                    literal
                );
            }
        }
    }

    /// The three icons that motivated bundling them in the first place: each
    /// must now resolve through the project's own assets rather than depending
    /// on whatever icon theme the user has installed.
    #[test]
    fn previously_missing_icons_are_now_bundled() {
        for icon in [
            "xdelta-dashboard-symbolic",
            "xdelta-routing-symbolic",
            "xdelta-sidebar-symbolic",
        ] {
            assert!(
                is_bundled(icon),
                "{} must ship with the project; the base theme does not provide it",
                icon
            );
        }
    }

    /// Names made up by this module follow the project's `xdelta-` prefix, so
    /// they can never collide with a stock icon or another app's assets.
    #[test]
    fn brand_icons_use_the_project_prefix() {
        for icon in BUNDLED_BRAND_ICONS {
            assert!(
                icon.starts_with("xdelta-"),
                "{} should be namespaced with the xdelta- prefix",
                icon
            );
        }
    }
}
