//! Turning raw measurements into three scores from 0 (bad) to 1 (good).
//!
//! Scoring is cheap and runs from cached measurements, so it can change
//! between versions without decoding footage again.

use crate::analyze::{ClipAnalysis, FrameTable};
use crate::tone::{log_exposure, ExposureIssue, StopsTable, ToneProfile};
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
    /// Camera movement per second, as a share of picture width. Used to keep
    /// camera moves whole and to cut between them where the camera settles.
    #[serde(default)]
    pub motion: Vec<f32>,
    /// For log footage: middle tones in stops from middle grey, per sample.
    #[serde(default)]
    pub key_stops: Vec<f32>,
    /// Share of the picture clipped, per sample (log footage only).
    #[serde(default)]
    pub clipped: Vec<f32>,
    /// The main exposure problem per sample, as `ExposureIssue::code`.
    #[serde(default)]
    pub issue: Vec<u8>,
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

/// The largest standard exposure penalty, as an issue.
fn standard_issue(p: ExposurePenalties) -> ExposureIssue {
    let list = [
        (p.highlights, ExposureIssue::ClippedHighlights),
        (p.shadows, ExposureIssue::DeepShadows),
        (p.flat, ExposureIssue::Flat),
        (p.level, ExposureIssue::Underexposed),
    ];
    // `level` covers both ends; the caller turns it into overexposed for bright frames.
    list.into_iter()
        .fold((0.05f32, ExposureIssue::None), |b, c| if c.0 > b.0 { c } else { b })
        .1
}

/// How exposure is judged for a clip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToneContext {
    pub profile: ToneProfile,
    /// The file was recorded with full-range levels rather than video levels.
    pub full_range: bool,
}

impl Default for ToneContext {
    fn default() -> Self {
        ToneContext {
            profile: ToneProfile::Standard,
            full_range: false,
        }
    }
}

/// Exposure scores, middle tones in stops, clipped share and main issue per sample.
pub fn exposure_series(f: &FrameTable, tone: ToneContext) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<u8>) {
    let n = f.len();
    let (mut score, mut key, mut clip, mut issue) =
        (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
    if tone.profile.is_log() && f.has_ladder() {
        let table = StopsTable::new(tone.profile, tone.full_range);
        for i in 0..n {
            let r = log_exposure(&f.ladder[i], f.vmax[i], f.top[i], &table);
            score.push(r.score);
            key.push((r.key * 100.0).round() / 100.0);
            clip.push(r.clipped);
            issue.push(r.issue.code());
        }
    } else {
        for i in 0..n {
            let p = exposure_penalties(f.mean[i], f.lo[i], f.hi[i], f.p2[i], f.p98[i]);
            let mut iss = standard_issue(p);
            if iss == ExposureIssue::Underexposed && f.mean[i] > 128.0 {
                iss = ExposureIssue::Overexposed;
            }
            score.push(exposure_score(p));
            issue.push(iss.code());
        }
    }
    (score, key, clip, issue)
}

/// Camera movement per second as a share of picture width, for each sample.
pub fn motion_speeds(f: &FrameTable, width: u32) -> Vec<f32> {
    let n = f.len();
    let w = width.max(1) as f32;
    (0..n)
        .map(|i| {
            let j = if i == 0 && n > 1 { 1 } else { i };
            if j == 0 {
                return 0.0;
            }
            let dt = (f.t[j] - f.t[j - 1]).max(1e-3);
            if f.motion_ok[j] == 0 {
                // Too fast or too featureless to follow: treat a large shift as moving.
                let d = (f.dx[j] * f.dx[j] + f.dy[j] * f.dy[j]).sqrt() / w;
                return if d > 0.02 { d / dt } else { 0.0 };
            }
            ((f.dx[j] * f.dx[j] + f.dy[j] * f.dy[j]).sqrt() / w / dt * 1000.0).round() / 1000.0
        })
        .collect()
}

/// Camera velocity at every sample, in analysis pixels per nominal sample
/// interval, and whether it was measured.
///
/// Motion of sample i is measured from i-1 to i, so sample 0 has none of its
/// own. Samples are not exactly evenly spaced (frames are skipped at the
/// decoder, footage may be variable frame rate), so each shift is scaled to
/// the nominal interval. A sample is unmeasured when its picture was
/// featureless or too noisy to line up. An unmeasured sample with a non-zero
/// shift moved faster than the search can follow; its shift is a lower bound
/// and counts as measured.
pub fn velocities(f: &FrameTable, nominal: f32) -> (Vec<(f32, f32)>, Vec<bool>) {
    let n = f.len();
    let mut v = vec![(0.0f32, 0.0f32); n];
    let mut known = vec![false; n];
    for j in 1..n {
        let too_fast = f.motion_ok[j] == 0 && (f.dx[j] != 0.0 || f.dy[j] != 0.0);
        if f.motion_ok[j] != 0 || too_fast {
            let dt = f.t[j] - f.t[j - 1];
            let scale = if dt > 1e-4 { nominal / dt } else { 1.0 };
            v[j] = (f.dx[j] * scale, f.dy[j] * scale);
            known[j] = true;
        }
    }
    (v, known)
}

/// Stability separates smooth camera movement from shake. Each sample's
/// motion is compared with the average of its neighbours: a steady pan or
/// push has nearly constant motion (low jitter), handheld shake does not.
/// Very fast movement (whip pans) costs a little on top.
///
/// Only measured samples are compared, skipping over unmeasured ones, so a
/// sample that could not be lined up neither adds shake nor hides it. An
/// unmeasured sample takes the average score of the nearest measured samples
/// on each side. A clip with no measured motion at all scores as steady.
pub fn stability_scores(f: &FrameTable, width: u32, sample_rate: f64) -> Vec<f32> {
    let n = f.len();
    let nominal = (1.0 / sample_rate.max(0.01)) as f32;
    let w = width.max(1) as f32;
    let mut out = vec![1.0f32; n];
    if n == 0 {
        return out;
    }
    let (v, known) = velocities(f, nominal);
    let measured: Vec<usize> = (0..n).filter(|&i| known[i]).collect();
    for (k, &i) in measured.iter().enumerate() {
        let (vx, vy) = v[i];
        let (px, py) = v[measured[k.saturating_sub(1)]];
        let (nx, ny) = v[measured[(k + 1).min(measured.len() - 1)]];
        let jx = vx - 0.5 * (px + nx);
        let jy = vy - 0.5 * (py + ny);
        let jitter = (jx * jx + jy * jy).sqrt() / w;
        let speed = (vx * vx + vy * vy).sqrt() / w;
        let shake_pen = ramp(jitter, SHAKE_LOW, SHAKE_HIGH);
        let speed_pen = ramp(speed, SPEED_LOW, SPEED_HIGH) * 0.5;
        out[i] = (1.0 - shake_pen) * (1.0 - speed_pen);
    }
    if measured.is_empty() {
        return out;
    }
    let scored = out.clone();
    for i in 0..n {
        if known[i] {
            continue;
        }
        let before = measured.iter().rev().find(|&&m| m < i).map(|&m| scored[m]);
        let after = measured.iter().find(|&&m| m > i).map(|&m| scored[m]);
        out[i] = match (before, after) {
            (Some(a), Some(b)) => 0.5 * (a + b),
            (Some(a), None) | (None, Some(a)) => a,
            (None, None) => 1.0,
        };
    }
    out
}

pub fn score_frames(frames: &FrameTable, sample_rate: f64, width: u32) -> ScoreSeries {
    score_frames_with(frames, sample_rate, width, ToneContext::default())
}

pub fn score_frames_with(frames: &FrameTable, sample_rate: f64, width: u32, tone: ToneContext) -> ScoreSeries {
    let (exposure, key_stops, clipped, issue) = exposure_series(frames, tone);
    ScoreSeries {
        t: frames.t.clone(),
        focus: focus_scores(&frames.lap),
        exposure,
        stability: stability_scores(frames, width, sample_rate),
        motion: motion_speeds(frames, width),
        key_stops,
        clipped,
        issue,
    }
}

/// Scores judged as standard footage.
pub fn score_clip(a: &ClipAnalysis) -> ScoreSeries {
    score_clip_with(a, ToneContext::default())
}

pub fn score_clip_with(a: &ClipAnalysis, tone: ToneContext) -> ScoreSeries {
    score_frames_with(&a.frames, a.sample_rate, a.width, tone)
}

/// Average of a slice, or 0 for an empty one.
pub fn mean(v: &[f32]) -> f32 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f32>() / v.len() as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(dx: &[f32], ok: &[u8]) -> FrameTable {
        let n = dx.len();
        FrameTable {
            t: (0..n).map(|i| i as f32 * 0.25).collect(),
            lap: vec![100.0; n],
            mean: vec![120.0; n],
            lo: vec![0.0; n],
            hi: vec![0.0; n],
            p2: vec![20.0; n],
            p98: vec![220.0; n],
            dx: dx.to_vec(),
            dy: vec![0.0; n],
            motion_ok: ok.to_vec(),
            ..Default::default()
        }
    }

    #[test]
    fn unmeasured_samples_neither_add_nor_hide_shake() {
        // A steady pan of 4 px a sample with two samples that could not be lined up.
        let pan = table(&[0.0, 4.0, 4.0, 0.0, 4.0, 4.0, 0.0, 4.0, 4.0], &[1, 1, 1, 0, 1, 1, 0, 1, 1]);
        let s = stability_scores(&pan, 480, 4.0);
        assert!(s.iter().all(|&v| v > 0.99), "{s:?}");
        // The same samples read as "not moving" would be shake.
        let misread = table(&[0.0, 4.0, 4.0, 0.0, 4.0, 4.0, 0.0, 4.0, 4.0], &[1; 9]);
        let m = stability_scores(&misread, 480, 4.0);
        assert!(m[3] < 0.8, "{m:?}");
        // Shake around an unmeasured sample still shows.
        let shaky = table(&[0.0, 9.0, -9.0, 0.0, -9.0, 9.0, -9.0], &[1, 1, 1, 0, 1, 1, 1]);
        let k = stability_scores(&shaky, 480, 4.0);
        assert!(k[3] < 0.5, "{k:?}");
    }

    #[test]
    fn a_whip_pan_too_fast_to_follow_is_not_steady() {
        // Unmeasured but with a shift: faster than the search can follow.
        let whip = table(&[0.0, 2.0, 2.0, 160.0, 160.0, 160.0, 2.0, 2.0], &[1, 1, 1, 0, 0, 0, 1, 1]);
        let s = stability_scores(&whip, 480, 4.0);
        assert!(s[3] < 0.5 && s[6] < 0.5, "{s:?}");
        assert!(s[4] < 1.0, "speed costs a little on top: {s:?}");
    }
}
