use crate::model::Node;
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
    /// Session totals, updated alongside the live rates.
    download_total_label: Label,
    upload_total_label: Label,
    /// Badge showing which capture mode is active (proxy vs TUN).
    mode_badge: Label,
    /// Status line for subscription loading and server testing.
    subscription_status: Label,
    /// Lets the user pick a different server without leaving the dashboard.
    pub server_selector: gtk4::DropDown,
    /// Probes, picks the lowest-latency working server and connects to it.
    pub fastest_button: Button,
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

        // Top Status Badge, paired with the active capture mode.
        let status_badge = Label::builder()
            .label("DISCONNECTED")
            .halign(Align::Center)
            .build();
        status_badge.add_css_class("status-badge-live");
        status_badge.add_css_class("status-disconnected");

        let mode_badge = Label::builder()
            .label("")
            .halign(Align::Center)
            .visible(false)
            .build();
        mode_badge.add_css_class("badge-protocol");

        // Status line for subscription loading and the server test pass.
        let subscription_status = Label::builder()
            .label("")
            .halign(Align::Center)
            .wrap(true)
            .visible(false)
            .build();
        subscription_status.add_css_class("caption");

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
            .label("No server selected")
            .halign(Align::Start)
            .build();
        server_name_label.add_css_class("active-server-title");

        let server_sub_label = Label::builder()
            .label("Load the server list to begin")
            .halign(Align::Start)
            .build();
        server_sub_label.add_css_class("caption");

        server_text_box.append(&server_name_label);
        server_text_box.append(&server_sub_label);

        // Server picker, so the active server can be changed from the dashboard
        // without navigating to the Servers page.
        let server_selector = gtk4::DropDown::builder()
            .valign(Align::Center)
            .tooltip_text("Select the server to use")
            .build();
        server_selector.add_css_class("server-selector");
        server_selector.update_property(&[gtk4::accessible::Property::Label("Server selection")]);

        server_banner.append(&server_icon);
        server_banner.append(&server_text_box);
        server_banner.append(&server_selector);

        // One action that does the whole job: test, choose the fastest working
        // server and connect. Worth its own button because doing it by hand
        // means waiting for a multi-second probe and then picking from a list.
        let fastest_button = Button::builder()
            .label("Connect to Fastest")
            .tooltip_text("Test the servers and connect to the one with the lowest latency")
            .halign(Align::Center)
            .build();
        fastest_button.add_css_class("suggested-action");
        fastest_button.add_css_class("fastest-button");
        fastest_button.update_property(&[gtk4::accessible::Property::Label(
            "Connect to the fastest server",
        )]);

        power_card.append(&status_badge);
        power_card.append(&mode_badge);
        power_card.append(&subscription_status);
        power_card.append(&power_button);
        power_card.append(&server_banner);
        power_card.append(&fastest_button);

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
            .label("Session Total: 0 B")
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
            .label("Session Total: 0 B")
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
            download_total_label: down_sub,
            upload_total_label: up_sub,
            mode_badge,
            subscription_status,
            server_selector,
            fastest_button,
        }
    }

    /// Renders a connection outcome: status, detail line and mode badge.
    pub fn render_outcome(&self, outcome: &crate::core::TransitionOutcome) {
        let state = &outcome.state;

        self.status_badge.set_label(state.badge_label());
        for class in ["status-connected", "status-error", "status-disconnected"] {
            self.status_badge.remove_css_class(class);
        }
        self.status_badge.add_css_class(state.css_class());

        let detail = match &outcome.note {
            Some(note) => note.clone(),
            None => state.detail(),
        };
        self.server_sub_label.set_label(&detail);

        if state.is_active() {
            self.set_power_state(true);
        } else {
            self.set_power_state(false);
        }
    }

    /// Reports a problem that stopped the action before it could start.
    pub fn render_error(&self, message: &str) {
        self.status_badge.set_label("ERROR");
        self.status_badge.remove_css_class("status-connected");
        self.status_badge.remove_css_class("status-disconnected");
        self.status_badge.add_css_class("status-error");
        self.server_sub_label.set_label(message);
        self.set_power_state(false);
    }

    /// Keeps the power button and icons in sync with the connection state.
    pub fn set_power_state(&self, connected: bool) {
        if connected {
            self.power_button.remove_css_class("power-btn-disconnected");
            self.power_button.add_css_class("power-btn-connected");
            self.power_icon
                .set_icon_name(Some("xdelta-connected-symbolic"));
        } else {
            self.power_button.remove_css_class("power-btn-connected");
            self.power_button.add_css_class("power-btn-disconnected");
            self.power_icon
                .set_icon_name(Some("xdelta-disconnected-symbolic"));
        }
    }

    /// Rebuilds the dashboard server picker from the current node list.
    ///
    /// Returning the previous selection when it still exists avoids silently
    /// switching the user's server every time the list is refreshed.
    pub fn set_server_choices(&self, nodes: &[Node], selected_id: Option<&str>) -> Option<String> {
        let labels: Vec<String> = nodes
            .iter()
            .map(|node| match node.latency_ms {
                Some(ms) if node.verified == Some(true) => {
                    format!("{} — {} ms", node.name, ms)
                }
                Some(ms) => format!("{} — {} ms (untested)", node.name, ms),
                None => node.name.clone(),
            })
            .collect();

        let model = gtk4::StringList::new(&labels.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        self.server_selector.set_model(Some(&model));

        // Restore the previous choice when it is still present.
        let restored = selected_id
            .and_then(|id| nodes.iter().position(|n| n.id == id))
            .map(|i| {
                self.server_selector.set_selected(i as u32);
                nodes[i].id.clone()
            });

        if restored.is_none() && !nodes.is_empty() {
            self.server_selector.set_selected(0);
            return Some(nodes[0].id.clone());
        }

        restored
    }

    /// Returns the node id currently chosen in the dashboard picker.
    pub fn selected_server_index(&self) -> usize {
        self.server_selector.selected() as usize
    }

    /// Shows which server is active and how it performed.
    pub fn set_active_server(&self, name: &str, detail: &str) {
        self.server_name_label.set_label(name);
        self.server_sub_label.set_label(detail);
    }

    /// Updates the subscription/server-test status line.
    pub fn set_subscription_status(&self, message: &str) {
        self.subscription_status.set_label(message);
        self.subscription_status
            .set_visible(!message.trim().is_empty());
    }

    /// Shows which capture mode is active, or hides the badge when idle.
    pub fn set_active_mode(&self, mode: Option<crate::core::ConnectionMode>) {
        match mode {
            Some(mode) => {
                self.mode_badge
                    .set_label(&format!("MODE: {}", mode.label().to_uppercase()));
                self.mode_badge.set_visible(true);
            }
            None => self.mode_badge.set_visible(false),
        }
    }

    /// Updates the live speed readouts and session totals.
    pub fn render_traffic(
        &self,
        rates: crate::net::TrafficRates,
        totals: crate::net::TrafficCounters,
    ) {
        self.download_speed_label
            .set_label(&crate::net::format_rate(rates.downlink_bytes_per_sec));
        self.upload_speed_label
            .set_label(&crate::net::format_rate(rates.uplink_bytes_per_sec));

        self.download_total_label.set_label(&format!(
            "Session Total: {}",
            crate::net::format_total(totals.downlink_bytes)
        ));
        self.upload_total_label.set_label(&format!(
            "Session Total: {}",
            crate::net::format_total(totals.uplink_bytes)
        ));
    }

    /// Clears the traffic readouts, for example after disconnecting.
    pub fn clear_traffic(&self) {
        self.render_traffic(crate::net::TrafficRates::default(), Default::default());
    }
}

impl Default for DashboardView {
    fn default() -> Self {
        Self::new()
    }
}
