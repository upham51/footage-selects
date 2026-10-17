//! Shared app state and the snapshots sent to the window.

use crate::runner::RunnerInner;
use selects_engine::project::{FileStatus, ProfileSource, Project, ProjectClip};
use selects_engine::taste::{report as taste_report, MissReason, TasteReport};
use selects_engine::tone::{ToneProfile, ALL_PROFILES};
use selects_engine::rate::{Rate, TIMELINE_RATES};
use selects_engine::selection::{no_select_reason, ClipSelection, NoSelectReason, SegmentStatus, SelectionSettings, Summary};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Something about analysis changed (progress, a clip finished).
pub const EVENT_ANALYSIS: &str = "analysis-changed";
/// Something else changed; the window should refresh everything.
pub const EVENT_STATE: &str = "state-changed";
/// Files are being read after being added: payload {done, total}.
pub const EVENT_PROBE: &str = "probe-progress";
/// Files are hovering over the window: payload bool.
pub const EVENT_DROP_HOVER: &str = "drop-hover";
/// The window was asked to close with unsaved changes.
pub const EVENT_CLOSE_REQUESTED: &str = "close-requested";

#[derive(Default)]
pub struct AppState {
    pub project: Mutex<Project>,
    pub project_path: Mutex<Option<PathBuf>>,
    pub dirty: AtomicBool,
    pub runner: Mutex<RunnerInner>,
    pub probing: AtomicBool,
    /// A finished video being read: (name, videos done, videos total).
    pub tracing: Mutex<Option<(String, usize, usize)>>,
    pub tracing_cancel: AtomicBool,
}

#[derive(Serialize)]
pub struct SegDto {
    pub start: f64,
    pub end: f64,
    pub focus: f32,
    pub exposure: f32,
    pub stability: f32,
    pub score: f32,
}

#[derive(Serialize)]
pub struct ClipRow {
    pub id: u64,
    pub name: String,
    pub path: String,
    pub folder: String,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub rate: String,
    pub variable_frame_rate: bool,
    pub codec: String,
    pub has_audio: bool,
    pub rotation: i32,
    pub timecode: Option<String>,
    /// waiting, analyzing, analyzed, failed, unreadable, missing, changed, partial, not_analyzed
    pub status: &'static str,
    pub status_detail: Option<String>,
    /// Share of the clip measured so far, 0 to 1.
    pub progress: f64,
    pub kept: Vec<SegDto>,
    pub too_short: usize,
    pub over_limit: usize,
    /// The picture profile exposure is judged with, and where it came from.
    pub profile_label: String,
    pub profile_is_log: bool,
    pub profile_source: ProfileSource,
    /// Shots from finished videos that came from this clip.
    pub used: Vec<(f64, f64)>,
    /// The camera's shutter was far faster than the frame rate suits.
    pub choppy_shutter: bool,
    /// For an analysed clip with no selects: which score kept it out.
    pub no_select: Option<NoSelectReason>,
}

#[derive(Serialize)]
pub struct AnalysisDto {
    pub running: bool,
    pub stopping: bool,
    pub clips_total: usize,
    pub clips_done: usize,
    pub active: usize,
    /// 0 to 1 by seconds of footage.
    pub fraction: f64,
    pub elapsed: f64,
    pub eta_seconds: Option<f64>,
    /// Seconds of footage analysed per second of wall time.
    pub speed: Option<f64>,
    /// Clips that could be analysed now (new, changed, stopped part way, or at another sample rate).
    pub pending: usize,
    pub probing: bool,
}

#[derive(Serialize)]
pub struct RateOption {
    pub num: u32,
    pub den: u32,
    pub label: String,
    pub drop_frame_capable: bool,
}

#[derive(Serialize)]
pub struct ExportDto {
    pub timeline_rate: RateOption,
    pub rate_is_auto: bool,
    pub auto_rate_label: String,
    pub drop_frame: bool,
    pub layout: selects_engine::project::TimelineLayout,
    pub timeline_name: String,
    pub rates: Vec<RateOption>,
    pub resolution: (u32, u32),
}

#[derive(Serialize)]
pub struct ProfileOption {
    pub id: ToneProfile,
    pub label: &'static str,
    pub approximate: bool,
}

#[derive(Serialize)]
pub struct FinishedDto {
    pub name: String,
    pub path: String,
    pub duration: f64,
    pub shots: usize,
    pub traced: usize,
    /// More clips have been analysed since it was traced.
    pub stale: bool,
}

#[derive(Serialize)]
pub struct TasteDto {
    pub report: TasteReport,
    /// Plain words for each reason, in the same order as `report.misses`.
    pub miss_words: Vec<String>,
}

#[derive(Serialize)]
pub struct TracingDto {
    pub name: String,
    pub done: usize,
    pub total: usize,
}

#[derive(Serialize)]
pub struct StateDto {
    pub clips: Vec<ClipRow>,
    pub summary: Summary,
    pub selection: SelectionSettings,
    pub sample_rate: f64,
    pub export: ExportDto,
    pub analysis: AnalysisDto,
    pub project_name: Option<String>,
    pub project_path: Option<String>,
    pub dirty: bool,
    pub fallback_profile: Option<ToneProfile>,
    pub profiles: Vec<ProfileOption>,
    pub finished_videos: Vec<FinishedDto>,
    pub taste: Option<TasteDto>,
    pub tracing: Option<TracingDto>,
}

pub fn profile_options() -> Vec<ProfileOption> {
    ALL_PROFILES
        .iter()
        .map(|&p| ProfileOption {
            id: p,
            label: p.label(),
            approximate: p.approximate(),
        })
        .collect()
}

fn miss_words(m: &[(MissReason, usize)]) -> Vec<String> {
    m.iter().map(|(r, _)| r.words().to_string()).collect()
}

pub fn rate_option(r: Rate) -> RateOption {
    RateOption {
        num: r.num,
        den: r.den,
        label: r.label(),
        drop_frame_capable: r.supports_drop_frame(),
    }
}

fn seg_dto(s: &selects_engine::selection::Segment) -> SegDto {
    SegDto {
        start: s.start,
        end: s.end,
        focus: s.focus,
        exposure: s.exposure,
        stability: s.stability,
        score: s.score,
    }
}

fn clip_status(
    c: &ProjectClip,
    sample_rate: f64,
    runner: &RunnerInner,
) -> (&'static str, Option<String>) {
    if runner.active.contains(&c.id) {
        return ("analyzing", None);
    }
    if runner.queue.contains(&c.id) {
        return ("waiting", None);
    }
    if c.info.is_none() {
        return ("unreadable", c.error.clone());
    }
    match c.file_status() {
        FileStatus::Missing => {
            return (
                "missing",
                Some("The file is no longer where it was. Its scores are kept, but exports will point to the old location.".into()),
            )
        }
        FileStatus::Changed => {
            return (
                "changed",
                Some("The file changed after it was analysed. Analyse it again to update its scores.".into()),
            )
        }
        FileStatus::Present => {}
    }
    if let Some(e) = &c.error {
        return ("failed", Some(e.clone()));
    }
    match &c.analysis {
        Some(a) if !c.needs_analysis(sample_rate) => {
            let _ = a;
            ("analyzed", None)
        }
        Some(a) if a.complete => (
            "not_analyzed",
            Some(format!(
                "Scored at {} {} per second. Analyse again to use the new sample rate.",
                a.sample_rate,
                if a.sample_rate == 1.0 { "frame" } else { "frames" }
            )),
        ),
        Some(a) if !a.frames.is_empty() => (
            "partial",
            Some("Analysis stopped part way. The part already measured is used.".into()),
        ),
        _ => ("not_analyzed", None),
    }
}

pub fn clip_rows(project: &Project, selections: &[ClipSelection], runner: &RunnerInner) -> Vec<ClipRow> {
    let all_used = selects_engine::taste::used_moments(project);
    project
        .clips
        .iter()
        .zip(selections.iter())
        .map(|(c, sel)| {
            let (status, detail) = clip_status(c, project.sample_rate, runner);
            let info = c.info.as_ref();
            let path = std::path::Path::new(&c.path);
            let duration = info.map(|i| i.duration).unwrap_or(0.0);
            let progress = if duration > 0.0 {
                (c.measured_until() / duration).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let resolved = c.resolved_profile(project.fallback_profile);
            let used: Vec<(f64, f64)> = all_used
                .iter()
                .filter(|m| m.clip_id == c.id)
                .map(|m| (m.start, m.end))
                .collect();
            let choppy_shutter = info
                .and_then(|i| i.camera.as_ref().and_then(|n| n.shutter_warning(i.rate.fps())))
                .is_some();
            let no_select = if status == "analyzed" { no_select_reason(sel, &project.selection) } else { None };
            ClipRow {
                no_select,
                profile_label: resolved.profile.label().to_string(),
                profile_is_log: resolved.profile.is_log(),
                profile_source: resolved.source,
                used,
                choppy_shutter,
                id: c.id,
                name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| c.path.clone()),
                path: c.path.clone(),
                folder: path
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default(),
                duration,
                width: info.map(|i| i.width).unwrap_or(0),
                height: info.map(|i| i.height).unwrap_or(0),
                rate: info.map(|i| i.rate.label()).unwrap_or_default(),
                variable_frame_rate: info.map(|i| i.variable_frame_rate).unwrap_or(false),
                codec: info.map(|i| i.codec.clone()).unwrap_or_default(),
                has_audio: info.map(|i| i.has_audio).unwrap_or(false),
                rotation: info.map(|i| i.rotation).unwrap_or(0),
                timecode: info.and_then(|i| i.timecode.clone()),
                status,
                status_detail: detail,
                progress,
                kept: sel.kept().map(seg_dto).collect(),
                too_short: sel
                    .segments
                    .iter()
                    .filter(|s| s.status == SegmentStatus::TooShort)
                    .count(),
                over_limit: sel
                    .segments
                    .iter()
                    .filter(|s| s.status == SegmentStatus::OverLimit)
                    .count(),
            }
        })
        .collect()
}

pub fn analysis_dto(project: &Project, runner: &RunnerInner, probing: bool) -> AnalysisDto {
    let mut done_seconds = runner.finished_seconds;
    for id in &runner.active {
        if let Some(c) = project.clip(*id) {
            done_seconds += c.measured_until();
        }
    }
    let elapsed = runner.started.map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0);
    let fraction = if runner.total_seconds > 0.0 {
        (done_seconds / runner.total_seconds).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let speed = if runner.running && elapsed > 1.5 && done_seconds > 0.0 {
        Some(done_seconds / elapsed)
    } else {
        None
    };
    let eta = match speed {
        Some(s) if elapsed > 3.0 => Some(((runner.total_seconds - done_seconds) / s).max(0.0)),
        _ => None,
    };
    let pending = if runner.running {
        0
    } else {
        project.clips_needing_analysis().len()
    };
    AnalysisDto {
        running: runner.running,
        stopping: runner.running && runner.cancel.load(Ordering::Relaxed),
        clips_total: runner.clips_total,
        clips_done: runner.clips_done,
        active: runner.active.len(),
        fraction,
        elapsed,
        eta_seconds: eta,
        speed,
        pending,
        probing,
    }
}

pub fn export_dto(project: &Project) -> ExportDto {
    let rate = project.timeline_rate();
    ExportDto {
        timeline_rate: rate_option(rate),
        rate_is_auto: project.export.timeline_rate.is_none(),
        auto_rate_label: project.most_common_rate().label(),
        drop_frame: project.timeline_drop_frame(),
        layout: project.export.layout,
        timeline_name: project.export.timeline_name.clone(),
        rates: TIMELINE_RATES.iter().map(|r| rate_option(*r)).collect(),
        resolution: project.most_common_resolution(),
    }
}

impl AppState {
    pub fn snapshot(&self) -> StateDto {
        let project = self.project.lock().unwrap();
        let runner = self.runner.lock().unwrap();
        let (selections, summary) = project.select_all();
        let path = self.project_path.lock().unwrap().clone();
        StateDto {
            clips: clip_rows(&project, &selections, &runner),
            summary,
            selection: project.selection.clone(),
            sample_rate: project.sample_rate,
            export: export_dto(&project),
            analysis: analysis_dto(&project, &runner, self.probing.load(Ordering::Relaxed)),
            project_name: path
                .as_ref()
                .and_then(|p| p.file_stem())
                .map(|s| s.to_string_lossy().to_string()),
            project_path: path.map(|p| p.to_string_lossy().to_string()),
            dirty: self.dirty.load(Ordering::Relaxed),
            fallback_profile: project.fallback_profile,
            profiles: profile_options(),
            finished_videos: {
                let searchable = selects_engine::taste::clips_with_prints(&project);
                project
                    .finished_videos
                    .iter()
                    .map(|f| FinishedDto {
                        name: f.name.clone(),
                        path: f.path.clone(),
                        duration: f.duration,
                        shots: f.shots(),
                        traced: f.traced(),
                        stale: f.clips_searched < searchable,
                    })
                    .collect()
            },
            taste: if project.finished_videos.is_empty() {
                None
            } else {
                let report = taste_report(&project);
                Some(TasteDto {
                    miss_words: miss_words(&report.misses),
                    report,
                })
            },
            tracing: self.tracing.lock().unwrap().as_ref().map(|(n, d, t)| TracingDto {
                name: n.clone(),
                done: *d,
                total: *t,
            }),
        }
    }
}
