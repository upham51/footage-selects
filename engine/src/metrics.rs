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
    /// Luma at each share in `LADDER`, 0 to 255. Tone curves are monotonic,
    /// so these can be mapped through any log curve later without decoding again.
    pub ladder: [u8; LADDER_LEN],
    /// Brightest luma in the picture.
    pub vmax: u8,
    /// Share of pixels within one level of the brightest. Clipped highlights
    /// pile up at one value wherever the camera's curve tops out, which for
    /// log footage is well below 255.
    pub top: f32,
    /// Picture fingerprints for matching finished videos back to this clip.
    pub fp: [u64; 3],
    /// How far the picture is rotated from level, in degrees; positive means
    /// rotated clockwise. Only meaningful when `roll_conf` is high enough.
    pub roll: f32,
    pub roll_conf: f32,
    /// Light flicker banding against the previous sample, as a share of
    /// brightness. Negative when it could not be measured (the camera moved).
    pub flicker: f32,
    /// Rolling-shutter wobble against the previous sample, in pixels of the
    /// half-size picture. Negative when it could not be measured.
    pub wobble: f32,
    /// Average chroma of near-neutral pixels (Cb and Cr minus 128), for white
    /// balance. Only meaningful when `wb_ok`.
    pub cb: f32,
    pub cr: f32,
    pub wb_ok: bool,
}

/// Shares of pixels at which `FrameMetrics::ladder` records the luma.
pub const LADDER: [f32; LADDER_LEN] = [0.01, 0.02, 0.05, 0.10, 0.25, 0.50, 0.75, 0.90, 0.95, 0.98, 0.99];
pub const LADDER_LEN: usize = 11;

/// Everything the histogram gives us about one frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ExposureStats {
    pub mean: f32,
    pub lo: f32,
    pub hi: f32,
    pub p2: f32,
    pub p98: f32,
    pub ladder: [u8; LADDER_LEN],
    pub vmax: u8,
    pub top: f32,
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

/// Histogram-based exposure measurements.
pub fn exposure_stats(gray: &[u8]) -> ExposureStats {
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
    let pct = |p: f64| -> u8 {
        let target = p * n;
        let mut acc = 0f64;
        for (i, &c) in hist.iter().enumerate() {
            acc += c as f64;
            if acc >= target {
                return i as u8;
            }
        }
        255
    };
    let mut ladder = [0u8; LADDER_LEN];
    for (k, &p) in LADDER.iter().enumerate() {
        ladder[k] = pct(p as f64);
    }
    let vmax = hist.iter().rposition(|&c| c > 0).unwrap_or(0) as u8;
    let near_top: u32 = hist[vmax.saturating_sub(1) as usize..=vmax as usize].iter().sum();
    ExposureStats {
        mean: (total as f64 / n) as f32,
        lo: (lo as f64 / n) as f32,
        hi: (hi as f64 / n) as f32,
        p2: pct(0.02) as f32,
        p98: pct(0.98) as f32,
        ladder,
        vmax,
        top: (near_top as f64 / n) as f32,
    }
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
    /// Mean of each row of the analysis picture, for flicker.
    pub rows: Vec<f32>,
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
    MotionFrame { levels, rows: crate::picture::row_means(gray, w, h) }
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
        acc += row_difference(cr, pr, step) as u64;
        n += ((x1 - x0 + step - 1) / step) as u64;
    }
    if n == 0 {
        f32::MAX
    } else {
        acc as f32 / n as f32
    }
}

/// Sum of absolute differences of every `step`th pixel of two rows. Written
/// so the compiler can use vector instructions; the sum is exact either way.
fn row_difference(a: &[u8], b: &[u8], step: usize) -> u32 {
    match step {
        1 => a.iter().zip(b).map(|(x, y)| x.abs_diff(*y) as u32).sum(),
        2 => {
            let pairs = a.chunks_exact(2).zip(b.chunks_exact(2));
            let tail = match (a.len() % 2, a.last(), b.last()) {
                (1, Some(x), Some(y)) => x.abs_diff(*y) as u32,
                _ => 0,
            };
            pairs.map(|(x, y)| x[0].abs_diff(y[0]) as u32).sum::<u32>() + tail
        }
        _ => a.iter().zip(b).step_by(step).map(|(x, y)| x.abs_diff(*y) as u32).sum(),
    }
}

/// Tiles across the picture for `shift_is_global`.
const TILES_X: usize = 4;
const TILES_Y: usize = 4;

/// Tiles with detail needed before a weak shift is trusted. With fewer, a
/// subject moving over a plain scene cannot be told from a camera move, and
/// the sample is left unmeasured.
const MIN_DETAILED_TILES: usize = 6;

/// Like `shifted_difference`, but one mean absolute difference for each tile
/// of a `TILES_X` by `TILES_Y` grid over `cur`. Tiles with no overlap get `f32::MAX`.
pub fn tile_differences(prev: &Plane, cur: &Plane, sx: i32, sy: i32, step: usize) -> Vec<f32> {
    let (w, h) = (cur.w as i32, cur.h as i32);
    let mut acc = vec![0u64; TILES_X * TILES_Y];
    let mut cnt = vec![0u64; TILES_X * TILES_Y];
    if prev.w == cur.w && prev.h == cur.h && sx.abs() < w / 2 && sy.abs() < h / 2 {
        let x0 = sx.max(0) as usize;
        let x1 = (w + sx.min(0)) as usize;
        let y0 = sy.max(0) as usize;
        let y1 = (h + sy.min(0)) as usize;
        let wu = cur.w;
        for y in (y0..y1).step_by(step) {
            let ty = (y * TILES_Y / cur.h).min(TILES_Y - 1);
            let py = (y as i32 - sy) as usize;
            for x in (x0..x1).step_by(step) {
                let px = (x as i32 - sx) as usize;
                let tx = (x * TILES_X / cur.w).min(TILES_X - 1);
                let k = ty * TILES_X + tx;
                acc[k] += (cur.px[y * wu + x] as i32 - prev.px[py * wu + px] as i32).unsigned_abs() as u64;
                cnt[k] += 1;
            }
        }
    }
    acc.iter()
        .zip(cnt.iter())
        .map(|(&a, &n)| if n == 0 { f32::MAX } else { a as f32 / n as f32 })
        .collect()
}

/// Whether a shift that only slightly improves the match is still a real
/// movement of the whole picture.
///
/// In dark, noisy footage noise dominates the difference between two frames,
/// so even a correct shift lowers it by only a few percent. A real camera
/// move improves the match across the picture: in most tiles that have any
/// detail, lining up by the shift recovers a good part of what misaligning by
/// two more pixels would lose. A subject moving over a still, plain scene
/// improves only the tiles it covers, and the plain tiles have no detail to
/// tell either way, so it does not pass.
pub fn shift_is_global(prev: &Plane, cur: &Plane, sx: i32, sy: i32) -> bool {
    let still = tile_differences(prev, cur, 0, 0, 2);
    let best = tile_differences(prev, cur, sx, sy, 2);
    let around = [(2, 0), (-2, 0), (0, 2), (0, -2)].map(|(ox, oy)| tile_differences(prev, cur, sx + ox, sy + oy, 2));
    let (mut detailed, mut agree) = (0usize, 0usize);
    for k in 0..still.len() {
        let (s, b) = (still[k], best[k]);
        if s == f32::MAX || b == f32::MAX || around.iter().any(|a| a[k] == f32::MAX) {
            continue;
        }
        let far = around.iter().map(|a| a[k]).sum::<f32>() / 4.0;
        // A tile has detail when misaligning it visibly raises the difference.
        if far - b <= 0.03 * b + 0.05 {
            continue;
        }
        detailed += 1;
        if s - b >= 0.15 * (far - b) {
            agree += 1;
        }
    }
    detailed >= MIN_DETAILED_TILES && agree * 4 >= detailed * 3
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
        // Faster than the search can follow; the picture is moving a lot. The
        // shift found is a lower bound, returned in analysis pixels like any
        // other, and marked unreliable. A non-zero shift on an unreliable
        // sample therefore means "too fast to follow", see `scores`.
        let k = (1i32 << top) as f32;
        return (sx as f32 * k, sy as f32 * k, false);
    }
    if (sx != 0 || sy != 0) && best > still * 0.9 && !shift_is_global(p, c, sx, sy) {
        // Lining the frames up barely helps. Either the camera is still and a
        // subject moves over a plain background, or the picture is so noisy
        // (night footage) that noise hides a real shift. Neither is a
        // measurement of the camera, so the sample is left unmeasured and its
        // motion is filled in from its neighbours. Calling it "not moving"
        // instead made noisy pans and handheld shots look shaky.
        return (0.0, 0.0, false);
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

/// Largest shift between samples, in analysis pixels, at which the camera
/// counts as still enough to compare rows for flicker.
pub const FLICKER_STILL_SHIFT: f32 = 1.0;
/// Largest shift, in analysis pixels, at which wobble is still measured: the
/// band search covers a few pixels around the global shift at half size.
pub const WOBBLE_MAX_SHIFT: f32 = 60.0;

/// Measure one frame from its full-range YUV 4:2:0 planes: everything
/// `measure_frame` does on the luma, plus white balance from the chroma.
pub fn measure_frame_yuv(
    y: &[u8],
    u: &[u8],
    v: &[u8],
    w: usize,
    h: usize,
    prev: Option<&MotionFrame>,
) -> (FrameMetrics, MotionFrame) {
    let (mut m, proj) = measure_frame(y, w, h, prev);
    if let Some((cb, cr)) = crate::picture::neutral_chroma(y, u, v, w, h) {
        m.cb = cb;
        m.cr = cr;
        m.wb_ok = true;
    }
    (m, proj)
}

/// Measure one frame. `prev` holds the previous sample's pyramid, if any.
pub fn measure_frame(
    gray: &[u8],
    w: usize,
    h: usize,
    prev: Option<&MotionFrame>,
) -> (FrameMetrics, MotionFrame) {
    let lap = laplacian_variance(gray, w, h) as f32;
    let e = exposure_stats(gray);
    let proj = motion_frame(gray, w, h);
    let (dx, dy, motion_ok) = match prev {
        Some(p) => estimate_shift(p, &proj),
        None => (0.0, 0.0, true),
    };
    let (roll, roll_conf) = crate::picture::horizon_roll(gray, w, h);
    let mut flicker = -1.0;
    let mut wobble = -1.0;
    if let Some(p) = prev {
        if motion_ok && dx.abs() < FLICKER_STILL_SHIFT && dy.abs() < FLICKER_STILL_SHIFT && p.rows.len() == proj.rows.len() {
            flicker = crate::picture::flicker_strength(&p.rows, &proj.rows);
        }
        if motion_ok && dx.abs() < WOBBLE_MAX_SHIFT && dy.abs() < WOBBLE_MAX_SHIFT && p.levels.len() > 1 && proj.levels.len() > 1 {
            wobble = crate::picture::wobble(&p.levels[1], &proj.levels[1], dx / 2.0, dy / 2.0);
        }
    }
    (
        FrameMetrics {
            lap,
            mean: e.mean,
            lo: e.lo,
            hi: e.hi,
            p2: e.p2,
            p98: e.p98,
            dx,
            dy,
            motion_ok,
            ladder: e.ladder,
            vmax: e.vmax,
            top: e.top,
            fp: crate::fingerprint::frame_hashes(gray, w, h),
            roll,
            roll_conf,
            flicker,
            wobble,
            cb: 0.0,
            cr: 0.0,
            wb_ok: false,
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
        let e = exposure_stats(&g);
        assert!((e.lo - 0.1).abs() < 1e-6);
        assert!((e.hi - 0.1).abs() < 1e-6);
        assert_eq!(e.p2, 0.0);
        assert_eq!(e.p98, 255.0);
        assert_eq!(e.vmax, 255);
        assert!((e.top - 0.1).abs() < 1e-6);
        assert_eq!(e.ladder[5], 128);
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

    #[test]
    fn row_difference_matches_the_plain_sum() {
        let mut seed: u32 = 99;
        for len in [0usize, 1, 2, 3, 7, 60, 479, 480] {
            let mut next = || {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                (seed >> 16) as u8
            };
            let a: Vec<u8> = (0..len).map(|_| next()).collect();
            let b: Vec<u8> = (0..len).map(|_| next()).collect();
            for step in 1..=3 {
                let plain: u32 = a
                    .iter()
                    .zip(&b)
                    .step_by(step)
                    .map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs())
                    .sum();
                assert_eq!(row_difference(&a, &b, step), plain, "len {len} step {step}");
            }
        }
    }

    /// Gaussian-like noise with standard deviation `sigma`, one value per pixel.
    fn noisy(px: &[u8], sigma: f32, seed: u32) -> Vec<u8> {
        let mut s = seed;
        let mut next = || {
            s = s.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            ((s >> 16) & 0x7fff) as f32 / 32767.0
        };
        px.iter()
            .map(|&v| {
                // The sum of 4 uniforms has standard deviation 0.577.
                let n = (next() + next() + next() + next() - 2.0) / 0.577 * sigma;
                (v as f32 + n).round().clamp(0.0, 255.0) as u8
            })
            .collect()
    }

    fn dim(px: &[u8]) -> Vec<u8> {
        px.iter().map(|&v| (16.0 + (v as f32 - 16.0) * 0.15) as u8).collect()
    }

    #[test]
    fn shift_is_recovered_in_dark_noisy_frames() {
        let (w, h) = (480, 270);
        let a = noisy(&dim(&pattern(w, h, 0, 0)), 3.0, 1);
        let b = noisy(&dim(&pattern(w, h, 2, -1)), 3.0, 2);
        let (dx, dy, ok) = estimate_shift(&motion_frame(&a, w, h), &motion_frame(&b, w, h));
        assert!(ok, "a real shift in noise must count as measured");
        assert!((dx - 2.0).abs() < 0.75 && (dy + 1.0).abs() < 0.75, "dx {dx} dy {dy}");
    }

    #[test]
    fn a_small_subject_over_a_plain_noisy_scene_is_not_camera_movement() {
        let (w, h) = (480, 270);
        let tex = pattern(w, h, 0, 0);
        let frame = |ox: usize| {
            let mut f = vec![60u8; w * h];
            for y in 100..170 {
                for x in 200..280 {
                    f[y * w + x + ox] = tex[y * w + x];
                }
            }
            f
        };
        let a = noisy(&frame(0), 3.0, 3);
        let b = noisy(&frame(4), 3.0, 4);
        let (pa, pb) = (motion_frame(&a, w, h), motion_frame(&b, w, h));
        assert!(!shift_is_global(&pa.levels[0], &pb.levels[0], 4, 0));
        // The same shift of the whole picture is camera movement.
        let c = noisy(&dim(&pattern(w, h, 0, 0)), 3.0, 5);
        let d = noisy(&dim(&pattern(w, h, 4, 0)), 3.0, 6);
        let (pc, pd) = (motion_frame(&c, w, h), motion_frame(&d, w, h));
        assert!(shift_is_global(&pc.levels[0], &pd.levels[0], 4, 0));
    }
}
