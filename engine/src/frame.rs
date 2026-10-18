//! One full-colour still from a clip, for the frame viewer in the clip view,
//! plus the numbers the viewer needs to draw exposure overlays on it.
//!
//! Stills are decoded on demand by ffmpeg and never stored. The overlays
//! (false colour, focus peaking, clipping) are drawn by the window from the
//! still's pixels, using `ToneGuide` so false colour reads in the same stops
//! from middle grey that the exposure score uses.

use crate::analyze::analysis_size;
use crate::ffmpeg;
use crate::metrics::HIGHLIGHT_LEVEL;
use crate::probe::ClipInfo;
use crate::project::ProjectClip;
use crate::tone::{StopsTable, ToneProfile, CLIP_MIN_STOPS};
use serde::Serialize;
use std::process::Stdio;

/// Longest edge of a still, in pixels, before the window scales it to fit.
pub const STILL_WIDTH: u32 = 960;

/// A decoded still: `width` by `height` pixels, four bytes each (RGBA).
pub struct Still {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Still {
    /// Width and height as little-endian u32s, then the pixels: the form the
    /// window reads in one transfer.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + self.rgba.len());
        out.extend_from_slice(&self.width.to_le_bytes());
        out.extend_from_slice(&self.height.to_le_bytes());
        out.extend_from_slice(&self.rgba);
        out
    }
}

fn run(path: &str, t: f64, w: u32, h: u32) -> Result<Vec<u8>, String> {
    let vf = format!("scale={w}:{h}:flags=bicubic,setsar=1,format=rgba");
    let out = ffmpeg::command("ffmpeg")
        .args(["-hide_banner", "-nostdin", "-v", "error"])
        .args(["-ss", &format!("{:.4}", t.max(0.0))])
        .args(["-i", path])
        .args(["-map", "0:v:0", "-frames:v", "1", "-an", "-sn", "-dn", "-vf", &vf])
        .args(["-f", "rawvideo", "-"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("The video reader did not start ({e})."))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(crate::probe::friendly_ffmpeg_error(&err));
    }
    Ok(out.stdout)
}

/// The frame at `t` seconds from the clip start, about `STILL_WIDTH` wide.
/// A time past the last frame falls back to a little earlier.
pub fn grab_still(info: &ClipInfo, path: &str, t: f64) -> Result<Still, String> {
    let (w, h) = analysis_size(info, STILL_WIDTH);
    let need = (w * h * 4) as usize;
    let mut at = t.clamp(0.0, info.duration.max(0.0));
    for _ in 0..3 {
        let px = run(path, at, w, h)?;
        if px.len() >= need {
            return Ok(Still { width: w, height: h, rgba: px[..need].to_vec() });
        }
        if at <= 0.0 {
            break;
        }
        at = (at - 0.5).max(0.0);
    }
    Err("No picture could be read at that moment of the clip. Try another point on the graphs.".into())
}

/// How the window turns the still's brightness into stops for false colour.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ToneGuide {
    /// Stops from middle grey for every 8-bit luma value of the still.
    pub stops: Vec<f32>,
    /// Luma at and above which the picture counts as clipped.
    pub clip_luma: u8,
    /// The picture profile the stops are read through.
    pub profile: ToneProfile,
}

/// The tone guide for one clip, judged with `profile`.
///
/// Standard footage clips at the same level the exposure score uses. Log
/// footage clips at the top of the camera's curve, which sits well below
/// white; the brightest value the analysis saw in the clip marks it when it
/// is high enough on the curve to be a real clip point.
pub fn tone_guide(clip: &ProjectClip, profile: ToneProfile) -> ToneGuide {
    let full_range = clip.info.as_ref().map(|i| i.full_range).unwrap_or(false);
    let table = StopsTable::new(profile, full_range);
    let stops: Vec<f32> = (0..=255u8).map(|g| (table.stops(g) * 100.0).round() / 100.0).collect();
    let mut clip_luma = HIGHLIGHT_LEVEL;
    if profile.is_log() {
        let brightest = clip
            .analysis
            .as_ref()
            .and_then(|a| a.frames.vmax.iter().copied().max())
            .filter(|&v| table.stops(v) >= CLIP_MIN_STOPS);
        if let Some(v) = brightest {
            clip_luma = v.saturating_sub(1).min(HIGHLIGHT_LEVEL);
        }
    }
    ToneGuide { stops, clip_luma, profile }
}
