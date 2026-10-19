#!/bin/bash
# Build the ffmpeg and ffprobe that ship inside the app, from FFmpeg's own
# source release, under the LGPL only.
#
#   scripts/build-ffmpeg.sh macos   OUTDIR   (run on a Mac; builds for that Mac's chip,
#                                             Apple silicon or Intel, which needs nasm)
#   scripts/build-ffmpeg.sh windows OUTDIR   (run on Linux; cross-compiles with mingw-w64)
#   scripts/build-ffmpeg.sh linux   OUTDIR   (for development and tests)
#
# No external libraries are linked apart from what the operating system
# provides (VideoToolbox on macOS, Direct3D video decoding on Windows) and
# zlib. GPL and non-free parts are never enabled, and the build stops if the
# finished configuration says otherwise. OUTDIR receives ffmpeg, ffprobe and
# BUILD_INFO.txt (the exact source, checksums and configure line).
set -euo pipefail

PLATFORM=${1:?platform: macos, windows or linux}
OUT=$(mkdir -p "${2:?output folder}" && cd "$2" && pwd)

FFMPEG_VERSION=9.0.2
FFMPEG_SHA256=8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e
ZLIB_VERSION=1.3.2
ZLIB_SHA256=bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16

WORK=${FFMPEG_WORK_DIR:-$(mktemp -d)}
mkdir -p "$WORK"
cd "$WORK"
JOBS=$(getconf _NPROCESSORS_ONLN 2>/dev/null || sysctl -n hw.ncpu)

sha256() { if command -v sha256sum >/dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi | cut -d' ' -f1; }

fetch() { # url file expected_sha256
  [ -f "$2" ] || curl -fsSL --retry 4 --retry-delay 5 -o "$2" "$1"
  local got; got=$(sha256 "$2")
  if [ "$got" != "$3" ]; then echo "Checksum mismatch for $2: expected $3, got $got" >&2; exit 1; fi
}

fetch "https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz" "ffmpeg-$FFMPEG_VERSION.tar.xz" "$FFMPEG_SHA256"
rm -rf "ffmpeg-$FFMPEG_VERSION" && tar xf "ffmpeg-$FFMPEG_VERSION.tar.xz"

# Shared by every platform. Everything here is part of FFmpeg itself and LGPL.
COMMON=(
  --enable-static --disable-shared
  --disable-autodetect          # nothing is picked up from the build machine by accident
  --disable-doc --disable-ffplay --disable-network --disable-debug
  --disable-indevs --enable-indev=lavfi --disable-outdevs
)

EXE=""
case "$PLATFORM" in
  macos)
    EXTRA=(--enable-pthreads --enable-zlib --enable-videotoolbox --enable-audiotoolbox)
    if [ "$(uname -m)" = arm64 ]; then
      EXTRA+=(--enable-neon)
    else
      # Intel: FFmpeg's fast x86 code is assembled with nasm (brew install nasm).
      command -v nasm >/dev/null || { echo "Install nasm first (brew install nasm)." >&2; exit 1; }
    fi
    EXTRA+=(--extra-cflags=-mmacosx-version-min=11.0 --extra-ldflags=-mmacosx-version-min=11.0)
    ;;
  linux)
    EXTRA=(--enable-pthreads --enable-zlib)
    ;;
  windows)
    EXE=.exe
    # Cross-compiled on Linux with mingw-w64 (Ubuntu package gcc-mingw-w64-x86-64).
    command -v x86_64-w64-mingw32-gcc >/dev/null || { echo "Install gcc-mingw-w64-x86-64 first." >&2; exit 1; }

    # zlib, built static for Windows.
    fetch "https://zlib.net/fossils/zlib-$ZLIB_VERSION.tar.gz" "zlib-$ZLIB_VERSION.tar.gz" "$ZLIB_SHA256"
    rm -rf "zlib-$ZLIB_VERSION" && tar xf "zlib-$ZLIB_VERSION.tar.gz"
    ZPREFIX="$WORK/zlib-win"
    (cd "zlib-$ZLIB_VERSION" &&
      make -f win32/Makefile.gcc -j"$JOBS" PREFIX=x86_64-w64-mingw32- libz.a >/dev/null &&
      mkdir -p "$ZPREFIX/include" "$ZPREFIX/lib" &&
      cp zlib.h zconf.h "$ZPREFIX/include/" && cp libz.a "$ZPREFIX/lib/")

    EXTRA=(--target-os=mingw32 --arch=x86_64 --enable-cross-compile
           --cross-prefix=x86_64-w64-mingw32-
           --enable-w32threads --disable-pthreads
           --enable-d3d11va --enable-dxva2 --enable-zlib
           --extra-cflags="-I$ZPREFIX/include" --extra-ldflags="-L$ZPREFIX/lib -static")
    ;;
  *) echo "Unknown platform $PLATFORM" >&2; exit 1 ;;
esac

cd "ffmpeg-$FFMPEG_VERSION"
CONFIGURE=(./configure --prefix="$WORK/install" "${COMMON[@]}" "${EXTRA[@]}")
"${CONFIGURE[@]}" > "$WORK/configure.log" 2>&1 || { tail -40 "$WORK/configure.log"; tail -60 ffbuild/config.log; exit 1; }

# The license gate: stop if anything GPL, version 3 or non-free was switched on.
for flag in GPL GPLV3 VERSION3 NONFREE; do
  if grep -q "^CONFIG_$flag=yes" ffbuild/config.mak; then
    echo "License check failed: CONFIG_$flag is on. The app may only ship an LGPL version 2.1 build." >&2
    exit 1
  fi
done
grep -q '^CONFIG_FFMPEG=yes' ffbuild/config.mak
echo "License check passed: GPL, version 3 and non-free parts are all off."

make -j"$JOBS" "ffmpeg$EXE" "ffprobe$EXE" > "$WORK/make.log" 2>&1 || { tail -60 "$WORK/make.log"; exit 1; }
install -m 755 "ffmpeg$EXE" "ffprobe$EXE" "$OUT/"
if [ "$PLATFORM" = macos ]; then
  # Apple silicon refuses to run unsigned code; an ad-hoc signature is enough, and harmless on Intel.
  codesign --force --sign - "$OUT/ffmpeg" "$OUT/ffprobe"
fi

{
  echo "FFmpeg $FFMPEG_VERSION, built for $PLATFORM$([ "$PLATFORM" = macos ] && echo " ($(uname -m))") under the GNU LGPL version 2.1 or later."
  echo "Source: https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz"
  echo "Source SHA-256: $FFMPEG_SHA256"
  if [ "$PLATFORM" = windows ]; then
    echo "zlib $ZLIB_VERSION (zlib license): https://zlib.net/fossils/zlib-$ZLIB_VERSION.tar.gz"
    echo "zlib SHA-256: $ZLIB_SHA256"
  fi
  echo "Configured with: ${CONFIGURE[*]:1}" | sed "s#$WORK#<build>#g"
  echo "ffmpeg SHA-256: $(sha256 "$OUT/ffmpeg$EXE")"
  echo "ffprobe SHA-256: $(sha256 "$OUT/ffprobe$EXE")"
} > "$OUT/BUILD_INFO.txt"
cat "$OUT/BUILD_INFO.txt"
