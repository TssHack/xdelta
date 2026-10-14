use crate::core::session::ConnectionState;
use crate::core::{ConnectionController, ConnectionProfile};
use crate::model::Node;
use crate::net::tun::{self, TunReadiness};
use crate::parser::fetch_and_parse_subscription;
use crate::ui::tray::{TrayEvent, XdeltaTray};
use crate::ui::MainWindow;
use gtk4::gdk::Display;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::CssProvider;
use libadwaita::Application;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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

pub struct App {
    app: Application,
    _hold_guard: Rc<RefCell<Option<gtk4::gio::ApplicationHoldGuard>>>,
}

impl App {
    pub fn new() -> Self {
        let app = Application::builder()
            .application_id("io.github.xdelta")
            .build();

        app.connect_startup(|_| {
            crate::core::CoreProcessManager::terminate_stale_cores();
            Self::load_css();
            Self::init_icon_theme();
            Self::apply_saved_appearance();
        });

        let hold_slot = Rc::new(RefCell::new(None));
        let hold_slot_for_activate = hold_slot.clone();

        app.connect_activate(move |app| {
            // Keep application alive when window is minimized/hidden to system tray
            if hold_slot_for_activate.borrow().is_none() {
                *hold_slot_for_activate.borrow_mut() = Some(app.hold());
            }

            let main_window = Rc::new(MainWindow::new(app));

            // Core Process State, created first because the subscription loader
            // and the power button both need it.
            let controller = Rc::new(RefCell::new(ConnectionController::new()));

            // AppIndicator / System Tray service
            let is_window_visible = Arc::new(AtomicBool::new(true));
            let (tray_sender, tray_receiver) = async_channel::unbounded::<TrayEvent>();
            let tray_handle: Rc<RefCell<Option<crate::ui::tray::TrayHandle>>> =
                Rc::new(RefCell::new(None));
            let last_notified_state = Rc::new(RefCell::new(None));

            {
                let tray_handle_for_init = tray_handle.clone();
                let is_window_visible_for_tray = is_window_visible.clone();
                let (h_tx, h_rx) = async_channel::bounded(1);
                tokio::spawn(async move {
                    let res =
                        XdeltaTray::spawn_service(tray_sender, is_window_visible_for_tray).await;
                    let _ = h_tx.send(res).await;
                });

                glib::MainContext::default().spawn_local(async move {
                    match h_rx.recv().await {
                        Ok(Ok(handle)) => {
                            *tray_handle_for_init.borrow_mut() = Some(handle);
                        }
                        Ok(Err(e)) => {
                            log::warn!("StatusNotifierItem (tray) unavailable on D-Bus: {e}");
                        }
                        Err(_) => {}
                    }
                });
            }

            // Minimize to system tray on window close if enabled
            {
                let is_window_visible_close = is_window_visible.clone();
                let settings_for_close = main_window.settings_view.clone();
                let controller_for_close = controller.clone();
                let tray_handle_for_close = tray_handle.clone();
                main_window.window.connect_close_request(move |win| {
                    if settings_for_close.minimize_to_tray_switch.is_active() {
                        log::info!("Window close requested; minimizing to system tray");
                        is_window_visible_close.store(false, Ordering::Relaxed);
                        win.set_visible(false);
                        if let Some(h) = tray_handle_for_close.borrow().as_ref() {
                            let h = h.clone();
                            glib::MainContext::default().spawn_local(async move {
                                let _ = h.update(|_| {}).await;
                            });
                        }
                        glib::Propagation::Stop
                    } else {
                        log::info!("Window close requested; shutting down");
                        let mut ctl = controller_for_close.borrow_mut();
                        if ctl.state().is_active() {
                            let _ = ctl.disconnect();
                        }
                        glib::Propagation::Proceed
                    }
                });
            }

            // Wire tray menu events (Show/Hide, Connect/Disconnect, Quit)
            {
                let win = main_window.window.clone();
                let is_window_visible_events = is_window_visible.clone();
                let power_btn = main_window.dashboard_view.power_button.clone();
                let controller_for_quit = controller.clone();
                let app_for_quit = app.clone();

                glib::MainContext::default().spawn_local(async move {
                    while let Ok(event) = tray_receiver.recv().await {
                        match event {
                            TrayEvent::ToggleWindow => {
                                let is_vis = is_window_visible_events.load(Ordering::Relaxed);
                                if is_vis && win.is_active() {
                                    win.set_visible(false);
                                    is_window_visible_events.store(false, Ordering::Relaxed);
                                } else {
                                    win.set_visible(true);
                                    win.present();
                                    is_window_visible_events.store(true, Ordering::Relaxed);
                                }
                            }
                            TrayEvent::ToggleConnection => {
                                power_btn.emit_clicked();
                            }
                            TrayEvent::Quit => {
                                log::info!("Quit requested from system tray AppIndicator");
                                let mut ctl = controller_for_quit.borrow_mut();
                                if ctl.state().is_active() {
                                    let _ = ctl.disconnect();
                                }
                                app_for_quit.quit();
                            }
                        }
                    }
                });
            }

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
            Self::wire_power_button(
                main_window.clone(),
                controller.clone(),
                tray_handle.clone(),
                app.clone(),
                last_notified_state.clone(),
            );

            // Dashboard server picker and the one-click fastest-server action.
            Self::wire_dashboard_server_controls(
                main_window.clone(),
                controller.clone(),
                tray_handle.clone(),
                app.clone(),
                last_notified_state.clone(),
            );

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

        Self {
            app,
            _hold_guard: hold_slot,
        }
    }

    fn sync_tray_and_notify(
        controller: &ConnectionController,
        tray_handle: &Option<crate::ui::tray::TrayHandle>,
        app: &Application,
        last_notified: &RefCell<Option<ConnectionState>>,
    ) {
        let current_state = controller.state().clone();
        let (server_name, latency) = match &current_state {
            ConnectionState::Connected => {
                let node = controller.active_node();
                (
                    node.map(|n| n.name.clone()),
                    node.and_then(|n| n.latency_ms),
                )
            }
            _ => (None, None),
        };

        if let Some(handle) = tray_handle {
            let handle = handle.clone();
            let s_name = server_name.clone();
            let st = current_state.clone();
            glib::MainContext::default().spawn_local(async move {
                let _ = handle
                    .update(move |tray| {
                        tray.update_connection(st, s_name, latency);
                    })
                    .await;
            });
        }

        let mut last = last_notified.borrow_mut();
        let prev_active = last.as_ref().map(|s| s.is_active()).unwrap_or(false);
        let curr_active = current_state.is_active();

        if last.as_ref() != Some(&current_state) {
            *last = Some(current_state.clone());

            if curr_active && !prev_active {
                let notif = gtk4::gio::Notification::new("xdelta");
                notif.set_title("xdelta: Connected");
                let s_text = server_name.as_deref().unwrap_or("Proxy Server");
                notif.set_body(Some(&format!("Secure tunnel established via {s_text}")));
                notif.set_icon(&gtk4::gio::ThemedIcon::new("xdelta-connected-symbolic"));
                app.send_notification(Some("xdelta-connection-state"), &notif);
            } else if !curr_active && prev_active {
                let notif = gtk4::gio::Notification::new("xdelta");
                notif.set_title("xdelta: Disconnected");
                notif.set_body(Some("Proxy connection has been stopped"));
                notif.set_icon(&gtk4::gio::ThemedIcon::new("xdelta-disconnected-symbolic"));
                app.send_notification(Some("xdelta-connection-state"), &notif);
            }
        }
    }

    /// Connects the dashboard's server picker and the "Connect to Fastest"
    /// button to the shared selection state.
    ///
    /// The dashboard picker and the Servers page edit the same selection, so
    /// changing one is reflected in the other rather than letting them drift.
    fn wire_dashboard_server_controls(
        main_window: Rc<MainWindow>,
        controller: Rc<RefCell<ConnectionController>>,
        tray_handle: Rc<RefCell<Option<crate::ui::tray::TrayHandle>>>,
        app: Application,
        last_notified_state: Rc<RefCell<Option<ConnectionState>>>,
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
                button.set_label("Finding fastest server...");
                dashboard.set_subscription_status("Testing servers for lowest latency...");

                let candidates = {
                    let nodes = servers_view.nodes.borrow();
                    crate::net::probe::deduplicate(&crate::net::probe::usable_nodes(&nodes))
                };

                let dashboard = dashboard.clone();
                let servers_view = servers_view.clone();
                let main_window = main_window.clone();
                let controller = controller.clone();
                let button = button.clone();
                let tray_handle = tray_handle.clone();
                let app = app.clone();
                let last_notified_state = last_notified_state.clone();

                glib::MainContext::default().spawn_local(async move {
                    if candidates.is_empty() {
                        button.set_sensitive(true);
                        button.set_label("Connect to Fastest");
                        dashboard.set_subscription_status("No servers available to test");
                        return;
                    }

                    // Fast concurrent TCP ping across candidates (1-2s total)
                    let mut updated = crate::net::batch_ping_nodes(
                        candidates,
                        30,
                        std::time::Duration::from_secs(2),
                    )
                    .await;
                    updated.sort_by_key(|n| n.sort_latency());

                    let working_list: Vec<Node> = updated
                        .into_iter()
                        .filter(|n| n.latency_ms.is_some())
                        .collect();

                    button.set_sensitive(true);
                    button.set_label("Connect to Fastest");

                    if working_list.is_empty() {
                        dashboard.set_subscription_status("No responsive server found");
                        return;
                    }

                    let best = working_list[0].clone();
                    servers_view.set_nodes(working_list);
                    dashboard.set_server_choices(&servers_view.nodes.borrow(), Some(&best.id));
                    *servers_view.selected_node_id.borrow_mut() = Some(best.id.clone());

                    dashboard.set_active_server(
                        &best.name,
                        &format!("{} • {}", best.protocol.as_str(), describe_latency(&best)),
                    );

                    // Connect asynchronously without blocking the UI thread
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

                    let win_c = main_window.clone();
                    let ctl_c = controller.clone();
                    let view_c = dashboard.clone();
                    let tray_c = tray_handle.clone();
                    let app_c = app.clone();
                    let last_c = last_notified_state.clone();
                    let auto_proxy = main_window.dashboard_view.proxy_switch.is_active();

                    win_c.hide_error();
                    {
                        let mut ctl = ctl_c.borrow_mut();
                        let _ = ctl.start_connecting();
                        view_c.render_outcome(&crate::core::TransitionOutcome::ok(
                            crate::core::ConnectionState::Connecting,
                        ));
                        Self::sync_tray_and_notify(&ctl, &tray_c.borrow(), &app_c, &last_c);
                    }

                    let prof_clone = profile.clone();
                    glib::MainContext::default().spawn_local(async move {
                        let res = tokio::task::spawn_blocking(move || {
                            crate::core::ConnectionController::start_and_verify(&prof_clone)
                        })
                        .await;

                        let mut ctl = ctl_c.borrow_mut();
                        if !matches!(ctl.state(), crate::core::ConnectionState::Connecting) {
                            return;
                        }

                        let outcome = match res {
                            Ok(Ok((process, path, latency))) => {
                                let outcome = ctl
                                    .finish_connect(&profile, process, path, latency, auto_proxy);
                                win_c.hide_error();
                                outcome
                            }
                            Ok(Err(e)) => {
                                win_c.show_error(&format!("Connection failed: {e}"));
                                ctl.fail_connect(e)
                            }
                            Err(e) => {
                                let msg = format!("Task failure: {e}");
                                win_c.show_error(&msg);
                                ctl.fail_connect(msg)
                            }
                        };

                        view_c.render_outcome(&outcome);
                        view_c.set_active_mode(ctl.active_mode());
                        if !outcome.state.is_active() {
                            view_c.clear_traffic();
                        }
                        Self::sync_tray_and_notify(&ctl, &tray_c.borrow(), &app_c, &last_c);
                    });
                });
            });
        }
    }

    fn wire_power_button(
        main_window: Rc<MainWindow>,
        controller: Rc<RefCell<ConnectionController>>,
        tray_handle: Rc<RefCell<Option<crate::ui::tray::TrayHandle>>>,
        app: Application,
        last_notified_state: Rc<RefCell<Option<ConnectionState>>>,
    ) {
        let status_badge = main_window.dashboard_view.status_badge.clone();
        let server_name_lbl = main_window.dashboard_view.server_name_label.clone();
        let server_sub_label = main_window.dashboard_view.server_sub_label.clone();
        let auto_proxy = main_window.dashboard_view.proxy_switch.clone();
        let servers_view = main_window.servers_view.clone();
        let settings_view = main_window.settings_view.clone();
        let routing_view = main_window.routing_view.clone();

        // The power button triggers a completely non-blocking connection
        // cycle: the UI transitions to Connecting immediately, and the core
        // start + network verification run on a background worker thread.
        {
            let view = main_window.dashboard_view.clone();
            let win = main_window.clone();
            let controller = controller.clone();
            let servers_view = servers_view.clone();
            let settings_view = settings_view.clone();
            let routing_view = routing_view.clone();
            let power_btn = main_window.dashboard_view.power_button.clone();
            let tray_handle = tray_handle.clone();
            let app = app.clone();
            let last_notified_state = last_notified_state.clone();

            power_btn.connect_clicked(move |_| {
                let mut ctl = controller.borrow_mut();
                let was_active = ctl.state().is_active()
                    || matches!(ctl.state(), crate::core::ConnectionState::Connecting);

                if was_active {
                    let outcome = ctl.disconnect();
                    view.render_outcome(&outcome);
                    view.set_active_mode(None);
                    view.clear_traffic();
                    win.hide_error();
                    Self::sync_tray_and_notify(
                        &ctl,
                        &tray_handle.borrow(),
                        &app,
                        &last_notified_state,
                    );
                    return;
                }

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
                    let has_nodes = !servers_view.nodes.borrow().is_empty();
                    let msg = if has_nodes {
                        "Select a server in the list first."
                    } else {
                        "No servers available yet. The list is still loading, \
                         or the server test found none that work — use \
                         \"Connect to Fastest\" to test again."
                    };
                    view.render_error(msg);
                    win.show_error(msg);
                    return;
                };

                let mut profile = ConnectionProfile::new(node, settings_view.connection_mode());
                settings_view.apply_ports_to(&mut profile);
                routing_view.apply_to(&mut profile);
                settings_view.apply_tun_to(&mut profile);

                if let Err(e) = ctl.start_connecting() {
                    view.render_error(&e);
                    win.show_error(&e);
                    return;
                }

                win.hide_error();
                let connecting_outcome =
                    crate::core::TransitionOutcome::ok(crate::core::ConnectionState::Connecting);
                view.render_outcome(&connecting_outcome);
                Self::sync_tray_and_notify(&ctl, &tray_handle.borrow(), &app, &last_notified_state);
                drop(ctl);

                // Run the background connection establishment and verification off the GTK thread
                let controller_c = controller.clone();
                let view_c = view.clone();
                let win_c = win.clone();
                let tray_c = tray_handle.clone();
                let app_c = app.clone();
                let last_c = last_notified_state.clone();
                let auto_proxy_active = auto_proxy.is_active();
                let prof = profile.clone();

                glib::MainContext::default().spawn_local(async move {
                    let connect_res = tokio::task::spawn_blocking(move || {
                        crate::core::ConnectionController::start_and_verify(&prof)
                    })
                    .await;

                    let mut ctl = controller_c.borrow_mut();
                    if !matches!(ctl.state(), crate::core::ConnectionState::Connecting) {
                        return; // user cancelled while connecting
                    }

                    let outcome = match connect_res {
                        Ok(Ok((process, path, latency))) => {
                            let outcome = ctl.finish_connect(
                                &profile,
                                process,
                                path,
                                latency,
                                auto_proxy_active,
                            );
                            win_c.hide_error();
                            outcome
                        }
                        Ok(Err(e)) => {
                            win_c.show_error(&format!("Connection failed: {e}"));
                            ctl.fail_connect(e)
                        }
                        Err(e) => {
                            let msg = format!("Background task error: {e}");
                            win_c.show_error(&msg);
                            ctl.fail_connect(msg)
                        }
                    };

                    view_c.render_outcome(&outcome);
                    view_c.set_active_mode(ctl.active_mode());
                    if !outcome.state.is_active() {
                        view_c.clear_traffic();
                    }
                    Self::sync_tray_and_notify(&ctl, &tray_c.borrow(), &app_c, &last_c);
                });
            });
        }

        let _ = (&status_badge, &server_name_lbl, &server_sub_label);

        // Periodic tick: non-blocking background traffic polling and health checks.
        let controller = controller.clone();
        let view = main_window.dashboard_view.clone();
        let win = main_window.clone();
        let tray_handle_for_tick = tray_handle.clone();
        let app_for_tick = app.clone();
        let last_notified_for_tick = last_notified_state.clone();
        let was_connected = Rc::new(std::cell::Cell::new(false));
        let last_stats_ok = Rc::new(std::cell::Cell::new(true));

        static STATS_POLL_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

        glib::timeout_add_local(TRAFFIC_POLL_INTERVAL, move || {
            let mut ctl = controller.borrow_mut();

            // Always sync tray state on tick
            Self::sync_tray_and_notify(
                &ctl,
                &tray_handle_for_tick.borrow(),
                &app_for_tick,
                &last_notified_for_tick,
            );

            // Live traffic is polled only while a connection is up
            if matches!(ctl.state(), crate::core::ConnectionState::Connected) {
                was_connected.set(true);

                if !ctl.is_core_alive() {
                    let tail = ctl.core_output_tail();
                    log::error!(
                        "core died while connected; last output:\n  {}",
                        if tail.is_empty() {
                            "(no output captured)".to_string()
                        } else {
                            tail.join("\n  ")
                        }
                    );

                    let outcome = ctl.note_core_died();
                    view.render_outcome(&outcome);
                    view.set_active_mode(None);
                    view.clear_traffic();
                    win.show_error(
                        "Proxy core exited unexpectedly. Check Diagnostics & Logs for details.",
                    );
                    return glib::ControlFlow::Continue;
                }

                // Offload querying stats & health check to a background worker
                // so the GTK main loop NEVER blocks on subprocesses or network timeouts.
                if STATS_POLL_IN_FLIGHT
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    let (api_port, socks_port, _) = ctl.live_ports();
                    let should_health = ctl.should_recheck_health();
                    let ctl_c = controller.clone();
                    let view_c = view.clone();
                    let win_c = win.clone();
                    let last_ok_c = last_stats_ok.clone();

                    glib::MainContext::default().spawn_local(async move {
                        let (stats_res, health_res) = tokio::task::spawn_blocking(move || {
                            let stats = crate::net::stats::query_counters(api_port);
                            let health = if should_health {
                                Some(crate::net::probe::verify_connection(
                                    socks_port,
                                    std::time::Duration::from_secs(3),
                                ))
                            } else {
                                None
                            };
                            (stats, health)
                        })
                        .await
                        .unwrap_or((Err("join error".into()), None));

                        STATS_POLL_IN_FLIGHT.store(false, Ordering::Relaxed);

                        let mut ctl = ctl_c.borrow_mut();
                        if matches!(ctl.state(), crate::core::ConnectionState::Connected) {
                            match stats_res {
                                Ok(counters) => {
                                    if !last_ok_c.get() {
                                        last_ok_c.set(true);
                                    }
                                    let (rates, totals) = ctl.apply_counters(counters);
                                    view_c.render_traffic(rates, totals);
                                }
                                Err(e) => {
                                    if last_ok_c.get() {
                                        log::warn!("traffic statistics unavailable: {e}");
                                        last_ok_c.set(false);
                                    }
                                }
                            }

                            if let Some(Err(reason)) = health_res {
                                log::warn!("connection health check failed: {reason}");
                                win_c.show_error(&format!("Tunnel degraded: {reason}"));
                                let outcome = ctl.note_connection_degraded(&reason);
                                view_c.render_outcome(&outcome);
                                view_c.clear_traffic();
                            }
                        }
                    });
                }
            } else if was_connected.replace(false) {
                log::info!("connection is no longer active; polling stopped");
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

            let settings_c = settings_view_click.clone();
            let ctl_c = controller_click.clone();
            let btn_c = btn.clone();

            glib::MainContext::default().spawn_local(async move {
                let res = tokio::task::spawn_blocking(move || tun::run_setup(&binary)).await;

                btn_c.set_label("Grant Permission");

                match res {
                    Ok(Ok(())) => {
                        log::info!("one-time TUN authorization completed successfully");
                    }
                    Ok(Err(e)) => {
                        log::warn!("TUN capability setup failed: {e}");
                    }
                    Err(e) => {
                        log::warn!("TUN setup task failed: {e}");
                    }
                }

                // Re-read the real state rather than assuming the grant worked.
                let readiness = ctl_c.borrow().tun_readiness();
                settings_c.render_tun_status(&readiness);
            });
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
                let initial = crate::net::probe::top_servers(
                    &cached.nodes,
                    crate::net::probe::TOP_SERVERS_SHOWN,
                );
                servers_view.set_nodes(initial);
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
                    let initial = crate::net::probe::top_servers(
                        &nodes,
                        crate::net::probe::TOP_SERVERS_SHOWN,
                    );
                    view_for_fetch.set_nodes(initial);

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

    /// Runs the non-blocking latency pass over the servers and keeps the fastest.
    fn probe_and_rank(main_window: Rc<MainWindow>, _controller: Rc<RefCell<ConnectionController>>) {
        use std::sync::atomic::Ordering;

        if PROBE_IN_FLIGHT.swap(true, Ordering::SeqCst) {
            log::debug!("a probe pass is already running; skipping this request");
            return;
        }

        let servers_view = main_window.servers_view.clone();
        let dashboard = main_window.dashboard_view.clone();

        let candidates = crate::net::probe::deduplicate(&crate::net::probe::usable_nodes(
            &servers_view.nodes.borrow(),
        ));
        if candidates.is_empty() {
            PROBE_IN_FLIGHT.store(false, Ordering::SeqCst);
            return;
        }

        glib::MainContext::default().spawn_local(async move {
            dashboard.set_subscription_status(&format!(
                "Testing latency for {} servers...",
                candidates.len()
            ));

            let mut updated =
                crate::net::batch_ping_nodes(candidates, 30, std::time::Duration::from_secs(2))
                    .await;
            updated.sort_by_key(|n| n.sort_latency());

            let working_count = updated.iter().filter(|n| n.latency_ms.is_some()).count();
            let shown =
                crate::net::probe::top_servers(&updated, crate::net::probe::TOP_SERVERS_SHOWN);

            servers_view.set_nodes(shown.clone());
            dashboard.set_subscription_status(&format!(
                "{working_count} of {} servers responsive; showing the fastest {}",
                updated.len(),
                shown.len()
            ));

            log::info!(
                "ping pass finished: {working_count} responsive of {} tested, showing {}",
                updated.len(),
                shown.len()
            );

            PROBE_IN_FLIGHT.store(false, Ordering::SeqCst);
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
