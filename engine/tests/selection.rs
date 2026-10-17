//! The selection step returns the expected segments for given thresholds.

mod common;

use selects_engine::analyze::{analyze_clip, AnalyzeOptions};
use selects_engine::probe::probe;
use selects_engine::scores::{score_clip, ScoreSeries};
use selects_engine::selection::{select_clip, smooth, SegmentStatus, SelectionSettings};
use std::sync::atomic::AtomicBool;

/// Build a 4 Hz series from per-second (focus, exposure, stability) values.
fn series(per_second: &[(f32, f32, f32)]) -> ScoreSeries {
    let mut s = ScoreSeries::default();
    for (sec, &(f, e, st)) in per_second.iter().enumerate() {
        for k in 0..4 {
            s.t.push(sec as f32 + k as f32 * 0.25);
            s.focus.push(f);
            s.exposure.push(e);
            s.stability.push(st);
        }
    }
    s
}

fn settings() -> SelectionSettings {
    SelectionSettings {
        focus_min: 0.5,
        exposure_min: 0.5,
        stability_min: 0.5,
        smoothing: 0.0,
        min_length: 2.0,
        max_length: 10.0,
        max_per_clip: 0,
    }
}

fn kept(sel: &selects_engine::selection::ClipSelection) -> Vec<(f64, f64)> {
    sel.kept().map(|s| (s.start, s.end)).collect()
}

fn approx(a: &[(f64, f64)], b: &[(f64, f64)]) -> bool {
    a.len() == b.len()
        && a
            .iter()
            .zip(b)
            .all(|(x, y)| (x.0 - y.0).abs() < 1e-6 && (x.1 - y.1).abs() < 1e-6)
}

const GOOD: (f32, f32, f32) = (0.9, 0.9, 0.9);
const SOFT: (f32, f32, f32) = (0.2, 0.9, 0.9);
const DARK: (f32, f32, f32) = (0.9, 0.1, 0.9);
const SHAKY: (f32, f32, f32) = (0.9, 0.9, 0.1);

#[test]
fn good_bad_good_gives_two_selects_and_names_the_failing_score() {
    let s = series(&[GOOD, GOOD, GOOD, SOFT, SOFT, SOFT, GOOD, GOOD, GOOD]);
    let sel = select_clip(&s, 9.0, &settings());
    // Boundaries fall halfway between the last good and first bad sample (2.75 and 3.0).
    assert!(approx(&kept(&sel), &[(0.0, 2.875), (5.875, 9.0)]), "{:?}", kept(&sel));
    let dropped: Vec<_> = sel
        .segments
        .iter()
        .filter(|g| g.status == SegmentStatus::BelowThreshold)
        .collect();
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0].failed, vec!["focus".to_string()]);
    assert!((dropped[0].start - 2.875).abs() < 1e-6 && (dropped[0].end - 5.875).abs() < 1e-6);
    // Segments tile the whole clip without gaps.
    for w in sel.segments.windows(2) {
        assert!((w[0].end - w[1].start).abs() < 1e-9);
    }
    assert_eq!(sel.segments.first().unwrap().start, 0.0);
    assert_eq!(sel.segments.last().unwrap().end, 9.0);
}

#[test]
fn each_threshold_is_respected_and_reported() {
    let s = series(&[GOOD, GOOD, GOOD, DARK, DARK, DARK, SHAKY, SHAKY, SHAKY, GOOD, GOOD, GOOD]);
    let sel = select_clip(&s, 12.0, &settings());
    assert_eq!(kept(&sel).len(), 2);
    let reasons: Vec<Vec<String>> = sel
        .segments
        .iter()
        .filter(|g| g.status == SegmentStatus::BelowThreshold)
        .map(|g| g.failed.clone())
        .collect();
    // Dark then shaky are adjacent failing stretches, so they form one run naming both.
    assert_eq!(reasons.len(), 1);
    assert!(reasons[0].contains(&"exposure".to_string()));
    assert!(reasons[0].contains(&"stability".to_string()));
    assert!(!reasons[0].contains(&"focus".to_string()));

    // Lowering the exposure and stability thresholds lets the whole clip through.
    let mut lenient = settings();
    lenient.exposure_min = 0.05;
    lenient.stability_min = 0.05;
    lenient.max_length = 20.0;
    let sel = select_clip(&s, 12.0, &lenient);
    assert!(approx(&kept(&sel), &[(0.0, 12.0)]), "{:?}", kept(&sel));

    // Raising focus above every value keeps nothing.
    let mut strict = settings();
    strict.focus_min = 0.95;
    assert_eq!(select_clip(&s, 12.0, &strict).kept().count(), 0);
}

#[test]
fn short_stretches_are_dropped_as_too_short() {
    let s = series(&[SOFT, SOFT, GOOD, SOFT, SOFT, GOOD, GOOD, GOOD, SOFT]);
    let sel = select_clip(&s, 9.0, &settings());
    let short: Vec<_> = sel
        .segments
        .iter()
        .filter(|g| g.status == SegmentStatus::TooShort)
        .collect();
    assert_eq!(short.len(), 1, "{:?}", sel.segments);
    assert!((short[0].duration() - 1.0).abs() < 1e-6);
    assert!(approx(&kept(&sel), &[(4.875, 7.875)]), "{:?}", kept(&sel));
}

#[test]
fn long_stretches_are_split_into_equal_parts_within_the_maximum() {
    let s = series(&[GOOD; 25]);
    let mut st = settings();
    st.max_length = 10.0;
    let sel = select_clip(&s, 25.0, &st);
    // 25 s split into 3 parts of 8.33 s each.
    let k = kept(&sel);
    assert_eq!(k.len(), 3);
    for (a, b) in &k {
        assert!((b - a - 25.0 / 3.0).abs() < 1e-6);
    }
    assert_eq!(k[0].0, 0.0);
    assert!((k[2].1 - 25.0).abs() < 1e-9);

    // When equal parts would be shorter than the minimum, parts are exactly the
    // maximum length, centred, with the ends trimmed.
    let s = series(&[GOOD; 11]);
    let mut st = settings();
    st.min_length = 6.0;
    st.max_length = 6.0;
    let sel = select_clip(&s, 11.0, &st);
    assert!(approx(&kept(&sel), &[(2.5, 8.5)]), "{:?}", kept(&sel));
}

#[test]
fn per_clip_cap_keeps_the_best_scoring_segments() {
    let a = (0.95, 0.95, 0.95);
    let b = (0.6, 0.6, 0.6);
    let c = (0.8, 0.8, 0.8);
    let d = (0.7, 0.7, 0.7);
    let s = series(&[a, a, a, SOFT, b, b, b, SOFT, c, c, c, SOFT, d, d, d]);
    let mut st = settings();
    st.max_per_clip = 2;
    let sel = select_clip(&s, 15.0, &st);
    let kept: Vec<f32> = sel.kept().map(|g| (g.score * 100.0).round() / 100.0).collect();
    // Kept in time order, but chosen by score: a (0.95) and c (0.8).
    assert_eq!(kept, vec![0.95, 0.8]);
    let over = sel
        .segments
        .iter()
        .filter(|g| g.status == SegmentStatus::OverLimit)
        .count();
    assert_eq!(over, 2);
}

#[test]
fn smoothing_bridges_a_single_bad_sample() {
    let mut s = series(&[GOOD; 8]);
    s.focus[16] = 0.0; // One soft sample at 4.0 s.
    let st = settings();
    assert_eq!(select_clip(&s, 8.0, &st).kept().count(), 2);
    let mut smoothed = st.clone();
    smoothed.smoothing = 1.0;
    let sel = select_clip(&s, 8.0, &smoothed);
    assert!(approx(&kept(&sel), &[(0.0, 8.0)]), "{:?}", kept(&sel));
}

#[test]
fn smoothing_is_a_centred_moving_average() {
    let t = [0.0, 0.25, 0.5, 0.75, 1.0];
    let v = [0.0, 0.0, 1.0, 0.0, 0.0];
    let out = smooth(&v, &t, 0.5);
    // Window of +/-0.25 s covers three samples.
    assert!((out[2] - 1.0 / 3.0).abs() < 1e-6);
    assert!((out[1] - 1.0 / 3.0).abs() < 1e-6);
    assert!((out[0] - 0.0).abs() < 1e-6);
    assert_eq!(smooth(&v, &t, 0.0), v.to_vec());
}

#[test]
fn real_clip_sharp_blurred_sharp_yields_the_two_sharp_parts() {
    let f = common::footage();
    let info = probe(&f.sharp_blur_sharp).unwrap();
    let a = analyze_clip(&info, &AnalyzeOptions::default(), &AtomicBool::new(false), |_| {}).unwrap();
    let scores = score_clip(&a);
    let mut st = SelectionSettings::default();
    st.min_length = 1.0;
    let sel = select_clip(&scores, info.duration, &st);
    let k = kept(&sel);
    println!("{:?}", sel.segments.iter().map(|g| (g.start, g.end, g.status)).collect::<Vec<_>>());
    assert_eq!(k.len(), 2, "{k:?}");
    // Blur runs from 3 s to 6 s. Smoothing over 1 s can move edges by up to half a second.
    assert!(k[0].0 < 0.01 && (k[0].1 - 3.0).abs() <= 0.6, "{k:?}");
    assert!((k[1].0 - 6.0).abs() <= 0.6 && (k[1].1 - info.duration).abs() < 0.01, "{k:?}");
    let below: Vec<_> = sel
        .segments
        .iter()
        .filter(|g| g.status == SegmentStatus::BelowThreshold)
        .collect();
    assert_eq!(below.len(), 1);
    assert_eq!(below[0].failed[0], "focus");
}
