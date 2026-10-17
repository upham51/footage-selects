#!/bin/bash
# Export selects at every supported timeline rate, in both layouts, and read
# each export back with OpenTimelineIO (dev/validate_exports.py).
#
# Usage: dev/validate_matrix.sh <work folder> <python with opentimelineio>
# The synthetic clips come from the engine tests: run `cargo test -p selects_engine` first.
set -eu
REPO=$(cd "$(dirname "$0")/.." && pwd)
WORK=$1
PY=$2
SRC=$REPO/target/tmp/synthetic-footage
mkdir -p "$WORK"
cargo build -q --release -p selects_engine --example export_sample --manifest-path "$REPO/Cargo.toml"
fail=0
for spec in "24000/1001:0:23976" "24/1:0:24" "25/1:0:25" "30000/1001:0:2997ndf" "30000/1001:1:2997df" "30/1:0:30" "50/1:0:50" "60000/1001:1:5994df" "60/1:0:60"; do
  IFS=: read -r rate df tag <<< "$spec"
  dir=$WORK/shoot_$tag
  mkdir -p "$dir"
  tc=01:00:00:00
  [ "$df" = 1 ] && tc="01:00:00;00"
  for f in sharp sharp_blur_sharp panning; do
    [ -f "$dir/$f.mov" ] || ffmpeg -v error -y -i "$SRC/$f.mp4" -vf "fps=$rate" -c:v libx264 -preset ultrafast -timecode "$tc" "$dir/$f.mov"
  done
  skip=""
  case $rate in */1001) skip="--skip-fcpxml" ;; esac
  for layout in full selects; do
    "$REPO/target/release/examples/export_sample" "$dir" "$WORK/$tag-$layout" "$rate" "$df" "$layout" > /dev/null
    if out=$("$PY" "$REPO/dev/validate_exports.py" "$WORK/$tag-$layout" $skip 2>&1); then
      echo "$tag $layout: $(echo "$out" | tail -1)"
    else
      echo "$tag $layout: FAILED"; echo "$out"; fail=1
    fi
  done
done
exit $fail
