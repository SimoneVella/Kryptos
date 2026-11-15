#!/usr/bin/env bash
# Notarizes, staples, and verifies a signed macOS Kryptos DMG bundle.
#
# Usage: scripts/notarize-macos.sh [path/to/Kryptos.dmg]
#
# Authentication (provide one of):
#   1. App Store Connect API Key:
#        APPLE_API_KEY       Key ID
#        APPLE_API_ISSUER    Issuer UUID
#        APPLE_API_KEY_PATH  Path to AuthKey_<ID>.p8
#   2. Apple ID + App-Specific Password:
#        APPLE_ID            Apple ID email
#        APPLE_PASSWORD      App-specific password from account.apple.com
#        APPLE_TEAM_ID       Optional, defaults to LVTY7AGJGB
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DMG="${1:-$ROOT/target/release/bundle/dmg/Kryptos_0.1.0_aarch64.dmg}"

if [[ ! -f "$DMG" ]]; then
  echo "DMG not found: $DMG" >&2
  echo "Build it first with:" >&2
  echo "  cd apps/desktop && APPLE_SIGNING_IDENTITY=\"Developer ID Application: Simone Vella (LVTY7AGJGB)\" npx tauri build" >&2
  exit 1
fi

AUTH_ARGS=()
if [[ -n "${APPLE_API_KEY:-}" && -n "${APPLE_API_ISSUER:-}" && -n "${APPLE_API_KEY_PATH:-}" ]]; then
  if [[ ! -f "$APPLE_API_KEY_PATH" ]]; then
    echo "API key file not found: $APPLE_API_KEY_PATH" >&2
    exit 1
  fi
  AUTH_ARGS=(--key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER")
elif [[ -n "${APPLE_ID:-}" && -n "${APPLE_PASSWORD:-}" ]]; then
  AUTH_ARGS=(--apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "${APPLE_TEAM_ID:-LVTY7AGJGB}")
else
  echo "Missing notarization credentials." >&2
  echo "Set either APPLE_API_KEY + APPLE_API_ISSUER + APPLE_API_KEY_PATH," >&2
  echo "or APPLE_ID + APPLE_PASSWORD (app-specific password; APPLE_TEAM_ID defaults to LVTY7AGJGB)." >&2
  exit 1
fi

echo "Submitting $DMG to Apple notarization service..."
xcrun notarytool submit "$DMG" "${AUTH_ARGS[@]}" --wait

echo "Stapling notarization ticket..."
xcrun stapler staple "$DMG"
xcrun stapler validate "$DMG"

echo "Verifying Gatekeeper assessment..."
spctl -a -t open --context context:primary-signature -v "$DMG"

echo "Done: $DMG is notarized and stapled."
