#!/bin/bash
# Launch Services end-to-end check against a disposable app, never user documents.
set -euo pipefail
cd "$(dirname "$0")/.."
bin=${1:-dist/AMSpace.app/Contents/MacOS/amspace}
[[ -x "$bin" ]] || { echo "No se encuentra el bundle compilado: $bin" >&2; exit 1; }
display=$("$bin" --diagnose | awk '/^Visible desktops: \[\([0-9]+,/ {sub(/^[^(]*\(/, ""); sub(/,.*/, ""); print}')
[[ "$display" =~ ^[0-9]+$ ]] || { echo 'No hay un escritorio normal visible.' >&2; exit 1; }
# Launch Services may omit apps under TMPDIR even after explicit registration.
mkdir -p "$HOME/Applications"
root=$(mktemp -d "$HOME/Applications/AMSpace-scene.XXXXXX")
app="$root/SceneLaunchFixture.app"
id="io.github.amartinrojarvis.amspace.fixture.scene-$(uuidgen | tr A-Z a-z)"
lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
cleanup() {
  if [[ -f "$root/fixture.pid" ]]; then
    local pid
    pid=$(cat "$root/fixture.pid")
    if [[ "$pid" =~ ^[0-9]+$ && "$(ps -p "$pid" -o comm= || :)" == "$app/Contents/MacOS/SceneLaunchFixture" ]]; then
      kill -TERM "$pid" 2>/dev/null || :
    fi
  fi
  "$lsregister" -u "$app" >/dev/null 2>&1 || :
  rm -rf "$root"
}
trap cleanup EXIT
mkdir -p "$app/Contents/MacOS"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>SceneLaunchFixture</string>
<key>CFBundleIdentifier</key><string>$id</string>
<key>CFBundleName</key><string>AMSpace Scene Fixture</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>1</string>
<key>LSUIElement</key><true/>
<key>AMSpaceFixtureDisplay</key><integer>$display</integer>
</dict></plist>
PLIST
/usr/bin/xcrun clang -fobjc-arc -framework AppKit scripts/scene-launch-fixture.m -o "$app/Contents/MacOS/SceneLaunchFixture"
codesign --force --sign - "$app"
"$lsregister" -f "$app"
AMSPACE_SCENE_LAUNCH_FIXTURE="$id" "$bin" --check-work-scenes
