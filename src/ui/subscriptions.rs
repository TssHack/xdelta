use crate::model::Subscription;
use gtk4::prelude::*;
use gtk4::{Align, Box, Button, Entry, ListBox, Orientation, ScrolledWindow};
use libadwaita::prelude::*;
use libadwaita::{ActionRow, PreferencesGroup};
use std::cell::RefCell;
use std::rc::Rc;

pub struct SubscriptionsView {
    pub container: Box,
    pub url_entry: Entry,
    pub add_button: Button,
    pub paste_button: Button,
    pub subs_list: ListBox,
    pub subscriptions: Rc<RefCell<Vec<Subscription>>>,
}

impl SubscriptionsView {
    pub fn new() -> Self {
        let container = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(18)
            .margin_top(20)
            .margin_bottom(20)
            .margin_start(20)
            .margin_end(20)
            .build();

        // 1. Add Subscription Group
        let add_group = PreferencesGroup::builder()
            .title("Add New Subscription")
            .description("Enter a Base64 subscription URL or raw configuration link")
            .build();

        let input_box = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(10)
            .build();

        let url_entry = Entry::builder()
            .placeholder_text("https://example.com/api/v1/client/subscribe?token=...")
            .hexpand(true)
            .build();

        let add_button = Button::builder()
            .label("Add")
            .icon_name("list-add-symbolic")
            .valign(Align::Center)
            .build();
        add_button.add_css_class("suggested-action");

        let paste_button = Button::builder()
            .label("Paste Link")
            .icon_name("edit-paste-symbolic")
            .valign(Align::Center)
            .build();

        input_box.append(&url_entry);
        input_box.append(&paste_button);
        input_box.append(&add_button);

        add_group.add(&input_box);

        // 2. Existing Subscriptions Group
        let list_group = PreferencesGroup::builder()
            .title("Active Subscriptions")
            .build();

        let subs_list = ListBox::builder()
            .selection_mode(gtk4::SelectionMode::None)
            .build();
        subs_list.add_css_class("boxed-list");

        list_group.add(&subs_list);

        let scrolled = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .vexpand(true)
            .child(&list_group)
            .build();

        container.append(&add_group);
        container.append(&scrolled);

        Self {
            container,
            url_entry,
            add_button,
            paste_button,
            subs_list,
            subscriptions: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn render_subscriptions(&self, subs: &[Subscription]) {
        while let Some(child) = self.subs_list.first_child() {
            self.subs_list.remove(&child);
        }

        for sub in subs {
            let row = ActionRow::builder()
                .title(&sub.name)
                .subtitle(&format!("{} nodes • {}", sub.node_count, sub.url))
                .build();
            row.add_prefix(&gtk4::Image::from_icon_name("folder-download-symbolic"));

            let refresh_btn = Button::builder()
                .icon_name("view-refresh-symbolic")
                .valign(Align::Center)
                .build();
            refresh_btn.add_css_class("flat");

            row.add_suffix(&refresh_btn);
            self.subs_list.append(&row);
        }
    }
}

impl Default for SubscriptionsView {
    fn default() -> Self {
        Self::new()
    }
}
