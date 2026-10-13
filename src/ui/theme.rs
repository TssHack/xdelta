//! Runtime theming: lets the user pick a brand accent color that is applied live
//! across the whole interface.
//!
//! The bundled `style.css` keeps its hardcoded defaults; we never rewrite it.
//! Instead a dedicated `CssProvider` with application priority layers a `:root`
//! override on top, which is the supported way to retheme GTK4 widgets.
//!
//! Note: this binding (libadwaita 0.7 / adw 1.10) only exposes the *readable*
//! `accent_color()`. Driving the accent through CSS variables is both the only
//! option here and the more flexible one, since it accepts arbitrary colors
//! rather than the nine predefined `AdwAccentColor` values.

use gtk4::gdk::Display;
use gtk4::CssProvider;
use std::cell::RefCell;

/// Named accent presets offered in the settings view.
///
/// `css_class` must match a rule defined in `data/resources/style.css`; a unit
/// test asserts that every preset color appears in the stylesheet so the two
/// cannot silently drift apart.
pub struct Preset {
    pub name: &'static str,
    pub hex: &'static str,
    pub css_class: &'static str,
}

pub const PRESETS: &[Preset] = &[
    Preset {
        name: "Emerald",
        hex: "#2ec27e",
        css_class: "preset-swatch-emerald",
    },
    Preset {
        name: "Cyan",
        hex: "#3584e4",
        css_class: "preset-swatch-cyan",
    },
    Preset {
        name: "Purple",
        hex: "#9141ac",
        css_class: "preset-swatch-purple",
    },
    Preset {
        name: "Rose",
        hex: "#e5484d",
        css_class: "preset-swatch-rose",
    },
    Preset {
        name: "Amber",
        hex: "#e5a50a",
        css_class: "preset-swatch-amber",
    },
];

thread_local! {
    /// Holds the override provider for the lifetime of the process.
    static PROVIDER: RefCell<Option<CssProvider>> = const { RefCell::new(None) };
}

/// Parses `#rrggbb` (or `#rgb`) into an `(r, g, b)` byte triple.
pub fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
    let trimmed = hex.trim().trim_start_matches('#');
    let expanded = match trimmed.len() {
        3 => trimmed.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 => trimmed.to_string(),
        _ => return None,
    };

    let parse = |range: std::ops::Range<usize>| u8::from_str_radix(&expanded[range], 16).ok();
    Some((parse(0..2)?, parse(2..4)?, parse(4..6)?))
}

/// Normalizes any accepted hex input into lowercase `#rrggbb`.
pub fn normalize_hex(hex: &str) -> Option<String> {
    let (r, g, b) = parse_hex(hex)?;
    Some(format!("#{:02x}{:02x}{:02x}", r, g, b))
}

/// Relative luminance (WCAG) of an sRGB triple, used to pick readable text.
fn relative_luminance(r: u8, g: u8, b: u8) -> f64 {
    let channel = |v: u8| {
        let c = v as f64 / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

/// Picks black or white text, whichever contrasts more with the background.
///
/// Uses the WCAG contrast ratio rather than a luminance midpoint: a mid-light
/// color such as amber sits below 0.5 luminance but is still far more readable
/// with dark text.
fn readable_foreground(accent_hex: &str) -> &'static str {
    let Some((r, g, b)) = parse_hex(accent_hex) else {
        return "#ffffff";
    };

    let l = relative_luminance(r, g, b);
    let ratio_with_black = (l + 0.05) / 0.05;
    let ratio_with_white = 1.05 / (l + 0.05);

    if ratio_with_black >= ratio_with_white {
        "#000000"
    } else {
        "#ffffff"
    }
}

/// Generates the color rules for the preset swatches from [`PRESETS`].
///
/// Keeping them here instead of in `style.css` means the palette exists in
/// exactly one place, so a swatch can never render a different color than the
/// one the accent picker actually applies.
fn swatch_css() -> String {
    let mut css = String::new();
    for preset in PRESETS {
        css.push_str(&format!(
            ".{} {{ background-color: {}; }}\n",
            preset.css_class, preset.hex
        ));
    }
    css
}

/// Builds the accent override block for one color.
///
/// Two mechanisms are needed because libadwaita reaches its widgets through
/// both:
///
/// * `--xdelta-brand*` are this stylesheet's own variables, used by every
///   custom xdelta surface.
/// * `@accent_bg_color` / `@accent_fg_color` are *named colors*. libadwaita
///   declares `:root { --accent-bg-color: @accent_bg_color; }`, so stock
///   widgets (switches, suggested buttons, selections, focus rings) only
///   follow the accent once the named color itself is overridden. Setting the
///   custom property alone leaves those widgets on the desktop theme's accent,
///   which is why only part of the interface used to recolor.
///
/// `--accent-color` is deliberately left to libadwaita, which derives a
/// contrast-safe variant from `--accent-bg-color` via oklab.
///
/// Semantic colors (success / warning / error) are deliberately left alone.
fn override_css(accent_hex: &str) -> String {
    let (r, g, b) = match parse_hex(accent_hex) {
        Some(rgb) => rgb,
        None => return String::new(),
    };
    let fg = readable_foreground(accent_hex);

    format!(
        "@define-color accent_bg_color {hex};\n\
         @define-color accent_fg_color {fg};\n\
         :root {{\n\
         \x20 --xdelta-brand: {hex};\n\
         \x20 --xdelta-brand-glow: rgba({r}, {g}, {b}, 0.35);\n\
         \x20 --xdelta-brand-bg: rgba({r}, {g}, {b}, 0.12);\n\
         \x20 --accent-bg-color: {hex};\n\
         \x20 --accent-fg-color: {fg};\n\
         }}\n",
        hex = accent_hex,
        fg = fg,
        r = r,
        g = g,
        b = b,
    )
}

/// Creates the override provider once and attaches it to the display.
///
/// The provider is registered at `USER` priority, above both the desktop theme
/// and libadwaita's own stylesheet. That matters because a desktop theme such
/// as `adw-gtk3` defines `@accent_bg_color` in its own provider; at the lower
/// `APPLICATION` priority the theme would win and the accent picker would only
/// affect part of the interface.
fn ensure_provider() -> Option<CssProvider> {
    PROVIDER.with(|cell| {
        let mut slot = cell.borrow_mut();
        if let Some(provider) = slot.as_ref() {
            return Some(provider.clone());
        }

        let display = Display::default()?;
        let provider = CssProvider::new();
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_USER,
        );
        *slot = Some(provider.clone());
        Some(provider)
    })
}

/// Applies an accent color to the running application immediately.
///
/// Passing `None` clears the override, restoring the system accent (or the
/// bundled default when the system exposes no accent).
pub fn apply(accent_hex: Option<&str>) {
    let Some(provider) = ensure_provider() else {
        return;
    };

    // Swatch colors are always present; the accent override is layered on top.
    let mut css = swatch_css();
    if let Some(hex) = accent_hex.and_then(normalize_hex) {
        css.push_str(&override_css(&hex));
    }

    provider.load_from_string(&css);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_and_short_hex() {
        assert_eq!(parse_hex("#2ec27e"), Some((46, 194, 126)));
        assert_eq!(parse_hex("2EC27E"), Some((46, 194, 126)));
        assert_eq!(parse_hex("#fff"), Some((255, 255, 255)));
    }

    #[test]
    fn rejects_malformed_hex() {
        assert_eq!(parse_hex("#12345"), None);
        assert_eq!(parse_hex("not-a-color"), None);
        assert_eq!(parse_hex("#gggggg"), None);
    }

    #[test]
    fn normalizes_to_lowercase_full_form() {
        assert_eq!(normalize_hex("#ABC"), Some("#aabbcc".to_string()));
        assert_eq!(normalize_hex("#2EC27E"), Some("#2ec27e".to_string()));
    }

    #[test]
    fn override_block_carries_accent_and_glow() {
        let css = override_css("#2ec27e");
        assert!(css.contains("--xdelta-brand: #2ec27e;"));
        assert!(css.contains("--xdelta-brand-glow: rgba(46, 194, 126, 0.35);"));
        assert!(css.contains("--xdelta-brand-bg: rgba(46, 194, 126, 0.12);"));
        assert!(css.contains("--accent-bg-color: #2ec27e;"));
    }

    /// Stock libadwaita widgets read the *named* color `@accent_bg_color`, not
    /// the custom property. Without this define, only xdelta's own stylesheet
    /// would recolor and switches / suggested buttons would keep the desktop
    /// theme's accent.
    #[test]
    fn override_redefines_libadwaita_named_colors() {
        let css = override_css("#2ec27e");
        assert!(
            css.contains("@define-color accent_bg_color #2ec27e;"),
            "must redefine the named color libadwaita resolves --accent-bg-color from"
        );
        assert!(
            css.contains("@define-color accent_fg_color"),
            "must provide a readable foreground for accent surfaces"
        );
    }

    /// A light accent (e.g. Amber) needs dark text; a dark accent needs light
    /// text. Getting this backwards renders unreadable labels.
    #[test]
    fn foreground_contrasts_with_the_accent() {
        assert_eq!(readable_foreground("#ffffff"), "#000000");
        assert_eq!(readable_foreground("#e5a50a"), "#000000");
        assert_eq!(readable_foreground("#000000"), "#ffffff");
        assert_eq!(readable_foreground("#2a638a"), "#ffffff");
    }

    /// Every preset must clear the WCAG AA text threshold (4.5:1) against the
    /// foreground this module picks for it.
    #[test]
    fn every_preset_passes_wcag_aa() {
        for preset in PRESETS {
            let (r, g, b) = parse_hex(preset.hex).expect("valid preset hex");
            let l = relative_luminance(r, g, b);
            let fg = readable_foreground(preset.hex);
            let ratio = if fg == "#000000" {
                (l + 0.05) / 0.05
            } else {
                1.05 / (l + 0.05)
            };
            assert!(
                ratio >= 4.5,
                "preset {} ({}) only reaches {:.2}:1 contrast with {}",
                preset.name,
                preset.hex,
                ratio,
                fg
            );
        }
    }

    /// The swatch rules are generated from `PRESETS`, so the palette lives in
    /// exactly one place and cannot disagree with what the picker applies.
    #[test]
    fn swatch_css_covers_every_preset() {
        let css = swatch_css();
        for preset in PRESETS {
            assert!(
                css.contains(&format!(
                    ".{} {{ background-color: {}; }}",
                    preset.css_class, preset.hex
                )),
                "missing generated swatch rule for {}",
                preset.name
            );
        }
    }

    /// Semantic colors must survive a theme change, otherwise "poor latency"
    /// would render green under an emerald accent and lose its meaning.
    #[test]
    fn override_block_leaves_semantic_colors_untouched() {
        let css = override_css("#9141ac");
        assert!(!css.contains("--xdelta-success"));
        assert!(!css.contains("--xdelta-warning"));
        assert!(!css.contains("--xdelta-error"));
    }

    #[test]
    fn override_block_is_empty_for_bad_input() {
        assert!(override_css("nope").is_empty());
    }

    #[test]
    fn presets_are_all_valid_hex() {
        for preset in PRESETS {
            assert!(
                normalize_hex(preset.hex).is_some(),
                "preset {} has invalid color {}",
                preset.name,
                preset.hex
            );
        }
    }

    /// Guards against the stylesheet referencing brand colors that the accent
    /// picker cannot reach. Swatch colors are generated at runtime, so the
    /// stylesheet must not carry palette literals of its own.
    #[test]
    fn stylesheet_has_no_hardcoded_brand_colors() {
        let css = include_str!("../../data/resources/style.css");
        // Strip the `:root` block: that is where the literal palette belongs.
        let body = css
            .split_once(
                "/* --------------------------------------------------------------------------",
            )
            .map(|(_, rest)| rest)
            .unwrap_or(css);

        for preset in PRESETS {
            assert!(
                !body.contains(preset.hex),
                "style.css uses the literal color {} (preset {}) outside :root; \
                 reference a --xdelta-brand* variable instead",
                preset.hex,
                preset.name
            );
        }
        assert!(
            !body.contains("--xdelta-cyan") && !body.contains("--xdelta-emerald"),
            "style.css still references a retired palette variable"
        );
    }

    /// GTK only reports CSS problems through a signal, and a broken rule fails
    /// silently at runtime. This parses the real stylesheet and the generated
    /// override block so syntax errors surface as test failures instead.
    ///
    /// Parsing requires an initialized GTK, so the test is skipped (and says so)
    /// on headless machines such as CI containers without a display.
    #[test]
    fn stylesheets_parse_without_errors() {
        if gtk4::init().is_err() {
            eprintln!("skipping stylesheets_parse_without_errors: no display available");
            return;
        }

        let sources = [
            (
                "data/resources/style.css",
                include_str!("../../data/resources/style.css").to_string(),
            ),
            ("generated override", override_css("#2ec27e")),
        ];

        for (label, css) in sources {
            let provider = CssProvider::new();
            let errors = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
            let sink = errors.clone();
            provider.connect_parsing_error(move |_, _section, error| {
                sink.borrow_mut().push(error.message().to_string());
            });

            provider.load_from_string(&css);
            assert!(
                errors.borrow().is_empty(),
                "{} produced CSS parsing errors: {:?}",
                label,
                errors.borrow()
            );
        }
    }
}
