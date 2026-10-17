//! Scores on synthetic clips must rank the way a person would rank them.

mod common;

use selects_engine::analyze::{analyze_clip, AnalyzeOptions, ClipAnalysis, Progress};
use selects_engine::probe::probe;
use selects_engine::rate::Rate;
use selects_engine::scores::{mean, score_clip, ScoreSeries};
use std::path::Path;
use std::sync::atomic::AtomicBool;

fn analyse(path: &Path) -> (ClipAnalysis, ScoreSeries) {
    let info = probe(path).expect("probe");
    let cancel = AtomicBool::new(false);
    let mut streamed = 0usize;
    let a = analyze_clip(&info, &AnalyzeOptions::default(), &cancel, |p| match p {
        Progress::Frames(t) => streamed += t.len(),
        Progress::Restart => streamed = 0,
    })
    .expect("analysis");
    assert_eq!(streamed, a.frames.len(), "progress reports every frame once");
    let s = score_clip(&a);
    (a, s)
}

/// Mean score over the middle of the clip, away from edge effects.
fn mid(v: &[f32]) -> f32 {
    let n = v.len();
    let a = n / 6;
    let b = n - n / 6;
    mean(&v[a..b.max(a + 1)])
}

#[test]
fn samples_at_the_requested_rate() {
    let f = common::footage();
    let (a, s) = analyse(&f.sharp);
    // 3 seconds at 4 samples per second.
    assert!((11..=13).contains(&a.frames.len()), "got {}", a.frames.len());
    assert_eq!(a.width, 480);
    assert_eq!(a.height, 270);
    // Sample times are real frame timestamps, within one 30 fps frame of the grid.
    for (i, t) in s.t.iter().enumerate() {
        assert!((t - i as f32 * 0.25).abs() <= 1.0 / 30.0 + 1e-3, "sample {i} at {t}");
    }
}

#[test]
fn sharp_ranks_above_blurred() {
    let f = common::footage();
    let (_, sharp) = analyse(&f.sharp);
    let (_, blur) = analyse(&f.blurred);
    let (fs, fb) = (mid(&sharp.focus), mid(&blur.focus));
    println!("focus sharp {fs:.3} blurred {fb:.3}");
    assert!(fs > fb + 0.3, "sharp {fs} should clearly beat blurred {fb}");
    assert!(fs > 0.7);
    assert!(fb < 0.45);
}

#[test]
fn well_exposed_ranks_above_dark_and_blown_out() {
    let f = common::footage();
    let (_, good) = analyse(&f.well_exposed);
    let (_, dark) = analyse(&f.dark);
    let (_, blown) = analyse(&f.blown_out);
    let (g, d, b) = (mid(&good.exposure), mid(&dark.exposure), mid(&blown.exposure));
    println!("exposure good {g:.3} dark {d:.3} blown {b:.3}");
    assert!(g > 0.7, "well exposed {g}");
    assert!(g > d + 0.4, "dark {d}");
    assert!(g > b + 0.4, "blown out {b}");
}

#[test]
fn steady_and_smooth_pan_rank_above_shaky() {
    let f = common::footage();
    let (_, steady) = analyse(&f.steady);
    let (_, pan) = analyse(&f.panning);
    let (pan_a, _) = analyse(&f.panning);
    let (_, shaky) = analyse(&f.shaky);
    let (_, fast) = analyse(&f.fast_pan);
    let fp = mid(&fast.stability);
    println!("stability fast pan {fp:.3}");
    assert!(fp > 0.75, "fast smooth pan {fp}");
    let (st, pa, sh) = (mid(&steady.stability), mid(&pan.stability), mid(&shaky.stability));
    println!("stability steady {st:.3} pan {pa:.3} shaky {sh:.3}");
    // The pan moves 60 px/s on a 1280 px frame: 22.5 px/s at 480 wide, 5.6 px per sample.
    let mid_dx: Vec<f32> = pan_a.frames.dx[2..pan_a.frames.len() - 1].to_vec();
    let avg_dx = mean(&mid_dx);
    println!("pan dx per sample {avg_dx:.2}");
    assert!((avg_dx.abs() - 5.6).abs() < 1.5, "pan speed measured {avg_dx}");
    assert!(st > 0.85, "steady {st}");
    assert!(pa > 0.75, "pan {pa}");
    assert!(st > sh + 0.35, "shaky {sh}");
    assert!(pa > sh + 0.3, "shaky {sh} vs pan {pa}");
}

#[test]
fn analysis_survives_rotation_and_variable_frame_rate() {
    let f = common::footage();
    let rot = probe(&f.rotated).unwrap();
    assert!(rot.rotation == 90 || rot.rotation == 270, "rotation {}", rot.rotation);
    assert_eq!((rot.width, rot.height), (720, 1280));
    let (a, _) = analyse(&f.rotated);
    assert_eq!(a.width, 480);
    assert_eq!(a.height, 854);
    assert!(a.frames.len() >= 7);

    let vfr = probe(&f.variable_rate).unwrap();
    assert!(vfr.variable_frame_rate, "{vfr:?}");
    let (a, _) = analyse(&f.variable_rate);
    assert!((11..=13).contains(&a.frames.len()), "vfr frames {}", a.frames.len());
}

#[test]
fn probe_reads_rates_timecode_and_audio() {
    let f = common::footage();
    let a = probe(&f.with_audio_25p).unwrap();
    assert_eq!(a.rate, Rate::new(25, 1));
    assert!(a.has_audio);
    assert_eq!(a.audio_channels, 2);
    assert_eq!(a.timecode.as_deref(), Some("10:00:00:00"));
    assert!((a.duration - 2.0).abs() < 0.1);

    let b = probe(&f.timecode_2997_df).unwrap();
    assert_eq!(b.rate, Rate::new(30000, 1001));
    assert!(!b.has_audio);
    assert_eq!(b.timecode.as_deref(), Some("01:00:00;00"));

    let c = probe(&f.sharp).unwrap();
    assert_eq!(c.rate, Rate::new(30, 1));
    assert!(!c.has_audio);
    assert_eq!(c.timecode, None);
}

#[test]
fn unreadable_file_reports_a_plain_error() {
    let p = common::fixtures_dir().join("not_video.mov");
    std::fs::write(&p, b"this is not a video file").unwrap();
    let err = probe(&p).unwrap_err().to_string();
    assert!(err.contains("could not be read as video"), "{err}");
}

#[test]
fn analysis_speed_is_reported() {
    let f = common::footage();
    let (a, _) = analyse(&f.panning);
    println!("4 s clip analysed in {:.3} s, decoder: {}", a.elapsed, a.decoder);
    // On macOS the GPU path should be the one that succeeds.
    if cfg!(target_os = "macos") {
        println!("macOS decoder in use: {}", a.decoder);
    }
    assert!(a.elapsed > 0.0);
}
