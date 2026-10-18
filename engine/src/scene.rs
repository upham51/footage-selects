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

struct TimeReading {
    ts: Timestamp,
    zone_known: bool,
    source: String,
}

fn time_reading(c: &ProjectClip) -> Option<TimeReading> {
    let info = c.info.as_ref()?;
    let cam = info.camera.as_ref();
    let tags = &info.tags;
    let camera_file = cam.map(|n| n.source.clone()).unwrap_or_default();
    if let Some(ts) = cam.and_then(|n| n.recorded.as_deref()).and_then(parse_timestamp) {
        if ts.offset_minutes.is_some() {
            return Some(TimeReading { ts, zone_known: true, source: format!("the camera's file {camera_file}") });
        }
    }
    if let Some(ts) = tags.apple_creation.as_deref().and_then(parse_timestamp) {
        if ts.offset_minutes.is_some() {
            return Some(TimeReading { ts, zone_known: true, source: "the phone's creation date".into() });
        }
    }
    if tags.phone {
        if let Some(ts) = tags.creation_time.as_deref().and_then(parse_timestamp) {
            let ts = Timestamp { wall: ts.wall, offset_minutes: Some(ts.offset_minutes.unwrap_or(0)) };
            return Some(TimeReading { ts, zone_known: true, source: "the phone's recording time".into() });
        }
    }
    if let Some(ts) = cam.and_then(|n| n.recorded.as_deref()).and_then(parse_timestamp) {
        let ts = Timestamp { wall: ts.wall, offset_minutes: None };
        return Some(TimeReading { ts, zone_known: false, source: format!("the camera's clock, in {camera_file}") });
    }
    if let Some(ts) = tags.creation_time.as_deref().and_then(parse_timestamp) {
        // Most cameras write their own clock time here with a UTC label.
        let ts = Timestamp { wall: ts.wall, offset_minutes: None };
        return Some(TimeReading { ts, zone_known: false, source: "the camera's clock".into() });
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
    for r in readings.iter().flatten().filter(|r| r.zone_known && r.source != "the phone's recording time") {
        let o = r.ts.offset_minutes.unwrap_or(0);
        match counts.iter_mut().find(|c| c.0 == o) {
            Some(c) => c.1 += 1,
            None => counts.push((o, 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1));
    counts.first().map(|c| c.0)
}

/// When, where, light and scene for every clip, in project order.
pub fn resolve(project: &Project) -> Vec<ClipScene> {
    let settings = &project.places;
    let readings: Vec<Option<TimeReading>> = project.clips.iter().map(time_reading).collect();
    let common = common_offset(&readings);
    let own: Vec<Option<((f64, f64), String)>> = project.clips.iter().map(own_place).collect();
    let mut scenes: Vec<ClipScene> = Vec::with_capacity(project.clips.len());
    for (i, c) in project.clips.iter().enumerate() {
        let r = readings[i].as_ref();
        let place = own[i].as_ref().map(|p| p.0);
        let (offset, estimated) = match r {
            Some(r) if r.zone_known => (r.ts.offset_minutes, false),
            Some(_) => match (settings.clock_offset_minutes, common, place) {
                (Some(o), _, _) => (Some(o), false),
                (None, Some(o), _) => (Some(o), true),
                (None, None, Some((_, lon))) => (Some(offset_from_longitude(lon)), true),
                _ => (None, false),
            },
            None => (None, false),
        };
        let utc = r.and_then(|r| offset.map(|o| r.ts.wall - o as f64 * 60.0));
        scenes.push(ClipScene {
            id: c.id,
            utc,
            wall: r.map(|r| r.ts.wall),
            offset_minutes: offset,
            time_source: r.map(|r| r.source.clone()),
            zone_estimated: estimated,
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
    // A clock with no zone and no other clue can take one from a place found since.
    for (i, sc) in scenes.iter_mut().enumerate() {
        if sc.utc.is_none() {
            if let (Some(r), Some((_, lon))) = (readings[i].as_ref(), sc.place) {
                let o = offset_from_longitude(lon);
                sc.offset_minutes = Some(o);
                sc.zone_estimated = true;
                sc.utc = Some(r.ts.wall - o as f64 * 60.0);
            }
        }
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
