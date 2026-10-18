//! Listening to a clip's sound: where someone is talking, where the pauses
//! between phrases are, and sound problems (wind, clipping, dropouts).
//!
//! Everything here is classical signal arithmetic on the decoded samples. No
//! machine learning and no network access.
//!
//! # Decoding
//!
//! ffmpeg decodes the first audio stream to 32-bit float samples at the
//! stream's own sample rate and pipes them to us. Mono stays mono and stereo
//! stays stereo, so clipping is judged on each channel as recorded; sources
//! with more than two channels are mixed down to stereo (`-ac 2`). The
//! samples are measured as they arrive, a chunk at a time, so a two hour clip
//! never sits in memory.
//!
//! Timing: ffmpeg runs with `-copyts`, the audio timestamps are shifted by the
//! clip's `start_time` (the video's start, as `analyze.rs` uses) and
//! `aresample=first_pts=0` pads the front with silence when the sound starts
//! after the picture, or trims it when the sound starts before (encoder
//! priming). So sample 0 of what we measure lines up with the picture's t=0.
//! Gaps in the middle of a stream are not filled.
//!
//! # Measurements
//!
//! The samples are cut into windows of `AUDIO_STEP` (0.05 s, 20 a second).
//! Before measuring loudness and bands, a 10 Hz high-pass removes any DC
//! offset. Per window the engine keeps:
//!
//! 1. `level`: RMS loudness in dBFS (a full-scale square wave is 0 dBFS, a
//!    full-scale sine about -3), taking the louder channel, so a dead channel
//!    beside a live one does not lower it. Stored in half dB steps as 0 to 255
//!    meaning 0 to -127.5 dBFS.
//! 2. `voice`: share of the energy (all channels together) between 300 and
//!    3400 Hz, after a second-order high-pass at 300 Hz and low-pass at
//!    3400 Hz. 0 to 255 means 0 to 1.
//! 3. `low`: share of the energy below about 150 Hz, after a fourth-order
//!    Butterworth low-pass at 150 Hz. 0 to 255 means 0 to 1.
//! 4. `clipped`: clipping events, each a run of 3 or more consecutive samples
//!    on one channel at or above 0.999 of full scale. An event is counted in
//!    the window where its third sample falls.
//! 5. `corr_low`: correlation between the two channels' low-passed signals,
//!    -100 to 100 for -1 to 1. Wind is turbulence at each microphone, so it
//!    is incoherent between channels, where real bass reaches both. Always 0
//!    for mono, where `channels` says 1.
//!
//! # Judgements
//!
//! Noise floor: the 10th percentile of window levels. A window has sound
//! present when it is above `max(-55, min(floor + 10, p95 - 12))` dBFS:
//! clearly above the floor, or close to the loud part of the clip for clips
//! that never go quiet (a steady tone throughout counts as sound present).
//!
//! Talking (see `talking`): sound present, a voice in the window, and the
//! loudness envelope dipping at syllable rate. A voice means at least 40% of
//! the window's energy in the voice band, or, for voices whose energy sits
//! mostly at the fundamental below 300 Hz (synthetic voices such as ffmpeg's
//! flite, some deep or close voices), at least 15% there with at most half
//! below 150 Hz while the spectral balance keeps changing: an average change
//! of the voice and low shares of at least 0.09 from one window to the next
//! over 0.75 s either way, which a melody, a tone, surf or footsteps do not reach.
//! A dip is a window whose level is at least 6 dB below the loudest window
//! within 0.3 s on both sides, so the envelope rises and falls at 2 Hz or
//! faster; talking needs 2 or more dips within 0.75 s either way. Steady
//! tones, hum, rumble and sustained notes have no such dips. Gaps of up to
//! 0.2 s inside talking are filled (gaps between syllables and words), so
//! pauses are 0.25 s or longer, and talking stretches shorter than 0.15 s
//! are dropped. A clip has speech when talking covers over 10% of it and at
//! least 1.5 s.
//!
//! Wind (see `wind`): sound present, at least 60% of the window's energy
//! below 150 Hz and 75% on average over a second around it, and an erratic
//! level. In stereo, when the channels' low bands correlate below 0.5 over
//! about half a second, a level spread of 1.5 dB over a second is enough.
//! Otherwise (mono, dual mono, or coherent stereo) it needs a 3 dB spread,
//! since steady bass, traffic hum and a bass tone vary much less than gusts.
//! Gaps of up to 0.5 s are filled and runs under 1 s dropped. A select is
//! noted for wind when wind covers 40% of it. Strong wind (`bad_sound_in`)
//! also needs half of the select and a median level of -30 dBFS or louder;
//! only strong wind, clipping and dropouts set selects aside.
//!
//! Dropouts (see `dropouts`): only in clips whose median level is above
//! -55 dBFS. Either 0.1 s or more of near digital silence (below -75 dBFS
//! and 8 dB below the noise floor) with louder sound on both sides, or a
//! sudden drop of 35 dB or more (against the 0.15 s on each side) to below
//! the noise floor for 0.1 to 2 s that comes back. The first and last 0.3 s
//! are ignored.
//!
//! # Known limits
//!
//! Rhythmic music in the voice band (a vocal line, a plucked melody, a drum
//! loop) and birdsong sweeping through the band can read as talking. Whispering and very distant speech may sit too close to the floor
//! to count. Wind in mono that blows steadily without gusts is missed, and
//! loud gusting bass music in mono can read as wind. Tracks that go to
//! digital silence for more than a tenth of their length (a noise gate,
//! edited sound) push the noise floor down to silence, so their dropouts are
//! not reported. A dropout on one channel of two is not reported, since the
//! level follows the louder channel. Clipping after lossy encoding (AAC)
//! often overshoots and rings instead of sitting flat, so some clipping in
//! compressed files is undercounted.

use crate::analyze::AnalyzeError;
use crate::ffmpeg;
use crate::probe::{friendly_ffmpeg_error, ClipInfo};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Seconds per measurement window.
pub const AUDIO_STEP: f32 = 0.05;
/// Windows per second, the exact reciprocal of `AUDIO_STEP`.
const WINDOWS_PER_SECOND: u64 = 20;

/// Level code for digital silence: -127.5 dBFS.
const SILENT_CODE: u8 = 255;
/// A clipping event: this many samples in a row at or above `CLIP_LEVEL`.
pub const CLIP_RUN: u32 = 3;
pub const CLIP_LEVEL: f32 = 0.999;

/// Per-window measurements stored column by column in compact integers.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AudioTable {
    /// Seconds per window (`AUDIO_STEP`).
    pub step: f32,
    /// Channels measured: 1 or 2.
    pub channels: u32,
    pub sample_rate: u32,
    /// RMS level of the louder channel in half dB below full scale: 0 is 0 dBFS, 255 is -127.5 dBFS or quieter.
    #[serde(default)]
    pub level: Vec<u8>,
    /// Share of energy between 300 and 3400 Hz, 0 to 255 for 0 to 1.
    #[serde(default)]
    pub voice: Vec<u8>,
    /// Share of energy below 150 Hz, 0 to 255 for 0 to 1.
    #[serde(default)]
    pub low: Vec<u8>,
    /// Clipping events that reached their third sample in this window.
    #[serde(default)]
    pub clipped: Vec<u16>,
    /// Correlation of the two channels' low bands, -100 to 100; 0 for mono.
    #[serde(default)]
    pub corr_low: Vec<i8>,
}

impl AudioTable {
    pub fn len(&self) -> usize {
        self.level.len()
    }
    pub fn is_empty(&self) -> bool {
        self.level.is_empty()
    }
    pub fn level_db(&self, i: usize) -> f32 {
        -(self.level[i] as f32) * 0.5
    }
    /// Share of window `i`'s energy in the voice band, 0 to 1.
    pub fn voice_share(&self, i: usize) -> f32 {
        self.voice.get(i).map(|&v| v as f32 / 255.0).unwrap_or(0.0)
    }
    /// Share of window `i`'s energy below 150 Hz, 0 to 1.
    pub fn low_share(&self, i: usize) -> f32 {
        self.low.get(i).map(|&v| v as f32 / 255.0).unwrap_or(0.0)
    }
    /// Correlation of the channels' low bands in window `i`, -1 to 1 (0 for mono).
    pub fn low_correlation(&self, i: usize) -> f32 {
        self.corr_low.get(i).map(|&v| v as f32 / 100.0).unwrap_or(0.0)
    }
    /// Clipping events in window `i`.
    pub fn clipped_in(&self, i: usize) -> u32 {
        self.clipped.get(i).copied().unwrap_or(0) as u32
    }
    fn step64(&self) -> f64 {
        let s = if self.step > 0.0 { self.step } else { AUDIO_STEP };
        // 0.05 as f32 is not exact; round the float noise away.
        (s as f64 * 1e6).round() / 1e6
    }
    /// Centre of window `i` in seconds.
    pub fn time_of(&self, i: usize) -> f64 {
        (i as f64 + 0.5) * self.step64()
    }
    /// Seconds covered by the table.
    pub fn duration(&self) -> f64 {
        self.len() as f64 * self.step64()
    }
    /// Windows whose centres fall in start..end.
    fn window_range(&self, start: f64, end: f64) -> (usize, usize) {
        let s = self.step64();
        let n = self.len();
        let first = |t: f64| -> usize {
            let i = (t / s - 0.5).ceil();
            if i <= 0.0 {
                0
            } else {
                (i as usize).min(n)
            }
        };
        let a = first(start);
        let b = first(end).max(a);
        (a, b)
    }
}

/// A sound problem worth telling people about.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AudioIssue {
    Wind,
    Clipped,
    Dropout,
}

impl AudioIssue {
    pub fn words(self) -> &'static str {
        match self {
            AudioIssue::Wind => "Wind noise",
            AudioIssue::Clipped => "Sound clipped",
            AudioIssue::Dropout => "Sound drops out",
        }
    }
}

/// What the whole clip's sound is like.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AudioSummary {
    pub has_speech: bool,
    /// Share of the clip with someone talking, 0 to 1.
    pub talk_share: f32,
    /// Share of the clip with wind noise, 0 to 1.
    pub wind_share: f32,
    pub clipped_events: u32,
    /// Start and end seconds of each dropout.
    pub dropouts: Vec<(f64, f64)>,
    pub noise_floor_db: f32,
    pub median_db: f32,
    /// The track is essentially digital silence throughout.
    pub silent: bool,
}

// ---------------------------------------------------------------------------
// Measuring

#[derive(Clone, Copy, Debug)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    /// Coefficients from the RBJ audio EQ cookbook.
    fn new(high: bool, freq: f64, sample_rate: f64, q: f64) -> Biquad {
        let f = freq.min(sample_rate * 0.45);
        let w0 = 2.0 * std::f64::consts::PI * f / sample_rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        let a0 = 1.0 + alpha;
        let (b0, b1, b2) = if high {
            ((1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0)
        } else {
            ((1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0)
        };
        Biquad {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }
    fn lowpass(freq: f64, sample_rate: f64, q: f64) -> Biquad {
        Biquad::new(false, freq, sample_rate, q)
    }
    fn highpass(freq: f64, sample_rate: f64, q: f64) -> Biquad {
        Biquad::new(true, freq, sample_rate, q)
    }
    #[inline]
    fn run(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        // Decaying state in silence would otherwise turn subnormal and slow.
        if self.z1.abs() < 1e-30 {
            self.z1 = 0.0;
        }
        if self.z2.abs() < 1e-30 {
            self.z2 = 0.0;
        }
        y
    }
}

#[derive(Clone, Debug)]
struct ChannelState {
    dc_r: f64,
    dc_x1: f64,
    dc_y1: f64,
    voice_hp: Biquad,
    voice_lp: Biquad,
    low_a: Biquad,
    low_b: Biquad,
    clip_run: u32,
}

impl ChannelState {
    fn new(sr: f64) -> Self {
        ChannelState {
            dc_r: 1.0 - 2.0 * std::f64::consts::PI * 10.0 / sr,
            dc_x1: 0.0,
            dc_y1: 0.0,
            voice_hp: Biquad::highpass(300.0, sr, std::f64::consts::FRAC_1_SQRT_2),
            voice_lp: Biquad::lowpass(3400.0, sr, std::f64::consts::FRAC_1_SQRT_2),
            // A fourth-order Butterworth as two second-order sections.
            low_a: Biquad::lowpass(150.0, sr, 0.541_196_1),
            low_b: Biquad::lowpass(150.0, sr, 1.306_563),
            clip_run: 0,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct WindowSums {
    energy: [f64; 2],
    voice: [f64; 2],
    low: [f64; 2],
    low_cross: f64,
    clipped: u32,
    frames: u64,
}

/// Streaming measurement: feed interleaved samples in any chunk sizes with
/// `push`, then `finish` for the table.
#[derive(Clone, Debug)]
pub struct AudioMeter {
    channels: usize,
    sample_rate: u32,
    state: Vec<ChannelState>,
    sums: WindowSums,
    /// Sample frames seen so far.
    frame: u64,
    /// Frame index where the current window ends.
    window_end: u64,
    window_start: u64,
    table: AudioTable,
    /// A frame split across two `push` calls.
    partial: Vec<f32>,
}

impl AudioMeter {
    /// `channels` is 1 or 2 (more are treated as the first two).
    pub fn new(channels: usize, sample_rate: u32) -> Self {
        let channels = channels.max(1);
        let sample_rate = sample_rate.max(1);
        let sr = sample_rate as f64;
        AudioMeter {
            channels,
            sample_rate,
            state: (0..channels.min(2)).map(|_| ChannelState::new(sr)).collect(),
            sums: WindowSums::default(),
            frame: 0,
            window_end: window_boundary(1, sample_rate),
            window_start: 0,
            table: AudioTable {
                step: AUDIO_STEP,
                channels: channels.min(2) as u32,
                sample_rate,
                ..Default::default()
            },
            partial: Vec::new(),
        }
    }

    /// Measure more interleaved samples.
    pub fn push(&mut self, samples: &[f32]) {
        let ch = self.channels;
        let mut rest = samples;
        if !self.partial.is_empty() {
            let need = ch - self.partial.len();
            let take = need.min(rest.len());
            self.partial.extend_from_slice(&rest[..take]);
            rest = &rest[take..];
            if self.partial.len() < ch {
                return;
            }
            let frame = std::mem::take(&mut self.partial);
            self.push_frame(&frame);
        }
        let whole = rest.len() / ch * ch;
        for f in rest[..whole].chunks_exact(ch) {
            self.push_frame(f);
        }
        self.partial.extend_from_slice(&rest[whole..]);
    }

    #[inline]
    fn push_frame(&mut self, f: &[f32]) {
        let used = self.state.len();
        let mut lows = [0.0f64; 2];
        for (c, st) in self.state.iter_mut().enumerate() {
            let x = f[c];
            if x.abs() >= CLIP_LEVEL {
                st.clip_run += 1;
                if st.clip_run == CLIP_RUN {
                    self.sums.clipped += 1;
                }
            } else {
                st.clip_run = 0;
            }
            let x = x as f64;
            let d = x - st.dc_x1 + st.dc_r * st.dc_y1;
            st.dc_x1 = x;
            st.dc_y1 = if d.abs() < 1e-30 { 0.0 } else { d };
            let v = st.voice_lp.run(st.voice_hp.run(d));
            let l = st.low_b.run(st.low_a.run(d));
            self.sums.energy[c] += d * d;
            self.sums.voice[c] += v * v;
            self.sums.low[c] += l * l;
            lows[c] = l;
        }
        if used == 2 {
            self.sums.low_cross += lows[0] * lows[1];
        }
        self.sums.frames += 1;
        self.frame += 1;
        if self.frame >= self.window_end {
            self.close_window();
        }
    }

    fn close_window(&mut self) {
        let s = std::mem::take(&mut self.sums);
        let n = s.frames.max(1) as f64;
        let used = self.state.len();
        let loudest = s.energy[..used].iter().cloned().fold(0.0f64, f64::max) / n;
        let level = if loudest > 0.0 {
            let db = 10.0 * loudest.log10();
            (-db * 2.0).round().clamp(0.0, SILENT_CODE as f64) as u8
        } else {
            SILENT_CODE
        };
        let total: f64 = s.energy[..used].iter().sum();
        let share = |part: f64| -> u8 {
            if total > 1e-20 {
                ((part / total).clamp(0.0, 1.0) * 255.0).round() as u8
            } else {
                0
            }
        };
        let voice = share(s.voice[..used].iter().sum());
        let low = share(s.low[..used].iter().sum());
        let corr = if used == 2 && s.low[0] > 1e-20 && s.low[1] > 1e-20 {
            (s.low_cross / (s.low[0] * s.low[1]).sqrt()).clamp(-1.0, 1.0)
        } else {
            0.0
        };
        self.table.level.push(level);
        self.table.voice.push(voice);
        self.table.low.push(low);
        self.table.clipped.push(s.clipped.min(u16::MAX as u32) as u16);
        self.table.corr_low.push((corr * 100.0).round() as i8);
        self.window_start = self.window_end;
        let next = self.table.len() as u64 + 1;
        self.window_end = window_boundary(next, self.sample_rate);
    }

    /// Windows measured so far.
    pub fn len(&self) -> usize {
        self.table.len()
    }
    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    /// The finished table. A last part-window counts when it holds at least half a window.
    pub fn finish(mut self) -> AudioTable {
        let full = self.window_end - self.window_start;
        if self.sums.frames > 0 && self.sums.frames * 2 >= full {
            self.close_window();
        }
        self.table
    }
}

/// First sample frame of window `i`.
fn window_boundary(i: u64, sample_rate: u32) -> u64 {
    i * sample_rate as u64 / WINDOWS_PER_SECOND
}

/// The same measurement on interleaved samples already in memory (used by tests).
pub fn analyze_samples(samples: &[f32], channels: usize, sample_rate: u32) -> AudioTable {
    let mut m = AudioMeter::new(channels, sample_rate);
    m.push(samples);
    m.finish()
}

// ---------------------------------------------------------------------------
// Decoding

/// Channels the decoder is asked for: mono stays mono, everything else is stereo.
pub fn decoded_channels(info: &ClipInfo) -> usize {
    if info.audio_channels == 1 {
        1
    } else {
        2
    }
}

/// The sample rate the decoder is asked for: the stream's own, or 48 kHz when unknown.
pub fn decoded_sample_rate(info: &ClipInfo) -> u32 {
    if info.audio_sample_rate > 0 {
        info.audio_sample_rate
    } else {
        48_000
    }
}

/// The ffmpeg command that writes the first audio stream to stdout as
/// interleaved 32-bit float samples lined up with the picture's t=0.
pub fn audio_command(info: &ClipInfo) -> std::process::Command {
    let channels = decoded_channels(info);
    let rate = decoded_sample_rate(info);
    let start = if info.start_time.is_finite() { info.start_time } else { 0.0 };
    let filter = format!("asetpts=PTS-{start:.6}/TB,aresample=first_pts=0");
    let mut cmd = ffmpeg::command("ffmpeg");
    cmd.args(["-hide_banner", "-nostdin", "-nostats", "-loglevel", "error", "-copyts"]);
    cmd.arg("-i").arg(&info.path);
    cmd.args(["-map", "0:a:0", "-vn", "-sn", "-dn", "-af", &filter]);
    // The same count as the source for mono and stereo (no mixing happens),
    // a mix to stereo for more channels or an unknown count.
    cmd.args(["-ac", &channels.to_string(), "-ar", &rate.to_string()]);
    cmd.args(["-f", "f32le", "-acodec", "pcm_f32le", "pipe:1"]);
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd
}

/// Decode and measure the first audio stream. Ok(None) when the clip has no audio.
pub fn analyze_audio(info: &ClipInfo, cancel: &AtomicBool) -> Result<Option<AudioTable>, AnalyzeError> {
    if !info.has_audio {
        return Ok(None);
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(AnalyzeError::Cancelled);
    }
    let channels = decoded_channels(info);
    let mut child = audio_command(info).spawn().map_err(|e| AnalyzeError::ToolMissing(e.to_string()))?;

    let stderr_lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let stderr = child.stderr.take().expect("stderr piped");
    let lines = stderr_lines.clone();
    let err_thread = std::thread::spawn(move || {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stderr);
        for line in reader.split(b'\n').map_while(Result::ok) {
            let mut l = lines.lock().unwrap();
            l.push(String::from_utf8_lossy(&line).trim().to_string());
            if l.len() > 40 {
                l.remove(0);
            }
        }
    });

    let mut stdout = child.stdout.take().expect("stdout piped");
    let mut meter = AudioMeter::new(channels, decoded_sample_rate(info));
    // About 85 ms of 48 kHz stereo per read.
    let mut buf = vec![0u8; 32 * 1024];
    let mut filled = 0usize;
    let mut samples: Vec<f32> = Vec::with_capacity(buf.len() / 4);
    let mut cancelled = false;
    loop {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        match stdout.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
        let whole = filled / 4 * 4;
        samples.clear();
        samples.extend(buf[..whole].chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])));
        meter.push(&samples);
        buf.copy_within(whole..filled, 0);
        filled -= whole;
    }
    if cancelled {
        let _ = child.kill();
    }
    drop(stdout);
    let status = child.wait();
    let _ = err_thread.join();
    if cancelled {
        return Err(AnalyzeError::Cancelled);
    }
    let ok = status.map(|s| s.success()).unwrap_or(false);
    let table = meter.finish();
    if !ok && table.is_empty() {
        let lines = stderr_lines.lock().unwrap();
        if lines.iter().any(|l| l.contains("matches no streams")) {
            return Ok(None);
        }
        let line = lines
            .iter()
            .rev()
            .find(|l| {
                let l = l.to_ascii_lowercase();
                l.contains("error") || l.contains("invalid") || l.contains("no such")
            })
            .or(lines.iter().rev().find(|l| !l.is_empty()))
            .cloned()
            .unwrap_or_default();
        return Err(AnalyzeError::Decode(format!("The sound could not be read. {}", friendly_ffmpeg_error(&line))));
    }
    if table.is_empty() {
        return Ok(None);
    }
    Ok(Some(table))
}

// ---------------------------------------------------------------------------
// Judging

/// Level percentiles of a whole clip, in dBFS.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LevelStats {
    /// 10th percentile: the noise floor.
    pub floor: f32,
    pub median: f32,
    pub p95: f32,
}

impl LevelStats {
    /// Windows louder than this have sound present.
    pub fn presence_threshold(&self) -> f32 {
        (self.floor + 10.0).min(self.p95 - 12.0).max(-55.0)
    }
}

/// Level percentiles from a histogram of the level codes.
pub fn level_stats(t: &AudioTable) -> LevelStats {
    let n = t.len();
    if n == 0 {
        let s = -(SILENT_CODE as f32) * 0.5;
        return LevelStats { floor: s, median: s, p95: s };
    }
    let mut hist = [0usize; 256];
    for &c in &t.level {
        hist[c as usize] += 1;
    }
    // Quietest first: high codes are quiet.
    let pct = |q: f64| -> f32 {
        let target = q * n as f64;
        let mut cum = 0usize;
        for code in (0..256).rev() {
            cum += hist[code];
            if cum as f64 > target {
                return -(code as f32) * 0.5;
            }
        }
        0.0
    };
    LevelStats { floor: pct(0.10), median: pct(0.50), p95: pct(0.95) }
}

/// Fill false gaps of up to `max_gap` windows between true runs, then clear
/// true runs shorter than `min_run` windows.
fn tidy_runs(v: &mut [bool], max_gap: usize, min_run: usize) {
    let n = v.len();
    let mut last_true: Option<usize> = None;
    for i in 0..n {
        if v[i] {
            if let Some(p) = last_true {
                let gap = i - p - 1;
                if gap > 0 && gap <= max_gap {
                    v[p + 1..i].iter_mut().for_each(|x| *x = true);
                }
            }
            last_true = Some(i);
        }
    }
    let mut i = 0;
    while i < n {
        if v[i] {
            let start = i;
            while i < n && v[i] {
                i += 1;
            }
            if i - start < min_run {
                v[start..i].iter_mut().for_each(|x| *x = false);
            }
        } else {
            i += 1;
        }
    }
}

/// Runs of true as (first, end) window indexes, end exclusive.
fn runs(v: &[bool]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < v.len() {
        if v[i] {
            let s = i;
            while i < v.len() && v[i] {
                i += 1;
            }
            out.push((s, i));
        } else {
            i += 1;
        }
    }
    out
}

/// Levels with the quiet end clamped, so silences do not dominate envelope measures.
fn clamped_levels(t: &AudioTable, s: &LevelStats) -> Vec<f32> {
    let lo = s.floor.max(-70.0).max(s.p95 - 30.0).min(s.p95);
    (0..t.len()).map(|i| t.level_db(i).max(lo)).collect()
}

/// Talking windows before gap filling: at least this share of the energy in
/// the voice band, or `TALK_VOICE_SOME` there with at most `TALK_LOW_MAX`
/// below 150 Hz and a changing spectral balance (`TALK_FLUX`).
const TALK_VOICE_SHARE: f32 = 0.4;
/// Voices whose fundamental and lowest harmonics (150 to 300 Hz) carry most
/// of the energy, as synthetic voices and some deep or close-miked ones do,
/// put only 15 to 30% in the 300 to 3400 Hz band.
const TALK_VOICE_SOME: f32 = 0.15;
const TALK_LOW_MAX: f32 = 0.5;
/// Smallest average change in the spectral balance (voice and low shares)
/// from one window to the next, over 0.75 s each side, for the weaker voice
/// band test. Speech moves between vowels, consonants and breaths every few
/// windows (flite's voices average 0.10 to 0.16); a melody, a steady tone,
/// surf, footsteps or noise keep their balance even when the level pulses
/// (0.00 to 0.05).
const TALK_FLUX: f32 = 0.09;
const DIP_DB: f32 = 6.0;
/// 0.3 s each side for a dip's surrounding peaks.
const DIP_REACH: usize = 6;
/// 0.75 s each side when counting dips.
const DIP_SPAN: usize = 15;
const TALK_MIN_DIPS: usize = 2;
/// Gaps up to 0.2 s stay talking.
const TALK_GAP: usize = 4;
const TALK_MIN_RUN: usize = 3;

/// Per window: someone is talking.
pub fn talking(t: &AudioTable) -> Vec<bool> {
    let n = t.len();
    if n == 0 {
        return Vec::new();
    }
    let s = level_stats(t);
    let thr = s.presence_threshold();
    let e = clamped_levels(t, &s);
    // Syllable-rate dips in the envelope.
    let mut dip_prefix = vec![0usize; n + 1];
    for i in 0..n {
        let mut dip = false;
        if i > 0 && i + 1 < n && e[i] <= e[i - 1] && e[i] < e[i + 1] {
            let before = e[i.saturating_sub(DIP_REACH)..i].iter().cloned().fold(f32::MIN, f32::max);
            let after = e[i + 1..(i + 1 + DIP_REACH).min(n)].iter().cloned().fold(f32::MIN, f32::max);
            dip = before >= e[i] + DIP_DB && after >= e[i] + DIP_DB;
        }
        dip_prefix[i + 1] = dip_prefix[i] + dip as usize;
    }
    // Change of spectral balance between consecutive windows with sound,
    // as prefix sums of the change and of the pairs counted.
    let present: Vec<bool> = (0..n).map(|i| t.level_db(i) > thr).collect();
    let mut flux_sum = vec![0f32; n + 1];
    let mut flux_cnt = vec![0u32; n + 1];
    for i in 0..n {
        let pair = i > 0 && present[i] && present[i - 1];
        let change = if pair { (t.voice_share(i) - t.voice_share(i - 1)).abs() + (t.low_share(i) - t.low_share(i - 1)).abs() } else { 0.0 };
        flux_sum[i + 1] = flux_sum[i] + change;
        flux_cnt[i + 1] = flux_cnt[i] + pair as u32;
    }
    let mut v: Vec<bool> = (0..n)
        .map(|i| {
            let a = i.saturating_sub(DIP_SPAN);
            let b = (i + DIP_SPAN + 1).min(n);
            let (voice, low) = (t.voice_share(i), t.low_share(i));
            // Pairs whose later window lies in a+1..b, so both lie in a..b.
            let pairs = flux_cnt[b] - flux_cnt[(a + 1).min(b)];
            let changing = || pairs > 0 && (flux_sum[b] - flux_sum[(a + 1).min(b)]) / pairs as f32 >= TALK_FLUX;
            let voiced = voice >= TALK_VOICE_SHARE || (voice >= TALK_VOICE_SOME && low <= TALK_LOW_MAX && changing());
            present[i] && voiced && dip_prefix[b] - dip_prefix[a] >= TALK_MIN_DIPS
        })
        .collect();
    tidy_runs(&mut v, TALK_GAP, TALK_MIN_RUN);
    v
}

fn speech_present(t: &AudioTable, talk: &[bool]) -> bool {
    let count = talk.iter().filter(|&&x| x).count();
    !t.is_empty() && count as f32 / t.len() as f32 > 0.10 && count as f64 * t.step64() >= 1.5
}

/// Quiet gaps of at least `min_len` seconds, as (start, end) seconds, only
/// meaningful when has_speech. Includes gaps before the first and after the
/// last talking stretch. Empty when the clip has no speech.
pub fn pauses(t: &AudioTable, min_len: f32) -> Vec<(f64, f64)> {
    let talk = talking(t);
    if !speech_present(t, &talk) {
        return Vec::new();
    }
    let step = t.step64();
    let mut gaps = Vec::new();
    let mut prev_end = 0usize;
    for (a, b) in runs(&talk) {
        gaps.push((prev_end, a));
        prev_end = b;
    }
    gaps.push((prev_end, t.len()));
    let min = min_len.max(0.0) as f64 - 1e-9;
    gaps.into_iter()
        .filter(|&(a, b)| b > a)
        .map(|(a, b)| (a as f64 * step, b as f64 * step))
        .filter(|&(a, b)| b - a >= min)
        .collect()
}

/// Share of start..end with someone talking, 0 to 1.
pub fn talk_share_in(t: &AudioTable, start: f64, end: f64) -> f32 {
    let (a, b) = t.window_range(start, end);
    if b <= a {
        return 0.0;
    }
    let talk = talking(t);
    talk[a..b].iter().filter(|&&x| x).count() as f32 / (b - a) as f32
}

const WIND_LOW_SHARE: f32 = 0.6;
/// Average share below 150 Hz over the windows with sound within
/// `WIND_SPREAD_SPAN`: gusts carry nearly all their energy there, while surf,
/// traffic or a voice over a breeze keep a good part above it.
const WIND_LOW_AVERAGE: f32 = 0.75;
/// Below this low band correlation, stereo counts as incoherent.
const WIND_INCOHERENT: f32 = 0.5;
const WIND_SPREAD_STEREO: f32 = 1.5;
const WIND_SPREAD_MONO: f32 = 3.0;
/// 0.5 s each side for the level spread.
const WIND_SPREAD_SPAN: usize = 10;
/// About 0.25 s each side for the correlation.
const WIND_CORR_SPAN: usize = 5;
const WIND_GAP: usize = 10;
/// Wind has to last a second; shorter bursts are bumps and handling noise.
const WIND_MIN_RUN: usize = 20;

/// Per window: wind noise.
pub fn wind(t: &AudioTable) -> Vec<bool> {
    let n = t.len();
    if n == 0 {
        return Vec::new();
    }
    let s = level_stats(t);
    let thr = s.presence_threshold();
    let e: Vec<f64> = (0..n).map(|i| t.level_db(i).max(s.floor) as f64).collect();
    let mut sum = vec![0.0f64; n + 1];
    let mut sq = vec![0.0f64; n + 1];
    let mut corr = vec![0.0f64; n + 1];
    let mut low = vec![0.0f64; n + 1];
    let mut present = vec![0usize; n + 1];
    for i in 0..n {
        sum[i + 1] = sum[i] + e[i];
        sq[i + 1] = sq[i] + e[i] * e[i];
        corr[i + 1] = corr[i] + t.low_correlation(i) as f64;
        let here = t.level_db(i) > thr;
        low[i + 1] = low[i] + if here { t.low_share(i) as f64 } else { 0.0 };
        present[i + 1] = present[i] + here as usize;
    }
    let stereo = t.channels >= 2;
    let mut v: Vec<bool> = (0..n)
        .map(|i| {
            if t.level_db(i) <= thr || t.low_share(i) < WIND_LOW_SHARE {
                return false;
            }
            let a = i.saturating_sub(WIND_SPREAD_SPAN);
            let b = (i + WIND_SPREAD_SPAN + 1).min(n);
            if ((low[b] - low[a]) / (present[b] - present[a]).max(1) as f64) < WIND_LOW_AVERAGE as f64 {
                return false;
            }
            let m = (b - a) as f64;
            let mean = (sum[b] - sum[a]) / m;
            let spread = ((sq[b] - sq[a]) / m - mean * mean).max(0.0).sqrt() as f32;
            let incoherent = stereo && {
                let a = i.saturating_sub(WIND_CORR_SPAN);
                let b = (i + WIND_CORR_SPAN + 1).min(n);
                (((corr[b] - corr[a]) / (b - a) as f64) as f32) < WIND_INCOHERENT
            };
            spread >= if incoherent { WIND_SPREAD_STEREO } else { WIND_SPREAD_MONO }
        })
        .collect();
    tidy_runs(&mut v, WIND_GAP, WIND_MIN_RUN);
    v
}

const DROPOUT_MEDIAN_MIN: f32 = -55.0;
const DROPOUT_DEEP_DB: f32 = -75.0;
const DROPOUT_BELOW_FLOOR: f32 = 8.0;
const DROPOUT_DROP_DB: f32 = 35.0;
/// 0.1 s.
const DROPOUT_MIN: usize = 2;
/// 2 s, for the sudden drop rule.
const DROPOUT_MAX_DROP: usize = 40;
/// 0.3 s at each end are ignored.
const DROPOUT_EDGE: usize = 6;
/// 0.15 s each side to judge a drop's suddenness.
const DROPOUT_SIDE: usize = 3;

/// Dropouts as (start, end) seconds, in order.
pub fn dropouts(t: &AudioTable) -> Vec<(f64, f64)> {
    let n = t.len();
    let s = level_stats(t);
    if n < 2 * DROPOUT_EDGE + DROPOUT_MIN || s.median <= DROPOUT_MEDIAN_MIN {
        return Vec::new();
    }
    let lv: Vec<f32> = (0..n).map(|i| t.level_db(i)).collect();
    let below = s.floor - DROPOUT_BELOW_FLOOR;
    let loud = s.median - 20.0;
    // Whether some window before i (or from i on) is clearly sounding.
    let mut loud_before = vec![false; n + 1];
    for i in 0..n {
        loud_before[i + 1] = loud_before[i] || lv[i] > loud;
    }
    let mut loud_after = vec![false; n + 1];
    for i in (0..n).rev() {
        loud_after[i] = loud_after[i + 1] || lv[i] > loud;
    }
    let inside = |a: usize, b: usize| a >= DROPOUT_EDGE && b + DROPOUT_EDGE <= n;
    let mut found: Vec<(usize, usize)> = Vec::new();

    let deep: Vec<bool> = lv.iter().map(|&l| l < DROPOUT_DEEP_DB && l < below).collect();
    for (a, b) in runs(&deep) {
        if b - a >= DROPOUT_MIN && inside(a, b) && loud_before[a] && loud_after[b] {
            found.push((a, b));
        }
    }

    let low: Vec<bool> = lv.iter().map(|&l| l < below).collect();
    for (a, b) in runs(&low) {
        if b - a < DROPOUT_MIN || b - a > DROPOUT_MAX_DROP || !inside(a, b) {
            continue;
        }
        let peak = lv[a..b].iter().cloned().fold(f32::MIN, f32::max);
        let before = lv[a.saturating_sub(DROPOUT_SIDE)..a].iter().cloned().fold(f32::MIN, f32::max);
        let after = lv[b..(b + DROPOUT_SIDE).min(n)].iter().cloned().fold(f32::MIN, f32::max);
        if peak + DROPOUT_DROP_DB <= before.min(after) {
            found.push((a, b));
        }
    }

    found.sort();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (a, b) in found {
        match merged.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => merged.push((a, b)),
        }
    }
    let step = t.step64();
    merged.into_iter().map(|(a, b)| (a as f64 * step, b as f64 * step)).collect()
}

pub fn summarize(t: &AudioTable) -> AudioSummary {
    if t.is_empty() {
        return AudioSummary { silent: true, ..Default::default() };
    }
    let s = level_stats(t);
    let talk = talking(t);
    let n = t.len() as f32;
    let wind = wind(t);
    AudioSummary {
        has_speech: speech_present(t, &talk),
        talk_share: talk.iter().filter(|&&x| x).count() as f32 / n,
        wind_share: wind.iter().filter(|&&x| x).count() as f32 / n,
        clipped_events: t.clipped.iter().map(|&c| c as u32).sum(),
        dropouts: dropouts(t),
        noise_floor_db: s.floor,
        median_db: s.median,
        silent: s.p95 < -85.0,
    }
}

/// Share of a range that must have wind before it is reported.
pub const WIND_REPORT_SHARE: f32 = 0.4;
/// Wind counts as strong, bad enough to set a select aside, when it covers at
/// least this share of the range and its windows have a median level of
/// `STRONG_WIND_DB` or louder. A light breeze rumbling well below the scene's
/// sound is reported but does not set selects aside.
pub const STRONG_WIND_SHARE: f32 = 0.5;
pub const STRONG_WIND_DB: f32 = -30.0;

/// Sound problems inside start..end, most important first, each listed once:
/// any dropout overlapping the range, any clipping event, and wind over at
/// least 40% of it.
pub fn issues_in(t: &AudioTable, start: f64, end: f64) -> Vec<AudioIssue> {
    let (a, b) = t.window_range(start, end);
    let mut out = Vec::new();
    if b <= a {
        return out;
    }
    if dropouts(t).iter().any(|&(s, e)| s < end && e > start) {
        out.push(AudioIssue::Dropout);
    }
    if t.clipped[a..b].iter().any(|&c| c > 0) {
        out.push(AudioIssue::Clipped);
    }
    let w = wind(t);
    if w[a..b].iter().filter(|&&x| x).count() as f32 >= WIND_REPORT_SHARE * (b - a) as f32 {
        out.push(AudioIssue::Wind);
    }
    out
}

/// Whether the sound inside start..end is bad enough to set a select aside
/// when people will use it: a dropout, clipping, or strong wind (see
/// `STRONG_WIND_SHARE`). A mild wind note alone is not.
pub fn bad_sound_in(t: &AudioTable, start: f64, end: f64) -> bool {
    let issues = issues_in(t, start, end);
    if issues.contains(&AudioIssue::Dropout) || issues.contains(&AudioIssue::Clipped) {
        return true;
    }
    if !issues.contains(&AudioIssue::Wind) {
        return false;
    }
    let (a, b) = t.window_range(start, end);
    let w = wind(t);
    let mut levels: Vec<f32> = (a..b).filter(|&i| w[i]).map(|i| t.level_db(i)).collect();
    if (levels.len() as f32) < STRONG_WIND_SHARE * (b - a) as f32 {
        return false;
    }
    levels.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    levels[levels.len() / 2] >= STRONG_WIND_DB
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    const SR: u32 = 48_000;

    /// Small deterministic random numbers.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 40) as f32 / (1u64 << 24) as f32
        }
        /// Roughly uniform in -1..1.
        fn noise(&mut self) -> f32 {
            self.next() * 2.0 - 1.0
        }
        fn range(&mut self, a: f32, b: f32) -> f32 {
            a + (b - a) * self.next()
        }
    }

    fn sine(freq: f32, amp: f32, secs: f32) -> Vec<f32> {
        let n = (secs * SR as f32) as usize;
        (0..n).map(|i| amp * (2.0 * PI * freq * i as f32 / SR as f32).sin()).collect()
    }

    fn interleave(l: &[f32], r: &[f32]) -> Vec<f32> {
        l.iter().zip(r).flat_map(|(&a, &b)| [a, b]).collect()
    }

    /// Low-passed noise below about 120 Hz with gusts, as wind on one microphone.
    fn wind_channel(seed: u64, secs: f32) -> Vec<f32> {
        let mut rng = Rng(seed);
        let n = (secs * SR as f32) as usize;
        let mut a = Biquad::lowpass(120.0, SR as f64, 0.541);
        let mut b = Biquad::lowpass(120.0, SR as f64, 1.307);
        let phase = rng.range(0.0, 6.0);
        let rate = rng.range(0.5, 0.9);
        (0..n)
            .map(|i| {
                let t = i as f32 / SR as f32;
                let gust = 0.15 + 0.85 * (0.5 + 0.5 * (2.0 * PI * rate * t + phase).sin()).powi(2);
                let x = b.run(a.run(rng.noise() as f64)) as f32;
                (x * 6.0 * gust).clamp(-0.98, 0.98)
            })
            .collect()
    }

    /// A speech-like signal: harmonic syllables with a 120 to 250 Hz
    /// fundamental and formant-shaped harmonics, about 4 syllables a second,
    /// in phrases of 1 to 3 s with pauses of 0.4 to 0.8 s, over quiet room
    /// noise near -60 dBFS. Returns the samples and the true pauses between
    /// phrases (end of one phrase's sound to the start of the next).
    fn speech(seed: u64, secs: f32) -> (Vec<f32>, Vec<(f64, f64)>) {
        let mut rng = Rng(seed);
        let n = (secs * SR as f32) as usize;
        let mut out: Vec<f32> = (0..n).map(|_| rng.noise() * 0.0017).collect();
        let mut t = 0.6f32;
        let mut gaps = Vec::new();
        let mut last_end = 0.0f32;
        let formants = [(500.0f32, 1.0f32), (1500.0, 0.6), (2500.0, 0.3)];
        while t < secs - 1.5 {
            if last_end > 0.0 {
                gaps.push((last_end as f64, t as f64));
            }
            let phrase_end = (t + rng.range(1.0, 3.0)).min(secs - 0.5);
            while t < phrase_end - 0.1 {
                let dur = rng.range(0.12, 0.22).min(phrase_end - t);
                let f0 = rng.range(120.0, 250.0);
                let shift = rng.range(0.8, 1.25);
                let amp = rng.range(0.05, 0.15);
                let a = (t * SR as f32) as usize;
                let len = (dur * SR as f32) as usize;
                for k in 0..len.min(n - a) {
                    let tt = k as f32 / SR as f32;
                    let env = 0.5 - 0.5 * (2.0 * PI * k as f32 / len as f32).cos();
                    let mut v = 0.0;
                    let mut h = 1;
                    while (h as f32) * f0 < 3800.0 {
                        let f = h as f32 * f0;
                        let g: f32 = formants
                            .iter()
                            .map(|&(fc, w)| w * (-((f - fc * shift) / 250.0).powi(2)).exp())
                            .sum::<f32>()
                            + 0.05;
                        v += g * (2.0 * PI * f * tt).sin();
                        h += 1;
                    }
                    out[a + k] += amp * env * v * 0.5;
                }
                last_end = t + dur;
                t += dur + rng.range(0.04, 0.1);
            }
            t = last_end + rng.range(0.4, 0.8);
        }
        (out, gaps)
    }

    #[test]
    fn audio_silence_is_silent() {
        let t = analyze_samples(&vec![0.0; SR as usize * 4], 1, SR);
        assert_eq!(t.len(), 80);
        assert_eq!(t.level_db(10), -127.5);
        let s = summarize(&t);
        assert!(s.silent);
        assert!(!s.has_speech);
        assert!(s.dropouts.is_empty());
        assert_eq!(s.wind_share, 0.0);
        assert!(pauses(&t, 0.25).is_empty());
        assert!(issues_in(&t, 0.0, 4.0).is_empty());
    }

    #[test]
    fn audio_levels_and_bands() {
        // A full-scale sine is about -3 dBFS, one at 0.1 about -23.
        let t = analyze_samples(&sine(1000.0, 0.1, 1.0), 1, SR);
        assert!((t.level_db(10) + 23.0).abs() < 0.6, "{}", t.level_db(10));
        assert!(t.voice_share(10) > 0.8);
        assert!(t.low_share(10) < 0.05);
        let t = analyze_samples(&sine(60.0, 0.5, 1.0), 1, SR);
        assert!(t.low_share(10) > 0.9, "{}", t.low_share(10));
        assert!(t.voice_share(10) < 0.1);
        assert_eq!((t.time_of(0) * 1000.0).round(), 25.0);
        assert_eq!((t.time_of(19) * 1000.0).round(), 975.0);
        // Chunked pushes give the same table as one push, even mid-frame.
        let st = interleave(&sine(440.0, 0.3, 1.0), &sine(90.0, 0.3, 1.0));
        let whole = analyze_samples(&st, 2, SR);
        let mut m = AudioMeter::new(2, SR);
        for c in st.chunks(1001) {
            m.push(c);
        }
        assert_eq!(m.finish(), whole);
        // An odd sample rate still gives 20 windows a second.
        let t = analyze_samples(&vec![0.1; 22050 * 3], 1, 22050);
        assert_eq!(t.len(), 60);
    }

    #[test]
    fn audio_steady_tone_is_not_speech() {
        let t = analyze_samples(&sine(440.0, 0.3, 6.0), 1, SR);
        let s = summarize(&t);
        assert!(!s.has_speech, "{s:?}");
        assert!(s.talk_share < 0.02);
        assert!(!s.silent);
        assert!(s.dropouts.is_empty());
        assert_eq!(s.clipped_events, 0);
        assert!(issues_in(&t, 0.0, 6.0).is_empty());
        // A tone that starts after silence is still not talking.
        let mut v = vec![0.0; SR as usize * 2];
        v.extend(sine(300.0, 0.3, 4.0));
        assert!(!summarize(&analyze_samples(&v, 1, SR)).has_speech);
        // Steady noise is not talking either.
        let mut rng = Rng(11);
        let v: Vec<f32> = (0..SR as usize * 6).map(|_| rng.noise() * 0.2).collect();
        let s = summarize(&analyze_samples(&v, 1, SR));
        assert!(!s.has_speech && s.talk_share < 0.05, "{s:?}");
        assert_eq!(s.wind_share, 0.0);
    }

    #[test]
    #[ignore]
    fn audio_meter_speed() {
        let v = interleave(&sine(440.0, 0.3, 60.0), &sine(220.0, 0.3, 60.0));
        let started = std::time::Instant::now();
        let t = analyze_samples(&v, 2, SR);
        let took = started.elapsed().as_secs_f64();
        let started = std::time::Instant::now();
        let _ = (summarize(&t), pauses(&t, 0.25), issues_in(&t, 0.0, 30.0));
        eprintln!("60 s of 48 kHz stereo measured in {took:.3} s, judged in {:.4} s", started.elapsed().as_secs_f64());
    }

    #[test]
    fn audio_speech_and_pauses_are_found() {
        for seed in [7u64, 99, 12345] {
            let (v, gaps) = speech(seed, 20.0);
            let t = analyze_samples(&v, 1, SR);
            let s = summarize(&t);
            assert!(s.has_speech, "seed {seed}: {s:?}");
            assert!(s.talk_share > 0.5, "seed {seed}: {s:?}");
            assert!(s.dropouts.is_empty(), "seed {seed}: {s:?}");
            assert!(issues_in(&t, 0.0, 20.0).is_empty());
            let found = pauses(&t, 0.3);
            for &(a, b) in &gaps {
                assert!(
                    found.iter().any(|&(fa, fb)| (fa - a).abs() <= 0.1 && (fb - b).abs() <= 0.1),
                    "seed {seed}: pause {a:.2}..{b:.2} not in {found:?}"
                );
            }
            // Leading and trailing quiet, plus one per gap, and nothing else.
            assert_eq!(found.len(), gaps.len() + 2, "seed {seed}: {gaps:?} vs {found:?}");
            assert!(found[0].0 == 0.0 && (found[0].1 - 0.6).abs() <= 0.1);
            // Inside a phrase is mostly talking, inside a pause is not.
            let (a, b) = gaps[0];
            assert!(talk_share_in(&t, a + 0.05, b - 0.05) < 0.1);
            assert!(talk_share_in(&t, 0.65, gaps[0].0 - 0.05) > 0.9);
            // The same speech in stereo.
            let st = analyze_samples(&interleave(&v, &v), 2, SR);
            assert!(summarize(&st).has_speech);
        }
    }

    /// Speech like ffmpeg's flite voices: most of each syllable's energy in
    /// its fundamental (160 to 220 Hz) and weak formants, with short hissy
    /// consonants between some syllables, in phrases separated by silence.
    fn fundamental_heavy_speech(seed: u64, secs: f32) -> Vec<f32> {
        let mut rng = Rng(seed);
        let n = (secs * SR as f32) as usize;
        let mut out: Vec<f32> = (0..n).map(|_| rng.noise() * 0.0005).collect();
        let mut hiss = Biquad::highpass(3000.0, SR as f64, 0.7);
        let mut t = 0.5f32;
        while t < secs - 1.0 {
            let phrase_end = (t + rng.range(1.5, 3.0)).min(secs - 0.5);
            while t < phrase_end - 0.1 {
                let dur = rng.range(0.12, 0.22);
                let f0 = rng.range(160.0, 220.0);
                let amp = rng.range(0.08, 0.2);
                let a = (t * SR as f32) as usize;
                let len = (dur * SR as f32) as usize;
                for k in 0..len.min(n - a) {
                    let tt = k as f32 / SR as f32;
                    let env = 0.5 - 0.5 * (2.0 * PI * k as f32 / len as f32).cos();
                    let mut v = (2.0 * PI * f0 * tt).sin();
                    for h in 2..12 {
                        let f = h as f32 * f0;
                        v += 0.4 / h as f32 * (1.0 + 2.0 * (-((f - 700.0) / 300.0).powi(2)).exp()) * (2.0 * PI * f * tt).sin();
                    }
                    out[a + k] += amp * env * v * 0.5;
                }
                t += dur + rng.range(0.03, 0.08);
                if rng.next() < 0.5 {
                    // A consonant: 60 to 100 ms of hiss.
                    let len = (rng.range(0.06, 0.1) * SR as f32) as usize;
                    let a = (t * SR as f32) as usize;
                    for k in 0..len.min(n - a) {
                        out[a + k] += 0.05 * hiss.run(rng.noise() as f64) as f32;
                    }
                    t += len as f32 / SR as f32 + 0.03;
                }
            }
            t += rng.range(0.6, 1.0);
        }
        out
    }

    #[test]
    fn audio_fundamental_heavy_speech_is_talking_and_a_pulsing_note_is_not() {
        for seed in [3u64, 21, 777] {
            let v = fundamental_heavy_speech(seed, 20.0);
            let t = analyze_samples(&interleave(&v, &v), 2, SR);
            let loud: Vec<usize> = (0..t.len()).filter(|&i| t.level_db(i) > -40.0).collect();
            let voice = loud.iter().map(|&i| t.voice_share(i)).sum::<f32>() / loud.len() as f32;
            let s = summarize(&t);
            println!("audio fundamental heavy speech seed {seed}: voice band share {voice:.2}, talking {:.2}", s.talk_share);
            // Below the 40% the plain voice band test needs, as flite's voices are.
            assert!(voice < 0.4, "seed {seed}: {voice}");
            // Phrases of 1.5 to 3 s with pauses of 0.6 to 1 s: about half is talking.
            assert!(s.has_speech && s.talk_share > 0.3, "seed {seed}: {s:?}");
            assert!(issues_in(&t, 0.0, 20.0).is_empty(), "seed {seed}: {:?}", issues_in(&t, 0.0, 20.0));
        }
        // A low note plucked three times a second: the level dips like
        // syllables and the fundamental dominates (under 40% in the voice
        // band, like the speech above), but the balance hardly changes.
        let n = 10 * SR as usize;
        let note: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / SR as f32;
                let env = (-6.0 * (t * 3.0).fract()).exp();
                let f0 = [165.0, 185.0, 196.0][((t * 3.0) as usize / 2) % 3];
                0.3 * env * ((2.0 * PI * f0 * t).sin() + 0.3 * (4.0 * PI * f0 * t).sin() + 0.2 * (6.0 * PI * f0 * t).sin())
            })
            .collect();
        let s = summarize(&analyze_samples(&note, 1, SR));
        println!("audio plucked low note: talking {:.2}", s.talk_share);
        assert!(!s.has_speech && s.talk_share < 0.05, "{s:?}");
    }

    #[test]
    fn audio_wind_is_flagged_and_bass_is_not() {
        let secs = 8.0;
        let l = wind_channel(1, secs);
        let r = wind_channel(2, secs);
        let t = analyze_samples(&interleave(&l, &r), 2, SR);
        let s = summarize(&t);
        assert!(s.wind_share > 0.6, "{s:?}");
        assert!(!s.has_speech);
        assert!(issues_in(&t, 1.0, 5.0).contains(&AudioIssue::Wind));
        let mean_corr: f32 = (0..t.len()).map(|i| t.low_correlation(i)).sum::<f32>() / t.len() as f32;
        assert!(mean_corr.abs() < 0.3, "{mean_corr}");

        // Mono wind relies on the gusts; the quiet spells between them, where
        // the level barely moves, are not called wind.
        let t = analyze_samples(&l, 1, SR);
        assert!(summarize(&t).wind_share > 0.35, "{:?}", summarize(&t));

        // A bass tone reaching both channels, slowly swelling, is not wind.
        let n = (secs * SR as f32) as usize;
        let bass: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f32 / SR as f32;
                0.3 * (1.0 + 0.2 * (2.0 * PI * 0.3 * t).sin()) * (2.0 * PI * 60.0 * t).sin()
            })
            .collect();
        let t = analyze_samples(&interleave(&bass, &bass), 2, SR);
        let s = summarize(&t);
        assert_eq!(s.wind_share, 0.0, "{s:?}");
        assert!(issues_in(&t, 0.0, secs as f64).is_empty());
        assert!(t.low_correlation(40) > 0.95);
        let t = analyze_samples(&bass, 1, SR);
        assert_eq!(summarize(&t).wind_share, 0.0);
    }

    #[test]
    fn audio_wind_needs_clear_evidence_and_only_strong_wind_is_bad() {
        let secs = 12.0;
        let (l, r) = (wind_channel(3, secs), wind_channel(4, secs));
        let scaled = |v: &[f32], k: f32| v.iter().map(|x| x * k).collect::<Vec<f32>>();
        let low_average = |t: &AudioTable| (0..t.len()).map(|i| t.low_share(i)).sum::<f32>() / t.len() as f32;

        // Loud gusts: wind, and bad enough to set a select aside.
        let t = analyze_samples(&interleave(&l, &r), 2, SR);
        let s = summarize(&t);
        println!("audio loud wind: share {:.2}, median {:.1} dBFS, low share {:.2}", s.wind_share, s.median_db, low_average(&t));
        assert!(s.wind_share > 0.6, "{s:?}");
        assert!(issues_in(&t, 2.0, 10.0).contains(&AudioIssue::Wind));
        assert!(bad_sound_in(&t, 2.0, 10.0));

        // The same gusts 30 dB down, a light breeze: noted, not set aside.
        let t = analyze_samples(&interleave(&scaled(&l, 0.03), &scaled(&r, 0.03)), 2, SR);
        let s = summarize(&t);
        println!("audio light breeze: share {:.2}, median {:.1} dBFS", s.wind_share, s.median_db);
        assert!(issues_in(&t, 2.0, 10.0).contains(&AudioIssue::Wind), "{s:?}");
        assert!(!bad_sound_in(&t, 2.0, 10.0));

        // A breeze under broadband sound (surf, leaves) that keeps a good
        // part of the energy above 150 Hz: borderline, so not called wind.
        let mut rng = Rng(9);
        let mix = |w: &[f32], rng: &mut Rng| w.iter().map(|x| x * 0.05 + rng.noise() * 0.005).collect::<Vec<f32>>();
        let (ml, mr) = (mix(&l, &mut rng), mix(&r, &mut rng));
        let t = analyze_samples(&interleave(&ml, &mr), 2, SR);
        let s = summarize(&t);
        println!("audio breeze under surf: share {:.2}, low share {:.2}", s.wind_share, low_average(&t));
        assert!(low_average(&t) > 0.55 && low_average(&t) < 0.75, "{}", low_average(&t));
        assert!(s.wind_share < 0.1, "{s:?}");
        assert!(issues_in(&t, 0.0, secs as f64).is_empty());

        // Clipping and dropouts are always bad.
        let clipped: Vec<f32> = sine(440.0, 1.6, 2.0).into_iter().map(|x| x.clamp(-1.0, 1.0)).collect();
        assert!(bad_sound_in(&analyze_samples(&clipped, 1, SR), 0.0, 2.0));
        let mut v = sine(440.0, 0.3, 6.0);
        v[3 * SR as usize..(3.3 * SR as f32) as usize].iter_mut().for_each(|x| *x = 0.0);
        let t = analyze_samples(&v, 1, SR);
        assert!(bad_sound_in(&t, 2.0, 4.0));
        assert!(!bad_sound_in(&t, 4.0, 6.0));
    }

    #[test]
    fn audio_clipping_is_counted() {
        let clipped: Vec<f32> = sine(440.0, 1.6, 2.0).into_iter().map(|x| x.clamp(-1.0, 1.0)).collect();
        let t = analyze_samples(&clipped, 1, SR);
        let s = summarize(&t);
        // Two flat tops per cycle.
        assert!((s.clipped_events as i64 - 1760).abs() < 10, "{}", s.clipped_events);
        assert_eq!(issues_in(&t, 0.5, 0.7), vec![AudioIssue::Clipped]);
        // Clipping on one channel of two is still counted.
        let quiet = sine(440.0, 0.2, 2.0);
        let t = analyze_samples(&interleave(&quiet, &clipped), 2, SR);
        assert!(summarize(&t).clipped_events > 1700);
        // Two samples at full scale are not an event, three are.
        let mut v = vec![0.0f32; 4800];
        v[100] = 1.0;
        v[101] = -1.0;
        v[3000..3003].copy_from_slice(&[1.0, 1.0, 1.0]);
        let t = analyze_samples(&v, 1, SR);
        assert_eq!(t.clipped.iter().map(|&c| c as u32).sum::<u32>(), 1);
        assert_eq!(t.clipped_in(0), 0);
        assert_eq!(t.clipped_in(1), 1);
        assert_eq!(summarize(&analyze_samples(&sine(440.0, 0.95, 2.0), 1, SR)).clipped_events, 0);
    }

    #[test]
    fn audio_dropout_in_the_middle_is_found() {
        let mut v = sine(440.0, 0.3, 6.0);
        let a = (3.0 * SR as f32) as usize;
        let b = (3.3 * SR as f32) as usize;
        v[a..b].iter_mut().for_each(|x| *x = 0.0);
        let t = analyze_samples(&v, 1, SR);
        let s = summarize(&t);
        assert_eq!(s.dropouts.len(), 1, "{s:?}");
        let (da, db) = s.dropouts[0];
        assert!((da - 3.0).abs() <= 0.06 && (db - 3.3).abs() <= 0.06, "{da} {db}");
        assert_eq!(issues_in(&t, 2.0, 4.0), vec![AudioIssue::Dropout]);
        assert!(issues_in(&t, 4.0, 6.0).is_empty());
        assert!(!s.has_speech);

        // Silence at the very start or end is not a dropout.
        let mut v = vec![0.0; SR as usize / 5];
        v.extend(sine(440.0, 0.3, 5.0));
        v.extend(vec![0.0; SR as usize / 2]);
        assert!(summarize(&analyze_samples(&v, 1, SR)).dropouts.is_empty());

        // A drop to faint hiss far below the noise floor counts too.
        let mut rng = Rng(5);
        let mut v: Vec<f32> = (0..SR as usize * 6).map(|_| rng.noise() * 0.2).collect();
        let a = (2.0 * SR as f32) as usize;
        let b = (2.5 * SR as f32) as usize;
        v[a..b].iter_mut().for_each(|x| *x *= 0.002);
        let d = dropouts(&analyze_samples(&v, 1, SR));
        assert_eq!(d.len(), 1, "{d:?}");
        assert!((d[0].0 - 2.0).abs() <= 0.06 && (d[0].1 - 2.5).abs() <= 0.06, "{d:?}");
    }

    #[test]
    fn audio_table_round_trips_through_json() {
        let (v, _) = speech(3, 5.0);
        let t = analyze_samples(&interleave(&v, &v), 2, SR);
        assert_eq!(t.channels, 2);
        assert_eq!(t.sample_rate, SR);
        assert_eq!(t.step, AUDIO_STEP);
        let json = serde_json::to_string(&t).unwrap();
        let back: AudioTable = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
        // About 5 to 15 bytes of JSON per window.
        assert!(json.len() < t.len() * 20, "{}", json.len());
        let s = summarize(&t);
        let back: AudioSummary = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        assert_eq!(serde_json::to_string(&AudioIssue::Dropout).unwrap(), "\"dropout\"");
        assert_eq!(AudioIssue::Wind.words(), "Wind noise");
    }
}
