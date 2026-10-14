#!/usr/bin/env bash
set -euo pipefail

# ==============================================================================
# xdelta Unified Linux Packaging Script
# Outputs:
#   - Standalone portable archive (.tar.gz)
#   - Debian/Ubuntu package (.deb)
#   - Fedora/RHEL package (.rpm)
#   - Arch Linux package (.pkg.tar.zst)
#   - Universal AppImage (.AppImage, if appimagetool is present)
# ==============================================================================

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BINARY_PATH="${PROJECT_ROOT}/target/release/xdelta"
ARCH="x86_64"
VERSION=""
DIST_DIR="${PROJECT_ROOT}/dist"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --binary)
            BINARY_PATH="$2"
            shift 2
            ;;
        --arch)
            ARCH="$2"
            shift 2
            ;;
        --version)
            VERSION="$2"
            shift 2
            ;;
        --dist)
            DIST_DIR="$2"
            shift 2
            ;;
        *)
            echo "Unknown argument: $1"
            exit 1
            ;;
    esac
done

mkdir -p "${DIST_DIR}"
DIST_DIR="$(cd "${DIST_DIR}" && pwd)"

if [[ -f "${BINARY_PATH}" ]]; then
    BINARY_PATH="$(cd "$(dirname "${BINARY_PATH}")" && pwd)/$(basename "${BINARY_PATH}")"
fi

if [[ -z "${VERSION}" ]]; then
    VERSION=$(grep -m1 '^version = ' "${PROJECT_ROOT}/Cargo.toml" | cut -d '"' -f2)
fi

# Map architecture names
case "${ARCH}" in
    x86_64|amd64)
        ARCH="x86_64"
        DEB_ARCH="amd64"
        RPM_ARCH="x86_64"
        ;;
    aarch64|arm64)
        ARCH="aarch64"
        DEB_ARCH="arm64"
        RPM_ARCH="aarch64"
        ;;
    *)
        echo "Unsupported architecture: ${ARCH}"
        exit 1
        ;;
esac

echo "=========================================================="
echo " Packaging xdelta v${VERSION} for ${ARCH} (${DEB_ARCH}/${RPM_ARCH})"
echo " Binary: ${BINARY_PATH}"
echo " Output: ${DIST_DIR}"
echo "=========================================================="

mkdir -p "${DIST_DIR}"

if [[ ! -f "${BINARY_PATH}" ]]; then
    echo "Error: Binary not found at ${BINARY_PATH}"
    exit 1
fi

TMP_STAGING=$(mktemp -d "/tmp/xdelta_pkg_XXXXXX")
trap 'rm -rf "${TMP_STAGING}"' EXIT

# ------------------------------------------------------------------------------
# 0. Fetch & Stage Xray Core & Assets for Out-of-the-Box Bundling
# ------------------------------------------------------------------------------
XRAY_STAGING="${TMP_STAGING}/xray_bin"
mkdir -p "${XRAY_STAGING}"

XRAY_ZIP_URL=""
if [[ "${ARCH}" == "x86_64" ]]; then
    XRAY_ZIP_URL="https://github.com/XTLS/Xray-core/releases/latest/download/Xray-linux-64.zip"
elif [[ "${ARCH}" == "aarch64" ]]; then
    XRAY_ZIP_URL="https://github.com/XTLS/Xray-core/releases/latest/download/Xray-linux-arm64-v8a.zip"
fi

XRAY_FOUND=false
CACHE_ZIP="${PROJECT_ROOT}/target/Xray-linux-${ARCH}.zip"

# 1. Check if cached zip exists
if [[ -f "${CACHE_ZIP}" ]]; then
    echo "==> Using cached Xray archive: ${CACHE_ZIP}"
    if unzip -q -o "${CACHE_ZIP}" -d "${XRAY_STAGING}" 2>/dev/null; then
        XRAY_FOUND=true
    fi
fi

# 2. Try fetching from official GitHub Releases
if [[ "${XRAY_FOUND}" != "true" ]] && [[ -n "${XRAY_ZIP_URL}" ]]; then
    echo "==> Fetching official Xray-core release for ${ARCH}..."
    if curl -fsSL -o "${TMP_STAGING}/xray.zip" "${XRAY_ZIP_URL}" 2>/dev/null; then
        if unzip -q -o "${TMP_STAGING}/xray.zip" -d "${XRAY_STAGING}" 2>/dev/null; then
            mkdir -p "${PROJECT_ROOT}/target"
            cp "${TMP_STAGING}/xray.zip" "${CACHE_ZIP}" 2>/dev/null || true
            XRAY_FOUND=true
            echo "    -> Successfully downloaded & extracted Xray-core"
        fi
    fi
fi

# 3. Fallback: Copy from local host system if available
if [[ "${XRAY_FOUND}" != "true" ]] && [[ -f "/usr/bin/xray" ]]; then
    echo "==> Bundling host system /usr/bin/xray..."
    cp "/usr/bin/xray" "${XRAY_STAGING}/xray"
    if [[ -d "/usr/share/xray" ]]; then
        cp /usr/share/xray/*.dat "${XRAY_STAGING}/" 2>/dev/null || true
    fi
    XRAY_FOUND=true
fi

# ------------------------------------------------------------------------------
# 1. Prepare Standard FHS Directory Tree
# ------------------------------------------------------------------------------
FHS_DIR="${TMP_STAGING}/fhs"
mkdir -p "${FHS_DIR}/usr/bin"
mkdir -p "${FHS_DIR}/usr/share/applications"
mkdir -p "${FHS_DIR}/usr/share/icons"
mkdir -p "${FHS_DIR}/usr/share/licenses/xdelta"
mkdir -p "${FHS_DIR}/usr/share/doc/xdelta"

install -m 755 "${BINARY_PATH}" "${FHS_DIR}/usr/bin/xdelta"
install -m 644 "${PROJECT_ROOT}/assets/xdelta.desktop" "${FHS_DIR}/usr/share/applications/xdelta.desktop"
cp -r "${PROJECT_ROOT}/assets/icons/hicolor" "${FHS_DIR}/usr/share/icons/"
install -m 644 "${PROJECT_ROOT}/LICENSE" "${FHS_DIR}/usr/share/licenses/xdelta/LICENSE"
install -m 644 "${PROJECT_ROOT}/README.md" "${FHS_DIR}/usr/share/doc/xdelta/README.md"

if [[ -f "${XRAY_STAGING}/xray" ]]; then
    mkdir -p "${FHS_DIR}/usr/lib/xdelta/bin"
    install -m 755 "${XRAY_STAGING}/xray" "${FHS_DIR}/usr/lib/xdelta/bin/xray"
    mkdir -p "${FHS_DIR}/usr/share/xdelta"
    for dat in "${XRAY_STAGING}"/*.dat; do
        if [[ -f "${dat}" ]]; then
            install -m 644 "${dat}" "${FHS_DIR}/usr/share/xdelta/"
        fi
    done
fi

# ------------------------------------------------------------------------------
# 2. Build Standalone Portable Tarball (.tar.gz)
# ------------------------------------------------------------------------------
echo "==> Building portable tarball..."
TAR_DIR="${TMP_STAGING}/xdelta-${VERSION}-linux-${ARCH}"
mkdir -p "${TAR_DIR}/assets/icons"
install -m 755 "${BINARY_PATH}" "${TAR_DIR}/xdelta"
install -m 644 "${PROJECT_ROOT}/assets/xdelta.desktop" "${TAR_DIR}/xdelta.desktop"
install -m 755 "${PROJECT_ROOT}/packaging/install.sh" "${TAR_DIR}/install.sh"
install -m 644 "${PROJECT_ROOT}/LICENSE" "${TAR_DIR}/LICENSE"
install -m 644 "${PROJECT_ROOT}/README.md" "${TAR_DIR}/README.md"
cp -r "${PROJECT_ROOT}/assets/icons/hicolor" "${TAR_DIR}/assets/icons/"

if [[ -f "${XRAY_STAGING}/xray" ]]; then
    mkdir -p "${TAR_DIR}/bin"
    install -m 755 "${XRAY_STAGING}/xray" "${TAR_DIR}/bin/xray"
    for dat in "${XRAY_STAGING}"/*.dat; do
        if [[ -f "${dat}" ]]; then
            install -m 644 "${dat}" "${TAR_DIR}/bin/"
        fi
    done
fi

tar -czf "${DIST_DIR}/xdelta-${VERSION}-linux-${ARCH}.tar.gz" -C "${TMP_STAGING}" "xdelta-${VERSION}-linux-${ARCH}"
echo "    -> Created: ${DIST_DIR}/xdelta-${VERSION}-linux-${ARCH}.tar.gz"

# ------------------------------------------------------------------------------
# 3. Build Debian / Ubuntu Package (.deb)
# ------------------------------------------------------------------------------
echo "==> Building Debian (.deb) package..."
DEB_DIR="${TMP_STAGING}/deb"
mkdir -p "${DEB_DIR}/DEBIAN"
cp -r "${FHS_DIR}/usr" "${DEB_DIR}/"

cat <<EOF > "${DEB_DIR}/DEBIAN/control"
Package: xdelta
Version: ${VERSION}
Section: net
Priority: optional
Architecture: ${DEB_ARCH}
Depends: libgtk-4-1 (>= 4.12), libadwaita-1-0 (>= 1.5)
Recommends: xray | sing-box
Maintainer: TssHack <info@github.com/TssHack>
Homepage: https://github.com/TssHack/xdelta
Description: Native, high-performance Linux proxy client for Xray and Sing-box
 xdelta is a 100% native Rust desktop proxy client built with GTK4 and Libadwaita
 designed specifically for the modern Linux desktop (Wayland/X11).
EOF

if command -v dpkg-deb >/dev/null 2>&1; then
    dpkg-deb --build --root-owner-group "${DEB_DIR}" "${DIST_DIR}/xdelta_${VERSION}_${DEB_ARCH}.deb"
    echo "    -> Created: ${DIST_DIR}/xdelta_${VERSION}_${DEB_ARCH}.deb"
elif command -v fpm >/dev/null 2>&1; then
    fpm -s dir -t deb \
        -n xdelta \
        -v "${VERSION}" \
        -a "${DEB_ARCH}" \
        -d "libgtk-4-1 >= 4.12" \
        -d "libadwaita-1-0 >= 1.5" \
        --license "GPL-3.0" \
        --vendor "TssHack" \
        --url "https://github.com/TssHack/xdelta" \
        --description "Native Linux proxy client for Xray and Sing-box" \
        -p "${DIST_DIR}/xdelta_${VERSION}_${DEB_ARCH}.deb" \
        -C "${FHS_DIR}" .
    echo "    -> Created: ${DIST_DIR}/xdelta_${VERSION}_${DEB_ARCH}.deb (via fpm)"
else
    echo "    [!] Neither dpkg-deb nor fpm found, skipping .deb generation"
fi

# ------------------------------------------------------------------------------
# 4. Build Fedora / RHEL Package (.rpm)
# ------------------------------------------------------------------------------
echo "==> Building Fedora (.rpm) package..."
if command -v fpm >/dev/null 2>&1; then
    fpm -s dir -t rpm \
        -n xdelta \
        -v "${VERSION}" \
        --iteration 1 \
        -a "${RPM_ARCH}" \
        -d "gtk4 >= 4.12" \
        -d "libadwaita >= 1.5" \
        --license "GPL-3.0" \
        --vendor "TssHack" \
        --url "https://github.com/TssHack/xdelta" \
        --description "Native Linux proxy client for Xray and Sing-box" \
        -p "${DIST_DIR}/xdelta-${VERSION}-1.${RPM_ARCH}.rpm" \
        -C "${FHS_DIR}" .
    echo "    -> Created: ${DIST_DIR}/xdelta-${VERSION}-1.${RPM_ARCH}.rpm"
elif command -v rpmbuild >/dev/null 2>&1; then
    RPM_TOP="${TMP_STAGING}/rpmbuild"
    mkdir -p "${RPM_TOP}"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
    
    RPM_EXTRA_FILES=""
    if [[ -d "${FHS_DIR}/usr/lib/xdelta" ]]; then
        RPM_EXTRA_FILES+=$'/usr/lib/xdelta\n'
    fi
    if [[ -d "${FHS_DIR}/usr/share/xdelta" ]]; then
        RPM_EXTRA_FILES+=$'/usr/share/xdelta\n'
    fi

    cat <<EOF > "${RPM_TOP}/SPECS/xdelta.spec"
Name:           xdelta
Version:        ${VERSION}
Release:        1%{?dist}
Summary:        Native, high-performance Linux proxy client for Xray and Sing-box
License:        GPL-3.0
URL:            https://github.com/TssHack/xdelta

AutoReqProv:    no
Requires:       gtk4 >= 4.12, libadwaita >= 1.5

%description
Native Linux proxy client for Xray and Sing-box built with Rust and Libadwaita.

%install
rm -rf \$RPM_BUILD_ROOT
mkdir -p \$RPM_BUILD_ROOT
cp -r ${FHS_DIR}/* \$RPM_BUILD_ROOT/

%files
/usr/bin/xdelta
/usr/share/applications/xdelta.desktop
/usr/share/icons/hicolor/*
/usr/share/licenses/xdelta/LICENSE
/usr/share/doc/xdelta/README.md
${RPM_EXTRA_FILES}

%changelog
* Tue Oct 13 2026 TssHack <info@github.com/TssHack> - ${VERSION}-1
- Release ${VERSION}
EOF
    rpmbuild --define "_topdir ${RPM_TOP}" --target "${RPM_ARCH}" -bb "${RPM_TOP}/SPECS/xdelta.spec"
    find "${RPM_TOP}/RPMS" -type f -name "*.rpm" -exec cp {} "${DIST_DIR}/" \;
    echo "    -> Created RPM in ${DIST_DIR}"
else
    echo "    [!] Neither fpm nor rpmbuild found, skipping .rpm generation"
fi

# ------------------------------------------------------------------------------
# 5. Build Arch Linux Package (.pkg.tar.zst)
# ------------------------------------------------------------------------------
echo "==> Building Arch Linux (.pkg.tar.zst) package..."
ARCH_PKG_DIR="${TMP_STAGING}/arch"
mkdir -p "${ARCH_PKG_DIR}"
cp -r "${FHS_DIR}/usr" "${ARCH_PKG_DIR}/"

PKG_SIZE=$(du -sb "${ARCH_PKG_DIR}/usr" | awk '{print $1}')
BUILD_DATE=$(date +%s)

cat <<EOF > "${ARCH_PKG_DIR}/.PKGINFO"
pkgname = xdelta
pkgver = ${VERSION}-1
pkgdesc = Native, high-performance Linux proxy client for Xray and Sing-box
url = https://github.com/TssHack/xdelta
builddate = ${BUILD_DATE}
packager = TssHack <https://github.com/TssHack/xdelta>
size = ${PKG_SIZE}
arch = ${ARCH}
license = GPL3
depend = gtk4
depend = libadwaita
optdepend = xray: Xray proxy engine
optdepend = sing-box: Sing-box proxy engine
EOF

if command -v tar >/dev/null 2>&1 && command -v zstd >/dev/null 2>&1; then
    (cd "${ARCH_PKG_DIR}" && tar --zstd -cf "${DIST_DIR}/xdelta-${VERSION}-1-${ARCH}.pkg.tar.zst" .PKGINFO usr)
    echo "    -> Created: ${DIST_DIR}/xdelta-${VERSION}-1-${ARCH}.pkg.tar.zst"
else
    echo "    [!] tar or zstd not found, skipping Arch package generation"
fi

# ------------------------------------------------------------------------------
# 6. Build Universal AppImage (.AppImage)
# ------------------------------------------------------------------------------
echo "==> Building AppImage..."
APPDIR="${TMP_STAGING}/AppDir"
mkdir -p "${APPDIR}"
cp -r "${FHS_DIR}/usr" "${APPDIR}/"
if [[ -f "${XRAY_STAGING}/xray" ]]; then
    install -m 755 "${XRAY_STAGING}/xray" "${APPDIR}/usr/bin/xray"
fi
install -m 755 "${PROJECT_ROOT}/packaging/appimage/AppRun" "${APPDIR}/AppRun"
install -m 644 "${PROJECT_ROOT}/assets/xdelta.desktop" "${APPDIR}/xdelta.desktop"
install -m 644 "${PROJECT_ROOT}/assets/icons/hicolor/scalable/apps/xdelta-app.svg" "${APPDIR}/xdelta-app.svg"
install -m 644 "${PROJECT_ROOT}/assets/icons/hicolor/scalable/apps/xdelta-app.svg" "${APPDIR}/.DirIcon"

APPIMAGETOOL="${APPIMAGETOOL:-}"
if [[ -z "${APPIMAGETOOL}" ]]; then
    if command -v appimagetool >/dev/null 2>&1; then
        APPIMAGETOOL="appimagetool"
    elif [[ -f "${PROJECT_ROOT}/appimagetool-${ARCH}.AppImage" ]]; then
        APPIMAGETOOL="${PROJECT_ROOT}/appimagetool-${ARCH}.AppImage"
    elif [[ -f "./appimagetool-${ARCH}.AppImage" ]]; then
        APPIMAGETOOL="./appimagetool-${ARCH}.AppImage"
    elif [[ -f "/tmp/appimagetool-${ARCH}.AppImage" ]]; then
        APPIMAGETOOL="/tmp/appimagetool-${ARCH}.AppImage"
    fi
fi

if [[ -n "${APPIMAGETOOL}" ]]; then
    if "${APPIMAGETOOL}" "${APPDIR}" "${DIST_DIR}/xdelta-${VERSION}-${ARCH}.AppImage" 2>/dev/null; then
        echo "    -> Created: ${DIST_DIR}/xdelta-${VERSION}-${ARCH}.AppImage"
    elif "${APPIMAGETOOL}" --appimage-extract-and-run "${APPDIR}" "${DIST_DIR}/xdelta-${VERSION}-${ARCH}.AppImage" 2>/dev/null; then
        echo "    -> Created: ${DIST_DIR}/xdelta-${VERSION}-${ARCH}.AppImage"
    else
        echo "    [!] appimagetool execution failed, continuing..."
    fi
else
    echo "    [!] appimagetool not found, skipping .AppImage generation"
fi

echo "=========================================================="
echo " Packaging completed! Artifacts in ${DIST_DIR}:"
ls -lh "${DIST_DIR}"
echo "=========================================================="
