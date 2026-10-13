#!/bin/sh
set -e

# Portable user-space installer for xdelta
PREFIX="${HOME}/.local"
BIN_DIR="${PREFIX}/bin"
APP_DIR="${PREFIX}/share/applications"
ICON_DIR="${PREFIX}/share/icons/hicolor"

echo "==> Installing xdelta to ${PREFIX}..."

mkdir -p "${BIN_DIR}" "${APP_DIR}" "${ICON_DIR}"

install -m 755 xdelta "${BIN_DIR}/xdelta"
install -m 644 xdelta.desktop "${APP_DIR}/xdelta.desktop"

if [ -d "assets/icons/hicolor" ]; then
    cp -rn assets/icons/hicolor/* "${ICON_DIR}/" || true
fi

# Update desktop database if available
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "${PREFIX}/share/applications" || true
fi

# Update icon cache if available
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "${ICON_DIR}" || true
fi

echo "==> Successfully installed xdelta!"
echo "    Executable: ${BIN_DIR}/xdelta"
echo "    Make sure ${BIN_DIR} is in your PATH."
