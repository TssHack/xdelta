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

const PNG_CONNECTED: &[u8] =
    include_bytes!("../../assets/icons/png/symbolic/xdelta-connected-32x32.png");
const PNG_DISCONNECTED: &[u8] =
    include_bytes!("../../assets/icons/png/symbolic/xdelta-disconnected-32x32.png");
const PNG_DEFAULT: &[u8] = include_bytes!("../../assets/icons/png/app/xdelta-app-32x32.png");

fn decode_png_to_sni_icon(png_bytes: &[u8]) -> Option<ksni::Icon> {
    use gtk4::gdk_pixbuf::Pixbuf;
    use gtk4::gio::MemoryInputStream;
    use gtk4::glib::Bytes;

    let bytes = Bytes::from(png_bytes);
    let stream = MemoryInputStream::from_bytes(&bytes);
    let pixbuf = Pixbuf::from_stream(&stream, None::<&gtk4::gio::Cancellable>).ok()?;
    let width = pixbuf.width();
    let height = pixbuf.height();
    let n_channels = pixbuf.n_channels();
    let rowstride = pixbuf.rowstride() as usize;
    let pixels = unsafe { pixbuf.pixels() };

    let mut argb_data = Vec::with_capacity((width * height * 4) as usize);

    for y in 0..height as usize {
        let row_start = y * rowstride;
        for x in 0..width as usize {
            let offset = row_start + x * (n_channels as usize);
            let r = pixels[offset];
            let g = pixels[offset + 1];
            let b = pixels[offset + 2];
            let a = if n_channels == 4 {
                pixels[offset + 3]
            } else {
                255
            };

            // StatusNotifierItem expects ARGB32 (Big Endian: A, R, G, B)
            argb_data.push(a);
            argb_data.push(r);
            argb_data.push(g);
            argb_data.push(b);
        }
    }

    Some(ksni::Icon {
        width,
        height,
        data: argb_data,
    })
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

    fn icon_theme_path(&self) -> String {
        for candidate in [
            "/usr/share/icons/hicolor",
            "/usr/local/share/icons/hicolor",
            "assets/icons/hicolor",
        ] {
            if std::path::Path::new(candidate).exists() {
                return candidate.to_string();
            }
        }
        if let Some(data_dir) = crate::core::persist::data_dir() {
            let p = data_dir.join("icons/hicolor");
            if p.exists() {
                return p.to_string_lossy().to_string();
            }
        }
        String::new()
    }

    fn icon_name(&self) -> String {
        match self.state {
            ConnectionState::Connected => "network-vpn-symbolic".into(),
            ConnectionState::Connecting | ConnectionState::Disconnecting => {
                "network-wireless-acquiring-symbolic".into()
            }
            ConnectionState::Failed(_) => "network-error-symbolic".into(),
            ConnectionState::Disconnected => "network-offline-symbolic".into(),
        }
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        let bytes = match self.state {
            ConnectionState::Connected => PNG_CONNECTED,
            ConnectionState::Connecting | ConnectionState::Disconnecting => PNG_CONNECTED,
            ConnectionState::Failed(_) => PNG_DISCONNECTED,
            ConnectionState::Disconnected => PNG_DEFAULT,
        };
        decode_png_to_sni_icon(bytes).into_iter().collect()
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

        let tip_icon_name = match self.state {
            ConnectionState::Connected => "network-vpn-symbolic".to_string(),
            ConnectionState::Connecting | ConnectionState::Disconnecting => {
                "network-wireless-acquiring-symbolic".to_string()
            }
            ConnectionState::Failed(_) => "network-error-symbolic".to_string(),
            ConnectionState::Disconnected => "network-offline-symbolic".to_string(),
        };

        ToolTip {
            title,
            description,
            icon_name: tip_icon_name,
            icon_pixmap: self.icon_pixmap(),
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
