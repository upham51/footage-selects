//! Turning raw measurements into three scores from 0 (bad) to 1 (good).
//!
//! Scoring is cheap and runs from cached measurements, so it can change
//! between versions without decoding footage again.

use crate::analyze::{ClipAnalysis, FrameTable};
use serde::{Deserialize, Serialize};

/// log10 of Laplacian variance at which focus starts to earn credit, and where it is full.
pub const FOCUS_LOG_LOW: f32 = 1.2;
pub const FOCUS_LOG_HIGH: f32 = 2.3;
/// Weight of the absolute sharpness measure; the rest is relative to the clip's sharpest frames.
pub const FOCUS_ABSOLUTE_WEIGHT: f32 = 0.65;

/// Frame-to-frame jitter, as a share of picture width, where shake starts to cost and where it is total.
pub const SHAKE_LOW: f32 = 0.003;
pub const SHAKE_HIGH: f32 = 0.02;
/// Movement per sample, as a share of width, treated as a whip pan.
pub const SPEED_LOW: f32 = 0.12;
pub const SPEED_HIGH: f32 = 0.35;

/// Linear ramp from 0 at `a` to 1 at `b`, clamped.
pub fn ramp(x: f32, a: f32, b: f32) -> f32 {
    if b == a {
        return if x >= b { 1.0 } else { 0.0 };
    }
    ((x - a) / (b - a)).clamp(0.0, 1.0)
}

/// Scores for every sample of one clip.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ScoreSeries {
    /// Sample times in seconds from the clip start.
    pub t: Vec<f32>,
    pub focus: Vec<f32>,
    pub exposure: Vec<f32>,
    pub stability: Vec<f32>,
}

impl ScoreSeries {
    pub fn len(&self) -> usize {
        self.t.len()
    }
    pub fn is_empty(&self) -> bool {
        self.t.is_empty()
    }
}

fn percentile(values: &[f32], p: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut v: Vec<f32> = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((v.len() - 1) as f32 * p).round() as usize;
    v[idx]
}

/// Focus from Laplacian variance: mostly an absolute measure, partly relative
/// to this clip's sharpest moments so low-detail but sharp shots are not
/// written off, while focus pulls within a clip still show.
pub fn focus_scores(lap: &[f32]) -> Vec<f32> {
    let p90 = percentile(lap, 0.9).max(1.0);
    lap.iter()
        .map(|&l| {
            let abs = ramp((l + 1.0).log10(), FOCUS_LOG_LOW, FOCUS_LOG_HIGH);
            let rel = (l / p90).clamp(0.0, 1.0);
            FOCUS_ABSOLUTE_WEIGHT * abs + (1.0 - FOCUS_ABSOLUTE_WEIGHT) * rel
        })
        .collect()
}

/// The four parts of the exposure score, each a penalty from 0 (none) to 1 (full).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExposurePenalties {
    pub highlights: f32,
    pub shadows: f32,
    pub flat: f32,
    pub level: f32,
}

pub fn exposure_penalties(mean: f32, lo: f32, hi: f32, p2: f32, p98: f32) -> ExposurePenalties {
    let spread = p98 - p2;
    ExposurePenalties {
        // A few specular highlights are fine; large blown areas are not.
        highlights: ramp(hi, 0.02, 0.25),
        // Deep shadows are more forgiving than blown highlights.
        shadows: ramp(lo, 0.05, 0.45),
        // Flat contrast: the middle 96% of pixels spans a small range.
        flat: 1.0 - ramp(spread, 25.0, 100.0),
        // Overall level far from the middle.
        level: ramp(40.0 - mean, 0.0, 30.0).max(ramp(mean - 215.0, 0.0, 30.0)),
    }
}

pub fn exposure_score(p: ExposurePenalties) -> f32 {
    (1.0 - p.highlights) * (1.0 - p.shadows) * (1.0 - p.flat) * (1.0 - p.level)
}

pub fn exposure_scores(f: &FrameTable) -> Vec<f32> {
    (0..f.len())
        .map(|i| exposure_score(exposure_penalties(f.mean[i], f.lo[i], f.hi[i], f.p2[i], f.p98[i])))
        .collect()
}

/// Stability separates smooth camera movement from shake. Each sample's
/// motion is compared with the average of its neighbours: a steady pan or
/// push has nearly constant motion (low jitter), handheld shake does not.
/// Very fast movement (whip pans) costs a little on top.
pub fn stability_scores(f: &FrameTable, width: u32, sample_rate: f64) -> Vec<f32> {
    let n = f.len();
    let nominal = (1.0 / sample_rate.max(0.01)) as f32;
    let w = width.max(1) as f32;
    let mut out = vec![1.0f32; n];
    if n == 0 {
        return out;
    }
    // Motion of sample i is measured from i-1 to i. Sample 0 has none, so borrow sample 1.
    // Samples are not exactly evenly spaced (frames are skipped at the decoder,
    // footage may be variable frame rate), so motion is scaled to the nominal interval.
    let vel = |i: usize| -> (f32, f32) {
        let j = if i == 0 && n > 1 { 1 } else { i };
        if j == 0 || f.motion_ok[j] == 0 {
            return (0.0, 0.0);
        }
        let dt = f.t[j] - f.t[j - 1];
        let scale = if dt > 1e-4 { nominal / dt } else { 1.0 };
        (f.dx[j] * scale, f.dy[j] * scale)
    };
    for (i, o) in out.iter_mut().enumerate() {
        let (vx, vy) = vel(i);
        let (px, py) = vel(i.saturating_sub(1));
        let (nx, ny) = vel((i + 1).min(n - 1));
        let jx = vx - 0.5 * (px + nx);
        let jy = vy - 0.5 * (py + ny);
        let jitter = (jx * jx + jy * jy).sqrt() / w;
        let speed = (vx * vx + vy * vy).sqrt() / w;
        let shake_pen = ramp(jitter, SHAKE_LOW, SHAKE_HIGH);
        let speed_pen = ramp(speed, SPEED_LOW, SPEED_HIGH) * 0.5;
        *o = (1.0 - shake_pen) * (1.0 - speed_pen);
    }
    out
}

pub fn score_frames(frames: &FrameTable, sample_rate: f64, width: u32) -> ScoreSeries {
    ScoreSeries {
        t: frames.t.clone(),
        focus: focus_scores(&frames.lap),
        exposure: exposure_scores(frames),
        stability: stability_scores(frames, width, sample_rate),
    }
}

pub fn score_clip(a: &ClipAnalysis) -> ScoreSeries {
    score_frames(&a.frames, a.sample_rate, a.width)
}

/// Average of a slice, or 0 for an empty one.
pub fn mean(v: &[f32]) -> f32 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f32>() / v.len() as f32
    }
}
