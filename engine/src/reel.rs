//! Cutting a reel to music.
//!
//! The song's beats and bars come from `beats`. Cuts fall every so many
//! beats, starting on a bar, so the reel is a row of slots between cuts. A
//! first bar that starts less than half a slot into the song does not get a
//! cut of its own (the first shot runs from zero to the next cut), and the
//! end is only cut short of a beat when at least half a slot is left. Each
//! slot gets one select: the best ones are chosen first, then placed in the
//! order they were shot (or best first), avoiding two shots from the same
//! clip in a row and alternating still and moving shots where it can. Inside
//! a select, the slot takes the stretch with the best scores.
//!
//! Nothing here looks at what is in the picture; the order is by time and
//! quality only.

use crate::beats::{cut_points, BeatGrid, MusicInfo};
use crate::project::Project;
use crate::scene::ClipScene;
use crate::selection::{CameraMove, ClipSelection, SegmentStatus};
use serde::{Deserialize, Serialize};

/// How often to cut.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Pace {
    /// Shots of about two seconds, whatever the tempo.
    #[default]
    Auto,
    EveryBeat,
    EveryTwoBeats,
    EveryBar,
    EveryTwoBars,
}

impl Pace {
    /// Beats between cuts at `tempo` beats per minute.
    pub fn beats_per_cut(self, tempo: f64) -> usize {
        match self {
            Pace::EveryBeat => 1,
            Pace::EveryTwoBeats => 2,
            Pace::EveryBar => 4,
            Pace::EveryTwoBars => 8,
            Pace::Auto => {
                let period = if tempo > 1.0 { 60.0 / tempo } else { 0.5 };
                // The power of two of beats closest to two seconds.
                [1usize, 2, 4, 8]
                    .into_iter()
                    .min_by(|a, b| {
                        let da = (*a as f64 * period / AUTO_SHOT_SECONDS).ln().abs();
                        let db = (*b as f64 * period / AUTO_SHOT_SECONDS).ln().abs();
                        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .unwrap_or(4)
            }
        }
    }
}

/// Shot length the automatic pace aims for, in seconds.
pub const AUTO_SHOT_SECONDS: f64 = 2.0;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReelOrder {
    /// In the order the shots were recorded, which usually tells the day as it happened.
    #[default]
    AsShot,
    BestFirst,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReelShape {
    /// The footage's own shape, usually 16:9.
    #[default]
    Landscape,
    /// 9:16 for phones, cut from the centre of the picture.
    Vertical,
    /// 1:1.
    Square,
}

/// The reel the person is putting together.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct ReelSettings {
    pub music: Option<MusicInfo>,
    /// The song's beats, found once when it was chosen.
    pub grid: Option<BeatGrid>,
    pub pace: Pace,
    /// Seconds of reel, None for the whole song.
    pub length: Option<f64>,
    pub order: ReelOrder,
    pub shape: ReelShape,
}

/// One shot in the reel.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ReelShot {
    pub clip_id: u64,
    pub clip_name: String,
    /// Seconds into the clip.
    pub source_start: f64,
    pub source_end: f64,
    /// Seconds into the reel.
    pub at: f64,
    pub length: f64,
    pub score: f32,
    pub camera: CameraMove,
    pub stars: u8,
    pub manual: bool,
    /// The slot was longer than the select, so footage around it was used too.
    pub stretched: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct ReelPlan {
    pub tempo: f64,
    pub beats_per_cut: usize,
    /// Seconds of reel.
    pub length: f64,
    pub shots: Vec<ReelShot>,
    /// Different clips used.
    pub clips_used: usize,
    /// Selects that were available to choose from.
    pub selects_available: usize,
    /// Plain sentences about anything the person should know.
    pub notes: Vec<String>,
    pub beat_confidence: f32,
}

#[derive(Clone, Debug)]
struct Candidate {
    clip: usize,
    seg: usize,
    start: f64,
    end: f64,
    quality: f32,
    moving: bool,
    when: f64,
    stars: u8,
    manual: bool,
}

/// The stretch of `len` seconds inside `start..end` with the best mean
/// score, using the selection's smoothed scores.
fn best_window(sel: &ClipSelection, rank: &dyn Fn(f32, f32, f32) -> f32, start: f64, end: f64, len: f64) -> f64 {
    let sm = &sel.smoothed;
    if end - start <= len + 1e-6 || sm.is_empty() {
        return start;
    }
    let idx: Vec<usize> = (0..sm.len()).filter(|&i| (sm.t[i] as f64) >= start && (sm.t[i] as f64) <= end).collect();
    if idx.len() < 2 {
        return start + (end - start - len) / 2.0;
    }
    let mut best = (f32::NEG_INFINITY, start);
    for &i in &idx {
        let a = (sm.t[i] as f64).min(end - len).max(start);
        let inside: Vec<usize> = idx.iter().copied().filter(|&j| (sm.t[j] as f64) >= a && (sm.t[j] as f64) <= a + len).collect();
        if inside.is_empty() {
            continue;
        }
        let v = inside.iter().map(|&j| rank(sm.focus[j], sm.exposure[j], sm.stability[j])).sum::<f32>() / inside.len() as f32;
        if v > best.0 + 1e-6 {
            best = (v, a);
        }
    }
    best.1
}

/// Plan a reel from the current selects.
pub fn plan(project: &Project, selections: &[ClipSelection], scenes: &[ClipScene], settings: &ReelSettings) -> Result<ReelPlan, String> {
    let music = settings.music.as_ref().ok_or("Choose a song first.")?;
    let grid = settings.grid.as_ref().ok_or("The song's beats have not been found yet. Choose the song again.")?;
    if grid.beats.len() < 4 {
        return Err("No steady beat was found in this song, so there is nothing to cut to. Try a song with a clear rhythm.".into());
    }
    let every = settings.pace.beats_per_cut(grid.tempo);
    let max = settings.length.unwrap_or(music.duration).min(music.duration).max(1.0);
    let mut cuts = cut_points(grid, every, max);
    let slot = every as f64 * 60.0 / grid.tempo.max(1.0);
    match cuts.first().copied() {
        // A first bar starting just after zero would leave a sliver of a
        // shot before it, so the first shot runs from zero to the next cut
        // instead. The music still starts at zero, so every later cut stays
        // on its beat.
        Some(c) if c > 0.05 && c < 0.5 * slot && cuts.len() >= 2 => cuts[0] = 0.0,
        Some(c) if c <= 0.05 => {}
        _ => cuts.insert(0, 0.0),
    }
    // End on the last cut at or before the length asked for, or on the
    // length itself when no cut comes close.
    if let Some(&last) = cuts.last() {
        if max - last > 0.5 * slot && max - last > 0.4 {
            cuts.push(max);
        }
    }
    cuts.dedup_by(|a, b| (*a - *b).abs() < 0.05);
    if cuts.len() < 2 {
        return Err("The song is too short to cut to at this pace. Pick a faster pace or a longer length.".into());
    }
    let slots: Vec<(f64, f64)> = cuts.windows(2).map(|w| (w[0], w[1])).collect();

    let s = &project.selection;
    let mut cands: Vec<Candidate> = Vec::new();
    for (ci, (c, sel)) in project.clips.iter().zip(selections).enumerate() {
        let when = scenes.get(ci).and_then(|sc| sc.utc.or(sc.wall)).unwrap_or(ci as f64 * 1e-3);
        for (si, g) in sel.segments.iter().enumerate() {
            if g.status != SegmentStatus::Kept || c.info.is_none() {
                continue;
            }
            cands.push(Candidate {
                clip: ci,
                seg: si,
                start: g.start,
                end: g.end,
                quality: g.score + 0.04 * g.stars as f32 + if g.manual { 0.08 } else { 0.0 },
                moving: g.camera_move,
                when: when + g.start,
                stars: g.stars,
                manual: g.manual,
            });
        }
    }
    if cands.is_empty() {
        return Err("There are no selects to build a reel from yet. Analyze footage or lower a threshold first.".into());
    }
    let available = cands.len();
    // The best selects, a few more than there are slots, so the order can still choose.
    cands.sort_by(|a, b| b.quality.partial_cmp(&a.quality).unwrap_or(std::cmp::Ordering::Equal));
    cands.truncate((slots.len() * 3 / 2 + 4).max(slots.len()));
    if settings.order == ReelOrder::AsShot {
        cands.sort_by(|a, b| a.when.partial_cmp(&b.when).unwrap_or(std::cmp::Ordering::Equal));
    }

    let rank = |f: f32, e: f32, st: f32| s.rank_score(f, e, st);
    let mut used = vec![false; cands.len()];
    let mut shots: Vec<ReelShot> = Vec::new();
    let mut last: Option<usize> = None;
    let mut reused = 0usize;
    let mut stretched = 0usize;
    for &(a, b) in &slots {
        let len = b - a;
        let prev = last.map(|k| cands[k].clone());
        let value = |k: usize| -> f32 {
            let c = &cands[k];
            let mut v = c.quality;
            if let Some(p) = &prev {
                if p.clip == c.clip {
                    v -= 0.35;
                }
                if p.moving == c.moving {
                    v -= 0.06;
                }
            }
            if c.end - c.start + 1e-6 < len {
                v -= 0.25;
            }
            v
        };
        let pick = match settings.order {
            ReelOrder::AsShot => {
                // The best of the next few unused selects in recording order.
                let start = last.map(|k| k + 1).unwrap_or(0);
                let ahead: Vec<usize> = (start..cands.len()).filter(|&k| !used[k]).take(4).collect();
                let pool = if ahead.is_empty() { (0..cands.len()).filter(|&k| !used[k]).collect::<Vec<_>>() } else { ahead };
                pool.into_iter().max_by(|&x, &y| value(x).partial_cmp(&value(y)).unwrap_or(std::cmp::Ordering::Equal))
            }
            ReelOrder::BestFirst => (0..cands.len()).filter(|&k| !used[k]).max_by(|&x, &y| value(x).partial_cmp(&value(y)).unwrap_or(std::cmp::Ordering::Equal)),
        };
        let k = match pick {
            Some(k) => k,
            None => {
                // Every select is used: start again from the best.
                used.iter_mut().for_each(|u| *u = false);
                reused += 1;
                (0..cands.len()).max_by(|&x, &y| value(x).partial_cmp(&value(y)).unwrap_or(std::cmp::Ordering::Equal)).unwrap()
            }
        };
        used[k] = true;
        last = Some(k);
        let c = &cands[k];
        let clip = &project.clips[c.clip];
        let duration = clip.info.as_ref().map(|i| i.duration).unwrap_or(c.end);
        let (mut from, mut to) = (c.start, c.end);
        let mut was_stretched = false;
        if to - from + 1e-6 < len {
            // Use the footage around a short select to fill the slot.
            let need = len - (to - from);
            from = (from - need / 2.0).max(0.0);
            to = (from + len).min(duration);
            from = (to - len).max(0.0);
            was_stretched = true;
            stretched += 1;
        }
        let at = best_window(&selections[c.clip], &rank, from, to, len.min(to - from));
        let g = &selections[c.clip].segments[c.seg];
        shots.push(ReelShot {
            clip_id: clip.id,
            clip_name: clip.info.as_ref().map(|i| i.name.clone()).unwrap_or_default(),
            source_start: at,
            source_end: (at + len).min(duration),
            at: a,
            length: len,
            score: g.score,
            camera: g.camera,
            stars: c.stars,
            manual: c.manual,
            stretched: was_stretched,
        });
    }
    let mut clips: Vec<u64> = shots.iter().map(|s| s.clip_id).collect();
    clips.sort();
    clips.dedup();
    let mut notes = Vec::new();
    if grid.confidence < 0.35 {
        notes.push("The beat in this song is not very steady, so some cuts may land slightly off the music.".into());
    }
    if reused > 0 {
        notes.push("There were fewer selects than shots, so some selects appear twice. Lower a threshold, add footage or pick a slower pace for a reel without repeats.".into());
    }
    if stretched > 0 {
        notes.push(format!(
            "{stretched} {} shorter than {} slot, so footage just around {} was used too.",
            if stretched == 1 { "select was" } else { "selects were" },
            if stretched == 1 { "its" } else { "their" },
            if stretched == 1 { "it" } else { "them" }
        ));
    }
    Ok(ReelPlan {
        tempo: (grid.tempo * 10.0).round() / 10.0,
        beats_per_cut: every,
        length: slots.last().map(|s| s.1).unwrap_or(0.0),
        clips_used: clips.len(),
        selects_available: available,
        shots,
        notes,
        beat_confidence: grid.confidence,
    })
}
