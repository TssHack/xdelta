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

            // Wire Connect Button toggle logic
            let is_connected = Rc::new(RefCell::new(false));
            let status_label = main_window.dashboard_view.status_label.clone();
            let sub_status_label = main_window.dashboard_view.sub_status_label.clone();
            let conn_btn = main_window.dashboard_view.connect_button.clone();

            let is_conn_clone = is_connected.clone();
            conn_btn.connect_clicked(move |btn| {
                let mut conn = is_conn_clone.borrow_mut();
                *conn = !*conn;

                if *conn {
                    btn.set_label("Disconnect");
                    btn.remove_css_class("suggested-action");
                    btn.add_css_class("destructive-action");
                    status_label.set_label("Connected");
                    sub_status_label.set_label("Active Server: Frankfurt 01 (Fast) • 142 ms");
                } else {
                    btn.set_label("Connect");
                    btn.remove_css_class("destructive-action");
                    btn.add_css_class("suggested-action");
                    status_label.set_label("Disconnected");
                    sub_status_label.set_label("Select a server to establish connection");
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
