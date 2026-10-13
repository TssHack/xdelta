//! Bundled background wallpapers.
//!
//! The images are embedded with `include_bytes!` rather than read from disk so
//! the application keeps working after installation, when the source tree is no
//! longer next to the binary.
//!
//! Every entry is 2560×1440 JPEG (quality 90); the originals were 4K PNG/JPEG
//! files totalling ~15 MB, which was too heavy to add to the binary. A
//! background sits behind a scrim at reduced visual weight, so the smaller size
//! is not noticeable while the whole set stays under 3 MB.

use gtk4::gdk;
use gtk4::glib;

/// One bundled wallpaper.
pub struct Wallpaper {
    /// Stable identifier persisted in the settings file. Never localize this.
    pub id: &'static str,
    /// Human-readable name shown in the picker.
    pub name: &'static str,
    /// Raw JPEG bytes, embedded at compile time.
    pub bytes: &'static [u8],
}

/// The bundled wallpapers, in the order they appear in the picker.
pub const WALLPAPERS: &[Wallpaper] = &[
    Wallpaper {
        id: "drift",
        name: "Drift",
        bytes: include_bytes!("../../assets/wallpapers/drift.jpg"),
    },
    Wallpaper {
        id: "bloom",
        name: "Bloom",
        bytes: include_bytes!("../../assets/wallpapers/bloom.jpg"),
    },
    Wallpaper {
        id: "evelyn",
        name: "Evelyn",
        bytes: include_bytes!("../../assets/wallpapers/evelyn.jpg"),
    },
    Wallpaper {
        id: "profile",
        name: "Profile",
        bytes: include_bytes!("../../assets/wallpapers/profile.jpg"),
    },
    Wallpaper {
        id: "velocity",
        name: "Velocity",
        bytes: include_bytes!("../../assets/wallpapers/velocity.jpg"),
    },
];

/// The wallpaper shown on a fresh install: dark and uncluttered, so interface
/// text stays readable on top of it.
pub const DEFAULT_ID: &str = "drift";

/// Looks up a wallpaper by its persisted id.
pub fn find(id: &str) -> Option<&'static Wallpaper> {
    WALLPAPERS.iter().find(|w| w.id == id)
}

/// Decodes a wallpaper into a texture ready for a `GtkPicture`.
///
/// Decoding is done once per selection rather than per frame, so the cost of
/// the JPEG decode is not on the paint path.
pub fn load_texture(id: &str) -> Option<gdk::Texture> {
    let wallpaper = find(id)?;
    let bytes = glib::Bytes::from_static(wallpaper.bytes);
    match gdk::Texture::from_bytes(&bytes) {
        Ok(texture) => Some(texture),
        Err(e) => {
            log::warn!("Could not decode wallpaper '{}': {}", id, e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallpaper_ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for wallpaper in WALLPAPERS {
            assert!(
                seen.insert(wallpaper.id),
                "duplicate wallpaper id: {}",
                wallpaper.id
            );
        }
    }

    /// Ids are persisted to disk and compared on startup, so they must be
    /// stable, lowercase and free of spaces.
    #[test]
    fn wallpaper_ids_are_filename_safe() {
        for wallpaper in WALLPAPERS {
            assert!(
                !wallpaper.id.is_empty()
                    && wallpaper
                        .id
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "wallpaper id '{}' is not lowercase/dash/digit only",
                wallpaper.id
            );
            assert!(!wallpaper.name.trim().is_empty());
        }
    }

    #[test]
    fn default_wallpaper_exists() {
        assert!(
            find(DEFAULT_ID).is_some(),
            "DEFAULT_ID '{}' does not match any bundled wallpaper",
            DEFAULT_ID
        );
    }

    #[test]
    fn unknown_id_resolves_to_nothing() {
        assert!(find("no-such-wallpaper").is_none());
    }

    /// Each embedded asset must be a real JPEG. This catches a truncated or
    /// wrongly-encoded file at test time instead of when a user picks it.
    #[test]
    fn every_wallpaper_is_a_jpeg() {
        for wallpaper in WALLPAPERS {
            let bytes = wallpaper.bytes;
            assert!(
                bytes.len() > 1024,
                "wallpaper '{}' looks truncated ({} bytes)",
                wallpaper.id,
                bytes.len()
            );
            assert_eq!(
                &bytes[..2],
                &[0xFF, 0xD8],
                "wallpaper '{}' is not a JPEG (bad SOI marker)",
                wallpaper.id
            );
            assert_eq!(
                &bytes[bytes.len() - 2..],
                &[0xFF, 0xD9],
                "wallpaper '{}' is not a JPEG (missing EOI marker)",
                wallpaper.id
            );
        }
    }
}
