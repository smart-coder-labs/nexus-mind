#!/usr/bin/env bash
# One-command macOS bootstrapper for the NexusMind Autonomous Mascot
# Usage: ./scripts/mascot/setup_macos.sh [--with-laya-mlx] [--install-launchd]

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
WITH_LAYA=0
INSTALL_LAUNCHD=0

for arg in "$@"; do
  case "$arg" in
    --with-laya-mlx) WITH_LAYA=1 ;;
    --install-launchd) INSTALL_LAUNCHD=1 ;;
    *) echo "Unknown flag: $arg"; exit 1 ;;
  esac
done

echo "=== NexusMind Mascot — macOS Setup ==="
if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "Warning: This script is designed for macOS (detected $(uname -s)). Building Rust daemon only."
fi

# 1. Build Rust daemon (release)
echo "[1/4] Building nexus-mascot-daemon (Rust)..."
(cd "$ROOT_DIR/apps/mascot-daemon" && cargo build --release)
DAEMON_BIN="$ROOT_DIR/apps/mascot-daemon/target/release/nexus-mascot-daemon"

# 2. Initialize ~/.nexusmind/mascot.yaml if absent
echo "[2/4] Initializing ~/.nexusmind/mascot.yaml..."
"$DAEMON_BIN" init-config

# 3. Optional local Laya MLX install on Apple Silicon
if [[ "$WITH_LAYA" -eq 1 ]]; then
  echo "[3/4] Installing laya-mlx for local Apple Silicon System 1 decisions..."
  python3 -m pip install --upgrade laya-mlx
else
  echo "[3/4] Skipping laya-mlx install (pass --with-laya-mlx to install local Laya)."
fi

# 4. Build native SwiftUI macOS app
if [[ "$(uname -s)" == "Darwin" ]]; then
  echo "[4/4] Building native SwiftUI macOS app (apps/mascot-macos)..."
  (cd "$ROOT_DIR/apps/mascot-macos" && swift build -c release)

  if [[ "$INSTALL_LAUNCHD" -eq 1 ]]; then
    PLIST="$HOME/Library/LaunchAgents/com.smartcoderlabs.nexusmind.mascot.plist"
    mkdir -p "$HOME/Library/LaunchAgents" "$HOME/.nexusmind/logs"
    cat > "$PLIST" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>com.smartcoderlabs.nexusmind.mascot</string>
  <key>ProgramArguments</key>
  <array>
    <string>$DAEMON_BIN</string>
    <string>run</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>StandardOutPath</key>
  <string>$HOME/.nexusmind/logs/daemon.out.log</string>
  <key>StandardErrorPath</key>
  <string>$HOME/.nexusmind/logs/daemon.err.log</string>
</dict>
</plist>
EOF
    launchctl unload "$PLIST" 2>/dev/null || true
    launchctl load "$PLIST"
    echo "Installed and loaded launchd agent: $PLIST"
  fi
fi

echo ""
echo "=== Setup complete! ==="
echo "Next steps:"
echo "  1) Edit ~/.nexusmind/mascot.yaml (choose laya_local / jev_cloud / hybrid & LLM provider)"
echo "  2) Start local Laya (if using laya_local/hybrid): laya-serve --port 8787"
echo "  3) Start daemon: $DAEMON_BIN run"
echo "  4) Launch macOS mascot app: (cd $ROOT_DIR/apps/mascot-macos && swift run)"
