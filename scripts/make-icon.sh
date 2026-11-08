#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p target/AMSpace.iconset
swift scripts/icon.swift assets/icon.png
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" assets/icon.png --out "target/AMSpace.iconset/icon_${size}x${size}.png" >/dev/null
  double=$((size*2))
  sips -z "$double" "$double" assets/icon.png --out "target/AMSpace.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns target/AMSpace.iconset -o assets/AMSpace.icns
python3 scripts/optimize-icon.py assets/AMSpace.icns
