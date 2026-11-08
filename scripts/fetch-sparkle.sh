#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=2.10.1
# The API route serves the same pinned release asset if the download host times out.
ASSET_ID=673946481
SHA256=1a856d2a9edb8e628de88121d7be04cf0e2167f8641650b78fca00b14244cf27
CACHE="$PWD/.cache/sparkle"
ARCHIVE="$CACHE/Sparkle-$VERSION.tar.xz"
mkdir -p "$CACHE"
if [[ ! -f "$ARCHIVE" ]]; then
  if ! curl --fail --location --proto '=https' --tlsv1.2 \
    --retry 2 --retry-delay 2 --retry-max-time 60 --connect-timeout 15 --max-time 30 \
    "https://github.com/sparkle-project/Sparkle/releases/download/$VERSION/Sparkle-$VERSION.tar.xz" \
    -o "$ARCHIVE.download"; then
    curl --fail --location --proto '=https' --tlsv1.2 \
      --retry 2 --retry-delay 2 --retry-max-time 60 --connect-timeout 15 --max-time 30 \
      --header 'Accept: application/octet-stream' \
      "https://api.github.com/repos/sparkle-project/Sparkle/releases/assets/$ASSET_ID" \
      -o "$ARCHIVE.download"
  fi
  printf '%s  %s\n' "$SHA256" "$ARCHIVE.download" | shasum -a 256 -c -
  mv "$ARCHIVE.download" "$ARCHIVE"
fi
printf '%s  %s\n' "$SHA256" "$ARCHIVE" | shasum -a 256 -c -
# Always extract from the verified archive, preserving symlinks and permissions.
tar -xJf "$ARCHIVE" -C "$CACHE"
