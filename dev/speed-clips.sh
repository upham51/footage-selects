#!/bin/bash
# Make 4K test clips that stand in for camera footage, for timing analysis:
#   dev/speed-clips.sh OUTDIR [seconds] [ffmpeg with libx264 and libx265]
# Noise keeps the bitrate near what cameras record; B-frames and long GOPs
# match how most cameras encode. These are for speed only: the test pattern
# has moving parts of its own, so do not judge stability scores from them.
set -euo pipefail
OUT=$(mkdir -p "$1" && cd "$1" && pwd); D=${2:-5}; FF=${3:-ffmpeg}
src() { echo "testsrc2=s=4096x2304:r=$1,crop=3840:2160:x='t*40':y=0,noise=alls=14:allf=t+u"; }
enc() { # name rate codec-args...
  local name=$1 rate=$2; shift 2
  [ -f "$OUT/$name.mp4" ] || "$FF" -hide_banner -y -loglevel error -f lavfi -i "$(src "$rate")" -t "$D" "$@" "$OUT/$name.mp4"
}
X264="bframes=2:b-pyramid=none:ref=2"
X265="bframes=4:b-pyramid=1:log-level=error"
enc h264_8bit_420_24   24000/1001  -c:v libx264 -preset ultrafast -x264-params "$X264:keyint=24"  -b:v 100M -pix_fmt yuv420p
enc h264_8bit_420_120  120000/1001 -c:v libx264 -preset ultrafast -x264-params "$X264:keyint=60"  -b:v 200M -pix_fmt yuv420p
enc h264_10bit_422_24  24000/1001  -c:v libx264 -preset ultrafast -x264-params "$X264:keyint=24"  -b:v 100M -pix_fmt yuv422p10le
enc hevc_10bit_420_24  24000/1001  -c:v libx265 -preset ultrafast -x265-params "$X265:keyint=24"  -b:v 100M -pix_fmt yuv420p10le -tag:v hvc1
enc hevc_10bit_420_120 120000/1001 -c:v libx265 -preset ultrafast -x265-params "$X265:keyint=120" -b:v 150M -pix_fmt yuv420p10le -tag:v hvc1
enc hevc_10bit_420_120_ponly 120000/1001 -c:v libx265 -preset ultrafast -x265-params "bframes=0:keyint=120:log-level=error" -b:v 150M -pix_fmt yuv420p10le -tag:v hvc1
enc hevc_10bit_422_24  24000/1001  -c:v libx265 -preset ultrafast -x265-params "$X265:keyint=24"  -b:v 100M -pix_fmt yuv422p10le -tag:v hvc1
[ -f "$OUT/hevc_10bit_420_24_vertical.mp4" ] || "$FF" -hide_banner -y -loglevel error -display_rotation 90 -i "$OUT/hevc_10bit_420_24.mp4" -c copy "$OUT/hevc_10bit_420_24_vertical.mp4"
ls -l "$OUT"
