//! The project file: clips, their cached per-frame measurements, and settings.
//!
//! Saved as JSON. Measurements are stored raw, so selection settings and
//! scoring can change freely after loading. Only a different sample rate (or
//! a changed source file) calls for analysing a clip again.

use crate::analyze::{ClipAnalysis, ANALYSIS_VERSION, DEFAULT_ANALYSIS_WIDTH, DEFAULT_SAMPLE_RATE};
use crate::probe::{file_stamp, ClipInfo};
use crate::rate::Rate;
use crate::scores::{score_clip_with, ScoreSeries, ToneContext};
use crate::tone::{looks_like_log, ToneProfile};
use crate::selection::{apply_cap, select_candidates_with, AsideReason, ClipExtras, ClipSelection, SegmentStatus, SelectionSettings, SimilarTo, Summary};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Bump when the file layout changes in a way older versions cannot read.
pub const PROJECT_FORMAT: u32 = 1;
pub const PROJECT_EXTENSION: &str = "selects";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TimelineLayout {
    /// Every clip in full on track 1, each select directly above its source on track 2.
    #[default]
    FullClipsBelow,
    /// Only the selects, back to back on track 2.
    SelectsOnly,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ExportSettings {
    /// None means the most common frame rate in the footage.
    pub timeline_rate: Option<Rate>,
    /// None means drop frame when most 29.97/59.94 clips carry drop frame timecode.
    pub drop_frame: Option<bool>,
    pub layout: TimelineLayout,
    pub timeline_name: String,
    pub start_timecode: String,
    /// Extra seconds of footage before and after each select, for transitions.
    pub handles: f32,
    /// Also write a still of the best frame of every select and a contact sheet.
    pub stills: bool,
}

impl Default for ExportSettings {
    fn default() -> Self {
        ExportSettings {
            timeline_rate: None,
            drop_frame: None,
            layout: TimelineLayout::FullClipsBelow,
            timeline_name: "Selects".into(),
            start_timecode: "01:00:00:00".into(),
            handles: 0.0,
            stills: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProjectClip {
    pub id: u64,
    pub path: String,
    pub info: Option<ClipInfo>,
    /// Why the clip could not be read or analysed, in plain words.
    pub error: Option<String>,
    pub analysis: Option<ClipAnalysis>,
    /// A profile chosen by hand for this clip, overriding everything else.
    #[serde(default)]
    pub profile: Option<ToneProfile>,
    /// Calls made by hand: stretches kept (with stars) or dropped.
    #[serde(default)]
    pub marks: Vec<crate::selection::Mark>,
}

/// Where a clip's picture profile came from.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProfileSource {
    /// Chosen by hand for this clip.
    Chosen,
    /// Named by the file's tags or a camera sidecar.
    Camera,
    /// The project's choice for clips that do not say.
    Project,
    /// Recognised from the clip's tones.
    Picture,
    /// Nothing suggests log.
    Default,
}

/// A clip's profile and how it was decided.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ResolvedProfile {
    pub profile: ToneProfile,
    pub source: ProfileSource,
    /// Plain words, for example "Named by the camera's file C0001M01.XML".
    pub explanation: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    Present,
    Missing,
    /// The file is there but its size or date differs from when it was analysed.
    Changed,
}

impl ProjectClip {
    pub fn file_status(&self) -> FileStatus {
        let p = Path::new(&self.path);
        if !p.exists() {
            return FileStatus::Missing;
        }
        match &self.info {
            Some(info) => {
                let (size, modified) = file_stamp(p);
                if size != info.size || modified != info.modified {
                    FileStatus::Changed
                } else {
                    FileStatus::Present
                }
            }
            None => FileStatus::Present,
        }
    }

    /// True when cached measurements are missing, partial, out of date, or
    /// taken at a different sample rate.
    pub fn needs_analysis(&self, sample_rate: f64) -> bool {
        if self.info.is_none() {
            return false;
        }
        match &self.analysis {
            None => true,
            Some(a) => {
                !a.complete
                    || a.version != ANALYSIS_VERSION
                    || (a.sample_rate - sample_rate).abs() > 1e-9
                    || self.file_status() == FileStatus::Changed
            }
        }
    }

    /// Which profile to judge exposure with. In order: chosen for this clip,
    /// named by the camera, the project's choice for clips that do not say,
    /// recognised from the tones, standard.
    pub fn resolved_profile(&self, fallback: Option<ToneProfile>) -> ResolvedProfile {
        if let Some(p) = self.profile {
            return ResolvedProfile {
                profile: p,
                source: ProfileSource::Chosen,
                explanation: "Chosen for this clip.".into(),
            };
        }
        if let Some(d) = self.info.as_ref().and_then(|i| i.detected_profile.as_ref()) {
            return ResolvedProfile {
                profile: d.profile,
                source: ProfileSource::Camera,
                explanation: format!("Named by {}.", d.source),
            };
        }
        if let Some(p) = fallback {
            return ResolvedProfile {
                profile: p,
                source: ProfileSource::Project,
                explanation: "The project's choice for clips whose camera does not say.".into(),
            };
        }
        if let Some(a) = &self.analysis {
            if a.frames.has_ladder() && looks_like_log(&a.frames.ladder, &a.frames.vmax) {
                return ResolvedProfile {
                    profile: ToneProfile::GenericLog,
                    source: ProfileSource::Picture,
                    explanation: "The camera does not say, but the tones look like log. Choose the exact profile for more accurate exposure.".into(),
                };
            }
        }
        ResolvedProfile {
            profile: ToneProfile::Standard,
            source: ProfileSource::Default,
            explanation: "Nothing in the file or its tones suggests log.".into(),
        }
    }

    pub fn tone(&self, fallback: Option<ToneProfile>) -> ToneContext {
        ToneContext {
            profile: self.resolved_profile(fallback).profile,
            full_range: self.info.as_ref().map(|i| i.full_range).unwrap_or(false),
        }
    }

    /// Scores, judging exposure by the clip's resolved profile.
    pub fn scores(&self, fallback: Option<ToneProfile>) -> Option<ScoreSeries> {
        let tone = self.tone(fallback);
        self.analysis
            .as_ref()
            .filter(|a| !a.frames.is_empty())
            .map(|a| score_clip_with(a, tone))
    }

    /// How far into the clip the measurements reach.
    pub fn measured_until(&self) -> f64 {
        let duration = self.info.as_ref().map(|i| i.duration).unwrap_or(0.0);
        match &self.analysis {
            Some(a) if a.complete => duration,
            Some(a) => match a.frames.t.last() {
                Some(&t) => (t as f64 + 0.5 / a.sample_rate).min(duration.max(t as f64)),
                None => 0.0,
            },
            None => 0.0,
        }
    }

    /// Selection for this clip on its own. Repeated takes are found across
    /// clips, so use `Project::selections` where they matter.
    pub fn select(&self, settings: &SelectionSettings, fallback: Option<ToneProfile>) -> ClipSelection {
        let mut sel = self.candidates(settings, fallback);
        apply_cap(&mut sel.segments, settings.max_per_clip);
        sel
    }

    fn candidates(&self, settings: &SelectionSettings, fallback: Option<ToneProfile>) -> ClipSelection {
        match self.scores(fallback) {
            Some(s) => select_candidates_with(&s, self.measured_until(), settings, &self.extras()),
            None => ClipSelection::default(),
        }
    }

    /// Everything besides the scores that selection uses for this clip.
    pub fn extras(&self) -> ClipExtras<'_> {
        let a = self.analysis.as_ref();
        ClipExtras {
            frames: a.map(|a| &a.frames),
            sound: a.and_then(|a| a.sound.as_ref()),
            audio: a.and_then(|a| a.audio.as_ref()),
            marks: &self.marks,
        }
    }

    /// Selection for this clip without its calls made by hand, as the
    /// settings alone would choose.
    pub fn select_automatic(&self, settings: &SelectionSettings, fallback: Option<ToneProfile>) -> ClipSelection {
        let mut x = self.extras();
        x.marks = &[];
        let mut sel = match self.scores(fallback) {
            Some(s) => select_candidates_with(&s, self.measured_until(), settings, &x),
            None => ClipSelection::default(),
        };
        apply_cap(&mut sel.segments, settings.max_per_clip);
        sel
    }

    /// Whole-picture fingerprints at a quarter, half and three quarters of
    /// the way through `start..end`, or None for clips analysed before
    /// fingerprints were measured.
    pub fn fingerprints(&self, start: f64, end: f64) -> Option<[u64; 3]> {
        let f = &self.analysis.as_ref()?.frames;
        if !f.has_ladder() || f.fp.len() != f.len() {
            return None;
        }
        let at = |share: f64| {
            let t = (start + share * (end - start)) as f32;
            let i = f.t.partition_point(|&x| x < t).min(f.len() - 1);
            let j = if i > 0 && (f.t[i - 1] - t).abs() < (f.t[i] - t).abs() { i - 1 } else { i };
            f.fp[j][crate::fingerprint::HASH_FULL]
        };
        Some([at(0.25), at(0.5), at(0.75)])
    }
}

/// Mean fingerprint distance, out of 64 bits, under which two selects from
/// different clips count as the same shot taken again. Different shots of
/// the same place from another angle are usually 20 or more apart; retakes
/// of one framing are usually under 8.
pub const REPEAT_DISTANCE: f32 = 10.0;

/// Mean distance between two sets of fingerprints.
pub fn repeat_distance(a: &[u64; 3], b: &[u64; 3]) -> f32 {
    (0..3).map(|k| crate::fingerprint::distance(a[k], b[k]) as f32).sum::<f32>() / 3.0
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Project {
    pub format: u32,
    pub app_version: String,
    pub sample_rate: f64,
    pub analysis_width: u32,
    pub selection: SelectionSettings,
    pub export: ExportSettings,
    pub clips: Vec<ProjectClip>,
    pub next_id: u64,
    /// Profile for clips whose file and camera do not say. None means
    /// decide from each clip's tones.
    pub fallback_profile: Option<ToneProfile>,
    /// Finished videos traced back to these clips, to learn what gets used.
    pub finished_videos: Vec<crate::taste::FinishedVideo>,
    /// Where the footage was shot and the cameras' clock zone, for clips that do not say.
    pub places: crate::scene::PlaceSettings,
    /// The music reel being put together, if any.
    pub reel: crate::reel::ReelSettings,
}

impl Default for Project {
    fn default() -> Self {
        Project {
            format: PROJECT_FORMAT,
            app_version: env!("CARGO_PKG_VERSION").into(),
            sample_rate: DEFAULT_SAMPLE_RATE,
            analysis_width: DEFAULT_ANALYSIS_WIDTH,
            selection: SelectionSettings::default(),
            export: ExportSettings::default(),
            clips: Vec::new(),
            next_id: 1,
            fallback_profile: None,
            finished_videos: Vec::new(),
            places: Default::default(),
            reel: Default::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProjectError {
    Read(String),
    NotAProject,
    TooNew(u32),
    Write(String),
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectError::Read(e) => write!(f, "The project file could not be opened ({e}). Nothing in it was changed."),
            ProjectError::NotAProject => write!(
                f,
                "This file is not a {} project, or it was damaged. Nothing in it was changed.",
                crate::PRODUCT_NAME
            ),
            ProjectError::TooNew(v) => write!(
                f,
                "This project was saved by a newer version of {} (format {v}). Update the app to open it. Nothing in it was changed.",
                crate::PRODUCT_NAME
            ),
            ProjectError::Write(e) => write!(
                f,
                "The project could not be saved ({e}). Your work is still open in the app; try saving to a different folder."
            ),
        }
    }
}

impl Project {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a clip unless the same path is already in the project. Returns its id.
    pub fn add_clip(&mut self, path: &str, info: Result<ClipInfo, String>) -> Option<u64> {
        if self.clips.iter().any(|c| c.path == path) {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        let (info, error) = match info {
            Ok(i) => (Some(i), None),
            Err(e) => (None, Some(e)),
        };
        self.clips.push(ProjectClip {
            id,
            path: path.to_string(),
            info,
            error,
            analysis: None,
            profile: None,
            marks: Vec::new(),
        });
        Some(id)
    }

    pub fn clip(&self, id: u64) -> Option<&ProjectClip> {
        self.clips.iter().find(|c| c.id == id)
    }

    pub fn clip_mut(&mut self, id: u64) -> Option<&mut ProjectClip> {
        self.clips.iter_mut().find(|c| c.id == id)
    }

    pub fn remove_clip(&mut self, id: u64) {
        self.clips.retain(|c| c.id != id);
    }

    /// Ids of clips whose cached measurements cannot be used as they are.
    pub fn clips_needing_analysis(&self) -> Vec<u64> {
        self.clips
            .iter()
            .filter(|c| c.needs_analysis(self.sample_rate) && c.file_status() != FileStatus::Missing)
            .map(|c| c.id)
            .collect()
    }

    /// Changing the sample rate is the one setting that invalidates measurements.
    pub fn set_sample_rate(&mut self, rate: f64) {
        self.sample_rate = rate.clamp(0.5, 30.0);
    }

    /// Selection for every clip, in project order.
    ///
    /// Good segments from every clip are compared best first. A segment that
    /// looks like a better one from another clip notes it in `similar_to`,
    /// and with `set_aside_repeats` it is set aside, so the better take wins
    /// and the clip's next best segment can take its place under the limit.
    pub fn selections(&self) -> Vec<ClipSelection> {
        let scenes = if self.selection.light != crate::selection::LightFilter::Any { Some(crate::scene::resolve(self)) } else { None };
        self.selections_with(scenes.as_deref())
    }

    /// `selections` with the clips' scenes already worked out (needed for the light filter).
    pub fn selections_with(&self, scenes: Option<&[crate::scene::ClipScene]>) -> Vec<ClipSelection> {
        let s = &self.selection;
        let mut sels: Vec<ClipSelection> = self.clips.iter().map(|c| c.candidates(s, self.fallback_profile)).collect();
        // Filters that look beyond the picture. Stretches kept by hand are never set aside.
        for (ci, sel) in sels.iter_mut().enumerate() {
            let light = scenes.and_then(|sc| sc.get(ci)).and_then(|sc| sc.light);
            for g in sel.segments.iter_mut().filter(|g| g.status == SegmentStatus::Kept && !g.manual) {
                if s.light != crate::selection::LightFilter::Any && !s.light.allows(light) {
                    g.status = SegmentStatus::SetAside;
                    g.aside = Some(AsideReason::Light);
                } else if s.set_aside_sound_problems && !g.sound.is_empty() {
                    g.status = SegmentStatus::SetAside;
                    g.aside = Some(AsideReason::Sound);
                }
            }
        }
        // (clip index, segment index, score, fingerprints)
        let mut cands: Vec<(usize, usize, f32, [u64; 3])> = Vec::new();
        for (ci, (c, sel)) in self.clips.iter().zip(&sels).enumerate() {
            for (si, g) in sel.segments.iter().enumerate() {
                if g.status == SegmentStatus::Kept {
                    if let Some(fp) = c.fingerprints(g.start, g.end) {
                        // Stretches kept by hand come first, so they are never the repeat.
                        cands.push((ci, si, if g.manual { 10.0 + g.score } else { g.score }, fp));
                    }
                }
            }
        }
        cands.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)).then(a.1.cmp(&b.1)));
        let mut accepted: Vec<(usize, usize, [u64; 3])> = Vec::new();
        for (ci, si, _, fp) in cands {
            let twin = accepted
                .iter()
                .filter(|a| a.0 != ci)
                .map(|a| (a, repeat_distance(&a.2, &fp)))
                .filter(|(_, d)| *d <= REPEAT_DISTANCE)
                .min_by(|x, y| x.1.partial_cmp(&y.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(a, _)| (a.0, a.1));
            match twin {
                Some(_) if sels[ci].segments[si].manual => accepted.push((ci, si, fp)),
                Some((tc, ts)) => {
                    let other = &sels[tc].segments[ts];
                    let similar = SimilarTo { clip: self.clips[tc].id, start: other.start, end: other.end };
                    let g = &mut sels[ci].segments[si];
                    g.similar_to = Some(similar);
                    if s.set_aside_repeats {
                        g.status = SegmentStatus::SetAside;
                        g.aside = Some(AsideReason::Repeat);
                    }
                }
                None => accepted.push((ci, si, fp)),
            }
        }
        for sel in sels.iter_mut() {
            apply_cap(&mut sel.segments, s.max_per_clip);
        }
        sels
    }

    /// Selection for every clip, in project order, plus the summary line.
    pub fn select_all(&self) -> (Vec<ClipSelection>, Summary) {
        let sels = self.selections();
        let mut out = Vec::with_capacity(self.clips.len());
        let mut sum = Summary {
            clips_total: self.clips.len(),
            ..Default::default()
        };
        for (c, sel) in self.clips.iter().zip(sels) {
            if let Some(info) = &c.info {
                sum.footage_seconds += info.duration;
            }
            if c.info.is_none() || (c.error.is_some() && c.analysis.is_none()) {
                sum.clips_failed += 1;
            }
            if c.analysis.as_ref().map(|a| a.complete).unwrap_or(false) {
                sum.clips_analyzed += 1;
                sum.analyzed_seconds += c.info.as_ref().map(|i| i.duration).unwrap_or(0.0);
            }
            sum.selects_count += sel.kept().count();
            sum.selects_seconds += sel.kept_seconds();
            out.push(sel);
        }
        (out, sum)
    }

    /// The most common standard frame rate among readable clips, weighted by
    /// clip count; ties go to the rate with more total footage.
    pub fn most_common_rate(&self) -> Rate {
        let mut counts: Vec<(Rate, usize, f64)> = Vec::new();
        for info in self.clips.iter().filter_map(|c| c.info.as_ref()) {
            match counts.iter_mut().find(|(r, _, _)| *r == info.rate) {
                Some(e) => {
                    e.1 += 1;
                    e.2 += info.duration;
                }
                None => counts.push((info.rate, 1, info.duration)),
            }
        }
        counts.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then(b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal))
        });
        let r = counts.first().map(|c| c.0).unwrap_or(Rate::new(24000, 1001));
        // High frame rate footage is usually played back in a normal timeline.
        if crate::rate::TIMELINE_RATES.contains(&r) {
            r
        } else {
            Rate::snap(r.fps() / ((r.fps() / 30.0).round().max(1.0)))
        }
    }

    pub fn timeline_rate(&self) -> Rate {
        self.export.timeline_rate.unwrap_or_else(|| self.most_common_rate())
    }

    /// Drop frame on when chosen, otherwise when most clips at a drop frame
    /// capable rate carry drop frame timecode. Off for rates without drop frame.
    pub fn timeline_drop_frame(&self) -> bool {
        let rate = self.timeline_rate();
        if !rate.supports_drop_frame() {
            return false;
        }
        if let Some(d) = self.export.drop_frame {
            return d;
        }
        let (mut df, mut ndf) = (0, 0);
        for info in self.clips.iter().filter_map(|c| c.info.as_ref()) {
            if info.rate.supports_drop_frame() {
                match info.timecode.as_deref() {
                    Some(tc) if crate::rate::is_drop_frame_string(tc) => df += 1,
                    _ => ndf += 1,
                }
            }
        }
        df > ndf
    }

    /// The most common picture size, used for the exported timeline.
    pub fn most_common_resolution(&self) -> (u32, u32) {
        let mut counts: Vec<((u32, u32), usize)> = Vec::new();
        for info in self.clips.iter().filter_map(|c| c.info.as_ref()) {
            let key = (info.width, info.height);
            match counts.iter_mut().find(|(k, _)| *k == key) {
                Some(e) => e.1 += 1,
                None => counts.push((key, 1)),
            }
        }
        counts.sort_by(|a, b| b.1.cmp(&a.1).then((b.0 .0 * b.0 .1).cmp(&(a.0 .0 * a.0 .1))));
        counts.first().map(|c| c.0).unwrap_or((1920, 1080))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("project serialises")
    }

    pub fn from_json(text: &str) -> Result<Project, ProjectError> {
        let v: serde_json::Value = serde_json::from_str(text).map_err(|_| ProjectError::NotAProject)?;
        let format = v
            .get("format")
            .and_then(|f| f.as_u64())
            .ok_or(ProjectError::NotAProject)? as u32;
        if format > PROJECT_FORMAT {
            return Err(ProjectError::TooNew(format));
        }
        if v.get("clips").map(|c| !c.is_array()).unwrap_or(true) {
            return Err(ProjectError::NotAProject);
        }
        let mut p: Project = serde_json::from_value(v).map_err(|_| ProjectError::NotAProject)?;
        let max_id = p.clips.iter().map(|c| c.id).max().unwrap_or(0);
        p.next_id = p.next_id.max(max_id + 1);
        Ok(p)
    }

    /// Write the project atomically: a crash part way never leaves a half-written file.
    pub fn save(&self, path: &Path) -> Result<(), ProjectError> {
        let tmp = path.with_extension("selects-saving");
        std::fs::write(&tmp, self.to_json()).map_err(|e| ProjectError::Write(e.to_string()))?;
        std::fs::rename(&tmp, path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            ProjectError::Write(e.to_string())
        })
    }

    pub fn load(path: &Path) -> Result<Project, ProjectError> {
        let text = std::fs::read_to_string(path).map_err(|e| ProjectError::Read(e.to_string()))?;
        Project::from_json(&text)
    }
}
