#!/usr/bin/env bash
#
# Build an unsigned macOS DMG for alpha distribution.
#
# Output:
#   target/dist/Switchbard-v<version>-macos-<arch>.dmg
#   target/dist/Switchbard-v<version>-macos-<arch>.dmg.sha256
#
# This intentionally does not use Developer ID signing or notarize the app. The
# bundle script still applies an ad-hoc signature so macOS can verify the bundle
# structure. See docs/INSTALL-MAC.md for first-open instructions users need.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

APP_NAME="Switchbard"
TARGET_DIR="target/release"
DIST_DIR="target/dist"
STAGING_DIR="$TARGET_DIR/dmg-staging"
APP_BUNDLE="$TARGET_DIR/${APP_NAME}.app"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: DMG packaging requires macOS" >&2
  exit 1
fi

if ! command -v hdiutil >/dev/null 2>&1; then
  echo "error: hdiutil is required to build a DMG" >&2
  exit 1
fi

VERSION="${HIVE_VERSION:-$(awk -F '"' '/^version = / { print $2; exit }' Cargo.toml)}"
if [[ -z "$VERSION" ]]; then
  echo "error: could not determine workspace version from Cargo.toml" >&2
  exit 1
fi

ARCH="${HIVE_ARCH:-$(uname -m)}"
case "$ARCH" in
  arm64)
    PLATFORM="macos-arm64"
    ;;
  x86_64)
    PLATFORM="macos-x86_64"
    ;;
  *)
    PLATFORM="macos-${ARCH}"
    ;;
esac

DMG_NAME="${APP_NAME}-v${VERSION}-${PLATFORM}.dmg"
DMG_PATH="$DIST_DIR/$DMG_NAME"
SHA_PATH="$DMG_PATH.sha256"
VOLUME_NAME="${APP_NAME} v${VERSION}"

echo "==> Building $APP_BUNDLE"
bash scripts/bundle-mac.sh

if [[ ! -d "$APP_BUNDLE" ]]; then
  echo "error: expected app bundle at $APP_BUNDLE" >&2
  exit 1
fi

echo "==> Staging DMG contents"
rm -rf "$STAGING_DIR"
mkdir -p "$STAGING_DIR"
mkdir -p "$DIST_DIR"

cp -R "$APP_BUNDLE" "$STAGING_DIR/${APP_NAME}.app"
ln -s /Applications "$STAGING_DIR/Applications"
cat > "$STAGING_DIR/README.txt" <<README
Switchbard is currently unnotarized and does not use Developer ID signing.

Install:
1. Drag Switchbard.app to Applications.
2. Right-click Switchbard.app and choose Open the first time.
3. Confirm macOS's unidentified developer prompt.

More detail: https://github.com/benpchandler/switchbard/blob/main/docs/INSTALL-MAC.md
README

echo "==> Creating $DMG_PATH"
rm -f "$DMG_PATH" "$SHA_PATH"
hdiutil create \
  -volname "$VOLUME_NAME" \
  -srcfolder "$STAGING_DIR" \
  -ov \
  -format UDZO \
  "$DMG_PATH"

(cd "$DIST_DIR" && shasum -a 256 "$DMG_NAME") > "$SHA_PATH"

echo "==> Release assets"
echo "    $DMG_PATH"
echo "    $SHA_PATH"
