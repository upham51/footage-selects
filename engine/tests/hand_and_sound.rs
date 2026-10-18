//! Calls made by hand, cuts in speech pauses, picture checks and scenes.

use selects_engine::analyze::{FrameTable, SoundMap};
use selects_engine::metrics::FrameMetrics;
use selects_engine::scores::ScoreSeries;
use selects_engine::selection::{apply_cap, select_candidates_with, AsideReason, Call, ClipExtras, Mark, SegmentStatus, SelectionSettings};

/// A 4 Hz series from per-second scores (all three the same).
fn series(per_second: &[f32]) -> ScoreSeries {
    let mut s = ScoreSeries::default();
    for (sec, &q) in per_second.iter().enumerate() {
        for k in 0..4 {
            s.t.push(sec as f32 + k as f32 * 0.25);
            s.focus.push(q);
            s.exposure.push(q);
            s.stability.push(q);
            s.motion.push(0.0);
        }
    }
    s
}

fn settings() -> SelectionSettings {
    SelectionSettings { smoothing: 0.0, min_length: 2.0, max_length: 10.0, max_per_clip: 0, keep_moves_whole: false, ..Default::default() }
}

#[test]
fn a_stretch_kept_by_hand_stays_whatever_the_scores() {
    // Good 0-6 s, bad 6-14 s, good 14-20 s.
    let mut v = vec![0.9; 6];
    v.extend(vec![0.2; 8]);
    v.extend(vec![0.9; 6]);
    let s = series(&v);
    let marks = [Mark { start: 8.0, end: 11.0, call: Call::Keep, stars: 3 }];
    let sel = select_candidates_with(&s, 20.0, &settings(), &ClipExtras { marks: &marks, ..Default::default() });
    let k = sel.segments.iter().find(|g| g.manual).expect("manual segment");
    assert_eq!((k.start, k.end, k.status, k.stars), (8.0, 11.0, SegmentStatus::Kept, 3));
    // The dropped stretch around it is cut into two pieces, still dropped.
    let below: Vec<_> = sel.segments.iter().filter(|g| g.status == SegmentStatus::BelowThreshold).collect();
    assert_eq!(below.len(), 2);
    assert!(below.iter().all(|g| g.end <= 8.0 + 1e-9 || g.start >= 11.0 - 1e-9));
    // Segments still cover the clip in order without gaps.
    for w in sel.segments.windows(2) {
        assert!((w[0].end - w[1].start).abs() < 1e-6, "{} then {}", w[0].end, w[1].start);
    }
}

#[test]
fn a_dropped_select_frees_its_place_and_hand_picks_do_not_count_toward_the_limit() {
    // Three good stretches of 4 s, best first, separated by bad seconds.
    let mut v = vec![0.95; 4];
    v.push(0.1);
    v.extend(vec![0.85; 4]);
    v.push(0.1);
    v.extend(vec![0.75; 4]);
    v.push(0.1);
    v.extend(vec![0.2; 4]);
    let s = series(&v);
    let mut st = settings();
    st.max_per_clip = 1;
    let marks = [Mark { start: 0.0, end: 4.0, call: Call::Drop, stars: 0 }, Mark { start: 15.0, end: 18.0, call: Call::Keep, stars: 0 }];
    let mut sel = select_candidates_with(&s, 19.0, &st, &ClipExtras { marks: &marks, ..Default::default() });
    apply_cap(&mut sel.segments, st.max_per_clip);
    let kept: Vec<(f64, bool)> = sel.kept().map(|g| (g.start, g.manual)).collect();
    // The best stretch was dropped, so the next best takes the one place; the hand pick is extra.
    // Segment edges sit halfway between samples, so the second stretch starts at 4.875 s.
    assert_eq!(kept, vec![(4.875, false), (15.0, true)]);
    let dropped = sel.segments.iter().find(|g| g.start == 0.0).unwrap();
    assert_eq!((dropped.status, dropped.aside), (SegmentStatus::SetAside, Some(AsideReason::Dropped)));
}

fn talk_map(talking: &[(f64, f64)], end: f64) -> SoundMap {
    let mut pauses = Vec::new();
    let mut last = 0.0;
    for &(a, b) in talking {
        if a - last >= 0.25 {
            pauses.push((last, a));
        }
        last = b;
    }
    if end - last >= 0.25 {
        pauses.push((last, end));
    }
    SoundMap { summary: selects_engine::audio::AudioSummary { has_speech: true, ..Default::default() }, talking: talking.to_vec(), pauses }
}

#[test]
fn long_takes_are_cut_in_the_pauses_between_sentences() {
    // 24 s, all good. Sentences with pauses at 7.2-7.8, 11.4-11.9 and 16.6-17.2.
    let s = series(&vec![0.9; 24]);
    let map = talk_map(&[(0.5, 7.2), (7.8, 11.4), (11.9, 16.6), (17.2, 23.5)], 24.0);
    let mut st = settings();
    st.max_length = 8.0;
    st.min_length = 3.0;
    let sel = select_candidates_with(&s, 24.0, &st, &ClipExtras { sound: Some(&map), ..Default::default() });
    let kept: Vec<_> = sel.kept().collect();
    assert!(kept.len() >= 3, "{kept:?}");
    for g in &kept {
        assert!(g.end - g.start <= 2.0 * st.max_length as f64 + 1e-9);
        for edge in [g.start, g.end] {
            let in_pause = map.pauses.iter().any(|&(a, b)| edge >= a - 1e-6 && edge <= b + 1e-6);
            assert!(in_pause, "cut at {edge} is not in a pause: {:?}", kept.iter().map(|g| (g.start, g.end)).collect::<Vec<_>>());
        }
        assert_eq!(g.speech_cuts, 0);
    }
    // With the option off, the same take is cut mid-sentence somewhere.
    st.snap_to_pauses = false;
    let off = select_candidates_with(&s, 24.0, &st, &ClipExtras { sound: Some(&map), ..Default::default() });
    let mid = off.kept().flat_map(|g| [g.start, g.end]).filter(|&t| map.talking_at(t + 0.01) && map.talking_at(t - 0.01)).count();
    assert!(mid > 0);
}

#[test]
fn horizon_flicker_and_jello_are_judged_per_select() {
    let s = series(&vec![0.9; 8]);
    let mut f = FrameTable::default();
    for i in 0..s.len() {
        let m = FrameMetrics { roll: 1.8, roll_conf: 0.9, flicker: 0.03, wobble: if i % 2 == 0 { 0.6 } else { 0.5 }, cb: -3.0, cr: 4.0, wb_ok: true, ..Default::default() };
        f.push(s.t[i] as f64, &m);
    }
    let sel = select_candidates_with(&s, 8.0, &settings(), &ClipExtras { frames: Some(&f), ..Default::default() });
    let g = sel.kept().next().unwrap();
    assert!(g.crooked() && g.flickers() && g.jello());
    assert_eq!(g.roll, Some(1.8));
    assert_eq!(g.wb, Some((-3.0, 4.0)));
    let notes = g.problem_notes().join(" | ");
    assert!(notes.contains("tilts down to the right by 1.8 degrees"), "{notes}");
    // Low confidence readings are ignored.
    let mut f2 = f.clone();
    f2.roll_conf.iter_mut().for_each(|c| *c = 0.1);
    let sel = select_candidates_with(&s, 8.0, &settings(), &ClipExtras { frames: Some(&f2), ..Default::default() });
    assert_eq!(sel.kept().next().unwrap().roll, None);
}

#[test]
fn phones_give_time_place_and_light_and_other_cameras_borrow_the_place() {
    use selects_engine::probe::{ClipInfo, RecordTags};
    use selects_engine::project::Project;
    use selects_engine::rate::Rate;
    use selects_engine::solar::Light;
    let info = |name: &str, tags: RecordTags| ClipInfo {
        path: format!("/trip/{name}"),
        name: name.into(),
        size: 1,
        modified: 1,
        duration: 20.0,
        start_time: 0.0,
        width: 1920,
        height: 1080,
        coded_width: 1920,
        coded_height: 1080,
        rotation: 0,
        avg_fps: 25.0,
        rate: Rate::new(25, 1),
        variable_frame_rate: false,
        codec: "h264".into(),
        pix_fmt: "yuv420p".into(),
        has_audio: false,
        audio_channels: 0,
        audio_sample_rate: 0,
        timecode: None,
        full_range: false,
        color_transfer: String::new(),
        detected_profile: None,
        camera: None,
        tags,
    };
    let mut p = Project::new();
    // Lisbon, 1 June 2024. Sunset there is about 19:50 UTC (20:50 local).
    let phone = RecordTags { creation_time: Some("2024-06-01T19:35:00.000000Z".into()), location: Some("+38.7223-009.1393/".into()), phone: true, ..Default::default() };
    p.add_clip("/trip/IMG_0001.MOV", Ok(info("IMG_0001.MOV", phone))).unwrap();
    // A camera with only its clock (local time, labelled UTC), ten minutes later.
    let cam = RecordTags { creation_time: Some("2024-06-01T20:45:00Z".into()), ..Default::default() };
    p.add_clip("/trip/P1000001.MOV", Ok(info("P1000001.MOV", cam))).unwrap();
    // Midday, the next day, from the phone.
    let noon = RecordTags { creation_time: Some("2024-06-02T12:30:00Z".into()), location: Some("+38.7223-009.1393/".into()), phone: true, ..Default::default() };
    p.add_clip("/trip/IMG_0002.MOV", Ok(info("IMG_0002.MOV", noon))).unwrap();
    let sc = selects_engine::scene::resolve(&p);
    assert_eq!(sc[0].light, Some(Light::GoldenHour), "{:?}", sc[0]);
    assert!(sc[1].place.is_some(), "the camera clip borrows the phone's place");
    assert!(sc[1].zone_estimated);
    // Lisbon is UTC+1 in summer; longitude gives UTC-1 (round(-9.1/15) = -1), an estimate.
    assert!(sc[1].utc.is_some());
    assert_eq!(sc[2].light, Some(Light::HarshMidday));
    assert_ne!(sc[0].group, sc[2].group, "two days are two scenes");
    assert!(sc[0].group.as_deref().unwrap().starts_with("Day 1"));
    assert!(sc[2].group.as_deref().unwrap().starts_with("Day 2"));
    // With the camera's clock zone set, the camera clip lines up with the phone.
    p.places.clock_offset_minutes = Some(60);
    let sc = selects_engine::scene::resolve(&p);
    assert_eq!(sc[1].group, sc[0].group);
    // The light filter sets aside clips in other light, and any without a known light.
    p.selection.light = selects_engine::selection::LightFilter::GoldenHour;
    assert!(p.selection.light.allows(Some(Light::GoldenHour)) && !p.selection.light.allows(None));
}

fn timed_clip(name: &str, tags: selects_engine::probe::RecordTags) -> selects_engine::probe::ClipInfo {
    use selects_engine::probe::ClipInfo;
    use selects_engine::rate::Rate;
    ClipInfo {
        path: format!("/shoot/{name}"),
        name: name.into(),
        size: 1,
        modified: 1,
        duration: 10.0,
        start_time: 0.0,
        width: 3840,
        height: 2160,
        coded_width: 3840,
        coded_height: 2160,
        rotation: 0,
        avg_fps: 23.976,
        rate: Rate::new(24000, 1001),
        variable_frame_rate: false,
        codec: "h264".into(),
        pix_fmt: "yuv420p".into(),
        has_audio: false,
        audio_channels: 0,
        audio_sample_rate: 0,
        timecode: None,
        full_range: false,
        color_transfer: String::new(),
        detected_profile: None,
        camera: None,
        tags,
    }
}

fn bright_analysis(level: f32) -> selects_engine::analyze::ClipAnalysis {
    let mut a: selects_engine::analyze::ClipAnalysis = serde_json::from_value(serde_json::json!({
        "version": 5, "sample_rate": 2.0, "width": 480, "height": 270,
        "frames": { "t": [], "lap": [], "mean": [], "lo": [], "hi": [], "p2": [], "p98": [], "dx": [], "dy": [], "motion_ok": [] },
        "complete": true, "decoder": "software", "elapsed": 1.0
    }))
    .unwrap();
    a.frames.mean = vec![level; 10];
    a
}

#[test]
fn sony_afternoon_clips_are_daylight_and_named_in_local_time() {
    use selects_engine::probe::RecordTags;
    use selects_engine::project::Project;
    use selects_engine::solar::Light;
    let mut p = Project::new();
    // Shi Shi beach, 22:50 UTC on 4 October 2026 is 15:50 local (UTC-7).
    let tags = RecordTags { creation_time: Some("2026-10-04T22:50:00.000000Z".into()), brand: Some("XAVC".into()), ..Default::default() };
    p.add_clip("/shoot/C7929.MP4", Ok(timed_clip("C7929.MP4", tags))).unwrap();
    // The same camera, the brand tag missing; the name gives it away.
    let tags = RecordTags { creation_time: Some("2026-10-04T23:05:00.000000Z".into()), ..Default::default() };
    p.add_clip("/shoot/C7935.MP4", Ok(timed_clip("C7935.MP4", tags))).unwrap();
    p.places.location = Some((48.284, -124.67));
    let sc = selects_engine::scene::resolve(&p);
    for s in &sc {
        assert!(matches!(s.light, Some(Light::Daylight) | Some(Light::HarshMidday)), "{s:?}");
        assert!(s.group.as_deref().unwrap().contains("afternoon"), "{s:?}");
        assert_eq!(s.offset_minutes, Some(-480), "shown in the place's local time");
    }
    // Choosing UTC for the camera clock gives the same light and names.
    p.places.clock_offset_minutes = Some(0);
    let again = selects_engine::scene::resolve(&p);
    assert_eq!(again[0].light, sc[0].light);
    assert_eq!(again[0].group, sc[0].group);
}

#[test]
fn a_clock_on_utc_is_recognised_from_bright_pictures() {
    use selects_engine::probe::RecordTags;
    use selects_engine::project::Project;
    use selects_engine::solar::Light;
    let mut p = Project::new();
    // A camera that does not say its zone, clock on UTC, two daylight clips.
    for (i, t) in ["2026-10-04T21:10:00Z", "2026-10-04T22:40:00Z"].iter().enumerate() {
        let name = format!("P100000{i}.MOV");
        let tags = RecordTags { creation_time: Some(t.to_string()), ..Default::default() };
        let id = p.add_clip(&format!("/shoot/{name}"), Ok(timed_clip(&name, tags))).unwrap();
        p.clip_mut(id).unwrap().analysis = Some(bright_analysis(110.0));
    }
    p.places.location = Some((48.284, -124.67));
    let sc = selects_engine::scene::resolve(&p);
    for s in &sc {
        assert!(matches!(s.light, Some(Light::Daylight) | Some(Light::HarshMidday)), "{s:?}");
        assert!(s.zone_estimated);
        assert!(s.group.as_deref().unwrap().contains("afternoon"), "{s:?}");
    }
    // Dark pictures at the same clock times read as local time: night.
    for c in p.clips.iter_mut() {
        c.analysis = Some(bright_analysis(12.0));
    }
    let sc = selects_engine::scene::resolve(&p);
    assert_eq!(sc[1].light, Some(Light::Night), "{:?}", sc[1]);
}
