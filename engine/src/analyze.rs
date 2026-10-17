//! Decoding sampled frames with ffmpeg and measuring them.
//!
//! ffmpeg decodes the clip, keeps the first frame in each sample interval
//! (`select`), downsizes to about 480 pixels wide and converts to 8-bit
//! grayscale, then streams raw pixels to us over a pipe. `showinfo` reports
//! each kept frame's real timestamp on stderr, so variable frame rate footage
//! and skipped frames are timed exactly. Non-reference frames are skipped at
//! the decoder, which cuts decode work by about 40% on long-GOP footage, and
//! hardware decoding is requested where available with a software fallback.

use crate::ffmpeg;
use crate::metrics::{measure_frame, FrameMetrics, MotionFrame, LADDER_LEN};
use crate::probe::{friendly_ffmpeg_error, ClipInfo};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Bump when measurements change meaning, so cached results are redone.
pub const ANALYSIS_VERSION: u32 = 3;
pub const DEFAULT_ANALYSIS_WIDTH: u32 = 480;
pub const DEFAULT_SAMPLE_RATE: f64 = 4.0;

#[derive(Clone, Debug)]
pub struct AnalyzeOptions {
    pub sample_rate: f64,
    pub width: u32,
    pub hardware_decode: bool,
}

impl Default for AnalyzeOptions {
    fn default() -> Self {
        AnalyzeOptions {
            sample_rate: DEFAULT_SAMPLE_RATE,
            width: DEFAULT_ANALYSIS_WIDTH,
            hardware_decode: true,
        }
    }
}

/// Per-frame measurements stored column by column, which keeps project files small.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct FrameTable {
    /// Seconds from the start of the clip.
    pub t: Vec<f32>,
    pub lap: Vec<f32>,
    pub mean: Vec<f32>,
    pub lo: Vec<f32>,
    pub hi: Vec<f32>,
    pub p2: Vec<f32>,
    pub p98: Vec<f32>,
    pub dx: Vec<f32>,
    pub dy: Vec<f32>,
    pub motion_ok: Vec<u8>,
    /// Luma at the shares in `metrics::LADDER`.
    #[serde(default)]
    pub ladder: Vec<[u8; LADDER_LEN]>,
    #[serde(default)]
    pub vmax: Vec<u8>,
    /// Share of pixels within one level of the brightest.
    #[serde(default)]
    pub top: Vec<f32>,
    /// Picture fingerprints: whole frame, 2.39:1 centre and 9:16 centre.
    #[serde(default)]
    pub fp: Vec<[u64; 3]>,
}

fn round4(v: f32) -> f32 {
    if v.abs() >= 1000.0 {
        (v * 10.0).round() / 10.0
    } else {
        (v * 10000.0).round() / 10000.0
    }
}

impl FrameTable {
    pub fn len(&self) -> usize {
        self.lap.len()
    }
    pub fn is_empty(&self) -> bool {
        self.lap.is_empty()
    }
    pub fn push(&mut self, t: f64, m: &FrameMetrics) {
        self.t.push(((t * 10000.0).round() / 10000.0) as f32);
        self.lap.push(round4(m.lap));
        self.mean.push(round4(m.mean));
        self.lo.push(round4(m.lo));
        self.hi.push(round4(m.hi));
        self.p2.push(m.p2);
        self.p98.push(m.p98);
        self.dx.push(round4(m.dx));
        self.dy.push(round4(m.dy));
        self.motion_ok.push(m.motion_ok as u8);
        self.ladder.push(m.ladder);
        self.vmax.push(m.vmax);
        self.top.push(round4(m.top));
        self.fp.push(m.fp);
    }
    /// True when the exposure ladder and fingerprints were measured (analysis version 3 and later).
    pub fn has_ladder(&self) -> bool {
        !self.lap.is_empty() && self.ladder.len() == self.lap.len()
    }
    pub fn get(&self, i: usize) -> FrameMetrics {
        FrameMetrics {
            lap: self.lap[i],
            mean: self.mean[i],
            lo: self.lo[i],
            hi: self.hi[i],
            p2: self.p2[i],
            p98: self.p98[i],
            dx: self.dx[i],
            dy: self.dy[i],
            motion_ok: self.motion_ok[i] != 0,
            ladder: self.ladder.get(i).copied().unwrap_or_default(),
            vmax: self.vmax.get(i).copied().unwrap_or(0),
            top: self.top.get(i).copied().unwrap_or(0.0),
            fp: self.fp.get(i).copied().unwrap_or_default(),
        }
    }
    pub fn extend_from(&mut self, other: &FrameTable) {
        self.t.extend_from_slice(&other.t);
        self.lap.extend_from_slice(&other.lap);
        self.mean.extend_from_slice(&other.mean);
        self.lo.extend_from_slice(&other.lo);
        self.hi.extend_from_slice(&other.hi);
        self.p2.extend_from_slice(&other.p2);
        self.p98.extend_from_slice(&other.p98);
        self.dx.extend_from_slice(&other.dx);
        self.dy.extend_from_slice(&other.dy);
        self.motion_ok.extend_from_slice(&other.motion_ok);
        self.ladder.extend_from_slice(&other.ladder);
        self.vmax.extend_from_slice(&other.vmax);
        self.top.extend_from_slice(&other.top);
        self.fp.extend_from_slice(&other.fp);
    }
}

/// The cached result of analysing one clip.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ClipAnalysis {
    pub version: u32,
    pub sample_rate: f64,
    /// Size of the analysed grayscale picture.
    pub width: u32,
    pub height: u32,
    pub frames: FrameTable,
    /// False while analysis is still running or if it was stopped part way.
    pub complete: bool,
    /// "hardware" or "software".
    pub decoder: String,
    /// Seconds of wall time spent.
    pub elapsed: f64,
}

impl ClipAnalysis {
    pub fn empty(sample_rate: f64, width: u32, height: u32) -> Self {
        ClipAnalysis {
            version: ANALYSIS_VERSION,
            sample_rate,
            width,
            height,
            frames: FrameTable::default(),
            complete: false,
            decoder: String::new(),
            elapsed: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AnalyzeError {
    Cancelled,
    ToolMissing(String),
    Decode(String),
}

impl std::fmt::Display for AnalyzeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnalyzeError::Cancelled => write!(f, "Analysis was stopped before this clip finished."),
            AnalyzeError::ToolMissing(e) => write!(
                f,
                "The video decoder that ships with the app could not start ({e}). Reinstalling the app usually fixes this."
            ),
            AnalyzeError::Decode(e) => write!(f, "The picture in this clip could not be decoded. {e}"),
        }
    }
}

/// Size of the analysis picture: about `target` wide, never upscaled, even dimensions.
pub fn analysis_size(info: &ClipInfo, target: u32) -> (u32, u32) {
    let dw = info.width.max(2) as f64;
    let dh = info.height.max(2) as f64;
    let w = (target as f64).min(dw);
    let w = ((w / 2.0).round() * 2.0).max(16.0);
    let h = ((w * dh / dw / 2.0).round() * 2.0).max(16.0);
    (w as u32, h as u32)
}

/// Ways to decode, tried in order until one produces frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decoder {
    /// macOS: VideoToolbox decodes, unwanted frames are dropped and the rest
    /// are downsized on the GPU, so only small frames are copied to memory.
    GpuScaled,
    /// Hardware decode with every frame copied back to memory.
    Hardware,
    Software,
}

impl Decoder {
    pub fn label(self) -> &'static str {
        match self {
            Decoder::GpuScaled => DECODED_GPU,
            Decoder::Hardware => DECODED_HARDWARE,
            Decoder::Software => DECODED_SOFTWARE,
        }
    }
}

/// What `ClipAnalysis::decoder` says, so the clip view can tell people how
/// their footage was read.
pub const DECODED_GPU: &str = "hardware, downsized on the GPU";
pub const DECODED_HARDWARE: &str = "hardware";
pub const DECODED_SOFTWARE: &str = "software";
pub const DECODED_SOFTWARE_FALLBACK: &str = "software, because hardware decoding was not available for this clip";

/// The pixel format VideoToolbox hands decoded frames back in, so the GPU
/// path can download them. This mirrors ffmpeg's own choice
/// (`videotoolbox_best_pixel_format` in libavcodec/videotoolbox.c): 4:2:2 and
/// 4:4:4 footage and anything over 10 bits come back in their own formats,
/// not the 4:2:0 ones. None for formats with no such mapping (alpha, RGB,
/// unusual chroma), which skip straight to the next decoder.
pub fn videotoolbox_download_format(pix_fmt: &str) -> Option<&'static str> {
    if pix_fmt.is_empty() {
        // Projects saved before the format was recorded: the common case.
        return Some("nv12");
    }
    let f = pix_fmt.strip_prefix("yuvj").map(|r| format!("yuv{r}")).unwrap_or_else(|| pix_fmt.to_string());
    let rest = f.strip_prefix("yuv")?;
    let (chroma, tail) = rest.split_at(rest.len().min(3));
    let tail = tail.strip_prefix('p')?;
    let depth: u32 = match tail.trim_end_matches("le").trim_end_matches("be") {
        "" => 8,
        d => d.parse().ok()?,
    };
    Some(match (chroma, depth) {
        ("444", d) if d > 10 => "p416le",
        ("420" | "422", d) if d > 10 => "p216le",
        ("444", d) if d > 8 => "p410le",
        ("444", _) => "nv24",
        ("422", d) if d > 8 => "p210le",
        ("422", _) => "nv16",
        ("420", d) if d > 8 => "p010le",
        ("420", _) => "nv12",
        _ => return None,
    })
}

/// The filter that turns a frame the way ffmpeg's automatic rotation would,
/// for the GPU path, which has to switch that off and rotate small frames itself.
pub fn rotation_filter(rotation: i32) -> Option<&'static str> {
    match rotation {
        90 => Some("transpose=clock"),
        180 => Some("hflip,vflip"),
        270 => Some("transpose=cclock"),
        _ => None,
    }
}

/// Formats hardware decoders copy frames back to memory in. Software decoders
/// give planar formats (yuv420p and so on) instead, which is how the app
/// tells whether hardware decoding really happened.
fn is_hardware_download_format(f: &str) -> bool {
    matches!(f, "nv12" | "nv16" | "nv24" | "p010le" | "p210le" | "p410le" | "p216le" | "p416le" | "videotoolbox_vld" | "d3d11" | "dxva2_vld")
}

/// The label for what actually ran, given the pixel format ffmpeg reported
/// for frames entering the filters.
pub fn decoder_used(decoder: Decoder, clip_pix_fmt: &str, filter_input: Option<&str>) -> &'static str {
    match decoder {
        Decoder::GpuScaled => DECODED_GPU,
        Decoder::Software => DECODED_SOFTWARE,
        Decoder::Hardware => match filter_input {
            Some(f) if f != clip_pix_fmt && is_hardware_download_format(f) => DECODED_HARDWARE,
            _ => DECODED_SOFTWARE_FALLBACK,
        },
    }
}

fn hwaccel_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "videotoolbox"
    } else {
        "auto"
    }
}

/// The decoders worth trying for a clip, best first.
pub fn decoder_order(info: &ClipInfo, hardware: bool) -> Vec<Decoder> {
    let mut v = Vec::new();
    if hardware {
        if cfg!(target_os = "macos") && videotoolbox_download_format(&info.pix_fmt).is_some() {
            v.push(Decoder::GpuScaled);
        }
        v.push(Decoder::Hardware);
    }
    v.push(Decoder::Software);
    v
}

/// Progress reported while a clip decodes.
pub enum Progress<'a> {
    /// New measurements since the last report, in order.
    Frames(&'a FrameTable),
    /// Hardware decoding failed part way; discard frames reported so far.
    Restart,
}

/// Decode and measure one clip.
///
/// `on_progress` is called every few frames with the new measurements since the
/// last call, so callers can show partial results while decoding continues.
pub fn analyze_clip(
    info: &ClipInfo,
    opts: &AnalyzeOptions,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(Progress),
) -> Result<ClipAnalysis, AnalyzeError> {
    let started = std::time::Instant::now();
    let attempts = decoder_order(info, opts.hardware_decode);
    let mut last_err = AnalyzeError::Decode("No frames came out of the decoder.".into());
    let last = *attempts.last().unwrap();
    for dec in attempts {
        let mut delivered = 0usize;
        let result = run_decoder(info, opts, dec, cancel, &mut |t: &FrameTable| {
            delivered += t.len();
            on_progress(Progress::Frames(t))
        });
        match result {
            Ok(mut a) => {
                if a.frames.is_empty() && dec != last {
                    if delivered > 0 {
                        on_progress(Progress::Restart);
                    }
                    last_err = AnalyzeError::Decode("No frames came out of the hardware decoder.".into());
                    continue;
                }
                a.elapsed = started.elapsed().as_secs_f64();
                return Ok(a);
            }
            Err(AnalyzeError::Cancelled) => return Err(AnalyzeError::Cancelled),
            Err(e @ AnalyzeError::ToolMissing(_)) => return Err(e),
            Err(e) => {
                // A software retry starts from scratch; tell the caller to discard partial frames.
                if delivered > 0 {
                    on_progress(Progress::Restart);
                }
                last_err = e;
            }
        }
    }
    Err(last_err)
}

/// The ffmpeg command that decodes `info` with `decoder` and writes small
/// gray frames to stdout, one `showinfo` line per frame on stderr.
pub fn decoder_command(info: &ClipInfo, opts: &AnalyzeOptions, decoder: Decoder) -> std::process::Command {
    let (w, h) = analysis_size(info, opts.width);
    let r = opts.sample_rate;
    let select = format!("select='isnan(prev_selected_t)+gt(floor(t*{r}),floor(prev_selected_t*{r}))'");
    let filter = match decoder {
        Decoder::GpuScaled => gpu_filter(info, &select, w, h, "scale_vt=w={w}:h={h},hwdownload,format={dl}"),
        _ => format!("{select},scale={w}:{h}:flags=area,format=gray,showinfo"),
    };
    let mut cmd = ffmpeg::command("ffmpeg");
    // Verbose so ffmpeg reports the pixel format frames arrive in, which says
    // whether hardware decoding really ran (see `decoder_used`).
    cmd.args(["-hide_banner", "-nostdin", "-nostats", "-loglevel", "verbose"]);
    match decoder {
        Decoder::GpuScaled => {
            cmd.args(["-hwaccel", "videotoolbox", "-hwaccel_output_format", "videotoolbox_vld"]);
            // ffmpeg's automatic rotation cannot run on GPU frames; small frames are turned after download.
            cmd.arg("-noautorotate");
        }
        Decoder::Hardware => {
            cmd.args(["-hwaccel", hwaccel_name()]);
        }
        Decoder::Software => {}
    }
    cmd.args(["-skip_frame", "noref"]);
    cmd.arg("-i").arg(&info.path);
    cmd.args(["-map", "0:v:0", "-an", "-sn", "-dn", "-vf", &filter]);
    cmd.args(["-fps_mode", "passthrough", "-f", "rawvideo", "-pix_fmt", "gray", "pipe:1"]);
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd
}

/// The GPU path's filters: pick frames, downsize on the GPU (`scale`, with
/// `{w}`, `{h}` and `{dl}` filled in), convert to gray and rotate the small
/// frame. Rotated clips are downsized to the unrotated size first.
pub fn gpu_filter(info: &ClipInfo, select: &str, w: u32, h: u32, scale: &str) -> String {
    let (sw, sh) = if info.rotation == 90 || info.rotation == 270 { (h, w) } else { (w, h) };
    let dl = videotoolbox_download_format(&info.pix_fmt).unwrap_or("nv12");
    let scale = scale.replace("{w}", &sw.to_string()).replace("{h}", &sh.to_string()).replace("{dl}", dl);
    match rotation_filter(info.rotation) {
        Some(rot) => format!("{select},{scale},format=gray,{rot},showinfo"),
        None => format!("{select},{scale},format=gray,showinfo"),
    }
}

/// Only the ffmpeg side of analysis: every sampled frame's pixels, and the
/// decoder that produced them. Used to time decoding on its own.
pub fn decoded_frames(info: &ClipInfo, opts: &AnalyzeOptions) -> Result<(Vec<Vec<u8>>, Decoder), AnalyzeError> {
    let (w, h) = analysis_size(info, opts.width);
    for dec in decoder_order(info, opts.hardware_decode) {
        let mut child = decoder_command(info, opts, dec)
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| AnalyzeError::ToolMissing(e.to_string()))?;
        let mut stdout = child.stdout.take().expect("stdout piped");
        let mut frames = Vec::new();
        let mut buf = vec![0u8; (w * h) as usize];
        while let Ok(true) = read_full(&mut stdout, &mut buf) {
            frames.push(buf.clone());
        }
        let _ = child.wait();
        if !frames.is_empty() {
            return Ok((frames, dec));
        }
    }
    Err(AnalyzeError::Decode("No frames came out of the decoder.".into()))
}

fn run_decoder(
    info: &ClipInfo,
    opts: &AnalyzeOptions,
    decoder: Decoder,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(&FrameTable),
) -> Result<ClipAnalysis, AnalyzeError> {
    let (w, h) = analysis_size(info, opts.width);
    let r = opts.sample_rate;
    let mut child = decoder_command(info, opts, decoder)
        .spawn()
        .map_err(|e| AnalyzeError::ToolMissing(e.to_string()))?;

    // stderr carries one showinfo line per kept frame plus any errors.
    let stderr_lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let stderr = child.stderr.take().expect("stderr piped");
    let lines = stderr_lines.clone();
    let filter_input: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let input_fmt = filter_input.clone();
    let (pts_tx, pts_rx) = std::sync::mpsc::channel::<f64>();
    let err_thread = std::thread::spawn(move || {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stderr);
        for line in reader.split(b'\n').map_while(Result::ok) {
            let line = String::from_utf8_lossy(&line);
            if let Some(i) = line.find("pts_time:") {
                let rest = &line[i + 9..];
                let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
                if let Ok(v) = rest[..end].parse::<f64>() {
                    let _ = pts_tx.send(v);
                }
            } else if line.contains("input from stream") && line.contains("pixfmt:") {
                if let Some(f) = line.split("pixfmt:").nth(1).and_then(|r| r.split_whitespace().next()) {
                    *input_fmt.lock().unwrap() = Some(f.to_string());
                }
            } else if !line.contains("Parsed_showinfo") && !line.starts_with(' ') {
                let mut l = lines.lock().unwrap();
                l.push(line.trim().to_string());
                if l.len() > 40 {
                    l.remove(0);
                }
            }
        }
    });

    let mut stdout = child.stdout.take().expect("stdout piped");
    let frame_bytes = (w * h) as usize;
    let mut buf = vec![0u8; frame_bytes];
    let mut prev: Option<MotionFrame> = None;
    let mut analysis = ClipAnalysis::empty(opts.sample_rate, w, h);
    analysis.decoder = decoder.label().into();
    let decoder_kind = decoder;
    let mut pending = FrameTable::default();
    let mut cancelled = false;
    let mut last_t: Option<f64> = None;
    loop {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        match read_full(&mut stdout, &mut buf) {
            Ok(true) => {}
            Ok(false) => break,
            Err(_) => break,
        }
        let expected = last_t.map(|t| t + 1.0 / r).unwrap_or(0.0);
        let t = match pts_rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(pts) => (pts - info.start_time).max(0.0),
            Err(_) => expected,
        };
        last_t = Some(t);
        let (m, proj) = measure_frame(&buf, w as usize, h as usize, prev.as_ref());
        prev = Some(proj);
        pending.push(t, &m);
        if pending.len() >= 8 {
            on_progress(&pending);
            analysis.frames.extend_from(&pending);
            pending = FrameTable::default();
        }
    }
    if cancelled {
        let _ = child.kill();
    }
    let status = child.wait();
    let _ = err_thread.join();
    if cancelled {
        return Err(AnalyzeError::Cancelled);
    }
    if !pending.is_empty() {
        on_progress(&pending);
        analysis.frames.extend_from(&pending);
    }
    let ok = status.map(|s| s.success()).unwrap_or(false);
    if !ok && analysis.frames.is_empty() {
        let lines = stderr_lines.lock().unwrap();
        let line = lines
            .iter()
            .rev()
            .find(|l| {
                let l = l.to_ascii_lowercase();
                l.contains("error") || l.contains("invalid") || l.contains("no such")
            })
            .or(lines.last())
            .cloned()
            .unwrap_or_default();
        return Err(AnalyzeError::Decode(friendly_ffmpeg_error(&line)));
    }
    // A decoder that exits with an error after producing most of the clip
    // (common with truncated camera files) still gives usable scores.
    analysis.complete = true;
    analysis.decoder = decoder_used(decoder_kind, &info.pix_fmt, filter_input.lock().unwrap().as_deref()).into();
    Ok(analysis)
}

/// Read exactly `buf.len()` bytes. Ok(false) on a clean end of stream.
fn read_full(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<bool> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = r.read(&mut buf[filled..])?;
        if n == 0 {
            return Ok(false);
        }
        filled += n;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_download_format_follows_ffmpeg() {
        let cases = [
            ("yuv420p", Some("nv12")),
            ("yuvj420p", Some("nv12")),
            ("yuv420p10le", Some("p010le")),
            ("yuv422p", Some("nv16")),
            ("yuv422p10le", Some("p210le")),
            ("yuv444p", Some("nv24")),
            ("yuv444p10le", Some("p410le")),
            ("yuv420p12le", Some("p216le")),
            ("yuv422p12le", Some("p216le")),
            ("yuv444p12le", Some("p416le")),
            ("", Some("nv12")),
            ("yuva420p", None),
            ("yuva444p12le", None),
            ("gbrp10le", None),
            ("rgb24", None),
            ("yuv411p", None),
            ("gray", None),
        ];
        for (f, want) in cases {
            assert_eq!(videotoolbox_download_format(f), want, "{f}");
        }
    }

    #[test]
    fn decoder_label_says_what_really_ran() {
        assert_eq!(decoder_used(Decoder::Hardware, "yuv420p", Some("nv12")), DECODED_HARDWARE);
        assert_eq!(decoder_used(Decoder::Hardware, "yuv420p10le", Some("p010le")), DECODED_HARDWARE);
        assert_eq!(decoder_used(Decoder::Hardware, "yuv420p", Some("yuv420p")), DECODED_SOFTWARE_FALLBACK);
        assert_eq!(decoder_used(Decoder::Hardware, "yuv422p10le", Some("yuv422p10le")), DECODED_SOFTWARE_FALLBACK);
        assert_eq!(decoder_used(Decoder::Hardware, "yuv420p", None), DECODED_SOFTWARE_FALLBACK);
        assert_eq!(decoder_used(Decoder::Software, "yuv420p", Some("yuv420p")), DECODED_SOFTWARE);
        assert_eq!(decoder_used(Decoder::GpuScaled, "yuv420p", Some("videotoolbox_vld")), DECODED_GPU);
    }
}
