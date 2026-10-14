use gtk4::prelude::*;
use gtk4::{Align, Box, Entry, Orientation, ScrolledWindow, Switch};
use libadwaita::prelude::*;
use libadwaita::{ActionRow, ComboRow, PreferencesGroup};

pub struct RoutingView {
    pub container: ScrolledWindow,
    pub routing_mode_row: ComboRow,
    pub bypass_iran_switch: Switch,
    pub block_ads_switch: Switch,
    pub direct_domain_entry: Entry,
}

impl RoutingView {
    pub fn new() -> Self {
        let content_box = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(20)
            .margin_top(20)
            .margin_bottom(24)
            .margin_start(28)
            .margin_end(28)
            .build();

        let saved_conn = crate::core::persist::load_connection();

        // 1. Preset Routing Mode
        let mode_group = PreferencesGroup::builder()
            .title("Routing Strategy")
            .description("Control how network traffic is classified and forwarded")
            .build();

        let modes = gtk4::StringList::new(&[
            "Bypass Domestic & Private (Rule Mode)",
            "Global Proxy (Forward All)",
            "Direct (Bypass All)",
        ]);

        let strategy_selected = match saved_conn.routing_strategy.as_str() {
            "global" => 1,
            "direct" => 2,
            _ => 0,
        };

        let routing_mode_row = ComboRow::builder()
            .title("Traffic Strategy")
            .model(&modes)
            .selected(strategy_selected)
            .build();
        routing_mode_row.add_prefix(&gtk4::Image::from_icon_name("xdelta-routing-symbolic"));

        mode_group.add(&routing_mode_row);

        // 2. GeoIP & GeoSite Rules
        let rules_group = PreferencesGroup::builder()
            .title("Predefined Rule Sets")
            .description("Built-in categorization using GeoIP and GeoSite databases")
            .build();

        let bypass_iran_switch = Switch::builder()
            .active(saved_conn.bypass_iran)
            .valign(Align::Center)
            .build();
        let bypass_row = ActionRow::builder()
            .title("Bypass Iran &amp; LAN")
            .subtitle("Route domestic domains (.ir) and local CIDRs directly")
            .activatable_widget(&bypass_iran_switch)
            .build();
        bypass_row.add_prefix(&gtk4::Image::from_icon_name("network-vpn-symbolic"));
        bypass_row.add_suffix(&bypass_iran_switch);

        let block_ads_switch = Switch::builder()
            .active(saved_conn.block_ads)
            .valign(Align::Center)
            .build();
        let ads_row = ActionRow::builder()
            .title("Block Advertisements &amp; Trackers")
            .subtitle("Drop traffic destined for known ad and telemetry domains")
            .activatable_widget(&block_ads_switch)
            .build();
        ads_row.add_prefix(&gtk4::Image::from_icon_name("security-high-symbolic"));
        ads_row.add_suffix(&block_ads_switch);

        rules_group.add(&bypass_row);
        rules_group.add(&ads_row);

        // 3. Custom Domain Rules
        let custom_group = PreferencesGroup::builder()
            .title("Custom Domain Rules")
            .description("Explicitly route specific domains directly or through the proxy")
            .build();

        let direct_domains_str = saved_conn.direct_domains.join(", ");
        let direct_domain_entry = Entry::builder()
            .text(&direct_domains_str)
            .placeholder_text("e.g. domain:local.net, mycompany.internal")
            .hexpand(true)
            .valign(Align::Center)
            .build();

        let custom_direct_row = ActionRow::builder()
            .title("Direct Domains")
            .subtitle("Domains that will always bypass the proxy")
            .activatable_widget(&direct_domain_entry)
            .build();
        custom_direct_row.add_prefix(&gtk4::Image::from_icon_name("insert-link-symbolic"));
        custom_direct_row.add_suffix(&direct_domain_entry);

        custom_group.add(&custom_direct_row);

        // Auto-save changes to persistent settings
        let strategy_c = routing_mode_row.clone();
        let bypass_c = bypass_iran_switch.clone();
        let ads_c = block_ads_switch.clone();
        let domains_c = direct_domain_entry.clone();

        let save_routing = move || {
            let mut s = crate::core::persist::load_connection();
            s.routing_strategy = match strategy_c.selected() {
                1 => "global".into(),
                2 => "direct".into(),
                _ => "rule".into(),
            };
            s.bypass_iran = bypass_c.is_active();
            s.block_ads = ads_c.is_active();
            let text = domains_c.text().to_string();
            s.direct_domains = text
                .split(',')
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty())
                .collect();
            crate::core::persist::save_connection(&s);
        };

        let sr = std::rc::Rc::new(save_routing);
        let sr1 = sr.clone();
        routing_mode_row.connect_selected_notify(move |_| sr1());
        let sr2 = sr.clone();
        bypass_iran_switch.connect_active_notify(move |_| sr2());
        let sr3 = sr.clone();
        block_ads_switch.connect_active_notify(move |_| sr3());
        let sr4 = sr.clone();
        direct_domain_entry.connect_changed(move |_| sr4());

        content_box.append(&mode_group);
        content_box.append(&rules_group);
        content_box.append(&custom_group);

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
            routing_mode_row,
            bypass_iran_switch,
            block_ads_switch,
            direct_domain_entry,
        }
    }

    /// Copies the Routing page's choices onto a connection profile.
    pub fn apply_to(&self, profile: &mut crate::core::ConnectionProfile) {
        profile.routing.strategy =
            crate::core::RoutingStrategy::from_index(self.routing_mode_row.selected());
        profile.routing.bypass_iran = self.bypass_iran_switch.is_active();
        profile.routing.block_ads = self.block_ads_switch.is_active();
        profile.routing.direct_domains = crate::core::RoutingOptions::parse_direct_domains(
            self.direct_domain_entry.text().as_ref(),
        );
    }
}

impl Default for RoutingView {
    fn default() -> Self {
        Self::new()
    }
}
