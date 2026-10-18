//! Tracing a finished video's shots back to the source clips they came from.
//!
//! 1. The finished video is split into shots wherever its fingerprint jumps.
//! 2. Every usable sample of a shot is compared with every source frame,
//!    using the closest of the source's three crops (whole picture, 2.39:1
//!    band, 9:16 column). Close source frames become candidate points
//!    (finished time, source time).
//! 3. Per clip, a straight line source = a + b * finished is fitted to those
//!    points by trying lines through pairs of them and counting how many
//!    points agree. The slope is the playback speed: 0.4 for 60p footage
//!    played in a 24p timeline, 1 for normal speed.
//! 4. The clip whose line most samples agree with wins the shot. Shots with
//!    too little agreement (title cards, stock footage, clips that are not in
//!    the project) stay unmatched rather than being matched wrongly.
//!
//! Comparison is a popcount over 64-bit hashes. Two hours of footage at four
//! samples per second against a five minute video is about half a billion
//! comparisons, a fraction of a second to a few seconds on one core.

use crate::fingerprint::{distance, HASH_FULL, HASH_TALL, HASH_WIDE};
use crate::reference::ReferencePrint;
use serde::{Deserialize, Serialize};

/// A source frame within this many differing bits (of 64) is a candidate.
pub const MATCH_DISTANCE: u32 = 12;
/// Consecutive finished-video samples further apart than this start a new shot.
pub const CUT_DISTANCE: u32 = 20;
/// Slowest and fastest playback speeds considered (source seconds per finished second).
pub const MIN_SPEED: f64 = 0.15;
pub const MAX_SPEED: f64 = 2.5;
/// Candidates kept per finished sample per clip.
const PER_SAMPLE: usize = 4;

/// A borrowed view of one analysed source clip's fingerprints.
#[derive(Clone, Copy, Debug)]
pub struct ClipPrints<'a> {
    pub clip_id: u64,
    /// Sample times in seconds from the clip start.
    pub t: &'a [f32],
    /// Fingerprints per sample: whole, 2.39:1 band, 9:16 column.
    pub fp: &'a [[u64; 3]],
    /// Clip length in seconds, used to keep matched ranges inside the clip.
    pub duration: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SourceRange {
    pub clip_id: u64,
    /// Seconds from the clip start.
    pub start: f64,
    pub end: f64,
    /// Source seconds per finished second: 1 is normal speed, 0.4 is 60p slowed to 24p.
    pub speed: f64,
    /// 0 to 1.
    pub confidence: f32,
    /// "full", "2.39:1" or "9:16": which part of the source picture the shot shows.
    pub crop: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MatchedShot {
    /// Seconds from the start of the finished video.
    pub ref_start: f64,
    pub ref_end: f64,
    pub source: Option<SourceRange>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct MatchReport {
    pub shots: Vec<MatchedShot>,
}

impl MatchReport {
    pub fn matched(&self) -> impl Iterator<Item = &SourceRange> {
        self.shots.iter().filter_map(|s| s.source.as_ref())
    }
}

const CROP_NAMES: [&str; 3] = ["full", "2.39:1", "9:16"];

#[derive(Clone, Copy, Debug)]
struct Point {
    /// Index of the finished-video sample.
    ri: usize,
    rt: f64,
    st: f64,
    d: u32,
    crop: usize,
}

/// Shots as ranges of sample indices [first, last], usable samples only at the ends.
fn split_shots(r: &ReferencePrint) -> Vec<(usize, usize)> {
    let n = r.t.len();
    let mut shots = Vec::new();
    let mut i = 0;
    while i < n {
        if !r.usable[i] {
            i += 1;
            continue;
        }
        let first = i;
        let mut last = i;
        while last + 1 < n && r.usable[last + 1] && distance(r.hash[last], r.hash[last + 1]) <= CUT_DISTANCE {
            last += 1;
        }
        shots.push((first, last));
        i = last + 1;
    }
    shots
}

/// Candidate points for one finished sample against one clip: the closest few frames.
fn candidates(hash: u64, clip: &ClipPrints, ri: usize, rt: f64, out: &mut Vec<Point>) {
    let mut best: [(u32, usize, usize); PER_SAMPLE] = [(u32::MAX, 0, 0); PER_SAMPLE];
    for (j, fp) in clip.fp.iter().enumerate() {
        let mut d = u32::MAX;
        let mut crop = 0;
        for (k, &v) in [HASH_FULL, HASH_WIDE, HASH_TALL].iter().enumerate() {
            let dk = distance(hash, fp[v]);
            if dk < d {
                d = dk;
                crop = k;
            }
        }
        if d <= MATCH_DISTANCE && d < best[PER_SAMPLE - 1].0 {
            let mut k = PER_SAMPLE - 1;
            best[k] = (d, j, crop);
            while k > 0 && best[k].0 < best[k - 1].0 {
                best.swap(k, k - 1);
                k -= 1;
            }
        }
    }
    for &(d, j, crop) in best.iter() {
        if d == u32::MAX {
            break;
        }
        out.push(Point { ri, rt, st: clip.t[j] as f64, d, crop });
    }
}

#[derive(Clone, Copy, Debug)]
struct Fit {
    a: f64,
    b: f64,
    /// Distinct finished samples agreeing with the line.
    support: usize,
    /// Support weighted by how close each agreeing frame is: an exact match counts 1.
    score: f64,
    mean_d: f64,
    crop: usize,
}

/// How far a point may sit from the line, in source seconds.
fn tolerance(b: f64, source_step: f64) -> f64 {
    (0.6 * source_step).max(0.12) + 0.08 * b.max(1.0)
}

fn evaluate(points: &[Point], a: f64, b: f64, step: f64) -> Fit {
    let tol = tolerance(b, step);
    // Best agreeing point per finished sample.
    let mut per: std::collections::BTreeMap<usize, (u32, usize)> = Default::default();
    for p in points {
        if (p.st - (a + b * p.rt)).abs() <= tol {
            let e = per.entry(p.ri).or_insert((p.d, p.crop));
            if p.d < e.0 {
                *e = (p.d, p.crop);
            }
        }
    }
    let support = per.len();
    let mean_d = if support > 0 { per.values().map(|v| v.0 as f64).sum::<f64>() / support as f64 } else { 64.0 };
    let score = per
        .values()
        .map(|v| (MATCH_DISTANCE + 1 - v.0.min(MATCH_DISTANCE)) as f64 / (MATCH_DISTANCE + 1) as f64)
        .sum::<f64>();
    let mut votes = [0usize; 3];
    for v in per.values() {
        votes[v.1] += 1;
    }
    let crop = (0..3).max_by_key(|&k| (votes[k], 3 - k)).unwrap_or(0);
    Fit { a, b, support, score, mean_d, crop }
}

/// Refine a line by least squares over the points that agree with it, keeping one point per sample.
fn refine(points: &[Point], f: Fit, step: f64) -> Fit {
    let tol = tolerance(f.b, step);
    let mut per: std::collections::BTreeMap<usize, Point> = Default::default();
    for p in points {
        let r = (p.st - (f.a + f.b * p.rt)).abs();
        if r <= tol {
            let e = per.entry(p.ri).or_insert(*p);
            let closer = (p.st - (f.a + f.b * p.rt)).abs() < (e.st - (f.a + f.b * e.rt)).abs();
            if p.d < e.d || (p.d == e.d && closer) {
                *e = *p;
            }
        }
    }
    if per.len() < 3 {
        return f;
    }
    let n = per.len() as f64;
    let (mut sx, mut sy, mut sxx, mut sxy) = (0.0, 0.0, 0.0, 0.0);
    for p in per.values() {
        sx += p.rt;
        sy += p.st;
        sxx += p.rt * p.rt;
        sxy += p.rt * p.st;
    }
    let den = n * sxx - sx * sx;
    if den.abs() < 1e-9 {
        return f;
    }
    let b = (n * sxy - sx * sy) / den;
    if !(MIN_SPEED..=MAX_SPEED).contains(&b) {
        return f;
    }
    let a = (sy - b * sx) / n;
    let g = evaluate(points, a, b, step);
    if g.score >= f.score - 1e-9 {
        g
    } else {
        f
    }
}

fn better(x: &Fit, y: &Fit) -> bool {
    // More close agreement wins. When two lines agree about equally (a still
    // shot fits any slope), the speed nearer normal wins.
    let tie = 0.02 * x.score.max(y.score).max(1.0);
    if (x.score - y.score).abs() > tie {
        return x.score > y.score;
    }
    x.b.ln().abs() < y.b.ln().abs()
}

/// Best line through one clip's points for one shot.
fn fit_clip(points: &[Point], step: f64) -> Option<Fit> {
    if points.is_empty() {
        return None;
    }
    let mut best: Option<Fit> = None;
    let consider = |f: Fit, best: &mut Option<Fit>| {
        if best.as_ref().map(|b| better(&f, b)).unwrap_or(true) {
            *best = Some(f);
        }
    };
    // Normal-speed lines through each point: covers short shots and still scenes.
    for p in points {
        consider(evaluate(points, p.st - p.rt, 1.0, step), &mut best);
    }
    // Lines through pairs of points from different samples.
    let n = points.len();
    let stride = (n * n / 6000).max(1);
    let mut k = 0usize;
    for i in 0..n {
        for j in i + 1..n {
            k += 1;
            if k % stride != 0 {
                continue;
            }
            let (p, q) = (points[i], points[j]);
            let dr = q.rt - p.rt;
            if dr.abs() < 0.3 {
                continue;
            }
            let b = (q.st - p.st) / dr;
            if !(MIN_SPEED..=MAX_SPEED).contains(&b) {
                continue;
            }
            consider(evaluate(points, p.st - b * p.rt, b, step), &mut best);
        }
    }
    best.map(|f| refine(points, f, step))
}

/// Median spacing between source samples, in seconds.
fn source_step(clip: &ClipPrints) -> f64 {
    if clip.t.len() < 2 {
        return 0.25;
    }
    let mut d: Vec<f32> = clip.t.windows(2).map(|w| w[1] - w[0]).collect();
    d.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (d[d.len() / 2] as f64).max(0.01)
}

/// Find where each shot of a finished video came from.
pub fn match_reference(reference: &ReferencePrint, clips: &[ClipPrints]) -> MatchReport {
    let n = reference.t.len();
    let step_r = 1.0 / reference.sample_rate.max(0.1);
    let shots = split_shots(reference);
    let steps: Vec<f64> = clips.iter().map(source_step).collect();
    let mut out: Vec<MatchedShot> = Vec::new();
    // The clip and line each matched shot came from, for the short-piece pass below.
    let mut lines: Vec<Option<(usize, Fit)>> = Vec::new();
    let mut spans: Vec<(usize, usize, f64, f64)> = Vec::new();
    for &(first, last) in &shots {
        // The cut lies halfway between samples.
        let rs = if first == 0 { 0.0 } else { 0.5 * (reference.t[first - 1] + reference.t[first]) as f64 };
        let re = if last + 1 >= n {
            (reference.t[last] as f64 + step_r).min(reference.duration.max(reference.t[last] as f64))
        } else {
            0.5 * (reference.t[last] + reference.t[last + 1]) as f64
        };
        let samples = last - first + 1;
        let mut ranked: Vec<(usize, Fit)> = Vec::new();
        let mut points = Vec::new();
        for (ci, clip) in clips.iter().enumerate() {
            points.clear();
            for ri in first..=last {
                candidates(reference.hash[ri], clip, ri, reference.t[ri] as f64, &mut points);
            }
            if let Some(f) = fit_clip(&points, steps[ci]) {
                ranked.push((ci, f));
            }
        }
        ranked.sort_by(|x, y| if better(&x.1, &y.1) { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater });
        let mut chosen: Option<(usize, Fit)> = None;
        let source = ranked.first().and_then(|&(ci, f)| {
            let need = ((samples as f64 * 0.5).ceil() as usize).clamp(2, 6).max(if samples >= 3 { 3 } else { 2 });
            let frac = f.support as f64 / samples as f64;
            if f.support < need || frac < 0.4 || f.mean_d > 10.0 {
                return None;
            }
            // A runner-up nearly as good means the shot could be from either clip.
            let ambiguity = ranked
                .get(1)
                .map(|r| r.1.support as f64 / f.support as f64)
                .unwrap_or(0.0);
            let clip = &clips[ci];
            let s0 = (f.a + f.b * rs).max(0.0);
            let s1 = (f.a + f.b * re).min(clip.duration.max(s0));
            if s1 - s0 < 0.05 {
                return None;
            }
            let conf = frac.min(1.0) * (1.0 - f.mean_d / 16.0).clamp(0.0, 1.0) * (1.0 - 0.5 * ambiguity.min(1.0));
            chosen = Some((ci, f));
            Some(SourceRange {
                clip_id: clip.clip_id,
                start: s0,
                end: s1,
                speed: f.b,
                confidence: conf.clamp(0.0, 1.0) as f32,
                crop: CROP_NAMES[f.crop].to_string(),
            })
        });
        out.push(MatchedShot { ref_start: rs, ref_end: re, source });
        lines.push(chosen);
        spans.push((first, last, rs, re));
    }
    rescue_short_pieces(reference, clips, &steps, &mut out, &mut lines, &spans);
    MatchReport { shots: merge_continuations(out) }
}

/// Shots of this many samples or fewer may be the tail or head of a
/// neighbouring shot, cut off by a picture change just over `CUT_DISTANCE`.
const SHORT_PIECE: usize = 3;

/// A short piece next to a matched shot whose samples carry on that shot's
/// source line is part of that shot, not a cut: a moving picture can change
/// by just over the cut threshold between two samples, and rounding in
/// different decoders decides which side of it a frame falls. On its own
/// such a piece has too few samples to be traced reliably, so it is given
/// the neighbour's line, and `merge_continuations` joins the two. A real cut
/// to other footage does not fit the neighbour's line and is left alone.
fn rescue_short_pieces(
    reference: &ReferencePrint,
    clips: &[ClipPrints],
    steps: &[f64],
    out: &mut [MatchedShot],
    lines: &mut [Option<(usize, Fit)>],
    spans: &[(usize, usize, f64, f64)],
) {
    let mut points = Vec::new();
    for k in 0..out.len() {
        let (first, last, rs, re) = spans[k];
        let samples = last - first + 1;
        if samples > SHORT_PIECE {
            continue;
        }
        let neighbours = [k.checked_sub(1), (k + 1 < out.len()).then_some(k + 1)];
        for j in neighbours.into_iter().flatten() {
            let Some((ci, f)) = lines[j] else { continue };
            if (spans[j].3 - rs).abs() > 0.2 && (re - spans[j].2).abs() > 0.2 {
                continue;
            }
            let clip = &clips[ci];
            points.clear();
            for ri in first..=last {
                candidates(reference.hash[ri], clip, ri, reference.t[ri] as f64, &mut points);
            }
            let g = evaluate(&points, f.a, f.b, steps[ci]);
            if g.support * 2 < samples || g.support == 0 || g.mean_d > MATCH_DISTANCE as f64 {
                continue;
            }
            let s0 = (f.a + f.b * rs).max(0.0);
            let s1 = (f.a + f.b * re).min(clip.duration.max(s0));
            if s1 - s0 < 0.02 {
                continue;
            }
            let conf = out[j].source.as_ref().map(|s| s.confidence).unwrap_or(0.5);
            out[k].source = Some(SourceRange {
                clip_id: clip.clip_id,
                start: s0,
                end: s1,
                speed: f.b,
                confidence: conf,
                crop: CROP_NAMES[f.crop].to_string(),
            });
            lines[k] = Some((ci, f));
            break;
        }
    }
}

/// Join neighbouring shots that continue the same source line: a cut detected
/// inside one take by a fast camera move, for example.
fn merge_continuations(shots: Vec<MatchedShot>) -> Vec<MatchedShot> {
    let mut out: Vec<MatchedShot> = Vec::new();
    for s in shots {
        if let (Some(prev), Some(cur)) = (out.last_mut(), s.source.as_ref()) {
            if let Some(p) = prev.source.as_mut() {
                let gap = s.ref_start - prev.ref_end;
                let speed_close = (p.speed - cur.speed).abs() / p.speed.max(cur.speed) < 0.1;
                if p.clip_id == cur.clip_id && gap.abs() < 0.2 && speed_close && (cur.start - p.end).abs() < 0.5 {
                    p.end = cur.end.max(p.end);
                    p.confidence = p.confidence.max(cur.confidence);
                    prev.ref_end = s.ref_end;
                    continue;
                }
            }
        }
        out.push(s);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sequence of pseudo-random hashes where nearby times differ by a few bits.
    fn walk(seed: u64, n: usize) -> Vec<u64> {
        let mut s = seed;
        let mut h = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                h ^= 1 << (s >> 58);
                h
            })
            .collect()
    }

    fn reference(hash: Vec<u64>, rate: f64) -> ReferencePrint {
        let n = hash.len();
        ReferencePrint {
            path: String::new(),
            name: String::new(),
            duration: n as f64 / rate,
            t: (0..n).map(|i| (i as f64 / rate) as f32).collect(),
            usable: vec![true; n],
            hash,
            sample_rate: rate,
        }
    }

    #[test]
    fn a_slowed_shot_is_found_with_its_speed() {
        // Source at 4 samples a second for 30 s.
        let src = walk(7, 120);
        let t: Vec<f32> = (0..120).map(|i| i as f32 / 4.0).collect();
        let fp: Vec<[u64; 3]> = src.iter().map(|&h| [h, !h, !h]).collect();
        let clip = ClipPrints { clip_id: 9, t: &t, fp: &fp, duration: 30.0 };
        // Finished video at 6 per second showing source 10 s to 12 s at 0.4x (5 s long).
        let hash: Vec<u64> = (0..30)
            .map(|i| {
                let st = 10.0 + 0.4 * (i as f64 / 6.0);
                src[(st * 4.0).round() as usize]
            })
            .collect();
        let r = match_reference(&reference(hash, 6.0), &[clip]);
        assert_eq!(r.shots.len(), 1);
        let s = r.shots[0].source.as_ref().expect("matched");
        assert_eq!(s.clip_id, 9);
        assert!((s.speed - 0.4).abs() < 0.04, "speed {}", s.speed);
        assert!((s.start - 10.0).abs() < 0.3, "start {}", s.start);
        assert!((s.end - 12.0).abs() < 0.3, "end {}", s.end);
    }

    #[test]
    fn a_short_tail_cut_off_a_shot_by_a_picture_change_stays_with_it() {
        // Source at 4 samples a second for 30 s.
        let src = walk(11, 120);
        let t: Vec<f32> = (0..120).map(|i| i as f32 / 4.0).collect();
        let fp: Vec<[u64; 3]> = src.iter().map(|&h| [h, !h, !h]).collect();
        let clip = ClipPrints { clip_id: 1, t: &t, fp: &fp, duration: 30.0 };
        // Finished video at 6 per second: source 5 s to 8 s at normal speed.
        // The last two samples change by just over the cut threshold, as a
        // moving picture can, but still match the source within tolerance.
        let tail_mask: u64 = 0x0000_0000_0000_07FF; // 11 bits
        let before_mask: u64 = 0x07FF_0000_0000_0000; // 11 other bits
        let mut hash: Vec<u64> = (0..18).map(|i| src[((5.0 + i as f64 / 6.0) * 4.0).round() as usize]).collect();
        hash[15] ^= before_mask;
        hash[16] ^= tail_mask;
        hash[17] ^= tail_mask;
        assert!(distance(hash[15], hash[16]) > CUT_DISTANCE, "the test needs a split");
        // A decoy clip holds the changed tail exactly, so on its own the tail would trace there.
        let decoy_src: Vec<u64> = (0..40).map(|i| if i == 20 { hash[16] } else if i == 21 { hash[17] } else { walk(77, 40)[i] }).collect();
        let dt: Vec<f32> = (0..40).map(|i| i as f32 / 4.0).collect();
        let dfp: Vec<[u64; 3]> = decoy_src.iter().map(|&h| [h, !h, !h]).collect();
        let decoy = ClipPrints { clip_id: 2, t: &dt, fp: &dfp, duration: 10.0 };
        let r = match_reference(&reference(hash, 6.0), &[clip, decoy]);
        let matched: Vec<_> = r.matched().collect();
        assert_eq!(matched.len(), 1, "{:#?}", r.shots);
        let s = matched[0];
        assert_eq!(s.clip_id, 1);
        assert!((s.start - 5.0).abs() < 0.3, "start {}", s.start);
        assert!((s.end - 8.0).abs() < 0.3, "end {}", s.end);
    }

    #[test]
    fn a_short_cut_to_other_footage_is_not_absorbed() {
        let src = walk(11, 120);
        let t: Vec<f32> = (0..120).map(|i| i as f32 / 4.0).collect();
        let fp: Vec<[u64; 3]> = src.iter().map(|&h| [h, !h, !h]).collect();
        let clip = ClipPrints { clip_id: 1, t: &t, fp: &fp, duration: 30.0 };
        // 3 s of the source, then a half-second insert of something else entirely.
        let mut hash: Vec<u64> = (0..18).map(|i| src[((5.0 + i as f64 / 6.0) * 4.0).round() as usize]).collect();
        hash.extend(walk(555, 3));
        let r = match_reference(&reference(hash, 6.0), &[clip]);
        let last = r.shots.last().unwrap();
        assert!(last.ref_start > 2.8, "{:#?}", r.shots);
        assert!(last.source.is_none(), "{:#?}", r.shots);
    }

    #[test]
    fn unrelated_pictures_stay_unmatched() {
        let src = walk(7, 120);
        let t: Vec<f32> = (0..120).map(|i| i as f32 / 4.0).collect();
        let fp: Vec<[u64; 3]> = src.iter().map(|&h| [h, h, h]).collect();
        let clip = ClipPrints { clip_id: 1, t: &t, fp: &fp, duration: 30.0 };
        let r = match_reference(&reference(walk(99, 30), 6.0), &[clip]);
        assert!(r.shots.iter().all(|s| s.source.is_none()));
    }
}
