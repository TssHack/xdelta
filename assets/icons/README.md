# 🎨 xdelta Icon Suite & Assets Guide

Comprehensive icon suite for **xdelta** designed for Linux (GNOME/Libadwaita), Freedesktop standards, Web, and cross-platform packaging.

---

## 📁 Directory Structure

```
assets/
├── xdelta.desktop                           # Standard Linux desktop launcher entry
└── icons/
    ├── xdelta.svg                           # Master Option 1: Standalone Adwaita 3D Emblem (UI / Header / Brand)
    ├── xdelta-app.svg                       # Master Option 3: Squircle Desktop App Icon (.desktop / App launcher)
    ├── xdelta-symbolic.svg                  # Master Option 4: Monochrome GNOME Symbolic Icon (Status tray / Top bar)
    ├── favicon.ico / xdelta.ico             # Multi-size ICO (16, 24, 32, 48, 64, 128, 256 px)
    │
    ├── hicolor/                             # Standard Freedesktop Icon Theme Directory
    │   ├── scalable/apps/
    │   │   ├── xdelta.svg
    │   │   └── xdelta-app.svg
    │   ├── symbolic/apps/
    │   │   ├── xdelta-symbolic.svg
    │   │   ├── xdelta-connected-symbolic.svg
    │   │   └── xdelta-disconnected-symbolic.svg
    │   ├── 16x16/apps/
    │   ├── 24x24/apps/
    │   ├── 32x32/apps/
    │   ├── 48x48/apps/
    │   ├── 64x64/apps/
    │   ├── 128x128/apps/
    │   ├── 256x256/apps/
    │   ├── 512x512/apps/
    │   └── 1024x1024/apps/
    │
    └── png/                                 # Quick-access flat PNG renders
        ├── app/                             # xdelta-app-NxN.png (16 to 1024 px)
        ├── emblem/                          # xdelta-NxN.png (16 to 1024 px)
        └── symbolic/                        # xdelta-symbolic, connected, disconnected (16 to 64 px)
```

---

## 🛠️ Usage by Component

| Component | Recommended Asset | Purpose |
| :--- | :--- | :--- |
| **Linux Desktop Launcher (`.desktop`)** | `hicolor/*/apps/xdelta-app.png` or `scalable/apps/xdelta-app.svg` | App icon in GNOME Dash, Flathub, AppStream |
| **In-App Sidebar & Header** | `xdelta.svg` or `png/emblem/xdelta-48x48.png` | Native UI branding alongside app title |
| **GNOME Top-Bar & System Tray** | `xdelta-symbolic.svg` (or connected/disconnected) | AppIndicator status icon (supports automatic theme tinting) |
| **Documentation & GitHub Readme** | `png/emblem/xdelta-256x256.png` or `xdelta.svg` | Hero branding |
| **Web & Favicon** | `favicon.ico` | Browser tab icon |

---

## 📦 Packaging Installation Snippet (PKGBUILD / Makefile / Meson)

```bash
# Install scalable SVGs
install -Dm644 assets/icons/hicolor/scalable/apps/xdelta-app.svg "$pkgdir/usr/share/icons/hicolor/scalable/apps/xdelta-app.svg"
install -Dm644 assets/icons/hicolor/scalable/apps/xdelta.svg "$pkgdir/usr/share/icons/hicolor/scalable/apps/xdelta.svg"
install -Dm644 assets/icons/hicolor/symbolic/apps/xdelta-symbolic.svg "$pkgdir/usr/share/icons/hicolor/symbolic/apps/xdelta-symbolic.svg"

# Install PNG bitmaps for all standard resolutions
for size in 16 24 32 48 64 128 256 512; do
  install -Dm644 "assets/icons/hicolor/${size}x${size}/apps/xdelta-app.png" "$pkgdir/usr/share/icons/hicolor/${size}x${size}/apps/xdelta-app.png"
done

# Install Desktop Entry
install -Dm644 assets/xdelta.desktop "$pkgdir/usr/share/applications/xdelta.desktop"
```
