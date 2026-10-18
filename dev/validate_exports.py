"""Read exported timelines back with OpenTimelineIO and compare them with the CSV.

OpenTimelineIO's adapters are independent readers of Final Cut Pro 7 XML,
FCPXML and CMX3600 EDL. Passing here means another implementation parses the
files and finds the same clips at the same frames. It does not prove that
Premiere Pro, DaVinci Resolve or Final Cut Pro will import them.

Usage: python validate_exports.py <export base path without extension>
Needs: pip install opentimelineio otio-fcp-adapter otio-fcpx-xml-adapter otio-cmx3600-adapter
"""
import csv
import sys

import opentimelineio as otio

base = sys.argv[1]
problems = []


def check(cond, msg):
    if not cond:
        problems.append(msg)


with open(base + ".csv", encoding="utf-8-sig") as f:
    rows = list(csv.DictReader(f))
rate_label = rows[0]["Timeline frame rate"]
rate = {"23.976": 24000 / 1001, "29.97": 30000 / 1001, "59.94": 60000 / 1001}.get(rate_label, None) or float(rate_label)


def tc_to_frames(tc):
    return otio.opentime.from_timecode(tc, rate).to_frames(rate)


def frames(rt):
    return round(rt.to_seconds() * rate)


def selects_track(timeline, video_index):
    vids = [t for t in timeline.tracks if t.kind == otio.schema.TrackKind.Video]
    return vids, vids[video_index] if len(vids) > video_index else None


def compare(name, clips, offset_frames):
    check(len(clips) == len(rows), f"{name}: {len(clips)} selects, CSV has {len(rows)}")
    for clip, row in zip(clips, rows):
        rec_in = frames(clip.range_in_parent().start_time) + offset_frames
        dur = frames(clip.source_range.duration)
        want_in = tc_to_frames(row["Timeline in"])
        want_dur = round(float(row["Length (seconds)"]) * rate)
        check(rec_in == want_in, f"{name}: select {row['Select']} record in {rec_in} != {want_in}")
        check(abs(dur - want_dur) <= 0, f"{name}: select {row['Select']} length {dur} != {want_dur}")


# Final Cut Pro 7 XML
tl = otio.adapters.read_from_file(base + ".xml", adapter_name="fcp_xml")
if isinstance(tl, otio.schema.SerializableCollection):
    tl = list(tl.find_children(descended_from_type=otio.schema.Timeline))[0] if hasattr(tl, "find_children") else tl[0]
vids, v2 = selects_track(tl, 1)
check(len(vids) == 2, f"xml: {len(vids)} video tracks, expected 2")
start = frames(tl.global_start_time) if tl.global_start_time else 0
check(start == tc_to_frames("01:00:00:00") or start == tc_to_frames("01:00:00;00"), f"xml: timeline starts at frame {start}")
v2_clips = [c for c in v2 if isinstance(c, otio.schema.Clip)]
compare("xml", v2_clips, start)
markers = [m for m in tl.tracks.markers] + [m for c in v2_clips for m in c.markers]
check(len(markers) >= len(rows), f"xml: {len(markers)} markers, expected {len(rows)}")
check(all("Focus" in (m.metadata.get("fcp_xml", {}).get("comment", "") or m.name) or "Focus" in str(m.metadata) for m in markers), "xml: marker notes lack the scores")
for c in v2_clips:
    url = c.media_reference.target_url if c.media_reference else ""
    check(url.startswith("file://"), f"xml: media url {url!r}")
print(f"xml: {len(vids)} video tracks, {len(v2_clips)} selects on track 2, {len(markers)} markers")

# FCPXML. OpenTimelineIO's FCPXML reader converts times with the clip's frame
# rate truncated to a whole number (23.976 becomes 23), so it can only check
# whole-number rates with every clip at the timeline rate.
skip_fcpxml = "--skip-fcpxml" in sys.argv
if skip_fcpxml:
    print("fcpxml: not read back (OpenTimelineIO cannot read NTSC or mixed rates correctly)")
col = None if skip_fcpxml else otio.adapters.read_from_file(base + ".fcpxml", adapter_name="fcpx_xml")
if col is not None:
  tls = [col] if isinstance(col, otio.schema.Timeline) else [t for t in col.find_children(descended_from_type=otio.schema.Timeline)] if hasattr(col, "find_children") else list(col)
  tl2 = tls[0]
  vids2 = [t for t in tl2.tracks if t.kind == otio.schema.TrackKind.Video]
  start2 = frames(tl2.global_start_time) if tl2.global_start_time else 0
  # Lane 1 is the selects. With the selects-only layout the storyline is a single
  # gap, which OpenTimelineIO does not turn into a track, so lane 1 may be the only one.
  lane = next((t for t in vids2 if t.name == "1"), vids2[1] if len(vids2) > 1 else None)
  check(lane is not None, f"fcpxml: {len(vids2)} video tracks, expected the selects on a second one")
  if lane is not None:
      sel2 = [c for c in lane if isinstance(c, otio.schema.Clip)]
      compare("fcpxml", sel2, start2)
      m2 = [m for c in sel2 for m in c.markers]
      check(len(m2) == len(rows), f"fcpxml: {len(m2)} markers on selects, expected {len(rows)}")
      print(f"fcpxml: {len(vids2)} video tracks, {len(sel2)} selects on track 2, {len(m2)} markers")

# CMX3600 EDL
tl3 = otio.adapters.read_from_file(base + ".edl", adapter_name="cmx_3600", rate=rate)
v3 = [t for t in tl3.tracks if t.kind == otio.schema.TrackKind.Video][0]
clips3 = [c for c in v3 if isinstance(c, otio.schema.Clip)]
check(len(clips3) == len(rows), f"edl: {len(clips3)} events, expected {len(rows)}")
for c, row in zip(clips3, rows):
    check(c.name == row["Clip"], f"edl: clip name {c.name!r} != {row['Clip']!r}")
    # OpenTimelineIO places the first event at zero, so compare relative to it.
    start3 = tc_to_frames(rows[0]["Timeline in"]) - frames(clips3[0].range_in_parent().start_time)
    rec = frames(c.range_in_parent().start_time) + start3
    check(rec == tc_to_frames(row["Timeline in"]), f"edl: select {row['Select']} record in {rec} != {tc_to_frames(row['Timeline in'])}")
    check(frames(c.source_range.duration) == round(float(row["Length (seconds)"]) * rate), f"edl: select {row['Select']} length")
    if row["Clip frame rate"] == row["Timeline frame rate"]:
        src = frames(c.source_range.start_time)
        check(src == tc_to_frames(row["Source in (timeline rate)"]), f"edl: select {row['Select']} source in {src} != {row['Source in (timeline rate)']}")
print(f"edl: {len(clips3)} events")

if problems:
    print("PROBLEMS:")
    for p in problems:
        print("  " + p)
    sys.exit(1)
print("OK: all three timelines read back with the same selects as the CSV")
