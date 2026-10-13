//! Connection orchestration: builds a config, starts the core, wires up the
//! desktop proxy and keeps the state machine honest.
//!
//! Everything that touches the outside world (process, GSettings, TUN
//! readiness) lives behind [`ConnectionController`], so [`crate::app`] only
//! translates user actions into calls here and renders the resulting state.

use crate::core::config::{generate_xray_config, write_config_to_temp_file};
use crate::core::process::CoreProcessManager;
use crate::core::session::{ConnectionMode, ConnectionProfile, ConnectionState};
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
    fn ok(state: ConnectionState) -> Self {
        Self { state, note: None }
    }

    fn with_note(state: ConnectionState, note: impl Into<String>) -> Self {
        Self {
            state,
            note: Some(note.into()),
        }
    }
}

/// Owns the running core process and the state needed to tear it down cleanly.
pub struct ConnectionController {
    state: ConnectionState,
    process: Option<CoreProcessManager>,
    /// Desktop proxy settings captured before we changed them.
    proxy_snapshot: Option<ProxySnapshot>,
    /// Mode of the connection that is currently up.
    active_mode: Option<ConnectionMode>,
    api_port: u16,
    sampler: TrafficSampler,
    config_path: Option<PathBuf>,
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
            api_port: crate::core::session::DEFAULT_API_PORT,
            sampler: TrafficSampler::new(),
            config_path: None,
        }
    }

    pub fn state(&self) -> &ConnectionState {
        &self.state
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
        self.active_mode = Some(profile.mode);
        self.sampler.reset();

        let mut note = None;

        // System proxy only applies to proxy mode. In TUN mode the device
        // carries all traffic, so touching the desktop proxy there would route
        // packets twice.
        if profile.mode == ConnectionMode::SystemProxy && auto_system_proxy {
            match GnomeProxyManager::snapshot() {
                Ok(snapshot) => {
                    self.proxy_snapshot = Some(snapshot);
                    if let Err(e) = GnomeProxyManager::enable(profile.http_port, profile.socks_port)
                    {
                        // The tunnel itself is up, so this is a partial success:
                        // stay connected but say the desktop proxy did not apply.
                        note = Some(format!("Core running, but system proxy failed: {e}"));
                    }
                }
                Err(e) => {
                    note = Some(format!("Core running, but system proxy failed: {e}"));
                }
            }
        }

        self.state = ConnectionState::Connected;
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
        self.state = ConnectionState::Disconnected;

        if problems.is_empty() {
            TransitionOutcome::ok(self.state.clone())
        } else {
            TransitionOutcome::with_note(self.state.clone(), problems.join("; "))
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

    /// Confirms the core is still running; used by the UI tick.
    pub fn is_core_alive(&mut self) -> bool {
        match self.process.as_mut() {
            Some(process) => process.is_running(),
            None => false,
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
