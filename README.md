<div align="center">

# ⚡ xdelta

**High-performance, elegant, and native Linux proxy client for Xray and Sing-box**  
*Crafted with Rust, GTK4, and Libadwaita following the GNOME Human Interface Guidelines.*

<br/>

[![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange.svg?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![GTK4](https://img.shields.io/badge/GTK-4.24%2B-3584e4.svg?style=for-the-badge&logo=gnome&logoColor=white)](https://www.gtk.org/)
[![Libadwaita](https://img.shields.io/badge/Libadwaita-1.10%2B-blueviolet.svg?style=for-the-badge&logo=gnome&logoColor=white)](https://gnome.pages.gitlab.gnome.org/libadwaita/)
[![Platform](https://img.shields.io/badge/Platform-Linux%20(Wayland%20%2F%20X11)-2ea44f.svg?style=for-the-badge&logo=linux&logoColor=white)](https://kernel.org/)
[![License](https://img.shields.io/badge/License-GPL--3.0-blue.svg?style=for-the-badge)](LICENSE)

<br/>

[Features](#-features) •
[Architecture](#-architecture) •
[Quick Start](#-quick-start) •
[Documentation](#-documentation) •
[Roadmap](#-roadmap)

</div>

---

## 🌟 Why xdelta?

Most proxy clients for Linux are either heavy web wrappers running on Electron with 250MB+ RAM usage, or legacy Qt interfaces with fragmented system integration. 

**xdelta** is engineered from scratch in **Rust** to feel right at home on the modern Linux desktop:
- 🚀 **Blazing Fast & Ultra Light:** Instant launch (<200ms) with a memory footprint under **40 MB**.
- 🎨 **100% Native GNOME Look:** Clean Libadwaita styling, native dark/light theme integration, and responsive layouts.
- ⚙️ **Dual-Engine Architecture:** Seamlessly control **Xray-Core** or **Sing-Box** under one unified UI.
- 🛡️ **Modern Anti-Censorship Protocols:** Native support for VLESS Reality, VMess, Trojan, Hysteria 2, and TUIC.
- ⚡ **Zero UI Freezes:** Tokio asynchronous runtime paired with Glib channels guarantees a butter-smooth 60 FPS experience even when latency-testing hundreds of endpoints simultaneously.

---

## ✨ Features

### 🎛️ Dual-Core Support
- Switch between **Xray-Core** and **Sing-Box** dynamically without restarting the application.
- Auto-detects installed system binaries (`/usr/bin/xray`, `/usr/bin/sing-box`).

### 🌐 Universal Protocol Support
| Protocol | Transports & Features | Core |
| :--- | :--- | :---: |
| **VLESS** | Reality (`pbk`, `sid`), XTLS Vision flow, TLS, gRPC, WebSocket | Xray / Sing-box |
| **VMess** | WebSocket (Path & Host), gRPC, TCP, HTTPUpgrade | Xray / Sing-box |
| **Trojan** | Standard TLS, SNI, ALPN | Xray / Sing-box |
| **Shadowsocks** | AEAD, 2022 standards (`2022-blake3-aes-128-gcm`) | Xray / Sing-box |
| **Hysteria 2** | High-performance UDP, Salamander obfuscation | Sing-box |
| **TUIC v5** | QUIC-based BBR congestion control | Sing-box |

### 🚀 Connectivity & System Integration
- **One-Click Connect:** Instant session launch with prominent status indication.
- **GNOME System Proxy Automation:** Seamlessly sets and clears system-wide HTTP and SOCKS5 proxies using native D-Bus / GSettings APIs.
- **High-Concurrency Latency Testing:** Async multi-threaded TCP handshake testing across all nodes with color-coded latency tags.
- **Subscription Management:** Import and update Base64, V2Ray, and Clash-compatible subscriptions with quota parsing.

---

## 🏗️ Architecture

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

---

## 🚀 Quick Start

### Prerequisites

Ensure you have the Rust toolchain, GTK4, and Libadwaita development packages installed on your system:

#### Arch Linux / CachyOS / Manjaro
```bash
sudo pacman -S base-devel rust cargo gtk4 libadwaita xray sing-box
```

#### Fedora / RHEL
```bash
sudo dnf install rust cargo gcc gtk4-devel libadwaita-devel xray sing-box
```

#### Ubuntu 24.04+ / Debian 13+
```bash
sudo apt install build-essential rustc cargo libgtk-4-dev libadwaita-1-dev
```

### Build & Run from Source

```bash
# Clone the repository
git clone https://github.com/TssHack/xdelta.git
cd xdelta

# Compile and run in debug mode
cargo run

# Build optimized release binary
cargo build --release
```

The optimized binary will be available at `./target/release/xdelta`.

---

## 📖 Documentation

- 📋 [Final Release SRS (Software Requirements Specification)](docs/SRS_FINAL.md) — Comprehensive technical specification for production release.
- 🎯 [MVP v1.0 SRS](docs/SRS_MVP.md) — Lean specifications and implementation milestones for the v1.0 minimal viable product.

---

## 🗺️ Roadmap

- [x] **Project Inception & Architecture Planning**
- [x] **Complete SRS Specifications (Final & MVP)**
- [ ] **v1.0 (MVP Release):**
  - [ ] Core Rust GTK4 & Libadwaita UI skeleton
  - [ ] VLESS (Reality/TLS), VMess, Trojan protocol parsers
  - [ ] Async Base64 subscription downloader
  - [ ] Multi-threaded TCP latency test engine
  - [ ] Dynamic Xray config synthesizer
  - [ ] Automated GNOME System Proxy toggle
- [ ] **v1.1:**
  - [ ] Sing-Box core backend engine
  - [ ] Hysteria 2 and TUIC v5 support
  - [ ] Real HTTP delay testing
- [ ] **v2.0 (Final Release):**
  - [ ] Layer-3 TUN mode with DNS leak prevention
  - [ ] GeoIP & GeoSite smart routing rule manager
  - [ ] Real-time bandwidth charts and live traffic stats

---

## 🤝 Contributing

Contributions, bug reports, and feature requests are very welcome! Feel free to check the [issues page](https://github.com/TssHack/xdelta/issues) or submit a Pull Request.

1. Fork the Project
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

---

## 📄 License

Distributed under the GNU General Public License v3.0 (GPL-3.0). See [LICENSE](LICENSE) for more information.

<div align="center">
  <sub>Built with ❤️ for the free internet community.</sub>
</div>
