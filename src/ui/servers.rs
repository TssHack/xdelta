use crate::model::{Node, Protocol};
use crate::net::tcp_ping;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Align, Box, Button, CheckButton, Label, ListBox, Orientation, ScrolledWindow, SearchEntry,
};
use libadwaita::prelude::*;
use libadwaita::ActionRow;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

pub struct ServersView {
    pub container: Box,
    pub search_entry: SearchEntry,
    pub ping_all_button: Button,
    pub list_box: ListBox,
    pub nodes: Rc<RefCell<Vec<Node>>>,
    pub selected_node_id: Rc<RefCell<Option<String>>>,
    pub current_filter: Rc<RefCell<Option<Protocol>>>,
    /// Listeners notified after the node list is replaced.
    ///
    /// The dashboard keeps its own server picker, and it has to be rebuilt when
    /// the list changes; without this the two views would show different lists.
    nodes_changed: NodeChangeListeners,
}

/// Boxed listeners invoked with the new node list.
///
/// A type alias keeps the field readable: the inline form spells out four
/// nesting levels for what is conceptually "a list of callbacks".
type NodeChangeListeners = Rc<RefCell<Vec<std::boxed::Box<dyn Fn(&[Node])>>>>;

impl ServersView {
    pub fn new() -> Self {
        let container = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(14)
            .margin_top(16)
            .margin_bottom(16)
            .margin_start(20)
            .margin_end(20)
            .build();

        // 1. Top Bar: Search Entry & Test All Button
        let top_box = Box::builder()
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
        ping_all_button.add_css_class("suggested-action");

        top_box.append(&search_entry);
        top_box.append(&ping_all_button);

        // 2. Filter Pills Bar (All, VLESS, VMess, Trojan)
        let filter_box = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .build();

        let all_btn = Button::builder().label("All Protocols").build();
        all_btn.add_css_class("pill");
        all_btn.add_css_class("active");

        let vless_btn = Button::builder().label("VLESS").build();
        vless_btn.add_css_class("pill");

        let vmess_btn = Button::builder().label("VMess").build();
        vmess_btn.add_css_class("pill");

        let trojan_btn = Button::builder().label("Trojan").build();
        trojan_btn.add_css_class("pill");

        filter_box.append(&all_btn);
        filter_box.append(&vless_btn);
        filter_box.append(&vmess_btn);
        filter_box.append(&trojan_btn);

        // 3. Server List
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

        let inner_box = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(14)
            .margin_top(16)
            .margin_bottom(16)
            .margin_start(16)
            .margin_end(16)
            .vexpand(true)
            .build();

        inner_box.append(&top_box);
        inner_box.append(&filter_box);
        inner_box.append(&scrolled);

        let clamp = libadwaita::Clamp::builder()
            .maximum_size(680)
            .tightening_threshold(420)
            .child(&inner_box)
            .vexpand(true)
            .build();

        container.append(&clamp);

        let nodes = Rc::new(RefCell::new(Vec::new()));
        let selected_node_id = Rc::new(RefCell::new(None));
        let current_filter = Rc::new(RefCell::new(None));

        let view = Self {
            container,
            search_entry,
            ping_all_button,
            list_box,
            nodes,
            selected_node_id,
            current_filter,
            nodes_changed: Rc::new(RefCell::new(Vec::new())),
        };

        view.setup_ping_all_handler();
        view
    }

    pub fn set_nodes(&self, nodes: Vec<Node>) {
        // Drop a selection that no longer exists in the new list. Leaving a
        // dangling id behind is what made Connect report "no server selected"
        // while a list was plainly on screen.
        {
            let mut selected = self.selected_node_id.borrow_mut();
            let still_present = selected
                .as_ref()
                .map(|id| nodes.iter().any(|n| &n.id == id))
                .unwrap_or(false);

            if !still_present {
                *selected = nodes.first().map(|n| n.id.clone());
            }
        }

        *self.nodes.borrow_mut() = nodes.clone();
        self.render_nodes(&nodes);

        // Let the dashboard mirror the new list.
        for listener in self.nodes_changed.borrow().iter() {
            listener(&nodes);
        }
    }

    /// Registers a listener that fires whenever the node list is replaced.
    pub fn connect_nodes_changed<F: Fn(&[Node]) + 'static>(&self, f: F) {
        self.nodes_changed
            .borrow_mut()
            .push(std::boxed::Box::new(f));
    }

    pub fn render_nodes(&self, nodes: &[Node]) {
        while let Some(child) = self.list_box.first_child() {
            self.list_box.remove(&child);
        }

        let mut first_radio: Option<CheckButton> = None;

        for node in nodes {
            let row = ActionRow::builder()
                .title(&node.name)
                .subtitle(format!("{}:{}", node.server, node.port))
                .activatable(true)
                .build();
            row.add_css_class("server-row");

            // Protocol badge
            let proto_badge = Label::builder().label(node.protocol.as_str()).build();
            proto_badge.add_css_class("badge-protocol");
            proto_badge.add_css_class(node.protocol.css_class());
            row.add_prefix(&proto_badge);

            // Latency badge
            let latency_badge = Label::builder().build();
            latency_badge.add_css_class("badge-latency");
            Self::update_latency_badge(&latency_badge, node.latency_ms);
            row.add_suffix(&latency_badge);

            // Individual Ping Button (Real-time TCP ping on click)
            let ping_btn = Button::builder()
                .icon_name("view-refresh-symbolic")
                .valign(Align::Center)
                .tooltip_text("Ping this server now")
                .build();
            ping_btn.add_css_class("flat");

            let host = node.server.clone();
            let port = node.port;
            let badge_clone = latency_badge.clone();
            let ping_btn_clone = ping_btn.clone();

            ping_btn.connect_clicked(move |_| {
                let badge = badge_clone.clone();
                let p_btn = ping_btn_clone.clone();
                badge.set_label("testing...");
                p_btn.set_sensitive(false);

                let host = host.clone();
                glib::MainContext::default().spawn_local(async move {
                    let result = tcp_ping(&host, port, Duration::from_secs(3)).await;
                    p_btn.set_sensitive(true);
                    match result {
                        Ok(ms) => Self::update_latency_badge(&badge, Some(ms)),
                        Err(_) => Self::update_latency_badge(&badge, None),
                    }
                });
            });
            row.add_suffix(&ping_btn);

            // Radio button for selection
            let is_selected = self
                .selected_node_id
                .borrow()
                .as_ref()
                .map(|id| id == &node.id)
                .unwrap_or(false);

            let radio = match &first_radio {
                Some(group_lead) => CheckButton::builder()
                    .group(group_lead)
                    .valign(Align::Center)
                    .active(is_selected)
                    .build(),
                None => {
                    let r = CheckButton::builder()
                        .valign(Align::Center)
                        .active(is_selected)
                        .build();
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

    fn update_latency_badge(badge: &Label, latency_ms: Option<u32>) {
        badge.remove_css_class("latency-good");
        badge.remove_css_class("latency-medium");
        badge.remove_css_class("latency-poor");
        badge.remove_css_class("latency-unknown");

        match latency_ms {
            Some(ms) => {
                badge.set_label(&format!("{} ms", ms));
                if ms < 250 {
                    badge.add_css_class("latency-good");
                } else if ms < 500 {
                    badge.add_css_class("latency-medium");
                } else {
                    badge.add_css_class("latency-poor");
                }
            }
            None => {
                badge.set_label("-- ms");
                badge.add_css_class("latency-unknown");
            }
        }
    }

    fn setup_ping_all_handler(&self) {
        let ping_btn = self.ping_all_button.clone();
        let nodes_ref = self.nodes.clone();
        let list_box = self.list_box.clone();

        ping_btn.connect_clicked(move |btn| {
            btn.set_sensitive(false);
            btn.set_label("Pinging All...");

            let nodes = nodes_ref.borrow().clone();
            let btn_clone = btn.clone();
            let nodes_ref_clone = nodes_ref.clone();
            let list_box_clone = list_box.clone();

            glib::MainContext::default().spawn_local(async move {
                let updated = crate::net::batch_ping_nodes(nodes, 10, Duration::from_secs(3)).await;
                *nodes_ref_clone.borrow_mut() = updated.clone();

                btn_clone.set_sensitive(true);
                btn_clone.set_label("Test All");

                // Update latency badges on existing rows
                let mut child = list_box_clone.first_child();
                let mut idx = 0;
                while let Some(widget) = child {
                    if idx < updated.len() {
                        if let Some(row) = widget.downcast_ref::<ActionRow>() {
                            if let Some(ms) = updated[idx].latency_ms {
                                row.set_subtitle(&format!("{} ms latency", ms));
                            }
                        }
                    }
                    child = widget.next_sibling();
                    idx += 1;
                }
            });
        });
    }
}

impl Default for ServersView {
    fn default() -> Self {
        Self::new()
    }
}
