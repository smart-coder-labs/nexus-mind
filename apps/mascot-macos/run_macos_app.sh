#!/usr/bin/env bash
# Builds and installs NexusMascot as a real macOS .app in /Applications (or ~/Applications)
# so macOS TCC properly lists and persists Screen Recording, Accessibility, Mic & Speech permissions.

set -euo pipefail

APP_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$APP_DIR"

echo "Building NexusMascot (release)..."
swift build -c release

BIN_PATH="$APP_DIR/.build/release/NexusMascot"

if [[ -w "/Applications" ]]; then
  INSTALL_ROOT="/Applications"
else
  INSTALL_ROOT="$HOME/Applications"
  mkdir -p "$INSTALL_ROOT"
fi
BUNDLE_DIR="$INSTALL_ROOT/NexusMascot.app"

# Stop any previous instance cleanly before replacing the app bundle
pkill -x NexusMascot 2>/dev/null || true
sleep 0.3

rm -rf "$BUNDLE_DIR"
mkdir -p "$BUNDLE_DIR/Contents/MacOS"
cp "$BIN_PATH" "$BUNDLE_DIR/Contents/MacOS/NexusMascot"
cp "$APP_DIR/Resources/Info.plist" "$BUNDLE_DIR/Contents/Info.plist"

# Ad-hoc sign with fixed bundle identifier so macOS TCC recognizes the app cleanly
codesign --force --deep --sign - --identifier com.smartcoderlabs.nexusmind.mascot "$BUNDLE_DIR" 2>/dev/null || true

echo "Installed: $BUNDLE_DIR"
echo "Launching NexusMascot.app..."
open "$BUNDLE_DIR"
