//! Reading clip metadata with ffprobe.

use crate::ffmpeg;
use crate::rate::Rate;
use crate::sidecar::{read_sidecars, CameraNotes};
use crate::tone::{profile_from_text, DetectedProfile, ToneProfile};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

/// What the app needs to know about a clip before and after analysis.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ClipInfo {
    pub path: String,
    pub name: String,
    /// File size in bytes, used with `modified` to notice changed files.
    pub size: u64,
    /// Modification time in seconds since the Unix epoch.
    pub modified: u64,
    pub duration: f64,
    pub start_time: f64,
    /// Display size after rotation and pixel aspect ratio.
    pub width: u32,
    pub height: u32,
    pub coded_width: u32,
    pub coded_height: u32,
    /// Clockwise display rotation in degrees: 0, 90, 180 or 270.
    pub rotation: i32,
    /// Measured average frames per second.
    pub avg_fps: f64,
    /// The standard rate this clip is treated as.
    pub rate: Rate,
    pub variable_frame_rate: bool,
    pub codec: String,
    /// Decoded pixel format, such as yuv420p or yuv422p10le.
    #[serde(default)]
    pub pix_fmt: String,
    pub has_audio: bool,
    pub audio_channels: u32,
    pub audio_sample_rate: u32,
    /// Source start timecode as written in the file, if any.
    pub timecode: Option<String>,
    /// The file stores full-range levels (0 to 255) instead of video levels.
    #[serde(default)]
    pub full_range: bool,
    /// The transfer characteristic tag, such as bt709 or arib-std-b67.
    #[serde(default)]
    pub color_transfer: String,
    /// The picture profile, when the file or a camera sidecar names it.
    #[serde(default)]
    pub detected_profile: Option<DetectedProfile>,
    /// What the camera wrote beside the clip: profile, ISO, shutter, place.
    #[serde(default)]
    pub camera: Option<CameraNotes>,
    /// When and where the file says it was recorded, as written in its tags.
    #[serde(default)]
    pub tags: RecordTags,
}

/// Recording time and place as the file's own tags give them. Kept as text
/// so how they are read can improve without reading the files again.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct RecordTags {
    /// The container's `creation_time`. Phones write true UTC here; many
    /// cameras write their own clock time with a UTC label.
    pub creation_time: Option<String>,
    /// Apple's creation date, which carries the time zone, such as 2024-05-02T18:01:23+0200.
    pub apple_creation: Option<String>,
    /// ISO 6709 position, such as +48.8584+002.2945+035.000/.
    pub location: Option<String>,
    /// Who made the file, from its tags, such as Apple or samsung.
    pub make: Option<String>,
    /// The file came from a phone, whose `creation_time` is true UTC.
    pub phone: bool,
    /// The container's `major_brand`, such as XAVC for Sony cameras.
    pub brand: Option<String>,
}

/// Recording time and place from the container's tags.
pub fn record_tags(json: &Value) -> RecordTags {
    let mut t = RecordTags::default();
    let mut pairs: Vec<(String, String)> = Vec::new();
    if let Some(Value::Object(m)) = json.get("format").and_then(|f| f.get("tags")) {
        for (k, v) in m {
            if let Some(v) = v.as_str() {
                pairs.push((k.to_ascii_lowercase(), v.trim().to_string()));
            }
        }
    }
    if let Some(streams) = json.get("streams").and_then(|s| s.as_array()) {
        for st in streams {
            if let Some(Value::Object(m)) = st.get("tags") {
                for (k, v) in m {
                    if let Some(v) = v.as_str() {
                        pairs.push((k.to_ascii_lowercase(), v.trim().to_string()));
                    }
                }
            }
        }
    }
    for (k, v) in &pairs {
        if v.is_empty() {
            continue;
        }
        match k.as_str() {
            "creation_time" if t.creation_time.is_none() => t.creation_time = Some(v.clone()),
            "com.apple.quicktime.creationdate" => t.apple_creation = Some(v.clone()),
            "com.apple.quicktime.location.iso6709" | "location" | "location-eng" if t.location.is_none() => t.location = Some(v.clone()),
            "com.apple.quicktime.make" | "make" | "com.android.manufacturer" if t.make.is_none() => t.make = Some(v.clone()),
            "major_brand" if t.brand.is_none() => t.brand = Some(v.clone()),
            _ => {}
        }
        if k.starts_with("com.apple.quicktime.") || k.starts_with("com.android.") {
            t.phone = true;
        }
    }
    t
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProbeError {
    /// ffprobe could not be started at all.
    ToolMissing(String),
    /// ffprobe ran but could not read the file.
    Unreadable(String),
    /// The file has no video stream.
    NoVideo,
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeError::ToolMissing(e) => write!(
                f,
                "The video reader that ships with the app could not start ({e}). Reinstalling the app usually fixes this."
            ),
            ProbeError::Unreadable(e) => {
                write!(f, "This file could not be read as video. {e}")
            }
            ProbeError::NoVideo => write!(f, "This file has no video picture, only audio or data."),
        }
    }
}

/// File extensions treated as video when a folder is added.
pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mov", "mp4", "m4v", "mxf", "mts", "m2ts", "avi", "mkv", "webm", "mpg", "mpeg", "3gp", "braw",
    "r3d", "insv", "lrv", "dng", "crm", "wmv", "flv", "ts", "vob", "dv",
];

pub fn is_video_path(p: &Path) -> bool {
    let Some(ext) = p.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    let ext = ext.to_ascii_lowercase();
    // Skip macOS resource fork files such as ._A001.mov.
    if p
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with("._"))
        .unwrap_or(false)
    {
        return false;
    }
    VIDEO_EXTENSIONS.contains(&ext.as_str())
}

/// Recursively collect video files from a mix of files and folders, sorted by path.
pub fn collect_video_files(paths: &[std::path::PathBuf]) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    fn walk(p: &Path, out: &mut Vec<std::path::PathBuf>, depth: usize) {
        if depth > 12 {
            return;
        }
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(p) {
                for e in rd.flatten() {
                    let name = e.file_name();
                    if name.to_string_lossy().starts_with('.') {
                        continue;
                    }
                    walk(&e.path(), out, depth + 1);
                }
            }
        } else if is_video_path(p) {
            out.push(p.to_path_buf());
        }
    }
    for p in paths {
        if p.is_file() {
            // Files dropped explicitly are accepted even with unusual extensions.
            out.push(p.clone());
        } else {
            walk(p, &mut out, 0);
        }
    }
    out.sort();
    out.dedup();
    out
}

pub fn file_stamp(path: &Path) -> (u64, u64) {
    match std::fs::metadata(path) {
        Ok(m) => {
            let modified = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            (m.len(), modified)
        }
        Err(_) => (0, 0),
    }
}

/// Run ffprobe on one file.
pub fn probe(path: &Path) -> Result<ClipInfo, ProbeError> {
    let out = ffmpeg::command("ffprobe")
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(path)
        .output()
        .map_err(|e| ProbeError::ToolMissing(e.to_string()))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err.lines().last().unwrap_or("").trim().to_string();
        return Err(ProbeError::Unreadable(friendly_ffmpeg_error(&line)));
    }
    let json: Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| ProbeError::Unreadable(e.to_string()))?;
    let (size, modified) = file_stamp(path);
    let mut info = parse_probe(&json, path, size, modified)?;
    if let Some(notes) = read_sidecars(path) {
        // A sidecar is the camera's own statement, so it wins over tags.
        if let Some(d) = notes.detected() {
            info.detected_profile = Some(d);
        }
        info.camera = Some(notes);
    }
    Ok(info)
}

/// A profile named in the file's own tags: HLG's transfer tag, or a log
/// name written by the camera or an app into any metadata text.
fn profile_from_tags(json: &Value, video: &Value) -> Option<DetectedProfile> {
    let trc = video.get("color_transfer").and_then(|v| v.as_str()).unwrap_or("");
    if trc == "arib-std-b67" {
        return Some(DetectedProfile {
            profile: ToneProfile::Hlg,
            source: "the file's colour tags".into(),
        });
    }
    let mut texts: Vec<String> = Vec::new();
    let mut collect = |v: Option<&Value>| {
        if let Some(Value::Object(m)) = v {
            for (k, val) in m {
                if let Some(t) = val.as_str() {
                    let k = k.to_ascii_lowercase();
                    // Encoder and handler names mention codecs, not profiles.
                    if !k.contains("encoder") && !k.contains("handler") && !k.contains("vendor") {
                        texts.push(t.to_string());
                    }
                }
            }
        }
    };
    collect(json.get("format").and_then(|f| f.get("tags")));
    collect(video.get("tags"));
    texts.iter().find_map(|t| {
        profile_from_text(t).filter(|p| p.is_log()).map(|profile| DetectedProfile {
            profile,
            source: "the file's metadata".into(),
        })
    })
}

/// Whether every frame of the clip's picture stands on its own (ProRes, DNxHR,
/// all-intra H.264 or HEVC and the like), judged from the first 32 packets.
/// Such clips can skip unwanted frames before decoding them at all.
pub fn is_intra_only(path: &Path) -> bool {
    let Ok(out) = ffmpeg::command("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-read_intervals", "%+#32"])
        .args(["-show_entries", "packet=flags", "-of", "csv=p=0"])
        .arg(path)
        .output()
    else {
        return false;
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let flags: Vec<&str> = text.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    out.status.success() && flags.len() >= 8 && flags.iter().all(|f| f.starts_with('K'))
}

/// Turn a raw ffmpeg error line into something a person can act on.
pub fn friendly_ffmpeg_error(line: &str) -> String {
    let l = line.to_ascii_lowercase();
    if l.contains("no such file") {
        "The file was moved or deleted. Put it back or remove the clip.".into()
    } else if l.contains("permission denied") {
        "The app is not allowed to open this file. Check the file's permissions.".into()
    } else if l.contains("invalid data") || l.contains("moov atom not found") {
        "The file looks damaged or incomplete, for example a recording that stopped early.".into()
    } else if line.is_empty() {
        "No further detail was given.".into()
    } else {
        format!("Detail from the video reader: {line}")
    }
}

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn rotation_of(stream: &Value) -> i32 {
    let mut rot: Option<f64> = None;
    if let Some(list) = stream.get("side_data_list").and_then(|v| v.as_array()) {
        for sd in list {
            if let Some(r) = sd.get("rotation").and_then(num) {
                rot = Some(r);
            }
        }
    }
    if rot.is_none() {
        rot = stream
            .get("tags")
            .and_then(|t| t.get("rotate"))
            .and_then(num);
    }
    // ffprobe reports the display matrix rotation counter-clockwise (negative = clockwise).
    let r = rot.unwrap_or(0.0).round() as i32;
    (((-r) % 360) + 360) % 360 / 90 * 90
}

pub fn parse_probe(json: &Value, path: &Path, size: u64, modified: u64) -> Result<ClipInfo, ProbeError> {
    let streams = json
        .get("streams")
        .and_then(|s| s.as_array())
        .cloned()
        .unwrap_or_default();
    let video = streams
        .iter()
        .filter(|s| s.get("codec_type").and_then(|c| c.as_str()) == Some("video"))
        .find(|s| {
            // Ignore cover art and thumbnails embedded as video streams.
            s.get("disposition")
                .and_then(|d| d.get("attached_pic"))
                .and_then(|a| a.as_i64())
                .unwrap_or(0)
                == 0
        })
        .ok_or(ProbeError::NoVideo)?;
    let audio = streams
        .iter()
        .find(|s| s.get("codec_type").and_then(|c| c.as_str()) == Some("audio"));
    let format = json.get("format").cloned().unwrap_or(Value::Null);

    let coded_width = video.get("width").and_then(num).unwrap_or(0.0) as u32;
    let coded_height = video.get("height").and_then(num).unwrap_or(0.0) as u32;
    if coded_width == 0 || coded_height == 0 {
        return Err(ProbeError::NoVideo);
    }
    let sar = video
        .get("sample_aspect_ratio")
        .and_then(|v| v.as_str())
        .and_then(|s| {
            let mut it = s.split(':');
            let a: f64 = it.next()?.parse().ok()?;
            let b: f64 = it.next()?.parse().ok()?;
            if a > 0.0 && b > 0.0 {
                Some(a / b)
            } else {
                None
            }
        })
        .unwrap_or(1.0);
    let rotation = rotation_of(video);
    let mut w = (coded_width as f64 * sar).round() as u32;
    let mut h = coded_height;
    if rotation == 90 || rotation == 270 {
        std::mem::swap(&mut w, &mut h);
    }

    let r_fps = video
        .get("r_frame_rate")
        .and_then(|v| v.as_str())
        .and_then(Rate::parse_ratio);
    let avg_fps = video
        .get("avg_frame_rate")
        .and_then(|v| v.as_str())
        .and_then(Rate::parse_ratio);
    let (rate, avg, vfr) = choose_rate(r_fps, avg_fps);

    // The picture's own duration: a container's can be longer when audio runs past the last frame.
    let duration = video
        .get("duration")
        .and_then(num)
        .filter(|d| *d > 0.0)
        .or_else(|| format.get("duration").and_then(num))
        .unwrap_or(0.0);
    let start_time = video
        .get("start_time")
        .and_then(num)
        .or_else(|| format.get("start_time").and_then(num))
        .unwrap_or(0.0);

    let timecode = video
        .get("tags")
        .and_then(|t| t.get("timecode"))
        .and_then(|v| v.as_str())
        .or_else(|| {
            format
                .get("tags")
                .and_then(|t| t.get("timecode"))
                .and_then(|v| v.as_str())
        })
        .or_else(|| {
            // QuickTime files often keep timecode on a separate data track.
            streams.iter().find_map(|s| {
                s.get("tags")
                    .and_then(|t| t.get("timecode"))
                    .and_then(|v| v.as_str())
            })
        })
        .map(|s| s.to_string());

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());

    Ok(ClipInfo {
        path: path.to_string_lossy().to_string(),
        name,
        size,
        modified,
        duration,
        start_time,
        width: w.max(2),
        height: h.max(2),
        coded_width,
        coded_height,
        rotation,
        avg_fps: avg,
        rate,
        variable_frame_rate: vfr,
        codec: video
            .get("codec_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        pix_fmt: video
            .get("pix_fmt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        has_audio: audio.is_some(),
        audio_channels: audio
            .and_then(|a| a.get("channels"))
            .and_then(num)
            .unwrap_or(0.0) as u32,
        audio_sample_rate: audio
            .and_then(|a| a.get("sample_rate"))
            .and_then(num)
            .unwrap_or(0.0) as u32,
        timecode,
        full_range: video.get("color_range").and_then(|v| v.as_str()) == Some("pc")
            || video
                .get("pix_fmt")
                .and_then(|v| v.as_str())
                .map(|p| p.starts_with("yuvj"))
                .unwrap_or(false),
        color_transfer: video
            .get("color_transfer")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        detected_profile: profile_from_tags(json, video),
        camera: None,
        tags: record_tags(json),
    })
}

/// Pick the standard rate for a clip from ffprobe's nominal (`r_frame_rate`) and
/// average (`avg_frame_rate`) values. Phones record variable frame rate, where the
/// nominal value can be a timebase like 600 while the average is near 29.97.
pub fn choose_rate(r_fps: Option<f64>, avg_fps: Option<f64>) -> (Rate, f64, bool) {
    match (r_fps, avg_fps) {
        (Some(r), Some(a)) => {
            let rel = (r - a).abs() / a;
            let vfr = rel > 0.002;
            let r_rate = Rate::snap(r);
            let r_exact = (r_rate.fps() - r).abs() / r < 0.0005;
            if r_exact && rel < 0.02 {
                (r_rate, a, vfr)
            } else {
                (Rate::snap(a), a, true)
            }
        }
        (Some(r), None) => (Rate::snap(r), r, false),
        (None, Some(a)) => (Rate::snap(a), a, false),
        (None, None) => (Rate::new(30, 1), 30.0, false),
    }
}
