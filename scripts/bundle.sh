#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
export MACOSX_DEPLOYMENT_TARGET=13.0
mkdir -p dist
./scripts/fetch-sparkle.sh
BUNDLE_STAGE="$(mktemp -d dist/.amspace.XXXXXX)"
trap 'rm -rf "$BUNDLE_STAGE"' EXIT
APP="$BUNDLE_STAGE/AMSpace.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
if [[ "${1:-}" == "--universal" ]]; then
  cargo build --locked --release --target aarch64-apple-darwin
  cargo build --locked --release --target x86_64-apple-darwin
  lipo -create target/aarch64-apple-darwin/release/amspace target/x86_64-apple-darwin/release/amspace -output "$APP/Contents/MacOS/amspace"
else
  cargo build --locked --release
  cp target/release/amspace "$APP/Contents/MacOS/amspace"
fi
cp LICENSE "$APP/Contents/Resources/LICENSE.txt"
cp docs/ISS-LICENSE.txt "$APP/Contents/Resources/ISS-LICENSE.txt"
cp docs/STRAFE-LICENSE.txt "$APP/Contents/Resources/STRAFE-LICENSE.txt"
cp docs/YABAI-LICENSE.txt "$APP/Contents/Resources/YABAI-LICENSE.txt"
cp docs/SPARKLE-LICENSE.txt "$APP/Contents/Resources/SPARKLE-LICENSE.txt"
cp assets/Info.plist "$APP/Contents/Info.plist"
for language in es en; do
  ditto "assets/localizations/$language.lproj" "$APP/Contents/Resources/$language.lproj"
done
if [[ ! -f assets/AMSpace.icns ]]; then ./scripts/make-icon.sh; fi
cp assets/AMSpace.icns "$APP/Contents/Resources/AMSpace.icns"
mkdir -p "$APP/Contents/Frameworks"
ditto .cache/sparkle/Sparkle.framework "$APP/Contents/Frameworks/Sparkle.framework"
codesign --force --deep --sign "${SIGNING_IDENTITY:--}" --identifier io.github.amartinrojarvis.amspace "$APP"
codesign --verify --deep --strict "$APP"
if [[ -e dist/AMSpace.app ]]; then mv dist/AMSpace.app "$BUNDLE_STAGE/previous.app"; fi
mv "$APP" dist/AMSpace.app
echo "Built dist/AMSpace.app"
