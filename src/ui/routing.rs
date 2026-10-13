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

        let routing_mode_row = ComboRow::builder()
            .title("Traffic Strategy")
            .model(&modes)
            .selected(0)
            .build();
        routing_mode_row.add_prefix(&gtk4::Image::from_icon_name("route-symbolic"));

        mode_group.add(&routing_mode_row);

        // 2. GeoIP & GeoSite Rules
        let rules_group = PreferencesGroup::builder()
            .title("Predefined Rule Sets")
            .description("Built-in categorization using GeoIP and GeoSite databases")
            .build();

        let bypass_iran_switch = Switch::builder().active(true).valign(Align::Center).build();
        let bypass_row = ActionRow::builder()
            .title("Bypass Iran & LAN")
            .subtitle("Route domestic domains (.ir) and local CIDRs directly")
            .activatable_widget(&bypass_iran_switch)
            .build();
        bypass_row.add_prefix(&gtk4::Image::from_icon_name("network-vpn-symbolic"));
        bypass_row.add_suffix(&bypass_iran_switch);

        let block_ads_switch = Switch::builder().active(true).valign(Align::Center).build();
        let ads_row = ActionRow::builder()
            .title("Block Advertisements & Trackers")
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

        let direct_domain_entry = Entry::builder()
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

        content_box.append(&mode_group);
        content_box.append(&rules_group);
        content_box.append(&custom_group);

        let scrolled = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .child(&content_box)
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
}

impl Default for RoutingView {
    fn default() -> Self {
        Self::new()
    }
}
