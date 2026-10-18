//! Turning scores into selects. This runs on cached scores only, takes
//! milliseconds for hours of footage, and can be re-run on every slider move.
//!
//! Steps:
//! 1. Smooth each score with a centred moving average over `smoothing` seconds.
//! 2. A sample passes when all three smoothed scores meet their thresholds.
//! 3. Each run of passing samples becomes a candidate segment.
//! 4. Candidates shorter than `min_length` are dropped as too short.
//!    Candidates longer than `max_length` are split, at moments where the
//!    camera is still when there are any, never in the middle of a camera
//!    move if it can be helped. With `keep_moves_whole`, a single camera move
//!    up to twice `max_length` stays in one piece.
//! 5. Each candidate gets the camera movement it shows (still, a pan, a tilt
//!    or another move). With a camera filter, good stretches are first cut
//!    where the camera starts or stops moving, and candidates whose movement
//!    does not match are set aside.
//! 6. Each candidate is scored by a weighted mean of its three smoothed
//!    scores. Only the best `max_per_clip` are kept; the rest are dropped as
//!    over the limit. The project can set aside repeated takes across clips
//!    before this last step (`project::Project::selections`).

use crate::scores::{mean, ramp, ScoreSeries};
use crate::tone::ExposureIssue;
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
    /// Let one camera move run up to twice the longest select rather than cut it in two.
    pub keep_moves_whole: bool,
    /// Which camera movement a select must show.
    pub camera: CameraFilter,
    /// How much each score counts when selects are ranked against each other,
    /// 0 to 1. Thresholds are not affected.
    pub focus_weight: f32,
    pub exposure_weight: f32,
    pub stability_weight: f32,
    /// Set aside selects that look like a better select from another clip.
    pub set_aside_repeats: bool,
    /// Start and end selects in a pause when someone is talking.
    pub snap_to_pauses: bool,
    /// Set aside selects whose sound has wind noise, clipping or a dropout.
    pub set_aside_sound_problems: bool,
    /// Which light a select must be in (from the sun's height when and where it was recorded).
    pub light: LightFilter,
}

/// Which light a select must be in.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LightFilter {
    #[default]
    Any,
    GoldenHour,
    BlueHour,
    /// Daylight or harsh midday sun.
    Daylight,
    Night,
}

impl LightFilter {
    pub fn allows(self, light: Option<crate::solar::Light>) -> bool {
        use crate::solar::Light;
        match (self, light) {
            (LightFilter::Any, _) => true,
            (_, None) => false,
            (LightFilter::GoldenHour, Some(l)) => l == Light::GoldenHour,
            (LightFilter::BlueHour, Some(l)) => l == Light::BlueHour,
            (LightFilter::Daylight, Some(l)) => matches!(l, Light::Daylight | Light::HarshMidday),
            (LightFilter::Night, Some(l)) => l == Light::Night,
        }
    }
}

/// A call made by hand on part of a clip. It survives slider changes: a kept
/// stretch stays exactly as marked, a dropped one stays out.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    pub start: f64,
    pub end: f64,
    pub call: Call,
    /// 0 to 5 stars, for kept stretches.
    #[serde(default)]
    pub stars: u8,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Call {
    Keep,
    Drop,
}

/// What selection can use besides the scores: the clip's measurements for
/// the picture checks, where someone talks, and calls made by hand.
#[derive(Clone, Copy, Default)]
pub struct ClipExtras<'a> {
    pub frames: Option<&'a crate::analyze::FrameTable>,
    pub sound: Option<&'a crate::analyze::SoundMap>,
    pub audio: Option<&'a crate::audio::AudioTable>,
    pub marks: &'a [Mark],
}

/// Horizon readings under this confidence are ignored.
pub const ROLL_MIN_CONFIDENCE: f32 = 0.35;
/// A horizon this many degrees or more off level is called crooked.
pub const CROOKED_DEGREES: f32 = 1.0;
/// Flicker bands swinging this share of brightness or more are called flicker.
pub const FLICKER_SHARE: f32 = 0.012;
/// Rolling-shutter wobble of this many half-size pixels or more is called jello.
pub const WOBBLE_PIXELS: f32 = 0.35;
/// How far a select's start or end may move to land in a pause, in seconds.
pub const PAUSE_REACH: f64 = 2.0;

/// Which camera movement a select must show.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CameraFilter {
    #[default]
    Any,
    Still,
    /// Any movement: pans, tilts and other moves.
    Moving,
    Pans,
    Tilts,
}

/// The camera movement a segment shows. Movement is measured as sideways and
/// up-and-down shift of the picture, so a push-in or zoom with little
/// sideways movement reads as still.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CameraMove {
    #[default]
    Still,
    PanLeft,
    PanRight,
    TiltUp,
    TiltDown,
    /// Moving, but not mostly one way: diagonal, wandering or orbiting.
    Other,
}

impl CameraMove {
    pub fn words(self) -> &'static str {
        match self {
            CameraMove::Still => "Still",
            CameraMove::PanLeft => "Pan left",
            CameraMove::PanRight => "Pan right",
            CameraMove::TiltUp => "Tilt up",
            CameraMove::TiltDown => "Tilt down",
            CameraMove::Other => "Moving",
        }
    }
}

impl CameraFilter {
    pub fn allows(self, m: CameraMove) -> bool {
        match self {
            CameraFilter::Any => true,
            CameraFilter::Still => m == CameraMove::Still,
            CameraFilter::Moving => m != CameraMove::Still,
            CameraFilter::Pans => matches!(m, CameraMove::PanLeft | CameraMove::PanRight),
            CameraFilter::Tilts => matches!(m, CameraMove::TiltUp | CameraMove::TiltDown),
        }
    }
}

/// Why a good segment was set aside.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AsideReason {
    /// Its camera movement does not match the camera filter.
    Camera,
    /// It looks like a better select from another clip.
    Repeat,
    /// Its light does not match the light filter, or the light is not known.
    Light,
    /// Its sound has wind noise, clipping or a dropout.
    Sound,
    /// Dropped by hand.
    Dropped,
}

/// A select in another clip that a segment looks like.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SimilarTo {
    pub clip: u64,
    pub start: f64,
    pub end: f64,
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
            keep_moves_whole: true,
            camera: CameraFilter::Any,
            focus_weight: 1.0,
            exposure_weight: 1.0,
            stability_weight: 1.0,
            set_aside_repeats: false,
            snap_to_pauses: true,
            set_aside_sound_problems: false,
            light: LightFilter::Any,
        }
    }
}

impl SelectionSettings {
    /// Weighted mean of three scores. Equal weights when all are zero.
    pub fn rank_score(&self, focus: f32, exposure: f32, stability: f32) -> f32 {
        let w = [self.focus_weight.max(0.0), self.exposure_weight.max(0.0), self.stability_weight.max(0.0)];
        let sum = w[0] + w[1] + w[2];
        if sum <= 1e-6 {
            return (focus + exposure + stability) / 3.0;
        }
        (w[0] * focus + w[1] * exposure + w[2] * stability) / sum
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SegmentStatus {
    #[default]
    Kept,
    /// Good enough, but shorter than the minimum length.
    TooShort,
    /// Good enough, but the clip already has its maximum number of better selects.
    OverLimit,
    /// At least one score is under its threshold.
    BelowThreshold,
    /// Good enough, but set aside: see `Segment::aside`.
    SetAside,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Segment {
    /// Seconds from the clip start.
    pub start: f64,
    pub end: f64,
    pub status: SegmentStatus,
    /// Mean smoothed scores over the segment.
    pub focus: f32,
    pub exposure: f32,
    pub stability: f32,
    /// Weighted mean of the three scores; used to rank selects.
    pub score: f32,
    /// For `BelowThreshold`: which scores fell short, most frequent first.
    pub failed: Vec<String>,
    /// The most common exposure problem in the segment, if any.
    #[serde(default)]
    pub exposure_issue: Option<ExposureIssue>,
    /// Log footage: average middle tones in stops from middle grey.
    #[serde(default)]
    pub key_stops: Option<f32>,
    /// The camera is moving for most of the segment.
    #[serde(default)]
    pub camera_move: bool,
    /// Which way the camera moves, if it does.
    #[serde(default)]
    pub camera: CameraMove,
    /// For `SetAside`: why.
    #[serde(default)]
    pub aside: Option<AsideReason>,
    /// A better select in another clip that this one looks like.
    #[serde(default)]
    pub similar_to: Option<SimilarTo>,
    /// Kept by hand: exactly this stretch, whatever the scores and filters say.
    #[serde(default)]
    pub manual: bool,
    /// Stars given by hand, 0 to 5.
    #[serde(default)]
    pub stars: u8,
    /// How far the horizon is off level in degrees (positive: rotated
    /// clockwise), when the picture has clear lines to judge it by.
    #[serde(default)]
    pub roll: Option<f32>,
    /// Light flicker banding, as a share of brightness, when it could be measured.
    #[serde(default)]
    pub flicker: Option<f32>,
    /// Rolling-shutter wobble in half-size pixels, when it could be measured.
    #[serde(default)]
    pub wobble: Option<f32>,
    /// Average chroma of near-neutral parts of the picture (Cb, Cr), for white balance.
    #[serde(default)]
    pub wb: Option<(f32, f32)>,
    /// Share of the segment with someone talking.
    #[serde(default)]
    pub talk: f32,
    /// Ends (0, 1 or 2) that still cut while someone talks.
    #[serde(default)]
    pub speech_cuts: u8,
    /// Ends moved into a pause in the talking.
    #[serde(default)]
    pub snapped: u8,
    /// Sound problems inside the segment.
    #[serde(default)]
    pub sound: Vec<crate::audio::AudioIssue>,
}

impl Segment {
    pub fn crooked(&self) -> bool {
        self.roll.map(|r| r.abs() >= CROOKED_DEGREES).unwrap_or(false)
    }
    pub fn flickers(&self) -> bool {
        self.flicker.map(|f| f >= FLICKER_SHARE).unwrap_or(false)
    }
    pub fn jello(&self) -> bool {
        self.wobble.map(|w| w >= WOBBLE_PIXELS).unwrap_or(false)
    }
    /// Plain sentences about problems a person should know before using it.
    pub fn problem_notes(&self) -> Vec<String> {
        let mut v = Vec::new();
        if let Some(r) = self.roll.filter(|_| self.crooked()) {
            v.push(format!(
                "Horizon tilts down to the {} by {:.1} degrees; rotate {:.1} degrees {} to level it",
                if r > 0.0 { "right" } else { "left" },
                r.abs(),
                r.abs(),
                if r > 0.0 { "counter-clockwise" } else { "clockwise" }
            ));
        }
        if self.flickers() {
            v.push("Light flicker: bands roll through the picture".into());
        }
        if self.jello() {
            v.push("Jello: the picture wobbles, typical of propeller vibration or a loose mount".into());
        }
        for i in &self.sound {
            v.push(i.words().to_string());
        }
        if self.speech_cuts > 0 {
            v.push(if self.speech_cuts == 2 { "Starts and ends mid-sentence".into() } else { "Cuts mid-sentence".into() });
        }
        v
    }

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

fn segment_indices(sm: &ScoreSeries, bounds: &[f64], start: f64, end: f64) -> Vec<usize> {
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
    idx
}

/// Camera speed, as a share of picture width per second, below which the camera counts as still.
pub const STILL_SPEED: f32 = 0.015;
/// Speed from which the camera counts as fully moving.
pub const MOVING_SPEED: f32 = 0.06;

/// 0 when the camera is still at sample `i`, 1 when it is clearly moving.
fn movingness(sm: &ScoreSeries, i: usize) -> f32 {
    sm.motion.get(i).map(|&v| ramp(v, STILL_SPEED, MOVING_SPEED)).unwrap_or(0.0)
}

/// Share of the mean speed that must point one way for a move to count as
/// a pan or a tilt rather than wandering.
const DIRECTED_SHARE: f32 = 0.5;

/// The camera movement over samples `idx`.
fn camera_move_of(sm: &ScoreSeries, idx: &[usize]) -> CameraMove {
    let moving: Vec<usize> = idx.iter().copied().filter(|&i| movingness(sm, i) >= 0.5).collect();
    if moving.len() * 2 < idx.len().max(1) || moving.is_empty() {
        return CameraMove::Still;
    }
    if sm.pan.len() != sm.len() || sm.tilt.len() != sm.len() {
        return CameraMove::Other;
    }
    let n = moving.len() as f32;
    let p = moving.iter().map(|&i| sm.pan[i]).sum::<f32>() / n;
    let q = moving.iter().map(|&i| sm.tilt[i]).sum::<f32>() / n;
    let speed = moving.iter().map(|&i| (sm.pan[i] * sm.pan[i] + sm.tilt[i] * sm.tilt[i]).sqrt()).sum::<f32>() / n;
    if speed <= 1e-6 || (p * p + q * q).sqrt() < DIRECTED_SHARE * speed {
        return CameraMove::Other;
    }
    if p.abs() >= 2.0 * q.abs() {
        if p > 0.0 { CameraMove::PanRight } else { CameraMove::PanLeft }
    } else if q.abs() >= 2.0 * p.abs() {
        if q > 0.0 { CameraMove::TiltUp } else { CameraMove::TiltDown }
    } else {
        CameraMove::Other
    }
}

fn make_segment(sm: &ScoreSeries, bounds: &[f64], start: f64, end: f64, status: SegmentStatus, s: &SelectionSettings) -> Segment {
    let exposure_min = s.exposure_min;
    let idx = segment_indices(sm, bounds, start, end);
    let pick = |v: &Vec<f32>| mean(&idx.iter().map(|&i| v[i]).collect::<Vec<_>>());
    let (f, e) = (pick(&sm.focus), pick(&sm.exposure));
    // Count issues where exposure fell short first; otherwise anywhere.
    let mut counts = [0usize; 6];
    let short: Vec<usize> = idx.iter().copied().filter(|&i| sm.exposure[i] < exposure_min).collect();
    let pool = if short.is_empty() { &idx } else { &short };
    for &i in pool {
        if let Some(&c) = sm.issue.get(i) {
            counts[(c as usize).min(5)] += 1;
        }
    }
    let (best, n) = counts.iter().enumerate().skip(1).max_by_key(|(_, &n)| n).map(|(c, &n)| (c, n)).unwrap_or((0, 0));
    let exposure_issue = (n * 4 >= pool.len().max(1)).then(|| ExposureIssue::from_code(best as u8)).filter(|i| *i != ExposureIssue::None);
    let key_stops = if sm.key_stops.len() == sm.len() && !idx.is_empty() {
        Some((pick(&sm.key_stops) * 10.0).round() / 10.0)
    } else {
        None
    };
    let camera_move = !idx.is_empty() && idx.iter().map(|&i| movingness(sm, i)).sum::<f32>() / idx.len() as f32 >= 0.5;
    let camera = if camera_move { camera_move_of(sm, &idx) } else { CameraMove::Still };
    let st = pick(&sm.stability);
    Segment {
        start,
        end,
        status,
        focus: f,
        exposure: e,
        stability: st,
        score: s.rank_score(f, e, st),
        failed: Vec::new(),
        exposure_issue,
        key_stops,
        camera_move,
        camera: if camera_move && camera == CameraMove::Still { CameraMove::Other } else { camera },
        aside: None,
        similar_to: None,
        manual: false,
        stars: 0,
        roll: None,
        flicker: None,
        wobble: None,
        wb: None,
        talk: 0.0,
        speech_cuts: 0,
        snapped: 0,
        sound: Vec::new(),
    }
}

/// Split a long good run into parts, choosing cut points where the camera is
/// still. Returns None when no split satisfies the length limits.
///
/// Costs: cutting while the camera moves costs a lot, cutting while it is
/// still costs almost nothing; parts much shorter than `max` cost a little
/// (so parts come out even); a part longer than `max` is only allowed when
/// `keep_moves` is on and the part is mostly one camera move, and costs a
/// fixed amount so it only wins over a cut in the middle of the move.
fn split_at_rest(sm: &ScoreSeries, bounds: &[f64], run: &Run, min: f64, max: f64, keep_moves: bool, sound: Option<&crate::analyze::SoundMap>) -> Option<Vec<(f64, f64)>> {
    // Cut positions: run start, every sample boundary inside, run end.
    let mut pos: Vec<f64> = vec![run.start];
    let mut cut_cost: Vec<f32> = vec![0.0];
    for k in run.first + 1..=run.last {
        pos.push(bounds[k]);
        let m = 0.5 * (movingness(sm, k - 1) + movingness(sm, k));
        let speed = 0.5 * (sm.motion.get(k - 1).copied().unwrap_or(0.0) + sm.motion.get(k).copied().unwrap_or(0.0));
        // Cutting while someone talks costs about as much as cutting a camera move.
        let talk = sound.map(|sd| if sd.talking_at(bounds[k]) { 3.0 } else { 0.0 }).unwrap_or(0.0);
        cut_cost.push(4.0 * m + 0.3 * ramp(speed, 0.0, STILL_SPEED) + talk);
    }
    pos.push(run.end);
    cut_cost.push(0.0);
    let n = pos.len();
    // Running sum of movingness, to get a part's average quickly.
    let mut mov_prefix = vec![0f32; n];
    for j in 1..n {
        let i = run.first + j - 1;
        mov_prefix[j] = mov_prefix[j - 1] + movingness(sm, i.min(run.last));
    }
    let long_limit = if keep_moves { 2.0 * max } else { max };
    let mut best = vec![f32::INFINITY; n];
    let mut from = vec![usize::MAX; n];
    best[0] = 0.0;
    for j in 1..n {
        for i in (0..j).rev() {
            let len = pos[j] - pos[i];
            if len > long_limit + 1e-9 {
                break;
            }
            if len + 1e-9 < min || !best[i].is_finite() {
                continue;
            }
            let mut c = best[i] + cut_cost[j];
            if len > max + 1e-9 {
                let share = (mov_prefix[j] - mov_prefix[i]) / (j - i) as f32;
                if share < 0.5 {
                    continue;
                }
                c += 2.0 + ((len - max) / max) as f32;
            } else {
                let short = ((max - len) / max) as f32;
                c += 0.5 * short * short;
            }
            if c < best[j] {
                best[j] = c;
                from[j] = i;
            }
        }
    }
    if !best[n - 1].is_finite() {
        return None;
    }
    let mut parts = Vec::new();
    let mut j = n - 1;
    while j > 0 {
        let i = from[j];
        parts.push((pos[i], pos[j]));
        j = i;
    }
    parts.reverse();
    Some(parts)
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

/// Pieces of a good run shorter than this are joined to a neighbour when a
/// run is cut where the camera starts or stops moving, so a moment of
/// hesitation does not chop a shot up.
const CAMERA_PIECE_MIN: f64 = 1.0;

/// Cut a good run where the camera starts or stops moving. Used only with a
/// camera filter, so a still shot that turns into a pan gives one still
/// candidate and one moving candidate instead of one mixed one.
fn camera_pieces(sm: &ScoreSeries, bounds: &[f64], run: &Run) -> Vec<Run> {
    let moving = |i: usize| movingness(sm, i) >= 0.5;
    let mut pieces: Vec<(bool, Run)> = Vec::new();
    let mut i = run.first;
    while i <= run.last {
        let m = moving(i);
        let mut j = i;
        while j < run.last && moving(j + 1) == m {
            j += 1;
        }
        pieces.push((m, Run { first: i, last: j, start: bounds[i].max(run.start), end: bounds[j + 1].min(run.end) }));
        i = j + 1;
    }
    // Join short pieces to the previous piece (or the next, for the first),
    // then join neighbours that now share a state.
    let mut merged: Vec<(bool, Run)> = Vec::new();
    for (m, r) in pieces {
        let short = r.end - r.start < CAMERA_PIECE_MIN;
        match merged.last_mut() {
            Some(prev) if short || prev.0 == m => {
                prev.1.last = r.last;
                prev.1.end = r.end;
            }
            _ => merged.push((m, r)),
        }
    }
    if merged.len() > 1 && merged[0].1.end - merged[0].1.start < CAMERA_PIECE_MIN {
        let first = merged.remove(0);
        merged[0].1.first = first.1.first;
        merged[0].1.start = first.1.start;
    }
    merged.into_iter().map(|(_, r)| r).collect()
}

/// Turn one good run into kept and too-short segments, splitting it when it
/// is longer than the maximum. With `sound`, ends that would cut through
/// talking move into the nearest pause inside the run when one is close.
fn place_run(sm: &ScoreSeries, bounds: &[f64], run: &Run, s: &SelectionSettings, sound: Option<&crate::analyze::SoundMap>, segments: &mut Vec<Segment>) {
    let min = s.min_length.max(0.0) as f64;
    let max = (s.max_length as f64).max(min);
    if run.end - run.start + 1e-9 < min {
        segments.push(make_segment(sm, bounds, run.start, run.end, SegmentStatus::TooShort, s));
        return;
    }
    let talking = sound.filter(|sd| !sd.talking.is_empty());
    let sound = talking.filter(|_| s.snap_to_pauses);
    let needs_split = run.end - run.start > max + 1e-9;
    let mut parts = if needs_split && sm.motion.len() == sm.len() {
        split_at_rest(sm, bounds, run, min, max, s.keep_moves_whole, sound).unwrap_or_else(|| split(run.start, run.end, min, max))
    } else {
        split(run.start, run.end, min, max)
    };
    let mut snapped = vec![(0u8, 0u8); parts.len()];
    if let Some(sd) = sound {
        snap_parts(&mut parts, &mut snapped, run, min, max, sd);
    }
    // Trimmed ends from splitting are reported as too short.
    let first_start = parts.first().map(|p| p.0).unwrap_or(run.start);
    let last_end = parts.last().map(|p| p.1).unwrap_or(run.end);
    if first_start - run.start > 1e-6 {
        segments.push(make_segment(sm, bounds, run.start, first_start, SegmentStatus::TooShort, s));
    }
    for (k, (a, b)) in parts.iter().copied().enumerate() {
        let mut g = make_segment(sm, bounds, a, b, SegmentStatus::Kept, s);
        // Ends that cut while someone talks are reported with the pause setting on or off.
        if let Some(sd) = talking {
            g.speech_cuts = sd.talking_at(a + 0.02) as u8 + sd.talking_at(b - 0.02) as u8;
            g.snapped = snapped[k].0 + snapped[k].1;
        }
        segments.push(g);
    }
    if run.end - last_end > 1e-6 {
        segments.push(make_segment(sm, bounds, last_end, run.end, SegmentStatus::TooShort, s));
    }
}

/// The best point in `lo..hi` to cut at, inside a pause: `Edge::Start` cuts
/// just before talking resumes, `Edge::End` just after it stops, `Edge::Both`
/// in the middle of the pause. None when no pause reaches into the range.
fn pause_point(sd: &crate::analyze::SoundMap, near: f64, lo: f64, hi: f64, edge: Edge) -> Option<f64> {
    let mut best: Option<f64> = None;
    for &(a, b) in &sd.pauses {
        if b < lo || a > hi {
            continue;
        }
        let lead = ((b - a) / 2.0).min(0.25);
        let want = match edge {
            Edge::Start => b - lead,
            Edge::End => a + lead,
            Edge::Both => 0.5 * (a + b),
        };
        let p = want.clamp(lo.max(a), hi.min(b));
        if best.map_or(true, |q| (p - near).abs() < (q - near).abs()) {
            best = Some(p);
        }
    }
    best
}

#[derive(Clone, Copy)]
enum Edge {
    Start,
    End,
    Both,
}

/// Move the ends of `parts` that cut through talking into pauses, keeping
/// every part within the run and within the length limits.
fn snap_parts(parts: &mut [(f64, f64)], snapped: &mut [(u8, u8)], run: &Run, min: f64, max: f64, sd: &crate::analyze::SoundMap) {
    let n = parts.len();
    let long = 2.0 * max;
    for k in 0..n {
        // Start of this part.
        let (a, b) = parts[k];
        if sd.talking_at(a + 0.02) {
            let (lo, edge) = if k == 0 { (run.start, Edge::Start) } else { ((a - PAUSE_REACH).max(parts[k - 1].0 + min), Edge::Both) };
            let hi = (a + PAUSE_REACH).min(b - min);
            if lo <= hi {
                if let Some(p) = pause_point(sd, a, lo, hi, edge) {
                    let ok_prev = k == 0 || (p - parts[k - 1].0 <= long && p - parts[k - 1].0 + 1e-9 >= min);
                    if ok_prev && b - p <= long {
                        parts[k].0 = p;
                        snapped[k].0 = 1;
                        if k > 0 {
                            parts[k - 1].1 = p;
                            snapped[k - 1].1 = 1;
                        }
                    }
                }
            }
        }
        // End of the last part (inner ends were handled as the next part's start).
        if k == n - 1 {
            let (a, b) = parts[k];
            if sd.talking_at(b - 0.02) {
                let lo = (b - PAUSE_REACH).max(a + min);
                let hi = run.end.min(b + PAUSE_REACH);
                if lo <= hi {
                    if let Some(p) = pause_point(sd, b, lo, hi, Edge::End) {
                        parts[k].1 = p;
                        snapped[k].1 = 1;
                    }
                }
            }
        }
    }
}

/// Run selection for one clip. `end_time` is the clip duration, or for a clip
/// still being analysed, the time up to which it has been measured.
pub fn select_clip(scores: &ScoreSeries, end_time: f64, s: &SelectionSettings) -> ClipSelection {
    let mut sel = select_candidates(scores, end_time, s);
    apply_cap(&mut sel.segments, s.max_per_clip);
    sel
}

/// Everything `select_clip` does except the per-clip limit, so the project
/// can set aside repeated takes across clips first.
pub fn select_candidates(scores: &ScoreSeries, end_time: f64, s: &SelectionSettings) -> ClipSelection {
    select_candidates_with(scores, end_time, s, &ClipExtras::default())
}

/// `select_candidates` with what else is known about the clip: picture
/// checks and sound for each segment, cuts in speech pauses, and calls made
/// by hand.
pub fn select_candidates_with(scores: &ScoreSeries, end_time: f64, s: &SelectionSettings, x: &ClipExtras) -> ClipSelection {
    let n = scores.len();
    if n == 0 {
        return ClipSelection::default();
    }
    let sm = ScoreSeries {
        t: scores.t.clone(),
        focus: smooth(&scores.focus, &scores.t, s.smoothing),
        exposure: smooth(&scores.exposure, &scores.t, s.smoothing),
        stability: smooth(&scores.stability, &scores.t, s.smoothing),
        motion: scores.motion.clone(),
        key_stops: scores.key_stops.clone(),
        clipped: scores.clipped.clone(),
        issue: scores.issue.clone(),
        pan: scores.pan.clone(),
        tilt: scores.tilt.clone(),
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
            let mut seg = make_segment(&sm, &bounds, run.start, run.end, SegmentStatus::BelowThreshold, s);
            seg.failed = counts
                .iter()
                .filter(|c| c.1 > 0)
                .map(|c| c.0.to_string())
                .collect();
            segments.push(seg);
            continue;
        }
        if s.camera != CameraFilter::Any && sm.motion.len() == n {
            for piece in camera_pieces(&sm, &bounds, run) {
                place_run(&sm, &bounds, &piece, s, x.sound, &mut segments);
            }
        } else {
            place_run(&sm, &bounds, run, s, x.sound, &mut segments);
        }
    }

    for g in segments.iter_mut() {
        if g.status == SegmentStatus::Kept && !s.camera.allows(g.camera) {
            g.status = SegmentStatus::SetAside;
            g.aside = Some(AsideReason::Camera);
        }
    }

    if !x.marks.is_empty() {
        segments = apply_marks(&sm, &bounds, end_time, s, segments, x.marks);
    }
    for g in segments.iter_mut() {
        add_checks(&sm, &bounds, g, x);
    }

    ClipSelection {
        smoothed: sm,
        segments,
    }
}

/// Middle value of a list, or None for an empty one.
fn median(mut v: Vec<f32>) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(v[v.len() / 2])
}

/// Fill in a segment's horizon, flicker, wobble, white balance and sound.
fn add_checks(sm: &ScoreSeries, bounds: &[f64], g: &mut Segment, x: &ClipExtras) {
    if let Some(f) = x.frames.filter(|f| f.has_picture_checks() && f.len() == sm.len()) {
        let idx = segment_indices(sm, bounds, g.start, g.end);
        let rolls: Vec<f32> = idx.iter().filter(|&&i| f.roll_conf[i] >= ROLL_MIN_CONFIDENCE).map(|&i| f.roll[i]).collect();
        if rolls.len() >= 2 && rolls.len() * 5 >= idx.len() * 2 {
            g.roll = median(rolls).map(|r| (r * 10.0).round() / 10.0);
        }
        let flick: Vec<f32> = idx.iter().map(|&i| f.flicker[i]).filter(|&v| v >= 0.0).collect();
        if flick.len() >= 3 {
            g.flicker = median(flick);
        }
        let wob: Vec<f32> = idx.iter().map(|&i| f.wobble[i]).filter(|&v| v >= 0.0).collect();
        if wob.len() >= 3 {
            g.wobble = median(wob);
        }
        let ok: Vec<usize> = idx.iter().copied().filter(|&i| f.wb_ok[i] != 0).collect();
        if ok.len() * 2 >= idx.len().max(1) && !ok.is_empty() {
            let cb = median(ok.iter().map(|&i| f.cb[i]).collect()).unwrap_or(0.0);
            let cr = median(ok.iter().map(|&i| f.cr[i]).collect()).unwrap_or(0.0);
            g.wb = Some((cb, cr));
        }
    }
    if let Some(t) = x.audio {
        g.sound = crate::audio::issues_in(t, g.start, g.end);
        if x.sound.map(|sd| sd.summary.has_speech).unwrap_or(false) {
            g.talk = crate::audio::talk_share_in(t, g.start, g.end);
        }
    }
}

/// Apply calls made by hand. Dropped stretches set aside the kept segments
/// they mostly cover; kept stretches replace whatever was there with one
/// kept segment exactly as marked.
fn apply_marks(sm: &ScoreSeries, bounds: &[f64], end_time: f64, s: &SelectionSettings, mut segments: Vec<Segment>, marks: &[Mark]) -> Vec<Segment> {
    for m in marks.iter().filter(|m| m.call == Call::Drop) {
        for g in segments.iter_mut() {
            let overlap = g.end.min(m.end) - g.start.max(m.start);
            if overlap > 0.0 && overlap * 2.0 >= g.duration().min(m.end - m.start) && matches!(g.status, SegmentStatus::Kept | SegmentStatus::SetAside) {
                g.status = SegmentStatus::SetAside;
                g.aside = Some(AsideReason::Dropped);
            }
        }
    }
    let mut keeps: Vec<(f64, f64, u8)> = marks
        .iter()
        .filter(|m| m.call == Call::Keep)
        .map(|m| (m.start.max(0.0), m.end.min(end_time.max(m.start)), m.stars.min(5)))
        .filter(|k| k.1 - k.0 > 0.05)
        .collect();
    if keeps.is_empty() {
        return segments;
    }
    keeps.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let min = s.min_length.max(0.0) as f64;
    let mut out: Vec<Segment> = Vec::with_capacity(segments.len() + keeps.len() * 2);
    for g in segments {
        // What is left of this segment outside every kept mark.
        let mut pieces = vec![(g.start, g.end)];
        for &(ka, kb, _) in &keeps {
            let mut next = Vec::new();
            for (a, b) in pieces {
                if kb <= a || ka >= b {
                    next.push((a, b));
                    continue;
                }
                if ka > a {
                    next.push((a, ka));
                }
                if kb < b {
                    next.push((kb, b));
                }
            }
            pieces = next;
        }
        if pieces.len() == 1 && (pieces[0].0 - g.start).abs() < 1e-9 && (pieces[0].1 - g.end).abs() < 1e-9 {
            out.push(g);
            continue;
        }
        for (a, b) in pieces.into_iter().filter(|p| p.1 - p.0 > 0.05) {
            let mut status = g.status;
            if status == SegmentStatus::Kept && b - a + 1e-9 < min {
                status = SegmentStatus::TooShort;
            }
            let mut piece = make_segment(sm, bounds, a, b, status, s);
            piece.failed = g.failed.clone();
            piece.aside = if status == SegmentStatus::SetAside { g.aside } else { None };
            out.push(piece);
        }
    }
    for (a, b, stars) in keeps {
        let mut k = make_segment(sm, bounds, a, b, SegmentStatus::Kept, s);
        k.manual = true;
        k.stars = stars;
        out.push(k);
    }
    out.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Keep only the best `cap` kept segments; the rest go over the limit. 0 means no limit.
/// Segments kept by hand always stay and do not count toward the limit.
pub fn apply_cap(segments: &mut [Segment], cap: u32) {
    if cap == 0 {
        return;
    }
    let mut ranked: Vec<usize> = segments
        .iter()
        .enumerate()
        .filter(|(_, g)| g.status == SegmentStatus::Kept && !g.manual)
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
    for &i in ranked.iter().skip(cap as usize) {
        segments[i].status = SegmentStatus::OverLimit;
    }
}

/// Why a fully analysed clip has no selects, in the terms the sliders use.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NoSelectReason {
    /// This score's average over the clip is under its threshold. Both are 0 to 100.
    Average { score: String, average: u32, needs: u32 },
    /// The averages pass, but this score is under its threshold for `share`
    /// percent of the clip, so no stretch passes all three at once.
    Share { score: String, share: u32, needs: u32 },
    /// Some stretches pass, but all are shorter than the shortest select.
    TooShort { longest: f64, needs: f64 },
    /// Good stretches exist, but all were set aside: `camera` by the camera
    /// filter, `repeats` as repeated takes of selects in other clips.
    SetAside {
        camera: usize,
        repeats: usize,
        #[serde(default)]
        light: usize,
        #[serde(default)]
        sound: usize,
        #[serde(default)]
        dropped: usize,
    },
}

/// The main reason a clip got no selects, or None when it has some (or
/// nothing has been measured). The score that misses its threshold by the
/// most points is named; ties go to focus, then exposure, then stability.
pub fn no_select_reason(sel: &ClipSelection, s: &SelectionSettings) -> Option<NoSelectReason> {
    let sm = &sel.smoothed;
    if sm.is_empty() || sel.segments.iter().any(|g| g.status == SegmentStatus::Kept) {
        return None;
    }
    let aside = |r: AsideReason| sel.segments.iter().filter(|g| g.status == SegmentStatus::SetAside && g.aside == Some(r)).count();
    let (camera, repeats) = (aside(AsideReason::Camera), aside(AsideReason::Repeat));
    let (light, sound, dropped) = (aside(AsideReason::Light), aside(AsideReason::Sound), aside(AsideReason::Dropped));
    if camera + repeats + light + sound + dropped > 0 {
        return Some(NoSelectReason::SetAside { camera, repeats, light, sound, dropped });
    }
    let scores = [
        ("focus", &sm.focus, s.focus_min),
        ("exposure", &sm.exposure, s.exposure_min),
        ("stability", &sm.stability, s.stability_min),
    ];
    let pts = |v: f32| (v * 100.0).clamp(0.0, 100.0);
    let mut worst: Option<(&str, f32, f32)> = None;
    for (name, v, min) in scores {
        let avg = pts(crate::scores::mean(v));
        let short = pts(min) - avg;
        if short > 0.0 && worst.map_or(true, |w| short > w.2) {
            worst = Some((name, avg, short));
        }
    }
    if let Some((name, avg, _)) = worst {
        let needs = scores.iter().find(|x| x.0 == name).map(|x| pts(x.2)).unwrap_or(0.0);
        // Rounded down, so a near miss never reads as meeting the threshold.
        return Some(NoSelectReason::Average { score: name.into(), average: (avg + 0.001).floor() as u32, needs: needs.round() as u32 });
    }
    let mut most: Option<(&str, usize, f32)> = None;
    for (name, v, min) in scores {
        let under = v.iter().filter(|x| **x < min).count();
        if under > 0 && most.map_or(true, |m| under > m.1) {
            most = Some((name, under, min));
        }
    }
    let too_short: Vec<&Segment> = sel.segments.iter().filter(|g| g.status == SegmentStatus::TooShort).collect();
    if !too_short.is_empty() && most.map_or(true, |m| m.1 * 2 < sm.len()) {
        let longest = too_short.iter().map(|g| g.duration()).fold(0.0, f64::max);
        return Some(NoSelectReason::TooShort { longest, needs: s.min_length as f64 });
    }
    most.map(|(name, under, min)| NoSelectReason::Share {
        score: name.into(),
        share: ((under as f32 / sm.len() as f32) * 100.0).round() as u32,
        needs: pts(min).round() as u32,
    })
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
