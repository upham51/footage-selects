//! Per-frame measurements on a small grayscale picture.
//!
//! Everything here is plain arithmetic on 8-bit luma:
//! 1. Focus: variance of the 4-neighbour Laplacian.
//! 2. Exposure: a 256-bin histogram, from which we keep the share of clipped
//!    highlights, the share of crushed shadows, the 2nd and 98th percentiles
//!    (contrast spread) and the mean.
//! 3. Motion: global translation between consecutive samples, estimated by
//!    coarse-to-fine block matching on an image pyramid.

use serde::{Deserialize, Serialize};

/// Pixel values at or below this count as crushed shadows.
pub const SHADOW_LEVEL: u8 = 5;
/// Pixel values at or above this count as clipped highlights.
pub const HIGHLIGHT_LEVEL: u8 = 250;

/// Raw measurements for one sampled frame. Scores are derived from these later,
/// so changing scoring never requires decoding the footage again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FrameMetrics {
    /// Variance of the Laplacian.
    pub lap: f32,
    /// Mean luma, 0 to 255.
    pub mean: f32,
    /// Share of pixels at or below `SHADOW_LEVEL`, 0 to 1.
    pub lo: f32,
    /// Share of pixels at or above `HIGHLIGHT_LEVEL`, 0 to 1.
    pub hi: f32,
    /// 2nd percentile of luma.
    pub p2: f32,
    /// 98th percentile of luma.
    pub p98: f32,
    /// Horizontal shift from the previous sample, in analysis pixels.
    pub dx: f32,
    /// Vertical shift from the previous sample, in analysis pixels.
    pub dy: f32,
    /// False when the picture had too little texture to measure motion.
    pub motion_ok: bool,
}

/// Variance of the 4-neighbour Laplacian over the interior of the picture.
pub fn laplacian_variance(gray: &[u8], w: usize, h: usize) -> f64 {
    if w < 3 || h < 3 {
        return 0.0;
    }
    let mut sum: i64 = 0;
    let mut sum_sq: i64 = 0;
    for y in 1..h - 1 {
        let up = &gray[(y - 1) * w..y * w];
        let mid = &gray[y * w..(y + 1) * w];
        let down = &gray[(y + 1) * w..(y + 2) * w];
        let mut row_sum: i64 = 0;
        let mut row_sq: i64 = 0;
        for x in 1..w - 1 {
            let l = 4 * mid[x] as i32
                - mid[x - 1] as i32
                - mid[x + 1] as i32
                - up[x] as i32
                - down[x] as i32;
            row_sum += l as i64;
            row_sq += (l * l) as i64;
        }
        sum += row_sum;
        sum_sq += row_sq;
    }
    let n = ((w - 2) * (h - 2)) as f64;
    let mean = sum as f64 / n;
    (sum_sq as f64 / n - mean * mean).max(0.0)
}

/// Histogram-based exposure measurements: (mean, lo share, hi share, p2, p98).
pub fn exposure_stats(gray: &[u8]) -> (f32, f32, f32, f32, f32) {
    let mut hist = [0u32; 256];
    for &v in gray {
        hist[v as usize] += 1;
    }
    let n = gray.len().max(1) as f64;
    let mut total: u64 = 0;
    for (i, &c) in hist.iter().enumerate() {
        total += i as u64 * c as u64;
    }
    let lo: u32 = hist[..=SHADOW_LEVEL as usize].iter().sum();
    let hi: u32 = hist[HIGHLIGHT_LEVEL as usize..].iter().sum();
    let pct = |p: f64| -> f32 {
        let target = p * n;
        let mut acc = 0f64;
        for (i, &c) in hist.iter().enumerate() {
            acc += c as f64;
            if acc >= target {
                return i as f32;
            }
        }
        255.0
    };
    (
        (total as f64 / n) as f32,
        (lo as f64 / n) as f32,
        (hi as f64 / n) as f32,
        pct(0.02),
        pct(0.98),
    )
}

/// A grayscale picture.
#[derive(Clone, Debug, Default)]
pub struct Plane {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Plane {
    /// Half size by averaging 2x2 blocks.
    pub fn half(&self) -> Plane {
        let (w, h) = (self.w / 2, self.h / 2);
        let mut px = vec![0u8; w * h];
        for y in 0..h {
            let r0 = &self.px[(2 * y) * self.w..(2 * y + 1) * self.w];
            let r1 = &self.px[(2 * y + 1) * self.w..(2 * y + 2) * self.w];
            for x in 0..w {
                let s = r0[2 * x] as u32 + r0[2 * x + 1] as u32 + r1[2 * x] as u32 + r1[2 * x + 1] as u32;
                px[y * w + x] = ((s + 2) / 4) as u8;
            }
        }
        Plane { w, h, px }
    }
}

/// An image pyramid of one frame, kept to compare with the next sample.
#[derive(Clone, Debug, Default)]
pub struct MotionFrame {
    /// Level 0 is the analysis picture; each next level is half the size.
    pub levels: Vec<Plane>,
}

/// Coarsest level used for the search (1/8 of 480 is 60 pixels wide).
const PYRAMID_LEVELS: usize = 4;
/// Search radius at the coarsest level, in that level's pixels.
const COARSE_RADIUS: i32 = 10;

pub fn motion_frame(gray: &[u8], w: usize, h: usize) -> MotionFrame {
    let mut levels = vec![Plane {
        w,
        h,
        px: gray.to_vec(),
    }];
    while levels.len() < PYRAMID_LEVELS {
        let last = levels.last().unwrap();
        if last.w < 32 || last.h < 18 {
            break;
        }
        let next = last.half();
        levels.push(next);
    }
    MotionFrame { levels }
}

/// Mean absolute difference between `cur` and `prev` displaced by (sx, sy),
/// over the overlapping area, visiting every `step`th pixel. Content that
/// moved right by sx pixels gives the minimum at +sx.
pub fn shifted_difference(prev: &Plane, cur: &Plane, sx: i32, sy: i32, step: usize) -> f32 {
    let (w, h) = (cur.w as i32, cur.h as i32);
    if prev.w != cur.w || prev.h != cur.h || sx.abs() >= w / 2 || sy.abs() >= h / 2 {
        return f32::MAX;
    }
    let x0 = sx.max(0) as usize;
    let x1 = (w + sx.min(0)) as usize;
    let y0 = sy.max(0) as usize;
    let y1 = (h + sy.min(0)) as usize;
    let mut acc: u64 = 0;
    let mut n: u64 = 0;
    let wu = cur.w;
    for y in (y0..y1).step_by(step) {
        let py = (y as i32 - sy) as usize;
        let cr = &cur.px[y * wu + x0..y * wu + x1];
        let start = (x0 as i32 - sx) as usize;
        let pr = &prev.px[py * wu + start..py * wu + start + (x1 - x0)];
        let mut row: u32 = 0;
        for (a, b) in cr.iter().zip(pr.iter()).step_by(step) {
            row += (*a as i32 - *b as i32).unsigned_abs();
        }
        acc += row as u64;
        n += ((x1 - x0 + step - 1) / step) as u64;
    }
    if n == 0 {
        f32::MAX
    } else {
        acc as f32 / n as f32
    }
}

fn search(prev: &Plane, cur: &Plane, cx: i32, cy: i32, r: i32, step: usize) -> (i32, i32, f32) {
    let mut best = (cx, cy, f32::MAX);
    for sy in cy - r..=cy + r {
        for sx in cx - r..=cx + r {
            let d = shifted_difference(prev, cur, sx, sy, step);
            // Prefer the smaller shift on ties so flat areas read as still.
            let better = d < best.2 - 1e-4
                || ((d - best.2).abs() <= 1e-4 && sx.abs() + sy.abs() < best.0.abs() + best.1.abs());
            if better {
                best = (sx, sy, d);
            }
        }
    }
    best
}

fn parabola(l: f32, c: f32, r: f32) -> f32 {
    let denom = l - 2.0 * c + r;
    if denom.abs() < 1e-6 || !l.is_finite() || !r.is_finite() {
        0.0
    } else {
        (0.5 * (l - r) / denom).clamp(-0.5, 0.5)
    }
}

/// Global translation between two samples as (dx, dy, reliable), in pixels
/// of the analysis picture.
///
/// Coarse-to-fine block matching: an exhaustive search on the smallest
/// pyramid level, then a small refinement on each larger level, then a
/// sub-pixel parabola fit. A shift is only trusted when lining the frames up
/// leaves a clearly smaller difference than not moving them, which rejects
/// featureless pictures and subjects crossing a plain background.
pub fn estimate_shift(prev: &MotionFrame, cur: &MotionFrame) -> (f32, f32, bool) {
    let levels = prev.levels.len().min(cur.levels.len());
    if levels == 0 {
        return (0.0, 0.0, false);
    }
    let top = levels - 1;
    let (mut sx, mut sy, _) = search(&prev.levels[top], &cur.levels[top], 0, 0, COARSE_RADIUS, 1);
    let at_edge = sx.abs() == COARSE_RADIUS || sy.abs() == COARSE_RADIUS;
    for lvl in (0..top).rev() {
        let step = if lvl == 0 { 2 } else { 1 };
        let r = if lvl == top - 1 { 2 } else { 1 };
        let (x, y, _) = search(&prev.levels[lvl], &cur.levels[lvl], sx * 2, sy * 2, r, step);
        sx = x;
        sy = y;
    }
    let p = &prev.levels[0];
    let c = &cur.levels[0];
    let best = shifted_difference(p, c, sx, sy, 2);
    let still = shifted_difference(p, c, 0, 0, 2);
    if at_edge {
        // Faster than the search can follow; the picture is moving a lot.
        return (sx as f32, sy as f32, false);
    }
    if (sx != 0 || sy != 0) && best > still * 0.9 {
        return (0.0, 0.0, still > 0.5);
    }
    if still < 0.5 && best < 0.5 {
        // Identical frames (a still image or a frozen picture).
        return (0.0, 0.0, true);
    }
    let fx = parabola(
        shifted_difference(p, c, sx - 1, sy, 2),
        best,
        shifted_difference(p, c, sx + 1, sy, 2),
    );
    let fy = parabola(
        shifted_difference(p, c, sx, sy - 1, 2),
        best,
        shifted_difference(p, c, sx, sy + 1, 2),
    );
    (sx as f32 + fx, sy as f32 + fy, true)
}

/// Measure one frame. `prev` holds the previous sample's pyramid, if any.
pub fn measure_frame(
    gray: &[u8],
    w: usize,
    h: usize,
    prev: Option<&MotionFrame>,
) -> (FrameMetrics, MotionFrame) {
    let lap = laplacian_variance(gray, w, h) as f32;
    let (mean, lo, hi, p2, p98) = exposure_stats(gray);
    let proj = motion_frame(gray, w, h);
    let (dx, dy, motion_ok) = match prev {
        Some(p) => estimate_shift(p, &proj),
        None => (0.0, 0.0, true),
    };
    (
        FrameMetrics {
            lap,
            mean,
            lo,
            hi,
            p2,
            p98,
            dx,
            dy,
            motion_ok,
        },
        proj,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoothed pseudo-random texture, cropped at an offset. Not periodic, so
    /// a shift has one right answer.
    fn pattern(w: usize, h: usize, ox: i32, oy: i32) -> Vec<u8> {
        let pad = 24usize;
        let (bw, bh) = (w + 2 * pad, h + 2 * pad);
        let mut seed: u32 = 12345;
        let mut base = vec![0f32; bw * bh];
        for v in base.iter_mut() {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            *v = ((seed >> 16) & 0xff) as f32;
        }
        let mut out = vec![0u8; w * h];
        for y in 0..h {
            for x in 0..w {
                let bx = (x as i32 + pad as i32 - ox) as usize;
                let by = (y as i32 + pad as i32 - oy) as usize;
                let mut acc = 0f32;
                for dy in 0..3 {
                    for dx in 0..3 {
                        acc += base[(by + dy - 1) * bw + bx + dx - 1];
                    }
                }
                out[y * w + x] = (acc / 9.0) as u8;
            }
        }
        out
    }

    #[test]
    fn laplacian_is_zero_for_flat_and_high_for_texture() {
        let flat = vec![128u8; 64 * 64];
        assert_eq!(laplacian_variance(&flat, 64, 64), 0.0);
        let tex = pattern(64, 64, 0, 0);
        assert!(laplacian_variance(&tex, 64, 64) > 10.0);
    }

    #[test]
    fn exposure_stats_count_clipping() {
        let mut g = vec![128u8; 1000];
        for v in g.iter_mut().take(100) {
            *v = 255;
        }
        for v in g.iter_mut().skip(900) {
            *v = 0;
        }
        let (_mean, lo, hi, p2, p98) = exposure_stats(&g);
        assert!((lo - 0.1).abs() < 1e-6);
        assert!((hi - 0.1).abs() < 1e-6);
        assert_eq!(p2, 0.0);
        assert_eq!(p98, 255.0);
    }

    #[test]
    fn shift_estimate_is_zero_for_identical_and_flat_frames() {
        let a = pattern(240, 136, 0, 0);
        let pa = motion_frame(&a, 240, 136);
        let (dx, dy, _) = estimate_shift(&pa, &pa);
        assert_eq!((dx, dy), (0.0, 0.0));
        let flat = vec![90u8; 240 * 136];
        let pf = motion_frame(&flat, 240, 136);
        let (dx, dy, _) = estimate_shift(&pf, &pf);
        assert_eq!((dx, dy), (0.0, 0.0));
    }

    #[test]
    fn shift_estimate_recovers_translation() {
        let (w, h) = (240, 136);
        let a = pattern(w, h, 0, 0);
        let b = pattern(w, h, 6, -3);
        let pa = motion_frame(&a, w, h);
        let pb = motion_frame(&b, w, h);
        let (dx, dy, ok) = estimate_shift(&pa, &pb);
        assert!(ok);
        assert!((dx - 6.0).abs() < 0.75, "dx {dx}");
        assert!((dy + 3.0).abs() < 0.75, "dy {dy}");
    }
}
