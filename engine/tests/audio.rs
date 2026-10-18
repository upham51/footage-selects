//! Sound measured from real files decoded by ffmpeg.

mod common;

use selects_engine::analyze::AnalyzeError;
use selects_engine::audio::{analyze_audio, issues_in, summarize, AudioIssue, AudioTable};
use selects_engine::probe::probe;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

/// Make a test file once with ffmpeg (tests run in parallel and share fixtures).
fn make(name: &str, args: &[&str]) -> PathBuf {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let out = common::fixtures_dir().join(name);
    if out.exists() {
        return out;
    }
    let tmp = out.with_extension(format!("tmp.{}", out.extension().unwrap().to_string_lossy()));
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-v", "error", "-y"])
        .args(args)
        .arg(&tmp)
        .status()
        .expect("ffmpeg must be installed to run the audio tests");
    assert!(status.success(), "ffmpeg failed for {name}");
    std::fs::rename(&tmp, &out).unwrap();
    out
}

const VIDEO: [&str; 4] = ["-f", "lavfi", "-i", "testsrc2=s=320x240:r=25:d=4"];
const VIDEO_CODEC: [&str; 4] = ["-c:v", "mpeg4", "-q:v", "5"];

fn measure(path: &Path) -> Option<AudioTable> {
    let info = probe(path).expect("probe");
    analyze_audio(&info, &AtomicBool::new(false)).expect("audio analysis")
}

#[test]
fn audio_mono_aac_tone() {
    let path = make(
        "audio_mono_tone.mp4",
        &[&VIDEO[..], &["-f", "lavfi", "-i", "sine=f=440:sample_rate=48000:d=4"], &VIDEO_CODEC[..], &["-c:a", "aac", "-ac", "1", "-shortest"]].concat(),
    );
    let t = measure(&path).expect("has audio");
    assert_eq!(t.channels, 1);
    assert_eq!(t.sample_rate, 48000);
    assert!((t.len() as i64 - 80).abs() <= 2, "{}", t.len());
    // ffmpeg's sine source is at 1/8 of full scale: about -21 dBFS RMS.
    let mid = t.level_db(40);
    assert!((mid + 21.0).abs() < 1.5, "{mid}");
    let s = summarize(&t);
    assert!(!s.has_speech && !s.silent, "{s:?}");
    assert!(s.dropouts.is_empty() && s.clipped_events == 0 && s.wind_share == 0.0, "{s:?}");
}

#[test]
fn audio_stereo_keeps_channels_and_counts_clipping() {
    // Left clipped hard, right clean, as 16-bit PCM so the flat tops survive.
    let path = make(
        "audio_stereo_clipped.mov",
        &[
            &VIDEO[..],
            &["-f", "lavfi", "-i", "aevalsrc=clip(1.5*sin(2*PI*440*t)\\,-1\\,1)|0.2*sin(2*PI*220*t):s=48000:d=4"],
            &VIDEO_CODEC[..],
            &["-c:a", "pcm_s16le", "-shortest"],
        ]
        .concat(),
    );
    let info = probe(&path).unwrap();
    assert_eq!(info.audio_channels, 2);
    let t = measure(&path).expect("has audio");
    assert_eq!(t.channels, 2);
    let s = summarize(&t);
    assert!(s.clipped_events > 3000, "{s:?}");
    assert_eq!(issues_in(&t, 1.0, 2.0), vec![AudioIssue::Clipped]);
}

#[test]
fn audio_surround_is_mixed_to_stereo() {
    let path = make(
        "audio_surround.mov",
        &[
            &VIDEO[..],
            &["-f", "lavfi", "-i", "sine=f=300:sample_rate=48000:d=4", "-ac", "6"],
            &VIDEO_CODEC[..],
            &["-c:a", "pcm_s16le", "-shortest"],
        ]
        .concat(),
    );
    assert_eq!(probe(&path).unwrap().audio_channels, 6);
    let t = measure(&path).expect("has audio");
    assert_eq!(t.channels, 2);
    assert!(t.level_db(40) > -40.0);
}

#[test]
fn audio_lines_up_with_the_picture() {
    // The sound starts 1 s after the picture.
    let path = make(
        "audio_late.mov",
        &[
            &VIDEO[..],
            &["-itsoffset", "1", "-f", "lavfi", "-i", "sine=f=440:sample_rate=48000:d=3"],
            &VIDEO_CODEC[..],
            &["-c:a", "pcm_s16le"],
        ]
        .concat(),
    );
    let t = measure(&path).expect("has audio");
    let first = (0..t.len()).find(|&i| t.level_db(i) > -40.0).unwrap();
    assert!((t.time_of(first) - 1.0).abs() <= 0.05, "{}", t.time_of(first));
    assert!((t.len() as i64 - 80).abs() <= 2, "{}", t.len());
}

#[test]
fn audio_dropout_in_a_file() {
    let path = make(
        "audio_dropout.mov",
        &[
            &VIDEO[..],
            &["-f", "lavfi", "-i", "aevalsrc=0.3*sin(2*PI*440*t)*not(between(t\\,2\\,2.3)):s=48000:d=4"],
            &VIDEO_CODEC[..],
            &["-c:a", "pcm_s16le", "-shortest"],
        ]
        .concat(),
    );
    let s = summarize(&measure(&path).unwrap());
    assert_eq!(s.dropouts.len(), 1, "{s:?}");
    assert!((s.dropouts[0].0 - 2.0).abs() <= 0.06 && (s.dropouts[0].1 - 2.3).abs() <= 0.06, "{s:?}");
}

#[test]
fn audio_none_without_a_sound_track() {
    let path = make("audio_none.mp4", &[&VIDEO[..], &VIDEO_CODEC[..]].concat());
    let mut info = probe(&path).unwrap();
    assert!(!info.has_audio);
    assert_eq!(analyze_audio(&info, &AtomicBool::new(false)), Ok(None));
    // Even when told there is sound, a file without it gives None.
    info.has_audio = true;
    info.audio_channels = 2;
    info.audio_sample_rate = 48000;
    assert_eq!(analyze_audio(&info, &AtomicBool::new(false)), Ok(None));
}

#[test]
fn audio_stops_when_cancelled() {
    let path = make(
        "audio_mono_tone_cancel.mp4",
        &[&VIDEO[..], &["-f", "lavfi", "-i", "sine=f=440:sample_rate=48000:d=4"], &VIDEO_CODEC[..], &["-c:a", "aac", "-shortest"]].concat(),
    );
    let info = probe(&path).unwrap();
    assert_eq!(analyze_audio(&info, &AtomicBool::new(true)), Err(AnalyzeError::Cancelled));
}
