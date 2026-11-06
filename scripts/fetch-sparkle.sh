#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=2.10.1
SHA256=1a856d2a9edb8e628de88121d7be04cf0e2167f8641650b78fca00b14244cf27
CACHE="$PWD/.cache/sparkle"
ARCHIVE="$CACHE/Sparkle-$VERSION.tar.xz"
mkdir -p "$CACHE"
if [[ ! -f "$ARCHIVE" ]]; then
  curl --fail --location --proto '=https' --tlsv1.2 \
    "https://github.com/sparkle-project/Sparkle/releases/download/$VERSION/Sparkle-$VERSION.tar.xz" \
    -o "$ARCHIVE.download"
  mv "$ARCHIVE.download" "$ARCHIVE"
fi
printf '%s  %s\n' "$SHA256" "$ARCHIVE" | shasum -a 256 -c -
# Always extract from the verified archive, preserving symlinks and permissions.
tar -xJf "$ARCHIVE" -C "$CACHE"
