//! When and where the sun was: recording times, phone GPS tags and the light.
//!
//! 1. `parse_timestamp` reads the dates cameras and ffprobe write
//!    (`creation_time`, EXIF style `2023:05:01 10:00:00`, with or without a
//!    zone) into a `Timestamp`, with our own calendar arithmetic.
//! 2. `parse_iso6709` reads the location tag phones write into video files.
//! 3. `sun_elevation`, `sun_azimuth` and `sun_times` follow NOAA's general
//!    solar position calculations (fractional year, equation of time,
//!    declination, hour angle), with NOAA's atmospheric refraction correction
//!    for the elevation. They are good to a fraction of a degree and a minute
//!    or two, plenty for naming the light.
//! 4. `Light` names the light from the sun's elevation.
//!
//! Everything is plain arithmetic, no time zone database and no network.

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

const DAY: f64 = 86_400.0;

/// A moment as seconds since the Unix epoch, with the clock's offset from UTC when known.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Timestamp {
    /// Seconds since 1970-01-01T00:00:00 of the wall-clock reading, as if it were UTC.
    pub wall: f64,
    /// Minutes east of UTC when the text said (Z, +02:00, +0200, -0500). None when the zone is unknown.
    pub offset_minutes: Option<i32>,
}

impl Timestamp {
    /// True UTC seconds when the zone is known.
    pub fn utc(&self) -> Option<f64> {
        self.offset_minutes.map(|m| self.wall - m as f64 * 60.0)
    }

    /// UTC seconds, assuming `offset_minutes` when the text named no zone.
    pub fn utc_assuming(&self, offset_minutes: i32) -> f64 {
        self.wall - self.offset_minutes.unwrap_or(offset_minutes) as f64 * 60.0
    }
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's `days_from_civil`).
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // 0..=399
    let mp = (month as i64 + 9) % 12; // March is 0
    let doy = (153 * mp + 2) / 5 + day as i64 - 1; // 0..=365
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // 0..=146096
    era * 146_097 + doe - 719_468
}

/// The date of a day count since 1970-01-01 (Howard Hinnant's `civil_from_days`).
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // 0..=146096
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // 0..=399
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // 0..=365
    let mp = (5 * doy + 2) / 153; // 0..=11
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

pub fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Reads a run of ASCII digits of exactly `n` characters as a number.
fn digits(s: &[u8], n: usize) -> Option<u32> {
    if s.len() < n || !s[..n].iter().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(s[..n].iter().fold(0, |v, b| v * 10 + (b - b'0') as u32))
}

/// Parse "2023-05-01T10:00:00.000000Z", "2023-05-01T10:00:00+0200", "2023-05-01T10:00:00+02:00", "2023-05-01 10:00:00", "2023:05:01 10:00:00", "2023-05-01T10:00:00" and similar camera formats. None for anything else or impossible dates.
///
/// Accepted: a four-digit year, month and day separated by `-`, `:` or `/`
/// (the same both times), then `T`, `t` or spaces, then hours and minutes with
/// optional seconds and fraction, then an optional zone: `Z`, `UTC`, `GMT`,
/// `+HH`, `+HHMM` or `+HH:MM` (also with `-`), with or without a space before
/// it. A time is required. Leap seconds (`:60`) are accepted and count as the
/// next second.
pub fn parse_timestamp(s: &str) -> Option<Timestamp> {
    let b = s.trim().as_bytes();
    let year = digits(b, 4)? as i64;
    let sep = *b.get(4)?;
    if !matches!(sep, b'-' | b':' | b'/') || b.get(7) != Some(&sep) {
        return None;
    }
    let month = digits(&b[5..], 2)?;
    let day = digits(b.get(8..)?, 2)?;
    if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
        return None;
    }
    let mut i = 10;
    match b.get(i) {
        Some(b'T') | Some(b't') => i += 1,
        Some(b' ') => {
            while b.get(i) == Some(&b' ') {
                i += 1;
            }
        }
        _ => return None,
    }
    let hour = digits(b.get(i..)?, 2)?;
    if b.get(i + 2) != Some(&b':') {
        return None;
    }
    let minute = digits(b.get(i + 3..)?, 2)?;
    i += 5;
    let mut second = 0.0;
    if b.get(i) == Some(&b':') {
        second = digits(b.get(i + 1..)?, 2)? as f64;
        i += 3;
        if matches!(b.get(i), Some(b'.') | Some(b',')) {
            let start = i + 1;
            let mut end = start;
            while end < b.len() && b[end].is_ascii_digit() {
                end += 1;
            }
            if end == start {
                return None;
            }
            let frac: f64 = format!("0.{}", std::str::from_utf8(&b[start..end]).ok()?).parse().ok()?;
            second += frac;
            i = end;
        }
    }
    if hour > 23 || minute > 59 || second >= 61.0 {
        return None;
    }
    while b.get(i) == Some(&b' ') {
        i += 1;
    }
    let rest = &b[i..];
    let offset_minutes = parse_zone(rest)?;
    let days = days_from_civil(year, month, day);
    let wall = days as f64 * DAY + hour as f64 * 3600.0 + minute as f64 * 60.0 + second;
    Some(Timestamp { wall, offset_minutes })
}

/// The zone at the end of a timestamp. Outer None means the text is not a
/// zone; inner None means there was no zone.
fn parse_zone(z: &[u8]) -> Option<Option<i32>> {
    if z.is_empty() {
        return Some(None);
    }
    let upper: Vec<u8> = z.to_ascii_uppercase();
    if upper == b"Z" || upper == b"UTC" || upper == b"GMT" {
        return Some(Some(0));
    }
    // "UTC+02:00" and "GMT-5" style.
    let z = if upper.starts_with(b"UTC") || upper.starts_with(b"GMT") { &z[3..] } else { z };
    let sign = match z.first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let r = &z[1..];
    let (hours, minutes) = match r.len() {
        1 => (digits(r, 1)?, 0),
        2 => (digits(r, 2)?, 0),
        4 => (digits(r, 2)?, digits(&r[2..], 2)?),
        5 if r[2] == b':' => (digits(r, 2)?, digits(&r[3..], 2)?),
        _ => return None,
    };
    if hours > 14 || minutes > 59 {
        return None;
    }
    Some(Some(sign * (hours as i32 * 60 + minutes as i32)))
}

/// Reads one signed ISO 6709 coordinate. `degree_digits` is 2 for latitude
/// and 3 for longitude; the integer part decides between degrees (DD),
/// degrees and minutes (DDMM) and degrees, minutes and seconds (DDMMSS).
fn iso6709_part(part: &str, degree_digits: usize) -> Option<f64> {
    let (sign, body) = match part.as_bytes().first()? {
        b'+' => (1.0, &part[1..]),
        b'-' => (-1.0, &part[1..]),
        _ => return None,
    };
    if body.is_empty() || !body.bytes().all(|c| c.is_ascii_digit() || c == b'.') || body.matches('.').count() > 1 {
        return None;
    }
    let int_len = body.find('.').unwrap_or(body.len());
    let value: f64 = body.parse().ok()?;
    let v = if int_len <= degree_digits {
        value
    } else if int_len == degree_digits + 2 {
        let deg = (value / 100.0).trunc();
        let min = value - deg * 100.0;
        if min >= 60.0 {
            return None;
        }
        deg + min / 60.0
    } else if int_len == degree_digits + 4 {
        let deg = (value / 10_000.0).trunc();
        let min = ((value - deg * 10_000.0) / 100.0).trunc();
        let sec = value - deg * 10_000.0 - min * 100.0;
        if min >= 60.0 || sec >= 60.0 {
            return None;
        }
        deg + min / 60.0 + sec / 3600.0
    } else {
        return None;
    };
    Some(sign * v)
}

/// Parse ISO 6709 location strings as phones write them: "+48.8584+002.2945+035.000/", "+48.8584+002.2945/", "-33.8688+151.2093/". Returns (latitude, longitude).
///
/// The altitude and an optional `CRS...` suffix are ignored, and the
/// trailing slash may be missing. Degrees-and-minutes (`+4851.50+00217.67/`)
/// and degrees-minutes-seconds forms are read too. Exactly `+00.0000+000.0000`
/// is what cameras write without a fix, so it gives None.
pub fn parse_iso6709(s: &str) -> Option<(f64, f64)> {
    let s = s.trim();
    let s = s.split("CRS").next()?.trim_end();
    let s = s.strip_suffix('/').unwrap_or(s);
    // Split into signed pieces: latitude, longitude and maybe altitude.
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0;
    for (i, c) in s.char_indices() {
        if (c == '+' || c == '-') && i > start {
            parts.push(&s[start..i]);
            start = i;
        }
    }
    if start < s.len() {
        parts.push(&s[start..]);
    }
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    let lat = iso6709_part(parts[0], 2)?;
    let lon = iso6709_part(parts[1], 3)?;
    if parts.len() == 3 {
        let alt = parts[2];
        let ok = alt.len() > 1
            && (alt.starts_with('+') || alt.starts_with('-'))
            && alt[1..].parse::<f64>().is_ok();
        if !ok {
            return None;
        }
    }
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    if lat == 0.0 && lon == 0.0 {
        return None;
    }
    Some((lat, lon))
}

/// What the NOAA equations need for one moment: equation of time in
/// minutes and declination in radians, from the fractional year.
fn sun_angles(utc: f64) -> (f64, f64) {
    let days = (utc / DAY).floor();
    let (year, _, _) = civil_from_days(days as i64);
    let day_of_year = days - days_from_civil(year, 1, 1) as f64 + 1.0;
    let hour = (utc - days * DAY) / 3600.0;
    let year_days = if is_leap_year(year) { 366.0 } else { 365.0 };
    let g = 2.0 * PI / year_days * (day_of_year - 1.0 + (hour - 12.0) / 24.0);
    let eqtime = 229.18
        * (0.000075 + 0.001868 * g.cos() - 0.032077 * g.sin() - 0.014615 * (2.0 * g).cos()
            - 0.040849 * (2.0 * g).sin());
    let decl = 0.006918 - 0.399912 * g.cos() + 0.070257 * g.sin() - 0.006758 * (2.0 * g).cos()
        + 0.000907 * (2.0 * g).sin()
        - 0.002697 * (3.0 * g).cos()
        + 0.00148 * (3.0 * g).sin();
    (eqtime, decl)
}

/// Hour angle in radians (0 at solar noon, positive in the afternoon) and declination.
fn hour_angle(lon: f64, utc: f64) -> (f64, f64) {
    let (eqtime, decl) = sun_angles(utc);
    let minutes = (utc - (utc / DAY).floor() * DAY) / 60.0;
    let true_solar = minutes + eqtime + 4.0 * lon;
    let ha_deg = true_solar / 4.0 - 180.0;
    (ha_deg.to_radians(), decl)
}

/// Elevation without refraction, in degrees.
fn geometric_elevation(lat: f64, lon: f64, utc: f64) -> f64 {
    let (ha, decl) = hour_angle(lon, utc);
    let phi = lat.to_radians();
    let cos_zenith = (phi.sin() * decl.sin() + phi.cos() * decl.cos() * ha.cos()).clamp(-1.0, 1.0);
    90.0 - cos_zenith.acos().to_degrees()
}

/// NOAA's approximate atmospheric refraction in degrees for a geometric elevation.
fn refraction(elevation: f64) -> f64 {
    let e = elevation;
    let t = e.to_radians().tan();
    let arcsec = if e > 85.0 {
        0.0
    } else if e > 5.0 {
        58.1 / t - 0.07 / t.powi(3) + 0.000086 / t.powi(5)
    } else if e > -0.575 {
        1735.0 + e * (-518.2 + e * (103.4 + e * (-12.79 + e * 0.711)))
    } else {
        -20.772 / t
    };
    arcsec / 3600.0
}

/// Sun elevation above the horizon in degrees (refraction-corrected), NOAA's solar position equations. `utc` is Unix seconds.
pub fn sun_elevation(lat: f64, lon: f64, utc: f64) -> f64 {
    let e = geometric_elevation(lat, lon, utc);
    e + refraction(e)
}

/// Sun azimuth in degrees clockwise from north.
pub fn sun_azimuth(lat: f64, lon: f64, utc: f64) -> f64 {
    let (ha, decl) = hour_angle(lon, utc);
    let phi = lat.to_radians();
    // Measured from south, positive to the west, then turned to north.
    let from_south = ha.sin().atan2(ha.cos() * phi.sin() - decl.tan() * phi.cos());
    (from_south.to_degrees() + 180.0).rem_euclid(360.0)
}

/// The UTC moment of sunrise (`sign` -1) or sunset (`sign` 1) near `noon`,
/// refined by recomputing the sun's position at each estimate.
fn sun_event(lat: f64, lon: f64, noon: f64, day_start: f64, sign: f64) -> Option<f64> {
    let phi = lat.to_radians();
    let zenith = 90.833_f64.to_radians();
    let mut t = noon;
    for _ in 0..4 {
        let (eqtime, decl) = sun_angles(t);
        let c = zenith.cos() / (phi.cos() * decl.cos()) - phi.tan() * decl.tan();
        if !(-1.0..=1.0).contains(&c) {
            return None;
        }
        let ha = c.acos().to_degrees();
        let minutes = 720.0 - 4.0 * (lon - sign * ha) - eqtime;
        t = day_start + minutes * 60.0;
    }
    Some(t)
}

/// Sunrise and sunset (Unix seconds) for the local day containing `utc`, None in polar day or night.
///
/// The local day is the calendar day at the longitude's own offset
/// (`offset_from_longitude`), so the answer pairs the sunrise and sunset
/// around that day's solar noon.
pub fn sun_times(lat: f64, lon: f64, utc: f64) -> Option<(f64, f64)> {
    if !(lat.is_finite() && lon.is_finite() && utc.is_finite()) {
        return None;
    }
    let offset = offset_from_longitude(lon) as f64 * 60.0;
    let day_start = ((utc + offset) / DAY).floor() * DAY;
    // Solar noon of that local day, in UTC, close enough to start refining.
    let noon = day_start + (720.0 - 4.0 * lon) * 60.0;
    let rise = sun_event(lat, lon, noon, day_start, -1.0)?;
    let set = sun_event(lat, lon, noon, day_start, 1.0)?;
    Some((rise, set))
}

/// The light, named from the sun's elevation.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Light {
    Night,
    BlueHour,
    GoldenHour,
    Daylight,
    HarshMidday,
}

impl Light {
    /// Night below -8 degrees, blue hour -8 to -4, golden hour -4 to 6, daylight 6 to 50, harsh midday above 50 (document the thresholds).
    ///
    /// Each band includes its lower edge: -8 is blue hour, -4 is golden hour,
    /// 6 is daylight, and 50 is still daylight (harsh midday starts above it).
    /// A value that is not a number gives daylight, the neutral answer.
    pub fn from_elevation(deg: f64) -> Light {
        if deg.is_nan() {
            Light::Daylight
        } else if deg < -8.0 {
            Light::Night
        } else if deg < -4.0 {
            Light::BlueHour
        } else if deg < 6.0 {
            Light::GoldenHour
        } else if deg <= 50.0 {
            Light::Daylight
        } else {
            Light::HarshMidday
        }
    }

    pub fn words(self) -> &'static str {
        match self {
            Light::Night => "Night",
            Light::BlueHour => "Blue hour",
            Light::GoldenHour => "Golden hour",
            Light::Daylight => "Daylight",
            Light::HarshMidday => "Harsh midday",
        }
    }
}

/// UTC offset in minutes to assume for a clock reading with no zone: round(lon / 15) hours, clamped to -12..14.
pub fn offset_from_longitude(lon: f64) -> i32 {
    if !lon.is_finite() {
        return 0;
    }
    ((lon / 15.0).round().clamp(-12.0, 14.0) as i32) * 60
}

/// Local seconds split into the day count and the second of that day.
fn local_parts(utc: f64, offset_minutes: i32) -> (i64, i64) {
    let t = (utc + offset_minutes as f64 * 60.0).floor() as i64;
    (t.div_euclid(86_400), t.rem_euclid(86_400))
}

/// Format a Unix second count as local "HH:MM" given an offset in minutes, and the local calendar date "YYYY-MM-DD".
pub fn local_clock(utc: f64, offset_minutes: i32) -> String {
    let (_, s) = local_parts(utc, offset_minutes);
    format!("{:02}:{:02}", s / 3600, s % 3600 / 60)
}

pub fn local_date(utc: f64, offset_minutes: i32) -> String {
    let (days, _) = local_parts(utc, offset_minutes);
    let (y, m, d) = civil_from_days(days);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// Local hour of the day, 0 to 23.
pub fn local_hour(utc: f64, offset_minutes: i32) -> u32 {
    (local_parts(utc, offset_minutes).1 / 3600) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(y: i64, mo: u32, d: u32, h: u32, mi: u32) -> f64 {
        days_from_civil(y, mo, d) as f64 * DAY + h as f64 * 3600.0 + mi as f64 * 60.0
    }

    #[test]
    fn civil_round_trip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(days_from_civil(1969, 12, 31), -1);
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        for d in (-800_000..800_000).step_by(997) {
            let (y, m, dd) = civil_from_days(d);
            assert!(dd >= 1 && dd <= days_in_month(y, m));
            assert_eq!(days_from_civil(y, m, dd), d);
        }
    }

    #[test]
    fn parses_camera_timestamps() {
        let base = utc(2023, 5, 1, 10, 0);
        let t = parse_timestamp("2023-05-01T10:00:00.000000Z").unwrap();
        assert_eq!(t, Timestamp { wall: base, offset_minutes: Some(0) });
        assert_eq!(t.utc(), Some(base));

        let t = parse_timestamp("2023-05-01T10:00:00+0200").unwrap();
        assert_eq!(t.offset_minutes, Some(120));
        assert_eq!(t.utc(), Some(base - 7200.0));
        assert_eq!(parse_timestamp("2023-05-01T10:00:00+02:00"), Some(t));
        assert_eq!(parse_timestamp("2023-05-01T10:00:00+02").unwrap().offset_minutes, Some(120));

        let t = parse_timestamp("2023-05-01T10:00:00-0500").unwrap();
        assert_eq!(t.utc(), Some(base + 5.0 * 3600.0));
        assert_eq!(parse_timestamp("2023-05-01T10:00:00-03:30").unwrap().offset_minutes, Some(-210));
        assert_eq!(parse_timestamp("2023-05-01T10:00:00+05:45").unwrap().offset_minutes, Some(345));

        for s in [
            "2023-05-01 10:00:00",
            "2023:05:01 10:00:00",
            "2023-05-01T10:00:00",
            "2023/05/01 10:00:00",
            "2023-05-01t10:00:00",
            "2023-05-01T10:00",
            "  2023-05-01  10:00:00  ",
        ] {
            let t = parse_timestamp(s).unwrap_or_else(|| panic!("{s}"));
            assert_eq!(t, Timestamp { wall: base, offset_minutes: None }, "{s}");
            assert_eq!(t.utc(), None);
            assert_eq!(t.utc_assuming(60), base - 3600.0);
        }

        for (s, off) in [
            ("2023:05:01 10:00:00+02:00", 120),
            ("2023-05-01 10:00:00 +0200", 120),
            ("2023-05-01 10:00:00 UTC", 0),
            ("2023-05-01T10:00:00z", 0),
            ("2023-05-01 10:00:00 GMT-5", -300),
        ] {
            assert_eq!(parse_timestamp(s).unwrap().offset_minutes, Some(off), "{s}");
        }

        let t = parse_timestamp("2023-05-01T10:00:07.25Z").unwrap();
        assert!((t.wall - (base + 7.25)).abs() < 1e-6);
        let t = parse_timestamp("2016-12-31T23:59:60Z").unwrap();
        assert_eq!(t.wall, utc(2017, 1, 1, 0, 0));
        assert_eq!(parse_timestamp("1969-07-20T20:17:40Z").unwrap().wall, -14_182_940.0);
    }

    #[test]
    fn rejects_bad_timestamps() {
        for s in [
            "",
            "hello",
            "2023-05-01",
            "2023-13-01 10:00:00",
            "2023-00-01 10:00:00",
            "2023-02-29 10:00:00",
            "1900-02-29 10:00:00",
            "2023-04-31 10:00:00",
            "2023-05-00 10:00:00",
            "2023-05-01 24:00:00",
            "2023-05-01 10:60:00",
            "2023-05-01 10:00:61",
            "2023-05-01X10:00:00",
            "2023-05:01 10:00:00",
            "23-05-01 10:00:00",
            "2023-5-1 10:00:00",
            "2023-05-01 10:00:00+2500",
            "2023-05-01 10:00:00+02:75",
            "2023-05-01 10:00:00 PST",
            "2023-05-01 10:00:00.Z",
            "2023-05-01 10:00:00+020",
            "2023-05-01 10",
            "２023-05-01 10:00:00",
        ] {
            assert_eq!(parse_timestamp(s), None, "{s}");
        }
        assert!(parse_timestamp("2024-02-29 10:00:00").is_some());
        assert!(parse_timestamp("2000-02-29 10:00:00").is_some());
    }

    #[test]
    fn parses_iso6709() {
        let close = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9;
        assert!(close(parse_iso6709("+48.8584+002.2945+035.000/").unwrap(), (48.8584, 2.2945)));
        assert!(close(parse_iso6709("+48.8584+002.2945/").unwrap(), (48.8584, 2.2945)));
        assert!(close(parse_iso6709("-33.8688+151.2093/").unwrap(), (-33.8688, 151.2093)));
        assert!(close(parse_iso6709("+40.7128-074.0060-010.5/").unwrap(), (40.7128, -74.006)));
        assert!(close(parse_iso6709(" +40.7128-074.0060 ").unwrap(), (40.7128, -74.006)));
        assert!(close(parse_iso6709("+35.6895+139.6917+040.000/CRSWGS_84/").unwrap(), (35.6895, 139.6917)));
        assert!(close(parse_iso6709("+48+002/").unwrap(), (48.0, 2.0)));
        // Degrees and minutes, then degrees, minutes and seconds.
        assert!(close(parse_iso6709("+4851.5+00217.5/").unwrap(), (48.0 + 51.5 / 60.0, 2.0 + 17.5 / 60.0)));
        assert!(close(
            parse_iso6709("-335207.7+1511233.5/").unwrap(),
            (-(33.0 + 52.0 / 60.0 + 7.7 / 3600.0), 151.0 + 12.0 / 60.0 + 33.5 / 3600.0)
        ));
        for s in [
            "",
            "/",
            "+48.8584/",
            "48.8584+002.2945/",
            "+91.0000+002.2945/",
            "+48.8584+181.0000/",
            "+48.8584+002.2945+035.000+1/",
            "+48.85.84+002.2945/",
            "+48.8584+0a2.2945/",
            "+48.8584+002.2945+/",
            "+4875.0+00217.5/",
            "+00.0000+000.0000/",
            "+-48.8584+002.2945/",
            "hello",
        ] {
            assert_eq!(parse_iso6709(s), None, "{s}");
        }
    }

    #[test]
    fn sun_elevation_matches_reference_values() {
        // References from the full NOAA spreadsheet (Meeus based) equations.
        let cases = [
            (48.8566, 2.3522, utc(2024, 6, 21, 12, 0), 64.55, 184.0),
            (-33.8688, 151.2093, utc(2024, 6, 21, 2, 0), 32.71, 359.2),
            (40.7128, -74.0060, utc(2024, 12, 21, 17, 0), 25.87, 181.4),
            (69.6496, 18.9560, utc(2024, 6, 21, 22, 45), 3.31, 359.7),
            (51.5074, -0.1278, utc(2024, 3, 20, 15, 0), 27.25, 230.2),
            (34.0522, -118.2437, utc(2024, 7, 4, 3, 0), 1.06, 297.4),
        ];
        for (lat, lon, t, el, az) in cases {
            let e = sun_elevation(lat, lon, t);
            assert!((e - el).abs() < 0.3, "elevation {e} expected {el} at {lat},{lon}");
            let a = sun_azimuth(lat, lon, t);
            let da = ((a - az + 540.0) % 360.0 - 180.0).abs();
            assert!(da < 1.0, "azimuth {a} expected {az} at {lat},{lon}");
        }
        // Paris is never higher than about 64.6 degrees.
        assert!(sun_elevation(48.8566, 2.3522, utc(2024, 12, 21, 0, 0)) < -60.0);
    }

    #[test]
    fn sunrise_and_sunset() {
        let near = |a: f64, b: f64| (a - b).abs() < 120.0;
        // New York, 2024-12-21: 12:16:50 and 21:32:10 UTC.
        let (rise, set) = sun_times(40.7128, -74.0060, utc(2024, 12, 21, 17, 0)).unwrap();
        assert!(near(rise, utc(2024, 12, 21, 12, 16) + 50.0), "{}", local_clock(rise, 0));
        assert!(near(set, utc(2024, 12, 21, 21, 32) + 10.0), "{}", local_clock(set, 0));
        // Any moment of the same local day gives the same answer.
        let late = sun_times(40.7128, -74.0060, utc(2024, 12, 22, 4, 0)).unwrap();
        assert!(near(late.0, rise) && near(late.1, set));
        // Paris, 2024-06-21: 03:47:10 and 19:58:00 UTC.
        let (rise, set) = sun_times(48.8566, 2.3522, utc(2024, 6, 21, 12, 0)).unwrap();
        assert!(near(rise, utc(2024, 6, 21, 3, 47) + 10.0), "{}", local_clock(rise, 0));
        assert!(near(set, utc(2024, 6, 21, 19, 58)), "{}", local_clock(set, 0));
        // Sydney, local 2024-06-21: 21:00:10 the day before and 06:54 UTC.
        let (rise, set) = sun_times(-33.8688, 151.2093, utc(2024, 6, 21, 2, 0)).unwrap();
        assert!(near(rise, utc(2024, 6, 20, 21, 0) + 10.0), "{}", local_clock(rise, 0));
        assert!(near(set, utc(2024, 6, 21, 6, 54)), "{}", local_clock(set, 0));
        // Tromso has midnight sun in June and polar night in December.
        assert_eq!(sun_times(69.6496, 18.9560, utc(2024, 6, 21, 12, 0)), None);
        assert_eq!(sun_times(69.6496, 18.9560, utc(2024, 12, 21, 12, 0)), None);
        assert!(sun_elevation(69.6496, 18.9560, utc(2024, 6, 21, 22, 45)) > 0.0);
        assert_eq!(sun_times(f64::NAN, 0.0, 0.0), None);
    }

    #[test]
    fn names_the_light() {
        assert_eq!(Light::from_elevation(-30.0), Light::Night);
        assert_eq!(Light::from_elevation(-8.01), Light::Night);
        assert_eq!(Light::from_elevation(-8.0), Light::BlueHour);
        assert_eq!(Light::from_elevation(-5.0), Light::BlueHour);
        assert_eq!(Light::from_elevation(-4.0), Light::GoldenHour);
        assert_eq!(Light::from_elevation(3.0), Light::GoldenHour);
        assert_eq!(Light::from_elevation(6.0), Light::Daylight);
        assert_eq!(Light::from_elevation(50.0), Light::Daylight);
        assert_eq!(Light::from_elevation(50.1), Light::HarshMidday);
        assert_eq!(Light::from_elevation(f64::NAN), Light::Daylight);
        assert_eq!(Light::BlueHour.words(), "Blue hour");
        assert_eq!(Light::HarshMidday.words(), "Harsh midday");
        assert_eq!(serde_json::to_string(&Light::GoldenHour).unwrap(), "\"golden_hour\"");
        // Paris at midsummer noon is harsh, Sydney at midwinter noon is not.
        assert_eq!(Light::from_elevation(sun_elevation(48.8566, 2.3522, utc(2024, 6, 21, 12, 0))), Light::HarshMidday);
        assert_eq!(Light::from_elevation(sun_elevation(-33.8688, 151.2093, utc(2024, 6, 21, 2, 0))), Light::Daylight);
    }

    #[test]
    fn offsets_from_longitude() {
        assert_eq!(offset_from_longitude(0.0), 0);
        assert_eq!(offset_from_longitude(2.35), 0);
        assert_eq!(offset_from_longitude(7.6), 60);
        assert_eq!(offset_from_longitude(-74.0), -300);
        assert_eq!(offset_from_longitude(151.2), 600);
        assert_eq!(offset_from_longitude(-180.0), -720);
        assert_eq!(offset_from_longitude(180.0), 720);
        assert_eq!(offset_from_longitude(250.0), 840);
        assert_eq!(offset_from_longitude(-250.0), -720);
        assert_eq!(offset_from_longitude(f64::NAN), 0);
    }

    #[test]
    fn local_clock_and_date_cross_midnight() {
        let t = utc(2024, 6, 21, 23, 30);
        assert_eq!(local_clock(t, 0), "23:30");
        assert_eq!(local_date(t, 0), "2024-06-21");
        assert_eq!(local_clock(t, 120), "01:30");
        assert_eq!(local_date(t, 120), "2024-06-22");
        let t = utc(2024, 1, 1, 2, 15);
        assert_eq!(local_clock(t, -300), "21:15");
        assert_eq!(local_date(t, -300), "2023-12-31");
        assert_eq!(local_clock(t, 345), "08:00");
        assert_eq!(local_date(utc(2024, 2, 28, 22, 0), 180), "2024-02-29");
        assert_eq!(local_date(utc(2023, 2, 28, 22, 0), 180), "2023-03-01");
        // Seconds are dropped, not rounded.
        assert_eq!(local_clock(utc(2024, 6, 21, 10, 59) + 59.9, 0), "10:59");
        assert_eq!(local_clock(-1.0, 0), "23:59");
        assert_eq!(local_date(-1.0, 0), "1969-12-31");
        assert_eq!(local_hour(utc(2024, 6, 21, 23, 30), 120), 1);
    }
}
