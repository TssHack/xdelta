//! Connection orchestration: builds a config, starts the core, wires up the
//! desktop proxy and keeps the state machine honest.
//!
//! Everything that touches the outside world (process, GSettings, TUN
//! readiness) lives behind [`ConnectionController`], so [`crate::app`] only
//! translates user actions into calls here and renders the resulting state.

use crate::core::config::{generate_xray_config, write_config_to_temp_file};
use crate::core::process::CoreProcessManager;
use crate::core::session::{ConnectionMode, ConnectionProfile, ConnectionState};
use crate::model::Node;
use crate::net::stats::{TrafficCounters, TrafficRates, TrafficSampler};
use crate::net::sysproxy::{GnomeProxyManager, ProxySnapshot};
use crate::net::tun::{self, TunReadiness};
use std::path::PathBuf;

/// Result of a connect/disconnect attempt, ready for the UI to render.
#[derive(Debug, Clone)]
pub struct TransitionOutcome {
    pub state: ConnectionState,
    /// Extra context for the status line, such as why a fallback happened.
    pub note: Option<String>,
}

impl TransitionOutcome {
    pub fn ok(state: ConnectionState) -> Self {
        Self { state, note: None }
    }

    pub fn with_note(state: ConnectionState, note: impl Into<String>) -> Self {
        Self {
            state,
            note: Some(note.into()),
        }
    }
}

/// How often a live connection is re-confirmed to carry traffic.
///
/// Frequent enough to notice a drop quickly, rare enough that the check itself
/// is not a traffic source of its own.
const HEALTH_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(20);

/// Owns the running core process and the state needed to tear it down cleanly.
pub struct ConnectionController {
    state: ConnectionState,
    process: Option<CoreProcessManager>,
    /// Desktop proxy settings captured before we changed them.
    proxy_snapshot: Option<ProxySnapshot>,
    /// Mode of the connection that is currently up.
    active_mode: Option<ConnectionMode>,
    api_port: u16,
    /// Local SOCKS port of the live connection, used by health checks.
    socks_port: u16,
    /// Local HTTP port of the live connection.
    ///
    /// Tracked separately because browsers configured for the desktop proxy use
    /// this port, not the SOCKS one; checking only SOCKS could declare a healthy
    /// browser session dead.
    http_port: u16,
    /// When the last health check ran.
    last_health_check: Option<std::time::Instant>,
    sampler: TrafficSampler,
    config_path: Option<PathBuf>,
    active_node: Option<Node>,
}

impl Default for ConnectionController {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionController {
    pub fn new() -> Self {
        Self {
            state: ConnectionState::Disconnected,
            process: None,
            proxy_snapshot: None,
            active_mode: None,
            active_node: None,
            api_port: crate::core::session::DEFAULT_API_PORT,
            socks_port: crate::core::session::DEFAULT_SOCKS_PORT,
            http_port: crate::core::session::DEFAULT_HTTP_PORT,
            last_health_check: None,
            sampler: TrafficSampler::new(),
            config_path: None,
        }
    }

    pub fn state(&self) -> &ConnectionState {
        &self.state
    }

    pub fn active_node(&self) -> Option<&Node> {
        self.active_node.as_ref()
    }

    pub fn active_mode(&self) -> Option<ConnectionMode> {
        self.active_mode
    }

    pub fn api_port(&self) -> u16 {
        self.api_port
    }

    /// Reports whether TUN can be used, for the settings page.
    pub fn tun_readiness(&self) -> TunReadiness {
        match CoreProcessManager::find_xray_binary() {
            Ok(binary) => tun::check_readiness(&binary),
            Err(e) => TunReadiness::Unsupported(e),
        }
    }

    /// Pre-validates, starts and verifies the core process on a background thread.
    pub fn start_and_verify(
        profile: &ConnectionProfile,
    ) -> Result<(CoreProcessManager, PathBuf, Option<u32>), String> {
        if profile.mode == ConnectionMode::Tun {
            let binary = CoreProcessManager::find_xray_binary()?;
            match tun::check_readiness(&binary) {
                TunReadiness::Ready => {}
                readiness => {
                    return Err(format!("TUN unavailable: {}", readiness.message()));
                }
            }
            if std::net::TcpListener::bind(("127.0.0.1", profile.api_port)).is_err() {
                CoreProcessManager::terminate_stale_cores();
                if let Err(e) = std::net::TcpListener::bind(("127.0.0.1", profile.api_port)) {
                    return Err(format!(
                        "API port {} is already in use by another application: {e}",
                        profile.api_port
                    ));
                }
            }
        } else {
            for (label, port) in [
                ("SOCKS", profile.socks_port),
                ("HTTP", profile.http_port),
                ("API", profile.api_port),
            ] {
                if std::net::TcpListener::bind(("127.0.0.1", port)).is_err() {
                    CoreProcessManager::terminate_stale_cores();
                    if let Err(e) = std::net::TcpListener::bind(("127.0.0.1", port)) {
                        return Err(format!(
                            "{label} port {port} is already in use by another application: {e}"
                        ));
                    }
                }
            }
        }

        let config = generate_xray_config(profile)?;
        let path = write_config_to_temp_file(&config)?;
        let mut process = CoreProcessManager::start(&path)?;

        if let Err(e) = process.wait_until_ready(std::time::Duration::from_secs(3)) {
            let _ = process.stop();
            let _ = std::fs::remove_file(&path);
            return Err(e);
        }

        let mut latency = None;
        if profile.mode == ConnectionMode::SystemProxy {
            match crate::net::probe::verify_connection(
                profile.socks_port,
                std::time::Duration::from_secs(6),
            ) {
                Ok(lat) => {
                    latency = Some(lat);
                }
                Err(e) => {
                    let _ = process.stop();
                    let _ = std::fs::remove_file(&path);
                    return Err(format!(
                        "Server reachable but proxy verification failed: {e}"
                    ));
                }
            }
        }

        Ok((process, path, latency))
    }

    /// Brings a connection up.
    ///
    /// The order is deliberate: validate, generate, write, start, then touch
    /// the desktop. Nothing outside the app is modified until the core is
    /// confirmed running, so a failed start leaves the machine untouched.
    pub fn connect(
        &mut self,
        profile: &ConnectionProfile,
        auto_system_proxy: bool,
    ) -> TransitionOutcome {
        if !self.state.can_connect() {
            return TransitionOutcome::with_note(
                self.state.clone(),
                "a connection is already in progress",
            );
        }

        log::info!(
            "connecting: {} {}:{} via {}",
            profile.node.protocol.as_str(),
            profile.node.server,
            profile.node.port,
            profile.mode.label()
        );

        self.state = ConnectionState::Connecting;

        // TUN needs a kernel device, so refuse early with an actionable message
        // rather than letting the core fail with an opaque error.
        if profile.mode == ConnectionMode::Tun {
            match self.tun_readiness() {
                TunReadiness::Ready => {}
                readiness => {
                    self.state = ConnectionState::Failed(format!(
                        "TUN not available: {}",
                        readiness.message()
                    ));
                    return TransitionOutcome::with_note(
                        self.state.clone(),
                        format!("TUN unavailable. {}", readiness.message()),
                    );
                }
            }
        }

        let config = match generate_xray_config(profile) {
            Ok(config) => config,
            Err(e) => {
                self.state = ConnectionState::Failed(e.clone());
                return TransitionOutcome::with_note(self.state.clone(), e);
            }
        };

        let path = match write_config_to_temp_file(&config) {
            Ok(path) => path,
            Err(e) => {
                self.state = ConnectionState::Failed(e.clone());
                return TransitionOutcome::with_note(self.state.clone(), e);
            }
        };

        let mut process = match CoreProcessManager::start(&path) {
            Ok(process) => process,
            Err(e) => {
                self.state = ConnectionState::Failed(e.clone());
                return TransitionOutcome::with_note(self.state.clone(), e);
            }
        };

        // Give the core a moment to bind its listeners. A process that exits
        // immediately (bad config, missing geo assets) is a failure, not a
        // connection, and reporting it as connected was the original bug.
        match process.wait_until_ready(std::time::Duration::from_secs(3)) {
            Ok(()) => {}
            Err(e) => {
                let _ = process.stop();
                self.state = ConnectionState::Failed(e.clone());
                return TransitionOutcome::with_note(self.state.clone(), e);
            }
        }

        self.process = Some(process);
        self.config_path = Some(path);
        self.api_port = profile.api_port;
        self.socks_port = profile.socks_port;
        self.http_port = profile.http_port;
        self.active_mode = Some(profile.mode);
        self.sampler.reset();
        self.last_health_check = None;

        let mut note = None;

        // Verify the tunnel BEFORE touching the desktop proxy.
        //
        // Order matters here, and getting it wrong was a real bug: `curl`
        // honours the desktop proxy settings in addition to `-x`. Once the
        // system proxy pointed at our own listener, the check's traffic was
        // routed back into the proxy it was testing, failed instantly, and the
        // connection was then torn down as if the server were dead. The
        // listener exists either way, so checking first avoids the loop.
        if profile.mode == ConnectionMode::SystemProxy {
            match crate::net::probe::verify_connection(
                profile.socks_port,
                std::time::Duration::from_secs(10),
            ) {
                Ok(latency) => {
                    note = Some(format!("verified in {latency} ms"));
                }
                Err(e) => {
                    // The core is up but carries nothing. That is a failed
                    // connection, so everything this attempt changed is undone
                    // rather than leaving the user with a half-applied state.
                    self.teardown_after_failed_verification();
                    self.state = ConnectionState::Failed(format!("server not working: {e}"));
                    return TransitionOutcome::with_note(self.state.clone(), e);
                }
            }
        }

        // In TUN mode, install the system routes so traffic actually flows into xdelta0
        if profile.mode == ConnectionMode::Tun {
            if let Err(e) = crate::net::tun::apply_tun_routes("xdelta0", &profile.node.server) {
                log::warn!("Could not apply TUN system routes: {e}");
                let msg = format!("TUN routing: {e}");
                note = Some(match note {
                    Some(existing) => format!("{existing}; {msg}"),
                    None => msg,
                });
            }
        }

        // Apply desktop proxy whenever auto_system_proxy is enabled.
        // This provides an immediate, zero-latency proxy path for web browsers,
        // Telegram and desktop applications.
        if auto_system_proxy {
            match GnomeProxyManager::snapshot() {
                Ok(snapshot) => {
                    self.proxy_snapshot = Some(snapshot);
                    if let Err(e) = GnomeProxyManager::enable(profile.http_port, profile.socks_port)
                    {
                        // The tunnel itself works, so this is a partial success:
                        // stay connected but report that the desktop proxy did
                        // not apply.
                        let msg = format!("system proxy not applied: {e}");
                        note = Some(match note {
                            Some(existing) => format!("{existing}; {msg}"),
                            None => msg,
                        });
                    }
                }
                Err(e) => {
                    let msg = format!("system proxy not applied: {e}");
                    note = Some(match note {
                        Some(existing) => format!("{existing}; {msg}"),
                        None => msg,
                    });
                }
            }
        }

        self.state = ConnectionState::Connected;

        log::info!(
            "connected: {} ({})",
            profile.node.name,
            note.as_deref().unwrap_or("no note")
        );
        self.active_node = Some(profile.node.clone());

        match note {
            Some(note) => TransitionOutcome::with_note(self.state.clone(), note),
            None => TransitionOutcome::ok(self.state.clone()),
        }
    }

    /// Tears a connection down.
    ///
    /// Every step is attempted even if an earlier one fails, so a failure in one
    /// cleanup cannot leave the desktop pointing at a proxy that no longer
    /// exists.
    pub fn disconnect(&mut self) -> TransitionOutcome {
        if matches!(self.state, ConnectionState::Disconnected) {
            return TransitionOutcome::ok(self.state.clone());
        }

        self.state = ConnectionState::Disconnecting;
        let mut problems: Vec<String> = Vec::new();

        // In TUN mode, tear down system routes first
        if self.active_mode == Some(ConnectionMode::Tun) {
            let _ = crate::net::tun::revert_tun_routes("xdelta0", "");
        }

        // Restore the desktop first: the core is still alive here, so the
        // window where traffic would hit a dead proxy is as small as possible.
        if let Some(snapshot) = self.proxy_snapshot.take() {
            if let Err(e) = GnomeProxyManager::restore(&snapshot) {
                problems.push(format!("could not restore system proxy: {e}"));
                // Do not leave the desktop pointing at our now-dead port.
                let _ = GnomeProxyManager::disable();
            }
        } else if self.active_mode == Some(ConnectionMode::SystemProxy) {
            let _ = GnomeProxyManager::disable();
        }

        if let Some(mut process) = self.process.take() {
            if let Err(e) = process.stop() {
                problems.push(format!("could not stop core: {e}"));
            }
        }

        if let Some(path) = self.config_path.take() {
            let _ = std::fs::remove_file(path);
        }

        self.sampler.reset();
        self.active_mode = None;
        self.active_node = None;
        self.state = ConnectionState::Disconnected;

        if problems.is_empty() {
            TransitionOutcome::ok(self.state.clone())
        } else {
            TransitionOutcome::with_note(self.state.clone(), problems.join("; "))
        }
    }

    /// Undoes everything a failed connection attempt changed.
    ///
    /// Reuses the ordinary disconnect path so there is exactly one place that
    /// knows how to stop the core, restore the desktop proxy and remove the
    /// temporary config. Duplicating that logic here would let the two drift.
    fn teardown_after_failed_verification(&mut self) {
        let outcome = self.disconnect();
        if let Some(note) = outcome.note {
            log::warn!("cleanup after a failed connection reported: {}", note);
        }
    }

    /// Polls live traffic counters while connected.
    pub fn poll_traffic(&mut self) -> Option<(TrafficRates, TrafficCounters)> {
        if !matches!(self.state, ConnectionState::Connected) {
            return None;
        }

        match crate::net::stats::query_counters(self.api_port) {
            Ok(counters) => Some((self.sampler.update(counters), counters)),
            Err(e) => {
                log::debug!("traffic poll failed: {}", e);
                None
            }
        }
    }

    /// Updates traffic stats with counters queried from a background task.
    pub fn apply_counters(&mut self, counters: TrafficCounters) -> (TrafficRates, TrafficCounters) {
        let rates = self.sampler.update(counters);
        (rates, counters)
    }

    /// Live ports tuple: `(api_port, socks_port, http_port)`.
    pub fn live_ports(&self) -> (u16, u16, u16) {
        (self.api_port, self.socks_port, self.http_port)
    }

    /// Marks the start of a connection attempt.
    pub fn start_connecting(&mut self) -> Result<(), String> {
        if !self.state.can_connect() {
            return Err("a connection is already in progress".to_string());
        }
        self.state = ConnectionState::Connecting;
        Ok(())
    }

    /// Completes the connection after the background worker started the core.
    pub fn finish_connect(
        &mut self,
        profile: &ConnectionProfile,
        process: CoreProcessManager,
        config_path: PathBuf,
        verified_latency: Option<u32>,
        auto_system_proxy: bool,
    ) -> TransitionOutcome {
        self.process = Some(process);
        self.config_path = Some(config_path);
        self.api_port = profile.api_port;
        self.socks_port = profile.socks_port;
        self.http_port = profile.http_port;
        self.active_mode = Some(profile.mode);
        self.sampler.reset();
        self.last_health_check = None;
        self.active_node = Some(profile.node.clone());

        let mut note = verified_latency.map(|l| format!("verified in {l} ms"));

        // In TUN mode, install the system routes so traffic actually flows into xdelta0
        if profile.mode == ConnectionMode::Tun {
            if let Err(e) = crate::net::tun::apply_tun_routes("xdelta0", &profile.node.server) {
                log::warn!("Could not apply TUN system routes: {e}");
                let msg = format!("TUN routing: {e}");
                note = Some(match note {
                    Some(existing) => format!("{existing}; {msg}"),
                    None => msg,
                });
            }
        }

        // Apply desktop proxy whenever auto_system_proxy is enabled.
        if auto_system_proxy {
            match GnomeProxyManager::snapshot() {
                Ok(snapshot) => {
                    self.proxy_snapshot = Some(snapshot);
                    if let Err(e) = GnomeProxyManager::enable(profile.http_port, profile.socks_port)
                    {
                        let msg = format!("system proxy not applied: {e}");
                        note = Some(match note {
                            Some(existing) => format!("{existing}; {msg}"),
                            None => msg,
                        });
                    }
                }
                Err(e) => {
                    let msg = format!("system proxy not applied: {e}");
                    note = Some(match note {
                        Some(existing) => format!("{existing}; {msg}"),
                        None => msg,
                    });
                }
            }
        }

        self.state = ConnectionState::Connected;
        log::info!(
            "connected: {} ({})",
            profile.node.name,
            note.as_deref().unwrap_or("no note")
        );

        match note {
            Some(note) => TransitionOutcome::with_note(self.state.clone(), note),
            None => TransitionOutcome::ok(self.state.clone()),
        }
    }

    /// Sets the connection state to failed after an error.
    pub fn fail_connect(&mut self, error: String) -> TransitionOutcome {
        self.teardown_after_failed_verification();
        self.state = ConnectionState::Failed(error.clone());
        TransitionOutcome::with_note(self.state.clone(), error)
    }

    /// Confirms the core is still running; used by the UI tick.
    pub fn is_core_alive(&mut self) -> bool {
        match self.process.as_mut() {
            Some(process) => process.is_running(),
            None => false,
        }
    }

    /// Recent output from the core, for diagnosing a drop.
    pub fn core_output_tail(&self) -> Vec<String> {
        match self.process.as_ref() {
            Some(process) => process.recent_output(),
            None => Vec::new(),
        }
    }

    /// Whether it is time to confirm the tunnel still carries traffic.
    ///
    /// A started connection can stop forwarding at any moment: most servers in
    /// the bundled subscription are unstable, and a core can also be alive while
    /// its listener is unusable. Without a periodic check, such a connection
    /// stays displayed as healthy forever, which is exactly the "connects then
    /// silently stops" behaviour.
    pub fn should_recheck_health(&self) -> bool {
        if !matches!(self.state, ConnectionState::Connected) {
            return false;
        }

        // Only proxy mode has a local listener that can be checked without
        // touching the host's own routing.
        if self.active_mode != Some(ConnectionMode::SystemProxy) {
            return false;
        }

        match self.last_health_check {
            Some(at) => at.elapsed() >= HEALTH_CHECK_INTERVAL,
            None => true,
        }
    }

    /// Runs the periodic connectivity check across every local listener.
    ///
    /// All listeners are checked, not one. Applications differ in which they
    /// use: a browser configured for the desktop proxy takes the HTTP port, while
    /// the SOCKS port serves anything speaking SOCKS. Checking only SOCKS meant a
    /// perfectly healthy browser session could be declared dead and torn down.
    ///
    /// Returns `Ok(None)` when no check was due, so the caller can distinguish
    /// "nothing to do" from "checked and fine".
    pub fn recheck_health(&mut self) -> Result<Option<u32>, String> {
        if !self.should_recheck_health() {
            return Ok(None);
        }

        self.last_health_check = Some(std::time::Instant::now());

        let mut failures: Vec<String> = Vec::new();
        let mut best: Option<u32> = None;

        for (label, port, via_http) in [
            ("SOCKS", self.socks_port, false),
            ("HTTP", self.http_port, true),
        ] {
            let result = if via_http {
                crate::net::probe::verify_connection_via_http(
                    port,
                    std::time::Duration::from_secs(8),
                )
            } else {
                crate::net::probe::verify_connection(port, std::time::Duration::from_secs(8))
            };

            match result {
                Ok(latency) => best = Some(best.map_or(latency, |b: u32| b.min(latency))),
                Err(e) => failures.push(format!("{label}: {e}")),
            }
        }

        // A connection is only unhealthy when every listener fails. One working
        // listener is enough for the connection to be useful, and a single
        // failure is reported without claiming the tunnel is broken.
        if failures.len() == 2 {
            return Err(failures.join("; "));
        }

        if !failures.is_empty() {
            log::debug!("health check: partial failure ({})", failures.join("; "));
        }

        Ok(best)
    }

    /// Reports that traffic stopped flowing, without tearing the connection down.
    ///
    /// Deliberately does NOT disconnect: the core is still running and a single
    /// failed probe is weak evidence. Applications also retry on their own, and
    /// killing the tunnel mid-retry guarantees the failure the user sees. The
    /// state is surfaced so the UI can say something is wrong, and the user
    /// decides whether to reconnect.
    pub fn note_connection_degraded(&mut self, reason: &str) -> TransitionOutcome {
        log::warn!("connectivity check failed but the connection is kept up: {reason}");
        TransitionOutcome::with_note(self.state.clone(), format!("degraded: {reason}"))
    }

    /// Every proxy port a client might be pointed at.
    ///
    /// Used by the settings page to describe which ports are being served.
    pub fn served_ports(&self) -> Vec<(&'static str, u16)> {
        match self.active_mode {
            Some(ConnectionMode::SystemProxy) => {
                vec![("SOCKS", self.socks_port), ("HTTP", self.http_port)]
            }
            Some(ConnectionMode::Tun) => vec![],
            None => vec![],
        }
    }

    /// Marks the connection as failed after the core died unexpectedly.
    pub fn note_core_died(&mut self) -> TransitionOutcome {
        if let Some(mut process) = self.process.take() {
            let _ = process.stop();
        }
        if let Some(snapshot) = self.proxy_snapshot.take() {
            let _ = GnomeProxyManager::restore(&snapshot);
        }
        self.active_mode = None;
        self.active_node = None;
        self.sampler.reset();
        self.state = ConnectionState::Failed("the proxy core stopped unexpectedly".to_string());
        TransitionOutcome::ok(self.state.clone())
    }
}

impl Drop for ConnectionController {
    /// Best-effort cleanup if the app exits while connected, so the desktop is
    /// not left pointing at a dead proxy.
    fn drop(&mut self) {
        if self.proxy_snapshot.is_some() {
            if let Some(snapshot) = self.proxy_snapshot.take() {
                let _ = GnomeProxyManager::restore(&snapshot);
            }
        }
        if let Some(mut process) = self.process.take() {
            let _ = process.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::session::{RoutingStrategy, TunOptions};
    use crate::model::{Node, Protocol};

    fn profile(mode: ConnectionMode) -> ConnectionProfile {
        let node = Node::new(
            "1".into(),
            "Test".into(),
            "127.0.0.1".into(),
            1,
            Protocol::Vless,
            "00000000-0000-0000-0000-000000000009".into(),
        );
        let mut p = ConnectionProfile::new(node, mode);
        // Keep the generated config valid without touching the network.
        p.routing.strategy = RoutingStrategy::Direct;
        p.tun = TunOptions::default();
        p
    }

    /// Rejecting a second connect while one is in flight keeps the state
    /// machine from being driven into an inconsistent state.
    #[test]
    fn connect_is_rejected_while_connecting() {
        let mut controller = ConnectionController::new();
        controller.state = ConnectionState::Connecting;

        let outcome = controller.connect(&profile(ConnectionMode::SystemProxy), false);
        assert!(outcome.note.unwrap().contains("in progress"));
    }

    /// An invalid profile must fail before the core is even started, so nothing
    /// on the system is touched.
    #[test]
    fn invalid_profile_fails_without_starting_anything() {
        let mut controller = ConnectionController::new();
        let mut p = profile(ConnectionMode::SystemProxy);
        p.socks_port = p.http_port;

        let outcome = controller.connect(&p, false);
        assert!(matches!(outcome.state, ConnectionState::Failed(_)));
        assert!(!outcome.state.is_active());
        assert!(controller.process.is_none());
    }

    /// Unsupported protocols must surface as a clear failure, not a connection.
    #[test]
    fn unsupported_protocol_fails_cleanly() {
        let mut controller = ConnectionController::new();
        let mut p = profile(ConnectionMode::SystemProxy);
        p.node.protocol = Protocol::Shadowsocks;

        let outcome = controller.connect(&p, false);
        assert!(matches!(outcome.state, ConnectionState::Failed(_)));
        assert_eq!(outcome.state.badge_label(), "ERROR");
    }

    /// TUN without the capability must be refused before touching the system.
    #[test]
    fn tun_without_privileges_is_refused_early() {
        let mut controller = ConnectionController::new();
        if controller.tun_readiness().is_ready() {
            // Machine already has the capability; the refusal path cannot be
            // exercised here without changing system state.
            return;
        }

        let outcome = controller.connect(&profile(ConnectionMode::Tun), false);
        assert!(matches!(outcome.state, ConnectionState::Failed(_)));
        assert!(controller.process.is_none());
    }

    /// Disconnecting an idle controller is a no-op, not an error.
    #[test]
    fn disconnect_when_idle_is_harmless() {
        let mut controller = ConnectionController::new();
        let outcome = controller.disconnect();
        assert_eq!(outcome.state, ConnectionState::Disconnected);
        assert!(outcome.note.is_none());
    }

    /// Traffic polling must do nothing unless a connection is established.
    #[test]
    fn traffic_poll_is_inert_when_disconnected() {
        let mut controller = ConnectionController::new();
        assert!(controller.poll_traffic().is_none());

        controller.state = ConnectionState::Connecting;
        assert!(controller.poll_traffic().is_none());
    }

    #[test]
    fn core_is_not_alive_without_a_process() {
        let mut controller = ConnectionController::new();
        assert!(!controller.is_core_alive());
    }

    /// A core that dies must not leave the UI claiming success.
    #[test]
    fn noting_a_dead_core_produces_a_failure_state() {
        let mut controller = ConnectionController::new();
        controller.state = ConnectionState::Connected;
        controller.active_mode = Some(ConnectionMode::SystemProxy);

        let outcome = controller.note_core_died();
        assert!(matches!(outcome.state, ConnectionState::Failed(_)));
        assert!(controller.active_mode.is_none());
    }

    #[test]
    fn default_controller_starts_disconnected() {
        let controller = ConnectionController::new();
        assert_eq!(*controller.state(), ConnectionState::Disconnected);
        assert!(controller.active_mode().is_none());
    }
}
