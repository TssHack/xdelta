//! FreeDesktop StatusNotifierItem (AppIndicator / System Tray) integration.
//!
//! Communicates over D-Bus with KDE Plasma, GNOME Shell (via AppIndicator
//! extension), Waybar, XFCE panel, and any desktop environment implementing
//! the StatusNotifierItem specification.

use crate::core::session::ConnectionState;
use ksni::menu::{MenuItem, StandardItem};
use ksni::{Handle, Status, ToolTip, Tray, TrayMethods};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub type TrayHandle = Handle<XdeltaTray>;

/// Events originating from tray user interactions, dispatched to GTK's main thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    /// User clicked the tray icon or "Show / Hide Window" menu item.
    ToggleWindow,
    /// User selected "Connect" or "Disconnect".
    ToggleConnection,
    /// User selected "Quit xdelta".
    Quit,
}

/// System tray item state and menu model.
pub struct XdeltaTray {
    sender: async_channel::Sender<TrayEvent>,
    state: ConnectionState,
    server_name: Option<String>,
    latency_ms: Option<u32>,
    is_window_visible: Arc<AtomicBool>,
}

impl XdeltaTray {
    pub fn new(
        sender: async_channel::Sender<TrayEvent>,
        is_window_visible: Arc<AtomicBool>,
    ) -> Self {
        Self {
            sender,
            state: ConnectionState::Disconnected,
            server_name: None,
            latency_ms: None,
            is_window_visible,
        }
    }

    /// Spawns the D-Bus StatusNotifierItem service.
    pub async fn spawn_service(
        sender: async_channel::Sender<TrayEvent>,
        is_window_visible: Arc<AtomicBool>,
    ) -> Result<TrayHandle, ksni::Error> {
        let tray = Self::new(sender, is_window_visible);
        let handle = tray.spawn().await?;
        log::info!("StatusNotifierItem (AppIndicator) registered on D-Bus");
        Ok(handle)
    }

    /// Updates the internal connection state and parameters.
    pub fn update_connection(
        &mut self,
        state: ConnectionState,
        server_name: Option<String>,
        latency_ms: Option<u32>,
    ) {
        self.state = state;
        self.server_name = server_name;
        self.latency_ms = latency_ms;
    }
}

impl Tray for XdeltaTray {
    fn id(&self) -> String {
        "xdelta".into()
    }

    fn title(&self) -> String {
        "xdelta".into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::ApplicationStatus
    }

    fn status(&self) -> Status {
        Status::Active
    }

    fn icon_name(&self) -> String {
        match self.state {
            ConnectionState::Connected => "xdelta-connected-symbolic".into(),
            ConnectionState::Connecting | ConnectionState::Disconnecting => {
                "xdelta-symbolic".into()
            }
            ConnectionState::Failed(_) => "xdelta-disconnected-symbolic".into(),
            ConnectionState::Disconnected => "xdelta-app".into(),
        }
    }

    fn tool_tip(&self) -> ToolTip {
        let (title, description) = match self.state {
            ConnectionState::Connected => {
                let s_name = self.server_name.as_deref().unwrap_or("Active Server");
                let lat = self
                    .latency_ms
                    .map(|l| format!(" • {l} ms"))
                    .unwrap_or_default();
                ("xdelta: Connected".into(), format!("{s_name}{lat}"))
            }
            ConnectionState::Connecting => (
                "xdelta: Connecting...".into(),
                "Establishing secure proxy tunnel".into(),
            ),
            ConnectionState::Disconnecting => (
                "xdelta: Disconnecting...".into(),
                "Shutting down proxy tunnel".into(),
            ),
            ConnectionState::Failed(_) => (
                "xdelta: Connection Failed".into(),
                "Core process exited or error occurred".into(),
            ),
            ConnectionState::Disconnected => {
                ("xdelta: Disconnected".into(), "Proxy is inactive".into())
            }
        };

        ToolTip {
            title,
            description,
            icon_name: self.icon_name(),
            icon_pixmap: Vec::new(),
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.sender.try_send(TrayEvent::ToggleWindow);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let mut items = Vec::new();

        // 1. Connection Status Banner Item (disabled header)
        let status_text = match self.state {
            ConnectionState::Connected => {
                let s_name = self.server_name.as_deref().unwrap_or("Connected");
                let lat = self
                    .latency_ms
                    .map(|l| format!(" ({l} ms)"))
                    .unwrap_or_default();
                format!("● {s_name}{lat}")
            }
            ConnectionState::Connecting => "○ Connecting...".into(),
            ConnectionState::Disconnecting => "○ Disconnecting...".into(),
            ConnectionState::Failed(_) => "✕ Connection Failed".into(),
            ConnectionState::Disconnected => "● Disconnected".into(),
        };

        items.push(
            StandardItem {
                label: status_text,
                enabled: false,
                ..Default::default()
            }
            .into(),
        );

        items.push(MenuItem::Separator);

        // 2. Connect / Disconnect Action
        let (action_label, action_icon) = match self.state {
            ConnectionState::Connected => ("Disconnect", "media-playback-stop-symbolic"),
            ConnectionState::Connecting => ("Cancel Connection", "process-stop-symbolic"),
            ConnectionState::Disconnecting => ("Disconnecting...", "process-working-symbolic"),
            ConnectionState::Disconnected | ConnectionState::Failed(_) => {
                ("Connect", "media-playback-start-symbolic")
            }
        };

        let is_trans = self.state.is_transitioning();
        let sender_toggle = self.sender.clone();
        items.push(
            StandardItem {
                label: action_label.into(),
                icon_name: action_icon.into(),
                enabled: !is_trans,
                activate: Box::new(move |_| {
                    let _ = sender_toggle.try_send(TrayEvent::ToggleConnection);
                }),
                ..Default::default()
            }
            .into(),
        );

        items.push(MenuItem::Separator);

        // 3. Toggle / Show Window
        let win_visible = self.is_window_visible.load(Ordering::Relaxed);
        let win_label = if win_visible {
            "Hide Window"
        } else {
            "Show Window"
        };
        let sender_win = self.sender.clone();
        items.push(
            StandardItem {
                label: win_label.into(),
                icon_name: "window-maximize-symbolic".into(),
                activate: Box::new(move |_| {
                    let _ = sender_win.try_send(TrayEvent::ToggleWindow);
                }),
                ..Default::default()
            }
            .into(),
        );

        items.push(MenuItem::Separator);

        // 4. Quit Application
        let sender_quit = self.sender.clone();
        items.push(
            StandardItem {
                label: "Quit xdelta".into(),
                icon_name: "application-exit-symbolic".into(),
                activate: Box::new(move |_| {
                    let _ = sender_quit.try_send(TrayEvent::Quit);
                }),
                ..Default::default()
            }
            .into(),
        );

        items
    }
}
