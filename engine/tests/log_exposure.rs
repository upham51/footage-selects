//! Exposure on log footage: judged in stops through the camera's curve, not
//! on how flat the picture looks.

mod common;

use selects_engine::analyze::{analyze_clip, AnalyzeOptions, ClipAnalysis, FrameTable};
use selects_engine::metrics::measure_frame;
use selects_engine::probe::probe;
use selects_engine::project::{ProfileSource, Project};
use selects_engine::scores::{score_frames_with, ToneContext};
use selects_engine::tone::{ExposureIssue, ToneProfile};
use std::sync::atomic::AtomicBool;

/// Sony's S-Log3 curve: scene light (0.18 is middle grey) to a 10-bit code.
fn slog3_code(x: f64) -> f64 {
    if x >= 0.01125 {
        420.0 + ((x + 0.01) / 0.19).log10() * 261.5
    } else {
        x * (171.2102946929 - 95.0) / 0.01125 + 95.0
    }
}

/// The 8-bit analysis luma a legal-range file gives for a 10-bit code.
fn luma(code: f64) -> u8 {
    (((code - 64.0) / 876.0) * 255.0).round().clamp(0.0, 255.0) as u8
}

/// A textured scene with tones spread around `key` stops from middle grey,
/// recorded in S-Log3. `clip_share` of the picture is a blown sky at the top
/// of the curve.
fn slog3_scene(w: usize, h: usize, key: f64, clip_share: f64) -> Vec<u8> {
    slog3_scene_range(w, h, key, 3.0, clip_share)
}

/// As `slog3_scene`, with tones spread `half` stops either side of the key.
fn slog3_scene_range(w: usize, h: usize, key: f64, half: f64, clip_share: f64) -> Vec<u8> {
    let mut seed: u32 = 99;
    let mut out = vec![0u8; w * h];
    let sky_rows = (h as f64 * clip_share).round() as usize;
    for y in 0..h {
        for x in 0..w {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            let noise = ((seed >> 16) & 0xff) as f64 / 255.0 - 0.5;
            let code = if y < sky_rows {
                // Sensor clip: S-Log3 tops out around 94% of the legal range.
                64.0 + 0.94 * 876.0
            } else {
                // A smooth scene from 3 stops under to 3 over the key, plus grain.
                let s = key + half * ((x as f64 / w as f64) * 2.0 - 1.0) * (0.6 + 0.4 * (y as f64 / 13.0).sin()) + noise * 0.4;
                slog3_code(0.18 * 2f64.powf(s))
            };
            out[y * w + x] = luma(code);
        }
    }
    out
}

fn table_of(frames: &[Vec<u8>], w: usize, h: usize) -> FrameTable {
    let mut t = FrameTable::default();
    let mut prev = None;
    for (i, f) in frames.iter().enumerate() {
        let (m, p) = measure_frame(f, w, h, prev.as_ref());
        prev = Some(p);
        t.push(i as f64 * 0.25, &m);
    }
    t
}

fn mean(v: &[f32]) -> f32 {
    v.iter().sum::<f32>() / v.len() as f32
}

const W: usize = 240;
const H: usize = 136;
const SLOG3: ToneContext = ToneContext { profile: ToneProfile::SLog3, full_range: false };
const STANDARD: ToneContext = ToneContext { profile: ToneProfile::Standard, full_range: false };

#[test]
fn a_soft_light_slog3_shot_is_marked_down_as_standard_but_scores_well_as_log() {
    // Overcast light: about 3 stops from shadow to highlight.
    let frames: Vec<Vec<u8>> = (0..8).map(|_| slog3_scene_range(W, H, 0.0, 1.5, 0.0)).collect();
    let t = table_of(&frames, W, H);
    let as_standard = mean(&score_frames_with(&t, 4.0, W as u32, STANDARD).exposure);
    let as_log = score_frames_with(&t, 4.0, W as u32, SLOG3);
    assert!(as_standard < 0.7, "standard rules see log as flat: {as_standard}");
    assert!(mean(&as_log.exposure) > 0.8, "log rules: {}", mean(&as_log.exposure));
    let key = mean(&as_log.key_stops);
    assert!(key.abs() < 0.6, "middle tones read {key} stops");
}

#[test]
fn underexposed_slog3_is_caught_and_named() {
    let frames: Vec<Vec<u8>> = (0..8).map(|_| slog3_scene(W, H, -4.0, 0.0)).collect();
    let t = table_of(&frames, W, H);
    let s = score_frames_with(&t, 4.0, W as u32, SLOG3);
    assert!(mean(&s.exposure) < 0.45, "{}", mean(&s.exposure));
    let issue = ExposureIssue::from_code(s.issue[3]);
    assert!(matches!(issue, ExposureIssue::Underexposed | ExposureIssue::DeepShadows), "{issue:?}");
    let key = mean(&s.key_stops);
    assert!((key + 4.0).abs() < 0.8, "middle tones read {key} stops");
}

#[test]
fn two_and_a_half_stops_under_passes_standard_rules_but_not_log_rules() {
    // Log's raised blacks keep the picture looking bright enough to standard rules.
    let frames: Vec<Vec<u8>> = (0..8).map(|_| slog3_scene(W, H, -2.5, 0.0)).collect();
    let t = table_of(&frames, W, H);
    let as_standard = mean(&score_frames_with(&t, 4.0, W as u32, STANDARD).exposure);
    let s = score_frames_with(&t, 4.0, W as u32, SLOG3);
    assert!(as_standard > 0.7, "{as_standard}");
    assert!(mean(&s.exposure) < 0.7, "{}", mean(&s.exposure));
    assert_eq!(ExposureIssue::from_code(s.issue[3]), ExposureIssue::Underexposed);
}

#[test]
fn a_blown_sky_in_slog3_is_caught_though_standard_rules_never_see_it() {
    let frames: Vec<Vec<u8>> = (0..8).map(|_| slog3_scene(W, H, 0.5, 0.2)).collect();
    let t = table_of(&frames, W, H);
    // The clip point is far below 255, so the standard highlight count stays at zero.
    assert!(t.hi.iter().all(|&h| h == 0.0));
    let s = score_frames_with(&t, 4.0, W as u32, SLOG3);
    assert_eq!(ExposureIssue::from_code(s.issue[2]), ExposureIssue::ClippedHighlights);
    assert!(mean(&s.exposure) < 0.3, "{}", mean(&s.exposure));
    assert!(mean(&s.clipped) > 0.15);
}

#[test]
fn a_bit_bright_is_forgiven_on_log() {
    // Exposing a stop and a half over keeps shadows clean; it should not cost much.
    let frames: Vec<Vec<u8>> = (0..8).map(|_| slog3_scene(W, H, 1.5, 0.0)).collect();
    let t = table_of(&frames, W, H);
    let s = score_frames_with(&t, 4.0, W as u32, SLOG3);
    assert!(mean(&s.exposure) > 0.8, "{}", mean(&s.exposure));
}

fn project_with_log_tones() -> Project {
    let path = common::footage().sharp.clone();
    let mut p = Project::new();
    let info = probe(&path).unwrap();
    let id = p.add_clip(&info.path.clone(), Ok(info)).unwrap();
    let frames: Vec<Vec<u8>> = (0..12).map(|_| slog3_scene_range(W, H, 0.0, 1.5, 0.0)).collect();
    let mut a = ClipAnalysis::empty(4.0, W as u32, H as u32);
    a.frames = table_of(&frames, W, H);
    a.complete = true;
    p.clip_mut(id).unwrap().analysis = Some(a);
    p
}

#[test]
fn profile_is_chosen_by_hand_then_camera_then_project_then_picture() {
    let mut p = project_with_log_tones();
    let fb = p.fallback_profile;
    // The test clip names no profile, but its tones look like log.
    let r = p.clips[0].resolved_profile(fb);
    assert_eq!((r.profile, r.source), (ToneProfile::GenericLog, ProfileSource::Picture));
    // The project's choice for clips that do not say comes next.
    p.fallback_profile = Some(ToneProfile::SLog3);
    let r = p.clips[0].resolved_profile(p.fallback_profile);
    assert_eq!((r.profile, r.source), (ToneProfile::SLog3, ProfileSource::Project));
    // A camera that names its profile wins over the project's choice.
    p.clips[0].info.as_mut().unwrap().detected_profile = Some(selects_engine::tone::DetectedProfile {
        profile: ToneProfile::VLog,
        source: "test".into(),
    });
    let r = p.clips[0].resolved_profile(p.fallback_profile);
    assert_eq!((r.profile, r.source), (ToneProfile::VLog, ProfileSource::Camera));
    // A choice made for this clip wins over everything.
    p.clips[0].profile = Some(ToneProfile::DLogM);
    let r = p.clips[0].resolved_profile(p.fallback_profile);
    assert_eq!((r.profile, r.source), (ToneProfile::DLogM, ProfileSource::Chosen));
}

#[test]
fn changing_the_profile_changes_selects_without_analyzing_again() {
    let mut p = project_with_log_tones();
    // Only exposure matters here.
    p.selection.exposure_min = 0.8;
    p.selection.focus_min = 0.0;
    p.selection.stability_min = 0.0;
    p.fallback_profile = Some(ToneProfile::Standard);
    let as_standard = p.select_all().1.selects_seconds;
    p.fallback_profile = Some(ToneProfile::SLog3);
    let as_log = p.select_all().1.selects_seconds;
    assert!(as_log > as_standard, "log {as_log} vs standard {as_standard}");
    // Round trip through the project file keeps the choice.
    let back = Project::from_json(&p.to_json()).unwrap();
    assert_eq!(back.fallback_profile, Some(ToneProfile::SLog3));
}

#[test]
fn sony_and_dji_sidecars_are_read_when_footage_is_added() {
    let dir = common::fixtures_dir().join("sidecars");
    std::fs::create_dir_all(&dir).unwrap();
    let src = common::footage().sharp.clone();
    for name in ["C0001.MP4", "DJI_0001.MP4"] {
        let dst = dir.join(name);
        if !dst.exists() {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
    std::fs::write(
        dir.join("C0001M01.XML"),
        r#"<NonRealTimeMeta><Device manufacturer="Sony" modelName="ILCE-7M4"/><AcquisitionRecord><Group name="CameraUnitMetadataSet"><Item name="CaptureGammaEquation" value="s-log3-cine"/></Group></AcquisitionRecord></NonRealTimeMeta>"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("DJI_0001.SRT"),
        "1\n00:00:00,000 --> 00:00:00,033\n[iso: 100] [shutter: 1/2000.0] [fnum: 1.7] [color_md: dlog_m] [latitude: 47.1] [longitude: 8.5] [rel_alt: 80.0 abs_alt: 500.0]\n",
    )
    .unwrap();
    let sony = probe(&dir.join("C0001.MP4")).unwrap();
    assert_eq!(sony.detected_profile.as_ref().unwrap().profile, ToneProfile::SLog3);
    assert!(sony.detected_profile.unwrap().source.contains("C0001M01.XML"));
    let dji = probe(&dir.join("DJI_0001.MP4")).unwrap();
    assert_eq!(dji.detected_profile.as_ref().unwrap().profile, ToneProfile::DLogM);
    let notes = dji.camera.unwrap();
    assert!(notes.shutter_warning(dji.rate.fps()).is_some());
    assert_eq!(notes.max_height, Some(80.0));
}

#[test]
fn analysis_fills_the_ladder_and_fingerprints() {
    let info = probe(&common::footage().sharp).unwrap();
    let a = analyze_clip(&info, &AnalyzeOptions::default(), &AtomicBool::new(false), |_| {}).unwrap();
    assert!(a.frames.has_ladder());
    assert_eq!(a.frames.fp.len(), a.frames.len());
    assert!(a.frames.fp.iter().any(|h| h[0] != 0));
}
