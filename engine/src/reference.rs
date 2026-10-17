//! Fingerprinting a finished video, so its shots can be traced back to the
//! source clips they were cut from (see `matching`).
//!
//! The finished video is decoded the same way as source footage: a few frames
//! per second, downsized to about 480 pixels wide, in grayscale. Black bars
//! from letterboxing or pillarboxing are trimmed before hashing, so a 2.39:1
//! export compares against the matching band of the 16:9 source. Frames that
//! are nearly one flat colour (black gaps, fades, plain title cards) carry no
//! picture to match and are marked unusable.

use crate::analyze::{analysis_size, AnalyzeError};
use crate::ffmpeg;
use crate::fingerprint::{dhash, Region};
use crate::probe::{friendly_ffmpeg_error, probe};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

/// Samples per second taken from a finished video.
pub const REFERENCE_SAMPLE_RATE: f64 = 6.0;

/// Rows or columns darker than this, on average, can be a black bar.
const BAR_MEAN: f32 = 24.0;
/// A bar is nearly uniform: its standard deviation stays under this.
const BAR_STD: f32 = 5.0;
/// A frame whose (trimmed) picture varies less than this is treated as flat.
const FLAT_STD: f32 = 6.0;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ReferencePrint {
    pub path: String,
    pub name: String,
    pub duration: f64,
    /// Sample times in seconds from the start of the video.
    pub t: Vec<f32>,
    /// Difference hash of each sample's picture with bars trimmed.
    pub hash: Vec<u64>,
    /// False for flat frames (black, fades, plain cards) that must not be matched.
    pub usable: Vec<bool>,
    /// Samples per second actually used.
    pub sample_rate: f64,
}

fn line_stats(vals: impl Iterator<Item = u8>) -> (f32, f32) {
    let mut n = 0f64;
    let mut s = 0f64;
    let mut s2 = 0f64;
    for v in vals {
        let v = v as f64;
        n += 1.0;
        s += v;
        s2 += v * v;
    }
    if n == 0.0 {
        return (0.0, 0.0);
    }
    let m = s / n;
    (m as f32, (s2 / n - m * m).max(0.0).sqrt() as f32)
}

fn is_bar((mean, std): (f32, f32)) -> bool {
    mean < BAR_MEAN && std < BAR_STD
}

/// The picture area inside any letterbox or pillarbox bars.
///
/// Bars are only trimmed when they come in a matching pair (top and bottom, or
/// left and right) of similar size and leave at least a third of the picture,
/// so a dark sky or a dark floor in an ordinary frame is never cut away.
pub fn trim_bars(gray: &[u8], w: usize, h: usize) -> Region {
    let row = |y: usize| line_stats(gray[y * w..(y + 1) * w].iter().copied());
    let col = |x: usize| line_stats((0..h).map(|y| gray[y * w + x]));
    let mut top = 0;
    while top < h / 2 && is_bar(row(top)) {
        top += 1;
    }
    let mut bottom = 0;
    while bottom < h / 2 && is_bar(row(h - 1 - bottom)) {
        bottom += 1;
    }
    let mut left = 0;
    while left < w / 2 && is_bar(col(left)) {
        left += 1;
    }
    let mut right = 0;
    while right < w / 2 && is_bar(col(w - 1 - right)) {
        right += 1;
    }
    let paired = |a: usize, b: usize, total: usize| -> bool {
        let min = a.min(b);
        let max = a.max(b);
        // At least 2% of the picture each side, within a few pixels of each other.
        min * 50 >= total && max - min <= (total / 40).max(3) && total - a - b >= total / 3
    };
    let (mut y0, mut y1) = (0, h);
    if paired(top, bottom, h) {
        y0 = top;
        y1 = h - bottom;
    }
    let (mut x0, mut x1) = (0, w);
    if paired(left, right, w) {
        x0 = left;
        x1 = w - right;
    }
    (x0, y0, x1, y1)
}

/// True when the region is nearly one flat value.
pub fn is_flat(gray: &[u8], w: usize, r: Region) -> bool {
    let (x0, y0, x1, y1) = r;
    let it = (y0..y1).flat_map(|y| gray[y * w + x0..y * w + x1].iter().copied());
    line_stats(it).1 < FLAT_STD
}

/// A hash bit is decisive when neighbouring cells differ by at least this much luma.
const DECISIVE_STEP: f32 = 3.0;
/// Fewest decisive bits (of 64) for a frame to be worth matching.
pub const MIN_DECISIVE_BITS: u32 = 24;

/// How many of the 64 hash comparisons in `r` are clear-cut rather than a
/// coin toss between two nearly equal cells. A plain title card with one box
/// on it has few; such frames would match any flat picture.
pub fn decisive_bits(gray: &[u8], w: usize, r: Region) -> u32 {
    let (x0, y0, x1, y1) = r;
    let (rw, rh) = ((x1 - x0).max(1), (y1 - y0).max(1));
    let mut cells = [0f32; 72];
    for row in 0..8 {
        let ya = y0 + row * rh / 8;
        let yb = (y0 + (row + 1) * rh / 8).max(ya + 1).min(y1.max(ya + 1));
        for c in 0..9 {
            let xa = x0 + c * rw / 9;
            let xb = (x0 + (c + 1) * rw / 9).max(xa + 1).min(x1.max(xa + 1));
            let mut sum = 0u64;
            let mut n = 0u64;
            for y in ya..yb {
                for &v in &gray[y * w + xa..y * w + xb] {
                    sum += v as u64;
                    n += 1;
                }
            }
            cells[row * 9 + c] = if n > 0 { sum as f32 / n as f32 } else { 0.0 };
        }
    }
    let mut k = 0;
    for row in 0..8 {
        for c in 0..8 {
            if (cells[row * 9 + c] - cells[row * 9 + c + 1]).abs() >= DECISIVE_STEP {
                k += 1;
            }
        }
    }
    k
}

/// Hash one finished-video frame: (hash, usable).
pub fn reference_hash(gray: &[u8], w: usize, h: usize) -> (u64, bool) {
    let r = trim_bars(gray, w, h);
    if is_flat(gray, w, r) || decisive_bits(gray, w, r) < MIN_DECISIVE_BITS {
        return (0, false);
    }
    (dhash(gray, w, h, r), true)
}

/// Decode a finished video and fingerprint it.
pub fn fingerprint_reference(
    path: &Path,
    sample_rate: f64,
    cancel: &AtomicBool,
) -> Result<ReferencePrint, AnalyzeError> {
    let info = probe(path).map_err(|e| match e {
        crate::probe::ProbeError::ToolMissing(s) => AnalyzeError::ToolMissing(s),
        other => AnalyzeError::Decode(other.to_string()),
    })?;
    let (w, h) = analysis_size(&info, 480);
    let r = if sample_rate > 0.0 { sample_rate } else { REFERENCE_SAMPLE_RATE };
    let select = format!("select='isnan(prev_selected_t)+gt(floor(t*{r}),floor(prev_selected_t*{r}))'");
    let filter = format!("{select},scale={w}:{h}:flags=area,format=gray,showinfo");
    let mut cmd = ffmpeg::command("ffmpeg");
    cmd.args(["-hide_banner", "-nostdin", "-nostats", "-loglevel", "info"]);
    cmd.arg("-i").arg(&info.path);
    cmd.args(["-map", "0:v:0", "-an", "-sn", "-dn", "-vf", &filter]);
    cmd.args(["-fps_mode", "passthrough", "-f", "rawvideo", "-pix_fmt", "gray", "pipe:1"]);
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| AnalyzeError::ToolMissing(e.to_string()))?;

    let stderr = child.stderr.take().expect("stderr piped");
    let (pts_tx, pts_rx) = std::sync::mpsc::channel::<f64>();
    let err_thread = std::thread::spawn(move || {
        use std::io::BufRead;
        let mut last_err = String::new();
        for line in std::io::BufReader::new(stderr).split(b'\n').map_while(Result::ok) {
            let line = String::from_utf8_lossy(&line);
            if let Some(i) = line.find("pts_time:") {
                let rest = &line[i + 9..];
                let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
                if let Ok(v) = rest[..end].parse::<f64>() {
                    let _ = pts_tx.send(v);
                }
            } else if !line.contains("Parsed_showinfo") && !line.starts_with(' ') {
                let l = line.to_ascii_lowercase();
                if l.contains("error") || l.contains("invalid") || l.contains("no such") {
                    last_err = line.trim().to_string();
                }
            }
        }
        last_err
    });

    let mut stdout = child.stdout.take().expect("stdout piped");
    let (wu, hu) = (w as usize, h as usize);
    let mut buf = vec![0u8; wu * hu];
    let mut out = ReferencePrint {
        path: info.path.clone(),
        name: info.name.clone(),
        duration: info.duration,
        t: Vec::new(),
        hash: Vec::new(),
        usable: Vec::new(),
        sample_rate: r,
    };
    let mut last_t: Option<f64> = None;
    let mut cancelled = false;
    loop {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        if !read_full(&mut stdout, &mut buf).unwrap_or(false) {
            break;
        }
        let expected = last_t.map(|t| t + 1.0 / r).unwrap_or(0.0);
        let t = match pts_rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(pts) => (pts - info.start_time).max(0.0),
            Err(_) => expected,
        };
        last_t = Some(t);
        let (hash, usable) = reference_hash(&buf, wu, hu);
        out.t.push(t as f32);
        out.hash.push(hash);
        out.usable.push(usable);
    }
    if cancelled {
        let _ = child.kill();
    }
    let status = child.wait();
    let err_line = err_thread.join().unwrap_or_default();
    if cancelled {
        return Err(AnalyzeError::Cancelled);
    }
    if out.t.is_empty() {
        let ok = status.map(|s| s.success()).unwrap_or(false);
        let detail = if ok { "No frames came out of the decoder.".to_string() } else { friendly_ffmpeg_error(&err_line) };
        return Err(AnalyzeError::Decode(detail));
    }
    Ok(out)
}

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

    fn scene(w: usize, h: usize) -> Vec<u8> {
        (0..w * h)
            .map(|i| {
                let (x, y) = ((i % w) as f32, (i / w) as f32);
                (128.0 + 60.0 * (x / 11.0).sin() + 50.0 * (y / 7.0).cos()) as u8
            })
            .collect()
    }

    #[test]
    fn letterbox_bars_are_trimmed() {
        let (w, h) = (320, 180);
        let mut g = scene(w, h);
        for y in 0..23 {
            for x in 0..w {
                g[y * w + x] = 0;
                g[(h - 1 - y) * w + x] = 1;
            }
        }
        assert_eq!(trim_bars(&g, w, h), (0, 23, w, h - 23));
    }

    #[test]
    fn a_dark_sky_alone_is_not_a_bar() {
        let (w, h) = (320, 180);
        let mut g = scene(w, h);
        for v in g.iter_mut().take(w * 40) {
            *v = 3;
        }
        assert_eq!(trim_bars(&g, w, h), (0, 0, w, h));
    }

    #[test]
    fn black_and_plain_frames_are_unusable() {
        let (w, h) = (320, 180);
        assert!(!reference_hash(&vec![0u8; w * h], w, h).1);
        assert!(!reference_hash(&vec![90u8; w * h], w, h).1);
        // A plain card with one bright box on it.
        let mut card = vec![60u8; w * h];
        for y in 80..100 {
            for x in 60..260 {
                card[y * w + x] = 230;
            }
        }
        assert!(!reference_hash(&card, w, h).1);
        assert!(reference_hash(&scene(w, h), w, h).1);
    }
}
