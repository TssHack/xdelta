use gtk4::prelude::*;
use gtk4::{Align, Box, Button, Label, Orientation, Switch};
use libadwaita::prelude::*;
use libadwaita::{ActionRow, PreferencesGroup};

pub struct DashboardView {
    pub container: Box,
    pub status_label: Label,
    pub sub_status_label: Label,
    pub connect_button: Button,
    pub proxy_switch: Switch,
}

impl DashboardView {
    pub fn new() -> Self {
        let container = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(18)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .build();

        // 1. Hero Card
        let hero_card = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(16)
            .halign(Align::Fill)
            .build();
        hero_card.add_css_class("hero-card");

        let status_label = Label::builder()
            .label("Disconnected")
            .halign(Align::Center)
            .build();
        status_label.add_css_class("hero-status-title");

        let sub_status_label = Label::builder()
            .label("Select a server to establish connection")
            .halign(Align::Center)
            .build();
        sub_status_label.add_css_class("hero-status-sub");

        let connect_button = Button::builder()
            .label("Connect")
            .halign(Align::Center)
            .cursor(&gtk4::gdk::Cursor::from_name("pointer", None).unwrap())
            .build();
        connect_button.add_css_class("connect-button");
        connect_button.add_css_class("suggested-action");

        hero_card.append(&status_label);
        hero_card.append(&sub_status_label);
        hero_card.append(&connect_button);

        // 2. Local Proxy Endpoints Group
        let endpoints_group = PreferencesGroup::builder()
            .title("Local Inbound Endpoints")
            .description("Local listeners forwarded by xdelta core")
            .build();

        let socks_row = ActionRow::builder()
            .title("SOCKS5 Proxy")
            .subtitle("127.0.0.1:10808")
            .build();
        socks_row.add_prefix(&gtk4::Image::from_icon_name("network-wired-symbolic"));

        let http_row = ActionRow::builder()
            .title("HTTP Proxy")
            .subtitle("127.0.0.1:10809")
            .build();
        http_row.add_prefix(&gtk4::Image::from_icon_name("web-browser-symbolic"));

        endpoints_group.add(&socks_row);
        endpoints_group.add(&http_row);

        // 3. System Integration Group
        let sys_group = PreferencesGroup::builder()
            .title("System Integration")
            .build();

        let proxy_switch = Switch::builder().active(true).valign(Align::Center).build();

        let auto_proxy_row = ActionRow::builder()
            .title("Automatic GNOME System Proxy")
            .subtitle("Set system-wide HTTP and SOCKS5 proxy settings on connect")
            .activatable_widget(&proxy_switch)
            .build();
        auto_proxy_row.add_prefix(&gtk4::Image::from_icon_name(
            "preferences-system-network-symbolic",
        ));
        auto_proxy_row.add_suffix(&proxy_switch);

        sys_group.add(&auto_proxy_row);

        // Assemble into main container
        container.append(&hero_card);
        container.append(&endpoints_group);
        container.append(&sys_group);

        Self {
            container,
            status_label,
            sub_status_label,
            connect_button,
            proxy_switch,
        }
    }
}

impl Default for DashboardView {
    fn default() -> Self {
        Self::new()
    }
}
