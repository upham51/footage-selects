//! Camera direction and the camera filter, ranking weights, repeated takes
//! across clips, and stills for the frame viewer.

mod common;

use selects_engine::analyze::{analyze_clip, AnalyzeOptions};
use selects_engine::frame::{grab_still, tone_guide};
use selects_engine::probe::probe;
use selects_engine::project::Project;
use selects_engine::scores::ScoreSeries;
use selects_engine::selection::{
    no_select_reason, select_clip, AsideReason, CameraFilter, CameraMove, NoSelectReason, SegmentStatus, SelectionSettings,
};
use selects_engine::tone::ToneProfile;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn analysed_project(paths: &[&Path]) -> Project {
    let mut p = Project::new();
    for path in paths {
        let info = probe(path).map_err(|e| e.to_string());
        let id = p.add_clip(&path.to_string_lossy(), info).unwrap();
        let clip = p.clip(id).unwrap().clone();
        let a = analyze_clip(clip.info.as_ref().unwrap(), &AnalyzeOptions::default(), &AtomicBool::new(false), |_| {}).unwrap();
        p.clip_mut(id).unwrap().analysis = Some(a);
    }
    p
}

fn loose() -> SelectionSettings {
    SelectionSettings {
        focus_min: 0.3,
        exposure_min: 0.3,
        stability_min: 0.3,
        min_length: 1.0,
        max_length: 20.0,
        max_per_clip: 0,
        ..Default::default()
    }
}

/// The camera move of the longest kept segment of a clip.
fn main_move(path: &Path) -> CameraMove {
    let p = analysed_project(&[path]);
    let mut q = p.clone();
    q.selection = loose();
    let (sels, _) = q.select_all();
    let seg = sels[0]
        .kept()
        .max_by(|a, b| a.duration().partial_cmp(&b.duration()).unwrap())
        .unwrap_or_else(|| panic!("no kept segment in {}: {:?}", path.display(), sels[0].segments));
    seg.camera
}

#[test]
fn pans_and_tilts_are_told_apart_and_point_the_right_way() {
    let f = common::footage();
    // The crop window slides right over the scene: the camera pans right.
    assert_eq!(main_move(&f.panning), CameraMove::PanRight);
    let left = common::custom_clip("pan_left.mp4", 4.0, "crop=1280:720:'260-t*60':90");
    assert_eq!(main_move(&left), CameraMove::PanLeft);
    // The window slides down the scene: the camera tilts down.
    let down = common::custom_clip("tilt_down_fast.mp4", 3.0, "crop=1280:720:160:'5+t*55'");
    assert_eq!(main_move(&down), CameraMove::TiltDown);
    let up = common::custom_clip("tilt_up_fast.mp4", 3.0, "crop=1280:720:160:'172-t*55'");
    assert_eq!(main_move(&up), CameraMove::TiltUp);
    // Stored sideways and shown upright, a pan reads as a tilt.
    assert!(matches!(main_move(&f.rotated_pan), CameraMove::TiltUp | CameraMove::TiltDown));
    assert_eq!(main_move(&f.steady), CameraMove::Still);
}

/// A 4 Hz series of good scores with camera velocity (pan, tilt) per second.
fn moving_series(per_second: &[(f32, f32)]) -> ScoreSeries {
    let mut s = ScoreSeries::default();
    for (sec, &(pan, tilt)) in per_second.iter().enumerate() {
        for k in 0..4 {
            s.t.push(sec as f32 + k as f32 * 0.25);
            s.focus.push(0.9);
            s.exposure.push(0.9);
            s.stability.push(0.9);
            s.motion.push((pan * pan + tilt * tilt).sqrt());
            s.pan.push(pan);
            s.tilt.push(tilt);
        }
    }
    s
}

#[test]
fn the_camera_filter_cuts_where_the_camera_starts_moving_and_sets_aside_the_rest() {
    // Still for 6 s, panning left for 8 s, still for 6 s.
    let mut v = vec![(0.0, 0.0); 6];
    v.extend(vec![(-0.1, 0.0); 8]);
    v.extend(vec![(0.0, 0.0); 6]);
    let s = moving_series(&v);
    let mut st = loose();
    st.min_length = 2.0;

    // Without a filter: one long good stretch.
    let any = select_clip(&s, 20.0, &st);
    assert_eq!(any.kept().count(), 1, "{:?}", any.segments);

    st.camera = CameraFilter::Still;
    let still = select_clip(&s, 20.0, &st);
    let kept: Vec<_> = still.kept().map(|g| (g.start, g.end, g.camera)).collect();
    assert_eq!(kept.len(), 2, "{kept:?}");
    assert!(kept.iter().all(|k| k.2 == CameraMove::Still));
    assert!(kept[0].1 <= 6.3 && kept[1].0 >= 13.7, "{kept:?}");
    let aside: Vec<_> = still.segments.iter().filter(|g| g.status == SegmentStatus::SetAside).collect();
    assert_eq!(aside.len(), 1);
    assert_eq!(aside[0].camera, CameraMove::PanLeft);
    assert_eq!(aside[0].aside, Some(AsideReason::Camera));

    st.camera = CameraFilter::Pans;
    let pans = select_clip(&s, 20.0, &st);
    let kept: Vec<_> = pans.kept().collect();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].camera, CameraMove::PanLeft);
    assert!(kept[0].start >= 5.7 && kept[0].end <= 14.3, "{:?}", kept[0]);

    // Nothing tilts, so nothing is kept, and the reason says why.
    st.camera = CameraFilter::Tilts;
    let tilts = select_clip(&s, 20.0, &st);
    assert_eq!(tilts.kept().count(), 0);
    assert_eq!(no_select_reason(&tilts, &st), Some(NoSelectReason::SetAside { camera: 3, repeats: 0 }));
}

#[test]
fn a_brief_hesitation_does_not_chop_a_pan_in_two() {
    // A 10 s pan right with half a second of rest in the middle.
    let mut v = vec![(0.08, 0.0); 5];
    let mut s = moving_series(&v);
    let rest = moving_series(&[(0.0, 0.0)]);
    for k in 0..2 {
        s.t.push(5.0 + k as f32 * 0.25);
        s.focus.push(0.9);
        s.exposure.push(0.9);
        s.stability.push(0.9);
        s.motion.push(rest.motion[k]);
        s.pan.push(0.0);
        s.tilt.push(0.0);
    }
    v = vec![(0.08, 0.0); 5];
    let tail = moving_series(&v);
    for k in 0..tail.len() {
        s.t.push(5.5 + tail.t[k]);
        s.focus.push(0.9);
        s.exposure.push(0.9);
        s.stability.push(0.9);
        s.motion.push(tail.motion[k]);
        s.pan.push(tail.pan[k]);
        s.tilt.push(tail.tilt[k]);
    }
    let mut st = loose();
    st.camera = CameraFilter::Pans;
    let sel = select_clip(&s, 10.5, &st);
    let kept: Vec<_> = sel.kept().collect();
    assert_eq!(kept.len(), 1, "{:?}", sel.segments);
    assert_eq!(kept[0].camera, CameraMove::PanRight);
}

#[test]
fn ranking_weights_decide_which_selects_win_the_limit() {
    // Two good stretches split by a bad one: the first sharper, the second steadier.
    let mut s = ScoreSeries::default();
    let parts = [(0.95, 0.8, 0.6), (0.1, 0.1, 0.1), (0.6, 0.8, 0.95)];
    for (p, &(f, e, st)) in parts.iter().enumerate() {
        for k in 0..16 {
            s.t.push(p as f32 * 4.0 + k as f32 * 0.25);
            s.focus.push(f);
            s.exposure.push(e);
            s.stability.push(st);
        }
    }
    let mut set = loose();
    set.max_per_clip = 1;
    set.focus_weight = 1.0;
    set.stability_weight = 0.0;
    let sharp_first = select_clip(&s, 12.0, &set);
    assert!(sharp_first.kept().next().unwrap().start < 1.0);
    set.focus_weight = 0.0;
    set.stability_weight = 1.0;
    let steady_first = select_clip(&s, 12.0, &set);
    assert!(steady_first.kept().next().unwrap().start > 7.0);
    // All weights at zero counts the three scores equally rather than failing.
    set.focus_weight = 0.0;
    set.exposure_weight = 0.0;
    set.stability_weight = 0.0;
    let g = select_clip(&s, 12.0, &set);
    let k = g.segments.iter().find(|x| x.status != SegmentStatus::BelowThreshold).unwrap();
    assert!((k.score - (k.focus + k.exposure + k.stability) / 3.0).abs() < 1e-5);
}

fn copy_of(src: &Path, name: &str) -> PathBuf {
    let dst = common::fixtures_dir().join(name);
    if !dst.exists() {
        std::fs::copy(src, &dst).unwrap();
    }
    dst
}

#[test]
fn repeated_takes_across_clips_are_found_and_can_be_set_aside() {
    let f = common::footage();
    let again = copy_of(&f.sharp, "sharp_again.mp4");
    // The same framing nudged a few pixels: a second take from the tripod.
    let nudged = common::custom_clip("sharp_nudged.mp4", 3.0, "crop=1280:720:166:94");
    // A different picture.
    let other = common::custom_clip("sharp_flipped.mp4", 3.0, "crop=1280:720:160:90,hflip,vflip");
    let mut p = analysed_project(&[&f.sharp, &again, &nudged, &other]);
    p.selection = loose();
    let ids: Vec<u64> = p.clips.iter().map(|c| c.id).collect();

    // Found but left in place by default.
    let (sels, before) = p.select_all();
    for k in 0..4 {
        assert!(sels[k].kept().count() >= 1, "clip {k} should have a select: {:?}", sels[k].segments);
    }
    let twin = |k: usize| sels[k].kept().filter_map(|g| g.similar_to.as_ref()).map(|s| s.clip).next();
    let firsts: Vec<Option<u64>> = (0..4).map(twin).collect();
    // Exactly one of the three alike clips is the original; the other two point at it.
    let originals = (0..3).filter(|&k| firsts[k].is_none()).count();
    assert_eq!(originals, 1, "{firsts:?}");
    for k in 0..3 {
        if let Some(c) = firsts[k] {
            assert!(ids[..3].contains(&c) && c != ids[k], "{firsts:?}");
        }
    }
    assert_eq!(firsts[3], None, "a different picture is not a repeat: {firsts:?}");

    // Set aside: only the original and the different picture keep selects.
    p.selection.set_aside_repeats = true;
    let (sels, after) = p.select_all();
    let with_selects = sels.iter().filter(|s| s.kept().count() > 0).count();
    assert_eq!(with_selects, 2);
    assert!(after.selects_seconds < before.selects_seconds);
    for s in &sels {
        for g in s.segments.iter().filter(|g| g.status == SegmentStatus::SetAside) {
            assert_eq!(g.aside, Some(AsideReason::Repeat));
            assert!(g.similar_to.is_some());
        }
    }
    let reasons: Vec<_> = sels.iter().filter_map(|s| no_select_reason(s, &p.selection)).collect();
    assert_eq!(reasons.len(), 2);
    assert!(reasons.iter().all(|r| matches!(r, NoSelectReason::SetAside { camera: 0, repeats } if *repeats >= 1)));

    // A setting of a clip on its own never sees other clips.
    let alone = p.clips[1].select(&p.selection, None);
    assert!(alone.kept().count() >= 1);
}

#[test]
fn a_repeat_set_aside_makes_room_for_the_next_best_under_the_limit() {
    let f = common::footage();
    // Two clips: the same sharp picture, and a clip that has that picture
    // for 3 s followed by a different one for 3 s.
    let mixed = common::fixtures_dir().join("same_then_different.mp4");
    if !mixed.exists() {
        let other = common::custom_clip("sharp_flipped.mp4", 3.0, "crop=1280:720:160:90,hflip,vflip");
        let list = common::fixtures_dir().join("concat_list.txt");
        std::fs::write(&list, format!("file '{}'\nfile '{}'\n", f.sharp.display(), other.display())).unwrap();
        let status = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-v", "error", "-y", "-f", "concat", "-safe", "0", "-i"])
            .arg(&list)
            .args(["-c", "copy"])
            .arg(&mixed)
            .status()
            .unwrap();
        assert!(status.success());
    }
    let mut p = analysed_project(&[&f.sharp, &mixed]);
    p.selection = loose();
    p.selection.max_length = 3.0;
    p.selection.min_length = 1.0;
    p.selection.max_per_clip = 1;
    // Make the sharp-picture half of the second clip win its own ranking.
    p.selection.set_aside_repeats = true;
    let (sels, _) = p.select_all();
    let kept: Vec<_> = sels[1].kept().collect();
    assert_eq!(kept.len(), 1, "{:?}", sels[1].segments);
    assert!(kept[0].similar_to.is_none(), "the kept select should be the different picture: {:?}", sels[1].segments);
    assert!(kept[0].start >= 2.5, "{:?}", kept[0]);
}

#[test]
fn a_still_comes_back_at_display_size_with_real_pixels() {
    let f = common::footage();
    for path in [&f.sharp, &f.rotated] {
        let info = probe(path).unwrap();
        let still = grab_still(&info, &path.to_string_lossy(), 1.0).unwrap();
        assert_eq!(still.rgba.len(), (still.width * still.height * 4) as usize);
        // Upright: wider than tall for landscape, taller than wide for the rotated clip.
        assert_eq!(still.width > still.height, info.width > info.height, "{}", path.display());
        let lumas: Vec<u8> = still.rgba.chunks(4).map(|p| p[1]).collect();
        let (lo, hi) = (lumas.iter().min().unwrap(), lumas.iter().max().unwrap());
        assert!(hi - lo > 60, "a real picture has contrast");
        let bytes = still.to_bytes();
        assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), still.width);
        assert_eq!(bytes.len(), 8 + still.rgba.len());
    }
    // A moment past the end falls back to the last frame.
    let info = probe(&f.sharp).unwrap();
    assert!(grab_still(&info, &f.sharp.to_string_lossy(), info.duration + 5.0).is_ok());
    // A different moment of a moving clip gives a different picture.
    let info = probe(&f.panning).unwrap();
    let a = grab_still(&info, &f.panning.to_string_lossy(), 0.2).unwrap();
    let b = grab_still(&info, &f.panning.to_string_lossy(), 3.0).unwrap();
    assert_ne!(a.rgba, b.rgba);
}

#[test]
fn the_tone_guide_reads_stops_through_the_clips_profile() {
    let f = common::footage();
    let p = analysed_project(&[&f.sharp]);
    let c = &p.clips[0];
    let std = tone_guide(c, ToneProfile::Standard);
    assert_eq!(std.stops.len(), 256);
    assert_eq!(std.clip_luma, 250);
    assert!(std.stops.windows(2).all(|w| w[1] >= w[0]), "stops rise with brightness");
    // Middle grey sits in the middle tones for standard footage, and higher on a log curve.
    let grey = |g: &selects_engine::frame::ToneGuide| g.stops.iter().position(|&s| s >= 0.0).unwrap();
    let slog = tone_guide(c, ToneProfile::SLog3);
    assert!((90..150).contains(&grey(&std)), "{}", grey(&std));
    assert!(grey(&slog) > 60 && grey(&slog) < 140, "{}", grey(&slog));
    assert!(slog.stops[255] > std.stops[255] + 2.0, "log holds more stops over grey");
}

#[test]
fn marker_notes_name_the_camera_move() {
    use selects_engine::export::{SelectMeta, Tier};
    let meta = |camera_move: bool, camera: CameraMove| SelectMeta {
        number: 1,
        start_seconds: 0.0,
        end_seconds: 4.0,
        focus: 0.9,
        exposure: 0.8,
        stability: 0.7,
        score: 0.8,
        tier: Tier::from_score(0.8),
        exposure_note: None,
        camera_move,
        camera,
    };
    assert!(meta(true, CameraMove::PanLeft).marker_note().ends_with(" Pan left."));
    assert!(meta(true, CameraMove::TiltUp).marker_note().ends_with(" Tilt up."));
    assert!(meta(true, CameraMove::Other).marker_note().ends_with(" Camera move."));
    assert!(meta(false, CameraMove::Still).marker_note().ends_with("(overall 80)."));
}
