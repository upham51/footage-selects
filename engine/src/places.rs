//! Grouping clips into scenes by when and where they were shot.
//!
//! A scene is a run of clips without a long pause and without moving far.
//! Groups get plain names from the shoot day and the part of the day ("Day 2,
//! evening"), with a place number or a clock time only when that is needed
//! to tell two groups apart. Clips that carry no time are grouped by folder.

use crate::solar::{local_clock, local_date, local_hour, offset_from_longitude};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// A pause longer than this between clips starts a new group (45 minutes).
pub const GAP_SECONDS: f64 = 2700.0;
/// A clip further than this from its group's centre starts a new group.
pub const PLACE_METRES: f64 = 1500.0;

/// Mean radius of the Earth used for distances, in metres.
const EARTH_RADIUS: f64 = 6_371_008.8;

#[derive(Clone, Debug, PartialEq)]
pub struct WhenWhere {
    pub id: u64,
    /// Recording start as true UTC seconds, when known.
    pub utc: Option<f64>,
    /// Offset in minutes from UTC of the place, known or estimated, for naming days and parts of day.
    pub offset_minutes: Option<i32>,
    pub place: Option<(f64, f64)>,
    pub duration: f64,
    /// Folder of the file, used to group clips that have no time.
    pub folder: String,
}

impl WhenWhere {
    fn start(&self) -> Option<f64> {
        self.utc.filter(|t| t.is_finite())
    }

    fn length(&self) -> f64 {
        if self.duration.is_finite() && self.duration > 0.0 {
            self.duration
        } else {
            0.0
        }
    }

    fn position(&self) -> Option<(f64, f64)> {
        self.place.filter(|(a, b)| a.is_finite() && b.is_finite())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Group {
    /// Plain name such as "Day 1, morning" or "Day 2, evening, place 3" or a folder name for clips without time.
    pub name: String,
    pub clip_ids: Vec<u64>,
    /// Local calendar date "YYYY-MM-DD" when known.
    pub date: Option<String>,
    /// Centre of the group's GPS positions.
    pub place: Option<(f64, f64)>,
    pub start_utc: Option<f64>,
    pub end_utc: Option<f64>,
}

/// Great-circle distance in metres.
pub fn distance_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (la1, lo1) = (a.0.to_radians(), a.1.to_radians());
    let (la2, lo2) = (b.0.to_radians(), b.1.to_radians());
    let h = ((la2 - la1) / 2.0).sin().powi(2) + la1.cos() * la2.cos() * ((lo2 - lo1) / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS * h.sqrt().min(1.0).asin()
}

/// Running centre of a set of positions, averaged as unit vectors so groups
/// across the date line or near a pole come out right.
#[derive(Clone, Copy, Default)]
struct Centre {
    x: f64,
    y: f64,
    z: f64,
    n: usize,
}

impl Centre {
    fn add(&mut self, (lat, lon): (f64, f64)) {
        let (la, lo) = (lat.to_radians(), lon.to_radians());
        self.x += la.cos() * lo.cos();
        self.y += la.cos() * lo.sin();
        self.z += la.sin();
        self.n += 1;
    }

    fn get(&self) -> Option<(f64, f64)> {
        if self.n == 0 {
            return None;
        }
        let h = (self.x * self.x + self.y * self.y).sqrt();
        if h < 1e-12 && self.z.abs() < 1e-12 {
            return None;
        }
        Some((self.z.atan2(h).to_degrees(), self.y.atan2(self.x).to_degrees()))
    }
}

/// A group of timed clips while it is being built.
struct Building {
    items: Vec<usize>,
    start: f64,
    end: f64,
    centre: Centre,
}

fn part_of_day(hour: u32) -> &'static str {
    match hour {
        5..=11 => "morning",
        12..=16 => "afternoon",
        17..=20 => "evening",
        _ => "night",
    }
}

/// The last part of a folder path, for naming a group of untimed clips.
fn folder_name(folder: &str) -> String {
    let trimmed = folder.trim_end_matches(['/', '\\']);
    let last = trimmed.rsplit(['/', '\\']).next().unwrap_or("");
    if last.is_empty() {
        if trimmed.is_empty() && folder.is_empty() {
            "Clips without a time".to_string()
        } else if trimmed.is_empty() {
            folder.to_string()
        } else {
            trimmed.to_string()
        }
    } else {
        last.to_string()
    }
}

/// Group clips into scenes: sort by time, start a new group at a gap of more than `GAP_SECONDS` (45 minutes) between one clip's end and the next one's start, or when the next clip with a position is more than `PLACE_METRES` (1500 m) from the group's centre. Clips with no time are grouped by folder after the timed groups. Days are numbered from the first local date in the footage (Day 1, Day 2, ...); parts of day: night (before 5), morning (5 to 12), afternoon (12 to 17), evening (17 to 21), night (21 on). When one day and part of day has several groups, add ", place N" numbering places in order of first appearance across the whole shoot (the same place visited twice keeps its number; reuse a place number when within PLACE_METRES of an earlier group's centre), and when that still collides add a time "at 14:05". Output groups in time order. Every input id appears in exactly one group.
///
/// Details:
/// 1. Clips recorded at the same time in two places (two cameras) can
///    interleave. A clip joins the open group at its place (nearest centre
///    within `PLACE_METRES`, last clip ending no more than `GAP_SECONDS`
///    before it) rather than splitting the other camera's group; a clip
///    without a position joins the group most recently added to, if still
///    open. Otherwise it starts a new group.
/// 2. A group's date, day and part of day come from its first clip's local
///    time: the first clip offset given in the group, else the longitude of
///    its centre (`offset_from_longitude`), else UTC.
/// 3. Days count shooting days: the first local date is Day 1, the next date
///    with footage is Day 2, even if days without footage lie between.
/// 4. Untimed groups are named after the last part of the folder (the full
///    folder when two share that name) and come after the timed groups,
///    ordered by folder.
/// 5. Names that are still the same after the place and clock time get
///    " (2)", " (3)" and so on, so every name is unique.
pub fn group_clips(items: &[WhenWhere]) -> Vec<Group> {
    let mut timed: Vec<usize> = (0..items.len()).filter(|&i| items[i].start().is_some()).collect();
    timed.sort_by(|&a, &b| {
        let (ta, tb) = (items[a].start().unwrap(), items[b].start().unwrap());
        ta.total_cmp(&tb).then(items[a].id.cmp(&items[b].id)).then(a.cmp(&b))
    });

    let mut building: Vec<Building> = Vec::new();
    let mut last_touched: Option<usize> = None;
    for &i in &timed {
        let it = &items[i];
        let start = it.start().unwrap();
        let end = start + it.length();
        let is_open = |g: &Building| start - g.end <= GAP_SECONDS;
        let target = match it.position() {
            Some(p) => {
                let nearest = building
                    .iter()
                    .enumerate()
                    .filter(|(_, g)| is_open(g))
                    .filter_map(|(k, g)| g.centre.get().map(|c| (k, distance_m(c, p))))
                    .filter(|&(_, d)| d <= PLACE_METRES)
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(k, _)| k);
                // A group with no position yet takes the clip if it is the current one.
                nearest.or_else(|| {
                    last_touched.filter(|&k| is_open(&building[k]) && building[k].centre.n == 0)
                })
            }
            None => last_touched.filter(|&k| is_open(&building[k])),
        };
        let k = match target {
            Some(k) => k,
            None => {
                building.push(Building { items: Vec::new(), start, end, centre: Centre::default() });
                building.len() - 1
            }
        };
        let g = &mut building[k];
        g.items.push(i);
        g.end = g.end.max(end);
        if let Some(p) = it.position() {
            g.centre.add(p);
        }
        last_touched = Some(k);
    }
    building.sort_by(|a, b| a.start.total_cmp(&b.start));

    // Local date, hour and base name of each timed group.
    struct Named {
        date: String,
        hour_text: String,
        hour: u32,
        centre: Option<(f64, f64)>,
    }
    let named: Vec<Named> = building
        .iter()
        .map(|g| {
            let centre = g.centre.get();
            let offset = g
                .items
                .iter()
                .find_map(|&i| items[i].offset_minutes)
                .or_else(|| centre.map(|c| offset_from_longitude(c.1)))
                .unwrap_or(0);
            Named {
                date: local_date(g.start, offset),
                hour_text: local_clock(g.start, offset),
                hour: local_hour(g.start, offset),
                centre,
            }
        })
        .collect();
    let dates: BTreeSet<&str> = named.iter().map(|n| n.date.as_str()).collect();
    let day_number: HashMap<&str, usize> = dates.iter().enumerate().map(|(k, d)| (*d, k + 1)).collect();
    let mut names: Vec<String> = named
        .iter()
        .map(|n| format!("Day {}, {}", day_number[n.date.as_str()], part_of_day(n.hour)))
        .collect();

    // Place numbers in order of first appearance, reused for a return visit.
    let mut place_number: Vec<Option<usize>> = vec![None; named.len()];
    let mut places = 0;
    for k in 0..named.len() {
        let Some(c) = named[k].centre else { continue };
        let earlier = (0..k)
            .filter_map(|j| Some((place_number[j]?, distance_m(named[j].centre?, c))))
            .filter(|&(_, d)| d <= PLACE_METRES)
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        place_number[k] = Some(match earlier {
            Some((n, _)) => n,
            None => {
                places += 1;
                places
            }
        });
    }
    for k in colliding(&names) {
        if let Some(n) = place_number[k] {
            names[k] = format!("{}, place {}", names[k], n);
        }
    }
    for k in colliding(&names) {
        names[k] = format!("{} at {}", names[k], named[k].hour_text);
    }

    let mut groups: Vec<Group> = building
        .iter()
        .zip(named.iter())
        .zip(names)
        .map(|((g, n), name)| Group {
            name,
            clip_ids: g.items.iter().map(|&i| items[i].id).collect(),
            date: Some(n.date.clone()),
            place: n.centre,
            start_utc: Some(g.start),
            end_utc: Some(g.end),
        })
        .collect();

    // Clips without a time, by folder.
    let mut by_folder: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, it) in items.iter().enumerate() {
        if it.start().is_none() {
            by_folder.entry(it.folder.as_str()).or_default().push(i);
        }
    }
    let mut short_count: HashMap<String, usize> = HashMap::new();
    for folder in by_folder.keys() {
        *short_count.entry(folder_name(folder)).or_default() += 1;
    }
    for (folder, members) in by_folder {
        let short = folder_name(folder);
        let name = if short_count[&short] > 1 && !folder.is_empty() { folder.to_string() } else { short };
        let mut centre = Centre::default();
        for &i in &members {
            if let Some(p) = items[i].position() {
                centre.add(p);
            }
        }
        groups.push(Group {
            name,
            clip_ids: members.iter().map(|&i| items[i].id).collect(),
            date: None,
            place: centre.get(),
            start_utc: None,
            end_utc: None,
        });
    }

    // Last resort so that every name is unique.
    let mut seen: HashMap<String, usize> = HashMap::new();
    for g in &mut groups {
        let n = seen.entry(g.name.clone()).or_insert(0);
        *n += 1;
        if *n > 1 {
            g.name = format!("{} ({})", g.name, n);
        }
    }
    groups
}

/// Indices of names that appear more than once.
fn colliding(names: &[String]) -> Vec<usize> {
    let mut count: HashMap<&str, usize> = HashMap::new();
    for n in names {
        *count.entry(n.as_str()).or_default() += 1;
    }
    (0..names.len()).filter(|&k| count[names[k].as_str()] > 1).collect()
}

/// For clips without a position, borrow one from the nearest-in-time clip that has one, when it is within `max_seconds`. Returns (id, borrowed position, id it came from).
///
/// Time apart is measured between the clips' spans: zero when they overlap,
/// otherwise from the end of one to the start of the other. Ties go to the
/// closer start time, then the lower id. Clips without a time borrow nothing.
/// Results follow the input order.
pub fn borrow_places(items: &[WhenWhere], max_seconds: f64) -> Vec<(u64, (f64, f64), u64)> {
    let donors: Vec<(&WhenWhere, f64, (f64, f64))> = items
        .iter()
        .filter_map(|it| Some((it, it.start()?, it.position()?)))
        .collect();
    let mut out = Vec::new();
    for it in items {
        if it.position().is_some() {
            continue;
        }
        let Some(start) = it.start() else { continue };
        let end = start + it.length();
        let best = donors
            .iter()
            .filter(|(d, _, _)| d.id != it.id)
            .map(|&(d, ds, p)| {
                let de = ds + d.length();
                let apart = (ds - end).max(start - de).max(0.0);
                (apart, (ds - start).abs(), d.id, p)
            })
            .filter(|&(apart, _, _, _)| apart <= max_seconds)
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
        if let Some((_, _, from, p)) = best {
            out.push((it.id, p, from));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solar::days_from_civil;

    const PARIS: (f64, f64) = (48.8566, 2.3522);
    // About 3 km east of the first point.
    const PARIS_EAST: (f64, f64) = (48.8566, 2.3932);
    // About 300 m from PARIS.
    const PARIS_NEAR: (f64, f64) = (48.8593, 2.3522);

    fn t(y: i64, mo: u32, d: u32, h: u32, mi: u32) -> f64 {
        days_from_civil(y, mo, d) as f64 * 86_400.0 + h as f64 * 3600.0 + mi as f64 * 60.0
    }

    fn clip(id: u64, utc: Option<f64>, place: Option<(f64, f64)>, duration: f64) -> WhenWhere {
        WhenWhere { id, utc, offset_minutes: Some(0), place, duration, folder: "/footage/a".into() }
    }

    fn names(g: &[Group]) -> Vec<&str> {
        g.iter().map(|g| g.name.as_str()).collect()
    }

    fn assert_partition(items: &[WhenWhere], groups: &[Group]) {
        let mut ids: Vec<u64> = groups.iter().flat_map(|g| g.clip_ids.iter().copied()).collect();
        ids.sort();
        let mut want: Vec<u64> = items.iter().map(|i| i.id).collect();
        want.sort();
        assert_eq!(ids, want);
    }

    #[test]
    fn distances() {
        assert!(distance_m(PARIS, PARIS) < 1e-6);
        let d = distance_m(PARIS, PARIS_EAST);
        assert!((d - 3000.0).abs() < 30.0, "{d}");
        // Paris to New York is about 5837 km.
        let d = distance_m(PARIS, (40.7128, -74.0060));
        assert!((d - 5_837_000.0).abs() < 10_000.0, "{d}");
        // Across the date line.
        let d = distance_m((0.0, 179.99), (0.0, -179.99));
        assert!((d - 2224.0).abs() < 5.0, "{d}");
    }

    #[test]
    fn splits_at_gaps() {
        let items = vec![
            clip(1, Some(t(2024, 6, 1, 9, 0)), None, 600.0),
            // 40 minutes after the end of the first: same group.
            clip(2, Some(t(2024, 6, 1, 9, 50)), None, 60.0),
            // 46 minutes after the end of the second: new group.
            clip(3, Some(t(2024, 6, 1, 10, 37)), None, 60.0),
        ];
        let g = group_clips(&items);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].clip_ids, vec![1, 2]);
        assert_eq!(g[1].clip_ids, vec![3]);
        assert_eq!(names(&g), vec!["Day 1, morning at 09:00", "Day 1, morning at 10:37"]);
        assert_eq!(g[0].date.as_deref(), Some("2024-06-01"));
        assert_eq!(g[0].start_utc, Some(t(2024, 6, 1, 9, 0)));
        assert_eq!(g[0].end_utc, Some(t(2024, 6, 1, 9, 51)));
        // Exactly 45 minutes still joins.
        let items = vec![
            clip(1, Some(t(2024, 6, 1, 13, 0)), None, 0.0),
            clip(2, Some(t(2024, 6, 1, 13, 45)), None, 0.0),
        ];
        assert_eq!(group_clips(&items).len(), 1);
    }

    #[test]
    fn input_order_does_not_matter() {
        let items = vec![
            clip(3, Some(t(2024, 6, 1, 18, 0)), Some(PARIS), 30.0),
            clip(1, Some(t(2024, 6, 1, 8, 0)), Some(PARIS), 30.0),
            clip(2, Some(t(2024, 6, 1, 8, 5)), Some(PARIS_NEAR), 30.0),
        ];
        let g = group_clips(&items);
        assert_eq!(names(&g), vec!["Day 1, morning", "Day 1, evening"]);
        assert_eq!(g[0].clip_ids, vec![1, 2]);
        let c = g[0].place.unwrap();
        assert!(distance_m(c, PARIS) < 200.0 && distance_m(c, PARIS_NEAR) < 200.0);
        assert_partition(&items, &g);
    }

    #[test]
    fn splits_by_place_at_the_same_time() {
        let start = t(2024, 6, 1, 14, 0);
        let items = vec![
            clip(1, Some(start), Some(PARIS), 300.0),
            clip(2, Some(start), Some(PARIS_EAST), 300.0),
            // Interleaved clips from both cameras join their own place.
            clip(3, Some(start + 400.0), Some(PARIS_NEAR), 300.0),
            clip(4, Some(start + 500.0), Some(PARIS_EAST), 300.0),
            // No position: joins the group added to last.
            clip(5, Some(start + 900.0), None, 60.0),
        ];
        let g = group_clips(&items);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].clip_ids, vec![1, 3]);
        assert_eq!(g[1].clip_ids, vec![2, 4, 5]);
        assert_eq!(names(&g), vec!["Day 1, afternoon, place 1", "Day 1, afternoon, place 2"]);
        assert_partition(&items, &g);
    }

    #[test]
    fn moving_far_starts_a_new_group() {
        let start = t(2024, 6, 1, 10, 0);
        let items = vec![
            clip(1, Some(start), Some(PARIS), 60.0),
            clip(2, Some(start + 120.0), None, 60.0),
            clip(3, Some(start + 240.0), Some(PARIS_EAST), 60.0),
            clip(4, Some(start + 360.0), Some(PARIS_EAST), 60.0),
        ];
        let g = group_clips(&items);
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].clip_ids, vec![1, 2]);
        assert_eq!(g[1].clip_ids, vec![3, 4]);
        assert_eq!(names(&g), vec!["Day 1, morning, place 1", "Day 1, morning, place 2"]);
    }

    #[test]
    fn untimed_clips_group_by_folder() {
        let mut items = vec![
            clip(1, Some(t(2024, 6, 1, 10, 0)), None, 60.0),
            clip(2, None, Some(PARIS), 60.0),
            clip(3, None, None, 60.0),
            clip(4, Some(f64::NAN), None, 60.0),
            clip(5, None, None, 60.0),
            clip(6, None, None, 60.0),
        ];
        items[1].folder = "/footage/b".into();
        items[2].folder = "/footage/b".into();
        items[3].folder = "C:\\cards\\drone".into();
        items[4].folder = "/other/a/".into();
        items[5].folder = "".into();
        let g = group_clips(&items);
        assert_eq!(names(&g), vec!["Day 1, morning", "Clips without a time", "b", "a", "drone"]);
        assert_eq!(g[1].clip_ids, vec![6]);
        assert_eq!(g[2].clip_ids, vec![2, 3]);
        assert_eq!(g[3].clip_ids, vec![5]);
        assert_eq!(g[4].clip_ids, vec![4]);
        assert_eq!(g[4].date, None);
        assert_eq!(g[4].start_utc, None);
        assert!(distance_m(g[2].place.unwrap(), PARIS) < 1.0);
        assert_eq!(g[3].place, None);
        assert_partition(&items, &g);
        // Two folders both called "a" fall back to the full path.
        let mut items = vec![clip(1, None, None, 1.0), clip(2, None, None, 1.0)];
        items[1].folder = "/elsewhere/a".into();
        assert_eq!(names(&group_clips(&items)), vec!["/elsewhere/a", "/footage/a"]);
    }

    #[test]
    fn days_follow_local_time() {
        // New York, UTC-5: 23:00 UTC on 1 June is 18:00 local, 02:00 UTC on
        // 2 June is still 21:00 on 1 June locally, 15:00 UTC on 2 June is
        // 10:00 local on 2 June.
        let ny = (40.7128, -74.0060);
        let mut items = vec![
            clip(1, Some(t(2024, 6, 1, 23, 0)), Some(ny), 60.0),
            clip(2, Some(t(2024, 6, 2, 2, 0)), Some(ny), 60.0),
            clip(3, Some(t(2024, 6, 2, 15, 0)), Some(ny), 60.0),
            // Four days later: still Day 3, the third day with footage.
            clip(4, Some(t(2024, 6, 6, 4, 30)), Some(ny), 60.0),
        ];
        for it in &mut items {
            it.offset_minutes = Some(-300);
        }
        let g = group_clips(&items);
        assert_eq!(names(&g), vec!["Day 1, evening", "Day 1, night", "Day 2, morning", "Day 3, night"]);
        assert_eq!(g[1].date.as_deref(), Some("2024-06-01"));
        assert_eq!(g[3].date.as_deref(), Some("2024-06-05"));
        // Without an offset the longitude gives one (-300 for New York).
        for it in &mut items {
            it.offset_minutes = None;
        }
        assert_eq!(names(&group_clips(&items)), names(&g));
        // Without offset or position, UTC days are used.
        for it in &mut items {
            it.place = None;
        }
        let g = group_clips(&items);
        assert_eq!(names(&g), vec!["Day 1, night", "Day 2, night", "Day 2, afternoon", "Day 3, night"]);
    }

    #[test]
    fn place_numbers_are_reused_and_times_break_ties() {
        let d = |h, m| Some(t(2024, 6, 1, h, m));
        let items = vec![
            clip(1, d(13, 0), Some(PARIS), 60.0),
            clip(2, d(14, 5), Some(PARIS_EAST), 60.0),
            // Back near the first place an hour later: place 1 again.
            clip(3, d(15, 10), Some(PARIS_NEAR), 60.0),
            // Same place, same part of day, after a gap: the time tells them apart.
            clip(4, d(16, 20), Some(PARIS), 60.0),
            // Evening has one group, so no number.
            clip(5, d(18, 0), Some(PARIS_EAST), 60.0),
        ];
        let g = group_clips(&items);
        assert_eq!(
            names(&g),
            vec![
                "Day 1, afternoon, place 1 at 13:00",
                "Day 1, afternoon, place 2",
                "Day 1, afternoon, place 1 at 15:10",
                "Day 1, afternoon, place 1 at 16:20",
                "Day 1, evening",
            ]
        );
        // A group with no place in a crowded part of day gets a time.
        let items = vec![
            clip(1, d(9, 0), Some(PARIS), 60.0),
            clip(2, d(10, 0), None, 60.0),
            clip(3, d(11, 0), Some(PARIS_EAST), 60.0),
        ];
        let g = group_clips(&items);
        assert_eq!(names(&g), vec!["Day 1, morning, place 1", "Day 1, morning", "Day 1, morning, place 2"]);
        // Two groups 2.8 km apart at the same minute, both within 1500 m of
        // an earlier group, share its place number and its clock time.
        let items = vec![
            clip(1, d(7, 0), Some(PARIS), 60.0),
            clip(2, d(9, 0), Some((48.8566, 2.3331)), 60.0),
            clip(3, d(9, 0), Some((48.8566, 2.3713)), 60.0),
        ];
        let g = group_clips(&items);
        assert_eq!(
            names(&g),
            vec![
                "Day 1, morning, place 1 at 07:00",
                "Day 1, morning, place 1 at 09:00",
                "Day 1, morning, place 1 at 09:00 (2)",
            ]
        );
    }

    #[test]
    fn empty_input() {
        assert!(group_clips(&[]).is_empty());
        assert!(borrow_places(&[], 60.0).is_empty());
    }

    #[test]
    fn borrows_nearest_place_in_time() {
        let s = t(2024, 6, 1, 10, 0);
        let items = vec![
            clip(1, Some(s), Some(PARIS), 100.0),
            // Starts 50 s after clip 1 ends.
            clip(2, Some(s + 150.0), None, 30.0),
            clip(3, Some(s + 200.0), Some(PARIS_EAST), 30.0),
            // Overlaps clip 3.
            clip(4, Some(s + 210.0), None, 30.0),
            // Far from everything.
            clip(5, Some(s + 5000.0), None, 30.0),
            // No time.
            clip(6, None, None, 30.0),
            // Already has a place.
            clip(7, Some(s + 150.0), Some(PARIS_NEAR), 1.0),
        ];
        let b = borrow_places(&items, 600.0);
        assert_eq!(b, vec![(2, PARIS_NEAR, 7), (4, PARIS_EAST, 3)]);
        let b = borrow_places(&items, 10.0);
        assert_eq!(b, vec![(2, PARIS_NEAR, 7), (4, PARIS_EAST, 3)]);
        let items2: Vec<WhenWhere> = items.iter().filter(|c| c.id != 7).cloned().collect();
        // Clip 2 is 50 s from clip 1 and 20 s from clip 3.
        assert_eq!(borrow_places(&items2, 600.0)[0], (2, PARIS_EAST, 3));
        assert_eq!(borrow_places(&items2, 10.0), vec![(4, PARIS_EAST, 3)]);
        assert_eq!(borrow_places(&items2, 6000.0).len(), 3);
    }
}
