//! Learning from finished videos.
//!
//! A finished video (an edit the person already made and exported) is traced
//! back to the clips in the project with `matching`. That says which moments
//! this person chose to use. From it:
//! 1. `report` says how many of those moments the current settings would
//!    keep, and why the others are dropped.
//! 2. `fit` searches the selection settings for the ones that keep the most
//!    used moments while keeping the selects short, and checks the result on
//!    a video the search did not see when there is more than one.
//!
//! The settings stay the same sliders the person already knows, so whatever
//! was learned can be read off them and changed by hand.

use crate::matching::{match_reference, ClipPrints, MatchReport};
use crate::project::Project;
use crate::reference::ReferencePrint;
use crate::scores::ScoreSeries;
use crate::selection::{select_clip, ClipSelection, SegmentStatus, SelectionSettings};
use serde::{Deserialize, Serialize};

/// A finished video and where its shots came from.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FinishedVideo {
    pub path: String,
    pub name: String,
    pub duration: f64,
    pub report: MatchReport,
    /// How many source clips were analysed when it was traced. Tracing again
    /// after adding footage can find more shots.
    #[serde(default)]
    pub clips_searched: usize,
    /// The video's fingerprints, kept so it can be traced again in a moment
    /// when footage is added, without decoding the video again.
    #[serde(default)]
    pub print: Option<ReferencePrint>,
}

impl FinishedVideo {
    pub fn shots(&self) -> usize {
        self.report.shots.len()
    }
    pub fn traced(&self) -> usize {
        self.report.matched().count()
    }
}

/// Trace a fingerprinted finished video against every analysed clip.
pub fn trace(project: &Project, print: &ReferencePrint) -> FinishedVideo {
    let views: Vec<ClipPrints> = project
        .clips
        .iter()
        .filter_map(|c| {
            let a = c.analysis.as_ref()?;
            if a.frames.fp.len() != a.frames.len() || a.frames.is_empty() {
                return None;
            }
            Some(ClipPrints {
                clip_id: c.id,
                t: &a.frames.t,
                fp: &a.frames.fp,
                duration: c.info.as_ref().map(|i| i.duration).unwrap_or(0.0),
            })
        })
        .collect();
    let report = match_reference(print, &views);
    FinishedVideo {
        path: print.path.clone(),
        name: print.name.clone(),
        duration: print.duration,
        report,
        clips_searched: views.len(),
        print: Some(print.clone()),
    }
}

/// Trace every finished video again against the clips analysed now.
pub fn retrace_all(project: &mut Project) {
    let prints: Vec<Option<ReferencePrint>> = project.finished_videos.iter().map(|f| f.print.clone()).collect();
    for (i, p) in prints.into_iter().enumerate() {
        if let Some(p) = p {
            let fresh = trace(project, &p);
            project.finished_videos[i] = fresh;
        }
    }
}

/// Number of clips with fingerprints that a trace would search now.
pub fn clips_with_prints(project: &Project) -> usize {
    project
        .clips
        .iter()
        .filter(|c| c.analysis.as_ref().map(|a| !a.frames.is_empty() && a.frames.fp.len() == a.frames.len()).unwrap_or(false))
        .count()
}

/// One moment a finished video used, on a source clip.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UsedMoment {
    pub video: usize,
    pub clip_id: u64,
    pub start: f64,
    pub end: f64,
}

/// Every traced moment, across all finished videos.
pub fn used_moments(project: &Project) -> Vec<UsedMoment> {
    let mut out = Vec::new();
    for (v, fv) in project.finished_videos.iter().enumerate() {
        for r in fv.report.matched() {
            // Only clips still in the project count.
            if project.clip(r.clip_id).is_some() && r.end > r.start {
                out.push(UsedMoment {
                    video: v,
                    clip_id: r.clip_id,
                    start: r.start,
                    end: r.end,
                });
            }
        }
    }
    out
}

/// Why a used moment is not among the selects.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MissReason {
    Focus,
    Exposure,
    Stability,
    TooShort,
    OverLimit,
    NotAnalyzed,
}

impl MissReason {
    pub fn words(self) -> &'static str {
        match self {
            MissReason::Focus => "focus under the threshold",
            MissReason::Exposure => "exposure under the threshold",
            MissReason::Stability => "stability under the threshold",
            MissReason::TooShort => "shorter than the shortest select",
            MissReason::OverLimit => "over the selects-per-clip limit",
            MissReason::NotAnalyzed => "clip not analyzed",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct TasteReport {
    pub videos: usize,
    /// Shots traced back to clips in this project.
    pub used: usize,
    /// Of those, how many the settings keep.
    pub kept: usize,
    /// Clips the used shots came from.
    pub clips_used: usize,
    /// Why the others are dropped, most common first.
    pub misses: Vec<(MissReason, usize)>,
    pub selects_seconds: f64,
    pub footage_seconds: f64,
}

/// A used moment counts as kept when selects cover at least half of it, or
/// at least 1.5 seconds of it.
fn covered(sel: &ClipSelection, start: f64, end: f64) -> bool {
    let len = (end - start).max(1e-6);
    let overlap: f64 = sel
        .kept()
        .map(|s| (s.end.min(end) - s.start.max(start)).max(0.0))
        .sum();
    overlap >= 0.5 * len || overlap >= 1.5
}

fn miss_reason(sel: &ClipSelection, start: f64, end: f64) -> MissReason {
    // The segment covering most of the moment decides.
    let mut best: Option<(&crate::selection::Segment, f64)> = None;
    for s in &sel.segments {
        let o = (s.end.min(end) - s.start.max(start)).max(0.0);
        if o > 0.0 && best.map(|b| o > b.1).unwrap_or(true) && s.status != SegmentStatus::Kept {
            best = Some((s, o));
        }
    }
    match best.map(|b| b.0) {
        None => MissReason::NotAnalyzed,
        Some(s) => match s.status {
            SegmentStatus::TooShort => MissReason::TooShort,
            SegmentStatus::OverLimit => MissReason::OverLimit,
            SegmentStatus::BelowThreshold => match s.failed.first().map(|f| f.as_str()) {
                Some("focus") => MissReason::Focus,
                Some("exposure") => MissReason::Exposure,
                _ => MissReason::Stability,
            },
            SegmentStatus::Kept => MissReason::OverLimit,
        },
    }
}

/// Scores computed once, so many settings can be tried quickly.
pub struct Prepared {
    clips: Vec<(u64, ScoreSeries, f64)>,
    footage_seconds: f64,
}

pub fn prepare(project: &Project) -> Prepared {
    let mut clips = Vec::new();
    let mut footage = 0.0;
    for c in &project.clips {
        if let (Some(s), Some(info)) = (c.scores(project.fallback_profile), c.info.as_ref()) {
            footage += info.duration;
            clips.push((c.id, s, c.measured_until()));
        }
    }
    Prepared {
        clips,
        footage_seconds: footage,
    }
}

fn evaluate(p: &Prepared, moments: &[UsedMoment], settings: &SelectionSettings) -> TasteReport {
    let mut r = TasteReport {
        footage_seconds: p.footage_seconds,
        ..Default::default()
    };
    let mut misses: Vec<(MissReason, usize)> = Vec::new();
    let mut clips_used = std::collections::HashSet::new();
    for (id, scores, until) in &p.clips {
        let sel = select_clip(scores, *until, settings);
        r.selects_seconds += sel.kept_seconds();
        for m in moments.iter().filter(|m| m.clip_id == *id) {
            r.used += 1;
            clips_used.insert(*id);
            if covered(&sel, m.start, m.end) {
                r.kept += 1;
            } else {
                let why = miss_reason(&sel, m.start, m.end);
                match misses.iter_mut().find(|x| x.0 == why) {
                    Some(x) => x.1 += 1,
                    None => misses.push((why, 1)),
                }
            }
        }
    }
    // Moments on clips without scores.
    let scored: std::collections::HashSet<u64> = p.clips.iter().map(|c| c.0).collect();
    let unscored = moments.iter().filter(|m| !scored.contains(&m.clip_id)).count();
    if unscored > 0 {
        r.used += unscored;
        misses.push((MissReason::NotAnalyzed, unscored));
    }
    misses.sort_by(|a, b| b.1.cmp(&a.1));
    r.misses = misses;
    r.clips_used = clips_used.len();
    r
}

/// How the current settings do against every finished video.
pub fn report(project: &Project) -> TasteReport {
    let p = prepare(project);
    let mut r = evaluate(&p, &used_moments(project), &project.selection);
    r.videos = project.finished_videos.len();
    r
}

/// Higher is better: share of used shots kept, minus a cost for selecting
/// more of the footage. Adding a tenth of the footage to the selects has to
/// win back at least a twentieth of the used shots.
fn objective(r: &TasteReport) -> f64 {
    if r.used == 0 {
        return 0.0;
    }
    let recall = r.kept as f64 / r.used as f64;
    let share = if r.footage_seconds > 0.0 { r.selects_seconds / r.footage_seconds } else { 0.0 };
    recall - 0.5 * share
}

const SCORE_STEPS: [f32; 15] = [0.1, 0.15, 0.2, 0.25, 0.3, 0.35, 0.4, 0.45, 0.5, 0.55, 0.6, 0.65, 0.7, 0.75, 0.8];
const MIN_LENGTHS: [f32; 6] = [0.5, 1.0, 1.5, 2.0, 3.0, 4.0];
const CAPS: [u32; 8] = [1, 2, 3, 4, 5, 6, 8, 0];
const SMOOTHING: [f32; 5] = [0.0, 0.5, 1.0, 1.5, 2.0];

/// Coordinate search: try every step of one setting while holding the
/// others, keep the best, and go round until nothing improves.
fn search(p: &Prepared, moments: &[UsedMoment], start: &SelectionSettings) -> SelectionSettings {
    let mut best = start.clone();
    let mut best_score = objective(&evaluate(p, moments, &best));
    for _round in 0..5 {
        let mut improved = false;
        for knob in 0..6 {
            let candidates: Vec<SelectionSettings> = match knob {
                0 => SCORE_STEPS.iter().map(|&v| SelectionSettings { focus_min: v, ..best.clone() }).collect(),
                1 => SCORE_STEPS.iter().map(|&v| SelectionSettings { exposure_min: v, ..best.clone() }).collect(),
                2 => SCORE_STEPS.iter().map(|&v| SelectionSettings { stability_min: v, ..best.clone() }).collect(),
                3 => MIN_LENGTHS
                    .iter()
                    .map(|&v| SelectionSettings {
                        min_length: v,
                        max_length: best.max_length.max(v),
                        ..best.clone()
                    })
                    .collect(),
                4 => CAPS.iter().map(|&v| SelectionSettings { max_per_clip: v, ..best.clone() }).collect(),
                _ => SMOOTHING.iter().map(|&v| SelectionSettings { smoothing: v, ..best.clone() }).collect(),
            };
            for c in candidates {
                let s = objective(&evaluate(p, moments, &c));
                // Small margin so ties keep the current value.
                if s > best_score + 1e-6 {
                    best_score = s;
                    best = c;
                    improved = true;
                }
            }
        }
        if !improved {
            break;
        }
    }
    best
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FitResult {
    pub settings: SelectionSettings,
    pub before: TasteReport,
    pub after: TasteReport,
    /// With two or more finished videos: for each, settings are fitted on
    /// the others and tried on it. Share of its used shots kept, averaged.
    pub held_out_kept_share: Option<f64>,
    /// Same check for the current settings, for comparison.
    pub held_out_before_share: Option<f64>,
}

/// Find settings that keep more of the moments this person used.
pub fn fit(project: &Project) -> Option<FitResult> {
    let moments = used_moments(project);
    if moments.is_empty() {
        return None;
    }
    let p = prepare(project);
    let mut before = evaluate(&p, &moments, &project.selection);
    let settings = search(&p, &moments, &project.selection);
    let mut after = evaluate(&p, &moments, &settings);
    before.videos = project.finished_videos.len();
    after.videos = before.videos;

    let videos: Vec<usize> = {
        let mut v: Vec<usize> = moments.iter().map(|m| m.video).collect();
        v.sort_unstable();
        v.dedup();
        v
    };
    let (mut held, mut held_before) = (None, None);
    if videos.len() >= 2 {
        let (mut sum, mut sum_before, mut n) = (0.0, 0.0, 0.0);
        for &v in &videos {
            let train: Vec<UsedMoment> = moments.iter().filter(|m| m.video != v).cloned().collect();
            let test: Vec<UsedMoment> = moments.iter().filter(|m| m.video == v).cloned().collect();
            let s = search(&p, &train, &project.selection);
            let r = evaluate(&p, &test, &s);
            let rb = evaluate(&p, &test, &project.selection);
            if r.used > 0 {
                sum += r.kept as f64 / r.used as f64;
                sum_before += rb.kept as f64 / rb.used as f64;
                n += 1.0;
            }
        }
        if n > 0.0 {
            held = Some(sum / n);
            held_before = Some(sum_before / n);
        }
    }
    Some(FitResult {
        settings,
        before,
        after,
        held_out_kept_share: held,
        held_out_before_share: held_before,
    })
}

/// Used moments on one clip, for the detail view: (start, end, video name).
pub fn moments_on_clip(project: &Project, clip_id: u64) -> Vec<(f64, f64, String)> {
    used_moments(project)
        .into_iter()
        .filter(|m| m.clip_id == clip_id)
        .map(|m| (m.start, m.end, project.finished_videos[m.video].name.clone()))
        .collect()
}
