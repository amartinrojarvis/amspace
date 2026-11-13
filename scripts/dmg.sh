#!/bin/bash
# Build only local distribution artifacts; never touch an installed app or updates/.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"
APP="$ROOT/dist/AMSpace.app"
if [[ $# -ne 0 ]]; then
  echo "Uso: ./scripts/dmg.sh (requiere dist/AMSpace.app y una sesión de Finder)" >&2
  exit 2
fi
if [[ ! -d "$APP" ]]; then
  echo "Compila primero: ./scripts/bundle.sh --universal" >&2
  exit 1
fi
codesign --verify --deep --strict "$APP"
VERSION=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")
WORK=$(mktemp -d "$ROOT/dist/.amspace-dmg.XXXXXX")
# Finder reduces a root-folder alias to a disk-name specifier. Its basename
# must be unique even when another application's installer is already mounted.
MOUNT="$WORK/AMSpace-${WORK##*.}"
ATTACHED=0
cleanup() {
  if [[ "$ATTACHED" == 1 ]]; then
    if ! hdiutil detach -quiet "$MOUNT"; then
      if ! hdiutil detach -quiet -force "$MOUNT"; then
        echo "No se pudo desmontar el volumen temporal: $MOUNT" >&2
        return
      fi
    fi
  fi
  rm -rf "$WORK"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir -p "$WORK/content/.background" "$MOUNT"
ditto "$APP" "$WORK/content/AMSpace.app"
ln -s /Applications "$WORK/content/Aplicaciones"
xcrun swift scripts/dmg-background.swift "$WORK/content/.background/background.png"
textutil -convert rtf -format html -output "$WORK/content/Léeme primero.rtf" assets/installer/LEEME.html
hdiutil create -quiet -volname "AMSpace $VERSION" -srcfolder "$WORK/content" -format UDRW -fs HFS+ "$WORK/writable.dmg"
ATTACHED=1
hdiutil attach -quiet -readwrite -nobrowse -mountpoint "$MOUNT" "$WORK/writable.dmg"
# Resolve the exact temporary mount, not a disk name that might match another image.
# Finder is needed only while packaging; consumers need no Automation permission.
osascript - "$MOUNT" <<'APPLESCRIPT'
on run argv
    set mountAlias to (POSIX file (item 1 of argv)) as alias
    set backgroundAlias to (POSIX file ((item 1 of argv) & "/.background/background.png")) as alias
    tell application "Finder"
        set installerFolder to folder mountAlias
        open installerFolder
        set installerWindow to container window of installerFolder
        set current view of installerWindow to icon view
        set toolbar visible of installerWindow to false
        set statusbar visible of installerWindow to false
        set bounds of installerWindow to {100, 80, 900, 722}
        set options to icon view options of installerWindow
        set arrangement of options to not arranged
        set icon size of options to 96
        set text size of options to 13
        set background picture of options to backgroundAlias
        set position of item "AMSpace.app" of installerFolder to {205, 210}
        set position of item "Aplicaciones" of installerFolder to {595, 210}
        set position of item "Léeme primero.rtf" of installerFolder to {683, 505}
        update installerFolder without registering applications
        delay 2
        -- A running window manager may resize Finder during the setup above.
        -- Save the intended frame immediately before closing, without a delay.
        set bounds of installerWindow to {100, 80, 900, 722}
        close installerWindow
    end tell
end run
APPLESCRIPT
if [[ ! -f "$MOUNT/.DS_Store" ]]; then
  echo "Finder no guardó la disposición del instalador; no se generará un DMG incompleto." >&2
  exit 1
fi
codesign --verify --deep --strict "$MOUNT/AMSpace.app"
hdiutil detach -quiet "$MOUNT"
ATTACHED=0
hdiutil convert -quiet "$WORK/writable.dmg" -format UDZO -imagekey zlib-level=9 -o "$WORK/AMSpace-installer.dmg"
hdiutil verify -quiet "$WORK/AMSpace-installer.dmg"
mv -f "$WORK/AMSpace-installer.dmg" "$ROOT/dist/AMSpace-installer.dmg"
echo "Creado dist/AMSpace-installer.dmg — arrastre a Aplicaciones, permisos y privacidad."
