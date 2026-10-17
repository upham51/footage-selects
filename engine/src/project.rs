//! The project file: clips, their cached per-frame measurements, and settings.
//!
//! Saved as JSON. Measurements are stored raw, so selection settings and
//! scoring can change freely after loading. Only a different sample rate (or
//! a changed source file) calls for analysing a clip again.

use crate::analyze::{ClipAnalysis, ANALYSIS_VERSION, DEFAULT_ANALYSIS_WIDTH, DEFAULT_SAMPLE_RATE};
use crate::probe::{file_stamp, ClipInfo};
use crate::rate::Rate;
use crate::scores::{score_clip, ScoreSeries};
use crate::selection::{select_clip, ClipSelection, SelectionSettings, Summary};
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
}

impl Default for ExportSettings {
    fn default() -> Self {
        ExportSettings {
            timeline_rate: None,
            drop_frame: None,
            layout: TimelineLayout::FullClipsBelow,
            timeline_name: "Selects".into(),
            start_timecode: "01:00:00:00".into(),
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

    pub fn scores(&self) -> Option<ScoreSeries> {
        self.analysis.as_ref().filter(|a| !a.frames.is_empty()).map(score_clip)
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

    pub fn select(&self, settings: &SelectionSettings) -> ClipSelection {
        match self.scores() {
            Some(s) => select_clip(&s, self.measured_until(), settings),
            None => ClipSelection::default(),
        }
    }
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
                "This file is not a Footage Selects project, or it was damaged. Nothing in it was changed."
            ),
            ProjectError::TooNew(v) => write!(
                f,
                "This project was saved by a newer version of Footage Selects (format {v}). Update the app to open it. Nothing in it was changed."
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

    /// Selection for every clip, in project order, plus the summary line.
    pub fn select_all(&self) -> (Vec<ClipSelection>, Summary) {
        let mut out = Vec::with_capacity(self.clips.len());
        let mut sum = Summary {
            clips_total: self.clips.len(),
            ..Default::default()
        };
        for c in &self.clips {
            let sel = c.select(&self.selection);
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
