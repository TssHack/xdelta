use crate::model::Subscription;
use gtk4::prelude::*;
use gtk4::{Align, Box, Button, Entry, Label, ListBox, Orientation, ProgressBar, ScrolledWindow};
use libadwaita::prelude::*;
use libadwaita::{ActionRow, ExpanderRow, PreferencesGroup};
use std::cell::RefCell;
use std::rc::Rc;

pub struct SubscriptionsView {
    pub container: ScrolledWindow,
    pub url_entry: Entry,
    pub name_entry: Entry,
    pub add_button: Button,
    pub paste_button: Button,
    pub subs_list: ListBox,
    pub subscriptions: Rc<RefCell<Vec<Subscription>>>,
}

impl SubscriptionsView {
    pub fn new() -> Self {
        let content_box = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(20)
            .margin_top(20)
            .margin_bottom(24)
            .margin_start(28)
            .margin_end(28)
            .build();

        // 1. Add Subscription Expander
        let add_group = PreferencesGroup::builder()
            .title("Add Subscription or Node")
            .description("Import remote profiles or paste raw proxy links")
            .build();

        let expander_row = ExpanderRow::builder()
            .title("New Remote Subscription")
            .subtitle("Supports Base64, V2Ray and Clash URLs")
            .expanded(true)
            .build();
        expander_row.add_prefix(&gtk4::Image::from_icon_name("list-add-symbolic"));

        let form_box = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(12)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(16)
            .margin_end(16)
            .build();

        let name_entry = Entry::builder()
            .placeholder_text("Subscription Name (Optional)")
            .build();

        let url_box = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(10)
            .build();

        let url_entry = Entry::builder()
            .placeholder_text("https://provider.com/api/v1/client/subscribe?token=...")
            .hexpand(true)
            .build();

        let add_button = Button::builder()
            .label("Add & Fetch")
            .icon_name("network-receive-symbolic")
            .valign(Align::Center)
            .build();
        add_button.add_css_class("suggested-action");

        url_box.append(&url_entry);
        url_box.append(&add_button);

        form_box.append(&name_entry);
        form_box.append(&url_box);

        expander_row.add_row(&ActionRow::builder().child(&form_box).build());

        // Quick Import from Clipboard Row
        let paste_button = Button::builder()
            .label("Import Clipboard")
            .icon_name("edit-paste-symbolic")
            .valign(Align::Center)
            .build();
        paste_button.add_css_class("flat");

        let paste_row = ActionRow::builder()
            .title("Quick Import from Clipboard")
            .subtitle("Directly parse vless://, vmess://, or trojan:// links copied to clipboard")
            .activatable_widget(&paste_button)
            .build();
        paste_row.add_prefix(&gtk4::Image::from_icon_name("edit-paste-symbolic"));
        paste_row.add_suffix(&paste_button);

        add_group.add(&expander_row);
        add_group.add(&paste_row);

        // 2. Active Subscriptions List
        let list_group = PreferencesGroup::builder()
            .title("Configured Subscriptions")
            .description("Subscriptions auto-sync based on provider headers")
            .build();

        let subs_list = ListBox::builder()
            .selection_mode(gtk4::SelectionMode::None)
            .build();
        subs_list.add_css_class("boxed-list");

        list_group.add(&subs_list);

        content_box.append(&add_group);
        content_box.append(&list_group);

        let scrolled = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .child(&content_box)
            .vexpand(true)
            .build();

        let view = Self {
            container: scrolled,
            url_entry,
            name_entry,
            add_button,
            paste_button,
            subs_list,
            subscriptions: Rc::new(RefCell::new(Vec::new())),
        };

        // Populate sample subscription
        let sample_sub = Subscription {
            id: "sub-1".into(),
            name: "Frankfurt VIP Highway".into(),
            url: "https://sub.freedom-net.org/vless/sub?token=sec_abc123".into(),
            update_interval_minutes: 1440,
            last_updated: Some(chrono::Utc::now()),
            userinfo: Some(crate::model::SubscriptionUserinfo {
                upload_bytes: 4 * 1024 * 1024 * 1024,
                download_bytes: 38 * 1024 * 1024 * 1024,
                total_bytes: 100 * 1024 * 1024 * 1024,
                expire_time: None,
            }),
            node_count: 3,
        };
        view.render_subscriptions(&[sample_sub]);

        view
    }

    pub fn render_subscriptions(&self, subs: &[Subscription]) {
        while let Some(child) = self.subs_list.first_child() {
            self.subs_list.remove(&child);
        }

        for sub in subs {
            let row = ExpanderRow::builder()
                .title(&sub.name)
                .subtitle(&format!("{} nodes • Active", sub.node_count))
                .build();
            row.add_prefix(&gtk4::Image::from_icon_name("folder-download-symbolic"));

            // Quota details row
            let info_box = Box::builder()
                .orientation(Orientation::Vertical)
                .spacing(8)
                .margin_top(12)
                .margin_bottom(12)
                .margin_start(16)
                .margin_end(16)
                .build();

            let url_lbl = Label::builder()
                .label(&format!("Endpoint: {}", sub.url))
                .halign(Align::Start)
                .ellipsize(gtk4::pango::EllipsizeMode::End)
                .build();
            url_lbl.add_css_class("caption");
            info_box.append(&url_lbl);

            if let Some(userinfo) = &sub.userinfo {
                let used_gb = (userinfo.upload_bytes + userinfo.download_bytes) as f64
                    / (1024.0 * 1024.0 * 1024.0);
                let total_gb = userinfo.total_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
                let fraction = if total_gb > 0.0 {
                    (used_gb / total_gb).clamp(0.0, 1.0)
                } else {
                    0.0
                };

                let quota_label = Label::builder()
                    .label(&format!(
                        "Data Usage: {:.1} GB / {:.1} GB ({:.0}%)",
                        used_gb,
                        total_gb,
                        fraction * 100.0
                    ))
                    .halign(Align::Start)
                    .build();
                quota_label.add_css_class("caption");

                let progress = ProgressBar::builder().fraction(fraction).build();

                info_box.append(&quota_label);
                info_box.append(&progress);
            }

            // Action buttons row (Update & Delete)
            let actions_box = Box::builder()
                .orientation(Orientation::Horizontal)
                .spacing(10)
                .halign(Align::End)
                .build();

            let update_btn = Button::builder()
                .label("Refresh")
                .icon_name("view-refresh-symbolic")
                .build();
            update_btn.add_css_class("flat");

            let delete_btn = Button::builder()
                .label("Remove")
                .icon_name("user-trash-symbolic")
                .build();
            delete_btn.add_css_class("flat");
            delete_btn.add_css_class("destructive-action");

            actions_box.append(&update_btn);
            actions_box.append(&delete_btn);
            info_box.append(&actions_box);

            row.add_row(&ActionRow::builder().child(&info_box).build());
            self.subs_list.append(&row);
        }
    }
}

impl Default for SubscriptionsView {
    fn default() -> Self {
        Self::new()
    }
}
