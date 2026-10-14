use gtk4::prelude::*;
use gtk4::{
    Align, Box, Button, ColorDialog, ColorDialogButton, Entry, Label, Orientation, ScrolledWindow,
    Switch,
};
use libadwaita::prelude::*;
use libadwaita::{AboutDialog, ActionRow, ComboRow, PreferencesGroup, StyleManager};

use crate::core::persist::{self, AppearanceSettings};
use crate::ui::theme;
use crate::ui::wallpaper;
use std::cell::RefCell;
use std::rc::Rc;

pub struct SettingsView {
    pub container: ScrolledWindow,
    pub socks_port_entry: Entry,
    pub http_port_entry: Entry,
    pub theme_row: ComboRow,
    pub auto_proxy_switch: Switch,
    /// Selects how traffic is captured: System Proxy or TUN.
    pub connection_mode_row: ComboRow,
    /// TUN device parameters.
    pub tun_stack_row: ComboRow,
    pub tun_mtu_entry: Entry,
    pub tun_dns_entry: Entry,
    pub tun_auto_route_switch: Switch,
    pub tun_strict_route_switch: Switch,
    /// Runs the one-time capability setup for TUN.
    pub tun_setup_button: Button,
    /// Shows whether TUN is usable right now.
    pub tun_status_label: Label,
    /// Selects how traffic is routed: Rule, Global or Direct.
    pub routing_strategy_row: ComboRow,
    /// Invoked with the new wallpaper id when the user picks one; `None`
    /// disables the background. Set by `connect_wallpaper_changed`.
    wallpaper_callback: WallpaperCallback,
}

/// Boxed listener for wallpaper changes.
///
/// The fully-qualified `std::boxed::Box` is required because this module also
/// imports `gtk4::Box`.
type WallpaperCallback = Rc<RefCell<Option<std::boxed::Box<dyn Fn(Option<&str>)>>>>;

impl SettingsView {
    pub fn new() -> Self {
        let content_box = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(20)
            .margin_top(20)
            .margin_bottom(24)
            .margin_start(28)
            .margin_end(28)
            .build();

        // 1. Core Selection & Binary Path
        let core_group = PreferencesGroup::builder()
            .title("Core Engine")
            .description("Select which proxy engine handles outbound connections")
            .build();

        let core_model = gtk4::StringList::new(&["Xray-Core (Active)", "Sing-Box (Experimental)"]);
        let core_row = ComboRow::builder()
            .title("Active Core")
            .subtitle("Dynamic process orchestration")
            .model(&core_model)
            .selected(0)
            .build();
        core_row.add_prefix(&gtk4::Image::from_icon_name("utilities-terminal-symbolic"));

        let binary_row = ActionRow::builder().title("Xray Core Engine").build();
        binary_row.add_prefix(&gtk4::Image::from_icon_name(
            "application-x-executable-symbolic",
        ));

        let binary_status_badge = Label::builder().build();
        binary_status_badge.add_css_class("badge-latency");

        let download_core_btn = Button::builder()
            .label("Download Core")
            .valign(Align::Center)
            .visible(false)
            .build();
        download_core_btn.add_css_class("suggested-action");

        binary_row.add_suffix(&download_core_btn);
        binary_row.add_suffix(&binary_status_badge);

        let refresh_core_row = {
            let binary_row = binary_row.clone();
            let badge = binary_status_badge.clone();
            let btn = download_core_btn.clone();
            std::rc::Rc::new(move || {
                match crate::core::process::CoreProcessManager::find_xray_binary() {
                    Ok(path) => {
                        let path_str = path.to_string_lossy().to_string();
                        let is_bundled = path_str.contains("/usr/lib/xdelta")
                            || path_str.contains(".local/share/xdelta");
                        binary_row.set_subtitle(&path_str);
                        badge.set_label(if is_bundled { "Bundled" } else { "System" });
                        badge.remove_css_class("latency-poor");
                        badge.add_css_class("latency-good");
                        btn.set_visible(false);
                    }
                    Err(_) => {
                        binary_row
                            .set_subtitle("Not found — click Download to install automatically");
                        badge.set_label("Missing");
                        badge.remove_css_class("latency-good");
                        badge.add_css_class("latency-poor");
                        btn.set_visible(true);
                        btn.set_sensitive(true);
                        btn.set_label("Download Core");
                    }
                }
            })
        };

        refresh_core_row();

        let refresh_clone = refresh_core_row.clone();
        let btn_clone = download_core_btn.clone();
        download_core_btn.connect_clicked(move |_| {
            btn_clone.set_sensitive(false);
            btn_clone.set_label("Downloading...");
            let refresh_cb = refresh_clone.clone();
            let btn_inner = btn_clone.clone();

            glib::MainContext::default().spawn_local(async move {
                let status_btn = btn_inner.clone();
                let res = crate::core::downloader::download_and_install_xray(move |msg| {
                    status_btn.set_label(msg);
                })
                .await;

                match res {
                    Ok(_) => {
                        refresh_cb();
                    }
                    Err(e) => {
                        log::error!("Core download error: {e}");
                        btn_inner.set_sensitive(true);
                        btn_inner.set_label("Retry Download");
                    }
                }
            });
        });

        core_group.add(&core_row);
        core_group.add(&binary_row);

        // 2. Inbound Network Ports
        let ports_group = PreferencesGroup::builder()
            .title("Local Inbound Endpoints")
            .description("Define local listener ports for desktop traffic")
            .build();

        let socks_port_entry = Entry::builder()
            .text("10808")
            .max_width_chars(6)
            .valign(Align::Center)
            .build();
        let socks_row = ActionRow::builder()
            .title("Local SOCKS5 Port")
            .subtitle("Default: 10808")
            .activatable_widget(&socks_port_entry)
            .build();
        socks_row.add_prefix(&gtk4::Image::from_icon_name("network-wired-symbolic"));
        socks_row.add_suffix(&socks_port_entry);

        let http_port_entry = Entry::builder()
            .text("10809")
            .max_width_chars(6)
            .valign(Align::Center)
            .build();
        let http_row = ActionRow::builder()
            .title("Local HTTP Proxy Port")
            .subtitle("Default: 10809")
            .activatable_widget(&http_port_entry)
            .build();
        http_row.add_prefix(&gtk4::Image::from_icon_name("web-browser-symbolic"));
        http_row.add_suffix(&http_port_entry);

        // 2b. Capture Mode & TUN
        let tunnel_group = PreferencesGroup::builder()
            .title("Tunnel Mode")
            .description("How traffic is captured. TUN routes the whole system, proxy mode only cooperating apps")
            .build();

        let mode_model = gtk4::StringList::new(&["System Proxy (userspace)", "TUN (whole system)"]);
        let connection_mode_row = ComboRow::builder()
            .title("Capture Mode")
            .subtitle("TUN requires a one-time permission grant")
            .model(&mode_model)
            .selected(0)
            .build();
        connection_mode_row.add_prefix(&gtk4::Image::from_icon_name("network-vpn-symbolic"));

        let tun_stack_model =
            gtk4::StringList::new(&["System (kernel)", "gVisor (userspace)", "Mixed"]);
        let tun_stack_row = ComboRow::builder()
            .title("TUN Stack")
            .model(&tun_stack_model)
            .selected(0)
            .build();
        tun_stack_row.add_prefix(&gtk4::Image::from_icon_name("network-wired-symbolic"));

        let tun_mtu_entry = Entry::builder()
            .text("1500")
            .max_width_chars(6)
            .valign(Align::Center)
            .build();
        let mtu_row = ActionRow::builder()
            .title("MTU")
            .subtitle("Interface packet size (576-9000)")
            .activatable_widget(&tun_mtu_entry)
            .build();
        mtu_row.add_suffix(&tun_mtu_entry);

        let tun_dns_entry = Entry::builder()
            .text("1.1.1.1, 8.8.8.8")
            .hexpand(true)
            .valign(Align::Center)
            .build();
        let dns_row = ActionRow::builder()
            .title("Tunnel DNS")
            .subtitle("Resolvers used inside the tunnel")
            .activatable_widget(&tun_dns_entry)
            .build();
        dns_row.add_suffix(&tun_dns_entry);

        let tun_auto_route_switch = Switch::builder().active(true).valign(Align::Center).build();
        let auto_route_row = ActionRow::builder()
            .title("Install Default Routes")
            .subtitle("Let the core route system traffic through the tunnel")
            .activatable_widget(&tun_auto_route_switch)
            .build();
        auto_route_row.add_suffix(&tun_auto_route_switch);

        let tun_strict_route_switch = Switch::builder()
            .active(false)
            .valign(Align::Center)
            .build();
        let strict_route_row = ActionRow::builder()
            .title("Strict Route (leak guard)")
            .subtitle("Refuse to fall back to a direct connection")
            .activatable_widget(&tun_strict_route_switch)
            .build();
        strict_route_row.add_suffix(&tun_strict_route_switch);

        let tun_status_label = Label::builder()
            .label("Checking...")
            .halign(Align::Start)
            .wrap(true)
            .build();
        tun_status_label.add_css_class("caption");

        let tun_setup_button = Button::builder()
            .label("Grant Permission")
            .valign(Align::Center)
            .build();
        tun_setup_button.add_css_class("suggested-action");

        let tun_status_row = ActionRow::builder()
            .title("TUN Permission")
            .subtitle("One-time setup so the core can create the tunnel device")
            .build();
        tun_status_row.add_prefix(&gtk4::Image::from_icon_name("security-high-symbolic"));
        tun_status_row.add_suffix(&tun_setup_button);

        // Read-only status text. This row is deliberately NOT activatable: a
        // `Label` cannot be activated, and pointing `activatable_widget` at a
        // child of the row creates a cycle (row activate -> mnemonic_activate
        // on the label -> propagates back to the row) that recurses until the
        // stack overflows.
        let tun_status_box = ActionRow::builder().title("Status").build();
        tun_status_box.add_suffix(&tun_status_label);

        tunnel_group.add(&connection_mode_row);
        tunnel_group.add(&tun_stack_row);
        tunnel_group.add(&mtu_row);
        tunnel_group.add(&dns_row);
        tunnel_group.add(&auto_route_row);
        tunnel_group.add(&strict_route_row);
        tunnel_group.add(&tun_status_row);
        tunnel_group.add(&tun_status_box);

        // 2c. Routing strategy
        let routing_group = PreferencesGroup::builder()
            .title("Routing")
            .description("How destinations are classified before forwarding")
            .build();

        let strategy_model = gtk4::StringList::new(&[
            "Rule (bypass domestic & private)",
            "Global (proxy everything)",
            "Direct (proxy nothing)",
        ]);
        let routing_strategy_row = ComboRow::builder()
            .title("Strategy")
            .model(&strategy_model)
            .selected(0)
            .build();
        routing_strategy_row.add_prefix(&gtk4::Image::from_icon_name("xdelta-routing-symbolic"));

        routing_group.add(&routing_strategy_row);

        ports_group.add(&socks_row);
        ports_group.add(&http_row);

        // 3. System Proxy & Desktop Behavior
        let sys_group = PreferencesGroup::builder()
            .title("Desktop Integration")
            .description("System proxy and desktop environment hooks")
            .build();

        let auto_proxy_switch = Switch::builder().active(true).valign(Align::Center).build();
        let auto_proxy_row = ActionRow::builder()
            .title("Auto System Proxy")
            .subtitle("Automatically set GNOME proxy settings upon connection")
            .activatable_widget(&auto_proxy_switch)
            .build();
        auto_proxy_row.add_prefix(&gtk4::Image::from_icon_name(
            "preferences-system-network-symbolic",
        ));
        auto_proxy_row.add_suffix(&auto_proxy_switch);

        let clear_on_exit = Switch::builder().active(true).valign(Align::Center).build();
        let clear_row = ActionRow::builder()
            .title("Revert Proxy on Exit")
            .subtitle("Clear system proxy settings when xdelta terminates")
            .activatable_widget(&clear_on_exit)
            .build();
        clear_row.add_prefix(&gtk4::Image::from_icon_name("system-shutdown-symbolic"));
        clear_row.add_suffix(&clear_on_exit);

        sys_group.add(&auto_proxy_row);
        sys_group.add(&clear_row);

        // 4. Appearance
        let theme_group = PreferencesGroup::builder()
            .title("Appearance")
            .description("Interface theming and visual behavior")
            .build();

        // Restore the persisted color scheme selection.
        let saved_appearance = persist::load_appearance();

        let theme_model = gtk4::StringList::new(&["System Default", "Dark Mode", "Light Mode"]);
        let theme_row = ComboRow::builder()
            .title("Color Scheme")
            .model(&theme_model)
            .selected(saved_appearance.scheme_index())
            .build();
        theme_row.add_prefix(&gtk4::Image::from_icon_name(
            "applications-graphics-symbolic",
        ));

        theme_row.connect_selected_notify(|row| {
            let style_manager = StyleManager::default();
            match row.selected() {
                1 => style_manager.set_color_scheme(libadwaita::ColorScheme::ForceDark),
                2 => style_manager.set_color_scheme(libadwaita::ColorScheme::ForceLight),
                _ => style_manager.set_color_scheme(libadwaita::ColorScheme::Default),
            }

            let scheme = AppearanceSettings::scheme_from_index(row.selected());
            let mut stored = persist::load_appearance();
            stored.set_scheme(scheme);
            persist::save_appearance(&stored);
        });

        theme_group.add(&theme_row);

        // Accent color: live-applied brand color, persisted across restarts.
        let saved = saved_appearance;
        let effective_accent = saved
            .accent_hex
            .clone()
            .unwrap_or_else(take_system_accent_hex);

        let accent_button = ColorDialogButton::builder()
            .dialog(
                &ColorDialog::builder()
                    .title("Choose Accent Color")
                    .with_alpha(false)
                    .modal(true)
                    .build(),
            )
            .valign(Align::Center)
            .tooltip_text("Pick a custom accent color")
            .build();
        if let Ok(rgba) = gtk4::gdk::RGBA::parse(&effective_accent) {
            accent_button.set_rgba(&rgba);
        }

        let accent_row = ActionRow::builder()
            .title("Accent Color")
            .subtitle("Recolors the interface instantly")
            .build();
        accent_row.add_prefix(&gtk4::Image::from_icon_name(
            "applications-graphics-symbolic",
        ));
        accent_row.add_suffix(&accent_button);

        let current_swatch = Label::new(None);
        current_swatch.add_css_class("badge-latency");
        set_swatch(&current_swatch, &effective_accent);
        accent_row.add_suffix(&current_swatch);

        // Ensure the swatch + accent rules are on the display before the view
        // is realized, so a freshly launched window is already themed.
        theme::apply(saved.accent_hex.as_deref());

        let presets_box = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .valign(Align::Center)
            .build();

        // One preset button per brand palette. Colors come from the same
        // `theme::PRESETS` table the accent picker applies, so a swatch can
        // never advertise a color that selecting it would not produce.
        for preset in theme::PRESETS {
            let swatch = Button::builder()
                .tooltip_text(preset.name)
                .valign(Align::Center)
                .build();
            swatch.add_css_class("preset-swatch");
            swatch.add_css_class(preset.css_class);
            swatch.set_size_request(26, 26);
            swatch.update_property(&[gtk4::accessible::Property::Label(&format!(
                "{} accent color",
                preset.name
            ))]);

            let hex_owned = preset.hex.to_string();
            let button_clone = accent_button.clone();
            let swatch_label_clone = current_swatch.clone();
            swatch.connect_clicked(move |_| {
                theme::apply(Some(&hex_owned));
                if let Ok(rgba) = gtk4::gdk::RGBA::parse(&hex_owned) {
                    button_clone.set_rgba(&rgba);
                }
                set_swatch(&swatch_label_clone, &hex_owned);
                persist_accent(Some(hex_owned.clone()));
            });

            presets_box.append(&swatch);
        }

        let reset_button = Button::builder()
            .label("System Default")
            .valign(Align::Center)
            .build();
        reset_button.add_css_class("flat");

        let button_clone = accent_button.clone();
        let swatch_label_clone = current_swatch.clone();
        reset_button.connect_clicked(move |_| {
            theme::apply(None);
            let system_accent = take_system_accent_hex();
            if let Ok(rgba) = gtk4::gdk::RGBA::parse(&system_accent) {
                button_clone.set_rgba(&rgba);
            }
            set_swatch(&swatch_label_clone, &system_accent);
            persist_accent(None);
        });

        presets_box.append(&reset_button);

        let presets_row = ActionRow::builder()
            .title("Theme Presets")
            .subtitle("Quick brand palettes")
            .build();
        presets_row.add_prefix(&gtk4::Image::from_icon_name("color-select-symbolic"));
        presets_row.add_suffix(&presets_box);

        let swatch_label_clone = current_swatch.clone();
        accent_button.connect_rgba_notify(move |btn| {
            let hex = rgba_to_hex(&btn.rgba());
            theme::apply(Some(&hex));
            set_swatch(&swatch_label_clone, &hex);
            persist_accent(Some(hex));
        });

        theme_group.add(&accent_row);
        theme_group.add(&presets_row);

        // Background wallpaper picker. The selection is applied through a
        // callback because the window (not this view) owns the picture widget.
        let wallpaper_callback: WallpaperCallback = Rc::new(RefCell::new(None));

        let gallery = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .valign(Align::Center)
            .build();

        // Radio-style buttons: selecting one deselects the others, including
        // the "None" entry, so the active choice is always visible.
        let mut group_leader: Option<gtk4::ToggleButton> = None;

        for entry in wallpaper::WALLPAPERS {
            let thumb = gtk4::Picture::builder()
                .content_fit(gtk4::ContentFit::Cover)
                .can_shrink(true)
                .width_request(52)
                .height_request(34)
                .build();
            if let Some(texture) = wallpaper::load_texture(entry.id) {
                thumb.set_paintable(Some(&texture));
            }

            let button = gtk4::ToggleButton::builder()
                .child(&thumb)
                .tooltip_text(entry.name)
                .valign(Align::Center)
                .build();
            button.add_css_class("wallpaper-thumb");
            button.update_property(&[gtk4::accessible::Property::Label(&format!(
                "{} wallpaper",
                entry.name
            ))]);

            match &group_leader {
                Some(leader) => button.set_group(Some(leader)),
                None => group_leader = Some(button.clone()),
            }

            if Some(entry.id) == saved.wallpaper_id.as_deref() {
                button.set_active(true);
            }

            let callback_clone = wallpaper_callback.clone();
            let id_owned = entry.id.to_string();
            button.connect_toggled(move |btn| {
                if !btn.is_active() {
                    return;
                }
                persist_wallpaper(Some(id_owned.clone()));
                if let Some(cb) = callback_clone.borrow().as_ref() {
                    cb(Some(&id_owned));
                }
            });

            gallery.append(&button);
        }

        // "None" entry: keeps the plain background.
        let none_thumb = gtk4::Image::from_icon_name("action-unavailable-symbolic");
        none_thumb.set_pixel_size(20);
        let none_button = gtk4::ToggleButton::builder()
            .child(&none_thumb)
            .tooltip_text("No background")
            .valign(Align::Center)
            .build();
        none_button.add_css_class("wallpaper-thumb");
        none_button.update_property(&[gtk4::accessible::Property::Label("No wallpaper")]);
        if let Some(leader) = &group_leader {
            none_button.set_group(Some(leader));
        }
        if saved.wallpaper_id.is_none() {
            none_button.set_active(true);
        }

        let callback_clone = wallpaper_callback.clone();
        none_button.connect_toggled(move |btn| {
            if !btn.is_active() {
                return;
            }
            persist_wallpaper(None);
            if let Some(cb) = callback_clone.borrow().as_ref() {
                cb(None);
            }
        });

        gallery.append(&none_button);

        let wallpaper_row = ActionRow::builder()
            .title("Background")
            .subtitle("Wallpaper behind the interface")
            .build();
        wallpaper_row.add_prefix(&gtk4::Image::from_icon_name("image-x-generic-symbolic"));
        wallpaper_row.add_suffix(&gallery);
        theme_group.add(&wallpaper_row);

        // 5. Software Updates
        let update_group = PreferencesGroup::builder()
            .title("Software Updates")
            .description("Check for new releases and security patches from GitHub")
            .build();

        let check_update_btn = Button::builder()
            .label("Check Now")
            .icon_name("software-update-available-symbolic")
            .valign(Align::Center)
            .build();
        check_update_btn.add_css_class("suggested-action");

        let update_status_badge = Label::builder().label("Up to date").build();
        update_status_badge.add_css_class("badge-latency");
        update_status_badge.add_css_class("latency-good");

        let update_row = ActionRow::builder()
            .title("Current Version")
            .subtitle(format!(
                "xdelta v{} (Installed)",
                crate::updater::CURRENT_VERSION
            ))
            .build();
        update_row.add_prefix(&gtk4::Image::from_icon_name(
            "software-update-available-symbolic",
        ));
        update_row.add_suffix(&update_status_badge);
        update_row.add_suffix(&check_update_btn);

        let badge_clone = update_status_badge.clone();
        let btn_clone = check_update_btn.clone();
        check_update_btn.connect_clicked(move |_| {
            let badge = badge_clone.clone();
            let btn = btn_clone.clone();
            btn.set_sensitive(false);
            btn.set_label("Checking...");
            badge.set_label("Checking...");

            gtk4::glib::MainContext::default().spawn_local(async move {
                match crate::updater::check_for_updates().await {
                    Ok(info) => {
                        btn.set_sensitive(true);
                        btn.set_label("Check Now");
                        if info.has_update {
                            badge.set_label(&format!("New: v{}", info.version));
                            badge.remove_css_class("latency-good");
                            badge.add_css_class("latency-poor");
                        } else {
                            badge.set_label("Up to date");
                            badge.remove_css_class("latency-poor");
                            badge.add_css_class("latency-good");
                        }
                    }
                    Err(_) => {
                        btn.set_sensitive(true);
                        btn.set_label("Retry");
                        badge.set_label("Check failed");
                    }
                }
            });
        });

        let releases_link_btn = gtk4::LinkButton::builder()
            .label("GitHub Releases")
            .uri("https://github.com/TssHack/xdelta/releases")
            .valign(Align::Center)
            .build();

        let releases_row = ActionRow::builder()
            .title("Release Channel")
            .subtitle("Official repository releases")
            .activatable_widget(&releases_link_btn)
            .build();
        releases_row.add_prefix(&gtk4::Image::from_icon_name("send-to-symbolic"));
        releases_row.add_suffix(&releases_link_btn);

        update_group.add(&update_row);
        update_group.add(&releases_row);

        // 6. About
        let about_group = PreferencesGroup::builder().title("About xdelta").build();
        let about_row = ActionRow::builder()
            .title("xdelta Client")
            .subtitle(format!(
                "Version {} • GPL-3.0",
                crate::updater::CURRENT_VERSION
            ))
            .activatable(true)
            .build();
        about_row.add_prefix(&gtk4::Image::from_icon_name("xdelta-app"));
        about_row.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));
        about_row.connect_activated(|_| {
            let about = AboutDialog::builder()
                .application_name("xdelta")
                .application_icon("xdelta-app")
                .developer_name("TssHack & xdelta Contributors")
                .version(crate::updater::CURRENT_VERSION)
                .comments("Native, high-performance Linux proxy client for Xray and Sing-box")
                .website("https://github.com/TssHack/xdelta")
                .issue_url("https://github.com/TssHack/xdelta/issues")
                .license_type(gtk4::License::Gpl30)
                .copyright("© 2026 xdelta contributors")
                .build();
            about.present(None::<&gtk4::Window>);
        });
        about_group.add(&about_row);

        content_box.append(&core_group);
        content_box.append(&ports_group);
        content_box.append(&tunnel_group);
        content_box.append(&routing_group);
        content_box.append(&sys_group);
        content_box.append(&theme_group);
        content_box.append(&update_group);
        content_box.append(&about_group);

        let clamp = libadwaita::Clamp::builder()
            .maximum_size(680)
            .tightening_threshold(420)
            .child(&content_box)
            .build();

        let scrolled = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .child(&clamp)
            .vexpand(true)
            .build();

        Self {
            container: scrolled,
            socks_port_entry,
            http_port_entry,
            theme_row,
            auto_proxy_switch,
            connection_mode_row,
            tun_stack_row,
            tun_mtu_entry,
            tun_dns_entry,
            tun_auto_route_switch,
            tun_strict_route_switch,
            tun_setup_button,
            tun_status_label,
            routing_strategy_row,
            wallpaper_callback,
        }
    }

    /// Selected capture mode.
    pub fn connection_mode(&self) -> crate::core::ConnectionMode {
        match self.connection_mode_row.selected() {
            1 => crate::core::ConnectionMode::Tun,
            _ => crate::core::ConnectionMode::SystemProxy,
        }
    }

    /// Applies the listener ports, falling back to the defaults when the field
    /// is empty or unparseable, so a stray keystroke cannot produce a port of 0.
    pub fn apply_ports_to(&self, profile: &mut crate::core::ConnectionProfile) {
        let parse = |text: String, fallback: u16| text.trim().parse::<u16>().unwrap_or(fallback);

        profile.socks_port = parse(
            self.socks_port_entry.text().to_string(),
            crate::core::session::DEFAULT_SOCKS_PORT,
        );
        profile.http_port = parse(
            self.http_port_entry.text().to_string(),
            crate::core::session::DEFAULT_HTTP_PORT,
        );
    }

    /// Applies the TUN parameters from the form.
    pub fn apply_tun_to(&self, profile: &mut crate::core::ConnectionProfile) {
        let mtu = self
            .tun_mtu_entry
            .text()
            .to_string()
            .trim()
            .parse::<u16>()
            .unwrap_or(1500);

        profile.tun = crate::core::TunOptions {
            stack: crate::core::TunStack::from_index(self.tun_stack_row.selected()),
            mtu,
            auto_route: self.tun_auto_route_switch.is_active(),
            strict_route: self.tun_strict_route_switch.is_active(),
            dns_servers: crate::core::TunOptions::parse_dns(self.tun_dns_entry.text().as_ref()),
        };

        profile.routing.strategy =
            crate::core::RoutingStrategy::from_index(self.routing_strategy_row.selected());
    }

    /// Updates the TUN status line and the setup button's enabled state.
    pub fn render_tun_status(&self, readiness: &crate::net::TunReadiness) {
        self.tun_status_label.set_label(&readiness.message());
        self.tun_status_label.remove_css_class("latency-good");
        self.tun_status_label.remove_css_class("latency-poor");
        self.tun_status_label
            .add_css_class(if readiness.is_ready() {
                "latency-good"
            } else {
                "latency-poor"
            });

        self.tun_setup_button
            .set_visible(readiness.is_fixable_by_setup());
        self.tun_setup_button
            .set_sensitive(readiness.is_fixable_by_setup());
    }

    /// Registers the handler that applies a wallpaper selection.
    ///
    /// The settings view does not own the window's picture widget, so the
    /// window subscribes here and stays the single place that mutates the UI.
    pub fn connect_wallpaper_changed<F: Fn(Option<&str>) + 'static>(&self, f: F) {
        *self.wallpaper_callback.borrow_mut() = Some(std::boxed::Box::new(f));
    }
}

impl Default for SettingsView {
    fn default() -> Self {
        Self::new()
    }
}

/// Reads the desktop accent color, falling back to the bundled brand default.
fn take_system_accent_hex() -> String {
    "#3584e4".to_string()
}

/// Formats a GDK color as lowercase `#rrggbb`.
fn rgba_to_hex(rgba: &gtk4::gdk::RGBA) -> String {
    let channel = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(rgba.red()),
        channel(rgba.green()),
        channel(rgba.blue())
    )
}

/// Updates the small textual indicator next to the accent row.
fn set_swatch(label: &Label, hex: &str) {
    label.set_label(hex);
}

/// Writes the accent choice to disk, keeping the stored color scheme intact.
fn persist_accent(accent: Option<String>) {
    let mut settings = persist::load_appearance();
    settings.accent_hex = accent;
    persist::save_appearance(&settings);
}

/// Writes the wallpaper choice to disk, leaving the other preferences alone.
fn persist_wallpaper(wallpaper_id: Option<String>) {
    let mut settings = persist::load_appearance();
    settings.wallpaper_id = wallpaper_id;
    persist::save_appearance(&settings);
}
