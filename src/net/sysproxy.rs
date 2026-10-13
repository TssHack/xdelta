use gio::prelude::*;
use gio::Settings;

pub struct GnomeProxyManager;

impl GnomeProxyManager {
    const SCHEMA_PROXY: &'static str = "org.gnome.system.proxy";
    const SCHEMA_HTTP: &'static str = "org.gnome.system.proxy.http";
    const SCHEMA_HTTPS: &'static str = "org.gnome.system.proxy.https";
    const SCHEMA_SOCKS: &'static str = "org.gnome.system.proxy.socks";

    pub fn is_available() -> bool {
        let default_source = gio::SettingsSchemaSource::default();
        if let Some(source) = default_source {
            source.lookup(Self::SCHEMA_PROXY, true).is_some()
        } else {
            false
        }
    }

    pub fn enable(http_port: u16, socks_port: u16) -> Result<(), String> {
        if !Self::is_available() {
            return Err(
                "GNOME Settings schema 'org.gnome.system.proxy' not found on system".into(),
            );
        }

        let proxy_settings = Settings::new(Self::SCHEMA_PROXY);
        let http_settings = Settings::new(Self::SCHEMA_HTTP);
        let https_settings = Settings::new(Self::SCHEMA_HTTPS);
        let socks_settings = Settings::new(Self::SCHEMA_SOCKS);

        // Configure HTTP
        http_settings
            .set_string("host", "127.0.0.1")
            .map_err(|e| e.to_string())?;
        http_settings
            .set_int("port", http_port as i32)
            .map_err(|e| e.to_string())?;
        http_settings
            .set_boolean("enabled", true)
            .map_err(|e| e.to_string())?;

        // Configure HTTPS
        https_settings
            .set_string("host", "127.0.0.1")
            .map_err(|e| e.to_string())?;
        https_settings
            .set_int("port", http_port as i32)
            .map_err(|e| e.to_string())?;

        // Configure SOCKS5
        socks_settings
            .set_string("host", "127.0.0.1")
            .map_err(|e| e.to_string())?;
        socks_settings
            .set_int("port", socks_port as i32)
            .map_err(|e| e.to_string())?;

        // Enable Manual Mode
        proxy_settings
            .set_string("mode", "manual")
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    pub fn disable() -> Result<(), String> {
        if !Self::is_available() {
            return Ok(());
        }

        let proxy_settings = Settings::new(Self::SCHEMA_PROXY);
        proxy_settings
            .set_string("mode", "none")
            .map_err(|e| e.to_string())?;

        Ok(())
    }
}
