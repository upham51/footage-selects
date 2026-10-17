//! Picture profiles: how a camera turned light into code values, and how to
//! judge exposure for each.
//!
//! Standard (Rec. 709 style) footage is judged on how it looks: blown
//! highlights, crushed shadows, flat contrast. Log footage is meant to look
//! flat, so judging it that way marks every good log shot down. Instead, each
//! measured luma is turned back into scene light with the camera maker's
//! published curve, and exposure is judged in stops from middle grey:
//! 1. Clipped highlights: pixels piled up at the top of the curve. This is
//!    the one mistake a grade cannot fix, so it costs the most.
//! 2. Underexposure: middle tones more than about 1.5 stops under, where log
//!    footage turns noisy when it is brought up in the grade.
//! 3. Shadows buried in the noise floor, more than about 5 stops under.
//! 4. A very small range of tones (fog, haze), which costs a little.
//! Overexposure is treated gently, because exposing log a stop or two bright
//! is a common way to keep shadows clean.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ToneProfile {
    /// Rec. 709 or any normal, ready-to-watch picture profile.
    Standard,
    SLog3,
    VLog,
    CLog3,
    LogC3,
    FLog,
    FLog2,
    DLog,
    /// DJI has not published this curve; D-Log's curve is used as a close stand-in.
    DLogM,
    AppleLog,
    NLog,
    Hlg,
    /// Log footage from a camera that does not say which curve it used.
    GenericLog,
}

pub const ALL_PROFILES: [ToneProfile; 13] = [
    ToneProfile::Standard,
    ToneProfile::SLog3,
    ToneProfile::VLog,
    ToneProfile::CLog3,
    ToneProfile::LogC3,
    ToneProfile::FLog,
    ToneProfile::FLog2,
    ToneProfile::DLog,
    ToneProfile::DLogM,
    ToneProfile::AppleLog,
    ToneProfile::NLog,
    ToneProfile::Hlg,
    ToneProfile::GenericLog,
];

/// Whether a curve's published formula gives values over the full code
/// range (0 is code 0, 1 is code 1023) or over the legal video range
/// (0 is code 64, 1 is code 940).
#[derive(Clone, Copy, PartialEq)]
enum Space {
    Full,
    Legal,
}

impl ToneProfile {
    pub fn label(self) -> &'static str {
        match self {
            ToneProfile::Standard => "Standard (Rec. 709)",
            ToneProfile::SLog3 => "Sony S-Log3",
            ToneProfile::VLog => "Panasonic V-Log",
            ToneProfile::CLog3 => "Canon Log 3",
            ToneProfile::LogC3 => "ARRI LogC3",
            ToneProfile::FLog => "Fujifilm F-Log",
            ToneProfile::FLog2 => "Fujifilm F-Log2",
            ToneProfile::DLog => "DJI D-Log",
            ToneProfile::DLogM => "DJI D-Log M",
            ToneProfile::AppleLog => "Apple Log",
            ToneProfile::NLog => "Nikon N-Log",
            ToneProfile::Hlg => "HLG",
            ToneProfile::GenericLog => "Log, curve unknown",
        }
    }

    pub fn is_log(self) -> bool {
        self != ToneProfile::Standard
    }

    /// True when the curve is a stand-in rather than the maker's own formula.
    pub fn approximate(self) -> bool {
        matches!(self, ToneProfile::DLogM | ToneProfile::GenericLog)
    }

    fn space(self) -> Space {
        match self {
            ToneProfile::CLog3 | ToneProfile::AppleLog | ToneProfile::Hlg | ToneProfile::GenericLog => Space::Legal,
            _ => Space::Full,
        }
    }

    /// Scene light (0.18 is middle grey) from a code value in the curve's own space.
    fn to_linear(self, v: f64) -> f64 {
        match self {
            ToneProfile::Standard => {
                // Close to how a Rec. 709 picture is displayed.
                v.max(0.0).powf(2.4) * 0.9
            }
            ToneProfile::SLog3 => {
                let c = v * 1023.0;
                if c >= 171.2102946929 {
                    10f64.powf((c - 420.0) / 261.5) * 0.19 - 0.01
                } else {
                    (c - 95.0) * 0.01125 / (171.2102946929 - 95.0)
                }
            }
            ToneProfile::VLog => {
                if v < 0.181 {
                    (v - 0.125) / 5.6
                } else {
                    10f64.powf((v - 0.598206) / 0.241514) - 0.00873
                }
            }
            ToneProfile::CLog3 => {
                // Canon's formula works in reflectance divided by 0.9.
                let x = if v < 0.097465473 {
                    -(10f64.powf((0.12783901 - v) / 0.36726845) - 1.0) / 14.98325
                } else if v <= 0.15277891 {
                    (v - 0.12512219) / 1.9754798
                } else {
                    (10f64.powf((v - 0.12240537) / 0.36726845) - 1.0) / 14.98325
                };
                x * 0.9
            }
            ToneProfile::LogC3 => {
                // Exposure index 800, the camera's base setting.
                let (cut, a, b, c, d, e, f) = (0.010591, 5.555556, 0.052272, 0.247190, 0.385537, 5.367655, 0.092809);
                if v > e * cut + f {
                    (10f64.powf((v - d) / c) - b) / a
                } else {
                    (v - f) / e
                }
            }
            ToneProfile::FLog => {
                let (a, b, c, d, e, f) = (0.555556, 0.009468, 0.344676, 0.790453, 8.735631, 0.092864);
                if v >= 0.100537775223865 {
                    10f64.powf((v - d) / c) / a - b / a
                } else {
                    (v - f) / e
                }
            }
            ToneProfile::FLog2 => {
                let (a, b, c, d, e, f) = (5.555556, 0.064829, 0.245281, 0.384316, 8.799461, 0.092864);
                if v >= 0.100686685370811 {
                    10f64.powf((v - d) / c) / a - b / a
                } else {
                    (v - f) / e
                }
            }
            ToneProfile::DLog | ToneProfile::DLogM => {
                if v <= 0.14 {
                    (v - 0.0929) / 6.025
                } else {
                    (10f64.powf(3.89616 * v - 2.27752) - 0.0108) / 0.9892
                }
            }
            ToneProfile::AppleLog => {
                let (r0, rt, c, beta, gamma, delta) = (-0.05641088, 0.01, 47.28711236, 0.00964052, 0.08550479, 0.69336945);
                if v >= c * (rt - r0) * (rt - r0) {
                    2f64.powf((v - delta) / gamma) - beta
                } else if v > 0.0 {
                    (v / c).sqrt() + r0
                } else {
                    r0
                }
            }
            ToneProfile::NLog => {
                let code = v * 1023.0;
                if code < 452.0 {
                    (code / 650.0).powi(3) - 0.0075
                } else {
                    ((code - 619.0) / 150.0).exp()
                }
            }
            ToneProfile::Hlg => {
                let (a, b, c) = (0.17883277, 0.28466892, 0.55991073);
                let e = if v <= 0.5 { v * v / 3.0 } else { (((v - c) / a).exp() + b) / 12.0 };
                // BT.2408 puts diffuse white at 75% signal, which is scene light 0.265.
                e / 0.265
            }
            ToneProfile::GenericLog => {
                // The average, in stops, of four published curves read at the
                // same signal level. `v` is in the legal range.
                let code = (64.0 + v * 876.0) / 1023.0;
                let curves = [ToneProfile::SLog3, ToneProfile::VLog, ToneProfile::LogC3, ToneProfile::DLog];
                let mean_log2 = curves
                    .iter()
                    .map(|p| p.to_linear(code).max(MIN_LIGHT).log2())
                    .sum::<f64>()
                    / curves.len() as f64;
                2f64.powf(mean_log2)
            }
        }
    }
}

/// Maps 8-bit analysis luma to stops from middle grey for one clip.
///
/// The analysis picture is grayscale with video black at 0 and video white
/// at 255 (ffmpeg expands the legal range), unless the file itself was
/// recorded full range, in which case the values are the file's own.
#[derive(Clone, Debug)]
pub struct StopsTable {
    pub profile: ToneProfile,
    stops: [f32; 256],
}

/// Floor for scene light, about 14 stops under middle grey.
const MIN_LIGHT: f64 = 0.18 / 16384.0;

impl StopsTable {
    pub fn new(profile: ToneProfile, full_range_file: bool) -> StopsTable {
        let mut stops = [0f32; 256];
        for (g, s) in stops.iter_mut().enumerate() {
            let x = g as f64 / 255.0;
            // Code value out of 1023.
            let code = if full_range_file { x * 1023.0 } else { 64.0 + x * 876.0 };
            let v = match profile.space() {
                Space::Full => code / 1023.0,
                Space::Legal => (code - 64.0) / 876.0,
            };
            let lin = profile.to_linear(v).max(MIN_LIGHT);
            *s = (lin / 0.18).log2() as f32;
        }
        StopsTable { profile, stops }
    }

    pub fn stops(&self, luma: u8) -> f32 {
        self.stops[luma as usize]
    }
}

/// What the exposure score found wrong in one frame, if anything.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExposureIssue {
    #[default]
    None,
    ClippedHighlights,
    Underexposed,
    Overexposed,
    DeepShadows,
    Flat,
}

impl ExposureIssue {
    pub fn code(self) -> u8 {
        self as u8
    }
    pub fn from_code(c: u8) -> ExposureIssue {
        match c {
            1 => ExposureIssue::ClippedHighlights,
            2 => ExposureIssue::Underexposed,
            3 => ExposureIssue::Overexposed,
            4 => ExposureIssue::DeepShadows,
            5 => ExposureIssue::Flat,
            _ => ExposureIssue::None,
        }
    }
    /// Plain words for a select's "why" line.
    pub fn words(self) -> &'static str {
        match self {
            ExposureIssue::None => "",
            ExposureIssue::ClippedHighlights => "highlights clipped",
            ExposureIssue::Underexposed => "underexposed",
            ExposureIssue::Overexposed => "overexposed",
            ExposureIssue::DeepShadows => "large areas lost in the shadows",
            ExposureIssue::Flat => "very flat light",
        }
    }
}

/// Exposure judgement for one log frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogReading {
    pub score: f32,
    /// Middle tones (the median pixel) in stops from middle grey.
    pub key: f32,
    /// Share of the picture clipped at the top of the curve.
    pub clipped: f32,
    /// Share of the picture more than `NOISE_FLOOR` stops under middle grey.
    pub deep: f32,
    pub issue: ExposureIssue,
}

/// Stops under middle grey where log footage is mostly noise.
pub const NOISE_FLOOR: f32 = -5.0;
/// The brightest value has to be at least this many stops over middle grey
/// before a pile-up there counts as clipping. A plain overcast sky can also
/// sit in a few neighbouring values, but much lower on the curve.
pub const CLIP_MIN_STOPS: f32 = 4.0;

fn ramp(x: f32, a: f32, b: f32) -> f32 {
    crate::scores::ramp(x, a, b)
}

/// Share of pixels darker than `limit` stops, read off the ladder of percentiles.
pub fn share_below(ladder: &[u8], table: &StopsTable, limit: f32) -> f32 {
    use crate::metrics::LADDER;
    let s: Vec<f32> = ladder.iter().map(|&g| table.stops(g)).collect();
    if limit <= s[0] {
        return 0.0;
    }
    for k in 1..s.len() {
        if limit <= s[k] {
            let span = s[k] - s[k - 1];
            let f = if span > 1e-6 { (limit - s[k - 1]) / span } else { 1.0 };
            return LADDER[k - 1] + f * (LADDER[k] - LADDER[k - 1]);
        }
    }
    1.0
}

/// Judge one frame of log footage from its ladder of percentiles.
pub fn log_exposure(ladder: &[u8], vmax: u8, top: f32, table: &StopsTable) -> LogReading {
    use crate::metrics::LADDER;
    let at = |share: f32| -> f32 {
        let k = LADDER.iter().position(|&p| (p - share).abs() < 1e-6).unwrap_or(5);
        table.stops(ladder[k])
    };
    let key = at(0.50);
    let clipped = if table.stops(vmax) >= CLIP_MIN_STOPS { top } else { 0.0 };
    let deep = share_below(ladder, table, NOISE_FLOOR);
    let range = at(0.98) - at(0.02);

    let pen_clip = ramp(clipped, 0.01, 0.12);
    let pen_under = ramp(-key, 1.5, 4.0);
    let pen_over = ramp(key, 2.5, 4.5) * 0.7;
    let pen_deep = ramp(deep, 0.15, 0.55) * 0.8;
    let pen_flat = (1.0 - ramp(range, 1.0, 3.0)) * 0.3;
    let score = (1.0 - pen_clip) * (1.0 - pen_under) * (1.0 - pen_over) * (1.0 - pen_deep) * (1.0 - pen_flat);

    let worst = [
        (pen_clip, ExposureIssue::ClippedHighlights),
        (pen_under, ExposureIssue::Underexposed),
        (pen_over, ExposureIssue::Overexposed),
        (pen_deep, ExposureIssue::DeepShadows),
        (pen_flat, ExposureIssue::Flat),
    ]
    .into_iter()
    .fold((0.05f32, ExposureIssue::None), |best, c| if c.0 > best.0 { c } else { best });

    LogReading {
        score,
        key,
        clipped,
        deep,
        issue: worst.1,
    }
}

/// True when a clip's tones look like log: black never reaches the bottom
/// and white never reaches the top across most of the clip. Used only when
/// neither the file nor a camera sidecar says which profile was used.
pub fn looks_like_log(ladders: &[[u8; crate::metrics::LADDER_LEN]], vmax: &[u8]) -> bool {
    if ladders.len() < 3 || vmax.len() != ladders.len() {
        return false;
    }
    let mut p1: Vec<u8> = ladders.iter().map(|l| l[0]).collect();
    let mut p99: Vec<u8> = ladders.iter().map(|l| l[10]).collect();
    let mut mx: Vec<u8> = vmax.to_vec();
    p1.sort_unstable();
    p99.sort_unstable();
    mx.sort_unstable();
    let median = |v: &Vec<u8>| v[v.len() / 2];
    let p90 = |v: &Vec<u8>| v[(v.len() * 9 / 10).min(v.len() - 1)];
    // Log puts true black around 4% of the range and rarely uses the top 5%.
    let raised_black = median(&p1) >= 8;
    let low_white = p90(&mx) <= 242 && median(&p99) <= 225;
    raised_black && low_white
}

/// Where a clip's profile came from, for the detail view.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DetectedProfile {
    pub profile: ToneProfile,
    /// Plain words: "the file's colour tags", "the camera's sidecar file C0001M01.XML".
    pub source: String,
}

/// Recognise a profile name written by a camera or editing app.
pub fn profile_from_text(text: &str) -> Option<ToneProfile> {
    let t: String = text
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    // Longest names first so "dlogm" is not read as "dlog", "flog2" as "flog".
    let table: [(&str, ToneProfile); 16] = [
        ("slog3", ToneProfile::SLog3),
        ("vlog", ToneProfile::VLog),
        ("canonlog3", ToneProfile::CLog3),
        ("clog3", ToneProfile::CLog3),
        ("logc3", ToneProfile::LogC3),
        ("logc", ToneProfile::LogC3),
        ("flog2", ToneProfile::FLog2),
        ("flog", ToneProfile::FLog),
        ("dlogm", ToneProfile::DLogM),
        ("dlog", ToneProfile::DLog),
        ("applelog", ToneProfile::AppleLog),
        ("nlog", ToneProfile::NLog),
        ("aribstdb67", ToneProfile::Hlg),
        ("hlg", ToneProfile::Hlg),
        ("slog2", ToneProfile::GenericLog),
        ("clog2", ToneProfile::GenericLog),
    ];
    table.iter().find(|(k, _)| t.contains(k)).map(|(_, p)| *p)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 8-bit analysis luma for a code value out of 1023 in a legal-range file.
    fn luma_for_code(code: f64) -> u8 {
        (((code - 64.0) / 876.0) * 255.0).round().clamp(0.0, 255.0) as u8
    }

    #[test]
    fn middle_grey_lands_near_zero_stops_on_every_published_curve() {
        // Middle grey code values (10-bit) from each maker's documentation.
        let cases = [
            (ToneProfile::SLog3, 420.0),
            (ToneProfile::VLog, 433.0),
            (ToneProfile::LogC3, 400.0),
            (ToneProfile::FLog, 470.0),
            (ToneProfile::DLog, 408.0),
            (ToneProfile::CLog3, 64.0 + 0.343 * 876.0),
        ];
        for (p, code) in cases {
            let t = StopsTable::new(p, false);
            let s = t.stops(luma_for_code(code));
            assert!(s.abs() < 0.2, "{p:?} middle grey reads {s} stops");
        }
    }

    #[test]
    fn each_stop_is_about_the_same_distance_up_the_curve() {
        let t = StopsTable::new(ToneProfile::SLog3, false);
        let grey = luma_for_code(420.0);
        let one_up = luma_for_code(420.0 + 261.5 * 2f64.log10());
        let s = t.stops(one_up) - t.stops(grey);
        assert!((s - 1.0).abs() < 0.1, "{s}");
    }

    #[test]
    fn stops_rise_with_luma_for_every_profile() {
        for p in ALL_PROFILES {
            let t = StopsTable::new(p, false);
            for g in 1..=255u8 {
                assert!(t.stops(g) >= t.stops(g - 1) - 1e-4, "{p:?} at {g}");
            }
        }
    }

    #[test]
    fn names_written_by_cameras_are_recognised() {
        assert_eq!(profile_from_text("s-log3-cine"), Some(ToneProfile::SLog3));
        assert_eq!(profile_from_text("dlog_m"), Some(ToneProfile::DLogM));
        assert_eq!(profile_from_text("D-Log"), Some(ToneProfile::DLog));
        assert_eq!(profile_from_text("F-Log2"), Some(ToneProfile::FLog2));
        assert_eq!(profile_from_text("arib-std-b67"), Some(ToneProfile::Hlg));
        assert_eq!(profile_from_text("bt709"), None);
        assert_eq!(profile_from_text("normal"), None);
    }

    fn ladder_at(stops: &[f32; 11], t: &StopsTable) -> [u8; 11] {
        let mut out = [0u8; 11];
        for (k, s) in stops.iter().enumerate() {
            out[k] = (0..=255u8).min_by(|&a, &b| (t.stops(a) - s).abs().partial_cmp(&(t.stops(b) - s).abs()).unwrap()).unwrap();
        }
        out
    }

    #[test]
    fn a_well_exposed_log_frame_scores_high_and_flat_looking_is_fine() {
        let t = StopsTable::new(ToneProfile::SLog3, false);
        let l = ladder_at(&[-4.5, -4.0, -3.2, -2.5, -1.3, -0.3, 0.8, 1.9, 2.5, 3.0, 3.3], &t);
        let r = log_exposure(&l, l[10] + 3, 0.002, &t);
        assert!(r.score > 0.9, "{r:?}");
        assert_eq!(r.issue, ExposureIssue::None);
    }

    #[test]
    fn underexposed_log_is_caught_and_named() {
        let t = StopsTable::new(ToneProfile::SLog3, false);
        let l = ladder_at(&[-8.0, -7.5, -7.0, -6.5, -5.5, -4.6, -3.6, -2.6, -2.0, -1.4, -1.0], &t);
        let r = log_exposure(&l, l[10] + 2, 0.002, &t);
        assert!(r.score < 0.4, "{r:?}");
        assert!(matches!(r.issue, ExposureIssue::Underexposed | ExposureIssue::DeepShadows));
    }

    #[test]
    fn clipping_at_the_top_of_the_log_curve_is_caught_even_far_below_255() {
        let t = StopsTable::new(ToneProfile::SLog3, false);
        let l = ladder_at(&[-3.0, -2.5, -1.8, -1.0, 0.0, 1.0, 2.5, 4.5, 5.0, 5.5, 5.5], &t);
        let vmax = l[10];
        assert!(vmax < 230, "S-Log3 clips well below 255: {vmax}");
        let r = log_exposure(&l, vmax, 0.18, &t);
        assert_eq!(r.issue, ExposureIssue::ClippedHighlights);
        assert!(r.score < 0.2, "{r:?}");
    }

    #[test]
    fn share_below_reads_the_ladder() {
        let t = StopsTable::new(ToneProfile::GenericLog, false);
        let l = ladder_at(&[-7.0, -6.0, -5.5, -5.0, -3.0, -1.0, 0.0, 1.0, 2.0, 3.0, 4.0], &t);
        let s = share_below(&l, &t, -5.0);
        assert!((s - 0.10).abs() < 0.03, "{s}");
    }

    #[test]
    fn log_tones_are_recognised_and_standard_tones_are_not() {
        let log: Vec<[u8; 11]> = (0..20).map(|_| [14, 18, 25, 33, 60, 95, 130, 160, 175, 190, 200]).collect();
        let std: Vec<[u8; 11]> = (0..20).map(|_| [0, 2, 8, 20, 60, 120, 180, 225, 240, 250, 254]).collect();
        assert!(looks_like_log(&log, &vec![215; 20]));
        assert!(!looks_like_log(&std, &vec![255; 20]));
    }
}
