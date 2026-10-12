#!/bin/bash
# Wrap a built takt binary in a macOS .app bundle.
# Usage: scripts/bundle.sh [path/to/takt-binary] [output-dir]
set -e

BINARY="${1:-target/release/takt}"
OUT_DIR="${2:-$(dirname "$BINARY")}"
APP="$OUT_DIR/takt.app"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp "$BINARY" "$APP/Contents/MacOS/takt"
mkdir -p "$APP/Contents/Resources"
cp "$(dirname "$0")/../assets/takt.icns" "$APP/Contents/Resources/takt.icns"

cat > "$APP/Contents/Info.plist" << 'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>takt</string>
  <key>CFBundleDisplayName</key>
  <string>takt</string>
  <key>CFBundleIdentifier</key>
  <string>com.tanzverein.takt</string>
  <key>CFBundleVersion</key>
  <string>0.1.0</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleExecutable</key>
  <string>takt</string>
  <key>CFBundleIconFile</key>
  <string>takt</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSMicrophoneUsageDescription</key>
  <string>takt needs access to your audio input to detect BPM.</string>
</dict>
</plist>
PLIST

echo "Bundle created at $APP"
