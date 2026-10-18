//! When and where each clip was recorded, the light it was recorded in, the
//! scene it belongs to, and white balance compared within each scene.
//!
//! Recording time, in order of trust:
//! 1. The camera's own file with a time zone (Sony's `M01.XML` CreationDate).
//! 2. Apple's creation date tag, which carries the time zone.
//! 3. A phone's `creation_time` tag, which phones write in true UTC.
//! 4. The camera's clock time with no zone: DJI captions, and the
//!    `creation_time` tag of other cameras, which most write as their own
//!    clock time labelled UTC.
//!
//! A clock time with no zone gets one from, in order: the zone chosen in the
//! project, the zone most other clips in the project state, the place's
//! longitude (15 degrees an hour, so it can be an hour out), or none.
//!
//! Place: the camera's file (DJI captions), the file's ISO 6709 tag (phones),
//! borrowed from the clip recorded nearest in time when within three hours,
//! or the place chosen in the project.
//!
//! Light comes from the sun's height above the horizon at the middle of the
//! clip, which needs both a true time and a place.

use crate::places::{borrow_places, group_clips, Group, WhenWhere};
use crate::project::{Project, ProjectClip};
use crate::solar::{offset_from_longitude, parse_iso6709, sun_elevation, Light, Timestamp};

/// Cameras with an unset clock write 1970 or similar; anything before 2000 is ignored.
fn parse_timestamp(s: &str) -> Option<Timestamp> {
    crate::solar::parse_timestamp(s).filter(|t| t.wall >= 946_684_800.0)
}
use serde::{Deserialize, Serialize};

/// How far apart in time two clips may be for one to lend the other its place.
pub const BORROW_SECONDS: f64 = 3.0 * 3600.0;

/// Places and clock settings the person can give for the whole project.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct PlaceSettings {
    /// Where the footage was shot, for clips that do not record it.
    pub location: Option<(f64, f64)>,
    /// The cameras' clock time zone in minutes east of UTC, for clocks that
    /// do not say. None means work it out.
    pub clock_offset_minutes: Option<i32>,
}

/// Where a reading came from, in plain words.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ClipScene {
    pub id: u64,
    /// Recording start in true UTC seconds, when the time zone is known or estimated.
    pub utc: Option<f64>,
    /// Recording start as the clock read, as if UTC, when any time is known.
    pub wall: Option<f64>,
    /// Minutes east of UTC used for this clip.
    pub offset_minutes: Option<i32>,
    /// Where the time came from, for example "the camera's file C0001M01.XML".
    pub time_source: Option<String>,
    /// The time zone was worked out rather than stated.
    pub zone_estimated: bool,
    pub place: Option<(f64, f64)>,
    pub place_source: Option<String>,
    /// Sun height in degrees at the middle of the clip.
    pub sun: Option<f64>,
    pub light: Option<Light>,
    /// Name of the scene group, such as "Day 1, morning".
    pub group: Option<String>,
    /// Typical white balance cue of the clip (Cb, Cr of near-neutral areas).
    pub wb: Option<(f32, f32)>,
    /// How the clip's white balance compares with others in its scene.
    pub wb_note: Option<String>,
}

/// What a recorded time says about its time zone.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Clock {
    /// The file states the zone, which is the local zone where it was shot.
    Stated(i32),
    /// The time is true UTC; the local zone is not known. True when it comes
    /// from a camera, where a zone the person chooses wins, in case the
    /// camera wrote its clock time instead.
    Utc(bool),
    /// The camera's clock time with no zone.
    Unknown,
}

struct TimeReading {
    /// The time as written, read as if UTC.
    wall: f64,
    clock: Clock,
    source: String,
}

/// Sony cameras write `creation_time` in true UTC, worked out from the
/// camera's own time zone setting. Their files carry the XAVC brand, or the
/// maker's name, and are named C0001.MP4 and so on.
fn sony_like(c: &ProjectClip) -> bool {
    let Some(info) = c.info.as_ref() else { return false };
    let t = &info.tags;
    if t.brand.as_deref().is_some_and(|b| b.trim().to_ascii_uppercase().starts_with("XAVC")) {
        return true;
    }
    if t.make.as_deref().is_some_and(|m| m.to_ascii_lowercase().contains("sony")) {
        return true;
    }
    let name = info.name.to_ascii_uppercase();
    let b = name.as_bytes();
    b.len() == 9 && b[0] == b'C' && b[1..5].iter().all(|d| d.is_ascii_digit()) && name.ends_with(".MP4")
}

fn time_reading(c: &ProjectClip) -> Option<TimeReading> {
    let info = c.info.as_ref()?;
    let cam = info.camera.as_ref();
    let tags = &info.tags;
    let camera_file = cam.map(|n| n.source.clone()).unwrap_or_default();
    if let Some(ts) = cam.and_then(|n| n.recorded.as_deref()).and_then(parse_timestamp) {
        if let Some(o) = ts.offset_minutes {
            return Some(TimeReading { wall: ts.wall, clock: Clock::Stated(o), source: format!("the camera's file {camera_file}") });
        }
    }
    if let Some(ts) = tags.apple_creation.as_deref().and_then(parse_timestamp) {
        if let Some(o) = ts.offset_minutes {
            return Some(TimeReading { wall: ts.wall, clock: Clock::Stated(o), source: "the phone's creation date".into() });
        }
    }
    if tags.phone {
        if let Some(ts) = tags.creation_time.as_deref().and_then(parse_timestamp) {
            let wall = ts.utc().unwrap_or(ts.wall);
            return Some(TimeReading { wall, clock: Clock::Utc(false), source: "the phone's recording time".into() });
        }
    }
    if let Some(ts) = cam.and_then(|n| n.recorded.as_deref()).and_then(parse_timestamp) {
        return Some(TimeReading { wall: ts.wall, clock: Clock::Unknown, source: format!("the camera's clock, in {camera_file}") });
    }
    if let Some(ts) = tags.creation_time.as_deref().and_then(parse_timestamp) {
        if sony_like(c) {
            let wall = ts.utc().unwrap_or(ts.wall);
            return Some(TimeReading { wall, clock: Clock::Utc(true), source: "the camera's recording time, which Sony cameras write in UTC".into() });
        }
        // Most other cameras write their own clock time here with a UTC label.
        return Some(TimeReading { wall: ts.wall, clock: Clock::Unknown, source: "the camera's clock".into() });
    }
    None
}

fn own_place(c: &ProjectClip) -> Option<((f64, f64), String)> {
    let info = c.info.as_ref()?;
    if let Some(n) = &info.camera {
        if let (Some(lat), Some(lon)) = (n.latitude, n.longitude) {
            return Some(((lat, lon), format!("the camera's file {}", n.source)));
        }
    }
    let p = info.tags.location.as_deref().and_then(parse_iso6709)?;
    Some((p, "the file's location tag".into()))
}

/// The most common stated time zone among the project's clips.
fn common_offset(readings: &[Option<TimeReading>]) -> Option<i32> {
    let mut counts: Vec<(i32, usize)> = Vec::new();
    for r in readings.iter().flatten() {
        let Clock::Stated(o) = r.clock else { continue };
        match counts.iter_mut().find(|c| c.0 == o) {
            Some(c) => c.1 += 1,
            None => counts.push((o, 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1));
    counts.first().map(|c| c.0)
}

/// How far, in minutes, a zone may sit from the one the longitude suggests
/// and still be taken as the local zone there (summer time, wide zones).
const LOCAL_REACH: i32 = 90;

/// The local zone to show times and name scenes in: the stated zone when
/// there is one, else a clock zone or the project's common zone that fits the
/// place, else the place's longitude. A camera clock left on UTC away from
/// Greenwich says nothing about local time.
fn local_offset(clock: Option<i32>, common: Option<i32>, place: Option<(f64, f64)>) -> Option<i32> {
    let lon = place.map(|p| offset_from_longitude(p.1));
    for c in [clock, common].into_iter().flatten() {
        if lon.is_none_or(|l| (c - l).abs() <= LOCAL_REACH) {
            return Some(c);
        }
    }
    lon.or(clock).or(common)
}

/// Brightness at or above which a clip is plainly not shot in the dark, and
/// at or below which it plainly is, on the analysis scale (video black 0,
/// white 255). Log footage of daylight sits around 90 to 130, night well
/// under 30.
const BRIGHT_LEVEL: f32 = 70.0;
const DARK_LEVEL: f32 = 30.0;

/// The clip's typical brightness, from its analysed frames.
fn clip_brightness(c: &ProjectClip) -> Option<f32> {
    let f = &c.analysis.as_ref()?.frames;
    if f.mean.len() < 3 {
        return None;
    }
    median(f.mean.clone())
}

/// How many clips a guessed clock zone puts in the wrong light: bright
/// pictures with the sun well below the horizon, or dark pictures in full day.
fn light_mismatches(clips: &[(f64, f64, (f64, f64), f32)], offset: i32) -> usize {
    clips
        .iter()
        .filter(|&&(wall, duration, (lat, lon), bright)| {
            let e = sun_elevation(lat, lon, wall - offset as f64 * 60.0 + duration / 2.0);
            (bright >= BRIGHT_LEVEL && e < -6.0) || (bright <= DARK_LEVEL && e > 10.0)
        })
        .count()
}

/// When, where, light and scene for every clip, in project order.
pub fn resolve(project: &Project) -> Vec<ClipScene> {
    let settings = &project.places;
    let readings: Vec<Option<TimeReading>> = project.clips.iter().map(time_reading).collect();
    let common = common_offset(&readings);
    let own: Vec<Option<((f64, f64), String)>> = project.clips.iter().map(own_place).collect();
    // The zone of the clock behind each reading, when known or chosen, and
    // whether it was chosen by the person.
    let clock_zone = |r: &TimeReading| -> Option<(i32, bool)> {
        match r.clock {
            Clock::Stated(o) => Some((o, true)),
            Clock::Utc(true) => Some((settings.clock_offset_minutes.unwrap_or(0), true)),
            Clock::Utc(false) => Some((0, true)),
            Clock::Unknown => settings.clock_offset_minutes.map(|o| (o, true)),
        }
    };
    let mut scenes: Vec<ClipScene> = Vec::with_capacity(project.clips.len());
    for (i, c) in project.clips.iter().enumerate() {
        let r = readings[i].as_ref();
        let place = own[i].as_ref().map(|p| p.0);
        let utc = r.and_then(|r| clock_zone(r).map(|(o, _)| r.wall - o as f64 * 60.0));
        scenes.push(ClipScene {
            id: c.id,
            utc,
            wall: r.map(|r| r.wall),
            offset_minutes: None,
            time_source: r.map(|r| r.source.clone()),
            zone_estimated: false,
            place,
            place_source: own[i].as_ref().map(|p| p.1.clone()),
            sun: None,
            light: None,
            group: None,
            wb: clip_wb(c),
            wb_note: None,
        });
    }
    // Clips without a place borrow one from the clip recorded nearest in time.
    let items = when_where(project, &scenes);
    for (id, p, from) in borrow_places(&items, BORROW_SECONDS) {
        let from_name = project.clip(from).and_then(|c| c.info.as_ref()).map(|i| i.name.clone()).unwrap_or_default();
        if let Some(sc) = scenes.iter_mut().find(|s| s.id == id && s.place.is_none()) {
            sc.place = Some(p);
            sc.place_source = Some(format!("{from_name}, recorded nearby in time"));
        }
    }
    for sc in scenes.iter_mut().filter(|s| s.place.is_none()) {
        if let Some(p) = settings.location {
            sc.place = Some(p);
            sc.place_source = Some("the place chosen for this project".into());
        }
    }
    // Clocks with no zone: the zone other clips state, else whichever of
    // "the clock is on UTC" and "the clock is on the place's local time"
    // agrees better with how bright the pictures are, else local time.
    let guessed: Option<i32> = common.or_else(|| {
        let clips: Vec<(f64, f64, (f64, f64), f32)> = project
            .clips
            .iter()
            .zip(scenes.iter())
            .zip(readings.iter())
            .filter_map(|((c, sc), r)| {
                let r = r.as_ref()?;
                if r.clock != Clock::Unknown {
                    return None;
                }
                Some((r.wall, c.info.as_ref()?.duration, sc.place?, clip_brightness(c)?))
            })
            .collect();
        let lon = scenes
            .iter()
            .zip(readings.iter())
            .filter(|(_, r)| r.as_ref().is_some_and(|r| r.clock == Clock::Unknown))
            .find_map(|(sc, _)| sc.place)
            .map(|p| offset_from_longitude(p.1))?;
        if lon != 0 && light_mismatches(&clips, 0) < light_mismatches(&clips, lon) {
            Some(0)
        } else {
            None
        }
    });
    for (i, sc) in scenes.iter_mut().enumerate() {
        let Some(r) = readings[i].as_ref() else { continue };
        let chosen = clock_zone(r);
        let clock = match chosen {
            Some((o, _)) => Some(o),
            None => guessed.or(sc.place.map(|p| offset_from_longitude(p.1))),
        };
        if sc.utc.is_none() {
            sc.utc = clock.map(|o| r.wall - o as f64 * 60.0);
        }
        let (local, estimated) = match r.clock {
            Clock::Stated(o) => (Some(o), false),
            Clock::Utc(_) => (local_offset(None, common, sc.place), true),
            Clock::Unknown => {
                let local = local_offset(clock, common, sc.place);
                // Exact only when the person chose the clock's zone and it is the local one.
                (local, !(chosen.is_some() && local == clock))
            }
        };
        sc.offset_minutes = if sc.utc.is_some() { local } else { None };
        sc.zone_estimated = sc.utc.is_some() && estimated;
    }
    for (c, sc) in project.clips.iter().zip(scenes.iter_mut()) {
        if let (Some(utc), Some((lat, lon))) = (sc.utc, sc.place) {
            let mid = utc + c.info.as_ref().map(|i| i.duration).unwrap_or(0.0) / 2.0;
            let e = sun_elevation(lat, lon, mid);
            sc.sun = Some((e * 10.0).round() / 10.0);
            sc.light = Some(Light::from_elevation(e));
        }
    }
    let groups = group_clips(&when_where(project, &scenes));
    for g in &groups {
        for id in &g.clip_ids {
            if let Some(sc) = scenes.iter_mut().find(|s| s.id == *id) {
                sc.group = Some(g.name.clone());
            }
        }
    }
    add_wb_notes(project, &mut scenes);
    scenes
}

/// Scene groups for the project.
pub fn groups(project: &Project, scenes: &[ClipScene]) -> Vec<Group> {
    group_clips(&when_where(project, scenes))
}

fn when_where(project: &Project, scenes: &[ClipScene]) -> Vec<WhenWhere> {
    project
        .clips
        .iter()
        .zip(scenes)
        .filter(|(c, _)| c.info.is_some())
        .map(|(c, sc)| WhenWhere {
            id: c.id,
            // Clocks with no zone still order clips within a shoot.
            utc: sc.utc.or(sc.wall),
            offset_minutes: sc.offset_minutes.or(sc.wall.map(|_| 0)),
            place: sc.place,
            duration: c.info.as_ref().map(|i| i.duration).unwrap_or(0.0),
            folder: std::path::Path::new(&c.path)
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
        })
        .collect()
}

fn median(mut v: Vec<f32>) -> Option<f32> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(v[v.len() / 2])
}

/// The clip's typical white balance cue over its measured frames.
pub fn clip_wb(c: &ProjectClip) -> Option<(f32, f32)> {
    let f = &c.analysis.as_ref()?.frames;
    if !f.has_picture_checks() {
        return None;
    }
    let ok: Vec<usize> = (0..f.len()).filter(|&i| f.wb_ok[i] != 0).collect();
    if ok.len() < 3 || ok.len() * 3 < f.len() {
        return None;
    }
    Some((median(ok.iter().map(|&i| f.cb[i]).collect())?, median(ok.iter().map(|&i| f.cr[i]).collect())?))
}

/// Temperature or tint difference, in chroma codes, from which a clip is
/// said to look different from the rest of its scene, and from which the
/// difference is called clear.
pub const WB_NOTICE: f32 = 3.5;
pub const WB_CLEAR: f32 = 7.0;

fn add_wb_notes(project: &Project, scenes: &mut [ClipScene]) {
    // Compare clips in the same scene and of the same kind (log is less saturated).
    let mut keys: Vec<(String, bool)> = Vec::new();
    for (c, sc) in project.clips.iter().zip(scenes.iter()) {
        if let (Some(g), Some(_)) = (&sc.group, sc.wb) {
            let k = (g.clone(), c.resolved_profile(project.fallback_profile).profile.is_log());
            if !keys.contains(&k) {
                keys.push(k);
            }
        }
    }
    for (group, log) in keys {
        let members: Vec<usize> = (0..scenes.len())
            .filter(|&i| {
                scenes[i].group.as_deref() == Some(group.as_str())
                    && scenes[i].wb.is_some()
                    && project.clips[i].resolved_profile(project.fallback_profile).profile.is_log() == log
            })
            .collect();
        if members.len() < 3 {
            continue;
        }
        let tt: Vec<(f32, f32)> = members.iter().map(|&i| crate::picture::temperature_tint(scenes[i].wb.unwrap().0, scenes[i].wb.unwrap().1)).collect();
        let mt = median(tt.iter().map(|x| x.0).collect()).unwrap_or(0.0);
        let mn = median(tt.iter().map(|x| x.1).collect()).unwrap_or(0.0);
        for (k, &i) in members.iter().enumerate() {
            let (dt, dn) = (tt[k].0 - mt, tt[k].1 - mn);
            let (amount, word) = if dt.abs() >= dn.abs() {
                (dt.abs(), if dt > 0.0 { "warmer" } else { "cooler" })
            } else {
                (dn.abs(), if dn > 0.0 { "greener" } else { "more magenta" })
            };
            if amount >= WB_NOTICE {
                let how = if amount >= WB_CLEAR { "Clearly" } else { "A little" };
                scenes[i].wb_note = Some(format!("{how} {word} than the other clips from {group}. Match white balance before grading them together."));
            }
        }
    }
}
