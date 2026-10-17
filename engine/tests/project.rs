//! Projects save, reload and never re-analyse unless they must.

mod common;

use selects_engine::analyze::{analyze_clip, AnalyzeOptions};
use selects_engine::probe::probe;
use selects_engine::project::{FileStatus, Project, ProjectError, PROJECT_FORMAT};
use std::sync::atomic::AtomicBool;

fn analysed_project(paths: &[&std::path::Path]) -> Project {
    let mut p = Project::new();
    for path in paths {
        let info = probe(path).map_err(|e| e.to_string());
        let id = p.add_clip(&path.to_string_lossy(), info).unwrap();
        let clip = p.clip(id).unwrap().clone();
        let a = analyze_clip(
            clip.info.as_ref().unwrap(),
            &AnalyzeOptions::default(),
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        p.clip_mut(id).unwrap().analysis = Some(a);
    }
    p
}

#[test]
fn save_and_reload_round_trips_everything() {
    let f = common::footage();
    let mut p = analysed_project(&[&f.sharp, &f.blurred, &f.shaky]);
    p.selection.focus_min = 0.42;
    p.selection.max_per_clip = 5;
    p.export.timeline_name = "Day 1 selects".into();
    let bad = common::fixtures_dir().join("not_video_project.mov");
    std::fs::write(&bad, b"nope").unwrap();
    p.add_clip(&bad.to_string_lossy(), probe(&bad).map_err(|e| e.to_string()));

    let path = common::fixtures_dir().join("roundtrip.selects");
    p.save(&path).unwrap();
    let q = Project::load(&path).unwrap();
    assert_eq!(p, q);
    assert_eq!(q.format, PROJECT_FORMAT);

    // Selection on the reloaded project matches the original exactly.
    assert_eq!(p.select_all(), q.select_all());
    let (_, summary) = q.select_all();
    assert_eq!(summary.clips_total, 4);
    assert_eq!(summary.clips_analyzed, 3);
    assert_eq!(summary.clips_failed, 1);
    assert!((summary.footage_seconds - 10.0).abs() < 0.2, "{summary:?}");

    // Nothing needs analysing again after reopening.
    assert!(q.clips_needing_analysis().is_empty());

    // No temporary file left behind.
    assert!(!path.with_extension("selects-saving").exists());
    let size = std::fs::metadata(&path).unwrap().len();
    println!("project with 3 analysed clips: {size} bytes");
}

#[test]
fn only_sample_rate_or_changed_files_trigger_reanalysis() {
    let f = common::footage();
    // Work on a copy so the shared fixture is never touched.
    let copy = common::fixtures_dir().join("project_copy.mp4");
    std::fs::copy(&f.sharp, &copy).unwrap();
    let mut p = analysed_project(&[&copy, &f.blurred]);
    assert!(p.clips_needing_analysis().is_empty());

    // Selection settings never invalidate measurements.
    p.selection.focus_min = 0.9;
    p.selection.smoothing = 3.0;
    p.export.timeline_rate = Some(selects_engine::rate::Rate::new(25, 1));
    assert!(p.clips_needing_analysis().is_empty());

    // A different sample rate does.
    p.set_sample_rate(2.0);
    assert_eq!(p.clips_needing_analysis().len(), 2);
    p.set_sample_rate(4.0);
    assert!(p.clips_needing_analysis().is_empty());

    // A file edited after analysis is noticed.
    let file = std::fs::OpenOptions::new().write(true).open(&copy).unwrap();
    file.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(120))
        .unwrap();
    drop(file);
    let id = p.clips[0].id;
    assert_eq!(p.clip(id).unwrap().file_status(), FileStatus::Changed);
    assert_eq!(p.clips_needing_analysis(), vec![id]);

    // A missing file is reported, not queued, and its cached selects stay usable.
    std::fs::remove_file(&copy).unwrap();
    assert_eq!(p.clip(id).unwrap().file_status(), FileStatus::Missing);
    assert!(p.clips_needing_analysis().is_empty());
    p.selection.focus_min = 0.5;
    p.selection.smoothing = 1.0;
    let (sels, _) = p.select_all();
    assert!(sels[0].kept().count() > 0);
}

#[test]
fn adding_the_same_file_twice_is_ignored() {
    let f = common::footage();
    let mut p = Project::new();
    let path = f.sharp.to_string_lossy().to_string();
    assert!(p.add_clip(&path, probe(&f.sharp).map_err(|e| e.to_string())).is_some());
    assert!(p.add_clip(&path, probe(&f.sharp).map_err(|e| e.to_string())).is_none());
    assert_eq!(p.clips.len(), 1);
}

#[test]
fn damaged_or_newer_files_give_plain_errors() {
    let dir = common::fixtures_dir();
    let junk = dir.join("junk.selects");
    std::fs::write(&junk, "{not json").unwrap();
    let e = Project::load(&junk).unwrap_err();
    assert_eq!(e, ProjectError::NotAProject);
    assert!(e.to_string().contains("Nothing in it was changed"));

    let newer = dir.join("newer.selects");
    std::fs::write(&newer, format!("{{\"format\": {}, \"clips\": []}}", PROJECT_FORMAT + 1)).unwrap();
    assert_eq!(Project::load(&newer).unwrap_err(), ProjectError::TooNew(PROJECT_FORMAT + 1));

    let missing = dir.join("does_not_exist.selects");
    assert!(matches!(Project::load(&missing).unwrap_err(), ProjectError::Read(_)));
}

#[test]
fn partial_analysis_still_produces_selects() {
    let f = common::footage();
    let mut p = analysed_project(&[&f.sharp_blur_sharp]);
    // Pretend analysis stopped after the first 4 seconds.
    let a = p.clips[0].analysis.as_mut().unwrap();
    let keep = a.frames.t.iter().filter(|&&t| t < 4.0).count();
    let mut cut = a.frames.clone();
    for col in [
        &mut cut.t, &mut cut.lap, &mut cut.mean, &mut cut.lo, &mut cut.hi, &mut cut.p2,
        &mut cut.p98, &mut cut.dx, &mut cut.dy,
    ] {
        col.truncate(keep);
    }
    cut.motion_ok.truncate(keep);
    a.frames = cut;
    a.complete = false;
    assert!(p.clips[0].measured_until() < 4.2);
    let mut st = p.selection.clone();
    st.min_length = 1.0;
    p.selection = st;
    let (sels, summary) = p.select_all();
    assert_eq!(summary.clips_analyzed, 0);
    let kept: Vec<_> = sels[0].kept().collect();
    assert_eq!(kept.len(), 1);
    assert!(kept[0].end < 3.7);
    assert_eq!(p.clips_needing_analysis().len(), 1);
}

#[test]
fn selection_over_two_hours_of_footage_is_instant() {
    use selects_engine::analyze::{ClipAnalysis, FrameTable};
    use selects_engine::metrics::FrameMetrics;
    let f = common::footage();
    let template = probe(&f.sharp).unwrap();
    let mut p = Project::new();
    let mut seed: u32 = 7;
    let mut rnd = move || {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        ((seed >> 8) & 0xffff) as f32 / 65535.0
    };
    // 170 clips of about 42 s each: just over two hours at 4 samples per second.
    for c in 0..170 {
        let mut info = template.clone();
        info.path = format!("/footage/clip_{c:03}.mov");
        info.duration = 42.4;
        let id = p.add_clip(&info.path.clone(), Ok(info)).unwrap();
        let mut table = FrameTable::default();
        let mut lap = 150.0;
        for i in 0..170 {
            lap = (lap + (rnd() - 0.5) * 60.0).clamp(5.0, 400.0);
            let m = FrameMetrics {
                lap,
                mean: 100.0 + rnd() * 60.0,
                lo: 0.0,
                hi: rnd() * 0.05,
                p2: 10.0,
                p98: 230.0,
                dx: (rnd() - 0.5) * 6.0,
                dy: (rnd() - 0.5) * 4.0,
                motion_ok: true,
            };
            table.push(i as f64 * 0.25, &m);
        }
        let mut a = ClipAnalysis::empty(4.0, 480, 270);
        a.frames = table;
        a.complete = true;
        p.clip_mut(id).unwrap().analysis = Some(a);
    }
    let start = std::time::Instant::now();
    let runs = 10;
    let mut total = 0;
    for _ in 0..runs {
        let (sels, summary) = p.select_all();
        total += sels.len();
        assert!(summary.footage_seconds > 7200.0);
    }
    let per_run = start.elapsed().as_secs_f64() / runs as f64;
    println!("selection over {:.0} s of footage: {:.1} ms per run", 170.0 * 42.4, per_run * 1000.0);
    assert_eq!(total, 170 * runs);
    assert!(per_run < 0.1, "selection took {per_run} s");
}
