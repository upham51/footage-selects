#!/bin/bash
# Download pinned, checksum-verified ffmpeg and ffprobe for macOS on Apple silicon
# (static builds by Martin Riedl, https://ffmpeg.martin-riedl.de) and place them
# where the Tauri bundler expects sidecar binaries. Also copies them to dev-bin/
# so the engine tests run against the exact binaries that ship.
set -euo pipefail
BASE=https://ffmpeg.martin-riedl.de/download/macos/arm64/1789931890_9.0.2
TRIPLE=aarch64-apple-darwin
cd "$(dirname "$0")/.."
mkdir -p src-tauri/binaries dev-bin
tmp=$(mktemp -d)
for tool in ffmpeg ffprobe; do
  case $tool in
    ffmpeg) sum=c8ed4c4e6978a03c485edbfe4e0a5dc2380f8a30bba5150531b31b094492d924 ;;
    ffprobe) sum=fcbe839537485eaee7a7a8bc5cbc0f90d53617e80943e8a5b2e31cb851197ea6 ;;
  esac
  curl -fsSL --retry 4 --retry-delay 5 -o "$tmp/$tool.zip" "$BASE/$tool.zip"
  echo "$sum  $tmp/$tool.zip" | shasum -a 256 -c -
  unzip -o -q "$tmp/$tool.zip" -d "$tmp/$tool"
  install -m 755 "$tmp/$tool/$tool" "src-tauri/binaries/$tool-$TRIPLE"
  install -m 755 "$tmp/$tool/$tool" "dev-bin/$tool"
done
file src-tauri/binaries/ffmpeg-$TRIPLE
dev-bin/ffmpeg -hide_banner -version | head -1
dev-bin/ffmpeg -hide_banner -hwaccels
