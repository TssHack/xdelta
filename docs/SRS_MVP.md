# Software Requirements Specification (SRS)
## Project: xdelta (Version 1.0 — MVP)
**Document Version:** 1.0.0-MVP  
**Status:** Approved Implementation Blueprint  
**Target Platform:** Linux Desktop (GNOME / Wayland & X11)  
**Implementation Stack:** Rust + GTK4 + Libadwaita  

---

## 1. Executive Summary & Vision

The objective of **xdelta v1.0 (MVP)** is to deliver a rock-solid, visually stunning, and blazing-fast native Linux proxy client that users can launch and connect to in seconds. 

Instead of getting bogged down by high-friction administrative features (like complex TUN routing or dual-core abstraction in the initial build), the MVP zeroes in on the **core value proposition**:
1. Effortless subscription and node importing.
2. Fast multi-threaded latency testing.
3. One-click reliable connection powered by **Xray-Core**.
4. Automatic GNOME System Proxy toggling.
5. A lightweight, beautiful **Libadwaita** interface adhering strictly to GNOME Human Interface Guidelines.

---

## 2. Scope & Target Capabilities

```
┌────────────────────────────────────────────────────────────────────────┐
│                          xdelta v1.0 (MVP)                             │
├────────────────────────────────────────────────────────────────────────┤
│  [✓] VLESS (Reality & TLS), VMess (WS & TCP), Trojan Link Parser       │
│  [✓] Base64 Subscription Downloader with Async HTTP Client             │
│  [✓] Multi-threaded TCP Handshake Ping Engine                          │
│  [✓] Single-Click Connect / Disconnect with Hero Status Widget         │
│  [✓] Dynamic Xray JSON Config Synthesizer (SOCKS: 10808, HTTP: 10809)  │
│  [✓] Automated GNOME Desktop Proxy Configuration via GSettings D-Bus   │
│  [✓] Clean Libadwaita UI with Dark/Light Support and Responsive Layout │
│  [✓] Clean Process Management with Guaranteed Teardown on Exit         │
└────────────────────────────────────────────────────────────────────────┘
```

### 2.1 Explicit Non-Goals for MVP (Deferred to v1.1+)
- **Virtual TUN Interface Mode:** Requires `root` or `CAP_NET_ADMIN` setup; deferred to next iteration.
- **Sing-Box Core Integration:** MVP focuses exclusively on **Xray-Core** for stability and speed.
- **Complex Rule Graph Editor:** Basic routing (Bypass Private IPs + Proxy Foreign) is hardwired for MVP.
- **Clash YAML Provider Parser:** Standard Base64 subscription format and direct URIs are prioritized.

---

## 3. Functional Requirements

### 3.1 Subscription & Node Management
- **FR-MVP-01: Subscription Import:** Users can input an HTTP/HTTPS subscription URL. The app fetches the payload asynchronously using `reqwest` and parses the Base64-encoded server list.
- **FR-MVP-02: Direct Link Import:** Users can paste raw configuration strings (`vless://`, `vmess://`, `trojan://`) directly from the clipboard.
- **FR-MVP-03: Protocol Ingestion:**
  - **VLESS:** Support standard TLS and XTLS Reality (`pbk`, `sid`, `sni`, `flow=xtls-rprx-vision`).
  - **VMess:** Support standard TCP and WebSocket (`path`, `host`).
  - **Trojan:** Support standard TLS with custom SNI.
- **FR-MVP-04: Local Storage:** Persist subscriptions and parsed nodes in a local JSON database under `~/.config/xdelta/nodes.json`.

### 3.2 Latency Testing Engine
- **FR-MVP-05: High-Concurrency Ping:** Perform concurrent TCP connection handshakes to server addresses and ports using a Tokio async pool.
- **FR-MVP-06: Batch Ping:** A single "Test All" button triggers parallel ping checks across all servers in the list.
- **FR-MVP-07: Visual Latency Status:**
  - `< 300 ms`: Green badge (`var(--accent-green)`).
  - `300 - 600 ms`: Amber badge (`var(--accent-yellow)`).
  - `> 600 ms` or Timeout: Red/Grey badge.

### 3.3 Core Management & Connectivity
- **FR-MVP-08: Config Synthesizer:** Dynamically assemble a valid Xray JSON configuration file in `/tmp/xdelta_active.json` containing:
  - Inbound 1: SOCKS5 on `127.0.0.1:10808`.
  - Inbound 2: HTTP Proxy on `127.0.0.1:10809`.
  - Outbound: Active selected node configuration.
  - Basic Routing: Bypass LAN / RFC1918 addresses; direct proxy for all other destinations.
- **FR-MVP-09: Process Supervisor:**
  - Launch `/usr/bin/xray` (or user-configured path) as a non-blocking child process.
  - Monitor exit status and pipe logs to internal ring buffer.
  - On Disconnect: Send `SIGTERM` and ensure process is terminated before resetting state.
- **FR-MVP-10: GNOME System Proxy Integration:**
  - On Connect: Update GSettings keys under `org.gnome.system.proxy`:
    - `mode` -> `'manual'`
    - `http/host` -> `'127.0.0.1'`, `http/port` -> `10809`
    - `socks/host` -> `'127.0.0.1'`, `socks/port` -> `10808`
  - On Disconnect / Application Exit: Revert `mode` -> `'none'`.

---

## 4. User Interface Specification

The user interface follows GNOME Human Interface Guidelines (HIG) with Libadwaita components.

```
┌────────────────────────────────────────────────────────────────────────┐
│  xdelta                  [ ⛯ Dashboard | ⛨ Servers | ⤓ Subscriptions ]  ⋮ │
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│                        DASHBOARD (ACTIVE VIEW)                         │
│                                                                        │
│   ┌────────────────────────────────────────────────────────────────┐   │
│   │                     STATUS HERO CARD                           │   │
│   │                                                                │   │
│   │                       CONNECTED                                │   │
│   │              Active Server: Frankfurt Highspeed 01             │   │
│   │               Protocol: VLESS-Reality  •  142 ms               │   │
│   │                                                                │   │
│   │                  [   DISCONNECT BUTTON   ]                     │   │
│   └────────────────────────────────────────────────────────────────┘   │
│                                                                        │
│   ┌──────────────────────────────┐  ┌──────────────────────────────┐   │
│   │         LOCAL SOCKS5         │  │          LOCAL HTTP          │   │
│   │        127.0.0.1:10808       │  │        127.0.0.1:10809       │   │
│   └──────────────────────────────┘  └──────────────────────────────┘   │
│                                                                        │
│   [✓] Auto GNOME System Proxy Enabled                                  │
│                                                                        │
└────────────────────────────────────────────────────────────────────────┘
```

### 4.1 Views Breakdown
1. **Dashboard Tab (`AdwViewStack` Page 1):**
   - **Hero Connection Card:** Elevated card featuring large connection status (Connected / Disconnected), active server title, and protocol badge.
   - **Main Action Button:** Large pill button (`suggested-action` when disconnected, `destructive-action` when connected).
   - **Endpoint Details:** Display local SOCKS and HTTP listener ports.
   - **System Proxy Switch:** Toggle to enable or disable automatic GNOME proxy configuration.
2. **Servers Tab (`AdwViewStack` Page 2):**
   - Search entry (`GtkSearchEntry`) with real-time text filter.
   - "Test All" button in the action bar.
   - List of servers rendered as interactive `AdwActionRow` widgets displaying:
     - Title: Node name.
     - Subtitle: Remote host and port.
     - Prefix: Protocol pill badge (e.g., `VLESS`, `VMESS`, `TROJAN`).
     - Suffix: Latency badge (ms) and selection radio/check button.
3. **Subscriptions Tab (`AdwViewStack` Page 3):**
   - Input row for new subscription URL with "Add & Update" action.
   - List of configured subscriptions showing total nodes, update time, and delete button.

---

## 5. Technical Architecture & Technology Stack

| Component | Choice | Justification |
| :--- | :--- | :--- |
| **Language** | **Rust 1.80+ (2021 Edition)** | Zero runtime overhead, fearless concurrency, memory safety |
| **UI Framework** | **GTK4 (gtk4 crate 0.9+)** | Native Linux GUI standard |
| **Widgets & Theming** | **libadwaita (libadwaita crate 0.7+)** | Official GNOME HIG look-and-feel, seamless dark/light mode |
| **Async Runtime** | **Tokio 1.x** | Multi-threaded networking, socket pinging, timers |
| **Inter-thread Comms** | **glib::MainContext / async-channel** | Safe communication between Tokio worker threads and GTK main thread |
| **HTTP Client** | **reqwest 0.12+ (rustls-tls)** | Fast async HTTP client with no OpenSSL dependency |
| **Data Serialization** | **serde, serde_json, base64, url** | Fast URI parsing and Xray config synthesis |
| **System Settings** | **gio::Settings** | Native GNOME GSettings manipulation over D-Bus |

---

## 6. Project Layout

```text
xdelta/
├── Cargo.toml
├── build.rs
├── data/
│   ├── resources/
│   │   ├── style.css
│   │   └── icons/
│   └── io.github.xdelta.desktop
└── src/
    ├── main.rs                  # Application entry point & Libadwaita init
    ├── app.rs                   # AdwApplication lifecycle
    ├── core/
    │   ├── mod.rs
    │   ├── config.rs            # Xray JSON synthesis
    │   └── process.rs           # Child process lifecycle (start, stop, kill)
    ├── model/
    │   ├── mod.rs
    │   ├── node.rs              # Protocol definitions & node struct
    │   └── subscription.rs      # Subscription entity
    ├── parser/
    │   ├── mod.rs
    │   ├── uri.rs               # vless://, vmess://, trojan:// parser
    │   └── subscription.rs      # Base64 decoder & subscriber fetcher
    ├── net/
    │   ├── mod.rs
    │   ├── ping.rs              # Concurrent async TCP handshake ping
    │   └── sysproxy.rs          # GNOME GSettings proxy controller
    └── ui/
        ├── mod.rs
        ├── window.rs            # Main AdwApplicationWindow
        ├── dashboard.rs         # Hero card, connect button, port status
        ├── servers.rs           # Server list, search entry, ping triggers
        └── subscriptions.rs     # Subscription manager view
```

---

## 7. Acceptance Criteria (Definition of Done for MVP)

1. **Compilation:** The project compiles cleanly via `cargo build --release` with zero warnings on modern Linux.
2. **Visual Fidelity:** The application launches in a native Libadwaita window matching GNOME HIG, respecting the system dark/light theme.
3. **Importing:** Successfully imports a standard Base64 subscription containing VLESS Reality, VMess, and Trojan links.
4. **Latency Measurement:** Clicking "Test All" completes in < 3 seconds across 50+ nodes and updates latency badges without freezing the UI.
5. **Connection & Traffic:**
   - Clicking "Connect" generates a valid Xray config and launches the process.
   - GNOME System Proxy is automatically enabled.
   - Browsing the web routes traffic through the selected server.
   - Clicking "Disconnect" cleanly terminates the process and restores system proxy settings to "none".
