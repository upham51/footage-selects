//! Beats and bars of a music track, for cutting a reel to a song.
//!
//! Everything here is classical signal processing on the decoded sound; there
//! is no machine learning and no network access. The steps follow Dan Ellis,
//! "Beat tracking by dynamic programming" (2007), with small additions for
//! sub-frame timing, intros and outros, and bar starts.
//!
//! 1. Onset strength. The song is decoded to mono at 22050 Hz and cut into
//!    1024 sample Hann windows every 512 samples (about 43 frames a second).
//!    Each window's spectrum is pooled into 40 bands spaced on the mel scale,
//!    and each band's magnitude is log-compressed as log(1 + 100 m / loudest),
//!    which behaves like decibels for loud sounds but keeps quiet ones quiet.
//!    The rises from one frame to the next are summed over bands (half-wave
//!    rectified spectral flux), once over all bands and once over the bands
//!    below 220 Hz. Each has a moving average over about half a second
//!    subtracted, negatives dropped, and is scaled to unit root mean square;
//!    the onset envelope is their average, so busy cymbals and noise do not
//!    outvote the kick drum that marks the beat in most music. A third
//!    envelope keeps the rises of plain magnitude between 30 and 160 Hz, for
//!    finding bar starts.
//! 2. Tempo. The autocorrelation of the onset envelope is taken over the lags
//!    of 60 to 200 beats per minute and weighted by a log-Gaussian preference
//!    centred on 120 BPM with a spread of one octave, after a light 1-2-1 blur
//!    so a period between whole frames keeps a single peak. The best peak is refined
//!    with parabolic interpolation and with the matching peaks at two, three
//!    and four times the lag. When the winner lies outside 80 to 160 BPM and
//!    the tempo twice or half as fast lies inside it with at least 70% of the
//!    winner's correlation, the one inside is used, so a 180 BPM song reads as
//!    90 and a 70 BPM ballad with busy hi-hats reads as 140 only when the
//!    evidence for 140 is nearly as good.
//! 3. Beats. Ellis' dynamic program picks the beat frames that maximise onset
//!    strength plus a penalty of `TIGHTNESS` times the squared log ratio of
//!    each gap to the beat period, then backtracks from the last strong beat.
//!    Each beat is moved to the onset peak within two frames and placed between
//!    frames by parabolic interpolation. The soft penalty lets the beats follow
//!    a performance that speeds up and slows down (rubato) by a few percent
//!    from beat to beat. Beats at the start and end whose onsets are weaker
//!    than 30% of the median beat (quiet intros, fades, silence) are replaced
//!    by a steady grid at the local beat period of the nearest strong part,
//!    extended to cover the whole song. Runs of weak beats inside the song
//!    (breakdowns) are respaced evenly between the strong beats either side.
//!    The reported tempo comes from a least squares line through the strong
//!    beats against beat numbers counted in whole periods (`fit_period`), so
//!    a beat the tracker added or lost in a fill or a breakdown does not
//!    move it; the edge grids use the same fitted period.
//! 4. Bars. Four beats a bar is assumed. Of the four ways to group beats into
//!    bars, the one whose first beats carry the most low-frequency rise (kick
//!    drums) plus half the overall onset strength wins.
//! 5. Confidence is the geometric mean of how strongly the onset envelope
//!    repeats at the beat period (its normalised autocorrelation there) and how
//!    much of the onset energy falls within two frames of a beat, above what a
//!    random grid would catch.
//!
//! Silence (an overall level below -80 dBFS) gives an empty
//! `BeatGrid` with zero tempo and zero confidence. Sound with no steady pulse
//! still gets a grid, at whatever tempo fits best, with a low confidence, so
//! the caller decides whether to trust it. Clips too short to hold two beat
//! periods are gridded at 120 BPM with zero confidence.

use crate::ffmpeg;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// The sample rate songs are decoded at for beat analysis.
pub const ANALYSIS_RATE: u32 = 22050;
/// Beats in a bar. Only four is supported.
pub const BEATS_PER_BAR: usize = 4;
/// The error `decode_mono` and `beat_grid` return when `cancel` is set.
pub const CANCELLED: &str = "The music analysis was stopped before it finished.";

const MIN_BPM: f64 = 60.0;
const MAX_BPM: f64 = 200.0;
const PRIOR_BPM: f64 = 120.0;
const PRIOR_OCTAVES: f64 = 1.0;
/// Weight of the beat gap penalty in the dynamic program (librosa's default).
const TIGHTNESS: f64 = 100.0;
const N_BANDS: usize = 40;
/// Log compression of band magnitudes, log(1 + LOG_GAMMA * m / loudest).
const LOG_GAMMA: f32 = 100.0;
/// Below this root mean square level (-80 dBFS) the song counts as silent.
const SILENCE_RMS: f64 = 1e-4;
/// Beats weaker than this share of the median beat count as weak at the edges.
const WEAK_EDGE: f32 = 0.3;
/// Beats at each end of the strong part whose period continues the grid outward.
const EDGE_BEATS: usize = 16;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MusicInfo {
    pub path: String,
    /// File name without folder.
    pub name: String,
    /// Seconds.
    pub duration: f64,
    /// The file's own sample rate.
    pub sample_rate: u32,
    pub channels: u32,
    pub codec: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct BeatGrid {
    /// Beats per minute.
    pub tempo: f64,
    /// Beat times in seconds, ascending, covering the whole song.
    pub beats: Vec<f64>,
    /// Indices into `beats` where a bar starts (4 beats a bar assumed).
    pub downbeats: Vec<usize>,
    /// Onset strength at each beat, 0 to 1 (1 = strongest in the song).
    pub strength: Vec<f32>,
    /// 0 to 1: how clearly the music has a steady beat.
    pub confidence: f32,
}

// Reading the file

/// ffprobe the file's first audio stream. Errors are plain sentences for a
/// non-technical reader, for example a file with no sound or one that cannot
/// be read.
pub fn probe_music(path: &Path) -> Result<MusicInfo, String> {
    let out = ffmpeg::command("ffprobe")
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .args(["-select_streams", "a:0"])
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| tool_missing(&e.to_string()))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err.lines().last().unwrap_or("").trim().to_string();
        return Err(music_error(&line));
    }
    let json: Value = serde_json::from_slice(&out.stdout).map_err(|_| music_error(""))?;
    parse_music_probe(&json, path)
}

/// Turn ffprobe's JSON for the first audio stream into `MusicInfo`.
pub fn parse_music_probe(json: &Value, path: &Path) -> Result<MusicInfo, String> {
    let stream = json
        .get("streams")
        .and_then(|s| s.as_array())
        .and_then(|a| a.iter().find(|s| s.get("codec_type").and_then(|c| c.as_str()) == Some("audio")))
        .ok_or_else(|| "This file has no sound, so it cannot be used as music for a reel.".to_string())?;
    let duration = stream
        .get("duration")
        .and_then(num)
        .filter(|d| *d > 0.0)
        .or_else(|| json.get("format").and_then(|f| f.get("duration")).and_then(num))
        .filter(|d| d.is_finite() && *d > 0.0)
        .ok_or_else(|| "The app could not tell how long this song is, so it cannot be used.".to_string())?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    Ok(MusicInfo {
        path: path.to_string_lossy().into_owned(),
        name,
        duration,
        sample_rate: stream.get("sample_rate").and_then(num).unwrap_or(0.0) as u32,
        channels: stream.get("channels").and_then(num).unwrap_or(0.0) as u32,
        codec: stream.get("codec_name").and_then(|c| c.as_str()).unwrap_or("").to_string(),
    })
}

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn tool_missing(e: &str) -> String {
    format!("The audio reader that ships with the app could not start ({e}). Reinstalling the app usually fixes this.")
}

/// A plain sentence for ffmpeg or ffprobe error text about a music file.
fn music_error(line: &str) -> String {
    let l = line.to_ascii_lowercase();
    if l.contains("no such file") {
        "The music file was moved or deleted. Put it back or pick it again.".into()
    } else if l.contains("permission denied") {
        "The app is not allowed to open this music file. Check the file's permissions.".into()
    } else if l.contains("matches no streams") || l.contains("does not contain any stream") {
        "This file has no sound, so it cannot be used as music for a reel.".into()
    } else {
        "This file could not be read as music. It may be damaged, or it may not be a sound or video file.".into()
    }
}

/// Decode to mono f32 at `rate` Hz with ffmpeg, checking `cancel` while
/// reading. Returns `Err(CANCELLED)` when cancelled.
pub fn decode_mono(path: &Path, rate: u32, cancel: &AtomicBool) -> Result<Vec<f32>, String> {
    let rate_arg = rate.max(1).to_string();
    let mut child = ffmpeg::command("ffmpeg")
        .args(["-hide_banner", "-nostdin", "-v", "error", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-vn", "-sn", "-dn", "-ac", "1", "-ar", &rate_arg])
        .args(["-f", "f32le", "pipe:1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| tool_missing(&e.to_string()))?;

    let lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let stderr = child.stderr.take().expect("stderr piped");
    let sink = lines.clone();
    let err_thread = std::thread::spawn(move || {
        use std::io::BufRead;
        for line in std::io::BufReader::new(stderr).split(b'\n').map_while(Result::ok) {
            let mut l = sink.lock().unwrap();
            l.push(String::from_utf8_lossy(&line).trim().to_string());
            if l.len() > 20 {
                l.remove(0);
            }
        }
    });

    let mut stdout = child.stdout.take().expect("stdout piped");
    let mut samples: Vec<f32> = Vec::with_capacity(rate as usize * 240);
    let mut buf = vec![0u8; 1 << 16];
    let mut carry: Vec<u8> = Vec::with_capacity(4);
    let mut cancelled = false;
    loop {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        let n = match stdout.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let mut bytes = &buf[..n];
        // A read can end partway through a sample; finish it from the next one.
        if !carry.is_empty() {
            let need = 4 - carry.len();
            let take = need.min(bytes.len());
            carry.extend_from_slice(&bytes[..take]);
            bytes = &bytes[take..];
            if carry.len() == 4 {
                samples.push(f32::from_le_bytes([carry[0], carry[1], carry[2], carry[3]]));
                carry.clear();
            }
        }
        let whole = bytes.len() / 4 * 4;
        samples.extend(bytes[..whole].chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])));
        carry.extend_from_slice(&bytes[whole..]);
    }
    if cancelled {
        let _ = child.kill();
    }
    let status = child.wait();
    let _ = err_thread.join();
    if cancelled {
        return Err(CANCELLED.into());
    }
    let ok = status.map(|s| s.success()).unwrap_or(false);
    if !ok && samples.is_empty() {
        // ffmpeg follows the line that matters with advice and summaries, so
        // the whole error text is searched.
        let text = lines.lock().unwrap().join("\n");
        return Err(music_error(&text));
    }
    // Damaged files that stop partway still give usable beats for what decoded.
    for s in samples.iter_mut() {
        if !s.is_finite() {
            *s = 0.0;
        }
    }
    Ok(samples)
}

/// probe + decode at `ANALYSIS_RATE` + analyze.
pub fn beat_grid(path: &Path, cancel: &AtomicBool) -> Result<(MusicInfo, BeatGrid), String> {
    let info = probe_music(path)?;
    let mut samples = decode_mono(path, ANALYSIS_RATE, cancel)?;
    // Some decoders pad the end a little; the grid stops where the file says.
    let len = (info.duration * ANALYSIS_RATE as f64).ceil() as usize;
    samples.truncate(len.max(1));
    Ok((info, analyze_music(&samples, ANALYSIS_RATE)))
}

// Cutting

/// Beat indices to cut on: every `every` beats starting from the first
/// downbeat, never past `max_seconds`. Returns times in seconds (the first cut
/// is the start of the first bar at or after 0). An `every` of 0 counts as 1.
/// A grid without downbeats starts from its first beat.
pub fn cut_points(grid: &BeatGrid, every: usize, max_seconds: f64) -> Vec<f64> {
    let every = every.max(1);
    let start = grid
        .downbeats
        .iter()
        .copied()
        .find(|&i| grid.beats.get(i).is_some_and(|t| *t >= 0.0))
        .unwrap_or(0);
    grid.beats
        .iter()
        .skip(start)
        .step_by(every)
        .copied()
        .take_while(|t| *t <= max_seconds)
        .filter(|t| *t >= 0.0)
        .collect()
}

// Analysis

/// Beats and bars of mono samples at `rate` Hz.
pub fn analyze_music(samples: &[f32], rate: u32) -> BeatGrid {
    if rate == 0 || samples.is_empty() {
        return BeatGrid::default();
    }
    let duration = samples.len() as f64 / rate as f64;
    let env = match Envelopes::compute(samples, rate) {
        Some(e) => e,
        None => return BeatGrid::default(),
    };
    let fr = env.frame_rate;

    // Tempo from the autocorrelation of the onset envelope.
    let tempo = estimate_tempo(&env.onset, fr);
    let (period, periodicity) = match tempo {
        Some(t) => (t.period, t.periodicity),
        None => (60.0 * fr / PRIOR_BPM, 0.0),
    };

    // Beats by dynamic programming, refined between frames.
    let frames = track_beats(&env.onset, period);
    let mut beat_frames: Vec<f64> = frames.iter().map(|&f| refine_peak(&env.onset, f, 2)).collect();
    beat_frames.dedup_by(|b, a| (*b - *a) < 0.5);

    let to_time = |f: f64| f / fr;
    let mut times: Vec<f64> = beat_frames.iter().map(|&f| to_time(f)).collect();
    let onset_at = |t: f64, reach: isize, e: &[f32]| -> f32 {
        let c = (t * fr).round() as isize;
        (c - reach..=c + reach)
            .filter(|&i| i >= 0 && (i as usize) < e.len())
            .map(|i| e[i as usize])
            .fold(0.0f32, f32::max)
    };

    // Weak beats at the edges give way to a steady grid from the strong part.
    let period_s = period / fr;
    let strengths: Vec<f32> = times.iter().map(|&t| onset_at(t, 1, &env.onset)).collect();
    let med = median(&strengths);
    let strong = |s: f32| med > 0.0 && s >= WEAK_EDGE * med;
    let first = strengths.iter().position(|&s| strong(s));
    let last = strengths.iter().rposition(|&s| strong(s));
    let (mut core, fitted) = match (first, last) {
        (Some(a), Some(b)) => {
            let core: Vec<f64> = times[a..=b].to_vec();
            let fitted = fit_period(&core, period_s);
            // Quiet stretches inside the song (breakdowns) are gridded between
            // the strong beats either side, so cuts there stay on the beat.
            let core = regularise_weak_runs(&core, &strengths[a..=b], &strong, fitted.unwrap_or(period_s));
            (core, fitted)
        }
        _ => {
            // No strong beats at all: one beat at the best guess, gridded out.
            let t0 = times.first().copied().unwrap_or(0.0);
            (vec![t0], None)
        }
    };
    // The beat period of the whole song, from a line through the strong beats
    // that is not thrown by an extra or missing beat; the autocorrelation's
    // when there are too few.
    let song_period = fitted.unwrap_or(period_s);
    // The grid at either end continues the local period of the beats next to it.
    let edge = |run: &[f64]| fit_period(run, song_period).filter(|p| (p / song_period - 1.0).abs() < 0.08).unwrap_or(song_period);
    let p_start = edge(&core[..core.len().min(EDGE_BEATS)]);
    let p_end = edge(&core[core.len().saturating_sub(EDGE_BEATS)..]);
    core.retain(|t| *t >= 0.0 && *t < duration);
    if core.is_empty() {
        core.push(0.0);
    }
    let mut lead = Vec::new();
    // A beat a hair before zero (rounding) is kept at zero.
    let mut t = core[0] - p_start;
    while t >= -0.05 * p_start && p_start > 0.0 {
        lead.push(t.max(0.0));
        t -= p_start;
    }
    lead.reverse();
    let core_len = core.len();
    let core_first = lead.len();
    times = lead;
    times.extend(core);
    let mut t = times[times.len() - 1] + p_end;
    while t < duration && p_end > 0.0 {
        times.push(t);
        t += p_end;
    }
    let core_times = &times[core_first..core_first + core_len];
    let bpm = 60.0 / song_period;

    // Strength at each beat, 1 for the strongest.
    let raw: Vec<f32> = times.iter().map(|&t| onset_at(t, 1, &env.onset)).collect();
    let top = raw.iter().copied().fold(0.0f32, f32::max);
    let strength: Vec<f32> = raw.iter().map(|&s| if top > 0.0 { s / top } else { 0.0 }).collect();

    // Bars: the grouping of four whose first beats carry the most kick.
    let low: Vec<f32> = times.iter().map(|&t| onset_at(t, 2, &env.low)).collect();
    let range = core_first..core_first + core_len;
    let mean_of = |v: &[f32]| -> f32 {
        let s: f32 = v[range.clone()].iter().sum();
        s / core_len.max(1) as f32
    };
    let (low_mean, on_mean) = (mean_of(&low), mean_of(&raw));
    let mut best_phase = 0;
    let mut best_score = f32::MIN;
    for phase in 0..BEATS_PER_BAR {
        let (mut sum, mut count) = (0.0f32, 0usize);
        for i in range.clone().filter(|i| i % BEATS_PER_BAR == phase) {
            let l = if low_mean > 0.0 { low[i] / low_mean } else { 0.0 };
            let o = if on_mean > 0.0 { raw[i] / on_mean } else { 0.0 };
            sum += l + 0.5 * o;
            count += 1;
        }
        let score = if count > 0 { sum / count as f32 } else { f32::MIN };
        if score > best_score {
            best_score = score;
            best_phase = phase;
        }
    }
    let downbeats: Vec<usize> = (best_phase..times.len()).step_by(BEATS_PER_BAR).collect();

    // Confidence: periodicity times alignment of onsets with beats.
    let alignment = beat_alignment(&env.onset, core_times, fr, period);
    let confidence = if tempo.is_some() { (periodicity.max(0.0) * alignment).sqrt() as f32 } else { 0.0 };

    BeatGrid {
        tempo: bpm,
        beats: times,
        downbeats,
        strength,
        confidence: confidence.clamp(0.0, 1.0),
    }
}

/// Onset envelopes at about 43 frames a second.
struct Envelopes {
    /// Frames per second.
    frame_rate: f64,
    /// Band-pooled log spectral flux, local mean removed, unit root mean square.
    onset: Vec<f32>,
    /// Rises of 30 to 160 Hz magnitude (kick drums).
    low: Vec<f32>,
}

impl Envelopes {
    /// None for silence.
    fn compute(samples: &[f32], rate: u32) -> Option<Envelopes> {
        let rms = (samples.iter().map(|&s| (s as f64) * (s as f64)).sum::<f64>() / samples.len() as f64).sqrt();
        if rms.is_nan() || rms < SILENCE_RMS {
            return None;
        }
        // About 1024 samples at 22050 Hz, scaled for other rates.
        let target = (1024.0 * rate as f64 / ANALYSIS_RATE as f64).round().max(64.0) as usize;
        let n_fft = target.next_power_of_two();
        let hop = n_fft / 2;
        let frame_rate = rate as f64 / hop as f64;
        let n_frames = samples.len() / hop + 1;
        let fft = Fft::new(n_fft);
        let window: Vec<f32> = (0..n_fft)
            .map(|i| (0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n_fft as f64).cos()) as f32)
            .collect();

        // Map spectrum bins to mel bands, and mark the kick drum bins.
        let bins = n_fft / 2 + 1;
        let bin_hz = rate as f64 / n_fft as f64;
        let mel = |f: f64| 2595.0 * (1.0 + f / 700.0).log10();
        let (lo_hz, hi_hz) = (30.0, (rate as f64 * 0.5 * 0.95).min(11000.0));
        let (mlo, mhi) = (mel(lo_hz), mel(hi_hz));
        let band_of: Vec<Option<usize>> = (0..bins)
            .map(|k| {
                let f = k as f64 * bin_hz;
                if f < lo_hz || f > hi_hz {
                    None
                } else {
                    Some((((mel(f) - mlo) / (mhi - mlo) * N_BANDS as f64) as usize).min(N_BANDS - 1))
                }
            })
            .collect();
        let used: Vec<bool> = (0..N_BANDS).map(|b| band_of.contains(&Some(b))).collect();
        let n_used = used.iter().filter(|u| **u).count().max(1);
        let kick: Vec<bool> = (0..bins).map(|k| (30.0..=160.0).contains(&(k as f64 * bin_hz))).collect();

        let mut db = vec![0.0f32; n_frames * N_BANDS];
        let mut low_mag = vec![0.0f32; n_frames];
        let mut re = vec![0.0f32; n_fft];
        let mut im = vec![0.0f32; n_fft];
        let half = (n_fft / 2) as isize;
        let mut top = 0.0f32;
        for t in 0..n_frames {
            let start = (t * hop) as isize - half;
            for i in 0..n_fft {
                let j = start + i as isize;
                re[i] = if j >= 0 && (j as usize) < samples.len() { samples[j as usize] * window[i] } else { 0.0 };
                im[i] = 0.0;
            }
            fft.run(&mut re, &mut im);
            let mut band = [0.0f64; N_BANDS];
            let mut lowp = 0.0f64;
            for k in 0..bins {
                let p = (re[k] as f64).powi(2) + (im[k] as f64).powi(2);
                if let Some(b) = band_of[k] {
                    band[b] += p;
                }
                if kick[k] {
                    lowp += p;
                }
            }
            low_mag[t] = lowp.sqrt() as f32;
            for b in 0..N_BANDS {
                let m = band[b].sqrt() as f32;
                db[t * N_BANDS + b] = m;
                if used[b] {
                    top = top.max(m);
                }
            }
        }
        let k = LOG_GAMMA / top.max(1e-12);
        for d in db.iter_mut() {
            *d = (1.0 + k * *d).ln();
        }

        // Bands that end below 220 Hz carry kick drums and bass notes.
        let inv_mel = |m: f64| 700.0 * (10f64.powf(m / 2595.0) - 1.0);
        let bass: Vec<bool> = (0..N_BANDS)
            .map(|b| used[b] && inv_mel(mlo + (b + 1) as f64 / N_BANDS as f64 * (mhi - mlo)) <= 220.0)
            .collect();
        let n_bass = bass.iter().filter(|u| **u).count();
        let mut flux = vec![0.0f32; n_frames];
        let mut bass_flux = vec![0.0f32; n_frames];
        let mut low = vec![0.0f32; n_frames];
        // The first frame rises from the silence before the song.
        for t in 0..n_frames {
            let (mut s, mut sb) = (0.0f32, 0.0f32);
            for b in 0..N_BANDS {
                if used[b] {
                    let before = if t > 0 { db[(t - 1) * N_BANDS + b] } else { 0.0 };
                    let rise = (db[t * N_BANDS + b] - before).max(0.0);
                    s += rise;
                    if bass[b] {
                        sb += rise;
                    }
                }
            }
            flux[t] = s / n_used as f32;
            bass_flux[t] = sb / n_bass.max(1) as f32;
            low[t] = (low_mag[t] - if t > 0 { low_mag[t - 1] } else { 0.0 }).max(0.0);
        }

        // Equal parts of the whole spectrum and the bass, so that busy
        // cymbals do not outvote the kick drum that usually marks the beat.
        let full = local_rises(&flux, frame_rate)?;
        let mut onset = match local_rises(&bass_flux, frame_rate) {
            Some(b) => full.iter().zip(&b).map(|(f, b)| 0.5 * (f + b)).collect(),
            None => full,
        };
        unit_rms(&mut onset)?;
        Some(Envelopes { frame_rate, onset, low })
    }
}

/// Remove the local mean over about half a second, keep the rises, and scale
/// to unit root mean square. None when nothing rises.
fn local_rises(flux: &[f32], frame_rate: f64) -> Option<Vec<f32>> {
    let n = flux.len();
    let r = ((0.5 * frame_rate).round() as usize) / 2;
    let mut prefix = vec![0.0f64; n + 1];
    for t in 0..n {
        prefix[t + 1] = prefix[t] + flux[t] as f64;
    }
    let mut out: Vec<f32> = (0..n)
        .map(|t| {
            let a = t.saturating_sub(r);
            let b = (t + r + 1).min(n);
            let mean = (prefix[b] - prefix[a]) / (b - a) as f64;
            (flux[t] as f64 - mean).max(0.0) as f32
        })
        .collect();
    unit_rms(&mut out)?;
    Some(out)
}

fn unit_rms(v: &mut [f32]) -> Option<()> {
    let scale = (v.iter().map(|&o| (o as f64).powi(2)).sum::<f64>() / v.len().max(1) as f64).sqrt();
    if scale.is_nan() || scale <= 1e-9 {
        return None;
    }
    for o in v.iter_mut() {
        *o = (*o as f64 / scale) as f32;
    }
    Some(())
}

#[derive(Clone, Copy, Debug)]
struct Tempo {
    /// Beat period in envelope frames.
    period: f64,
    /// Normalised autocorrelation at that period, -1 to 1.
    periodicity: f64,
}

/// The log-Gaussian tempo preference of Ellis (2007).
fn tempo_prior(bpm: f64) -> f64 {
    let x = (bpm / PRIOR_BPM).log2() / PRIOR_OCTAVES;
    (-0.5 * x * x).exp()
}

/// A 1-2-1 blur.
fn smooth(v: &[f32]) -> Vec<f32> {
    let n = v.len();
    (0..n)
        .map(|i| {
            let a = if i > 0 { v[i - 1] } else { v[i] };
            let b = if i + 1 < n { v[i + 1] } else { v[i] };
            0.25 * a + 0.5 * v[i] + 0.25 * b
        })
        .collect()
}

/// Normalised autocorrelation of the mean-removed envelope for lags 0 to `max_lag`.
fn autocorrelation(onset: &[f32], max_lag: usize) -> Vec<f64> {
    let n = onset.len();
    let mean = onset.iter().map(|&v| v as f64).sum::<f64>() / n as f64;
    let x: Vec<f64> = onset.iter().map(|&v| v as f64 - mean).collect();
    let mut ac = vec![0.0; max_lag + 1];
    for (l, a) in ac.iter_mut().enumerate() {
        if l >= n {
            break;
        }
        let s: f64 = x[..n - l].iter().zip(&x[l..]).map(|(a, b)| a * b).sum();
        *a = s / (n - l) as f64;
    }
    let zero = ac[0];
    if zero > 0.0 {
        for a in ac.iter_mut() {
            *a /= zero;
        }
    }
    ac
}

/// The vertex of a parabola through three points around `i`, as an offset in -0.5..0.5.
fn parabolic(y0: f64, y1: f64, y2: f64) -> f64 {
    let d = y0 - 2.0 * y1 + y2;
    if d.abs() < 1e-12 {
        0.0
    } else {
        (0.5 * (y0 - y2) / d).clamp(-0.5, 0.5)
    }
}

/// The highest point of `ac` within `reach` of `centre`, between lags.
fn peak_near(ac: &[f64], centre: f64, reach: usize) -> Option<(f64, f64)> {
    let c = centre.round() as isize;
    let lo = (c - reach as isize).max(1) as usize;
    let hi = ((c + reach as isize) as usize).min(ac.len().saturating_sub(2));
    if lo > hi {
        return None;
    }
    let i = (lo..=hi).max_by(|&a, &b| ac[a].total_cmp(&ac[b]))?;
    let off = parabolic(ac[i - 1], ac[i], ac[i + 1]);
    Some((i as f64 + off, ac[i]))
}

/// Beat period from the envelope, or None when the envelope is too short to
/// hold two periods of the slowest tempo that fits.
fn estimate_tempo(onset: &[f32], fr: f64) -> Option<Tempo> {
    let n = onset.len();
    let lag_min = ((60.0 * fr / MAX_BPM).floor() as usize).max(2);
    let lag_max = ((60.0 * fr / MIN_BPM).ceil() as usize).min(n / 2);
    if lag_max <= lag_min + 1 {
        return None;
    }
    // Onsets are one or two frames wide, so a period that falls between whole
    // frames would split its peak in two; a light blur keeps it whole.
    let blurred = smooth(&smooth(onset));
    let ac = autocorrelation(&blurred, (4 * lag_max + 2).min(n.saturating_sub(1)));
    let mut best: Option<(usize, f64)> = None;
    for l in lag_min..=lag_max {
        if l + 1 >= ac.len() || ac[l] < ac[l - 1] || ac[l] < ac[l + 1] || ac[l] <= 0.0 {
            continue;
        }
        let score = ac[l] * tempo_prior(60.0 * fr / l as f64);
        if best.is_none_or(|(_, s)| score > s) {
            best = Some((l, score));
        }
    }
    let (lag, _) = best?;
    let mut period = lag as f64 + parabolic(ac[lag - 1], ac[lag], ac[lag + 1]);
    let mut strength = ac[lag];

    // Prefer 80 to 160 BPM when the tempo twice or half as fast is nearly as good.
    let bpm = 60.0 * fr / period;
    if !(80.0..=160.0).contains(&bpm) {
        let alt = if bpm > 160.0 { period * 2.0 } else { period / 2.0 };
        if let Some((p, a)) = peak_near(&ac, alt, 1) {
            let alt_bpm = 60.0 * fr / p;
            if (80.0..=160.0).contains(&alt_bpm) && a >= 0.7 * strength {
                period = p;
                strength = a;
            }
        }
    }

    // Peaks at whole multiples of the period pin it down more finely.
    let (mut sum, mut weight) = (period, 1.0);
    for k in 2..=4 {
        let centre = period * k as f64;
        if centre + 3.0 >= ac.len() as f64 || centre > n as f64 / 2.0 {
            break;
        }
        if let Some((p, a)) = peak_near(&ac, centre, 2) {
            if a > 0.3 * strength && (p / k as f64 - period).abs() < 0.1 * period {
                sum += p;
                weight += k as f64;
            }
        }
    }
    period = sum / weight;
    Some(Tempo { period, periodicity: strength })
}

/// Ellis' dynamic programming beat tracker. Returns beat frames in order.
fn track_beats(onset: &[f32], period: f64) -> Vec<usize> {
    let n = onset.len();
    if n == 0 || period <= 1.0 {
        return Vec::new();
    }
    // Lightly smoothed onset strength as the local score.
    let radius = period.round() as isize;
    let kernel: Vec<f64> = (-radius..=radius)
        .map(|x| (-0.5 * (x as f64 * 32.0 / period).powi(2)).exp())
        .collect();
    let local: Vec<f64> = (0..n as isize)
        .map(|t| {
            let mut s = 0.0;
            for (j, k) in kernel.iter().enumerate() {
                let i = t + j as isize - radius;
                if i >= 0 && (i as usize) < n {
                    s += k * onset[i as usize] as f64;
                }
            }
            s
        })
        .collect();
    let d_min = ((period / 2.0).round() as usize).max(1);
    let d_max = ((period * 2.0).round() as usize).max(d_min);
    let penalty: Vec<f64> = (0..=d_max)
        .map(|d| if d == 0 { f64::MIN } else { -TIGHTNESS * (d as f64 / period).ln().powi(2) })
        .collect();
    let threshold = 0.01 * local.iter().copied().fold(0.0, f64::max);
    let mut cum = vec![0.0f64; n];
    let mut back: Vec<Option<usize>> = vec![None; n];
    let mut started = false;
    for i in 0..n {
        let mut best: Option<(usize, f64)> = None;
        for d in d_min..=d_max.min(i) {
            let s = cum[i - d] + penalty[d];
            if best.is_none_or(|(_, b)| s > b) {
                best = Some((i - d, s));
            }
        }
        if !started && local[i] < threshold {
            cum[i] = local[i];
            continue;
        }
        started = true;
        match best {
            Some((j, s)) => {
                cum[i] = local[i] + s;
                back[i] = Some(j);
            }
            None => cum[i] = local[i],
        }
    }
    // Start backtracking from the last strong peak of the cumulative score.
    let peaks: Vec<usize> = (1..n.saturating_sub(1)).filter(|&i| cum[i] > cum[i - 1] && cum[i] >= cum[i + 1]).collect();
    let end = if peaks.is_empty() {
        (0..n).max_by(|&a, &b| cum[a].total_cmp(&cum[b])).unwrap_or(0)
    } else {
        let mut vals: Vec<f64> = peaks.iter().map(|&i| cum[i]).collect();
        vals.sort_by(f64::total_cmp);
        let med = vals[vals.len() / 2];
        *peaks.iter().rev().find(|&&i| cum[i] >= 0.5 * med).unwrap_or(&peaks[peaks.len() - 1])
    };
    let mut beats = vec![end];
    let mut i = end;
    while let Some(j) = back[i] {
        beats.push(j);
        i = j;
    }
    beats.reverse();
    beats
}

/// The onset peak within `reach` frames of `frame`, placed between frames.
fn refine_peak(onset: &[f32], frame: usize, reach: usize) -> f64 {
    let n = onset.len();
    let lo = frame.saturating_sub(reach);
    let hi = (frame + reach).min(n.saturating_sub(1));
    let i = (lo..=hi).max_by(|&a, &b| onset[a].total_cmp(&onset[b])).unwrap_or(frame);
    if onset[i] <= 0.0 {
        return frame as f64;
    }
    if i == 0 || i + 1 >= n {
        return i as f64;
    }
    i as f64 + parabolic(onset[i - 1] as f64, onset[i] as f64, onset[i + 1] as f64)
}

/// The beat period, in seconds, of a run of beat times, from a least squares
/// line through the times against beat numbers. Each beat is numbered by how
/// many whole periods of `guess` (then of the line's own period, three
/// rounds) it lies after the first beat of its run, so a beat the tracker
/// added between two real ones, or one it missed, does not tilt the line the
/// way numbering the beats one by one would. A beat more than a fifth of a
/// period from a whole count is left out; after more than two of those in a
/// row a new run starts with its own offset (the tracker has moved to the
/// off-beat for a while). Beats more than 12% of a period off their run's
/// line are dropped before each fit is final. None for fewer than four beats
/// that can be counted.
fn fit_period(times: &[f64], guess: f64) -> Option<f64> {
    if times.len() < 4 || !(guess > 0.0) {
        return None;
    }
    let mut period = guess;
    for _ in 0..3 {
        let runs = number_beats(times, period);
        let mut p = pooled_slope(&runs)?;
        // Drop the beats far from their run's line, then fit again.
        let trimmed: Vec<Vec<(f64, f64)>> = runs
            .iter()
            .map(|run| {
                let (mi, mt) = means(run);
                run.iter().copied().filter(|&(i, t)| (t - (mt + p * (i - mi))).abs() <= 0.12 * p).collect()
            })
            .collect();
        if let Some(q) = pooled_slope(&trimmed) {
            p = q;
        }
        if !(p > 0.0) || (p / guess - 1.0).abs() > 0.5 {
            return None;
        }
        period = p;
    }
    Some(period)
}

/// Runs of (beat number, time), each counted in whole periods from its own
/// first beat, so small errors in each gap do not add up to a lost beat.
fn number_beats(times: &[f64], period: f64) -> Vec<Vec<(f64, f64)>> {
    let mut runs: Vec<Vec<(f64, f64)>> = vec![vec![(0.0, times[0])]];
    let (mut anchor, mut n, mut skipped) = (times[0], 0.0f64, 0usize);
    for &t in &times[1..] {
        let r = (t - anchor) / period;
        let k = r.round();
        if k > n && (r - k).abs() <= 0.2 {
            n = k;
            skipped = 0;
            runs.last_mut().unwrap().push((n, t));
        } else {
            skipped += 1;
            if skipped > 2 {
                runs.push(vec![(0.0, t)]);
                anchor = t;
                n = 0.0;
                skipped = 0;
            }
        }
    }
    runs
}

fn means(run: &[(f64, f64)]) -> (f64, f64) {
    let n = run.len().max(1) as f64;
    (run.iter().map(|p| p.0).sum::<f64>() / n, run.iter().map(|p| p.1).sum::<f64>() / n)
}

/// The least squares slope shared by several runs, each with its own offset.
/// None with fewer than four beats in runs of three or more.
fn pooled_slope(runs: &[Vec<(f64, f64)>]) -> Option<f64> {
    let (mut sxy, mut sxx, mut count) = (0.0, 0.0, 0usize);
    for run in runs.iter().filter(|r| r.len() >= 3) {
        let (mi, mt) = means(run);
        for &(i, t) in run {
            sxy += (i - mi) * (t - mt);
            sxx += (i - mi) * (i - mi);
        }
        count += run.len();
    }
    (count >= 4 && sxx > 0.0).then(|| sxy / sxx)
}

/// Replace each run of two or more weak beats between strong ones with beats
/// evenly spaced between those strong beats, when a whole number of `period`
/// fits between them to within 8%. Otherwise the run is left as tracked.
fn regularise_weak_runs(times: &[f64], strengths: &[f32], strong: &dyn Fn(f32) -> bool, period: f64) -> Vec<f64> {
    let mut out = Vec::with_capacity(times.len());
    let mut i = 0;
    while i < times.len() {
        out.push(times[i]);
        // The next strong beat after a strong one.
        if strong(strengths[i]) {
            if let Some(off) = strengths[i + 1..].iter().position(|&s| strong(s)) {
                let j = i + 1 + off;
                if j - i > 2 && period > 0.0 {
                    let span = times[j] - times[i];
                    let n = (span / period).round();
                    if n >= 1.0 && (span / n / period - 1.0).abs() <= 0.08 {
                        let step = span / n;
                        for k in 1..n as usize {
                            out.push(times[i] + k as f64 * step);
                        }
                        i = j;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    out
}

fn median(v: &[f32]) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(f32::total_cmp);
    s[s.len() / 2]
}

/// Share of onset energy within two frames of a beat, above what a random
/// grid at the same period would catch, 0 to 1.
fn beat_alignment(onset: &[f32], beats: &[f64], fr: f64, period: f64) -> f64 {
    if beats.len() < 2 {
        return 0.0;
    }
    let reach = 2isize;
    let a = ((beats[0] * fr).floor() as isize - reach).max(0) as usize;
    let b = (((beats[beats.len() - 1] * fr).ceil() as isize + reach).max(0) as usize).min(onset.len());
    if b <= a {
        return 0.0;
    }
    let total: f64 = onset[a..b].iter().map(|&v| v as f64).sum();
    if total <= 0.0 {
        return 0.0;
    }
    let mut near = vec![false; onset.len()];
    for &t in beats {
        let c = (t * fr).round() as isize;
        for i in c - reach..=c + reach {
            if i >= 0 && (i as usize) < near.len() {
                near[i as usize] = true;
            }
        }
    }
    let caught: f64 = (a..b).filter(|&i| near[i]).map(|i| onset[i] as f64).sum();
    let share = caught / total;
    let base = ((2 * reach + 1) as f64 / period).min(0.95);
    ((share - base) / (1.0 - base)).clamp(0.0, 1.0)
}

/// A plain iterative radix-2 FFT of one power-of-two size.
struct Fft {
    n: usize,
    cos: Vec<f32>,
    sin: Vec<f32>,
    rev: Vec<usize>,
}

impl Fft {
    fn new(n: usize) -> Fft {
        assert!(n.is_power_of_two(), "FFT size must be a power of two");
        let bits = n.trailing_zeros();
        let rev = (0..n).map(|i| if bits == 0 { 0 } else { i.reverse_bits() >> (usize::BITS - bits) }).collect();
        let (cos, sin) = (0..n / 2)
            .map(|k| {
                let a = -2.0 * std::f64::consts::PI * k as f64 / n as f64;
                (a.cos() as f32, a.sin() as f32)
            })
            .unzip();
        Fft { n, cos, sin, rev }
    }

    /// In place forward transform, X[k] = sum x[j] e^(-2 pi i j k / n).
    fn run(&self, re: &mut [f32], im: &mut [f32]) {
        let n = self.n;
        for i in 0..n {
            let j = self.rev[i];
            if i < j {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= n {
            let half = len / 2;
            let step = n / len;
            for start in (0..n).step_by(len) {
                for k in 0..half {
                    let (wr, wi) = (self.cos[k * step], self.sin[k * step]);
                    let a = start + k;
                    let b = a + half;
                    let tr = re[b] * wr - im[b] * wi;
                    let ti = re[b] * wi + im[b] * wr;
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                }
            }
            len *= 2;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small deterministic noise source.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
        }
    }

    /// A kick drum at each time in `beats`, accented on every fourth from
    /// `accent_phase`, with optional off-beat hi-hats and noise.
    fn drums(beats: &[f64], seconds: f64, rate: u32, hats: bool, noise: f32, accent_phase: usize) -> Vec<f32> {
        let n = (seconds * rate as f64) as usize;
        let mut out = vec![0.0f32; n];
        let mut rng = Lcg(7);
        for (i, &b) in beats.iter().enumerate() {
            let amp = if i % 4 == accent_phase { 0.9 } else { 0.45 };
            let s0 = (b * rate as f64).round() as usize;
            for j in 0..(0.25 * rate as f64) as usize {
                if s0 + j >= n {
                    break;
                }
                let t = j as f64 / rate as f64;
                let f = 2.0 * std::f64::consts::PI * (55.0 * t + 90.0 / 25.0 * (1.0 - (-25.0 * t).exp()));
                out[s0 + j] += (amp * f.sin() * (-14.0 * t).exp()) as f32;
            }
            if hats && i + 1 < beats.len() {
                let h0 = (((b + beats[i + 1]) / 2.0) * rate as f64).round() as usize;
                for j in 0..(0.05 * rate as f64) as usize {
                    if h0 + j >= n {
                        break;
                    }
                    let t = j as f64 / rate as f64;
                    out[h0 + j] += 0.15 * rng.next() * (-70.0 * t).exp() as f32;
                }
            }
        }
        for s in out.iter_mut() {
            *s += noise * rng.next();
        }
        out
    }

    fn steady(bpm: f64, seconds: f64) -> Vec<f64> {
        let p = 60.0 / bpm;
        (0..).map(|i| i as f64 * p).take_while(|t| *t < seconds).collect()
    }

    /// Share of true beats with a detected beat within `tol` seconds.
    fn hit_rate(truth: &[f64], found: &[f64], tol: f64) -> f64 {
        let hits = truth.iter().filter(|t| found.iter().any(|f| (f - *t).abs() <= tol)).count();
        hits as f64 / truth.len() as f64
    }

    fn mean_error(truth: &[f64], found: &[f64]) -> f64 {
        let errs: Vec<f64> = truth
            .iter()
            .map(|t| found.iter().map(|f| f - t).min_by(|a, b| a.abs().total_cmp(&b.abs())).unwrap())
            .collect();
        errs.iter().sum::<f64>() / errs.len() as f64
    }

    #[test]
    fn fft_matches_a_direct_transform() {
        let n = 16;
        let fft = Fft::new(n);
        let x: Vec<f32> = (0..n).map(|i| ((i * 7 % 5) as f32 - 2.0) * 0.3).collect();
        let (mut re, mut im) = (x.clone(), vec![0.0; n]);
        fft.run(&mut re, &mut im);
        for k in 0..n {
            let (mut r, mut i) = (0.0f64, 0.0f64);
            for (j, v) in x.iter().enumerate() {
                let a = -2.0 * std::f64::consts::PI * (j * k) as f64 / n as f64;
                r += *v as f64 * a.cos();
                i += *v as f64 * a.sin();
            }
            assert!((re[k] as f64 - r).abs() < 1e-4 && (im[k] as f64 - i).abs() < 1e-4, "bin {k}");
        }
    }

    #[test]
    fn beats_found_on_synthetic_drums() {
        let rate = ANALYSIS_RATE;
        for &(bpm, hats, noise) in &[(90.0, true, 0.02), (120.0, false, 0.0), (128.0, true, 0.05), (150.0, true, 0.0)] {
            let secs = 24.0;
            let truth = steady(bpm, secs);
            let samples = drums(&truth, secs, rate, hats, noise, 0);
            let g = analyze_music(&samples, rate);
            let err = (g.tempo - bpm).abs() / bpm;
            let hits = hit_rate(&truth, &g.beats, 0.035);
            eprintln!(
                "{bpm} BPM: tempo {:.2}, hits {:.1}%, mean offset {:.1} ms, confidence {:.2}, first downbeat {:?}",
                g.tempo,
                hits * 100.0,
                mean_error(&truth, &g.beats) * 1000.0,
                g.confidence,
                g.downbeats.first().map(|&i| g.beats[i])
            );
            assert!(err < 0.015, "{bpm} BPM read as {}", g.tempo);
            assert!(hits >= 0.9, "{bpm} BPM: {:.0}% of beats found", hits * 100.0);
            assert!(g.confidence > 0.5, "{bpm} BPM confidence {}", g.confidence);
            // Bars start on the accented beats, at multiples of four beats.
            let p = 60.0 / bpm;
            for &d in &g.downbeats {
                let k = (g.beats[d] / p).round() as i64;
                assert_eq!(k % 4, 0, "{bpm} BPM downbeat at {:.3}", g.beats[d]);
            }
        }
    }

    #[test]
    fn bars_follow_the_accent() {
        let truth = steady(120.0, 20.0);
        let g = analyze_music(&drums(&truth, 20.0, ANALYSIS_RATE, false, 0.0, 2), ANALYSIS_RATE);
        let first = g.beats[g.downbeats[0]];
        assert!((first - 1.0).abs() < 0.035, "first bar starts at {first}");
    }

    #[test]
    fn rubato_is_followed() {
        // Tempo drifting smoothly from 110 to 130 BPM and back over 30 seconds.
        let mut truth = vec![0.3];
        while *truth.last().unwrap() < 30.0 {
            let t = *truth.last().unwrap();
            let bpm = 120.0 - 10.0 * (2.0 * std::f64::consts::PI * t / 30.0).cos();
            truth.push(t + 60.0 / bpm);
        }
        truth.pop();
        let g = analyze_music(&drums(&truth, 30.0, ANALYSIS_RATE, false, 0.01, 0), ANALYSIS_RATE);
        let hits = hit_rate(&truth, &g.beats, 0.035);
        eprintln!("rubato: hits {:.1}%, tempo {:.1}", hits * 100.0, g.tempo);
        assert!(hits >= 0.9, "{:.0}% of drifting beats found", hits * 100.0);
    }

    #[test]
    fn grid_covers_quiet_intro_and_outro() {
        let bpm = 128.0;
        let p = 60.0 / bpm;
        let truth: Vec<f64> = steady(bpm, 26.0).into_iter().filter(|t| *t >= 4.0 && *t < 22.0).collect();
        let samples = drums(&truth, 26.0, ANALYSIS_RATE, true, 0.0, 0);
        let g = analyze_music(&samples, ANALYSIS_RATE);
        assert!(g.beats[0] >= 0.0 && g.beats[0] < p + 0.01, "first beat {}", g.beats[0]);
        assert!(*g.beats.last().unwrap() > 26.0 - p - 0.01, "last beat {}", g.beats.last().unwrap());
        assert!(g.beats.windows(2).all(|w| w[1] > w[0]));
        // The steady grid in the silent parts continues the real beat.
        let all = steady(bpm, 26.0);
        assert!(hit_rate(&all, &g.beats, 0.035) >= 0.9);
        assert_eq!(g.beats.len(), g.strength.len());
        assert!((g.strength.iter().copied().fold(0.0, f32::max) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn silence_gives_an_empty_grid() {
        let g = analyze_music(&vec![0.0; ANALYSIS_RATE as usize * 10], ANALYSIS_RATE);
        assert_eq!(g, BeatGrid::default());
        assert_eq!(analyze_music(&[], ANALYSIS_RATE), BeatGrid::default());
        assert!(cut_points(&g, 4, 30.0).is_empty());
    }

    #[test]
    fn noise_without_a_beat_has_low_confidence() {
        let mut rng = Lcg(3);
        let samples: Vec<f32> = (0..ANALYSIS_RATE * 15).map(|_| 0.2 * rng.next()).collect();
        let g = analyze_music(&samples, ANALYSIS_RATE);
        eprintln!("noise: tempo {:.1}, confidence {:.2}, {} beats", g.tempo, g.confidence, g.beats.len());
        assert!(g.confidence < 0.3, "confidence {}", g.confidence);
        assert!(!g.beats.is_empty());
    }

    #[test]
    fn very_short_clips_do_not_panic() {
        for secs in [0.01, 0.2, 1.0, 2.5] {
            let truth = steady(120.0, secs);
            let g = analyze_music(&drums(&truth, secs, ANALYSIS_RATE, false, 0.0, 0), ANALYSIS_RATE);
            assert!(g.beats.iter().all(|t| *t >= 0.0 && *t < secs.max(0.01)), "{secs} s: {:?}", g.beats);
            assert_eq!(g.beats.len(), g.strength.len());
            assert!(g.downbeats.iter().all(|&i| i < g.beats.len()));
        }
    }

    #[test]
    fn other_sample_rates_work() {
        let truth = steady(120.0, 20.0);
        let g = analyze_music(&drums(&truth, 20.0, 44100, true, 0.0, 0), 44100);
        assert!((g.tempo - 120.0).abs() < 1.8, "tempo {}", g.tempo);
        assert!(hit_rate(&truth, &g.beats, 0.035) >= 0.9);
    }

    #[test]
    fn cut_points_step_through_bars() {
        let beats: Vec<f64> = (0..40).map(|i| 0.25 + i as f64 * 0.5).collect();
        let grid = BeatGrid { tempo: 120.0, downbeats: (1..40).step_by(4).collect(), strength: vec![1.0; 40], beats, confidence: 1.0 };
        let cuts = cut_points(&grid, 4, 10.0);
        assert_eq!(cuts[0], 0.75);
        assert!(cuts.windows(2).all(|w| (w[1] - w[0] - 2.0).abs() < 1e-9));
        assert!(*cuts.last().unwrap() <= 10.0);
        assert_eq!(cuts.len(), 5);
        assert_eq!(cut_points(&grid, 0, 2.0), vec![0.75, 1.25, 1.75]);
        assert!(cut_points(&grid, 2, 0.5).is_empty());
        let no_bars = BeatGrid { downbeats: vec![], ..grid };
        assert_eq!(cut_points(&no_bars, 8, 9.0), vec![0.25, 4.25, 8.25]);
    }

    #[test]
    fn probe_json_is_read() {
        let json: Value = serde_json::from_str(
            r#"{"streams":[{"codec_type":"audio","codec_name":"aac","sample_rate":"48000","channels":2,"duration":"12.5"}],"format":{"duration":"13.0"}}"#,
        )
        .unwrap();
        let info = parse_music_probe(&json, Path::new("/music/song.m4a")).unwrap();
        assert_eq!(info.name, "song.m4a");
        assert_eq!((info.sample_rate, info.channels, info.duration), (48000, 2, 12.5));
        let none: Value = serde_json::from_str(r#"{"streams":[],"format":{"duration":"3"}}"#).unwrap();
        let err = parse_music_probe(&none, Path::new("a.mp4")).unwrap_err();
        assert!(err.contains("no sound"), "{err}");
    }

    fn tone(out: &mut [f32], at: f64, f: f64, amp: f64, len: f64, rate: u32) {
        let s0 = (at * rate as f64).round() as usize;
        for j in 0..(len * rate as f64) as usize {
            if s0 + j >= out.len() {
                break;
            }
            let t = j as f64 / rate as f64;
            let env = (t / 0.01).min(1.0) * (-3.0 * t).exp();
            let w = 2.0 * std::f64::consts::PI * f * t;
            out[s0 + j] += (amp * env * (w.sin() + 0.3 * (2.0 * w).sin())) as f32;
        }
    }

    fn burst(out: &mut [f32], at: f64, amp: f32, decay: f64, len: f64, rate: u32, rng: &mut Lcg) {
        let s0 = (at * rate as f64).round() as usize;
        for j in 0..(len * rate as f64) as usize {
            if s0 + j >= out.len() {
                break;
            }
            let t = j as f64 / rate as f64;
            out[s0 + j] += amp * rng.next() * (-decay * t).exp() as f32;
        }
    }

    /// A song in sections of eight bars: two of drums (kick on every beat,
    /// snare on two and four, off-beat hats, a sixteenth-note fill at the end
    /// of each eighth bar) and then a breakdown of pad chords that change a
    /// little after the bar, with no drums. Breakdowns and fills are where a
    /// beat tracker adds or drops a beat.
    fn song_with_breakdowns(bpm: f64, seconds: f64, t0: f64, seed: u64) -> Vec<f32> {
        let rate = ANALYSIS_RATE;
        let mut out = vec![0.0f32; (seconds * rate as f64) as usize];
        let mut rng = Lcg(seed + 5);
        let p = 60.0 / bpm;
        let bars = ((seconds - t0) / (4.0 * p)) as usize + 1;
        let mut kicks = Vec::new();
        for b in 0..bars {
            let at = |beat: f64| t0 + (4 * b) as f64 * p + beat * p;
            if (b / 8) % 3 == 2 {
                let x = 0.3 + 0.2 * (rng.next() as f64 + 1.0);
                tone(&mut out, at(x), 220.0 * (1.0 + 0.1 * (b % 3) as f64), 0.2, 3.0 * p, rate);
                tone(&mut out, at(x + 1.37), 330.0, 0.1, 2.0 * p, rate);
                continue;
            }
            kicks.extend((0..4).map(|k| at(k as f64)));
            for k in [1.0, 3.0] {
                burst(&mut out, at(k), 0.35, 18.0, 0.2, rate, &mut rng);
            }
            for k in 0..4 {
                burst(&mut out, at(k as f64 + 0.5), 0.1, 70.0, 0.05, rate, &mut rng);
            }
            if b % 8 == 7 {
                for k in 0..8 {
                    burst(&mut out, at(2.0 + k as f64 * 0.25), 0.3, 25.0, 0.1, rate, &mut rng);
                }
            }
        }
        let drums_only = drums(&kicks, seconds, rate, false, 0.0, 0);
        for (o, d) in out.iter_mut().zip(drums_only) {
            *o += d + 0.005 * rng.next();
        }
        out
    }

    #[test]
    fn tempo_is_exact_through_breakdowns_and_fills() {
        // Counting the tracked beats one by one read some of these up to
        // half a percent off, from a beat lost or added in a breakdown.
        for (bpm, seed) in [(143.6, 0), (97.0, 4), (121.3, 3), (128.0, 5)] {
            let (secs, t0) = (150.0, 0.31);
            let g = analyze_music(&song_with_breakdowns(bpm, secs, t0, seed), ANALYSIS_RATE);
            let err = (g.tempo - bpm) / bpm * 100.0;
            let truth: Vec<f64> = (0..).map(|i| t0 + i as f64 * 60.0 / bpm).take_while(|t| *t < secs - 0.1).collect();
            let hits = hit_rate(&truth, &g.beats, 0.035);
            eprintln!("{bpm} BPM with breakdowns: tempo {:.3} ({err:+.3}%), {:.1}% of beats within 35 ms", g.tempo, hits * 100.0);
            assert!(err.abs() < 0.3, "{bpm} BPM read as {}", g.tempo);
            assert!(hits >= 0.85, "{bpm} BPM: {:.0}% of beats on the beat", hits * 100.0);
        }
    }

    #[test]
    fn beats_are_counted_past_extra_and_missing_ones() {
        let p = 0.5;
        let mut t: Vec<f64> = (0..40).map(|i| 0.2 + i as f64 * p).collect();
        // One missing, one extra half way, three on the off-beat for a while.
        t.remove(10);
        t.insert(20, 0.2 + 20.5 * p);
        for x in t.iter_mut().skip(30).take(3) {
            *x += 0.5 * p;
        }
        let fitted = fit_period(&t, 0.49).unwrap();
        assert!((fitted - p).abs() < 1e-3, "{fitted}");
        assert!(fit_period(&t[..3], p).is_none());
        // A breakdown of weak beats that wander is put back on the grid.
        let beats: Vec<f64> = (0..12).map(|i| i as f64 * p + if (4..8).contains(&i) { 0.1 } else { 0.0 }).collect();
        let s: Vec<f32> = (0..12).map(|i| if (4..8).contains(&i) { 0.1 } else { 1.0 }).collect();
        let out = regularise_weak_runs(&beats, &s, &|x| x >= 0.3, p);
        assert_eq!(out.len(), 12);
        assert!(out.iter().enumerate().all(|(i, t)| (t - i as f64 * p).abs() < 1e-9), "{out:?}");
    }

}
