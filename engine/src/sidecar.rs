//! Files cameras write next to each clip, and what they tell us.
//!
//! 1. Sony writes `C0001M01.XML` beside `C0001.MP4`. It names the picture
//!    profile (`CaptureGammaEquation`, for example `s-log3-cine`), the camera
//!    and the lens.
//! 2. DJI drones and cameras write `DJI_0001.SRT` beside `DJI_0001.MP4` when
//!    video captions are on. Every frame carries ISO, shutter, aperture, the
//!    colour mode (`dlog_m`, `d_log`, `hlg`, `normal`), GPS and altitude.
//!
//! Only small text files are read, and nothing is written.

use crate::tone::{profile_from_text, DetectedProfile, ToneProfile};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// What a camera recorded about a clip, beyond the video file itself.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct CameraNotes {
    /// File the notes came from, without its folder.
    pub source: String,
    pub camera: Option<String>,
    pub lens: Option<String>,
    /// The colour mode or gamma exactly as the camera wrote it.
    pub picture_profile: Option<String>,
    /// Typical values over the clip (the median).
    pub iso: Option<f32>,
    /// Shutter time in seconds, for example 0.002 for 1/500.
    pub shutter: Option<f64>,
    pub aperture: Option<f32>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    /// Height above the take-off point in metres, the highest in the clip.
    pub max_height: Option<f32>,
    /// When recording started, as the camera wrote it (Sony with its time
    /// zone, DJI as the drone's own clock time).
    pub recorded: Option<String>,
}

impl CameraNotes {
    pub fn is_empty(&self) -> bool {
        self.recorded.is_none()
            && self.camera.is_none()
            && self.picture_profile.is_none()
            && self.iso.is_none()
            && self.shutter.is_none()
            && self.latitude.is_none()
    }

    /// The profile named in the notes, if it is one this app knows.
    pub fn profile(&self) -> Option<ToneProfile> {
        let p = self.picture_profile.as_deref()?;
        let l = p.to_ascii_lowercase();
        if l == "normal" || l == "default" || l == "none" || l.starts_with("rec709") || l == "standard" {
            return Some(ToneProfile::Standard);
        }
        profile_from_text(p)
    }

    pub fn detected(&self) -> Option<DetectedProfile> {
        self.profile().map(|profile| DetectedProfile {
            profile,
            source: format!("the camera's file {}", self.source),
        })
    }

    /// Plain-language warning when the shutter was much faster than the
    /// frame rate suits: motion looks choppy and cannot be fixed later.
    pub fn shutter_warning(&self, fps: f64) -> Option<String> {
        let s = self.shutter?;
        if s <= 0.0 || fps <= 0.0 {
            return None;
        }
        // 180 degrees is half the frame time; under 45 degrees motion strobes.
        let angle = s * fps * 360.0;
        if angle < 45.0 {
            Some(format!(
                "Shutter 1/{:.0} at {:.0} fps (about {:.0} degrees). Movement in this clip will look choppy; 1/{:.0} is the usual choice. An ND filter lets you keep the slower shutter in bright light.",
                1.0 / s,
                fps,
                angle,
                2.0 * fps.round()
            ))
        } else {
            None
        }
    }
}

/// Read whatever sidecar files a camera left next to `video`.
pub fn read_sidecars(video: &Path) -> Option<CameraNotes> {
    for p in sony_xml_paths(video) {
        if let Ok(text) = read_small(&p) {
            let mut n = parse_sony_xml(&text);
            if !n.is_empty() {
                n.source = file_name(&p);
                return Some(n);
            }
        }
    }
    for p in srt_paths(video) {
        if let Ok(text) = read_small(&p) {
            let mut n = parse_dji_srt(&text);
            if !n.is_empty() {
                n.source = file_name(&p);
                return Some(n);
            }
        }
    }
    None
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

/// Sidecars are small text files; refuse anything large rather than load it.
fn read_small(p: &Path) -> std::io::Result<String> {
    let meta = std::fs::metadata(p)?;
    if meta.len() > 64 * 1024 * 1024 {
        return Err(std::io::Error::other("too large for a sidecar"));
    }
    let bytes = std::fs::read(p)?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

fn with_suffix(video: &Path, suffixes: &[&str]) -> Vec<PathBuf> {
    let (Some(dir), Some(stem)) = (video.parent(), video.file_stem()) else {
        return Vec::new();
    };
    let stem = stem.to_string_lossy();
    suffixes.iter().map(|s| dir.join(format!("{stem}{s}"))).filter(|p| p.is_file()).collect()
}

fn sony_xml_paths(video: &Path) -> Vec<PathBuf> {
    with_suffix(video, &["M01.XML", "M01.xml"])
}

fn srt_paths(video: &Path) -> Vec<PathBuf> {
    with_suffix(video, &[".SRT", ".srt"])
}

/// The value of `attr="..."` inside one XML tag.
fn attr(tag: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let i = tag.find(&key)? + key.len();
    let j = tag[i..].find('"')? + i;
    Some(tag[i..j].to_string())
}

/// Each `<Name ...>` tag in the text.
fn tags<'a>(text: &'a str, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
    let open = format!("<{name} ");
    let mut rest = text;
    std::iter::from_fn(move || {
        let i = rest.find(&open)?;
        let after = &rest[i..];
        let j = after.find('>')?;
        let tag = &after[..=j];
        rest = &after[j + 1..];
        Some(tag)
    })
}

pub fn parse_sony_xml(text: &str) -> CameraNotes {
    let mut n = CameraNotes::default();
    for tag in tags(text, "Device") {
        let maker = attr(tag, "manufacturer").unwrap_or_default();
        let model = attr(tag, "modelName").unwrap_or_default();
        let name = format!("{maker} {model}").trim().to_string();
        if !name.is_empty() {
            n.camera = Some(name);
        }
    }
    for tag in tags(text, "Lens") {
        if let Some(m) = attr(tag, "modelName") {
            n.lens = Some(m);
        }
    }
    for tag in tags(text, "Item") {
        if attr(tag, "name").as_deref() == Some("CaptureGammaEquation") {
            n.picture_profile = attr(tag, "value");
        }
    }
    for tag in tags(text, "CreationDate") {
        if let Some(v) = attr(tag, "value") {
            n.recorded = Some(v);
        }
    }
    n
}

/// A "2024-05-02 18:01:23" style date and time at the start of a caption line.
fn caption_datetime(line: &str) -> Option<String> {
    let l = line.trim().trim_start_matches(|c: char| !c.is_ascii_digit());
    let b = l.as_bytes();
    if b.len() >= 19 && b[4] == b'-' && b[7] == b'-' && (b[10] == b' ' || b[10] == b'T') && b[13] == b':' && b[16] == b':' {
        if l[..19].bytes().enumerate().all(|(i, c)| matches!(i, 4 | 7 | 10 | 13 | 16) || c.is_ascii_digit()) {
            return Some(l[..19].to_string());
        }
    }
    None
}

fn median_f64(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(v[v.len() / 2])
}

/// Numbers after `key` in a DJI caption: `[iso: 100]`, `[iso : 100]`, `ISO 100`.
fn value_after(line: &str, keys: &[&str]) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    for key in keys {
        let mut from = 0;
        while let Some(i) = lower[from..].find(key) {
            let start = from + i;
            // Whole word only: "iso" must not match inside "isolated".
            let before_ok = start == 0 || !lower.as_bytes()[start - 1].is_ascii_alphanumeric();
            let mut j = start + key.len();
            let after_ok = j >= lower.len() || !lower.as_bytes()[j].is_ascii_alphanumeric() || key.ends_with(' ');
            if before_ok && after_ok {
                let bytes = lower.as_bytes();
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b':' || bytes[j] == b'=') {
                    j += 1;
                }
                let end = lower[j..]
                    .find(|c: char| c == ']' || c == ',' || c == ' ' || c == '[' || c == '<')
                    .map(|e| j + e)
                    .unwrap_or(lower.len());
                if end > j {
                    return Some(line[j..end].to_string());
                }
            }
            from = start + key.len();
        }
    }
    None
}

fn parse_shutter(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Some(d) = s.strip_prefix("1/") {
        let d: f64 = d.parse().ok()?;
        return (d > 0.0).then(|| 1.0 / d);
    }
    let v: f64 = s.parse().ok()?;
    // Some models write only the denominator ("SS 1000.0").
    if v >= 1.0 {
        Some(1.0 / v)
    } else if v > 0.0 {
        Some(v)
    } else {
        None
    }
}

pub fn parse_dji_srt(text: &str) -> CameraNotes {
    let mut n = CameraNotes::default();
    let (mut iso, mut shutter, mut fnum, mut lat, mut lon) = (vec![], vec![], vec![], vec![], vec![]);
    let mut height: Option<f32> = None;
    let mut mode: Option<String> = None;
    for line in text.lines() {
        if line.trim().is_empty() || line.contains("-->") {
            continue;
        }
        if let Some(v) = value_after(line, &["iso"]).and_then(|v| v.parse::<f64>().ok()) {
            iso.push(v);
        }
        if let Some(v) = value_after(line, &["shutter", "ss"]).and_then(|v| parse_shutter(&v)) {
            shutter.push(v);
        }
        if let Some(v) = value_after(line, &["fnum"]).and_then(|v| v.parse::<f64>().ok()) {
            // Older models write f/2.8 as 280.
            fnum.push(if v > 50.0 { v / 100.0 } else { v });
        } else if let Some(i) = line.find("F/") {
            if let Some(v) = line[i + 2..].split(|c: char| c == ',' || c == ' ').next().and_then(|v| v.parse::<f64>().ok()) {
                fnum.push(v);
            }
        }
        if let Some(v) = value_after(line, &["latitude"]).and_then(|v| v.parse::<f64>().ok()) {
            lat.push(v);
        }
        if let Some(v) = value_after(line, &["longitude", "longtitude"]).and_then(|v| v.parse::<f64>().ok()) {
            lon.push(v);
        }
        if let Some(v) = value_after(line, &["rel_alt"]).and_then(|v| v.parse::<f32>().ok()) {
            height = Some(height.map_or(v, |h| h.max(v)));
        }
        if mode.is_none() {
            mode = value_after(line, &["color_md"]);
        }
        if n.recorded.is_none() {
            n.recorded = caption_datetime(line);
        }
    }
    n.iso = median_f64(iso).map(|v| v as f32);
    n.shutter = median_f64(shutter);
    n.aperture = median_f64(fnum).map(|v| v as f32);
    // GPS reads 0, 0 before the drone has a fix.
    let lat: Vec<f64> = lat.into_iter().filter(|v| v.abs() > 1e-6).collect();
    let lon: Vec<f64> = lon.into_iter().filter(|v| v.abs() > 1e-6).collect();
    n.latitude = median_f64(lat);
    n.longitude = median_f64(lon);
    n.max_height = height;
    n.picture_profile = mode;
    let any = n.iso.is_some() || n.shutter.is_some() || n.latitude.is_some() || n.picture_profile.is_some();
    if any {
        n.camera = Some("DJI".into());
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    const DJI_NEW: &str = "1\n00:00:00,000 --> 00:00:00,033\n<font size=\"28\">FrameCnt: 1, DiffTime: 33ms\n2024-05-02 18:01:23.456\n[iso: 100] [shutter: 1/1000.0] [fnum: 1.7] [ev: 0] [color_md: dlog_m] [ae_meter_md: 1] [focal_len: 24.00] [dzoom_ratio: 1.00], [latitude: 47.376887] [longitude: 8.541694] [rel_alt: 50.300 abs_alt: 512.850] [ct: 5500] </font>\n\n2\n00:00:00,033 --> 00:00:00,066\n<font size=\"28\">FrameCnt: 2, DiffTime: 33ms\n2024-05-02 18:01:23.489\n[iso: 100] [shutter: 1/1000.0] [fnum: 1.7] [ev: 0] [color_md: dlog_m] [latitude: 47.376890] [longitude: 8.541700] [rel_alt: 52.100 abs_alt: 514.650] [ct: 5500] </font>\n";
    const DJI_OLD: &str = "1\n00:00:00,000 --> 00:00:01,000\n[iso : 400] [shutter : 1/50.0] [fnum : 280] [ev : 0] [ct : 5000] [color_md : default] [focal_len : 240] [latitude: 40.1] [longtitude: -3.2] [altitude: 600.0]\n";
    const DJI_MINI2: &str = "1\n00:00:00,000 --> 00:00:00,033\nF/2.8, SS 640.0, ISO 100, EV 0, DZOOM 1.000, GPS (8.5417, 47.3769, 20), D 12.34m, H 30.00m, H.S 0.00m/s, V.S 0.00m/s\n";
    const SONY: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<NonRealTimeMeta xmlns="urn:schemas-professionalDisc:nonRealTimeMeta:ver.2.20">
  <Duration value="250"/>
  <Device manufacturer="Sony" modelName="ILCE-7SM3" serialNo="1234"/>
  <Lens modelName="FE 24-70mm F2.8 GM II"/>
  <AcquisitionRecord>
    <Group name="CameraUnitMetadataSet">
      <Item name="CaptureGammaEquation" value="s-log3-cine"/>
      <Item name="CaptureColorPrimaries" value="s-gamut3-cine"/>
    </Group>
  </AcquisitionRecord>
</NonRealTimeMeta>"#;

    #[test]
    fn newer_dji_captions_give_profile_exposure_and_place() {
        let n = parse_dji_srt(DJI_NEW);
        assert_eq!(n.picture_profile.as_deref(), Some("dlog_m"));
        assert_eq!(n.profile(), Some(ToneProfile::DLogM));
        assert_eq!(n.iso, Some(100.0));
        assert!((n.shutter.unwrap() - 0.001).abs() < 1e-9);
        assert_eq!(n.aperture, Some(1.7));
        assert!((n.latitude.unwrap() - 47.37689).abs() < 1e-4);
        assert!((n.longitude.unwrap() - 8.5417).abs() < 1e-3);
        assert_eq!(n.max_height, Some(52.1));
    }

    #[test]
    fn older_dji_captions_are_read_too() {
        let n = parse_dji_srt(DJI_OLD);
        assert_eq!(n.iso, Some(400.0));
        assert!((n.shutter.unwrap() - 0.02).abs() < 1e-9);
        assert_eq!(n.aperture, Some(2.8));
        assert_eq!(n.profile(), Some(ToneProfile::Standard));
        assert!((n.longitude.unwrap() + 3.2).abs() < 1e-9);
        let m = parse_dji_srt(DJI_MINI2);
        assert_eq!(m.iso, Some(100.0));
        assert!((m.shutter.unwrap() - 1.0 / 640.0).abs() < 1e-9);
        assert_eq!(m.aperture, Some(2.8));
    }

    #[test]
    fn sony_xml_gives_camera_lens_and_gamma() {
        let n = parse_sony_xml(SONY);
        assert_eq!(n.camera.as_deref(), Some("Sony ILCE-7SM3"));
        assert_eq!(n.lens.as_deref(), Some("FE 24-70mm F2.8 GM II"));
        assert_eq!(n.profile(), Some(ToneProfile::SLog3));
    }

    #[test]
    fn a_fast_shutter_is_flagged_and_a_normal_one_is_not() {
        let fast = CameraNotes { shutter: Some(1.0 / 1000.0), ..Default::default() };
        let w = fast.shutter_warning(23.976).unwrap();
        assert!(w.contains("1/1000") && w.contains("1/48"), "{w}");
        let ok = CameraNotes { shutter: Some(1.0 / 50.0), ..Default::default() };
        assert!(ok.shutter_warning(25.0).is_none());
    }

    #[test]
    fn sidecars_are_found_next_to_the_clip() {
        let dir = std::env::temp_dir().join(format!("sidecar-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("C0001.MP4"), b"").unwrap();
        std::fs::write(dir.join("C0001M01.XML"), SONY).unwrap();
        std::fs::write(dir.join("DJI_0001.MP4"), b"").unwrap();
        std::fs::write(dir.join("DJI_0001.SRT"), DJI_NEW).unwrap();
        std::fs::write(dir.join("A001.MOV"), b"").unwrap();
        let s = read_sidecars(&dir.join("C0001.MP4")).unwrap();
        assert_eq!(s.source, "C0001M01.XML");
        assert_eq!(s.detected().unwrap().profile, ToneProfile::SLog3);
        let d = read_sidecars(&dir.join("DJI_0001.MP4")).unwrap();
        assert_eq!(d.source, "DJI_0001.SRT");
        assert!(read_sidecars(&dir.join("A001.MOV")).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
