use gtk4::prelude::*;
use gtk4::{Align, Box, Button, Entry, Label, Orientation, ScrolledWindow, Switch};
use libadwaita::prelude::*;
use libadwaita::{ActionRow, ComboRow, PreferencesGroup, StyleManager};

pub struct SettingsView {
    pub container: ScrolledWindow,
    pub socks_port_entry: Entry,
    pub http_port_entry: Entry,
    pub theme_row: ComboRow,
    pub auto_proxy_switch: Switch,
}

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

        let binary_row = ActionRow::builder()
            .title("Xray Binary Location")
            .subtitle("/usr/bin/xray")
            .build();
        let binary_status_badge = Label::builder().label("Installed").build();
        binary_status_badge.add_css_class("badge-latency");
        binary_status_badge.add_css_class("latency-good");
        binary_row.add_suffix(&binary_status_badge);

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

        let theme_model = gtk4::StringList::new(&["System Default", "Dark Mode", "Light Mode"]);
        let theme_row = ComboRow::builder()
            .title("Color Scheme")
            .model(&theme_model)
            .selected(0)
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
        });

        theme_group.add(&theme_row);

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
            .subtitle(&format!("xdelta v{} (Installed)", crate::updater::CURRENT_VERSION))
            .build();
        update_row.add_prefix(&gtk4::Image::from_icon_name("software-update-available-symbolic"));
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
            .subtitle("Version 1.0.0 • GNU General Public License v3.0")
            .build();
        about_row.add_prefix(&gtk4::Image::from_icon_name("help-about-symbolic"));
        about_group.add(&about_row);

        content_box.append(&core_group);
        content_box.append(&ports_group);
        content_box.append(&sys_group);
        content_box.append(&theme_group);
        content_box.append(&update_group);
        content_box.append(&about_group);

        let scrolled = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .child(&content_box)
            .vexpand(true)
            .build();

        Self {
            container: scrolled,
            socks_port_entry,
            http_port_entry,
            theme_row,
            auto_proxy_switch,
        }
    }
}

impl Default for SettingsView {
    fn default() -> Self {
        Self::new()
    }
}
