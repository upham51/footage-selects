//! Learning from finished videos: the report counts how many used shots the
//! settings keep, and fitting moves the sliders toward what was used.

use selects_engine::analyze::{ClipAnalysis, FrameTable};
use selects_engine::matching::{MatchReport, MatchedShot, SourceRange};
use selects_engine::metrics::FrameMetrics;
use selects_engine::probe::ClipInfo;
use selects_engine::project::Project;
use selects_engine::rate::Rate;
use selects_engine::taste::{fit, report, FinishedVideo, MissReason};

fn info(name: &str, duration: f64) -> ClipInfo {
    ClipInfo {
        path: format!("/footage/{name}"),
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
    }
}

/// A 30 s clip: sharp and well exposed throughout, steady except for
/// handheld stretches at the given seconds (moderate shake, stability about 0.35).
fn clip_frames(handheld: &[(f64, f64)]) -> FrameTable {
    let mut t = FrameTable::default();
    for i in 0..120 {
        let time = i as f64 * 0.25;
        let shaky = handheld.iter().any(|&(a, b)| time >= a && time < b);
        let jitter = if shaky { if i % 2 == 0 { 3.3 } else { -3.3 } } else { 0.0 };
        let m = FrameMetrics {
            lap: 300.0,
            mean: 120.0,
            lo: 0.0,
            hi: 0.0,
            p2: 10.0,
            p98: 240.0,
            dx: jitter,
            dy: 0.0,
            motion_ok: true,
            ..Default::default()
        };
        t.push(time, &m);
    }
    t
}

fn project() -> Project {
    let mut p = Project::new();
    p.selection.max_per_clip = 0;
    for (k, handheld) in [vec![(4.0, 10.0)], vec![(12.0, 20.0)], vec![(20.0, 26.0)], vec![]].into_iter().enumerate() {
        let inf = info(&format!("C{k:04}.MP4"), 30.0);
        let id = p.add_clip(&inf.path.clone(), Ok(inf)).unwrap();
        let mut a = ClipAnalysis::empty(4.0, 480, 270);
        a.frames = clip_frames(&handheld);
        a.complete = true;
        p.clip_mut(id).unwrap().analysis = Some(a);
    }
    p
}

fn video(name: &str, ranges: &[(u64, f64, f64)]) -> FinishedVideo {
    let mut t = 0.0;
    let shots = ranges
        .iter()
        .map(|&(clip_id, start, end)| {
            let s = MatchedShot {
                ref_start: t,
                ref_end: t + (end - start),
                source: Some(SourceRange { clip_id, start, end, speed: 1.0, confidence: 0.9, crop: "full".into() }),
            };
            t += end - start;
            s
        })
        .collect();
    FinishedVideo { path: format!("/exports/{name}"), name: name.into(), duration: t, report: MatchReport { shots }, clips_searched: 4, print: None }
}

#[test]
fn the_report_counts_used_shots_and_names_why_the_rest_are_dropped() {
    let mut p = project();
    // The edit used two handheld moments and one steady one.
    p.finished_videos.push(video("Lisbon.mp4", &[(1, 5.0, 8.0), (2, 13.0, 17.0), (4, 2.0, 6.0)]));
    let r = report(&p);
    assert_eq!(r.used, 3);
    assert_eq!(r.kept, 1);
    assert_eq!(r.clips_used, 3);
    assert_eq!(r.misses, vec![(MissReason::Stability, 2)]);
}

#[test]
fn fitting_learns_that_this_editor_uses_handheld_moments() {
    let mut p = project();
    p.finished_videos.push(video("Lisbon.mp4", &[(1, 5.0, 8.0), (2, 13.0, 17.0)]));
    p.finished_videos.push(video("Porto.mp4", &[(3, 21.0, 25.0), (4, 10.0, 14.0)]));
    let before = p.selection.stability_min;
    let f = fit(&p).expect("there are used shots to learn from");
    assert!(f.settings.stability_min < before, "{} -> {}", before, f.settings.stability_min);
    assert!(f.after.kept > f.before.kept);
    assert_eq!(f.after.kept, f.after.used);
    // With two videos, each is checked with settings learned from the other.
    let held = f.held_out_kept_share.unwrap();
    let held_before = f.held_out_before_share.unwrap();
    assert!(held > held_before, "{held} vs {held_before}");
}

#[test]
fn nothing_to_fit_without_finished_videos() {
    assert!(fit(&project()).is_none());
}

#[test]
fn finished_videos_survive_saving_and_opening() {
    let mut p = project();
    p.finished_videos.push(video("Lisbon.mp4", &[(1, 5.0, 8.0)]));
    let back = Project::from_json(&p.to_json()).unwrap();
    assert_eq!(back.finished_videos, p.finished_videos);
}

#[test]
fn fitting_two_hours_of_footage_takes_seconds() {
    // 170 clips of 42 s at 4 samples per second, two finished videos of 30 shots.
    let mut p = Project::new();
    let mut seed: u32 = 7;
    let mut rnd = || {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        ((seed >> 16) & 0x7fff) as f32 / 32768.0
    };
    for c in 0..170 {
        let inf = info(&format!("C{c:04}.MP4"), 42.5);
        let id = p.add_clip(&inf.path.clone(), Ok(inf)).unwrap();
        let mut t = FrameTable::default();
        let mut lap = 150.0;
        for i in 0..170 {
            lap = (lap + (rnd() - 0.5) * 60.0).clamp(5.0, 400.0);
            let j = if rnd() < 0.3 { (rnd() - 0.5) * 8.0 } else { 0.0 };
            t.push(i as f64 * 0.25, &FrameMetrics { lap, mean: 120.0, p2: 10.0, p98: 240.0, dx: j, motion_ok: true, ..Default::default() });
        }
        let mut a = ClipAnalysis::empty(4.0, 480, 270);
        a.frames = t;
        a.complete = true;
        p.clip_mut(id).unwrap().analysis = Some(a);
    }
    for v in 0..2 {
        let ranges: Vec<(u64, f64, f64)> = (0..30).map(|k| (1 + ((k * 5 + v * 3) % 170) as u64, 10.0, 14.0)).collect();
        p.finished_videos.push(video(&format!("v{v}.mp4"), &ranges));
    }
    let start = std::time::Instant::now();
    let f = fit(&p).unwrap();
    let took = start.elapsed().as_secs_f64();
    println!("fit with hold-out check took {took:.2} s");
    assert!(f.after.kept >= f.before.kept);
    assert!(took < 30.0, "{took}");
}
