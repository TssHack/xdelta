# Software Requirements Specification (SRS)
## Project: xdelta (Final Release)
**Document Version:** 1.0.0  
**Status:** Approved Architecture Draft  
**Target Platform:** Linux Desktop (GNOME / Wayland & X11)  
**Implementation Stack:** Rust + GTK4 + Libadwaita  

---

## 1. Introduction

### 1.1 Purpose
This document specifies the software requirements for the production release of **xdelta**, a high-performance, native Linux anti-censorship client. It defines the complete functional and non-functional requirements, architectural blueprints, user interface guidelines, and external interface specifications for developers, designers, and testers.

### 1.2 Scope
**xdelta** is an integrated graphical desktop client designed specifically for Linux, acting as a native frontend and orchestrator for modern anti-censorship cores: **Xray-Core** and **Sing-Box**. 

Key capabilities include:
- Dual-core dynamic engine management (Xray and Sing-Box).
- Comprehensive protocol support: VLESS (Reality, Vision), VMess, Trojan, Shadowsocks 2022, Hysteria 2, and TUIC v5.
- Dual networking modes: GNOME Desktop System Proxy and Layer-3 Virtual TUN interface.
- High-concurrency network latency testing (Async TCP Handshake and Real HTTP probe).
- Intelligent rule-based routing with GeoIP/GeoSite databases (Direct LAN/Domestic traffic, ad-blocking).
- Native GNOME Human Interface Guidelines (HIG) compliance using GTK4 and Libadwaita.
- Real-time traffic analytics, bandwidth monitoring, and isolated process logging.

### 1.3 Definitions, Acronyms, and Abbreviations
- **HIG:** Human Interface Guidelines (GNOME).
- **TUN:** Network TUNnel (Virtual Point-to-Point Layer 3 network device).
- **Reality:** XTLS Reality protocol eliminating server-side TLS certificate fingerprints.
- **IPC:** Inter-Process Communication.
- **GeoIP / GeoSite:** Geolocation and category-based domain/IP database rule sets.
- **SOCKS5 / HTTP Proxy:** Local client proxy endpoints exposed for application traffic.
- **GSettings / D-Bus:** GNOME desktop configuration registry and inter-process bus.

---

## 2. Overall Description

### 2.1 Product Perspective
Linux desktop users have historically relied on cross-platform Electron clients (high memory consumption, non-native appearance, slow startup) or outdated Qt/GTK3 wrappers with unmaintained backends. **xdelta** delivers a 100% native Rust application compiled directly against GTK4 and Libadwaita, maintaining a memory footprint under 40 MB and achieving instant startup times (<200 ms).

```
┌────────────────────────────────────────────────────────────────────────┐
│                        GTK4 / Libadwaita UI                            │
│   [Dashboard]  [Servers]  [Subscriptions]  [Routing]  [Settings] [Logs] │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ glib::MainContext / async-channel
┌───────────────────────────────────▼────────────────────────────────────┐
│                    xdelta Application Engine (Rust)                    │
│  ┌───────────────────────┬──────────────────────┬────────────────────┐ │
│  │ State Management      │ Subscription Parser  │ Multi-thread Ping  │ │
│  │ (Tokio Async Runtime) │ (Base64/Clash/JSON)  │ (TCP & Real HTTP)  │ │
│  └───────────────────────┴──────────────────────┴────────────────────┘ │
│  ┌───────────────────────┬──────────────────────┬────────────────────┐ │
│  │ Config Synthesizer    │ System Proxy Manager │ TUN Controller     │ │
│  │ (Dynamic JSON Memory) │ (GSettings / D-Bus)  │ (ip route / nft)   │ │
│  └───────────────────────┴──────────────────────┴────────────────────┘ │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ Subprocess stdio / Unix Socket
┌───────────────────────────────────▼────────────────────────────────────┐
│                  Proxy Core Subprocess Layer                           │
│           ┌──────────────────────┐    ┌──────────────────────┐         │
│           │      Xray-Core       │ OR │       Sing-Box       │         │
│           │  (vless, vmess, ...) │    │ (hy2, tuic, vless, ..│         │
│           └──────────────────────┘    └──────────────────────┘         │
└────────────────────────────────────────────────────────────────────────┘
```

### 2.2 User Classes and Characteristics
1. **General Desktop Users:** Expects single-click connect/disconnect, automatic desktop proxy switching, fast subscription importing, and clear connection indicators.
2. **Power Users & Developers:** Demands granular routing control, custom inbound/outbound ports, direct terminal routing via TUN, detailed connection logs, and latency diagnostics.
3. **Restricted Network Users:** Requires state-of-the-art obfuscation protocols (VLESS Reality, Hysteria 2 UDP, TUIC) capable of penetrating severe DPI and throttling.

### 2.3 Operating Environment
- **Operating System:** Linux (Kernel 5.15+; fully compatible with CachyOS, Arch Linux, Fedora, Ubuntu 22.04+, Debian 12+).
- **Display Servers:** Wayland (native) and X11.
- **Desktop Environments:** GNOME 44+ (primary target), KDE Plasma, XFCE, Hyprland, Sway.
- **Hardware Architecture:** x86_64 and aarch64.

### 2.4 Design and Implementation Constraints
- Built entirely in **Rust** (2021 Edition) for type safety, zero garbage collection pauses, and low resource overhead.
- Strict adherence to **GNOME HIG** utilizing Libadwaita 1.5+ widgets.
- The UI thread must never be blocked by network operations or filesystem I/O; all async tasks run on a Tokio runtime and report back via Glib channels.
- Avoid requiring the entire GUI application to run as `root`; privileged operations (TUN interface creation) must use standard capabilities (`cap_net_admin`) or Polkit (`pkexec`).

---

## 3. Specific Functional Requirements

### 3.1 Core Process & Lifecycle Engine
- **FR-CORE-01: Auto-detection:** Scan system search paths (`/usr/bin`, `/usr/local/bin`, `~/.local/bin`, and bundled runtime dirs) for `xray` and `sing-box` binaries on startup.
- **FR-CORE-02: Core Switching:** Allow runtime switching between Xray and Sing-Box without application restart.
- **FR-CORE-03: Process Watchdog:** Launch cores as managed child processes. Monitor PID, capture standard output/error asynchronously, and handle unexpected terminations gracefully.
- **FR-CORE-04: Graceful Shutdown:** When terminating a session, send `SIGTERM` followed by a bounded timeout before `SIGKILL`, ensuring all local sockets and system proxy settings are cleanly restored.

### 3.2 Protocol & Node Parser Engine
- **FR-PARSER-01: Universal Protocol Support:**
  - **VLESS:** Flow (`xtls-rprx-vision`), Security (`reality`, `tls`), transport (TCP, WebSocket, gRPC).
  - **VMess:** Encryption types, AlterId, WebSocket with path and custom Host headers, gRPC service names.
  - **Trojan:** SNI, ALPN, TLS fingerprinting.
  - **Shadowsocks:** Traditional ciphers (AEAD) and 2022 standards (`2022-blake3-aes-128-gcm`).
  - **Hysteria 2:** Bandwidth rate allocation, Salamander authentication, port hopping.
  - **TUIC v5:** QUIC congestion control (BBR, Cubic), UDP relay modes.
- **FR-PARSER-02: Format Ingestion:**
  - Universal Link strings (`vless://`, `vmess://`, `trojan://`, `ss://`, `hysteria2://`, `tuic://`).
  - Subscription endpoints delivering Base64 encoded link lists.
  - Clash / Meta YAML configuration profiles.
  - Native Sing-box JSON node profiles.
- **FR-PARSER-03: Node Normalization:** Map all incoming protocol variants into a single, strongly-typed internal Rust data model (`NodeModel`).

### 3.3 Subscription Management
- **FR-SUB-01: Asynchronous Fetching:** Download subscriptions over HTTPS with custom `User-Agent` headers and timeout guarantees.
- **FR-SUB-02: Auto-Update:** Configurable automatic subscription refresh intervals (e.g., hourly, daily, on app launch).
- **FR-SUB-03: Userinfo Extraction:** Read and parse `Subscription-Userinfo` headers (`upload`, `download`, `total`, `expire`) to display remaining quota and expiration date.
- **FR-SUB-04: Node Organization:** Group nodes by subscription source, allow custom renaming, favorite pinning, and batch deletion.

### 3.4 Latency & Network Diagnostics
- **FR-PING-01: Multi-threaded Async Batch Testing:** Execute parallel ping tests across hundreds of nodes using a worker pool.
- **FR-PING-02: Dual Measurement Modes:**
  - **TCP Handshake:** Rapid socket connect test to determine reachability and server RTT.
  - **Real HTTP Handshake:** Measure end-to-end round trip by issuing a lightweight GET request (`generate_204` / Cloudflare trace) through a temporary local outbound.
- **FR-PING-03: Visual Health Indicators:** Color-coded latency tags:
  - Low (< 250 ms): Accent Green.
  - Moderate (250 ms - 500 ms): Accent Yellow/Orange.
  - High (> 500 ms): Accent Red.
  - Timeout / Failed: Neutral Grey.

### 3.5 Smart Routing & DNS Management
- **FR-ROUTE-01: Rule Presets:**
  - **Bypass Iran & Private Networks:** Route `geoip:ir`, `geosite:category-ir`, and RFC1918 IPs directly; route all foreign traffic via proxy.
  - **Global Mode:** Route all traffic through the proxy tunnel.
  - **Direct Mode:** Direct all traffic (bypass proxy entirely).
- **FR-ROUTE-02: Security & Privacy:** Integrated ad-blocking and malware domain sinkholing via `geosite:category-ads-all`.
- **FR-ROUTE-03: Custom Rules:** User-defined list of domain keywords, regexes, and IP CIDRs for explicit Direct, Proxy, or Block routing.
- **FR-ROUTE-04: Leak-proof DNS:** Split DNS configuration resolving domestic domains via local DNS servers (e.g., 10.10.34.35 / 8.8.8.8) and foreign queries over encrypted DNS (DoH/DoT) through the proxy outbound.

### 3.6 System Integration & Proxy Modes
- **FR-SYS-01: GNOME System Proxy:**
  - Connect via GIO / D-Bus to `org.gnome.system.proxy`.
  - On Connect: Set mode to `manual`, configure HTTP proxy (`127.0.0.1:10809`) and SOCKS5 proxy (`127.0.0.1:10808`).
  - On Disconnect: Revert mode to `none` or prior saved system state.
- **FR-SYS-02: Layer 3 TUN Mode:**
  - Initialize a virtual network adapter (e.g., `tun0`) using Sing-box or Xray TUN inbound.
  - Intercept all OS traffic without application-level proxy configuration.
  - Automatic DNS hijacking to ensure system-wide leak prevention.
- **FR-SYS-03: Desktop Notifications:** Native FreeDesktop notifications on connection state changes and subscription update outcomes.
- **FR-SYS-04: System Tray / Status Indicator:** Background daemon mode with status icon, active node label, and quick connect/disconnect menu.

### 3.7 Real-time Monitoring & Statistics
- **FR-STAT-01: Speed & Bandwidth:** Real-time upload and download rate calculated every second via core API/stats counters.
- **FR-STAT-02: Session Totals:** Cumulative transferred bytes for the active session.
- **FR-STAT-03: Live Log Console:** Searchable, color-highlighted log stream with level filtering (Debug, Info, Warning, Error) and one-click log export.

---

## 4. User Interface & UX Specification

### 4.1 Visual Identity and Styling
- **Design Framework:** Follow GNOME HIG 4+ and Libadwaita styling paradigms.
- **Theme Support:** Native dynamic light and dark theme switching via `AdwStyleManager`.
- **Typography:** System default font stack (`Cantarell`), with monospace typography for IP addresses and log viewports (`JetBrains Mono` / system monospace).
- **CSS Architecture:** GTK4-compliant CSS utilizing semantic CSS variables (`var(--window-bg-color)`, `var(--card-bg-color)`, `var(--accent-bg-color)`) and `color-mix()` functions.

### 4.2 Application Navigation Structure
The main window uses an `AdwToolbarView` with an `AdwHeaderBar` and an `AdwViewSwitcher` hosting five dedicated views:

1. **Dashboard View:**
   - **Hero Connection Card:** Prominent status header (Connected / Disconnected / Connecting), active server details, country flag, and ping badge.
   - **Primary Action Button:** Large pill-shaped toggle button with visual feedback.
   - **Bandwidth Cards:** Real-time download rate (`↓ MB/s`) and upload rate (`↑ KB/s`) with session totals.
   - **Quick Mode Selector:** Toggle between System Proxy and TUN Mode.
2. **Servers View:**
   - Header with search entry (`GtkSearchEntry`), protocol filter dropdown, and batch `Test All` button.
   - Server list built with performant `GtkListView` or `AdwActionRow` items displaying protocol badges, server names, address subtitles, and color-coded latency tags.
   - Right-click context menu: Set Active, Test Latency, Edit, Copy Link, Delete.
3. **Subscriptions View:**
   - Management list of active subscriptions with server counts, update timestamps, and quota bars.
   - Floating Action Button or HeaderBar action to add new subscription dialogs.
4. **Routing View:**
   - Preference groups (`AdwPreferencesGroup`) for Bypass Rules, DNS configurations, and Ad-blocking switches.
5. **Settings & Logs View:**
   - Core selection (Xray vs. Sing-Box), port definitions (SOCKS, HTTP), launch at startup, and embedded live log viewer.

---

## 5. Non-Functional Requirements

### 5.1 Performance & Resource Efficiency
- **NFR-PERF-01:** UI launch to interactive state in under 200 ms on average hardware.
- **NFR-PERF-02:** Idle memory consumption must not exceed 40 MB RSS.
- **NFR-PERF-03:** Batch latency testing of 100 servers must complete in under 5 seconds without dropping frames on the GTK main thread (solid 60 fps).

### 5.2 Reliability & Fault Tolerance
- **NFR-REL-01:** If the core process terminates unexpectedly, the UI must detect it within 1 second, revert system proxy settings to prevent network blackouts, and prompt the user.
- **NFR-REL-02:** Clean termination guaranteed: `SIGINT` or `SIGTERM` handlers ensure no orphaned `xray` or `sing-box` processes remain running.

### 5.3 Security & Privacy
- **NFR-SEC-01:** Subscriptions and node configurations are stored locally in user configuration directories (`~/.config/xdelta/`) with `0600` permissions.
- **NFR-SEC-02:** Sensitive credentials (passwords, UUIDs, private keys) are never exposed in log outputs or non-secure temporary files.

---

## 6. Implementation & Packaging Plan

- **Language:** Rust 1.80+ (2021 Edition).
- **Core Crates:** `gtk4`, `libadwaita`, `tokio`, `glib`, `serde`, `serde_json`, `reqwest`, `url`.
- **Packaging Targets:**
  - Native Arch Linux PKGBUILD (AUR).
  - Flatpak (Flathub standard packaging with D-Bus proxy permissions).
  - AppImage / Standalone binary tarball.
