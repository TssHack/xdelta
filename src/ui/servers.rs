use crate::model::Node;
use gtk4::prelude::*;
use gtk4::{
    Align, Box, Button, CheckButton, Label, ListBox, Orientation, ScrolledWindow, SearchEntry,
};
use libadwaita::prelude::*;
use libadwaita::ActionRow;
use std::cell::RefCell;
use std::rc::Rc;

pub struct ServersView {
    pub container: Box,
    pub search_entry: SearchEntry,
    pub ping_all_button: Button,
    pub list_box: ListBox,
    pub nodes: Rc<RefCell<Vec<Node>>>,
    pub selected_node_id: Rc<RefCell<Option<String>>>,
}

impl ServersView {
    pub fn new() -> Self {
        let container = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(12)
            .margin_top(16)
            .margin_bottom(16)
            .margin_start(16)
            .margin_end(16)
            .build();

        // Header controls (Search & Ping All)
        let header_box = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(12)
            .build();

        let search_entry = SearchEntry::builder()
            .hexpand(true)
            .placeholder_text("Search servers by name or address...")
            .build();

        let ping_all_button = Button::builder()
            .label("Test All")
            .icon_name("network-transmit-receive-symbolic")
            .build();
        ping_all_button.add_css_class("flat");

        header_box.append(&search_entry);
        header_box.append(&ping_all_button);

        // Server list
        let list_box = ListBox::builder()
            .selection_mode(gtk4::SelectionMode::None)
            .build();
        list_box.add_css_class("boxed-list");

        let scrolled = ScrolledWindow::builder()
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .vexpand(true)
            .child(&list_box)
            .build();

        container.append(&header_box);
        container.append(&scrolled);

        Self {
            container,
            search_entry,
            ping_all_button,
            list_box,
            nodes: Rc::new(RefCell::new(Vec::new())),
            selected_node_id: Rc::new(RefCell::new(None)),
        }
    }

    pub fn set_nodes(&self, nodes: Vec<Node>) {
        *self.nodes.borrow_mut() = nodes.clone();
        self.render_nodes(&nodes);
    }

    pub fn render_nodes(&self, nodes: &[Node]) {
        // Clear existing children
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }

        let mut first_radio: Option<CheckButton> = None;

        for node in nodes {
            let row = ActionRow::builder()
                .title(&node.name)
                .subtitle(&format!("{}:{}", node.server, node.port))
                .activatable(true)
                .build();
            row.add_css_class("server-row");

            // Protocol badge
            let proto_badge = Label::builder().label(node.protocol.as_str()).build();
            proto_badge.add_css_class("badge-protocol");
            proto_badge.add_css_class(node.protocol.css_class());
            row.add_prefix(&proto_badge);

            // Latency badge
            let latency_text = match node.latency_ms {
                Some(ms) => format!("{} ms", ms),
                None => "-- ms".to_string(),
            };
            let latency_badge = Label::builder().label(&latency_text).build();
            latency_badge.add_css_class("badge-latency");
            match node.latency_ms {
                Some(ms) if ms < 300 => latency_badge.add_css_class("latency-good"),
                Some(ms) if ms < 600 => latency_badge.add_css_class("latency-medium"),
                Some(_) => latency_badge.add_css_class("latency-poor"),
                None => latency_badge.add_css_class("latency-unknown"),
            }
            row.add_suffix(&latency_badge);

            // Radio button for selection
            let radio = match &first_radio {
                Some(group_lead) => CheckButton::builder()
                    .group(group_lead)
                    .valign(Align::Center)
                    .build(),
                None => {
                    let r = CheckButton::builder().valign(Align::Center).build();
                    first_radio = Some(r.clone());
                    r
                }
            };

            let node_id = node.id.clone();
            let selected_ref = self.selected_node_id.clone();
            radio.connect_toggled(move |btn| {
                if btn.is_active() {
                    *selected_ref.borrow_mut() = Some(node_id.clone());
                }
            });

            row.add_suffix(&radio);
            self.list_box.append(&row);
        }
    }
}

impl Default for ServersView {
    fn default() -> Self {
        Self::new()
    }
}
