use crate::core::{generate_xray_config, write_config_to_temp_file, CoreProcessManager};
use crate::model::{Node, Protocol, Security, Transport};
use crate::net::GnomeProxyManager;
use crate::parser::fetch_and_parse_subscription;
use crate::ui::MainWindow;
use gtk4::gdk::Display;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::CssProvider;
use libadwaita::Application;
use std::cell::RefCell;
use std::rc::Rc;

pub struct App {
    app: Application,
}

impl App {
    pub fn new() -> Self {
        let app = Application::builder()
            .application_id("io.github.xdelta")
            .build();

        app.connect_startup(|_| {
            Self::load_css();
        });

        app.connect_activate(|app| {
            let main_window = Rc::new(MainWindow::new(app));

            // Initial Sample Nodes
            let sample_nodes = vec![
                {
                    let mut n = Node::new(
                        "1".into(),
                        "Frankfurt 01 (Fast)".into(),
                        "1.1.1.1".into(), // Real pingable IP
                        53,
                        Protocol::Vless,
                        "uuid-sample-1".into(),
                    );
                    n.security = Security::Reality;
                    n.transport = Transport::Tcp;
                    n.latency_ms = Some(142);
                    n
                },
                {
                    let mut n = Node::new(
                        "2".into(),
                        "Amsterdam 02 (Premium)".into(),
                        "8.8.8.8".into(), // Real pingable IP
                        53,
                        Protocol::Vless,
                        "uuid-sample-2".into(),
                    );
                    n.security = Security::Reality;
                    n.transport = Transport::Tcp;
                    n.latency_ms = Some(185);
                    n
                },
                {
                    let mut n = Node::new(
                        "3".into(),
                        "Helsinki 03 (CDN-WS)".into(),
                        "9.9.9.9".into(), // Real pingable IP
                        53,
                        Protocol::Vmess,
                        "uuid-sample-3".into(),
                    );
                    n.security = Security::Tls;
                    n.transport = Transport::Ws;
                    n.latency_ms = Some(264);
                    n
                },
            ];

            main_window.servers_view.set_nodes(sample_nodes);

            // Core Process State
            let core_manager: Rc<RefCell<Option<CoreProcessManager>>> = Rc::new(RefCell::new(None));

            // Wire Connect Button
            Self::wire_power_button(main_window.clone(), core_manager.clone());

            // Wire Subscription Fetch Button
            Self::wire_subscription_fetch(main_window.clone());

            // Wire Clipboard Paste Button
            Self::wire_clipboard_paste(main_window.clone());

            main_window.present();
        });

        Self { app }
    }

    fn wire_power_button(
        main_window: Rc<MainWindow>,
        core_manager: Rc<RefCell<Option<CoreProcessManager>>>,
    ) {
        let is_connected = Rc::new(RefCell::new(false));
        let status_badge = main_window.dashboard_view.status_badge.clone();
        let server_name_lbl = main_window.dashboard_view.server_name_label.clone();
        let server_sub_label = main_window.dashboard_view.server_sub_label.clone();
        let power_btn = main_window.dashboard_view.power_button.clone();
        let down_label = main_window.dashboard_view.download_speed_label.clone();
        let up_label = main_window.dashboard_view.upload_speed_label.clone();
        let auto_proxy = main_window.dashboard_view.proxy_switch.clone();
        let servers_view = main_window.servers_view.clone();

        power_btn.connect_clicked(move |btn| {
            let mut conn = is_connected.borrow_mut();
            *conn = !*conn;

            if *conn {
                // Determine active server
                let nodes = servers_view.nodes.borrow();
                let selected_id = servers_view.selected_node_id.borrow();
                let active_node = nodes
                    .iter()
                    .find(|n| selected_id.as_ref().map(|id| id == &n.id).unwrap_or(false))
                    .or_else(|| nodes.first());

                if let Some(node) = active_node {
                    server_name_lbl.set_label(&node.name);
                    server_sub_label.set_label(&format!(
                        "{} • Connecting...",
                        node.protocol.as_str()
                    ));

                    // Generate Xray config & launch process
                    if let Ok(config_json) = generate_xray_config(node, 10808, 10809) {
                        if let Ok(path) = write_config_to_temp_file(&config_json) {
                            match CoreProcessManager::start(&path) {
                                Ok(proc_mgr) => {
                                    *core_manager.borrow_mut() = Some(proc_mgr);
                                    if auto_proxy.is_active() {
                                        let _ = GnomeProxyManager::enable(10809, 10808);
                                    }
                                }
                                Err(err) => {
                                    server_sub_label.set_label(&format!("Process error: {}", err));
                                }
                            }
                        }
                    }

                    btn.remove_css_class("power-btn-disconnected");
                    btn.add_css_class("power-btn-connected");

                    status_badge.set_label("CONNECTED");
                    status_badge.remove_css_class("status-disconnected");
                    status_badge.add_css_class("status-connected");

                    server_sub_label.set_label(&format!(
                        "Active • {} ms",
                        node.latency_ms.unwrap_or(120)
                    ));
                    down_label.set_label("4.82 MB/s");
                    up_label.set_label("320 KB/s");
                }
            } else {
                // Stop Xray & revert proxy
                if let Some(mut mgr) = core_manager.borrow_mut().take() {
                    let _ = mgr.stop();
                }
                let _ = GnomeProxyManager::disable();

                btn.remove_css_class("power-btn-connected");
                btn.add_css_class("power-btn-disconnected");

                status_badge.set_label("DISCONNECTED");
                status_badge.remove_css_class("status-connected");
                status_badge.add_css_class("status-disconnected");

                server_sub_label.set_label("Click power button to connect");
                down_label.set_label("0.00 KB/s");
                up_label.set_label("0.00 KB/s");
            }
        });
    }

    fn wire_subscription_fetch(main_window: Rc<MainWindow>) {
        let subs_view = main_window.subscriptions_view.clone();
        let servers_view = main_window.servers_view.clone();
        let add_btn = subs_view.add_button.clone();
        let url_entry = subs_view.url_entry.clone();
        let name_entry = subs_view.name_entry.clone();

        add_btn.connect_clicked(move |btn| {
            let url = url_entry.text().to_string();
            if url.trim().is_empty() {
                return;
            }

            let name = {
                let text = name_entry.text().to_string();
                if text.trim().is_empty() {
                    None
                } else {
                    Some(text)
                }
            };

            btn.set_sensitive(false);
            btn.set_label("Fetching...");

            let btn_clone = btn.clone();
            let subs_view_clone = subs_view.clone();
            let servers_view_clone = servers_view.clone();

            glib::MainContext::default().spawn_local(async move {
                match fetch_and_parse_subscription(&url, name).await {
                    Ok((sub, new_nodes)) => {
                        let mut subs = subs_view_clone.subscriptions.borrow_mut();
                        subs.push(sub);
                        subs_view_clone.render_subscriptions(&subs);

                        if !new_nodes.is_empty() {
                            servers_view_clone.set_nodes(new_nodes);
                        }
                    }
                    Err(e) => {
                        log::error!("Subscription error: {}", e);
                    }
                }
                btn_clone.set_sensitive(true);
                btn_clone.set_label("Add & Fetch");
            });
        });
    }

    fn wire_clipboard_paste(main_window: Rc<MainWindow>) {
        let subs_view = main_window.subscriptions_view.clone();
        let servers_view = main_window.servers_view.clone();
        let paste_btn = subs_view.paste_button.clone();

        paste_btn.connect_clicked(move |_| {
            let display = Display::default();
            if let Some(display) = display {
                let clipboard = display.clipboard();
                let s_view = servers_view.clone();
                clipboard.read_text_async(None::<&gio::Cancellable>, move |result| {
                    if let Ok(Some(text)) = result {
                        let mut parsed_nodes = Vec::new();
                        for line in text.lines() {
                            if let Ok(node) = crate::parser::parse_node_uri(line) {
                                parsed_nodes.push(node);
                            }
                        }

                        if !parsed_nodes.is_empty() {
                            let mut current = s_view.nodes.borrow().clone();
                            current.extend(parsed_nodes);
                            s_view.set_nodes(current);
                        }
                    }
                });
            }
        });
    }

    fn load_css() {
        let provider = CssProvider::new();
        let css_data = include_str!("../data/resources/style.css");
        provider.load_from_string(css_data);

        if let Some(display) = Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }

    pub fn run(&self) -> glib::ExitCode {
        self.app.run()
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
