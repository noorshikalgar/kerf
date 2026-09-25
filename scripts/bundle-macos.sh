#!/usr/bin/env bash
# Builds dist/Kerf.app (and dist/Kerf-<version>-macos-<arch>.zip).
#
#   scripts/bundle-macos.sh              # native arch
#   scripts/bundle-macos.sh universal    # arm64 + x86_64 via lipo (needs both targets)
set -euo pipefail
cd "$(dirname "$0")/.."

ARCH="${1:-native}"
VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
APP="dist/Kerf.app"

if [[ "$ARCH" == "universal" ]]; then
  cargo build --release --target aarch64-apple-darwin
  cargo build --release --target x86_64-apple-darwin
  mkdir -p target/universal
  lipo -create -output target/universal/kerf \
    target/aarch64-apple-darwin/release/kerf \
    target/x86_64-apple-darwin/release/kerf
  BIN=target/universal/kerf
  LABEL=universal
else
  cargo build --release
  BIN=target/release/kerf
  LABEL="$(uname -m)"
fi

# Icon: regenerate the iconset if missing, then pack it.
if [[ ! -d assets/icon/Kerf.iconset ]]; then cargo run --quiet --example icon; fi
iconutil -c icns assets/icon/Kerf.iconset -o assets/icon/Kerf.icns

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/kerf"
cp assets/icon/Kerf.icns "$APP/Contents/Resources/Kerf.icns"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Kerf</string>
  <key>CFBundleDisplayName</key><string>Kerf</string>
  <key>CFBundleIdentifier</key><string>io.github.noorshikalgar.kerf</string>
  <key>CFBundleExecutable</key><string>kerf</string>
  <key>CFBundleIconFile</key><string>Kerf</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

# Ad-hoc signature so Apple Silicon will launch it (not notarized).
codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || true

ZIP="dist/Kerf-${VERSION}-macos-${LABEL}.zip"
rm -f "$ZIP"
(cd dist && ditto -c -k --keepParent Kerf.app "$(basename "$ZIP")")
echo "built $APP and $ZIP"
