#!/usr/bin/env bash
# Registers kryptos-native-host with Chromium-based browsers and Firefox.
#
# Dev fallback only: the app does this itself (Impostazioni → Estensione browser → Collega).
# Usage: scripts/install-native-host.sh [chrome-extension-id]
#   Defaults to the fixed ID derived from the "key" in extension/manifest.json.
set -euo pipefail

EXT_ID="${1:-jclbckdbmecjgoopnpchbijihdfeajkb}"
if [[ ! "$EXT_ID" =~ ^[a-p]{32}$ ]]; then
  echo "usage: $0 <chrome-extension-id>   (32 chars a-p, see chrome://extensions)" >&2
  exit 1
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NAME="com.kryptos.bridge"
FIREFOX_ID="kryptos@kryptos.local"

cargo build --release -p kryptos-native-host --manifest-path "$ROOT/Cargo.toml"

case "$(uname -s)" in
  Darwin)
    INSTALL_DIR="$HOME/Library/Application Support/com.kryptos.desktop/bin"
    CHROMIUM_DIRS=(
      "$HOME/Library/Application Support/Google/Chrome/NativeMessagingHosts"
      "$HOME/Library/Application Support/Chromium/NativeMessagingHosts"
      "$HOME/Library/Application Support/BraveSoftware/Brave-Browser/NativeMessagingHosts"
      "$HOME/Library/Application Support/Microsoft Edge/NativeMessagingHosts"
      "$HOME/Library/Application Support/Arc/User Data/NativeMessagingHosts"
    )
    FIREFOX_DIR="$HOME/Library/Application Support/Mozilla/NativeMessagingHosts"
    ;;
  Linux)
    INSTALL_DIR="$HOME/.local/share/com.kryptos.desktop/bin"
    CHROMIUM_DIRS=(
      "$HOME/.config/google-chrome/NativeMessagingHosts"
      "$HOME/.config/chromium/NativeMessagingHosts"
      "$HOME/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts"
      "$HOME/.config/microsoft-edge/NativeMessagingHosts"
    )
    FIREFOX_DIR="$HOME/.mozilla/native-messaging-hosts"
    ;;
  *)
    echo "Windows: register via HKCU\\Software\\Google\\Chrome\\NativeMessagingHosts (not scripted yet)" >&2
    exit 1
    ;;
esac

mkdir -p "$INSTALL_DIR"
chmod 700 "$(dirname "$INSTALL_DIR")"
install -m 755 "$ROOT/target/release/kryptos-native-host" "$INSTALL_DIR/kryptos-native-host"
HOST_PATH="$INSTALL_DIR/kryptos-native-host"

for dir in "${CHROMIUM_DIRS[@]}"; do
  # Only register for browsers that are installed (their profile dir exists).
  [[ -d "$(dirname "$dir")" ]] || continue
  mkdir -p "$dir"
  cat > "$dir/$NAME.json" <<EOF
{
  "name": "$NAME",
  "description": "Kryptos local vault bridge",
  "path": "$HOST_PATH",
  "type": "stdio",
  "allowed_origins": ["chrome-extension://$EXT_ID/"]
}
EOF
  echo "registered: $dir/$NAME.json"
done

if [[ -d "$(dirname "$FIREFOX_DIR")" ]]; then
  mkdir -p "$FIREFOX_DIR"
  cat > "$FIREFOX_DIR/$NAME.json" <<EOF
{
  "name": "$NAME",
  "description": "Kryptos local vault bridge",
  "path": "$HOST_PATH",
  "type": "stdio",
  "allowed_extensions": ["$FIREFOX_ID"]
}
EOF
  echo "registered: $FIREFOX_DIR/$NAME.json"
fi

echo "Done. Reload the extension and open its popup on a login page."
