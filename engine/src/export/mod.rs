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
    /// Which way the camera moves, when it does.
    pub camera: crate::selection::CameraMove,
    /// Stars given by hand, 0 to 5.
    pub stars: u8,
    /// Kept by hand rather than by the settings.
    pub manual: bool,
    /// The scene group, such as "Day 1, morning".
    pub group: Option<String>,
    pub light: Option<crate::solar::Light>,
    /// Problems worth knowing, as plain sentences.
    pub problems: Vec<String>,
    /// Words to search or collect by in the editor.
    pub keywords: Vec<String>,
    /// Timeline frames from where the placed clip starts to where the select
    /// itself starts (the extra time added before it).
    pub lead: i64,
    /// Timeline frames of extra time after the select.
    pub tail: i64,
}

impl SelectMeta {
    pub fn marker_name(&self) -> String {
        let mut s = format!("Select {}: {}", self.number, self.tier.word());
        if self.stars > 0 {
            s.push_str(&format!(", {} {}", self.stars, if self.stars == 1 { "star" } else { "stars" }));
        }
        if self.manual {
            s.push_str(", kept by hand");
        }
        s
    }
    /// Keywords joined for fields that hold one line of text.
    pub fn keyword_line(&self) -> String {
        self.keywords.join(", ")
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
        use crate::selection::CameraMove;
        match self.camera {
            CameraMove::Still | CameraMove::Other if self.camera_move => s.push_str(" Camera move."),
            CameraMove::Still | CameraMove::Other => {}
            m => {
                s.push(' ');
                s.push_str(m.words());
                s.push('.');
            }
        }
        if let Some(l) = self.light {
            s.push(' ');
            s.push_str(l.words());
            s.push('.');
        }
        for p in &self.problems {
            s.push(' ');
            s.push_str(p);
            s.push('.');
        }
        s
    }
}

/// Keywords for a select: strength, stars, camera movement, light, scene,
/// talking and problems, in that order.
pub fn select_keywords(seg: &crate::selection::Segment, tier: Tier, light: Option<crate::solar::Light>, group: Option<&str>) -> Vec<String> {
    use crate::selection::CameraMove;
    let mut k: Vec<String> = Vec::new();
    let mut w = tier.word().to_string();
    w[..1].make_ascii_uppercase();
    k.push(w);
    if seg.stars > 0 {
        k.push(format!("{} {}", seg.stars, if seg.stars == 1 { "star" } else { "stars" }));
    }
    if seg.manual {
        k.push("Kept by hand".into());
    }
    k.push(match seg.camera {
        CameraMove::Still if seg.camera_move => "Camera move".into(),
        CameraMove::Other => "Camera move".into(),
        m => m.words().to_string(),
    });
    if matches!(seg.camera, CameraMove::PanLeft | CameraMove::PanRight) {
        k.push("Pan".into());
    }
    if matches!(seg.camera, CameraMove::TiltUp | CameraMove::TiltDown) {
        k.push("Tilt".into());
    }
    if let Some(l) = light {
        k.push(l.words().to_string());
    }
    if let Some(g) = group {
        k.push(g.to_string());
    }
    if seg.talk >= 0.3 {
        k.push("Talking".into());
    }
    if seg.crooked() {
        k.push("Crooked horizon".into());
    }
    if seg.flickers() {
        k.push("Flicker".into());
    }
    if seg.jello() {
        k.push("Jello".into());
    }
    for i in &seg.sound {
        k.push(i.words().to_string());
    }
    k
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
    /// A music track under the whole timeline (reels).
    pub music: Option<Music>,
    /// Whether the selects carry their own sound when they are the only clips.
    pub select_audio: bool,
    /// The timeline is a different shape from the footage, so clips are
    /// scaled to fill it (cut from the centre).
    pub fill: bool,
}

/// An audio file laid under the timeline from its start.
#[derive(Clone, Debug)]
pub struct Music {
    pub path: String,
    pub name: String,
    pub sample_rate: u32,
    pub channels: u32,
    /// Length of the file in timeline frames.
    pub frames_total: i64,
    /// Length used on the timeline, in timeline frames.
    pub frames_used: i64,
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
        music: None,
        select_audio: true,
        fill: false,
    };
    let scenes = crate::scene::resolve(project);
    let handles = rate.seconds_to_frames(project.export.handles.clamp(0.0, 10.0) as f64);
    let mut cursor: i64 = 0;
    let (mut missing, mut unanalysed, mut partial) = (0, 0, 0);
    for ((clip, sel), scene) in project.clips.iter().zip(selections.iter()).zip(scenes.iter()) {
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
        // Extra time around each select, never reaching into the next select
        // of the same clip, so selects above their source never overlap.
        let spans: Vec<(i64, i64)> = kept
            .iter()
            .map(|seg| {
                let a = rate.seconds_to_frames(seg.start).clamp(0, frames_timeline - 1);
                let b = rate.seconds_to_frames(seg.end).clamp(a + 1, frames_timeline);
                (a, b)
            })
            .collect();
        for (k, seg) in kept.iter().enumerate() {
            let (sel_in, sel_out) = spans[k];
            let lo = if k > 0 && tl.layout == TimelineLayout::FullClipsBelow { (spans[k - 1].1 + sel_in + 1) / 2 } else { 0 };
            let hi = if k + 1 < spans.len() && tl.layout == TimelineLayout::FullClipsBelow { (sel_out + spans[k + 1].0) / 2 } else { frames_timeline };
            let src_in = (sel_in - handles).max(lo).max(0).min(sel_in);
            let src_out = (sel_out + handles).min(hi).min(frames_timeline).max(sel_out);
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
            let tier = Tier::from_score(seg.score);
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
                    tier,
                    exposure_note: profile_note(profile, seg.key_stops),
                    camera_move: seg.camera_move,
                    camera: seg.camera,
                    stars: seg.stars,
                    manual: seg.manual,
                    group: scene.group.clone(),
                    light: scene.light,
                    problems: seg.problem_notes(),
                    keywords: select_keywords(seg, tier, scene.light, scene.group.as_deref()),
                    lead: sel_in - src_in,
                    tail: src_out - sel_out,
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
    write_timeline(&tl, base, warnings)
}

/// Write the four files for any timeline.
pub fn write_timeline(tl: &Timeline, base: &Path, warnings: Vec<String>) -> Result<Written, ExportError> {
    let outputs = [
        ("xml", fcp7::write(tl)),
        ("fcpxml", fcpxml::write(tl)),
        ("edl", edl::write(tl)),
        ("csv", csv::write(tl)),
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

/// The timeline size for a reel shape, from the footage's most common size.
pub fn reel_size(project: &Project, shape: crate::reel::ReelShape) -> (u32, u32) {
    use crate::reel::ReelShape;
    let (w, h) = project.most_common_resolution();
    let short = w.min(h);
    let even = |v: u32| v / 2 * 2;
    match shape {
        ReelShape::Landscape => (w.max(h), short),
        ReelShape::Vertical => (short, even(short * 16 / 9)),
        ReelShape::Square => (short, short),
    }
}

/// A timeline of a planned reel: the shots back to back with the music under them.
pub fn build_reel_timeline(project: &Project, plan: &crate::reel::ReelPlan, music: &crate::beats::MusicInfo, shape: crate::reel::ReelShape, name: &str) -> Timeline {
    let rate = project.timeline_rate();
    let drop_frame = project.timeline_drop_frame();
    let start_frame = timecode_to_frames(&project.export.start_timecode, rate, drop_frame)
        .unwrap_or_else(|| timecode_to_frames("01:00:00:00", rate, drop_frame).unwrap());
    let (width, height) = reel_size(project, shape);
    let footage = project.most_common_resolution();
    let scenes = crate::scene::resolve(project);
    let mut tl = Timeline {
        name: name.to_string(),
        rate,
        drop_frame,
        start_frame,
        width,
        height,
        layout: TimelineLayout::SelectsOnly,
        sources: Vec::new(),
        base: Vec::new(),
        selects: Vec::new(),
        duration: 0,
        music: None,
        select_audio: false,
        fill: (width as u64 * footage.1 as u64) != (height as u64 * footage.0 as u64),
    };
    let mut source_of: Vec<(u64, usize)> = Vec::new();
    for (n, shot) in plan.shots.iter().enumerate() {
        let Some(clip) = project.clip(shot.clip_id) else { continue };
        let Some(info) = clip.info.clone() else { continue };
        let si = match source_of.iter().find(|(id, _)| *id == shot.clip_id) {
            Some((_, si)) => *si,
            None => {
                let tc_drop = info.timecode.as_deref().map(crate::rate::is_drop_frame_string).unwrap_or(false);
                let tc_frames = info.timecode.as_deref().and_then(|tc| timecode_to_frames(tc, info.rate, tc_drop)).unwrap_or(0);
                let frames_native = media_frames(info.duration, info.rate);
                let frames_timeline = media_frames(info.rate.frames_to_seconds(frames_native), rate);
                tl.sources.push(Source { info, tc_frames, tc_drop, frames_native, frames_timeline });
                source_of.push((shot.clip_id, tl.sources.len() - 1));
                tl.sources.len() - 1
            }
        };
        let src = &tl.sources[si];
        let record_in = rate.seconds_to_frames(shot.at);
        let record_out = rate.seconds_to_frames(shot.at + shot.length);
        let duration = (record_out - record_in).max(1);
        let source_in = rate.seconds_to_frames(shot.source_start).clamp(0, (src.frames_timeline - duration).max(0));
        let native = src.info.rate.seconds_to_frames(rate.frames_to_seconds(source_in)).clamp(0, src.frames_native - 1);
        let seg = crate::selection::Segment {
            start: shot.source_start,
            end: shot.source_end,
            score: shot.score,
            camera: shot.camera,
            camera_move: shot.camera != crate::selection::CameraMove::Still,
            stars: shot.stars,
            manual: shot.manual,
            ..Default::default()
        };
        let scene = scenes.iter().find(|s| s.id == shot.clip_id);
        let tier = Tier::from_score(shot.score);
        tl.selects.push(Placed {
            source: si,
            record_in,
            duration,
            source_in,
            source_in_native: native,
            select: Some(SelectMeta {
                number: n + 1,
                start_seconds: shot.source_start,
                end_seconds: shot.source_end,
                focus: 0.0,
                exposure: 0.0,
                stability: 0.0,
                score: shot.score,
                tier,
                exposure_note: None,
                camera_move: seg.camera_move,
                camera: shot.camera,
                stars: shot.stars,
                manual: shot.manual,
                group: scene.and_then(|s| s.group.clone()),
                light: scene.and_then(|s| s.light),
                problems: Vec::new(),
                keywords: select_keywords(&seg, tier, scene.and_then(|s| s.light), scene.and_then(|s| s.group.as_deref())),
                lead: 0,
                tail: 0,
            }),
        });
        tl.duration = tl.duration.max(record_in + duration);
    }
    // Scores for the markers, from the select each shot came from.
    let (selections, _) = project.select_all();
    for p in tl.selects.iter_mut() {
        let clip_index = project.clips.iter().position(|c| Some(c.path.as_str()) == Some(tl.sources[p.source].info.path.as_str()));
        if let (Some(ci), Some(m)) = (clip_index, p.select.as_mut()) {
            if let Some(g) = selections[ci].segments.iter().find(|g| g.start <= m.start_seconds + 0.05 && g.end >= m.start_seconds - 0.05 && g.status == crate::selection::SegmentStatus::Kept) {
                m.focus = g.focus;
                m.exposure = g.exposure;
                m.stability = g.stability;
            } else {
                m.focus = m.score;
                m.exposure = m.score;
                m.stability = m.score;
            }
        }
    }
    let total = media_frames(music.duration, rate);
    tl.music = Some(Music {
        path: music.path.clone(),
        name: music.name.clone(),
        sample_rate: music.sample_rate,
        channels: music.channels,
        frames_total: total,
        frames_used: tl.duration.min(total),
    });
    tl
}
