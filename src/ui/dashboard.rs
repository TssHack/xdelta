use gtk4::prelude::*;
use gtk4::{Align, Box, Button, Image, Label, Orientation, PolicyType, ScrolledWindow, Switch};
use libadwaita::prelude::*;
use libadwaita::{ActionRow, Clamp, PreferencesGroup};

pub struct DashboardView {
    pub container: ScrolledWindow,
    pub status_badge: Label,
    pub server_name_label: Label,
    pub server_sub_label: Label,
    pub power_button: Button,
    pub power_icon: Image,
    pub download_speed_label: Label,
    pub upload_speed_label: Label,
    pub proxy_switch: Switch,
}

impl DashboardView {
    pub fn new() -> Self {
        let content_box = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(18)
            .margin_top(16)
            .margin_bottom(24)
            .margin_start(16)
            .margin_end(16)
            .build();

        // 1. Futuristic Power Hub Card
        let power_card = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(18)
            .halign(Align::Fill)
            .build();
        power_card.add_css_class("power-hub-card");

        // Top Status Badge
        let status_badge = Label::builder()
            .label("DISCONNECTED")
            .halign(Align::Center)
            .build();
        status_badge.add_css_class("status-badge-live");
        status_badge.add_css_class("status-disconnected");

        // The Circular Power Button
        let power_icon = Image::from_icon_name("xdelta-disconnected-symbolic");
        power_icon.add_css_class("power-icon-large");

        let power_button = Button::builder()
            .child(&power_icon)
            .halign(Align::Center)
            .cursor(&gtk4::gdk::Cursor::from_name("pointer", None).unwrap())
            .build();
        power_button.add_css_class("power-btn");
        power_button.add_css_class("power-btn-disconnected");

        // Active Server Banner (inside Power Hub)
        let server_banner = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(12)
            .halign(Align::Center)
            .build();
        server_banner.add_css_class("active-server-banner");

        let server_icon = Image::from_icon_name("xdelta-symbolic");
        let server_text_box = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(2)
            .build();

        let server_name_label = Label::builder()
            .label("Frankfurt 01 (Fast)")
            .halign(Align::Start)
            .build();
        server_name_label.add_css_class("active-server-title");

        let server_sub_label = Label::builder()
            .label("VLESS Reality • 142 ms")
            .halign(Align::Start)
            .build();
        server_sub_label.add_css_class("caption");

        server_text_box.append(&server_name_label);
        server_text_box.append(&server_sub_label);

        server_banner.append(&server_icon);
        server_banner.append(&server_text_box);

        power_card.append(&status_badge);
        power_card.append(&power_button);
        power_card.append(&server_banner);

        // 2. Telemetry Grid (Download & Upload Speeds)
        let telemetry_box = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(12)
            .homogeneous(true)
            .build();

        // Download Speed Card
        let down_card = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(4)
            .build();
        down_card.add_css_class("telemetry-card");

        let down_header = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(6)
            .build();
        let down_icon = Image::from_icon_name("go-down-symbolic");
        down_icon.add_css_class("speed-down");
        let down_title = Label::builder().label("Download Speed").build();
        down_title.add_css_class("caption");
        down_header.append(&down_icon);
        down_header.append(&down_title);

        let download_speed_label = Label::builder()
            .label("0.00 KB/s")
            .halign(Align::Start)
            .build();
        download_speed_label.add_css_class("telemetry-speed");
        download_speed_label.add_css_class("speed-down");

        let down_sub = Label::builder()
            .label("Session Total: 0.0 MB")
            .halign(Align::Start)
            .build();
        down_sub.add_css_class("telemetry-sub");

        down_card.append(&down_header);
        down_card.append(&download_speed_label);
        down_card.append(&down_sub);

        // Upload Speed Card
        let up_card = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(4)
            .build();
        up_card.add_css_class("telemetry-card");

        let up_header = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(6)
            .build();
        let up_icon = Image::from_icon_name("go-up-symbolic");
        up_icon.add_css_class("speed-up");
        let up_title = Label::builder().label("Upload Speed").build();
        up_title.add_css_class("caption");
        up_header.append(&up_icon);
        up_header.append(&up_title);

        let upload_speed_label = Label::builder()
            .label("0.00 KB/s")
            .halign(Align::Start)
            .build();
        upload_speed_label.add_css_class("telemetry-speed");
        upload_speed_label.add_css_class("speed-up");

        let up_sub = Label::builder()
            .label("Session Total: 0.0 MB")
            .halign(Align::Start)
            .build();
        up_sub.add_css_class("telemetry-sub");

        up_card.append(&up_header);
        up_card.append(&upload_speed_label);
        up_card.append(&up_sub);

        telemetry_box.append(&down_card);
        telemetry_box.append(&up_card);

        // 3. System Integration Group
        let sys_group = PreferencesGroup::builder()
            .title("System Integration")
            .build();

        let proxy_switch = Switch::builder().active(true).valign(Align::Center).build();

        let auto_proxy_row = ActionRow::builder()
            .title("Automatic GNOME System Proxy")
            .subtitle("Configure desktop HTTP and SOCKS5 proxy settings on connect")
            .activatable_widget(&proxy_switch)
            .build();
        auto_proxy_row.add_prefix(&Image::from_icon_name(
            "preferences-system-network-symbolic",
        ));
        auto_proxy_row.add_suffix(&proxy_switch);

        sys_group.add(&auto_proxy_row);

        // Assemble all sections
        content_box.append(&power_card);
        content_box.append(&telemetry_box);
        content_box.append(&sys_group);

        // Fluid responsive Clamp
        let clamp = Clamp::builder()
            .maximum_size(680)
            .tightening_threshold(420)
            .child(&content_box)
            .build();

        let scrolled = ScrolledWindow::builder()
            .hscrollbar_policy(PolicyType::Never)
            .vscrollbar_policy(PolicyType::Automatic)
            .child(&clamp)
            .vexpand(true)
            .build();

        Self {
            container: scrolled,
            status_badge,
            server_name_label,
            server_sub_label,
            power_button,
            power_icon,
            download_speed_label,
            upload_speed_label,
            proxy_switch,
        }
    }
}

impl Default for DashboardView {
    fn default() -> Self {
        Self::new()
    }
}
