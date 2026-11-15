#!/usr/bin/env bash
# Builds and signs a branded macOS DMG with Finder icon layout and background artwork.
#
# Usage: scripts/package-dmg.sh [path/to/Kryptos.app] [path/to/output.dmg]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="${1:-$ROOT/target/release/bundle/macos/Kryptos.app}"
VERSION="$(sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$ROOT/apps/desktop/src-tauri/tauri.conf.json" | head -n 1)"
VERSION="${VERSION:-0.2.0}"
OUT_DMG="${2:-$ROOT/target/release/bundle/dmg/Kryptos_${VERSION}_aarch64.dmg}"
BG_PNG="$ROOT/apps/desktop/src-tauri/installer/dmg-background.png"
VOL_ICNS="$ROOT/apps/desktop/src-tauri/icons/icon.icns"
IDENTITY="${APPLE_SIGNING_IDENTITY:-Developer ID Application: Simone Vella (LVTY7AGJGB)}"
VOL_NAME="Kryptos"

if [[ ! -d "$APP" ]]; then
  echo "App bundle not found: $APP" >&2
  exit 1
fi
if [[ ! -f "$BG_PNG" ]]; then
  echo "DMG background image not found: $BG_PNG" >&2
  exit 1
fi

# Detach any previously mounted Kryptos or temporary dmg volumes so /Volumes/Kryptos is free.
while read -r dev; do
  [[ -n "$dev" ]] && hdiutil detach "$dev" -force >/dev/null 2>&1 || true
done < <(hdiutil info | awk '/\/Volumes\/(Kryptos|dmg\.)/ {print prev} {prev=$1}' | grep '^/dev/' || true)

for mnt_existing in /Volumes/Kryptos* /Volumes/dmg.*; do
  if [[ -e "$mnt_existing" ]]; then
    hdiutil detach "$mnt_existing" -force >/dev/null 2>&1 || true
  fi
done

WORK="$(mktemp -d /tmp/kryptos_dmg_build.XXXXXX)"
ATTACHED_DEV=""
MOUNT_DIR=""

cleanup() {
  if [[ -n "$ATTACHED_DEV" ]]; then
    hdiutil detach "$ATTACHED_DEV" -force >/dev/null 2>&1 || true
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT

STAGE="$WORK/stage"
mkdir -p "$STAGE/.background"
ditto "$APP" "$STAGE/Kryptos.app"
ln -s /Applications "$STAGE/Applications"
cp "$BG_PNG" "$STAGE/.background/dmg-background.png"
if [[ -f "$VOL_ICNS" ]]; then
  cp "$VOL_ICNS" "$STAGE/.VolumeIcon.icns"
fi

RW_DMG="$WORK/rw.dmg"
echo "Creating read-write HFS+ image..."
hdiutil create -srcfolder "$STAGE" -volname "$VOL_NAME" -fs HFS+ -fsargs "-c c=64,a=16,e=16" -format UDRW -size 60m "$RW_DMG" >/dev/null

echo "Mounting image for Finder layout..."
ATTACH_OUT="$(hdiutil attach -readwrite -noverify -noautoopen "$RW_DMG")"
ATTACHED_DEV="$(echo "$ATTACH_OUT" | awk '/^\/dev\// {print $1; exit}')"
MOUNT_DIR="$(echo "$ATTACH_OUT" | awk '/\/Volumes\// {sub(/.*\/Volumes\//, "/Volumes/"); print; exit}')"

if [[ -z "$MOUNT_DIR" || ! -d "$MOUNT_DIR" ]]; then
  echo "ERROR: could not find mounted volume path in: $ATTACH_OUT" >&2
  exit 1
fi
echo "Mounted at $MOUNT_DIR ($ATTACHED_DEV)"

sleep 1.5
echo "Configuring Finder window layout..."
/usr/bin/osascript <<EOF
tell application "Finder"
  tell disk "$VOL_NAME"
    open
    delay 1
    set current view of container window to icon view
    set toolbar visible of container window to false
    set statusbar visible of container window to false
    set the bounds of container window to {120, 120, 780, 568}
    delay 0.5
    set opts to the icon view options of container window
    set icon size of opts to 104
    set text size of opts to 13
    set arrangement of opts to not arranged
    set background color of opts to {2827, 3084, 3598}
    set bgFile to (POSIX file "$MOUNT_DIR/.background/dmg-background.png") as alias
    set background picture of opts to bgFile
    set position of item "Kryptos.app" of container window to {180, 210}
    set position of item "Applications" of container window to {480, 210}
    update without registering applications
    delay 2
  end tell
end tell
EOF

for _ in {1..15}; do
  if [[ -f "$MOUNT_DIR/.DS_Store" ]] && (( $(stat -f%z "$MOUNT_DIR/.DS_Store") > 6148 )); then
    break
  fi
  sleep 1
done

if [[ ! -f "$MOUNT_DIR/.DS_Store" ]]; then
  echo "ERROR: Finder did not generate .DS_Store inside mounted DMG." >&2
  exit 1
fi

SAVED_DS="$WORK/saved_DS_Store"
cp "$MOUNT_DIR/.DS_Store" "$SAVED_DS"

python3 - "$SAVED_DS" <<'PY'
import plistlib, re, struct, sys

path = sys.argv[1]
ds = bytearray(open(path, "rb").read())
offs = [m.start() for m in re.finditer(b"bplist00", ds)]
for o in offs:
    blen = int.from_bytes(ds[o-4:o], "big")
    chunk = bytes(ds[o:o+blen])
    try:
        p = plistlib.loads(chunk)
    except Exception:
        continue
    if "backgroundType" in p:
        for key in ("scrollPositionX", "scrollPositionY"):
            val = p.get(key, 0.0)
            if isinstance(val, float) and val != 0.0:
                needle = b"\x23" + struct.pack(">d", val)
                zero = b"\x23" + struct.pack(">d", 0.0)
                if needle in chunk:
                    chunk = chunk.replace(needle, zero)
        assert len(chunk) == blen
        ds[o:o+blen] = chunk

open(path, "wb").write(bytes(ds))

# Re-verify patched .DS_Store
ds_check = open(path, "rb").read()
offs = [m.start() for m in re.finditer(b"bplist00", ds_check)]
parsed = []
for o in offs:
    blen = int.from_bytes(ds_check[o-4:o], "big")
    parsed.append(plistlib.loads(ds_check[o:o+blen]))
has_bg = any(
    p.get("backgroundType") == 2
    and "backgroundImageAlias" in p
    and p.get("iconSize") == 104.0
    and p.get("scrollPositionX") == 0.0
    and p.get("scrollPositionY") == 0.0
    for p in parsed
)
print("Patched .DS_Store:", len(ds_check), "bytes, plists:", len(parsed), "has_bg:", has_bg)
for p in parsed:
    if "backgroundType" in p:
        print("  icvp:", {k: v for k, v in p.items() if k != "backgroundImageAlias"})
    elif "WindowBounds" in p:
        print("  bwsp:", p)
if not has_bg:
    sys.exit("ERROR: .DS_Store failed verification")
PY

/usr/bin/osascript -e "tell application \"Finder\" to close container window of disk \"$VOL_NAME\"" >/dev/null 2>&1 || true
hdiutil detach "$ATTACHED_DEV" -force >/dev/null
ATTACHED_DEV=""
sleep 1

# Re-attach with -nobrowse so Finder does not overwrite .DS_Store during unmount, then restore .DS_Store and flags.
ATTACH_OUT2="$(hdiutil attach -readwrite -noverify -noautoopen -nobrowse "$RW_DMG")"
ATTACHED_DEV="$(echo "$ATTACH_OUT2" | awk '/^\/dev\// {print $1; exit}')"
MOUNT_DIR="$(echo "$ATTACH_OUT2" | awk '/\/Volumes\// {sub(/.*\/Volumes\//, "/Volumes/"); print; exit}')"

cp "$SAVED_DS" "$MOUNT_DIR/.DS_Store"
if [[ -f "$VOL_ICNS" ]]; then
  cp "$VOL_ICNS" "$MOUNT_DIR/.VolumeIcon.icns"
  if command -v SetFile >/dev/null 2>&1; then
    SetFile -c icnC "$MOUNT_DIR/.VolumeIcon.icns" || true
    SetFile -a C "$MOUNT_DIR" || true
  elif xcrun -f SetFile >/dev/null 2>&1; then
    xcrun SetFile -c icnC "$MOUNT_DIR/.VolumeIcon.icns" || true
    xcrun SetFile -a C "$MOUNT_DIR" || true
  fi
fi

chmod -Rf go-w "$MOUNT_DIR" >/dev/null 2>&1 || true
rm -rf "$MOUNT_DIR/.fseventsd" || true
sync
hdiutil detach "$ATTACHED_DEV" >/dev/null
ATTACHED_DEV=""

mkdir -p "$(dirname "$OUT_DMG")"
rm -f "$OUT_DMG"
echo "Compressing final DMG to $OUT_DMG..."
hdiutil convert "$RW_DMG" -format UDZO -imagekey zlib-level=9 -o "$OUT_DMG" >/dev/null

echo "Signing $OUT_DMG with $IDENTITY..."
codesign --force --sign "$IDENTITY" --timestamp "$OUT_DMG"
codesign --verify -vv "$OUT_DMG"

echo "Done: $OUT_DMG packaged with custom background & Finder layout."
