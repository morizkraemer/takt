#!/bin/bash
set -e

BINARY="target/release/bpm2"
APP="$BINARY.app"

echo "Building release..."
cargo build --release

echo "Creating app bundle..."
mkdir -p "$APP/Contents/MacOS"
cp "$BINARY" "$APP/Contents/MacOS/bpm2"

cat > "$APP/Contents/Info.plist" << 'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>bpm2</string>
  <key>CFBundleDisplayName</key>
  <string>bpm2</string>
  <key>CFBundleIdentifier</key>
  <string>com.tanzverein.bpm2</string>
  <key>CFBundleVersion</key>
  <string>0.1.0</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleExecutable</key>
  <string>bpm2</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>NSMicrophoneUsageDescription</key>
  <string>bpm2 needs access to your audio input to detect BPM.</string>
</dict>
</plist>
PLIST

echo "Bundle created at $APP"
ls -lh "$APP/Contents/MacOS/bpm2"
