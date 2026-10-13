use crate::model::{Node, Protocol, Security, Transport};
use crate::ui::MainWindow;
use gtk4::gdk::Display;
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
            let main_window = MainWindow::new(app);

            // Populate sample data for instant interactive visual verification
            let sample_nodes = vec![
                {
                    let mut n = Node::new(
                        "1".into(),
                        "Frankfurt 01 (Fast)".into(),
                        "198.51.100.1".into(),
                        443,
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
                        "198.51.100.2".into(),
                        443,
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
                        "198.51.100.3".into(),
                        8080,
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

            // Wire Iconic Circular Power Button logic
            let is_connected = Rc::new(RefCell::new(false));
            let status_badge = main_window.dashboard_view.status_badge.clone();
            let server_sub_label = main_window.dashboard_view.server_sub_label.clone();
            let power_btn = main_window.dashboard_view.power_button.clone();
            let down_label = main_window.dashboard_view.download_speed_label.clone();
            let up_label = main_window.dashboard_view.upload_speed_label.clone();

            let is_conn_clone = is_connected.clone();
            power_btn.connect_clicked(move |btn| {
                let mut conn = is_conn_clone.borrow_mut();
                *conn = !*conn;

                if *conn {
                    btn.remove_css_class("power-btn-disconnected");
                    btn.add_css_class("power-btn-connected");

                    status_badge.set_label("CONNECTED");
                    status_badge.remove_css_class("status-disconnected");
                    status_badge.add_css_class("status-connected");

                    server_sub_label.set_label("Secure tunnel active • 142 ms");
                    down_label.set_label("4.82 MB/s");
                    up_label.set_label("320 KB/s");
                } else {
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

            main_window.present();
        });

        Self { app }
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
