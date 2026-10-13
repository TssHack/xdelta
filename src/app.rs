use crate::core::{ConnectionController, ConnectionProfile};
use crate::model::Node;
use crate::net::tun::{self, TunReadiness};
use crate::parser::fetch_and_parse_subscription;
use crate::ui::MainWindow;
use gtk4::gdk::Display;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::CssProvider;
use libadwaita::Application;
use std::cell::RefCell;
use std::rc::Rc;

/// How often the dashboard refreshes live traffic numbers.
const TRAFFIC_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// Human-readable latency for the dashboard banner.
fn describe_latency(node: &Node) -> String {
    match (node.latency_ms, node.verified) {
        (Some(ms), Some(true)) => format!("{ms} ms"),
        (Some(ms), _) => format!("{ms} ms (not verified)"),
        (None, Some(false)) => "unreachable".to_string(),
        (None, _) => "latency unknown".to_string(),
    }
}

/// Set while a probe pass is running.
///
/// Two concurrent passes would contend for the same probe ports and overwrite
/// each other's results, and the subscription loader can legitimately reach the
/// probe path twice (once for the cache, once after the network refresh).
static PROBE_IN_FLIGHT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Result of the background probe started by the fastest-server button.
enum FastestOutcome {
    Ready(Vec<Node>),
    Failed(String),
}

/// Messages sent from the probing worker thread back to the main loop.
///
/// The probe takes tens of seconds and must not run on the UI thread, so its
/// progress and final result travel through this channel.
enum ProbeMessage {
    Started {
        total: usize,
    },
    Progress {
        completed: usize,
        total: usize,
        working: usize,
    },
    Finished {
        candidates: Vec<Node>,
        results: Vec<crate::net::ProbeResult>,
    },
    Failed(String),
}

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
            Self::init_icon_theme();
            Self::apply_saved_appearance();
        });

        app.connect_activate(|app| {
            let main_window = Rc::new(MainWindow::new(app));

            // Core Process State, created first because the subscription loader
            // and the power button both need it.
            let controller = Rc::new(RefCell::new(ConnectionController::new()));

            // The bundled subscription is loaded instead of placeholder nodes.
            //
            // On a cold start this reads a cache if one exists and refreshes in
            // the background, so the window appears immediately and the list
            // fills in. The algorithm that ranks and trims this particular
            // subscription (probe, keep the fastest 30) is applied once its
            // nodes arrive.
            Self::load_default_subscription(main_window.clone(), controller.clone());

            // Apply the saved wallpaper now that the window exists.
            let saved_wallpaper = crate::core::persist::load_appearance().wallpaper_id;
            main_window.set_wallpaper(saved_wallpaper.as_deref());

            // Let the Appearance view drive the background after a selection.
            let window_for_wallpaper = main_window.clone();
            main_window
                .settings_view
                .connect_wallpaper_changed(move |id| {
                    window_for_wallpaper.set_wallpaper(id);
                });

            // TUN permission: report the current status and handle the one-time
            // grant. Nothing privileged happens without this explicit click.
            Self::wire_tun_permission(main_window.clone(), controller.clone());

            // Wire Connect Button
            Self::wire_power_button(main_window.clone(), controller.clone());

            // Dashboard server picker and the one-click fastest-server action.
            Self::wire_dashboard_server_controls(main_window.clone(), controller.clone());

            // Wire Subscription Fetch Button
            Self::wire_subscription_fetch(main_window.clone());

            // Wire Clipboard Paste Button
            Self::wire_clipboard_paste(main_window.clone());

            // Check for updates on startup
            let banner = main_window.update_banner.clone();
            glib::MainContext::default().spawn_local(async move {
                if let Ok(info) = crate::updater::check_for_updates().await {
                    if info.has_update {
                        banner.set_title(&format!(
                            "⚡ New release <b>v{}</b> available on GitHub!",
                            info.version
                        ));
                        banner.set_revealed(true);
                    }
                }
            });

            main_window.present();
        });

        Self { app }
    }

    /// Connects the dashboard's server picker and the "Connect to Fastest"
    /// button to the shared selection state.
    ///
    /// The dashboard picker and the Servers page edit the same selection, so
    /// changing one is reflected in the other rather than letting them drift.
    fn wire_dashboard_server_controls(
        main_window: Rc<MainWindow>,
        controller: Rc<RefCell<ConnectionController>>,
    ) {
        let dashboard = main_window.dashboard_view.clone();
        let servers_view = main_window.servers_view.clone();

        // Keep the picker in sync whenever the server list is replaced.
        {
            let dashboard_for_sync = dashboard.clone();
            let servers_for_sync = servers_view.clone();
            servers_view.connect_nodes_changed(move |nodes| {
                let previous = servers_for_sync.selected_node_id.borrow().clone();
                if let Some(id) = dashboard_for_sync.set_server_choices(nodes, previous.as_deref())
                {
                    *servers_for_sync.selected_node_id.borrow_mut() = Some(id);
                }
            });
        }

        // Choosing in the dashboard updates the shared selection and the banner.
        {
            let servers_for_pick = servers_view.clone();
            let dashboard_for_pick = dashboard.clone();
            dashboard
                .server_selector
                .connect_selected_notify(move |dropdown| {
                    let index = dropdown.selected() as usize;
                    let node = {
                        let nodes = servers_for_pick.nodes.borrow();
                        match nodes.get(index) {
                            Some(node) => node.clone(),
                            None => return,
                        }
                    };

                    *servers_for_pick.selected_node_id.borrow_mut() = Some(node.id.clone());
                    dashboard_for_pick.set_active_server(
                        &node.name,
                        &format!("{} • {}", node.protocol.as_str(), describe_latency(&node)),
                    );
                });
        }

        // Connect to the fastest working server: probe now, pick the lowest
        // real latency, then connect. The probe is the same pass used for the
        // bundled subscription, so results stay consistent with the list.
        {
            let dashboard_click = dashboard.clone();
            let servers_click = servers_view.clone();
            let window_click = main_window.clone();
            let controller_click = controller.clone();

            dashboard.fastest_button.connect_clicked(move |button| {
                let dashboard = dashboard_click.clone();
                let servers_view = servers_click.clone();
                let main_window = window_click.clone();
                let controller = controller_click.clone();

                button.set_sensitive(false);
                button.set_label("Testing servers...");
                dashboard.set_subscription_status("Finding the fastest working server...");

                // Only plain data crosses the thread boundary: GTK widgets and
                // Rc handles are not Send, so the probe cannot capture them.
                let candidates = {
                    let nodes = servers_view.nodes.borrow();
                    crate::net::probe::deduplicate(&crate::net::probe::usable_nodes(&nodes))
                };

                let (tx, rx) = async_channel::bounded::<FastestOutcome>(1);

                std::thread::spawn(move || {
                    use crate::net::probe;

                    let outcome = match probe::probe_nodes(&candidates, |_| {}) {
                        Ok(results) => {
                            let mut ranked = candidates.clone();
                            probe::apply_results(&mut ranked, &results);
                            probe::rank_nodes(&mut ranked);
                            let list = probe::top_servers(&ranked, probe::TOP_SERVERS_SHOWN);
                            FastestOutcome::Ready(list)
                        }
                        Err(e) => {
                            log::warn!("fastest-server probe failed: {}", e);
                            FastestOutcome::Failed(e)
                        }
                    };
                    let _ = tx.send_blocking(outcome);
                });

                let dashboard = dashboard.clone();
                let servers_view = servers_view.clone();
                let main_window = main_window.clone();
                let controller = controller.clone();
                let button = button.clone();

                glib::MainContext::default().spawn_local(async move {
                    let outcome = rx.recv().await;

                    button.set_sensitive(true);
                    button.set_label("Connect to Fastest");

                    let list = match outcome {
                        Ok(FastestOutcome::Ready(list)) if !list.is_empty() => list,
                        Ok(FastestOutcome::Ready(_)) => {
                            dashboard
                                .set_subscription_status("No working server was found in the list");
                            return;
                        }
                        Ok(FastestOutcome::Failed(e)) => {
                            dashboard.set_subscription_status(&format!("Server test failed: {e}"));
                            return;
                        }
                        Err(_) => return,
                    };

                    // The ranked list is already fastest-first, so entry zero is
                    // the lowest-latency server confirmed to carry traffic.
                    let best = list[0].clone();
                    servers_view.set_nodes(list);
                    dashboard.set_server_choices(&servers_view.nodes.borrow(), Some(&best.id));
                    *servers_view.selected_node_id.borrow_mut() = Some(best.id.clone());

                    dashboard.set_active_server(
                        &best.name,
                        &format!("{} • {}", best.protocol.as_str(), describe_latency(&best)),
                    );

                    // Connect through the same code path the power button uses,
                    // so settings, routing and TUN options all apply.
                    let profile = {
                        let settings = main_window.settings_view.clone();
                        let routing = main_window.routing_view.clone();
                        let mut profile =
                            ConnectionProfile::new(best.clone(), settings.connection_mode());
                        settings.apply_ports_to(&mut profile);
                        routing.apply_to(&mut profile);
                        settings.apply_tun_to(&mut profile);
                        profile
                    };

                    let auto_proxy = main_window.dashboard_view.proxy_switch.is_active();
                    let outcome = controller.borrow_mut().connect(&profile, auto_proxy);

                    dashboard.render_outcome(&outcome);
                    dashboard.set_active_mode(controller.borrow().active_mode());
                    dashboard.set_subscription_status(&format!(
                        "Connected to {} ({})",
                        best.name,
                        describe_latency(&best)
                    ));
                });
            });
        }
    }

    fn wire_power_button(
        main_window: Rc<MainWindow>,
        controller: Rc<RefCell<ConnectionController>>,
    ) {
        let status_badge = main_window.dashboard_view.status_badge.clone();
        let server_name_lbl = main_window.dashboard_view.server_name_label.clone();
        let server_sub_label = main_window.dashboard_view.server_sub_label.clone();
        let auto_proxy = main_window.dashboard_view.proxy_switch.clone();
        let servers_view = main_window.servers_view.clone();
        let settings_view = main_window.settings_view.clone();
        let routing_view = main_window.routing_view.clone();

        // The power button, icon and labels are all updated through the view's
        // own render methods, so the closure only needs the view handle.
        {
            let view = main_window.dashboard_view.clone();
            let controller = controller.clone();
            let servers_view = servers_view.clone();
            let settings_view = settings_view.clone();
            let routing_view = routing_view.clone();
            let power_btn = main_window.dashboard_view.power_button.clone();

            power_btn.connect_clicked(move |_| {
                let mut ctl = controller.borrow_mut();
                let was_active = ctl.state().is_active()
                    || matches!(ctl.state(), crate::core::ConnectionState::Connecting);

                let outcome = if was_active {
                    ctl.disconnect()
                } else {
                    // Build the profile from what the user actually configured.
                    let node = {
                        let nodes = servers_view.nodes.borrow();
                        let selected_id = servers_view.selected_node_id.borrow();
                        nodes
                            .iter()
                            .find(|n| selected_id.as_ref().map(|id| id == &n.id).unwrap_or(false))
                            .or_else(|| nodes.first())
                            .cloned()
                    };

                    let Some(node) = node else {
                        // Distinguish the two very different causes: an empty
                        // list (the server test found nothing, or it has not run
                        // yet) versus no selection in a non-empty list. Telling
                        // a user who has a subscription to "add a subscription"
                        // is actively misleading.
                        let has_nodes = !servers_view.nodes.borrow().is_empty();
                        view.render_error(if has_nodes {
                            "Select a server in the list first."
                        } else {
                            "No servers available yet. The list is still loading, \
                             or the server test found none that work — use \
                             \"Connect to Fastest\" to test again."
                        });
                        return;
                    };

                    let mut profile = ConnectionProfile::new(node, settings_view.connection_mode());
                    settings_view.apply_ports_to(&mut profile);
                    routing_view.apply_to(&mut profile);
                    settings_view.apply_tun_to(&mut profile);

                    ctl.connect(&profile, auto_proxy.is_active())
                };

                view.render_outcome(&outcome);
                view.set_active_mode(ctl.active_mode());

                if !outcome.state.is_active() {
                    view.clear_traffic();
                }
            });
        }

        let _ = (&status_badge, &server_name_lbl, &server_sub_label);

        // Periodic tick: refresh live traffic and notice a core that died.
        let controller = controller.clone();
        let view = main_window.dashboard_view.clone();

        glib::timeout_add_local(TRAFFIC_POLL_INTERVAL, move || {
            let mut ctl = controller.borrow_mut();

            // Live traffic is polled only while a connection is up; the core is
            // not running otherwise, so there is nothing to sample.
            if matches!(ctl.state(), crate::core::ConnectionState::Connected) {
                if !ctl.is_core_alive() {
                    let outcome = ctl.note_core_died();
                    view.render_outcome(&outcome);
                    view.set_active_mode(None);
                    view.clear_traffic();
                    return glib::ControlFlow::Continue;
                }

                if let Some((rates, totals)) = ctl.poll_traffic() {
                    view.render_traffic(rates, totals);
                }
            }

            glib::ControlFlow::Continue
        });
    }

    /// Reports TUN readiness and runs the one-time capability setup on demand.
    fn wire_tun_permission(
        main_window: Rc<MainWindow>,
        controller: Rc<RefCell<ConnectionController>>,
    ) {
        let settings_view = main_window.settings_view.clone();
        let button = settings_view.tun_setup_button.clone();

        let refresh = {
            let settings_view = settings_view.clone();
            let controller = controller.clone();
            move || {
                let readiness = controller.borrow().tun_readiness();
                settings_view.render_tun_status(&readiness);
            }
        };

        refresh();

        let settings_view_click = settings_view.clone();
        let controller_click = controller.clone();
        button.connect_clicked(move |btn| {
            let binary = match crate::core::CoreProcessManager::find_xray_binary() {
                Ok(binary) => binary,
                Err(e) => {
                    settings_view_click.render_tun_status(&TunReadiness::Unsupported(e));
                    return;
                }
            };

            btn.set_sensitive(false);
            btn.set_label("Waiting for authentication...");

            match tun::run_setup(&binary) {
                Ok(()) => {
                    log::info!("granted TUN capabilities to {}", binary.display());
                }
                Err(e) => {
                    log::warn!("TUN capability setup failed: {}", e);
                }
            }

            btn.set_label("Grant Permission");

            // Re-read the real state rather than assuming the grant worked.
            let readiness = controller_click.borrow().tun_readiness();
            settings_view_click.render_tun_status(&readiness);

            if !readiness.is_ready() && readiness.is_fixable_by_setup() {
                settings_view_click.render_tun_status(&TunReadiness::NeedsPrivileges { binary });
            }
        });
    }

    /// Loads the bundled default subscription, then probes and ranks it.
    ///
    /// Order of operations matters for perceived speed: cached nodes are shown
    /// at once, the network refresh happens in the background, and the probing
    /// pass runs only after a node list exists. Nothing here blocks the main
    /// loop, because the probe takes tens of seconds and the window must stay
    /// responsive throughout.
    fn load_default_subscription(
        main_window: Rc<MainWindow>,
        controller: Rc<RefCell<ConnectionController>>,
    ) {
        use crate::core::subscription_cache as cache;

        let url = cache::DEFAULT_SUBSCRIPTION_URL.to_string();
        let servers_view = main_window.servers_view.clone();
        let dashboard = main_window.dashboard_view.clone();

        // Show whatever was cached right away so the list is never empty while
        // a refresh is in flight.
        let mut shown_from_cache = false;
        if let Some(cached) = cache::load_cache(&url) {
            if cached.is_usable() {
                servers_view.set_nodes(cached.nodes.clone());
                shown_from_cache = true;
                if cached.is_stale(chrono::Utc::now()) {
                    log::info!("cached subscription is stale; refreshing in background");
                }
            }
        }

        if shown_from_cache {
            // Cached latency values are from a previous session and may no
            // longer hold, so a fresh probe is started on the cached list.
            Self::probe_and_rank(main_window.clone(), controller.clone());
        }

        let view_for_fetch = servers_view.clone();
        let view_for_probe = main_window.clone();
        let controller_for_probe = controller.clone();

        glib::MainContext::default().spawn_local(async move {
            dashboard.set_subscription_status("Refreshing server list...");

            match crate::parser::fetch_and_parse_subscription(
                &url,
                Some(cache::DEFAULT_SUBSCRIPTION_NAME.to_string()),
            )
            .await
            {
                Ok((_subscription, nodes)) => {
                    if nodes.is_empty() {
                        log::warn!("default subscription returned no usable nodes");
                        dashboard
                            .set_subscription_status("Server list is empty; check the network");
                        return;
                    }

                    log::info!("default subscription parsed {} nodes", nodes.len());
                    cache::save_cache(&url, &nodes);
                    view_for_fetch.set_nodes(nodes);

                    // Probing is the expensive step, so it runs after the list
                    // is already on screen.
                    Self::probe_and_rank(view_for_probe, controller_for_probe);
                }
                Err(e) => {
                    log::warn!("could not refresh default subscription: {}", e);
                    if !shown_from_cache {
                        dashboard
                            .set_subscription_status(&format!("Could not load server list: {e}"));
                    }
                }
            }
        });
    }

    /// Runs the probe pass over the default subscription and keeps the fastest
    /// servers.
    ///
    /// The probe is CPU- and network-heavy and blocking, so it runs on a worker
    /// thread. Progress and the final list are pushed back to the main loop
    /// through a channel, keeping the UI responsive while hundreds of candidates
    /// are tested.
    fn probe_and_rank(main_window: Rc<MainWindow>, _controller: Rc<RefCell<ConnectionController>>) {
        use crate::net::probe;
        use std::sync::atomic::Ordering;

        // Only one probe pass may run at a time. The subscription loader can
        // legitimately reach this path twice (once for the cache, once after the
        // network refresh), and two concurrent passes would contend for the same
        // probe ports and overwrite each other's results.
        if PROBE_IN_FLIGHT.swap(true, Ordering::SeqCst) {
            log::debug!("a probe pass is already running; skipping this request");
            return;
        }

        let servers_view = main_window.servers_view.clone();
        let dashboard = main_window.dashboard_view.clone();

        // Only the nodes currently displayed are considered, and duplicates are
        // dropped because the bundled list repeats many entries.
        let candidates = probe::deduplicate(&probe::usable_nodes(&servers_view.nodes.borrow()));
        if candidates.is_empty() {
            PROBE_IN_FLIGHT.store(false, Ordering::SeqCst);
            return;
        }

        let (tx, rx) = async_channel::bounded::<ProbeMessage>(8);

        std::thread::spawn(move || {
            let total = candidates.len();
            let _ = tx.send_blocking(ProbeMessage::Started { total });

            let result = probe::probe_nodes(&candidates, |progress| {
                // A full channel means the UI is behind; dropping an
                // intermediate progress update is harmless, so this must never
                // block the probe thread.
                let _ = tx.try_send(ProbeMessage::Progress {
                    completed: progress.completed,
                    total: progress.total,
                    working: progress.working,
                });
            });

            match result {
                Ok(results) => {
                    let _ = tx.send_blocking(ProbeMessage::Finished {
                        candidates,
                        results,
                    });
                }
                Err(e) => {
                    let _ = tx.send_blocking(ProbeMessage::Failed(e));
                }
            }
        });

        glib::MainContext::default().spawn_local(async move {
            while let Ok(message) = rx.recv().await {
                match message {
                    ProbeMessage::Started { total } => {
                        dashboard.set_subscription_status(&format!(
                            "Testing {total} servers for real connectivity..."
                        ));
                    }
                    ProbeMessage::Progress {
                        completed,
                        total,
                        working,
                    } => {
                        dashboard.set_subscription_status(&format!(
                            "Testing servers: {completed}/{total} ({working} working)"
                        ));
                    }
                    ProbeMessage::Finished {
                        candidates,
                        results,
                    } => {
                        let mut nodes = candidates;
                        probe::apply_results(&mut nodes, &results);
                        probe::rank_nodes(&mut nodes);

                        let working = nodes.iter().filter(|n| n.verified == Some(true)).count();
                        let shown = probe::top_servers(&nodes, probe::TOP_SERVERS_SHOWN);

                        servers_view.set_nodes(shown.clone());

                        dashboard.set_subscription_status(&format!(
                            "{working} of {} servers work; showing the fastest {}",
                            nodes.len(),
                            shown.len()
                        ));

                        log::info!(
                            "probe finished: {} working of {} tested, showing {}",
                            working,
                            nodes.len(),
                            shown.len()
                        );
                    }
                    ProbeMessage::Failed(e) => {
                        log::warn!("probing failed: {}", e);
                        dashboard.set_subscription_status(&format!("Server test failed: {e}"));
                    }
                }
            }

            // The channel closes when the worker finishes, which is exactly the
            // point at which another probe pass may start.
            PROBE_IN_FLIGHT.store(false, std::sync::atomic::Ordering::SeqCst);
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

    /// Restores the appearance the user last chose, before the window is shown.
    fn apply_saved_appearance() {
        let settings = crate::core::persist::load_appearance();

        let style_manager = libadwaita::StyleManager::default();
        style_manager.set_color_scheme(match settings.scheme_index() {
            1 => libadwaita::ColorScheme::ForceDark,
            2 => libadwaita::ColorScheme::ForceLight,
            _ => libadwaita::ColorScheme::Default,
        });

        crate::ui::theme::apply(settings.accent_hex.as_deref());

        // The wallpaper itself is applied once the window exists, in
        // `connect_activate`, because it needs the picture widget.
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

    fn init_icon_theme() {
        if let Some(display) = Display::default() {
            let icon_theme = gtk4::IconTheme::for_display(&display);
            icon_theme.add_search_path("assets/icons/hicolor");
            icon_theme.add_search_path("assets/icons/hicolor/scalable/apps");
            icon_theme.add_search_path("assets/icons/hicolor/symbolic/apps");
            icon_theme.add_search_path("assets/icons");

            if let Ok(exe_path) = std::env::current_exe() {
                if let Some(parent) = exe_path.parent() {
                    icon_theme.add_search_path(parent.join("assets/icons/hicolor"));
                    icon_theme.add_search_path(parent.join("../../assets/icons/hicolor"));
                    icon_theme.add_search_path(parent.join("assets/icons"));
                    icon_theme.add_search_path(parent.join("../../assets/icons"));
                }
            }
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
