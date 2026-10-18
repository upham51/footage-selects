//! Export writers: frame rate and timecode maths, track placement, marker
//! contents, and the published structure of each format.

use roxmltree::{Document, Node};
use selects_engine::analyze::ClipAnalysis;
use selects_engine::export::fcpxml::Time;
use selects_engine::export::{build_timeline, csv, edl, fcp7, fcpxml, file_url, write_all, Timeline};
use selects_engine::probe::ClipInfo;
use selects_engine::project::{Project, TimelineLayout};
use selects_engine::rate::{Rate, TIMELINE_RATES};
use selects_engine::scores::ScoreSeries;
use selects_engine::selection::{ClipSelection, Segment, SegmentStatus};

fn info(name: &str, rate: Rate, duration: f64, tc: Option<&str>, audio: bool) -> ClipInfo {
    ClipInfo {
        path: format!("/Volumes/Shoot 1/Day 1/{name}"),
        name: name.into(),
        size: 1,
        modified: 1,
        duration,
        start_time: 0.0,
        width: 1920,
        height: 1080,
        coded_width: 1920,
        coded_height: 1080,
        rotation: 0,
        avg_fps: rate.fps(),
        rate,
        variable_frame_rate: false,
        codec: "h264".into(),
        pix_fmt: "yuv420p".into(),
        has_audio: audio,
        audio_channels: if audio { 2 } else { 0 },
        audio_sample_rate: if audio { 48000 } else { 0 },
        timecode: tc.map(|s| s.into()),
        full_range: false,
        color_transfer: String::new(),
        detected_profile: None,
        camera: None,
        tags: Default::default(),
    }
}

fn seg(start: f64, end: f64, f: f32, e: f32, s: f32) -> Segment {
    Segment {
        start,
        end,
        status: SegmentStatus::Kept,
        focus: f,
        exposure: e,
        stability: s,
        score: (f + e + s) / 3.0,
        failed: vec![],
        exposure_issue: None,
        key_stops: None,
        camera_move: false,
        camera: Default::default(),
        aside: None,
        similar_to: None,
        ..Default::default()
    }
}

fn selection(segs: Vec<Segment>) -> ClipSelection {
    ClipSelection {
        smoothed: ScoreSeries::default(),
        segments: segs,
    }
}

/// Two clips at the given rate plus one at 25 fps, with known selects.
fn project(rate: Rate, df: Option<bool>, layout: TimelineLayout) -> (Project, Vec<ClipSelection>) {
    let mut p = Project::new();
    let tc = if rate.supports_drop_frame() { "01:00:00;00" } else { "01:00:00:00" };
    for (name, r, dur, tc, audio) in [
        ("A001_C001.mov", rate, 10.0, Some(tc), true),
        ("A001_C002.mov", rate, 6.0, None, false),
        ("B-cam, \"wide\".mp4", Rate::new(25, 1), 8.0, Some("10:00:00:00"), true),
    ] {
        let id = p.add_clip(&format!("/Volumes/Shoot 1/Day 1/{name}"), Ok(info(name, r, dur, tc, audio))).unwrap();
        let mut a = ClipAnalysis::empty(4.0, 480, 270);
        a.complete = true;
        p.clip_mut(id).unwrap().analysis = Some(a);
    }
    p.export.timeline_rate = Some(rate);
    p.export.drop_frame = df;
    p.export.layout = layout;
    p.export.timeline_name = "Day 1 selects".into();
    let sels = vec![
        selection(vec![seg(2.0, 5.0, 0.82, 0.74, 0.91), seg(7.5, 9.0, 0.6, 0.6, 0.7)]),
        selection(vec![]),
        selection(vec![seg(1.0, 3.5, 0.9, 0.9, 0.9)]),
    ];
    (p, sels)
}

fn timeline(rate: Rate, df: Option<bool>, layout: TimelineLayout) -> Timeline {
    let (p, sels) = project(rate, df, layout);
    build_timeline(&p, &sels).0
}

fn parse(xml: &str) -> Result<Document<'_>, roxmltree::Error> {
    let opts = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    Document::parse_with_options(xml, opts)
}

fn child<'a>(n: Node<'a, 'a>, tag: &str) -> Node<'a, 'a> {
    n.children()
        .find(|c| c.has_tag_name(tag))
        .unwrap_or_else(|| panic!("<{}> has no <{tag}>", n.tag_name().name()))
}
fn text<'a>(n: Node<'a, 'a>, tag: &str) -> &'a str {
    child(n, tag).text().unwrap_or("")
}
fn items<'a>(t: Node<'a, 'a>) -> Vec<Node<'a, 'a>> {
    t.children().filter(|c| c.has_tag_name("clipitem")).collect()
}
fn num(n: Node, tag: &str) -> i64 {
    text(n, tag).trim().parse().unwrap()
}

/* ---------------- timeline model ---------------- */

#[test]
fn track_placement_full_clips_below() {
    let r = Rate::new(24000, 1001);
    let tl = timeline(r, None, TimelineLayout::FullClipsBelow);
    // Track 1 has every clip back to back.
    assert_eq!(tl.base.len(), 3);
    assert_eq!(tl.base[0].record_in, 0);
    assert_eq!(tl.base[0].duration, 239); // floor(10 s x 23.976)
    assert_eq!(tl.base[1].record_in, 239);
    assert_eq!(tl.base[1].duration, 143); // floor(6 s x 23.976)
    assert_eq!(tl.base[2].record_in, 382);
    assert_eq!(tl.duration, 382 + 191); // 8 s at 25 fps is 200 frames, 191 at 23.976
    // Track 2 has each select directly above its source frames.
    assert_eq!(tl.selects.len(), 3);
    let s = &tl.selects[0];
    assert_eq!((s.source_in, s.record_in, s.duration), (48, 48, 120 - 48)); // 2.0 s and 5.0 s
    let s = &tl.selects[1];
    assert_eq!((s.source_in, s.record_in), (180, 180)); // 7.5 s
    let s = &tl.selects[2];
    assert_eq!(s.record_in, 382 + s.source_in);
    assert_eq!(s.source_in, 24); // 1.0 s
    // In the 25 fps clip's own frames, 1.0 s is frame 25.
    assert_eq!(s.source_in_native, 25);
    // Every select sits inside its source on track 1.
    for s in &tl.selects {
        let b = tl.base.iter().find(|b| b.source == s.source).unwrap();
        assert!(s.record_in >= b.record_in && s.record_out() <= b.record_out());
        assert_eq!(s.record_in - b.record_in, s.source_in);
    }
    // Numbered in timeline order.
    let numbers: Vec<usize> = tl.selects.iter().map(|s| s.select.as_ref().unwrap().number).collect();
    assert_eq!(numbers, vec![1, 2, 3]);
}

#[test]
fn track_placement_selects_only() {
    let tl = timeline(Rate::new(25, 1), None, TimelineLayout::SelectsOnly);
    assert!(tl.base.is_empty());
    assert_eq!(tl.selects.len(), 3);
    let mut at = 0;
    for s in &tl.selects {
        assert_eq!(s.record_in, at);
        at += s.duration;
    }
    assert_eq!(tl.duration, at);
    // The clip without selects is left out entirely.
    assert_eq!(tl.sources.len(), 2);
}

#[test]
fn timeline_rate_defaults_to_the_most_common_rate() {
    let mut p = Project::new();
    for (i, r) in [Rate::new(25, 1), Rate::new(30000, 1001), Rate::new(30000, 1001), Rate::new(24, 1)].iter().enumerate() {
        p.add_clip(&format!("/x/{i}.mov"), Ok(info(&format!("{i}.mov"), *r, 5.0, None, false)));
    }
    assert_eq!(p.timeline_rate(), Rate::new(30000, 1001));
    assert!(!p.timeline_drop_frame()); // no drop frame timecode on the clips
    p.clips[1].info.as_mut().unwrap().timecode = Some("01:00:00;00".into());
    p.clips[2].info.as_mut().unwrap().timecode = Some("02:00:00;00".into());
    assert!(p.timeline_drop_frame()); // most 29.97 clips carry drop frame timecode
    p.export.drop_frame = Some(false);
    assert!(!p.timeline_drop_frame());
    p.export.timeline_rate = Some(Rate::new(24, 1));
    assert_eq!(p.timeline_rate(), Rate::new(24, 1));
    // 120 fps slow motion clips play in a 30 fps family timeline by default.
    let mut q = Project::new();
    q.add_clip("/x/hfr.mov", Ok(info("hfr.mov", Rate::new(120000, 1001), 5.0, None, false)));
    assert_eq!(q.timeline_rate(), Rate::new(30000, 1001));
}

/* ---------------- Final Cut Pro 7 XML ---------------- */

#[test]
fn fcp7_xml_rates_timecode_tracks_and_markers_at_every_rate() {
    for &r in TIMELINE_RATES {
        for df in [false, true] {
            if df && !r.supports_drop_frame() {
                continue;
            }
            let tl = timeline(r, Some(df), TimelineLayout::FullClipsBelow);
            let xml = fcp7::write(&tl);
            let doc = parse(&xml).unwrap_or_else(|e| panic!("{r}: {e}"));
            let root = doc.root_element();
            assert_eq!(root.tag_name().name(), "xmeml");
            assert_eq!(root.attribute("version"), Some("4"));
            let seq = child(root, "sequence");
            assert_eq!(text(seq, "name"), "Day 1 selects");
            let rate = child(seq, "rate");
            assert_eq!(num(rate, "timebase"), r.timebase() as i64);
            assert_eq!(text(rate, "ntsc"), if r.ntsc() { "TRUE" } else { "FALSE" });
            let tc = child(seq, "timecode");
            let expect = if df { "01:00:00;00" } else { "01:00:00:00" };
            assert_eq!(text(tc, "string"), expect, "{r} df {df}");
            let hour_frames = match (r.timebase(), df) {
                (30, true) => 107_892,
                (60, true) => 215_784,
                (tb, _) => tb as i64 * 3600,
            };
            assert_eq!(num(tc, "frame"), hour_frames, "{r} df {df}");
            assert_eq!(text(tc, "displayformat"), if df { "DF" } else { "NDF" });
            assert_eq!(num(seq, "duration"), tl.duration);

            let video = child(child(seq, "media"), "video");
            let fmt = child(child(video, "format"), "samplecharacteristics");
            assert_eq!((num(fmt, "width"), num(fmt, "height")), (1920, 1080));
            let tracks: Vec<_> = video.children().filter(|c| c.has_tag_name("track")).collect();
            assert_eq!(tracks.len(), 2, "video track 1 and 2");
            let v1 = items(tracks[0]);
            let v2 = items(tracks[1]);
            assert_eq!(v1.len(), 3);
            assert_eq!(v2.len(), 3, "selects on video track 2");
            for (item, placed) in v2.iter().zip(&tl.selects) {
                assert_eq!(num(*item, "start"), placed.record_in);
                assert_eq!(num(*item, "end"), placed.record_out());
                assert_eq!(num(*item, "in"), placed.source_in);
                assert_eq!(num(*item, "out"), placed.source_out());
                assert_eq!(num(child(*item, "rate"), "timebase"), r.timebase() as i64);
                // Directly above its source on track 1: same file, same source frame.
                let file_id = child(*item, "file").attribute("id").unwrap();
                let base = v1.iter().find(|b| child(**b, "file").attribute("id") == Some(file_id)).unwrap();
                assert_eq!(num(*item, "start") - num(*base, "start"), num(*item, "in") - num(*base, "in"));
                let label = text(child(*item, "labels"), "label2");
                assert!(["Forest", "Cerulean", "Mango"].contains(&label));
            }
            // Each file is described once in full, then referenced by id.
            let files: Vec<_> = doc.descendants().filter(|n| n.has_tag_name("file")).collect();
            let full: Vec<_> = files.iter().filter(|f| f.children().any(|c| c.has_tag_name("pathurl"))).collect();
            assert_eq!(full.len(), 3);
            let pathurl = text(*full[0], "pathurl");
            assert_eq!(pathurl, "file://localhost/Volumes/Shoot%201/Day%201/A001_C001.mov");
            // The 25 fps file keeps its own rate and timecode.
            let b = full.iter().find(|f| text(***f, "name").starts_with("B-cam")).unwrap();
            assert_eq!(num(child(**b, "rate"), "timebase"), 25);
            assert_eq!(text(child(**b, "rate"), "ntsc"), "FALSE");
            assert_eq!(num(**b, "duration"), 200);
            assert_eq!(text(child(**b, "timecode"), "string"), "10:00:00:00");
            assert_eq!(num(child(**b, "timecode"), "frame"), 900_000);

            // Markers: one per select, named with its strength, noted with the three scores.
            let markers: Vec<_> = seq.children().filter(|c| c.has_tag_name("marker")).collect();
            assert_eq!(markers.len(), 3);
            assert_eq!(text(markers[0], "name"), "Select 1: strong");
            assert_eq!(
                text(markers[0], "comment"),
                "Focus 82, exposure 74, stability 91 (overall 82)."
            );
            assert_eq!(num(markers[0], "in"), tl.selects[0].record_in);
            assert_eq!(text(markers[1], "name"), "Select 2: fair");
            assert!(text(markers[1], "comment").starts_with("Focus 60, exposure 60, stability 70"));
            assert_eq!(text(markers[2], "name"), "Select 3: strong");
            assert_ne!(text(markers[0], "pproColor"), text(markers[1], "pproColor"));

            // Audio follows the full clips, two mono tracks for stereo sources.
            let audio = child(child(seq, "media"), "audio");
            let atracks: Vec<_> = audio.children().filter(|c| c.has_tag_name("track")).collect();
            assert_eq!(atracks.len(), 2);
            assert_eq!(items(atracks[0]).len(), 2, "two of the three clips have sound");
        }
    }
}

#[test]
fn fcp7_xml_selects_only_has_an_empty_track_one() {
    let tl = timeline(Rate::new(24, 1), None, TimelineLayout::SelectsOnly);
    let xml = fcp7::write(&tl);
    let doc = parse(&xml).unwrap();
    let video = doc.descendants().find(|n| n.has_tag_name("video") && n.parent().unwrap().has_tag_name("media") && n.parent().unwrap().parent().unwrap().has_tag_name("sequence")).unwrap();
    let tracks: Vec<_> = video.children().filter(|c| c.has_tag_name("track")).collect();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].children().filter(|c| c.has_tag_name("clipitem")).count(), 0);
    let v2: Vec<_> = tracks[1].children().filter(|c| c.has_tag_name("clipitem")).collect();
    assert_eq!(v2.len(), 3);
    assert_eq!(num(v2[0], "start"), 0);
    assert_eq!(num(v2[1], "start"), num(v2[0], "end"));
}

/* ---------------- FCPXML ---------------- */

/// What Apple's FCPXML reference allows as children of each element we write.
fn allowed_children(tag: &str) -> &'static [&'static str] {
    match tag {
        "fcpxml" => &["resources", "library"],
        "resources" => &["format", "asset"],
        "asset" => &["media-rep"],
        "library" => &["event"],
        "event" => &["asset-clip", "project"],
        "project" => &["sequence"],
        "sequence" => &["spine"],
        "spine" => &["asset-clip", "gap"],
        "asset-clip" | "gap" => &["adjust-conform", "asset-clip", "marker", "keyword", "rating"],
        _ => &[],
    }
}

fn required_attributes(tag: &str) -> &'static [&'static str] {
    match tag {
        "fcpxml" => &["version"],
        "format" => &["id", "frameDuration", "width", "height"],
        "asset" => &["id", "name", "start", "duration", "hasVideo", "format"],
        "media-rep" => &["kind", "src"],
        "project" => &["name"],
        "sequence" => &["format", "duration", "tcStart", "tcFormat"],
        "asset-clip" => &["ref", "offset", "name", "start", "duration"],
        "gap" => &["offset", "start", "duration"],
        "marker" => &["start", "duration", "value"],
        "keyword" | "rating" => &["start", "duration", "value"],
        _ => &[],
    }
}

fn t(n: Node, a: &str) -> Time {
    Time::parse(n.attribute(a).unwrap_or_else(|| panic!("{} lacks {a}", n.tag_name().name())))
        .unwrap_or_else(|| panic!("bad time {:?}", n.attribute(a)))
}

/// Check one FCPXML document against the published structure and our layout.
fn check_fcpxml(tl: &Timeline, xml: &str) {
    let doc = parse(xml).unwrap();
    let root = doc.root_element();
    assert_eq!(root.tag_name().name(), "fcpxml");
    assert_eq!(root.attribute("version"), Some("1.9"));
    let mut ids = std::collections::HashSet::new();
    for n in doc.descendants().filter(|n| n.is_element()) {
        let tag = n.tag_name().name();
        // An audio-only asset (a reel's music) has no picture format.
        let audio_only = tag == "asset" && n.attribute("hasVideo").is_none() && n.attribute("hasAudio") == Some("1");
        for a in required_attributes(tag) {
            if audio_only && (*a == "hasVideo" || *a == "format") {
                continue;
            }
            assert!(n.attribute(*a).is_some(), "<{tag}> must have {a}");
        }
        for c in n.children().filter(|c| c.is_element()) {
            assert!(
                allowed_children(tag).contains(&c.tag_name().name()),
                "<{}> is not allowed inside <{tag}>",
                c.tag_name().name()
            );
        }
        if let Some(id) = n.attribute("id") {
            assert!(ids.insert(id.to_string()), "duplicate id {id}");
        }
        // Every time attribute is a rational number of seconds ending in "s".
        for a in ["offset", "start", "duration", "tcStart", "frameDuration"] {
            if let Some(v) = n.attribute(a) {
                assert!(Time::parse(v).is_some(), "{tag} {a}={v}");
            }
        }
    }
    let resources = child(root, "resources");
    let format = |id: &str| resources.children().find(|c| c.has_tag_name("format") && c.attribute("id") == Some(id)).unwrap();
    let asset = |id: &str| resources.children().find(|c| c.has_tag_name("asset") && c.attribute("id") == Some(id)).unwrap();
    // Every reference points at a resource that exists.
    for n in doc.descendants().filter(|n| n.is_element()) {
        if let Some(f) = n.attribute("format") {
            format(f);
        }
        if let Some(r) = n.attribute("ref") {
            asset(r);
        }
    }
    let r = tl.rate;
    let seq = doc.descendants().find(|n| n.has_tag_name("sequence")).unwrap();
    let fd = Time::parse(format(seq.attribute("format").unwrap()).attribute("frameDuration").unwrap()).unwrap();
    assert_eq!(fd, Time::frames(1, r));
    assert_eq!(t(seq, "tcStart"), Time::frames(tl.start_frame, r));
    assert_eq!(t(seq, "duration"), Time::frames(tl.duration, r));
    assert_eq!(seq.attribute("tcFormat"), Some(if tl.drop_frame { "DF" } else { "NDF" }));

    let spine = child(seq, "spine");
    let top: Vec<_> = spine.children().filter(|c| c.is_element()).collect();
    // The primary storyline is contiguous and starts at the timeline start.
    let mut expect = t(seq, "tcStart");
    for item in &top {
        assert_eq!(t(*item, "offset"), expect);
        assert!(t(*item, "offset").on_frame(r) && t(*item, "duration").on_frame(r), "edit frame boundary");
        expect = expect.add(t(*item, "duration"));
    }
    // Connected selects on lane 1.
    let mut found = 0;
    for parent in &top {
        // The music under a reel sits on lane -1 and is checked separately.
        for c in parent.children().filter(|c| c.has_tag_name("asset-clip") && c.attribute("lane") != Some("-1")) {
            found += 1;
            assert_eq!(c.attribute("lane"), Some("1"));
            assert!(t(c, "duration").on_frame(r));
            // Where the select lands on the timeline, in timeline frames.
            let rel = Time::new(
                t(c, "offset").num * t(*parent, "start").den - t(*parent, "start").num * t(c, "offset").den,
                t(c, "offset").den * t(*parent, "start").den,
            );
            assert!(rel.on_frame(r), "connected offset on a timeline frame");
            let record = t(*parent, "offset").add(rel);
            let placed = &tl.selects[found - 1];
            assert_eq!(record, Time::frames(tl.start_frame + placed.record_in, r));
            assert_eq!(t(c, "duration"), Time::frames(placed.duration, r));
            // The source point is on the clip's own frame grid, inside the media.
            let a = asset(c.attribute("ref").unwrap());
            let native = Time::parse(format(a.attribute("format").unwrap()).attribute("frameDuration").unwrap()).unwrap();
            let start = t(c, "start");
            let rel_start = Time::new(start.num * t(a, "start").den - t(a, "start").num * start.den, start.den * t(a, "start").den);
            assert!(rel_start.num >= 0);
            assert_eq!((rel_start.num * native.den) % (rel_start.den * native.num), 0, "start on source frame");
            assert!(t(a, "start").add(t(a, "duration")).num * start.den > start.num * t(a, "start").add(t(a, "duration")).den);
            // One marker at the select's first frame, with the three scores.
            let markers: Vec<_> = c.children().filter(|m| m.has_tag_name("marker")).collect();
            assert_eq!(markers.len(), 1);
            let m = markers[0];
            assert_eq!(t(m, "start"), start);
            let meta = placed.select.as_ref().unwrap();
            assert_eq!(m.attribute("value").unwrap(), meta.marker_name());
            assert!(m.attribute("note").unwrap().starts_with(&format!(
                "Focus {}, exposure {}, stability {}",
                (meta.focus * 100.0).round(),
                (meta.exposure * 100.0).round(),
                (meta.stability * 100.0).round()
            )));
            let completed = m.attribute("completed");
            match meta.tier.word() {
                "strong" => assert_eq!(completed, Some("1")),
                "good" => assert_eq!(completed, None),
                _ => assert_eq!(completed, Some("0")),
            }
        }
    }
    assert_eq!(found, tl.selects.len());
}

#[test]
fn fcpxml_structure_times_and_markers_at_every_rate() {
    for &r in TIMELINE_RATES {
        for df in [false, true] {
            if df && !r.supports_drop_frame() {
                continue;
            }
            for layout in [TimelineLayout::FullClipsBelow, TimelineLayout::SelectsOnly] {
                let tl = timeline(r, Some(df), layout);
                check_fcpxml(&tl, &fcpxml::write(&tl));
            }
        }
    }
}

#[test]
fn fcpxml_frame_durations_and_timecode_starts() {
    let expect = [
        (Rate::new(24000, 1001), "1001/24000s"),
        (Rate::new(24, 1), "1/24s"),
        (Rate::new(25, 1), "1/25s"),
        (Rate::new(30000, 1001), "1001/30000s"),
        (Rate::new(30, 1), "1/30s"),
        (Rate::new(60000, 1001), "1001/60000s"),
    ];
    for (r, fd) in expect {
        let tl = timeline(r, Some(false), TimelineLayout::FullClipsBelow);
        let xml = fcpxml::write(&tl);
        assert!(xml.contains(&format!("id=\"r1\" frameDuration=\"{fd}\"")), "{r}");
    }
    // 01:00:00:00 at 23.976 is 86400 frames: 86400 x 1001/24000 = 3603.6 s.
    let tl = timeline(Rate::new(24000, 1001), None, TimelineLayout::FullClipsBelow);
    let xml = fcpxml::write(&tl);
    assert!(xml.contains("tcStart=\"18018/5s\""), "{}", &xml[..600]);
    // 01:00:00;00 drop frame at 29.97 is 107892 frames: 107892 x 1001/30000 = 3599.9964 s.
    let tl = timeline(Rate::new(30000, 1001), Some(true), TimelineLayout::FullClipsBelow);
    assert_eq!(Time::frames(tl.start_frame, tl.rate).fcp(), "8999991/2500s");
    // The 25 fps clip with 10:00:00:00 starts at 36000 s and keeps its own format.
    assert!(xml.contains("frameDuration=\"1/25s\""));
    assert!(fcpxml::write(&tl).contains("start=\"36000s\""));
    // Clips with sound keep it on the storyline; the selects above are picture only.
    let xml = fcpxml::write(&timeline(Rate::new(24, 1), None, TimelineLayout::FullClipsBelow));
    assert!(xml.contains("srcEnable=\"video\""));
    let xml = fcpxml::write(&timeline(Rate::new(24, 1), None, TimelineLayout::SelectsOnly));
    assert!(!xml.contains("srcEnable"));
    assert!(xml.contains("<gap name=\"Gap\""));
}

/* ---------------- EDL ---------------- */

fn parse_events(edl: &str) -> Vec<Vec<String>> {
    edl.lines()
        .filter(|l| l.len() > 3 && l.as_bytes()[0].is_ascii_digit())
        .map(|l| l.split_whitespace().map(|s| s.to_string()).collect())
        .collect()
}

#[test]
fn edl_header_events_and_locators() {
    let r = Rate::new(30000, 1001);
    let tl = timeline(r, Some(true), TimelineLayout::FullClipsBelow);
    let text = edl::write(&tl);
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("TITLE: Day 1 selects"));
    assert_eq!(lines.next(), Some("FCM: DROP FRAME"));
    assert!(text.contains("\r\n"), "CMX3600 lists use CRLF line ends");
    let events = parse_events(&text);
    assert_eq!(events.len(), 3);
    let tc_re = |s: &str, drop: bool| {
        let b = s.as_bytes();
        s.len() == 11 && b[2] == b':' && b[5] == b':' && b[8] == if drop { b';' } else { b':' }
    };
    for (i, e) in events.iter().enumerate() {
        assert_eq!(e[0], format!("{:03}", i + 1));
        assert_eq!(&e[1..4], &["AX", "V", "C"]);
        assert!(tc_re(&e[6], true) && tc_re(&e[7], true), "record timecode is drop frame: {e:?}");
    }
    // Event 1: clip timecode 01:00:00;00, select from 2.0 s (60 frames) for 90 frames.
    assert_eq!(&events[0][4..8], &["01:00:02;00", "01:00:05;00", "01:00:02;00", "01:00:05;00"]);
    // Record times follow track 2 of the XML timelines.
    for (e, p) in events.iter().zip(&tl.selects) {
        assert_eq!(e[6], selects_engine::rate::frames_to_timecode(tl.start_frame + p.record_in, r, true));
    }
    assert!(text.contains("* FROM CLIP NAME: A001_C001.mov"));
    assert!(text.contains("* SOURCE FILE: /Volumes/Shoot 1/Day 1/A001_C001.mov"));
    assert!(text.contains("* LOC: 01:00:02;00 GREEN   Select 1: strong. Focus 82, exposure 74, stability 91 (overall 82)."));
    assert!(text.contains("YELLOW  Select 2: fair."));
    // The 25 fps clip cannot keep its own timecode in a 29.97 list without implying a speed change.
    assert!(text.contains("* NOTE: SOURCE IS 25 FPS."));
    let e = &events[2];
    let len = |a: &str, b: &str| {
        selects_engine::rate::timecode_to_frames(b, r, true).unwrap() - selects_engine::rate::timecode_to_frames(a, r, true).unwrap()
    };
    assert_eq!(len(&e[4], &e[5]), len(&e[6], &e[7]), "source and record lengths match");
}

#[test]
fn edl_non_drop_frame_at_23976_uses_colons() {
    let tl = timeline(Rate::new(24000, 1001), None, TimelineLayout::SelectsOnly);
    let text = edl::write(&tl);
    assert!(text.contains("FCM: NON-DROP FRAME"));
    assert!(!text.lines().filter(|l| l.starts_with(char::is_numeric)).any(|l| l.contains(';')));
    let events = parse_events(&text);
    assert_eq!(events[0][6], "01:00:00:00");
    assert_eq!(&events[0][4..6], &["01:00:02:00", "01:00:05:00"]);
}

/* ---------------- CSV ---------------- */

#[test]
fn csv_rows_quote_and_scores() {
    let tl = timeline(Rate::new(25, 1), None, TimelineLayout::FullClipsBelow);
    let text = csv::write(&tl);
    assert!(text.starts_with('\u{feff}'));
    let lines: Vec<&str> = text.trim_start_matches('\u{feff}').lines().collect();
    assert_eq!(lines[0], csv::HEADER.join(","));
    assert_eq!(lines.len(), 4);
    assert!(lines[1].starts_with("1,A001_C001.mov,/Volumes/Shoot 1/Day 1/A001_C001.mov,01:00:02:00,01:00:05:00,01:00:02:00,01:00:05:00,2.000,5.000,3.000,01:00:02:00,01:00:05:00,V2,82,74,91,82,strong,25,25"), "{}", lines[1]);
    // A name with a comma and quotes is quoted and its quotes doubled.
    assert!(lines[3].contains("\"B-cam, \"\"wide\"\".mp4\""), "{}", lines[3]);
    assert_eq!(csv::field("=SUM(A1)"), "\"'=SUM(A1)\"");
    assert_eq!(csv::field("plain"), "plain");
}

#[test]
fn csv_source_timecode_at_timeline_rate_matches_the_edl() {
    // The 25 fps clip in a 29.97 timeline: the EDL gives its source at 29.97, and so must the CSV.
    let tl = timeline(Rate::new(30000, 1001), Some(true), TimelineLayout::FullClipsBelow);
    let events = parse_events(&edl::write(&tl));
    let text = csv::write(&tl);
    let rows: Vec<Vec<String>> = text
        .trim_start_matches('\u{feff}')
        .lines()
        .skip(1)
        .map(csv_fields)
        .collect();
    assert_eq!(rows.len(), events.len());
    for (row, e) in rows.iter().zip(&events) {
        assert_eq!(row.len(), csv::HEADER.len(), "{row:?}");
        assert_eq!(&row[3..5], &e[4..6], "{row:?}");
    }
    // The clip-rate columns keep the clip's own frame count: 10:00:01:00 at 25 fps.
    assert!(rows[2].contains(&"10:00:01:00".to_string()), "{:?}", rows[2]);
}

/// Split one CSV line, honouring quoted fields.
fn csv_fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    out.push(cur);
    out
}

/* ---------------- files and paths ---------------- */

#[test]
fn file_urls_are_encoded() {
    assert_eq!(file_url("/Users/me/My Shoot/a b.mov", false), "file:///Users/me/My%20Shoot/a%20b.mov");
    assert_eq!(file_url("/Users/me/café #2.mov", true), "file://localhost/Users/me/caf%C3%A9%20%232.mov");
    assert_eq!(file_url("C:\\Footage\\Day 1\\A001.MOV", false), "file:///C:/Footage/Day%201/A001.MOV");
    assert_eq!(file_url("D:\\x.mp4", true), "file://localhost/D:/x.mp4");
}

#[test]
fn write_all_writes_four_files_or_explains_why_not() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("export-test");
    std::fs::create_dir_all(&dir).unwrap();
    // A project whose clips have no selects: nothing is written, and the error says so.
    let (p, _) = project(Rate::new(24, 1), None, TimelineLayout::FullClipsBelow);
    let err = write_all(&p, &dir.join("empty")).err().unwrap().to_string();
    assert!(err.contains("nothing was written"), "{err}");
    assert!(!dir.join("empty.xml").exists());
}

#[test]
fn escapes_text_that_would_break_xml() {
    let (mut p, sels) = project(Rate::new(24, 1), None, TimelineLayout::FullClipsBelow);
    p.export.timeline_name = "Tom & Jerry <rough> \"cut\"".into();
    let tl = build_timeline(&p, &sels).0;
    for xml in [fcp7::write(&tl), fcpxml::write(&tl)] {
        let doc = parse(&xml).unwrap();
        assert!(doc.descendants().any(|n| n.text() == Some("Tom & Jerry <rough> \"cut\"") || n.attribute("name") == Some("Tom & Jerry <rough> \"cut\"")));
    }
}

/* ---------------- extra time, keywords and reels ---------------- */

#[test]
fn extra_time_pads_each_select_without_overlapping_the_next() {
    let r = Rate::new(24, 1);
    let (mut p, sels) = project(r, None, TimelineLayout::FullClipsBelow);
    p.export.handles = 2.0;
    let tl = build_timeline(&p, &sels).0;
    // Clip 1: selects at 2-5 s and 7.5-9 s. Two seconds before the first
    // reaches the clip start; after it, the padding stops halfway to the next.
    let a = &tl.selects[0];
    let m = a.select.as_ref().unwrap();
    assert_eq!(a.source_in, 0);
    assert_eq!(m.lead, 48);
    let b = &tl.selects[1];
    assert!(a.source_out() <= b.source_in, "selects on one clip must not overlap: {} > {}", a.source_out(), b.source_in);
    assert_eq!(a.source_out(), (120 + 180) / 2);
    // The second select of clip 1 runs to the clip end (10 s = 240 frames).
    assert_eq!(b.source_out(), 240);
    assert_eq!(b.select.as_ref().unwrap().tail, 240 - 216);
    // Markers sit where the select itself starts.
    let xml = fcp7::write(&tl);
    let doc = parse(&xml).unwrap();
    let ins: Vec<i64> = doc.descendants().filter(|n| n.has_tag_name("marker")).map(|n| num(n, "in")).collect();
    assert_eq!(ins[0], tl.selects[0].record_in + 48);
    // The spreadsheet says how much was added.
    let csv_text = csv::write(&tl);
    let row = csv_fields(csv_text.trim_start_matches('\u{feff}').lines().nth(1).unwrap());
    assert_eq!(row[row.len() - 2], "2.000");
}

#[test]
fn keywords_reach_every_editor() {
    let r = Rate::new(25, 1);
    let (mut p, mut sels) = project(r, None, TimelineLayout::FullClipsBelow);
    sels[0].segments[0].stars = 4;
    sels[0].segments[0].manual = true;
    sels[0].segments[0].camera = selects_engine::selection::CameraMove::PanLeft;
    sels[0].segments[0].camera_move = true;
    sels[0].segments[0].roll = Some(2.4);
    p.export.timeline_name = "Keywords".into();
    let tl = build_timeline(&p, &sels).0;
    let m = tl.selects[0].select.as_ref().unwrap();
    for k in ["Strong", "4 stars", "Kept by hand", "Pan left", "Pan", "Crooked horizon"] {
        assert!(m.keywords.iter().any(|x| x == k), "missing {k} in {:?}", m.keywords);
    }
    assert!(m.marker_name().ends_with("4 stars, kept by hand"), "{}", m.marker_name());
    assert!(m.marker_note().contains("rotate 2.4 degrees counter-clockwise"), "{}", m.marker_note());
    // Premiere Pro and Resolve: logging fields on the select.
    let xml = fcp7::write(&tl);
    let doc = parse(&xml).unwrap();
    let li = doc.descendants().find(|n| n.has_tag_name("logginginfo")).expect("logginginfo");
    assert!(text(li, "lognote").contains("Pan left"));
    assert_eq!(text(li, "good"), "TRUE");
    // Final Cut Pro: keyword ranges on browser clips, a favorite, keywords on the timeline clip.
    let fx = fcpxml::write(&tl);
    let doc = parse(&fx).unwrap();
    let event = doc.descendants().find(|n| n.has_tag_name("event")).unwrap();
    let browser: Vec<_> = event.children().filter(|n| n.has_tag_name("asset-clip")).collect();
    assert_eq!(browser.len(), 2, "one browser clip per source with selects");
    assert!(browser[0].children().any(|n| n.has_tag_name("keyword") && n.attribute("value").unwrap().contains("Pan left")));
    assert!(browser[0].children().any(|n| n.has_tag_name("rating") && n.attribute("value") == Some("favorite")));
    check_fcpxml(&tl, &fx);
    // EDL comment.
    assert!(edl::write(&tl).contains("* KEYWORDS: Strong, 4 stars"));
}

fn reel_project() -> (Project, Vec<ClipSelection>) {
    let r = Rate::new(25, 1);
    let mut p = Project::new();
    let mut sels = Vec::new();
    for k in 0..6 {
        let name = format!("C{k:04}.MP4");
        let id = p.add_clip(&format!("/footage/{name}"), Ok(info(&name, r, 20.0, None, true))).unwrap();
        let mut a = ClipAnalysis::empty(4.0, 480, 270);
        a.complete = true;
        p.clip_mut(id).unwrap().analysis = Some(a);
        let mut s = seg(2.0, 6.0, 0.9 - k as f32 * 0.02, 0.8, 0.8);
        s.camera_move = k % 2 == 1;
        let mut t = seg(10.0, 13.0, 0.7, 0.7, 0.7);
        t.camera_move = k % 2 == 0;
        sels.push(selection(vec![s, t]));
    }
    p.export.timeline_rate = Some(r);
    (p, sels)
}

fn grid(tempo: f64, seconds: f64) -> selects_engine::beats::BeatGrid {
    let period = 60.0 / tempo;
    let beats: Vec<f64> = (0..).map(|i| i as f64 * period).take_while(|&t| t < seconds).collect();
    selects_engine::beats::BeatGrid {
        tempo,
        downbeats: (0..beats.len()).step_by(4).collect(),
        strength: vec![1.0; beats.len()],
        beats,
        confidence: 0.9,
    }
}

#[test]
fn a_reel_fills_every_slot_on_the_beat_with_the_song_under_it() {
    use selects_engine::reel::{plan, Pace, ReelOrder, ReelSettings, ReelShape};
    let (p, sels) = reel_project();
    let music = selects_engine::beats::MusicInfo { path: "/music/song.mp3".into(), name: "song.mp3".into(), duration: 60.0, sample_rate: 44100, channels: 2, codec: "mp3".into() };
    let settings = ReelSettings { music: Some(music.clone()), grid: Some(grid(120.0, 60.0)), pace: Pace::EveryBar, length: Some(16.0), order: ReelOrder::BestFirst, shape: ReelShape::Vertical };
    let scenes = selects_engine::scene::resolve(&p);
    let r = plan(&p, &sels, &scenes, &settings).unwrap();
    // 120 bpm, a cut every bar (2 s), 16 s: 8 shots of 2 s.
    assert_eq!(r.beats_per_cut, 4);
    assert_eq!(r.shots.len(), 8);
    for (i, s) in r.shots.iter().enumerate() {
        assert!((s.at - 2.0 * i as f64).abs() < 1e-6, "shot {i} at {}", s.at);
        assert!((s.length - 2.0).abs() < 1e-6);
        assert!(s.source_start >= 0.0 && s.source_end <= 20.0);
        if i > 0 {
            assert_ne!(s.clip_id, r.shots[i - 1].clip_id, "two shots in a row from one clip");
        }
    }
    let tl = selects_engine::export::build_reel_timeline(&p, &r, &music, ReelShape::Vertical, "Reel");
    assert_eq!((tl.width, tl.height), (1080, 1920));
    assert!(tl.fill);
    assert_eq!(tl.duration, 16 * 25);
    let m = tl.music.as_ref().unwrap();
    assert_eq!(m.frames_used, 16 * 25);
    // Shots are back to back.
    for w in tl.selects.windows(2) {
        assert_eq!(w[0].record_out(), w[1].record_in);
    }
    let xml = fcp7::write(&tl);
    assert!(xml.contains("file-music") && xml.contains("song.mp3"));
    // Premiere Pro and Resolve: each video clip is scaled to fill the
    // vertical frame, 1080 / 1920 * 1920 / 1080 = 177.778%, centred.
    let doc = parse(&xml).unwrap();
    let video: Vec<_> = doc.descendants().filter(|n| n.has_tag_name("clipitem") && n.attribute("id").is_some_and(|i| i.starts_with("clipitem-v2-"))).collect();
    assert_eq!(video.len(), 8);
    for c in &video {
        let effect = c.descendants().find(|n| n.has_tag_name("effect")).expect("Basic Motion filter");
        assert_eq!(text(effect, "effectid"), "basic");
        let param = |id: &str| effect.children().find(|n| n.has_tag_name("parameter") && text(*n, "parameterid") == id).unwrap();
        assert_eq!(text(param("scale"), "value"), "177.778");
        let centre = param("center").children().find(|n| n.has_tag_name("value")).unwrap();
        assert_eq!((text(centre, "horiz"), text(centre, "vert")), ("0", "0"));
    }
    assert!(!doc.descendants().any(|n| n.has_tag_name("clipitem") && n.attribute("id").is_some_and(|i| i.starts_with("clipitem-music")) && n.descendants().any(|d| d.has_tag_name("filter"))));
    let fx = fcpxml::write(&tl);
    assert!(fx.contains("audioRole=\"music\"") && fx.contains("adjust-conform"));
    check_fcpxml(&tl, &fx);
    assert!(edl::write(&tl).contains("MUSIC"));
}

#[test]
fn reel_fill_scale_follows_each_clips_display_shape() {
    use selects_engine::reel::{plan, Pace, ReelOrder, ReelSettings, ReelShape};
    let (mut p, sels) = reel_project();
    // A phone clip shot upright: stored 1920 by 1080, turned 90 degrees, so
    // ClipInfo holds its display size of 1080 by 1920.
    let phone = p.clips[0].id;
    {
        let info = p.clip_mut(phone).unwrap().info.as_mut().unwrap();
        info.rotation = 90;
        info.width = 1080;
        info.height = 1920;
    }
    let music = selects_engine::beats::MusicInfo { path: "/m.wav".into(), name: "m.wav".into(), duration: 60.0, sample_rate: 48000, channels: 2, codec: "pcm".into() };
    let scenes = selects_engine::scene::resolve(&p);
    for shape in [ReelShape::Vertical, ReelShape::Square, ReelShape::Landscape] {
        let settings = ReelSettings { music: Some(music.clone()), grid: Some(grid(120.0, 60.0)), pace: Pace::EveryBar, length: Some(24.0), order: ReelOrder::BestFirst, shape };
        let r = plan(&p, &sels, &scenes, &settings).unwrap();
        assert!(r.shots.iter().any(|s| s.clip_id == phone));
        let tl = selects_engine::export::build_reel_timeline(&p, &r, &music, shape, "Reel");
        let xml = fcp7::write(&tl);
        if shape == ReelShape::Landscape {
            assert!(!tl.fill && !xml.contains("Basic Motion"));
            continue;
        }
        assert!(tl.fill);
        let doc = parse(&xml).unwrap();
        for c in doc.descendants().filter(|n| n.has_tag_name("clipitem") && n.attribute("id").is_some_and(|i| i.starts_with("clipitem-v2-"))) {
            let is_phone = text(c, "name").starts_with("C0000.MP4");
            let scale = c.descendants().find(|n| n.has_tag_name("parameter") && text(*n, "parameterid") == "scale").map(|n| text(n, "value")).unwrap();
            // Vertical 1080 by 1920; square 1080 by 1080.
            let want = match (shape, is_phone) {
                (ReelShape::Vertical, false) => "177.778",
                (ReelShape::Vertical, true) => "100",
                (_, _) => "100",
            };
            assert_eq!(scale, want, "{shape:?}, phone clip {is_phone}");
        }
    }
}

#[test]
fn a_reel_has_no_sliver_of_a_shot_at_either_end() {
    use selects_engine::reel::{plan, Pace, ReelOrder, ReelSettings};
    let (p, sels) = reel_project();
    let music = selects_engine::beats::MusicInfo { path: "/m.wav".into(), name: "m.wav".into(), duration: 60.0, sample_rate: 48000, channels: 2, codec: "pcm".into() };
    let scenes = selects_engine::scene::resolve(&p);
    // The tester's case: 143.6 BPM with the first bar 0.1 s in, a cut every
    // beat, 30 s. Also every bar, and a first bar a little later.
    for (offset, pace) in [(0.1, Pace::EveryBeat), (0.1, Pace::EveryBar), (0.3, Pace::EveryTwoBeats), (0.25, Pace::EveryBeat)] {
        let mut g = grid(143.6, 60.0);
        for b in g.beats.iter_mut() {
            *b += offset;
        }
        let beat = 60.0 / 143.6;
        let settings = ReelSettings { music: Some(music.clone()), grid: Some(g.clone()), pace, length: Some(30.0), order: ReelOrder::BestFirst, ..Default::default() };
        let r = plan(&p, &sels, &scenes, &settings).unwrap();
        let shortest = r.shots.iter().map(|s| s.length).fold(f64::MAX, f64::min);
        assert!(shortest >= 0.5 * beat, "{pace:?} at {offset}: a shot of {shortest:.3} s");
        assert_eq!(r.shots[0].at, 0.0);
        assert!(r.length <= 30.0 + 1e-9);
        // Every cut after the start is on a beat of the song.
        for s in &r.shots[1..] {
            assert!(g.beats.iter().any(|b| (b - s.at).abs() < 1e-9), "{pace:?}: cut at {} is off the beat", s.at);
        }
        for w in r.shots.windows(2) {
            assert!((w[0].at + w[0].length - w[1].at).abs() < 1e-9);
        }
    }
}

#[test]
fn as_shot_reels_keep_recording_order() {
    use selects_engine::reel::{plan, Pace, ReelOrder, ReelSettings};
    let (mut p, sels) = reel_project();
    // Recording times from the clips' tags, in reverse of their order in the project.
    for (k, c) in p.clips.iter_mut().enumerate() {
        let info = c.info.as_mut().unwrap();
        info.tags.creation_time = Some(format!("2024-06-01T1{}:00:00Z", 6 - k));
    }
    let music = selects_engine::beats::MusicInfo { path: "/m.wav".into(), name: "m.wav".into(), duration: 30.0, sample_rate: 48000, channels: 2, codec: "pcm".into() };
    let settings = ReelSettings { music: Some(music), grid: Some(grid(120.0, 30.0)), pace: Pace::EveryBar, length: Some(12.0), order: ReelOrder::AsShot, ..Default::default() };
    let scenes = selects_engine::scene::resolve(&p);
    let r = plan(&p, &sels, &scenes, &settings).unwrap();
    let order: Vec<usize> = r.shots.iter().map(|s| p.clips.iter().position(|c| c.id == s.clip_id).unwrap()).collect();
    // Later clips in the project were recorded first, so the reel starts with them.
    assert!(order.first() > order.last(), "{order:?}");
}
