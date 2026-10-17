//! Timeline export: Final Cut Pro 7 XML (Premiere Pro, DaVinci Resolve),
//! FCPXML (Final Cut Pro, DaVinci Resolve), CMX3600 EDL and CSV.
//!
//! All four writers share one timeline model, so frame and timecode maths is
//! done once. Every position and length on the timeline is a whole number of
//! timeline frames. Source positions are also whole frames, counted from the
//! first frame of the media.

pub mod csv;
pub mod edl;
pub mod fcp7;
pub mod fcpxml;
pub mod xml;

use crate::probe::ClipInfo;
use crate::project::{FileStatus, Project, TimelineLayout};
use crate::rate::{timecode_to_frames, Rate};
use crate::selection::ClipSelection;
use std::path::{Path, PathBuf};

/// How strong a select is, used for marker colors. Always shown with its word too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Strong,
    Good,
    Fair,
}

impl Tier {
    pub fn from_score(score: f32) -> Tier {
        if score >= 0.8 {
            Tier::Strong
        } else if score >= 0.65 {
            Tier::Good
        } else {
            Tier::Fair
        }
    }
    pub fn word(self) -> &'static str {
        match self {
            Tier::Strong => "strong",
            Tier::Good => "good",
            Tier::Fair => "fair",
        }
    }
}

#[derive(Clone, Debug)]
pub struct SelectMeta {
    /// 1-based, in timeline order.
    pub number: usize,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub focus: f32,
    pub exposure: f32,
    pub stability: f32,
    pub score: f32,
    pub tier: Tier,
    /// For log footage: the profile and where the middle tones sit, such as
    /// "S-Log3, middle tones -0.5 stops".
    pub exposure_note: Option<String>,
    pub camera_move: bool,
}

impl SelectMeta {
    pub fn marker_name(&self) -> String {
        format!("Select {}: {}", self.number, self.tier.word())
    }
    /// The marker note: the three scores on a 0 to 100 scale.
    pub fn marker_note(&self) -> String {
        let mut s = format!(
            "Focus {}, exposure {}, stability {} (overall {}).",
            pct(self.focus),
            pct(self.exposure),
            pct(self.stability),
            pct(self.score)
        );
        if let Some(n) = &self.exposure_note {
            s.push(' ');
            s.push_str(n);
            s.push('.');
        }
        if self.camera_move {
            s.push_str(" Camera move.");
        }
        s
    }
}

/// "S-Log3, middle tones -0.5 stops" for log footage, nothing for standard.
fn profile_note(profile: crate::tone::ToneProfile, key: Option<f32>) -> Option<String> {
    if !profile.is_log() {
        return None;
    }
    let label = profile.label();
    Some(match key {
        Some(k) => format!("{label}, middle tones {:+.1} stops", k),
        None => label.to_string(),
    })
}

pub fn pct(v: f32) -> i32 {
    (v * 100.0).round() as i32
}

/// One source file used by the timeline.
#[derive(Clone, Debug)]
pub struct Source {
    pub info: ClipInfo,
    /// Start timecode of the media as a frame count at the clip's own rate.
    pub tc_frames: i64,
    pub tc_drop: bool,
    /// Media length in the clip's own frames.
    pub frames_native: i64,
    /// Media length in timeline frames.
    pub frames_timeline: i64,
}

impl Source {
    pub fn native_rate(&self) -> Rate {
        self.info.rate
    }
    pub fn same_rate_as(&self, r: Rate) -> bool {
        self.info.rate == r
    }
}

/// A clip placed on a track.
#[derive(Clone, Debug)]
pub struct Placed {
    pub source: usize,
    /// Timeline frames from the start of the timeline.
    pub record_in: i64,
    pub duration: i64,
    /// Timeline frames from the first frame of the media.
    pub source_in: i64,
    /// The same point in the clip's own frames.
    pub source_in_native: i64,
    pub select: Option<SelectMeta>,
}

impl Placed {
    pub fn record_out(&self) -> i64 {
        self.record_in + self.duration
    }
    pub fn source_out(&self) -> i64 {
        self.source_in + self.duration
    }
}

#[derive(Clone, Debug)]
pub struct Timeline {
    pub name: String,
    pub rate: Rate,
    pub drop_frame: bool,
    /// Timeline start timecode as a frame count, e.g. 01:00:00:00.
    pub start_frame: i64,
    pub width: u32,
    pub height: u32,
    pub layout: TimelineLayout,
    pub sources: Vec<Source>,
    /// Track 1: full clips (empty when exporting selects only).
    pub base: Vec<Placed>,
    /// Track 2: the selects.
    pub selects: Vec<Placed>,
    pub duration: i64,
}

/// Frames of media at `rate`, rounded down so the last frame never runs past the end.
fn media_frames(seconds: f64, rate: Rate) -> i64 {
    ((seconds * rate.num as f64 / rate.den as f64) + 1e-6).floor().max(1.0) as i64
}

/// Build the timeline for a project from its current selection.
pub fn build_timeline(project: &Project, selections: &[ClipSelection]) -> (Timeline, Vec<String>) {
    let rate = project.timeline_rate();
    let drop_frame = project.timeline_drop_frame();
    let start_frame = timecode_to_frames(&project.export.start_timecode, rate, drop_frame)
        .unwrap_or_else(|| timecode_to_frames("01:00:00:00", rate, drop_frame).unwrap());
    let (width, height) = project.most_common_resolution();
    let mut warnings = Vec::new();
    let mut tl = Timeline {
        name: project.export.timeline_name.clone(),
        rate,
        drop_frame,
        start_frame,
        width,
        height,
        layout: project.export.layout,
        sources: Vec::new(),
        base: Vec::new(),
        selects: Vec::new(),
        duration: 0,
    };
    let mut cursor: i64 = 0;
    let (mut missing, mut unanalysed, mut partial) = (0, 0, 0);
    for (clip, sel) in project.clips.iter().zip(selections.iter()) {
        let Some(info) = clip.info.clone() else { continue };
        let profile = clip.resolved_profile(project.fallback_profile).profile;
        match &clip.analysis {
            None => unanalysed += 1,
            Some(a) if !a.complete => partial += 1,
            _ => {}
        }
        if clip.file_status() == FileStatus::Missing {
            missing += 1;
        }
        let kept: Vec<_> = sel.kept().collect();
        if tl.layout == TimelineLayout::SelectsOnly && kept.is_empty() {
            continue;
        }
        let tc_drop = info
            .timecode
            .as_deref()
            .map(crate::rate::is_drop_frame_string)
            .unwrap_or(false);
        let tc_frames = info
            .timecode
            .as_deref()
            .and_then(|tc| timecode_to_frames(tc, info.rate, tc_drop))
            .unwrap_or(0);
        // Length in whole native frames, then in whole timeline frames that fit
        // inside it, so a clip never runs past the end of its media.
        let frames_native = media_frames(info.duration, info.rate);
        let frames_timeline = media_frames(info.rate.frames_to_seconds(frames_native), rate);
        let source = Source {
            frames_native,
            frames_timeline,
            tc_frames,
            tc_drop,
            info,
        };
        let si = tl.sources.len();
        let clip_start = cursor;
        if tl.layout == TimelineLayout::FullClipsBelow {
            tl.base.push(Placed {
                source: si,
                record_in: clip_start,
                duration: frames_timeline,
                source_in: 0,
                source_in_native: 0,
                select: None,
            });
            cursor += frames_timeline;
        }
        for seg in kept {
            let src_in = rate.seconds_to_frames(seg.start).clamp(0, frames_timeline - 1);
            let src_out = rate.seconds_to_frames(seg.end).clamp(src_in + 1, frames_timeline);
            let duration = src_out - src_in;
            let record_in = match tl.layout {
                TimelineLayout::FullClipsBelow => clip_start + src_in,
                TimelineLayout::SelectsOnly => {
                    let r = cursor;
                    cursor += duration;
                    r
                }
            };
            // The same source point in the clip's own frames, taken from the
            // quantised timeline position so both describe one moment.
            let native = source
                .info
                .rate
                .seconds_to_frames(rate.frames_to_seconds(src_in))
                .clamp(0, source.frames_native - 1);
            tl.selects.push(Placed {
                source: si,
                record_in,
                duration,
                source_in: src_in,
                source_in_native: native,
                select: Some(SelectMeta {
                    number: 0,
                    start_seconds: seg.start,
                    end_seconds: seg.end,
                    focus: seg.focus,
                    exposure: seg.exposure,
                    stability: seg.stability,
                    score: seg.score,
                    tier: Tier::from_score(seg.score),
                    exposure_note: profile_note(profile, seg.key_stops),
                    camera_move: seg.camera_move,
                }),
            });
        }
        tl.sources.push(source);
    }
    tl.selects.sort_by_key(|p| p.record_in);
    for (i, p) in tl.selects.iter_mut().enumerate() {
        if let Some(m) = p.select.as_mut() {
            m.number = i + 1;
        }
    }
    tl.duration = cursor;
    if missing > 0 {
        warnings.push(format!(
            "{missing} {} missing from {} folder. The timeline points to where {} used to be; relink in your editor.",
            if missing == 1 { "clip is" } else { "clips are" },
            if missing == 1 { "its" } else { "their" },
            if missing == 1 { "it" } else { "they" }
        ));
    }
    if unanalysed > 0 {
        warnings.push(format!(
            "{unanalysed} {} not analyzed yet, so {} no selects.",
            if unanalysed == 1 { "clip was" } else { "clips were" },
            if unanalysed == 1 { "it has" } else { "they have" }
        ));
    }
    if partial > 0 {
        warnings.push(format!(
            "{partial} {} only partly analyzed. Selects from the analyzed part are included.",
            if partial == 1 { "clip was" } else { "clips were" }
        ));
    }
    (tl, warnings)
}

/// A file:// URL for a local path, percent-encoded. Windows drive paths become file:///C:/...
/// A select's source in and out as timecode at the timeline rate, the way the EDL
/// gives them. A clip at another rate has its start timecode and the select's
/// offset both converted to the timeline rate.
pub fn source_timecodes_at_timeline_rate(tl: &Timeline, p: &Placed) -> (String, String) {
    use crate::rate::frames_to_timecode;
    let r = tl.rate;
    let s = &tl.sources[p.source];
    if s.same_rate_as(r) {
        let a = s.tc_frames + p.source_in_native;
        let b = a + p.duration;
        (frames_to_timecode(a, r, s.tc_drop), frames_to_timecode(b, r, s.tc_drop))
    } else {
        let drop = tl.drop_frame && r.supports_drop_frame();
        let tc_secs = s.native_rate().frames_to_seconds(s.tc_frames);
        let a = r.seconds_to_frames(tc_secs) + p.source_in;
        let b = a + p.duration;
        (frames_to_timecode(a, r, drop), frames_to_timecode(b, r, drop))
    }
}

pub fn file_url(path: &str, localhost: bool) -> String {
    let mut p = path.replace('\\', "/");
    if p.len() >= 2 && p.as_bytes()[1] == b':' {
        p = format!("/{p}");
    }
    if !p.starts_with('/') {
        p = format!("/{p}");
    }
    let mut out = String::from(if localhost { "file://localhost" } else { "file://" });
    for b in p.bytes() {
        let keep = b.is_ascii_alphanumeric() || b"-._~/:".contains(&b);
        if keep {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub struct Written {
    pub files: Vec<PathBuf>,
    pub selects: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub struct ExportError(pub String);

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Write `<base>.xml`, `<base>.fcpxml`, `<base>.edl` and `<base>.csv`.
pub fn write_all(project: &Project, base: &Path) -> Result<Written, ExportError> {
    let (selections, _) = project.select_all();
    let (tl, warnings) = build_timeline(project, &selections);
    if tl.selects.is_empty() {
        return Err(ExportError(
            "There are no selects to export, so nothing was written. Lower a threshold or analyze more footage, then export again.".into(),
        ));
    }
    let outputs = [
        ("xml", fcp7::write(&tl)),
        ("fcpxml", fcpxml::write(&tl)),
        ("edl", edl::write(&tl)),
        ("csv", csv::write(&tl)),
    ];
    let mut files = Vec::new();
    for (ext, text) in outputs {
        let path = PathBuf::from(format!("{}.{ext}", base.to_string_lossy()));
        std::fs::write(&path, text).map_err(|e| {
            ExportError(format!(
                "{} could not be written ({e}). {} Choose a folder you can write to and export again.",
                path.display(),
                if files.is_empty() {
                    "Nothing was written.".to_string()
                } else {
                    format!("{} of the 4 files were written before this.", files.len())
                }
            ))
        })?;
        files.push(path);
    }
    Ok(Written {
        files,
        selects: tl.selects.len(),
        warnings,
    })
}
