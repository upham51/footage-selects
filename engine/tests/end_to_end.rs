//! The new measurements on real files: sound, picture checks, stills, report.

mod common;

use selects_engine::analyze::{analyze_clip, AnalyzeOptions};
use selects_engine::probe::probe;
use selects_engine::project::Project;
use std::sync::atomic::AtomicBool;

fn analyzed_project(paths: &[&std::path::Path]) -> Project {
    let mut p = Project::new();
    for path in paths {
        let info = probe(path).unwrap();
        // Some shared fixtures are the same file under two names.
        let Some(id) = p.add_clip(&path.to_string_lossy(), Ok(info.clone())) else { continue };
        let opts = AnalyzeOptions { hardware_decode: false, ..Default::default() };
        let a = analyze_clip(&info, &opts, &AtomicBool::new(false), |_| {}).unwrap();
        p.clip_mut(id).unwrap().analysis = Some(a);
    }
    p
}

#[test]
fn analysis_measures_sound_and_picture_checks() {
    let f = common::footage();
    let p = analyzed_project(&[&f.with_audio_25p, &f.steady]);
    let a = p.clips[0].analysis.as_ref().unwrap();
    assert!(a.frames.has_picture_checks());
    let t = a.audio.as_ref().expect("the clip with sound has a sound table");
    assert!(!t.is_empty());
    assert!(a.sound.is_some());
    assert!(a.audio_error.is_none());
    let b = p.clips[1].analysis.as_ref().unwrap();
    assert!(b.audio.is_none(), "a clip without sound has no sound table");
    // A steady test pattern is neither flickering nor wobbling.
    let sel = p.selections();
    for g in sel.iter().flat_map(|s| s.kept()) {
        assert!(!g.flickers(), "flicker {:?}", g.flicker);
        assert!(!g.jello(), "wobble {:?}", g.wobble);
    }
    // Projects with the new measurements save and load.
    let back = Project::from_json(&p.to_json()).unwrap();
    assert_eq!(back.clips[0].analysis, p.clips[0].analysis);
}

#[test]
fn stills_and_a_contact_sheet_are_written_for_every_select() {
    let f = common::footage();
    let mut p = analyzed_project(&[&f.sharp, &f.panning]);
    p.selection.min_length = 1.0;
    let n: usize = p.selections().iter().map(|s| s.kept().count()).sum();
    assert!(n > 0, "the sharp test clips have selects");
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("stills-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut seen = 0;
    let w = selects_engine::stills::export_stills(&p, &dir, "Test day", &AtomicBool::new(false), |d, _| seen = seen.max(d)).unwrap();
    assert_eq!(w.written, n);
    assert!(w.failed.is_empty(), "{:?}", w.failed);
    let jpgs: Vec<_> = std::fs::read_dir(&w.folder).unwrap().flatten().filter(|e| e.path().extension().map(|x| x == "jpg").unwrap_or(false)).collect();
    assert_eq!(jpgs.len(), n);
    for e in &jpgs {
        let bytes = std::fs::read(e.path()).unwrap();
        assert_eq!(&bytes[..2], &[0xFF, 0xD8], "a JPEG file");
        assert!(w.folder.join("small").join(e.file_name()).exists());
    }
    let sheet = std::fs::read_to_string(&w.sheet).unwrap();
    assert!(sheet.contains("<img src=\"small/") && sheet.contains("Test day"));
}

#[test]
fn the_shoot_report_reads_the_project() {
    let f = common::footage();
    let p = analyzed_project(&[&f.sharp, &f.blurred, &f.shaky, &f.panning]);
    let r = selects_engine::report::shoot_report(&p);
    assert_eq!(r.clips_analyzed, p.clips.len());
    assert!(!r.findings.is_empty());
    assert_eq!(r.findings[0].title, "What came out of the footage");
    let html = selects_engine::report::report_html("Test report", &r);
    assert!(html.contains("What came out of the footage"));
    assert!(!html.contains('\u{2014}'), "no em dashes in the report");
}

#[test]
fn camera_files_give_the_recording_time() {
    let sony = selects_engine::sidecar::parse_sony_xml(r#"<NonRealTimeMeta><CreationDate value="2024-05-02T18:01:23+02:00"/><Device manufacturer="Sony" modelName="ILCE-7SM3"/></NonRealTimeMeta>"#);
    assert_eq!(sony.recorded.as_deref(), Some("2024-05-02T18:01:23+02:00"));
    let dji = selects_engine::sidecar::parse_dji_srt("1\n00:00:00,000 --> 00:00:00,033\n<font size=\"28\">FrameCnt: 1, DiffTime: 33ms\n2024-05-02 18:01:23.456\n[iso: 100] [shutter: 1/1000.0] [latitude: 47.376887] [longitude: 8.541694]</font>\n");
    assert_eq!(dji.recorded.as_deref(), Some("2024-05-02 18:01:23"));
    let ts = selects_engine::solar::parse_timestamp(sony.recorded.as_deref().unwrap()).unwrap();
    assert_eq!(ts.offset_minutes, Some(120));
}
