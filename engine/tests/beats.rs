//! Beats and bars of music files made with ffmpeg, read back through the
//! same probe and decode path the app uses.

mod common;

use selects_engine::beats::{beat_grid, cut_points, decode_mono, probe_music, BeatGrid, ANALYSIS_RATE, CANCELLED};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

/// Run ffmpeg once per output file; tests share fixtures and run in parallel.
fn make(name: &str, args: &[&str]) -> PathBuf {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let out = common::fixtures_dir().join(name);
    if out.exists() {
        return out;
    }
    let ext = out.extension().unwrap().to_string_lossy().into_owned();
    let tmp = out.with_extension(format!("tmp.{ext}"));
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-v", "error", "-y"])
        .args(args)
        .arg(&tmp)
        .status()
        .expect("ffmpeg must be installed to run the music tests");
    assert!(status.success(), "ffmpeg failed for {name}");
    std::fs::rename(&tmp, &out).unwrap();
    out
}

/// An aevalsrc expression for a drum loop: a kick drum (a sine falling from
/// about 145 to 55 Hz and dying away) on every beat, louder on the first beat
/// of each bar, optionally a hi-hat of noise on every off-beat and a noise bed.
fn drum_expr(bpm: f64, hats: bool, noise: f64) -> String {
    let p = 60.0 / bpm;
    let ph = format!("mod(t,{p})");
    let mut e = format!(
        "if(eq(mod(floor(t/{p}),4),0),0.9,0.45)*sin(2*PI*(55*{ph}+3.6*(1-exp(-25*{ph}))))*exp(-14*{ph})"
    );
    if hats {
        e += &format!("+0.12*(2*random(0)-1)*exp(-70*mod(t+{h},{p}))*gte(t,{h})", h = p / 2.0);
    }
    if noise > 0.0 {
        e += &format!("+{noise}*(2*random(1)-1)");
    }
    e
}

fn song(name: &str, bpm: f64, seconds: f64, hats: bool, noise: f64, codec: &[&str]) -> PathBuf {
    let src = format!("aevalsrc='{}':s=44100:d={seconds}", drum_expr(bpm, hats, noise));
    let mut args = vec!["-f", "lavfi", "-i", &src];
    args.extend_from_slice(codec);
    make(name, &args)
}

fn truth(bpm: f64, seconds: f64) -> Vec<f64> {
    let p = 60.0 / bpm;
    (0..).map(|i| i as f64 * p).take_while(|t| *t < seconds - 0.05).collect()
}

/// Share of true beats with a detected beat within `tol` seconds, and the
/// median absolute timing error in milliseconds.
fn accuracy(truth: &[f64], found: &[f64], tol: f64) -> (f64, f64) {
    let mut errs: Vec<f64> = truth
        .iter()
        .map(|t| found.iter().map(|f| (f - t).abs()).fold(f64::MAX, f64::min))
        .collect();
    let hits = errs.iter().filter(|e| **e <= tol).count() as f64 / truth.len() as f64;
    errs.sort_by(f64::total_cmp);
    (hits, errs[errs.len() / 2] * 1000.0)
}

fn check_song(path: &Path, bpm: f64, seconds: f64) -> BeatGrid {
    let cancel = AtomicBool::new(false);
    let (info, grid) = beat_grid(path, &cancel).expect("beat grid");
    assert!((info.duration - seconds).abs() < 0.1, "duration {}", info.duration);
    let truth = truth(bpm, seconds);
    let (hits, median_ms) = accuracy(&truth, &grid.beats, 0.035);
    let tempo_err = (grid.tempo - bpm).abs() / bpm * 100.0;
    eprintln!(
        "{}: tempo {:.2} ({tempo_err:.2}% off), {:.1}% of beats within 35 ms, median error {median_ms:.1} ms, confidence {:.2}",
        info.name,
        grid.tempo,
        hits * 100.0,
        grid.confidence
    );
    assert!(tempo_err < 1.5, "{} read as {} BPM", info.name, grid.tempo);
    assert!(hits >= 0.9, "{}: {:.0}% of beats within 35 ms", info.name, hits * 100.0);
    assert!(grid.confidence > 0.4, "{} confidence {}", info.name, grid.confidence);

    // The grid covers the whole song, in order, one strength per beat.
    let p = 60.0 / bpm;
    assert!(grid.beats[0] >= 0.0 && grid.beats[0] < p, "first beat {}", grid.beats[0]);
    let last = *grid.beats.last().unwrap();
    assert!(last > seconds - p - 0.05 && last <= info.duration, "last beat {last} of {}", info.duration);
    assert!(grid.beats.windows(2).all(|w| w[1] > w[0]));
    assert_eq!(grid.strength.len(), grid.beats.len());
    assert!(grid.strength.iter().all(|s| (0.0..=1.0).contains(s)));

    // Bars start on the accented beats, every fourth.
    assert!(!grid.downbeats.is_empty());
    assert!(grid.downbeats.windows(2).all(|w| w[1] - w[0] == 4));
    for &d in &grid.downbeats {
        let k = (grid.beats[d] / p).round() as i64;
        assert_eq!(k % 4, 0, "{}: bar starts at {:.3}", info.name, grid.beats[d]);
    }
    grid
}

#[test]
fn beats_of_a_90_bpm_wav_with_hats_and_noise() {
    let path = song("beats_90.wav", 90.0, 24.0, true, 0.03, &["-c:a", "pcm_s16le"]);
    check_song(&path, 90.0, 24.0);
}

#[test]
fn beats_of_a_120_bpm_aac_song() {
    let path = song("beats_120.m4a", 120.0, 20.0, false, 0.0, &["-c:a", "aac", "-b:a", "160k"]);
    let grid = check_song(&path, 120.0, 20.0);
    // Every bar, never past 12.5 seconds: 0, 2, 4 ... 12.
    let cuts = cut_points(&grid, 4, 12.5);
    assert_eq!(cuts.len(), 7, "{cuts:?}");
    assert!(cuts[0] < 0.035);
    assert!(cuts.windows(2).all(|w| (w[1] - w[0] - 2.0).abs() < 0.05), "{cuts:?}");
    assert!(*cuts.last().unwrap() <= 12.5);
    assert!(cut_points(&grid, 4, 11.9).len() == 6);
    // Every two bars from the first bar, through the whole song.
    let cuts = cut_points(&grid, 8, 100.0);
    assert!(cuts.windows(2).all(|w| (w[1] - w[0] - 4.0).abs() < 0.05), "{cuts:?}");
    assert!(*cuts.last().unwrap() > 15.0);
}

#[test]
fn beats_of_a_128_bpm_flac_with_hats_and_noise() {
    let path = song("beats_128.flac", 128.0, 30.0, true, 0.05, &["-c:a", "flac"]);
    check_song(&path, 128.0, 30.0);
}

#[test]
fn beats_of_a_150_bpm_stereo_aac_with_hats() {
    let expr = drum_expr(150.0, true, 0.0);
    let src = format!("aevalsrc='{expr}|{expr}':s=48000:d=25");
    let path = make("beats_150.m4a", &["-f", "lavfi", "-i", &src, "-c:a", "aac", "-b:a", "192k"]);
    let info = probe_music(&path).unwrap();
    assert_eq!((info.channels, info.sample_rate, info.codec.as_str()), (2, 48000, "aac"));
    check_song(&path, 150.0, 25.0);
}

#[test]
fn beats_quiet_intro_is_gridded_from_the_beat() {
    // Four seconds of silence, then 120 BPM: the grid still starts near zero
    // and lines up with the beat that follows.
    let src = format!("aevalsrc='{}*gte(t,4)':s=44100:d=20", drum_expr(120.0, true, 0.0));
    let path = make("beats_intro.wav", &["-f", "lavfi", "-i", &src, "-c:a", "pcm_s16le"]);
    let cancel = AtomicBool::new(false);
    let (_, grid) = beat_grid(&path, &cancel).unwrap();
    assert!(grid.beats[0] < 0.5 + 0.035, "first beat {}", grid.beats[0]);
    let (hits, _) = accuracy(&truth(120.0, 20.0), &grid.beats, 0.035);
    assert!(hits >= 0.9, "{:.0}%", hits * 100.0);
}

#[test]
fn beats_silent_music_gives_an_empty_grid() {
    let path = make("beats_silence.wav", &["-f", "lavfi", "-i", "anullsrc=r=44100:cl=mono", "-t", "8", "-c:a", "pcm_s16le"]);
    let cancel = AtomicBool::new(false);
    let (info, grid) = beat_grid(&path, &cancel).unwrap();
    assert!((info.duration - 8.0).abs() < 0.05);
    assert_eq!(grid, BeatGrid::default());
    assert_eq!(grid.confidence, 0.0);
}

#[test]
fn beats_video_with_sound_gives_its_audio_details() {
    let path = make(
        "beats_video_sound.mp4",
        &[
            "-f", "lavfi", "-i", "testsrc2=s=320x240:r=25:d=3", "-f", "lavfi", "-i",
            "sine=frequency=220:sample_rate=32000:d=3", "-c:v", "mpeg4", "-q:v", "5", "-c:a", "aac", "-ac", "1",
        ],
    );
    let info = probe_music(&path).unwrap();
    assert_eq!(info.name, "beats_video_sound.mp4");
    assert_eq!((info.sample_rate, info.channels, info.codec.as_str()), (32000, 1, "aac"));
    assert!((info.duration - 3.0).abs() < 0.1, "duration {}", info.duration);
    let samples = decode_mono(&path, ANALYSIS_RATE, &AtomicBool::new(false)).unwrap();
    assert!((samples.len() as f64 / ANALYSIS_RATE as f64 - 3.0).abs() < 0.1);
}

#[test]
fn beats_files_without_sound_get_plain_errors() {
    let silent_video = make(
        "beats_video_only.mp4",
        &["-f", "lavfi", "-i", "testsrc2=s=320x240:r=25:d=1", "-c:v", "mpeg4", "-q:v", "5"],
    );
    let err = probe_music(&silent_video).unwrap_err();
    assert!(err.contains("no sound"), "{err}");
    let err = decode_mono(&silent_video, ANALYSIS_RATE, &AtomicBool::new(false)).unwrap_err();
    assert!(err.contains("no sound"), "{err}");

    let text = common::fixtures_dir().join("beats_not_music.txt");
    std::fs::write(&text, "these are words, not a song").unwrap();
    let err = probe_music(&text).unwrap_err();
    assert!(err.starts_with("This file could not be read as music"), "{err}");

    let err = probe_music(Path::new("/no/such/folder/song.mp3")).unwrap_err();
    assert!(err.contains("moved or deleted"), "{err}");
}

#[test]
fn beats_decoding_stops_when_cancelled() {
    let path = song("beats_120.m4a", 120.0, 20.0, false, 0.0, &["-c:a", "aac", "-b:a", "160k"]);
    let cancel = AtomicBool::new(true);
    assert_eq!(decode_mono(&path, ANALYSIS_RATE, &cancel).unwrap_err(), CANCELLED);
    assert_eq!(beat_grid(&path, &cancel).unwrap_err(), CANCELLED);
}

#[test]
fn beats_three_minute_song_runtime() {
    let path = song("beats_3min.m4a", 124.0, 180.0, true, 0.02, &["-c:a", "aac", "-b:a", "192k"]);
    let cancel = AtomicBool::new(false);
    let start = std::time::Instant::now();
    let info = probe_music(&path).unwrap();
    let samples = decode_mono(&path, ANALYSIS_RATE, &cancel).unwrap();
    let decoded = start.elapsed();
    let grid = selects_engine::beats::analyze_music(&samples, ANALYSIS_RATE);
    let total = start.elapsed();
    eprintln!(
        "3 minute song: probe and decode {:.2} s, analysis {:.3} s, total {:.2} s, {} beats at {:.2} BPM",
        decoded.as_secs_f64(),
        (total - decoded).as_secs_f64(),
        total.as_secs_f64(),
        grid.beats.len(),
        grid.tempo
    );
    assert!((info.duration - 180.0).abs() < 0.1);
    assert!((grid.tempo - 124.0).abs() / 124.0 < 0.015);
    let (hits, _) = accuracy(&truth(124.0, 180.0), &grid.beats, 0.035);
    assert!(hits >= 0.9);
    assert!(total.as_secs_f64() < 30.0);
}
