#!/usr/bin/env bash
#
# Build Hive.app — a real macOS application bundle.
#
# Output: target/release/Hive.app (drag to /Applications)
#
# This is the unsigned/un-notarized path used for personal use + dogfooding.
# Signing + notarization will be a separate script when we're ready to
# distribute to other Macs.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

BIN_NAME="hive"
APP_NAME="Hive"
ASSETS_DIR="crates/hive-gui/assets"
ICNS="$ASSETS_DIR/icon.icns"
TARGET_DIR="target/release"
APP_BUNDLE="$TARGET_DIR/${APP_NAME}.app"

if [[ ! -f "$ICNS" ]]; then
  echo "✗ missing $ICNS — regenerate from $ASSETS_DIR/icon.png with iconutil" >&2
  exit 1
fi

echo "→ cargo build --release -p hive-gui"
cargo build --release -p hive-gui

if [[ ! -x "$TARGET_DIR/$BIN_NAME" ]]; then
  echo "✗ expected binary at $TARGET_DIR/$BIN_NAME" >&2
  exit 1
fi

echo "→ assembling $APP_BUNDLE"
rm -rf "$APP_BUNDLE"
mkdir -p "$APP_BUNDLE/Contents/MacOS"
mkdir -p "$APP_BUNDLE/Contents/Resources"

cp "$TARGET_DIR/$BIN_NAME" "$APP_BUNDLE/Contents/MacOS/$APP_NAME"
cp "$ICNS"                 "$APP_BUNDLE/Contents/Resources/icon.icns"

VERSION="$(grep -m1 '^version' crates/hive-gui/Cargo.toml | sed -E 's/version = "(.*)"/\1/')"

cat > "$APP_BUNDLE/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>           <string>${APP_NAME}</string>
  <key>CFBundleDisplayName</key>    <string>${APP_NAME}</string>
  <key>CFBundleIdentifier</key>     <string>com.menanticcreek.hive</string>
  <key>CFBundleVersion</key>        <string>${VERSION}</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleExecutable</key>     <string>${APP_NAME}</string>
  <key>CFBundleIconFile</key>       <string>icon</string>
  <key>CFBundlePackageType</key>    <string>APPL</string>
  <key>LSMinimumSystemVersion</key> <string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
</dict>
</plist>
PLIST

echo "✓ built $APP_BUNDLE"
echo "  drag to /Applications, or run: open $APP_BUNDLE"
