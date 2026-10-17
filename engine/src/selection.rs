//! Turning scores into selects. This runs on cached scores only, takes
//! milliseconds for hours of footage, and can be re-run on every slider move.
//!
//! Steps:
//! 1. Smooth each score with a centred moving average over `smoothing` seconds.
//! 2. A sample passes when all three smoothed scores meet their thresholds.
//! 3. Each run of passing samples becomes a candidate segment.
//! 4. Candidates shorter than `min_length` are dropped as too short.
//!    Candidates longer than `max_length` are split into equal parts.
//! 5. Each candidate is scored by the mean of its three smoothed scores. Only
//!    the best `max_per_clip` are kept; the rest are dropped as over the limit.

use crate::scores::{mean, ScoreSeries};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SelectionSettings {
    /// Minimum smoothed focus score, 0 to 1.
    pub focus_min: f32,
    pub exposure_min: f32,
    pub stability_min: f32,
    /// Moving-average window in seconds. 0 turns smoothing off.
    pub smoothing: f32,
    /// Shortest select in seconds.
    pub min_length: f32,
    /// Longest select in seconds. Longer good stretches are split.
    pub max_length: f32,
    /// Most selects kept per clip. 0 means no limit.
    pub max_per_clip: u32,
}

impl Default for SelectionSettings {
    fn default() -> Self {
        SelectionSettings {
            focus_min: 0.5,
            exposure_min: 0.5,
            stability_min: 0.5,
            smoothing: 1.0,
            min_length: 2.0,
            max_length: 10.0,
            max_per_clip: 3,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SegmentStatus {
    Kept,
    /// Good enough, but shorter than the minimum length.
    TooShort,
    /// Good enough, but the clip already has its maximum number of better selects.
    OverLimit,
    /// At least one score is under its threshold.
    BelowThreshold,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Segment {
    /// Seconds from the clip start.
    pub start: f64,
    pub end: f64,
    pub status: SegmentStatus,
    /// Mean smoothed scores over the segment.
    pub focus: f32,
    pub exposure: f32,
    pub stability: f32,
    /// Mean of the three scores; used to rank selects within a clip.
    pub score: f32,
    /// For `BelowThreshold`: which scores fell short, most frequent first.
    pub failed: Vec<String>,
}

impl Segment {
    pub fn duration(&self) -> f64 {
        self.end - self.start
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ClipSelection {
    /// Smoothed scores, as drawn in the clip detail graph.
    pub smoothed: ScoreSeries,
    /// Every stretch of the clip, kept or dropped, in time order.
    pub segments: Vec<Segment>,
}

impl ClipSelection {
    pub fn kept(&self) -> impl Iterator<Item = &Segment> {
        self.segments.iter().filter(|s| s.status == SegmentStatus::Kept)
    }
    pub fn kept_seconds(&self) -> f64 {
        self.kept().map(|s| s.duration()).sum()
    }
}

/// Centred moving average over a time window, robust to uneven sample spacing.
pub fn smooth(values: &[f32], t: &[f32], window: f32) -> Vec<f32> {
    let n = values.len();
    if window <= 0.0 || n < 2 {
        return values.to_vec();
    }
    let half = window / 2.0;
    let mut out = Vec::with_capacity(n);
    let (mut lo, mut hi) = (0usize, 0usize);
    let mut sum = 0f64;
    for i in 0..n {
        while hi < n && t[hi] <= t[i] + half + 1e-6 {
            sum += values[hi] as f64;
            hi += 1;
        }
        while lo < i && t[lo] < t[i] - half - 1e-6 {
            sum -= values[lo] as f64;
            lo += 1;
        }
        out.push((sum / (hi - lo) as f64) as f32);
    }
    out
}

/// Time span covered by each sample: from halfway to the previous sample to
/// halfway to the next. The first starts at 0 and the last ends at `end_time`.
fn sample_bounds(t: &[f32], end_time: f64) -> Vec<f64> {
    let n = t.len();
    let mut b = Vec::with_capacity(n + 1);
    b.push(0.0);
    for i in 1..n {
        b.push(0.5 * (t[i - 1] as f64 + t[i] as f64));
    }
    let last = t[n - 1] as f64;
    b.push(end_time.max(last));
    b
}

struct Run {
    first: usize,
    last: usize,
    start: f64,
    end: f64,
}

fn segment_means(sm: &ScoreSeries, bounds: &[f64], start: f64, end: f64) -> (f32, f32, f32) {
    let mut idx = Vec::new();
    for i in 0..sm.len() {
        let mid = 0.5 * (bounds[i] + bounds[i + 1]);
        if mid >= start && mid < end {
            idx.push(i);
        }
    }
    if idx.is_empty() {
        // A sliver between samples: use the nearest sample.
        let mid = 0.5 * (start + end);
        let i = (0..sm.len())
            .min_by(|&a, &b| {
                (sm.t[a] as f64 - mid)
                    .abs()
                    .partial_cmp(&(sm.t[b] as f64 - mid).abs())
                    .unwrap()
            })
            .unwrap_or(0);
        idx.push(i);
    }
    let pick = |v: &Vec<f32>| mean(&idx.iter().map(|&i| v[i]).collect::<Vec<_>>());
    (pick(&sm.focus), pick(&sm.exposure), pick(&sm.stability))
}

fn make_segment(sm: &ScoreSeries, bounds: &[f64], start: f64, end: f64, status: SegmentStatus) -> Segment {
    let (f, e, s) = segment_means(sm, bounds, start, end);
    Segment {
        start,
        end,
        status,
        focus: f,
        exposure: e,
        stability: s,
        score: (f + e + s) / 3.0,
        failed: Vec::new(),
    }
}

/// Split a long good stretch into equal parts no longer than `max`, never
/// shorter than `min`. If both cannot hold, use parts of exactly `max`
/// centred in the stretch and trim the ends.
fn split(start: f64, end: f64, min: f64, max: f64) -> Vec<(f64, f64)> {
    let len = end - start;
    if max <= 0.0 || len <= max + 1e-9 {
        return vec![(start, end)];
    }
    let n = (len / max - 1e-9).ceil().max(1.0) as usize;
    let part = len / n as f64;
    if part + 1e-9 >= min {
        return (0..n)
            .map(|k| (start + part * k as f64, start + part * (k + 1) as f64))
            .collect();
    }
    let n = (len / max).floor().max(1.0) as usize;
    let used = max * n as f64;
    let offset = start + (len - used) / 2.0;
    (0..n)
        .map(|k| (offset + max * k as f64, offset + max * (k + 1) as f64))
        .collect()
}

/// Run selection for one clip. `end_time` is the clip duration, or for a clip
/// still being analysed, the time up to which it has been measured.
pub fn select_clip(scores: &ScoreSeries, end_time: f64, s: &SelectionSettings) -> ClipSelection {
    let n = scores.len();
    if n == 0 {
        return ClipSelection::default();
    }
    let sm = ScoreSeries {
        t: scores.t.clone(),
        focus: smooth(&scores.focus, &scores.t, s.smoothing),
        exposure: smooth(&scores.exposure, &scores.t, s.smoothing),
        stability: smooth(&scores.stability, &scores.t, s.smoothing),
    };
    let bounds = sample_bounds(&sm.t, end_time);
    let pass: Vec<bool> = (0..n)
        .map(|i| {
            sm.focus[i] >= s.focus_min
                && sm.exposure[i] >= s.exposure_min
                && sm.stability[i] >= s.stability_min
        })
        .collect();

    // Runs of equal pass/fail.
    let mut runs: Vec<(bool, Run)> = Vec::new();
    let mut i = 0;
    while i < n {
        let p = pass[i];
        let mut j = i;
        while j + 1 < n && pass[j + 1] == p {
            j += 1;
        }
        runs.push((
            p,
            Run {
                first: i,
                last: j,
                start: bounds[i],
                end: bounds[j + 1],
            },
        ));
        i = j + 1;
    }

    let min = s.min_length.max(0.0) as f64;
    let max = (s.max_length as f64).max(min);
    let mut segments: Vec<Segment> = Vec::new();
    for (p, run) in &runs {
        if !*p {
            let mut counts = [("focus", 0usize), ("exposure", 0usize), ("stability", 0usize)];
            for k in run.first..=run.last {
                if sm.focus[k] < s.focus_min {
                    counts[0].1 += 1;
                }
                if sm.exposure[k] < s.exposure_min {
                    counts[1].1 += 1;
                }
                if sm.stability[k] < s.stability_min {
                    counts[2].1 += 1;
                }
            }
            counts.sort_by(|a, b| b.1.cmp(&a.1));
            let mut seg = make_segment(&sm, &bounds, run.start, run.end, SegmentStatus::BelowThreshold);
            seg.failed = counts
                .iter()
                .filter(|c| c.1 > 0)
                .map(|c| c.0.to_string())
                .collect();
            segments.push(seg);
            continue;
        }
        if run.end - run.start + 1e-9 < min {
            segments.push(make_segment(&sm, &bounds, run.start, run.end, SegmentStatus::TooShort));
            continue;
        }
        let parts = split(run.start, run.end, min, max);
        // Trimmed ends from splitting are reported as too short.
        let first_start = parts.first().map(|p| p.0).unwrap_or(run.start);
        let last_end = parts.last().map(|p| p.1).unwrap_or(run.end);
        if first_start - run.start > 1e-6 {
            segments.push(make_segment(&sm, &bounds, run.start, first_start, SegmentStatus::TooShort));
        }
        for (a, b) in parts {
            segments.push(make_segment(&sm, &bounds, a, b, SegmentStatus::Kept));
        }
        if run.end - last_end > 1e-6 {
            segments.push(make_segment(&sm, &bounds, last_end, run.end, SegmentStatus::TooShort));
        }
    }

    if s.max_per_clip > 0 {
        let mut ranked: Vec<usize> = segments
            .iter()
            .enumerate()
            .filter(|(_, g)| g.status == SegmentStatus::Kept)
            .map(|(i, _)| i)
            .collect();
        // Highest score first; earlier segment wins a tie so results are stable.
        ranked.sort_by(|&a, &b| {
            segments[b]
                .score
                .partial_cmp(&segments[a].score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.cmp(&b))
        });
        for &i in ranked.iter().skip(s.max_per_clip as usize) {
            segments[i].status = SegmentStatus::OverLimit;
        }
    }

    ClipSelection {
        smoothed: sm,
        segments,
    }
}

/// Footage in, selects out.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub clips_total: usize,
    pub clips_analyzed: usize,
    pub clips_failed: usize,
    /// Total duration of every readable clip, in seconds.
    pub footage_seconds: f64,
    /// Duration of the analysed clips, in seconds.
    pub analyzed_seconds: f64,
    pub selects_count: usize,
    pub selects_seconds: f64,
}
