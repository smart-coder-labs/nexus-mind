#!/usr/bin/env bash
# Builds and launches NexusMascot as a real macOS .app bundle so TCC grants
# Microphone, Speech Recognition, Accessibility, and Screen Recording cleanly.

set -euo pipefail

APP_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$APP_DIR"

echo "Building NexusMascot (release)..."
swift build -c release

BIN_PATH="$APP_DIR/.build/release/NexusMascot"
BUNDLE_DIR="$APP_DIR/.build/NexusMascot.app"

rm -rf "$BUNDLE_DIR"
mkdir -p "$BUNDLE_DIR/Contents/MacOS"
cp "$BIN_PATH" "$BUNDLE_DIR/Contents/MacOS/NexusMascot"
cp "$APP_DIR/Resources/Info.plist" "$BUNDLE_DIR/Contents/Info.plist"

# Ad-hoc code sign so macOS TCC persists permissions across launches
codesign --force --deep --sign - "$BUNDLE_DIR" 2>/dev/null || true

echo "Launching $BUNDLE_DIR..."
open "$BUNDLE_DIR"
