#!/usr/bin/env bash
# package-macos.sh — build Rockbox.app and Rockbox.dmg from the Slint desktop app.
#
# Produces:
#   dist/Rockbox.app   — double-clickable bundle (icon, Info.plist, ad-hoc signed)
#   dist/Rockbox.dmg   — the artifact the release workflow uploads and the
#                        homebrew-tap cask installs
#
# The app embeds the whole daemon (librockboxd.a via desktop/build.rs), so the
# bundle is self-contained: no external rockboxd process, no CLI entry point.
# The five built-in skins are compiled into the binary (src/skin.rs); user skins
# are read from ~/.config/rockbox.org/skins/ at runtime.
#
# Usage:
#   bash desktop/package-macos.sh              # builds, then packages
#   SKIP_BUILD=1 bash desktop/package-macos.sh # package an existing binary (CI)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

APP_NAME="Rockbox"
BUNDLE_ID="org.rockbox.desktop"
# Release tags are dates (2026.09.10); CFBundleVersion wants a dotted number,
# which that already is. Falls back to the crate version for local builds.
VERSION="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}"
VERSION="${VERSION#v}"
BINARY="target/release/rockbox-desktop"
# Shared with the GPUI app and the Xcode project — one Rockbox icon, not three.
APPICONSET="../macos/Rockbox/Assets.xcassets/AppIcon.appiconset"
OUT_DIR="dist"
APP_BUNDLE="${OUT_DIR}/${APP_NAME}.app"
DMG_STAGING="${OUT_DIR}/dmg_staging"
DMG_OUT="${OUT_DIR}/${APP_NAME}.dmg"

if [ "$(uname -s)" != "Darwin" ]; then
    echo "package-macos.sh is for macOS packaging only (got $(uname -s))" >&2
    exit 1
fi

# ── 1. Build ─────────────────────────────────────────────────────────────────
if [ "${SKIP_BUILD:-0}" != "1" ]; then
    echo "▸ Building release binary…"
    # --locked: the committed lock pins slint-build 1.17.1 (since yanked); a
    # fresh resolution would fail, while the locked graph still downloads fine.
    cargo build --release --locked
fi

[ -f "$BINARY" ] || { echo "missing $BINARY — build first" >&2; exit 1; }

rm -rf "$APP_BUNDLE" "$DMG_STAGING" "$DMG_OUT"
mkdir -p "${APP_BUNDLE}/Contents/MacOS" "${APP_BUNDLE}/Contents/Resources"

# ── 2. Icon: appiconset PNGs → AppIcon.icns ──────────────────────────────────
echo "▸ Building app icon…"
ICONSET="${OUT_DIR}/AppIcon.iconset"
rm -rf "$ICONSET" && mkdir -p "$ICONSET"

resize_png() {
    local src="${APPICONSET}/${1}.png" name="$2" size="$3"
    sips -z "$size" "$size" "$src" --out "${ICONSET}/${name}.png" &>/dev/null
}

resize_png 16   icon_16x16        16
resize_png 32   icon_16x16@2x     32
resize_png 32   icon_32x32        32
resize_png 64   icon_32x32@2x     64
resize_png 128  icon_128x128      128
resize_png 256  icon_128x128@2x   256
resize_png 256  icon_256x256      256
resize_png 512  icon_256x256@2x   512
resize_png 512  icon_512x512      512
resize_png 1024 icon_512x512@2x   1024

iconutil -c icns "$ICONSET" -o "${APP_BUNDLE}/Contents/Resources/AppIcon.icns"
rm -rf "$ICONSET"

# ── 3. Assemble the bundle ───────────────────────────────────────────────────
echo "▸ Assembling ${APP_BUNDLE}…"
cp "$BINARY" "${APP_BUNDLE}/Contents/MacOS/${APP_NAME}"
chmod +x "${APP_BUNDLE}/Contents/MacOS/${APP_NAME}"
# Bundled skins: the built-ins are embedded, these are the editable copies the
# README points users at.
cp -r skins "${APP_BUNDLE}/Contents/Resources/skins"

# LSMinimumSystemVersion 11.0 matches the cask's `depends_on macos: :big_sur`.
#
# NSLocalNetworkUsageDescription / NSBonjourServices are load-bearing on macOS
# 15+: the embedded daemon advertises itself and discovers peers over mDNS, and
# an app bundle without them is silently denied local-network access — the
# device list just stays empty. A bare CLI binary never hits that gate, which
# is why this only shows up once the app is bundled.
cat > "${APP_BUNDLE}/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>          <string>${APP_NAME}</string>
    <key>CFBundleIdentifier</key>          <string>${BUNDLE_ID}</string>
    <key>CFBundleName</key>                <string>${APP_NAME}</string>
    <key>CFBundleDisplayName</key>         <string>${APP_NAME}</string>
    <key>CFBundleIconFile</key>            <string>AppIcon</string>
    <key>CFBundleVersion</key>             <string>${VERSION}</string>
    <key>CFBundleShortVersionString</key>  <string>${VERSION}</string>
    <key>CFBundlePackageType</key>         <string>APPL</string>
    <key>CFBundleInfoDictionaryVersion</key> <string>6.0</string>
    <key>LSApplicationCategoryType</key>   <string>public.app-category.music</string>
    <key>LSMinimumSystemVersion</key>      <string>11.0</string>
    <key>NSHighResolutionCapable</key>     <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key> <true/>
    <key>NSLocalNetworkUsageDescription</key>
    <string>Rockbox finds and controls other Rockbox players, Chromecast, AirPlay and Snapcast devices on your network.</string>
    <key>NSBonjourServices</key>
    <array>
        <string>_rockbox._tcp</string>
        <string>_googlecast._tcp</string>
        <string>_raop._tcp</string>
        <string>_airplay._tcp</string>
        <string>_jellyfin._tcp</string>
        <string>_plexmediasvr._tcp</string>
    </array>
</dict>
</plist>
PLIST

# ── 4. Ad-hoc code sign ──────────────────────────────────────────────────────
# Lets Gatekeeper run the app without a paid Developer ID. Users still get the
# "unidentified developer" prompt on first launch; the cask's `quarantine`
# handling in homebrew-tap takes care of the Homebrew install path.
echo "▸ Signing ad-hoc…"
codesign --force --deep --sign - "$APP_BUNDLE"
codesign --verify --deep --strict "$APP_BUNDLE"

# ── 5. DMG ───────────────────────────────────────────────────────────────────
echo "▸ Creating DMG…"
mkdir -p "$DMG_STAGING"
cp -R "$APP_BUNDLE" "$DMG_STAGING/"
ln -s /Applications "${DMG_STAGING}/Applications"

# Size the image explicitly. hdiutil's auto-estimate from -srcfolder is too
# tight for an 80 MB single-binary bundle and fails with "No space left on
# device" on a disk with tens of gigabytes free; 64 MB of slack covers the
# HFS+ metadata it does not account for.
SIZE_MB=$(( $(du -sk "$DMG_STAGING" | cut -f1) / 1024 + 64 ))

hdiutil create \
    -volname "${APP_NAME}" \
    -srcfolder "$DMG_STAGING" \
    -size "${SIZE_MB}m" \
    -fs HFS+ \
    -ov \
    -format UDZO \
    "$DMG_OUT"

rm -rf "$DMG_STAGING"

echo ""
echo "✓  ${APP_BUNDLE}"
echo "✓  ${DMG_OUT}"
