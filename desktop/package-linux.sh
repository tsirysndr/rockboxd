#!/usr/bin/env bash
# package-linux.sh — stage the Slint desktop app for the .deb / .rpm builds.
#
# Produces the tree the Linux release workflows copy wholesale into the
# package root:
#
#   dist/usr/bin/rockbox-desktop
#   dist/usr/share/applications/rockbox-desktop.desktop
#   dist/usr/share/pixmaps/rockbox-desktop.png
#
# The five built-in skins are embedded in the binary (see src/skin.rs), so
# nothing else needs shipping; user skins are read from
# ~/.config/rockbox.org/skins/ at runtime.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

BINARY="target/release/rockbox-desktop"
# Shared with the macOS bundle and the GPUI app — one Rockbox icon, not three.
ICON="../macos/Rockbox/Assets.xcassets/AppIcon.appiconset/256.png"
OUT_DIR="dist"

if [ "$(uname -s)" != "Linux" ]; then
    echo "package-linux.sh is for Linux packaging only (got $(uname -s))" >&2
    exit 1
fi

echo "▸ Building release binary…"
# --locked: the committed lock pins slint-build 1.17.1 (since yanked); a fresh
# resolution would fail, while the locked graph still downloads fine.
cargo build --release --locked

echo "▸ Creating Linux package structure…"
rm -rf "$OUT_DIR"
mkdir -p "${OUT_DIR}/usr/bin" \
         "${OUT_DIR}/usr/share/applications" \
         "${OUT_DIR}/usr/share/pixmaps"

install -m 0755 "$BINARY" "${OUT_DIR}/usr/bin/rockbox-desktop"
install -m 0644 "$ICON"   "${OUT_DIR}/usr/share/pixmaps/rockbox-desktop.png"

cat > "${OUT_DIR}/usr/share/applications/rockbox-desktop.desktop" <<DESKTOP
[Desktop Entry]
Name=Rockbox
Comment=Modern audio player with multi-room support
Exec=rockbox-desktop
Icon=rockbox-desktop
Terminal=false
Type=Application
Categories=AudioVideo;Audio;Player;
DESKTOP

echo ""
echo "✓  Linux package staged in ${OUT_DIR}/"
echo "   Binary:  ${OUT_DIR}/usr/bin/rockbox-desktop"
echo "   Desktop: ${OUT_DIR}/usr/share/applications/rockbox-desktop.desktop"
echo "   Icon:    ${OUT_DIR}/usr/share/pixmaps/rockbox-desktop.png"
