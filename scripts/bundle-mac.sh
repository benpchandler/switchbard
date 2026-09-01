#!/usr/bin/env bash
#
# Build Switchbard.app — a real macOS application bundle.
#
# Output: target/release/Switchbard.app (drag to /Applications)
#
# This is the non-Developer-ID/un-notarized path used for alpha distribution.
# We still ad-hoc sign the completed bundle so macOS sees a coherent app
# structure with sealed resources. Developer ID signing + notarization will be
# a separate release path when we're ready for wider distribution.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

BIN_NAME="switchbard"
APP_NAME="Switchbard"
ASSETS_DIR="crates/switchbard-gui/assets"
ICNS="$ASSETS_DIR/icon.icns"
TARGET_DIR="target/release"
APP_BUNDLE="$TARGET_DIR/${APP_NAME}.app"
SIDECAR_SOURCE="${XPLAN_SIDECAR_SOURCE:-}"
SIDECAR_ARCHIVE="${XPLAN_SIDECAR_ARCHIVE:-}"
SIDECAR_STAGE="$TARGET_DIR/xplan-mission-sidecar-stage"
SIDECAR_DEST="$APP_BUNDLE/Contents/Helpers/xplan-mission-sidecar"

if [[ ! -f "$ICNS" ]]; then
  echo "✗ missing $ICNS — regenerate from $ASSETS_DIR/icon.png with iconutil" >&2
  exit 1
fi
if [[ -z "$SIDECAR_SOURCE" || -z "$SIDECAR_ARCHIVE" ]]; then
  echo "✗ XPLAN_SIDECAR_SOURCE and XPLAN_SIDECAR_ARCHIVE are required" >&2
  exit 1
fi

rm -rf "$SIDECAR_STAGE"
"$REPO_ROOT/scripts/acquire-xplan-mission-sidecar.sh" \
  --pin "$REPO_ROOT/xplan-sidecar-pin.json" \
  --target-os macos \
  --arch arm64 \
  --source "$SIDECAR_SOURCE" \
  --artifact "$SIDECAR_ARCHIVE" \
  --mode local \
  --destination "$SIDECAR_STAGE" >/dev/null

echo "→ cargo build --release -p switchbard-gui"
cargo build --release -p switchbard-gui

if [[ ! -x "$TARGET_DIR/$BIN_NAME" ]]; then
  echo "✗ expected binary at $TARGET_DIR/$BIN_NAME" >&2
  exit 1
fi

echo "→ assembling $APP_BUNDLE"
rm -rf "$APP_BUNDLE"
mkdir -p "$APP_BUNDLE/Contents/MacOS"
mkdir -p "$APP_BUNDLE/Contents/Resources"
mkdir -p "$APP_BUNDLE/Contents/Helpers"

cp "$TARGET_DIR/$BIN_NAME" "$APP_BUNDLE/Contents/MacOS/$APP_NAME"
cp "$ICNS"                 "$APP_BUNDLE/Contents/Resources/icon.icns"
cp -R -P "$SIDECAR_STAGE" "$SIDECAR_DEST"

VERSION="$(awk -F '"' '/^version = / { print $2; exit }' Cargo.toml)"

if [[ -z "$VERSION" ]]; then
  echo "✗ could not determine workspace version from Cargo.toml" >&2
  exit 1
fi

cat > "$APP_BUNDLE/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>           <string>${APP_NAME}</string>
  <key>CFBundleDisplayName</key>    <string>${APP_NAME}</string>
  <key>CFBundleIdentifier</key>     <string>com.menanticcreek.switchbard</string>
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

echo "→ verifying nested xplan helper signatures before outer signing"
while IFS= read -r nested; do
  if file "$nested" | grep -q 'Mach-O'; then
    codesign --force --sign - "$nested"
    codesign --verify --strict "$nested"
  else
    chmod a-x "$nested"
  fi
done < <(find "$SIDECAR_DEST" -type f -print)

echo "→ ad-hoc signing $APP_BUNDLE"
codesign --force --sign - "$APP_BUNDLE"
codesign --verify --deep --strict "$APP_BUNDLE"

echo "✓ built $APP_BUNDLE"
echo "  drag to /Applications, or run: open $APP_BUNDLE"
