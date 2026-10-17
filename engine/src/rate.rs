//! Frame rates and SMPTE timecode, including drop frame for 29.97 and 59.94.

use serde::{Deserialize, Serialize};
use std::fmt;

/// An exact frame rate as a ratio, such as 24000/1001 for 23.976.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Rate {
    pub num: u32,
    pub den: u32,
}

/// Every rate the app knows how to snap footage to.
pub const STANDARD_RATES: &[Rate] = &[
    Rate { num: 24000, den: 1001 },
    Rate { num: 24, den: 1 },
    Rate { num: 25, den: 1 },
    Rate { num: 30000, den: 1001 },
    Rate { num: 30, den: 1 },
    Rate { num: 48000, den: 1001 },
    Rate { num: 48, den: 1 },
    Rate { num: 50, den: 1 },
    Rate { num: 60000, den: 1001 },
    Rate { num: 60, den: 1 },
    Rate { num: 100, den: 1 },
    Rate { num: 120000, den: 1001 },
    Rate { num: 120, den: 1 },
    Rate { num: 240000, den: 1001 },
    Rate { num: 240, den: 1 },
];

/// Rates offered for the exported timeline.
pub const TIMELINE_RATES: &[Rate] = &[
    Rate { num: 24000, den: 1001 },
    Rate { num: 24, den: 1 },
    Rate { num: 25, den: 1 },
    Rate { num: 30000, den: 1001 },
    Rate { num: 30, den: 1 },
    Rate { num: 50, den: 1 },
    Rate { num: 60000, den: 1001 },
    Rate { num: 60, den: 1 },
];

impl Rate {
    pub const fn new(num: u32, den: u32) -> Rate {
        Rate { num, den }
    }

    pub fn fps(self) -> f64 {
        self.num as f64 / self.den as f64
    }

    /// Integer frames per timecode second: 24 for 23.976, 30 for 29.97.
    pub fn timebase(self) -> u32 {
        (self.fps()).round() as u32
    }

    /// True for the 1000/1001 "NTSC" rates.
    pub fn ntsc(self) -> bool {
        self.den == 1001
    }

    /// Drop frame timecode exists only for 29.97 and 59.94 (and multiples of 30 in general).
    pub fn supports_drop_frame(self) -> bool {
        self.ntsc() && self.timebase() % 30 == 0
    }

    /// Duration of one frame in seconds as an exact ratio (num, den).
    pub fn frame_duration(self) -> (u64, u64) {
        (self.den as u64, self.num as u64)
    }

    pub fn label(self) -> String {
        let f = self.fps();
        if self.den == 1 {
            format!("{}", self.num)
        } else {
            let s = format!("{:.3}", f);
            let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
            s
        }
    }

    /// Nearest standard rate to a measured frames-per-second value.
    pub fn snap(fps: f64) -> Rate {
        if !fps.is_finite() || fps <= 0.0 {
            return Rate::new(30, 1);
        }
        let mut best = STANDARD_RATES[0];
        let mut best_err = f64::MAX;
        for r in STANDARD_RATES {
            let err = (r.fps() - fps).abs() / fps;
            if err < best_err {
                best_err = err;
                best = *r;
            }
        }
        best
    }

    /// Parse "30000/1001" style ratios from ffprobe.
    pub fn parse_ratio(s: &str) -> Option<f64> {
        let mut it = s.split('/');
        let n: f64 = it.next()?.trim().parse().ok()?;
        let d: f64 = match it.next() {
            Some(d) => d.trim().parse().ok()?,
            None => 1.0,
        };
        if d == 0.0 || n == 0.0 {
            return None;
        }
        Some(n / d)
    }

    /// Convert seconds to a whole frame count at this rate, rounding to nearest.
    pub fn seconds_to_frames(self, seconds: f64) -> i64 {
        (seconds * self.num as f64 / self.den as f64).round() as i64
    }

    /// Convert a frame count to seconds.
    pub fn frames_to_seconds(self, frames: i64) -> f64 {
        frames as f64 * self.den as f64 / self.num as f64
    }
}

impl fmt::Display for Rate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

fn drop_frames_per_minute(rate: Rate) -> i64 {
    // 2 frames per minute at 29.97, 4 at 59.94.
    (rate.timebase() as i64 / 30) * 2
}

/// Format a frame count (counted from 00:00:00:00) as SMPTE timecode.
/// Drop frame uses `;` before the frames field, non drop frame uses `:`.
pub fn frames_to_timecode(frames: i64, rate: Rate, drop_frame: bool) -> String {
    let tb = rate.timebase() as i64;
    let drop = drop_frame && rate.supports_drop_frame();
    let mut n = frames.max(0);
    if drop {
        let d = drop_frames_per_minute(rate);
        let per_10min = tb * 600 - d * 9;
        let per_min = tb * 60 - d;
        let tens = n / per_10min;
        let rem = n % per_10min;
        n += d * 9 * tens;
        if rem > d {
            n += d * ((rem - d) / per_min);
        }
    }
    let ff = n % tb;
    let total_secs = n / tb;
    let ss = total_secs % 60;
    let mm = (total_secs / 60) % 60;
    let hh = (total_secs / 3600) % 24;
    let sep = if drop { ';' } else { ':' };
    format!("{hh:02}:{mm:02}:{ss:02}{sep}{ff:02}")
}

/// Parse SMPTE timecode into a frame count. A `;` or `.` before the frames
/// field marks drop frame; `drop_frame` forces it on for rates that support it.
pub fn timecode_to_frames(tc: &str, rate: Rate, drop_frame: bool) -> Option<i64> {
    let tc = tc.trim();
    let marked_drop = tc.contains(';') || tc.contains('.');
    let parts: Vec<&str> = tc.split([':', ';', '.']).collect();
    if parts.len() != 4 {
        return None;
    }
    let nums: Vec<i64> = parts
        .iter()
        .map(|p| p.parse::<i64>().ok())
        .collect::<Option<Vec<_>>>()?;
    let (h, m, s, f) = (nums[0], nums[1], nums[2], nums[3]);
    let tb = rate.timebase() as i64;
    if m >= 60 || s >= 60 || f >= tb || h < 0 || m < 0 || s < 0 || f < 0 {
        return None;
    }
    let mut frames = ((h * 3600 + m * 60 + s) * tb) + f;
    if (drop_frame || marked_drop) && rate.supports_drop_frame() {
        let d = drop_frames_per_minute(rate);
        let total_minutes = h * 60 + m;
        frames -= d * (total_minutes - total_minutes / 10);
    }
    Some(frames)
}

/// True when a timecode string uses the drop frame separator.
pub fn is_drop_frame_string(tc: &str) -> bool {
    tc.contains(';')
}
