//! Picture faults measured on the small analysis frames.
//!
//! All plain arithmetic on 8-bit pictures, cheap enough for every sample:
//! 1. Horizon roll: the tilt of the dominant straight edges from level.
//! 2. Light flicker: moving horizontal bands from mains powered lights seen
//!    through a rolling shutter, from the row means of two samples.
//! 3. Rolling-shutter wobble ("jello"): bands of rows displaced by different
//!    amounts in a wavy pattern, from two pyramid planes.
//! 4. White balance cue: the average chroma of near-neutral pixels.

use crate::metrics::Plane;

// Horizon roll.

/// Share of the measured pixels, by gradient strength, that count as strong edges.
const EDGE_SHARE: f32 = 0.09;
/// Sobel magnitude a pixel needs to count as an edge at all. A step of about
/// 12 levels gives 48; a smooth sky gradient or vignette stays far below.
const EDGE_FLOOR: f32 = 48.0;
/// Strong edge pixels needed, as a share of the measured pixels, before reading a roll.
const MIN_EDGE_SHARE: f32 = 0.004;
/// Largest roll searched for, in degrees.
const ROLL_RANGE: f32 = 12.0;
/// Histogram bins per degree.
const ROLL_BINS_PER_DEGREE: f32 = 10.0;
/// Half width of the window around the histogram peak for its weighted mean.
const ROLL_PEAK_WINDOW: f32 = 1.5;
/// Half width of the window around the reading whose edge weight gives the confidence.
const ROLL_CONFIDENCE_WINDOW: f32 = 1.5;
/// Share of edge weight near the reading that a picture without lines still gets
/// (2 of 90 degrees for uniform orientations, a little more from chance peaks).
const ROLL_SHARE_BASE: f32 = 0.08;
/// Share of edge weight near the reading at which confidence reaches 1.
const ROLL_SHARE_FULL: f32 = 0.45;
/// Most edge pixels used for the projection refinement; more are thinned evenly.
const ROLL_PROJECTION_PIXELS: usize = 2500;
/// Projection search around the histogram reading: farthest reach and step, in degrees.
const ROLL_PROJECTION_SPAN: f32 = 3.0;
const ROLL_PROJECTION_STEP: f32 = 0.1;

/// Arc tangent for arguments between -1 and 1, in radians, with an error under
/// 0.001 degrees. Much faster than `f32::atan` and plenty for edge angles.
#[inline]
fn atan_unit(x: f32) -> f32 {
    let x2 = x * x;
    x * (0.999_977_26
        + x2 * (-0.332_623_47 + x2 * (0.193_543_46 + x2 * (-0.116_432_87 + x2 * (0.052_653_32 + x2 * -0.011_721_2)))))
}

/// Sobel gradients (x to the right, y down) of the interior of row `y`, one
/// per column from 1 to w - 2, and their squared magnitudes. A Sobel gradient
/// stays within 1020, so 16-bit lanes keep the loop wide on baseline x86-64.
#[inline]
fn sobel_row(gray: &[u8], w: usize, y: usize, gx: &mut [i16], gy: &mut [i16], mag2: &mut [i32]) {
    let iw = w - 2;
    let r0 = &gray[(y - 1) * w..y * w];
    let r1 = &gray[y * w..(y + 1) * w];
    let r2 = &gray[(y + 1) * w..(y + 2) * w];
    let (a, b, c) = (&r0[..iw], &r0[1..iw + 1], &r0[2..iw + 2]);
    let (d, f) = (&r1[..iw], &r1[2..iw + 2]);
    let (g, m, i) = (&r2[..iw], &r2[1..iw + 1], &r2[2..iw + 2]);
    let (gx, gy, mag2) = (&mut gx[..iw], &mut gy[..iw], &mut mag2[..iw]);
    for x in 0..iw {
        let (a, b, c) = (a[x] as i16, b[x] as i16, c[x] as i16);
        let (d, f) = (d[x] as i16, f[x] as i16);
        let (g, m, i) = (g[x] as i16, m[x] as i16, i[x] as i16);
        let sx = (c + 2 * f + i) - (a + 2 * d + g);
        let sy = (g + 2 * m + i) - (a + 2 * b + c);
        gx[x] = sx;
        gy[x] = sy;
        mag2[x] = sx as i32 * sx as i32 + sy as i32 * sy as i32;
    }
}

/// Sobel gradient magnitude at (x, y), or 0 on the picture's border.
fn sobel_magnitude(gray: &[u8], w: usize, x: usize, y: usize) -> f32 {
    let h = gray.len() / w;
    if x == 0 || y == 0 || x + 1 >= w || y + 1 >= h {
        return 0.0;
    }
    let p = |dx: usize, dy: usize| gray[(y + dy - 1) * w + x + dx - 1] as f32;
    let gx = (p(2, 0) + 2.0 * p(2, 1) + p(2, 2)) - (p(0, 0) + 2.0 * p(0, 1) + p(0, 2));
    let gy = (p(0, 2) + 2.0 * p(1, 2) + p(2, 2)) - (p(0, 0) + 2.0 * p(1, 0) + p(2, 0));
    (gx * gx + gy * gy).sqrt()
}

/// An edge pixel kept for the projection refinement.
#[derive(Clone, Copy)]
struct EdgePixel {
    /// Position across the edge's axis: y for a near-horizontal edge, x for a near-vertical one.
    across: f32,
    /// Position along the axis.
    along: f32,
    weight: f32,
    horizontal: bool,
}

/// Fractional positions per pixel in the projection kernel table.
const KERNEL_PHASES: usize = 64;

/// Gaussian weights (standard deviation 0.8 bins) over 5 bins for each
/// fractional position. A kernel this wide makes the sum of squares of the
/// profile almost independent of where the pixels fall between bins (it
/// varies by under 0.4%); with plain linear interpolation, pixels lying
/// exactly on bins score up to twice as high, which pulls readings onto
/// angles where rows of pixels line up with the grid, such as 0.
fn projection_kernel() -> Vec<[f32; 5]> {
    (0..KERNEL_PHASES)
        .map(|p| {
            let f = p as f32 / KERNEL_PHASES as f32;
            let mut k = [0f32; 5];
            for (j, v) in k.iter_mut().enumerate() {
                let d = j as f32 - 2.0 - f;
                *v = (-0.5 * d * d / 0.64).exp();
            }
            k
        })
        .collect()
}

/// How tightly the edge pixels line up when projected along lines at `deg`:
/// the sum of squares of their weight profile across the lines. Straight edges
/// at that angle pile into a few bins of the profile.
fn projection_score(pixels: &[EdgePixel], deg: f32, profile: &mut [f32], offset: f32, kernel: &[[f32; 5]]) -> f32 {
    let t = deg.to_radians().tan();
    profile.iter_mut().for_each(|v| *v = 0.0);
    let half = profile.len() / 2;
    let last = (half - 6) as f32;
    for p in pixels {
        // A level horizon tilted clockwise runs down to the right, y = c + x t;
        // an upright wall tilted the same way runs left going down, x = c - y t.
        let (pos, base) = if p.horizontal { (p.across - p.along * t, 0) } else { (p.across + p.along * t, half) };
        let v = (pos + offset).clamp(0.0, last);
        let i = v as usize;
        let phase = (((v - i as f32) * KERNEL_PHASES as f32) as usize).min(KERNEL_PHASES - 1);
        let bins = &mut profile[base + i..base + i + 5];
        for (b, k) in bins.iter_mut().zip(&kernel[phase]) {
            *b += p.weight * k;
        }
    }
    profile.iter().map(|v| v * v).sum()
}

/// Horizon roll: how far the picture is rotated from level, from the dominant near-horizontal and near-vertical edges.
/// Returns (degrees, confidence 0..1). Positive degrees mean the picture is rotated clockwise, so a level horizon in the scene runs down to the right in the frame; rotating the picture counter-clockwise by that many degrees levels it. Confidence is low for pictures without clear straight lines (foliage, water, faces, blank sky); callers ignore readings under about 0.35.
///
/// Sobel gradients give each pixel an edge orientation. The strongest 9% of
/// edges (above a floor) vote, weighted by strength, for their deviation from
/// the nearest axis; a level horizon and upright walls both vote 0. In picture
/// coordinates with y running down, a gradient at 90 + a degrees belongs to a
/// near-horizontal edge running down to the right by a degrees, and one at a
/// degrees to a near-vertical edge turned clockwise by the same amount, so both
/// fold to the same signed deviation. The peak of the smoothed votes within 12
/// degrees gives a first reading, and the confidence.
///
/// Per-pixel orientations of sharp edges lean toward the axis, by up to about
/// 15% of the angle on hard steps: the strongest pixels sit where the step
/// crosses the pixel grid squarely. So the reading is refined by projection:
/// the edge pixels that voted near the peak are projected along candidate
/// angles, and the angle at which they pile up most tightly wins. That depends
/// only on where the edge pixels are, not on their orientations.
///
/// Gradients are taken on every other row: the 3 by 3 window still sees every
/// row, straight edges are long enough that half their pixels place them as
/// well as all of them, and it halves the cost.
pub fn horizon_roll(gray: &[u8], w: usize, h: usize) -> (f32, f32) {
    if w < 16 || h < 16 || gray.len() < w * h {
        return (0.0, 0.0);
    }
    let iw = w - 2;
    let rows = (h - 1) / 2;
    let n = iw * rows;
    let mut gx = vec![0i16; n];
    let mut gy = vec![0i16; n];
    let mut mag2 = vec![0i32; n];
    for r in 0..rows {
        let o = r * iw;
        sobel_row(gray, w, 2 * r + 1, &mut gx[o..o + iw], &mut gy[o..o + iw], &mut mag2[o..o + iw]);
    }
    // Threshold for the strongest share of pixels, never below the floor, from
    // a histogram of every other pixel. Bins are 128 wide in squared
    // magnitude, finer than one level of magnitude above the floor.
    let mut hist = vec![0u32; ((2 * 1020 * 1020) >> 7) + 2];
    let top = hist.len() - 1;
    let mut sampled = 0u32;
    for row in mag2.chunks_exact(iw) {
        for &m2 in row.iter().step_by(2) {
            hist[((m2 as u32) >> 7).min(top as u32) as usize] += 1;
            sampled += 1;
        }
    }
    let target = ((sampled as f32 * EDGE_SHARE) as u32).max(1);
    let mut acc = 0u32;
    let mut bin = top;
    while bin > 0 {
        acc += hist[bin];
        if acc >= target {
            break;
        }
        bin -= 1;
    }
    let thr2 = ((bin << 7) as i32).max((EDGE_FLOOR * EDGE_FLOOR) as i32);
    // Gather the strong pixels without a branch per pixel; which pixels pass
    // is unpredictable, and mispredicted branches would cost more than the rest.
    let mut strong = vec![0u32; n + 1];
    let mut count = 0usize;
    for (k, &m2) in mag2.iter().enumerate() {
        strong[count] = k as u32;
        count += (m2 >= thr2) as usize;
    }
    strong.truncate(count);
    if (count as f32) < MIN_EDGE_SHARE * n as f32 {
        return (0.0, 0.0);
    }
    // Deviation of each strong edge from the nearest axis, in -45..45 degrees,
    // and its weight.
    let mut devs = Vec::with_capacity(count);
    let mut wgts = Vec::with_capacity(count);
    for &k in &strong {
        let k = k as usize;
        let (x, y) = (gx[k] as f32, gy[k] as f32);
        let (num, den) = if y.abs() >= x.abs() { (-x, y) } else { (y, x) };
        devs.push(atan_unit(num / den).to_degrees());
        wgts.push((mag2[k] as f32).sqrt());
    }
    let total: f32 = wgts.iter().sum();
    if total <= 0.0 {
        return (0.0, 0.0);
    }
    // Votes within the search range; the extra last bin collects the rest.
    let bins = (2.0 * ROLL_RANGE * ROLL_BINS_PER_DEGREE) as usize + 1;
    let mut votes = vec![0f32; bins + 1];
    for (&d, &wt) in devs.iter().zip(&wgts) {
        let b = ((d + ROLL_RANGE) * ROLL_BINS_PER_DEGREE).round();
        let b = if b >= 0.0 && b < bins as f32 { b as usize } else { bins };
        votes[b] += wt;
    }
    votes.truncate(bins);
    // Smooth the votes with a Gaussian of 0.5 degrees. Sharp edges near the
    // axes vote in two clusters either side of their true angle (where the
    // step sits on a row and where it straddles two), so the first reading
    // only needs to land between them; projection does the rest.
    let sigma = 0.5 * ROLL_BINS_PER_DEGREE;
    let reach = (3.0 * sigma) as isize;
    let taps: Vec<f32> = (-reach..=reach).map(|j| (-0.5 * (j as f32 / sigma).powi(2)).exp()).collect();
    let smooth: Vec<f32> = (0..bins as isize)
        .map(|i| {
            (-reach..=reach)
                .zip(&taps)
                .filter(|(j, _)| (0..bins as isize).contains(&(i + j)))
                .map(|(j, t)| t * votes[(i + j) as usize])
                .sum()
        })
        .collect();
    let peak = smooth
        .iter()
        .enumerate()
        .fold((0usize, f32::MIN), |best, (i, &v)| if v > best.1 { (i, v) } else { best })
        .0;
    let near = |est: f32, half: f32| -> (f32, f32) {
        let (mut sw, mut sd) = (0f32, 0f32);
        for (&d, &wt) in devs.iter().zip(&wgts) {
            let inside = ((d - est).abs() <= half) as u32 as f32;
            sw += inside * wt;
            sd += inside * wt * d;
        }
        (sw, sd)
    };
    let mut first = peak as f32 / ROLL_BINS_PER_DEGREE - ROLL_RANGE;
    for _ in 0..3 {
        let (sw, sd) = near(first, ROLL_PEAK_WINDOW);
        if sw <= 0.0 {
            break;
        }
        first = sd / sw;
    }
    // Confidence: how much of the strong edge weight agrees with the peak.
    let share = near(first, ROLL_CONFIDENCE_WINDOW).0 / total;
    let confidence = ((share - ROLL_SHARE_BASE) / (ROLL_SHARE_FULL - ROLL_SHARE_BASE)).clamp(0.0, 1.0);

    // Edge pixels that voted near the peak, in a window wide enough for the
    // two clusters and the lean.
    let window = 2.0 + 0.25 * first.abs();
    let voters: Vec<usize> = (0..count).filter(|&i| (devs[i] - first).abs() <= window).collect();
    if voters.len() < 16 {
        return (first, confidence);
    }
    let stride = voters.len().div_ceil(ROLL_PROJECTION_PIXELS);
    let pixels: Vec<EdgePixel> = voters
        .iter()
        .step_by(stride)
        .map(|&i| {
            let k = strong[i] as usize;
            let (x, y) = (k % iw + 1, k / iw * 2 + 1);
            let horizontal = gy[k].unsigned_abs() >= gx[k].unsigned_abs();
            // Where the edge crosses this pixel's column (or row), to a fraction
            // of a pixel: the peak of a parabola through the gradient magnitude
            // here and one pixel either side across the edge. Every pixel of
            // the edge's cross-section then reports the same position.
            let centre = (mag2[k] as f32).sqrt();
            let (before, after) = if horizontal {
                (sobel_magnitude(gray, w, x, y - 1), sobel_magnitude(gray, w, x, y + 1))
            } else {
                (sobel_magnitude(gray, w, x - 1, y), sobel_magnitude(gray, w, x + 1, y))
            };
            let denom = before - 2.0 * centre + after;
            let shift = if denom < 0.0 { (0.5 * (before - after) / denom).clamp(-1.0, 1.0) } else { 0.0 };
            let (x, y) = (x as f32, y as f32);
            let (across, along) = if horizontal { (y + shift, x) } else { (x + shift, y) };
            EdgePixel { across, along, weight: wgts[i], horizontal }
        })
        .collect();
    // Two profiles side by side, one for each kind of edge, each long enough
    // for any position at any searched angle.
    let reach = (w.max(h) as f32 * (ROLL_RANGE + ROLL_PROJECTION_SPAN + 2.0).to_radians().tan()).ceil();
    let half = w.max(h) + 2 * reach as usize + 8;
    let mut profile = vec![0f32; 2 * half];
    let offset = reach + 1.0;
    let kernel = projection_kernel();
    let mut score_at = |deg: f32| projection_score(&pixels, deg, &mut profile, offset, &kernel);
    // Climb from the first reading in small steps while the pile-up tightens.
    let step = ROLL_PROJECTION_STEP;
    let mut best = first;
    let mut best_score = score_at(best);
    let (mut lo, mut hi) = (score_at(best - step), score_at(best + step));
    while (lo > best_score || hi > best_score) && (best - first).abs() < ROLL_PROJECTION_SPAN {
        if hi >= lo {
            (lo, best_score) = (best_score, hi);
            best += step;
            hi = score_at(best + step);
        } else {
            (hi, best_score) = (best_score, lo);
            best -= step;
            lo = score_at(best - step);
        }
    }
    // Quarter steps around the best, then a parabola through them.
    let fine = 0.25 * step;
    let (l, c, r) = (score_at(best - fine), best_score, score_at(best + fine));
    let denom = l - 2.0 * c + r;
    if denom < 0.0 {
        best += fine * (0.5 * (l - r) / denom).clamp(-1.0, 1.0);
    }
    (best, confidence)
}

// Light flicker.

/// Row means below this are too dark to show bands reliably.
const FLICKER_DARK: f32 = 16.0;
/// Row means above this may be clipped, which flattens bands.
const FLICKER_BRIGHT: f32 = 248.0;
/// Band frequencies searched, in cycles over the picture height.
const FLICKER_MIN_CYCLES: f64 = 0.7;
const FLICKER_MAX_CYCLES: f64 = 12.0;
const FLICKER_STEP: f64 = 0.1;
/// How many noise standard errors the band difference must reach to count.
const FLICKER_SIGNIFICANCE: f64 = 4.0;
/// The band amplitude is at most this many times half the measured change
/// between the samples. Bands that barely moved cannot be told from scene
/// structure, so their amplitude is not trusted beyond this.
const FLICKER_MAX_GAIN: f64 = 2.0;

/// Mean of each row, for flicker detection between consecutive samples.
pub fn row_means(gray: &[u8], w: usize, h: usize) -> Vec<f32> {
    if w == 0 {
        return vec![0.0; h];
    }
    gray.chunks_exact(w)
        .take(h)
        .map(|row| row.iter().map(|&v| v as u32).sum::<u32>() as f32 / w as f32)
        .collect()
}

/// Least-squares cosine and sine coefficients of `d` at `cycles` over `n` rows,
/// counting only rows with weight 1, and the energy the fit explains.
fn sinusoid_fit(d: &[f64], wts: &[f64], cycles: f64) -> (f64, f64, f64) {
    let n = d.len();
    let step = 2.0 * std::f64::consts::PI * cycles / n as f64;
    let (sd, cd) = step.sin_cos();
    // Start at the centre of row 0.
    let (mut s, mut c) = (0.5 * step).sin_cos();
    let (mut scc, mut sss, mut scs, mut sdc, mut sds) = (0f64, 0f64, 0f64, 0f64, 0f64);
    for y in 0..n {
        let wt = wts[y];
        let (wc, ws) = (wt * c, wt * s);
        scc += wc * c;
        sss += ws * s;
        scs += wc * s;
        sdc += wc * d[y];
        sds += ws * d[y];
        let nc = c * cd - s * sd;
        s = s * cd + c * sd;
        c = nc;
    }
    let det = scc * sss - scs * scs;
    if det.abs() < 1e-9 {
        return (0.0, 0.0, 0.0);
    }
    let a = (sdc * sss - sds * scs) / det;
    let b = (sds * scc - sdc * scs) / det;
    (a, b, a * sdc + b * sds)
}

/// Removes the least-squares line through the weighted rows.
fn detrend(v: &mut [f64], wts: &[f64]) {
    let n = v.len();
    let (mut sw, mut st, mut stt, mut sv, mut stv) = (0f64, 0f64, 0f64, 0f64, 0f64);
    for y in 0..n {
        let t = (y as f64 + 0.5) / n as f64 - 0.5;
        let wt = wts[y];
        sw += wt;
        st += wt * t;
        stt += wt * t * t;
        sv += wt * v[y];
        stv += wt * t * v[y];
    }
    let det = sw * stt - st * st;
    let (a, b) = if det.abs() < 1e-12 { (sv / sw.max(1.0), 0.0) } else { ((sv * stt - stv * st) / det, (stv * sw - sv * st) / det) };
    for (y, x) in v.iter_mut().enumerate() {
        let t = (y as f64 + 0.5) / n as f64 - 0.5;
        *x = if wts[y] > 0.0 { *x - a - b * t } else { 0.0 };
    }
}

/// Largest vertical shift between the samples that `align_rows` makes up, in rows.
const FLICKER_MAX_SHIFT: f64 = 2.0;
/// Trial shifts for `align_rows` are this far apart, in rows.
const FLICKER_SHIFT_STEP: f64 = 0.25;
/// Charge for shifting, in log brightness per row of shift: profiles with
/// less fine structure than this stay put rather than chase noise.
const FLICKER_PLAIN: f64 = 0.002;

/// `lp` moved down by `shift` rows with linear interpolation, and the weight
/// of each row (1 where both the source rows and the current row are usable).
fn resample_rows(lp: &[f64], pw: &[f64], cw: &[f64], shift: f64) -> (Vec<f64>, Vec<f64>) {
    let n = lp.len();
    let mut out = vec![0f64; n];
    let mut wts = vec![0f64; n];
    for y in 0..n {
        let src = y as f64 - shift;
        if src < 0.0 || src > (n - 1) as f64 {
            continue;
        }
        let i = (src as usize).min(n - 2);
        let f = src - i as f64;
        if pw[i] > 0.0 && pw[i + 1] > 0.0 && cw[y] > 0.0 {
            out[y] = lp[i] * (1.0 - f) + lp[i + 1] * f;
            wts[y] = 1.0;
        }
    }
    (out, wts)
}

/// Resamples the previous log row means to line up with the current ones and
/// returns them with the weight of each row (1 where both are usable) and the shift.
/// Trial shifts are tried every 1/4 row up to 2 rows either way, and the best
/// refined with two more and a parabola.
///
/// Even a camera that "barely moved" shifts the picture by a fraction of a
/// row, and with strong horizontal structure (blinds, shelves, a horizon) that
/// alone changes the row means in a periodic way. The shift is found from the
/// fine structure only: both the difference and the slope of the profile go
/// through a second difference first, which keeps detail a few rows across
/// and all but removes flicker bands, whose period is at least 1/12 of the
/// height. Otherwise moving bands, which look like a shift of the bands, would
/// be lined up and cancelled in a plain scene.
fn align_rows(lp: &[f64], pw: &[f64], lc: &[f64], cw: &[f64]) -> (Vec<f64>, Vec<f64>, f64) {
    let n = lp.len();
    let resample = |shift: f64| resample_rows(lp, pw, cw, shift);
    // How badly the fine structure disagrees at a trial shift, plus a small
    // charge for shifting at all, so a plain picture stays where it is.
    let cost = |shift: f64| -> f64 {
        let (ls, wts) = resample(shift);
        let (mut sum, mut count) = (0f64, 0f64);
        for y in 1..n - 1 {
            if wts[y - 1] == 0.0 || wts[y] == 0.0 || wts[y + 1] == 0.0 {
                continue;
            }
            let q = (lc[y - 1] - ls[y - 1]) - 2.0 * (lc[y] - ls[y]) + (lc[y + 1] - ls[y + 1]);
            sum += q * q;
            count += 1.0;
        }
        if count < 16.0 {
            return f64::MAX;
        }
        sum / count + FLICKER_PLAIN * FLICKER_PLAIN * shift * shift
    };
    // Every coarse step, then half steps either side of the best, then a
    // parabola through the best three.
    let steps = (FLICKER_MAX_SHIFT / FLICKER_SHIFT_STEP).round() as i32;
    let costs: Vec<f64> = (-steps..=steps).map(|k| cost(k as f64 * FLICKER_SHIFT_STEP)).collect();
    let best = costs
        .iter()
        .enumerate()
        .fold((0usize, f64::MAX), |b, (i, &c)| if c < b.1 { (i, c) } else { b })
        .0;
    let mut shift = (best as i32 - steps) as f64 * FLICKER_SHIFT_STEP;
    let half = 0.5 * FLICKER_SHIFT_STEP;
    let (l, c, r) = (cost(shift - half), costs[best], cost(shift + half));
    let denom = l - 2.0 * c + r;
    if denom > 0.0 && l.is_finite() && r.is_finite() {
        shift += (0.5 * (l - r) / denom).clamp(-1.0, 1.0) * half;
    }
    let (ls, wts) = resample(shift);
    (ls, wts, shift)
}

/// Light flicker banding between two samples taken while the camera is still: lights on mains power pulse at 100 or 120 Hz, which a rolling shutter turns into horizontal bands that move between frames. Divide the current row means by the previous ones (ignoring very dark rows), detrend, and measure the strongest sinusoid with between 0.7 and 12 cycles over the picture height. Returns the band amplitude as a share of brightness (0.03 means the bands swing about 3% either side), 0 when nothing periodic is there. Static scene structure cancels out in the ratio. The caller only calls this when the camera barely moved between the samples.
///
/// The ratio is taken as a difference of logarithms. It only shows how much the
/// bands moved: bands of amplitude A whose phase moved by p give a ratio
/// amplitude of 2 A sin(p / 2). To recover A, the same sinusoid is fitted to
/// each sample's own row means, and the scene's part at that frequency is
/// taken as the smallest one consistent with both fits. That gives A whatever
/// the phase step for scenes without strong periodic structure; it is kept
/// between half and `FLICKER_MAX_GAIN` halves of the ratio amplitude, so bands
/// that barely moved, which look like scene structure, are not trusted further.
pub fn flicker_strength(prev_rows: &[f32], cur_rows: &[f32]) -> f32 {
    flicker_strength_strips(prev_rows, cur_rows, &[], &[])
}

/// Vertical strips of the picture whose row means `flicker_strength_strips` compares.
pub const FLICKER_STRIPS: usize = 6;

/// Mean of each row in each of `FLICKER_STRIPS` vertical strips, left to right.
pub fn strip_row_means(gray: &[u8], w: usize, h: usize) -> Vec<Vec<f32>> {
    if w < FLICKER_STRIPS {
        return Vec::new();
    }
    (0..FLICKER_STRIPS)
        .map(|s| {
            let (x0, x1) = (s * w / FLICKER_STRIPS, (s + 1) * w / FLICKER_STRIPS);
            gray.chunks_exact(w)
                .take(h)
                .map(|row| row[x0..x1].iter().map(|&v| v as u32).sum::<u32>() as f32 / (x1 - x0) as f32)
                .collect()
        })
        .collect()
}

/// `flicker_strength`, with the change between the samples read from vertical
/// strips of the picture (`strip_row_means`): for each row, the median of the
/// strips' changes. Flicker bands come from the shutter reading the sensor out
/// row by row, so they darken a whole row from edge to edge and every strip
/// shows them. Something moving in part of the picture (a passing car, a
/// sweeping light, a screen, the moving parts of a test pattern) changes the
/// means of the rows it crosses too, and a sinusoid fitted to whole rows takes
/// part of that for bands; in the median of the strips it only counts where it
/// covers most of the width. With fewer than 3 strips, whole rows are used.
pub fn flicker_strength_strips(prev_rows: &[f32], cur_rows: &[f32], prev_strips: &[Vec<f32>], cur_strips: &[Vec<f32>]) -> f32 {
    let n = prev_rows.len().min(cur_rows.len());
    if n < 32 {
        return 0.0;
    }
    let ok = |v: f32| v.is_finite() && (FLICKER_DARK..=FLICKER_BRIGHT).contains(&v);
    let pw: Vec<f64> = (0..n).map(|y| ok(prev_rows[y]) as u32 as f64).collect();
    let cw: Vec<f64> = (0..n).map(|y| ok(cur_rows[y]) as u32 as f64).collect();
    let lp_raw: Vec<f64> = (0..n).map(|y| (prev_rows[y].max(1.0) as f64).ln()).collect();
    let mut lc: Vec<f64> = (0..n).map(|y| (cur_rows[y].max(1.0) as f64).ln()).collect();
    let (mut lp, mut wts, shift) = align_rows(&lp_raw, &pw, &lc, &cw);
    if wts.iter().sum::<f64>() < (n as f64 / 3.0).max(32.0) {
        return 0.0;
    }
    for (c, w) in lc.iter_mut().zip(&mut wts) {
        if *w == 0.0 {
            *c = 0.0;
        }
    }
    detrend(&mut lp, &wts);
    detrend(&mut lc, &wts);
    let strips = prev_strips.len().min(cur_strips.len());
    let d: Vec<f64> = if strips >= 3 && prev_strips.iter().chain(cur_strips).all(|s| s.len() >= n) {
        // The same line-up as the whole rows, then each strip's change on its
        // own, detrended, and the median across the strips for each row.
        let changes: Vec<Vec<f64>> = (0..strips)
            .map(|k| {
                let (p, c) = (&prev_strips[k], &cur_strips[k]);
                let spw: Vec<f64> = (0..n).map(|y| (pw[y] > 0.0 && ok(p[y])) as u32 as f64).collect();
                let scw: Vec<f64> = (0..n).map(|y| (cw[y] > 0.0 && ok(c[y])) as u32 as f64).collect();
                let slp: Vec<f64> = (0..n).map(|y| (p[y].max(1.0) as f64).ln()).collect();
                let (mut sp, sw) = resample_rows(&slp, &spw, &scw, shift);
                let mut sc: Vec<f64> = (0..n).map(|y| if sw[y] > 0.0 { (c[y].max(1.0) as f64).ln() } else { 0.0 }).collect();
                detrend(&mut sp, &sw);
                detrend(&mut sc, &sw);
                (0..n).map(|y| if sw[y] > 0.0 && wts[y] > 0.0 { sc[y] - sp[y] } else { f64::NAN }).collect()
            })
            .collect();
        let mut column = Vec::with_capacity(strips);
        (0..n)
            .map(|y| {
                column.clear();
                column.extend(changes.iter().map(|c| c[y]).filter(|v| v.is_finite()));
                if column.len() * 2 <= strips {
                    // Too few strips usable in this row: leave it out.
                    wts[y] = 0.0;
                    return 0.0;
                }
                median_of(&mut column)
            })
            .collect()
    } else {
        lc.iter().zip(&lp).map(|(c, p)| c - p).collect()
    };
    let valid = wts.iter().sum::<f64>();
    if valid < (n as f64 / 3.0).max(32.0) {
        return 0.0;
    }

    // Strongest sinusoid in the ratio, on a grid refined by a parabola.
    let steps = ((FLICKER_MAX_CYCLES - FLICKER_MIN_CYCLES) / FLICKER_STEP).round() as usize;
    let energies: Vec<f64> = (0..=steps)
        .map(|k| sinusoid_fit(&d, &wts, FLICKER_MIN_CYCLES + k as f64 * FLICKER_STEP).2)
        .collect();
    let best = energies
        .iter()
        .enumerate()
        .fold((0usize, f64::MIN), |b, (k, &e)| if e > b.1 { (k, e) } else { b })
        .0;
    let mut cycles = FLICKER_MIN_CYCLES + best as f64 * FLICKER_STEP;
    if best > 0 && best < steps {
        let (l, c, r) = (energies[best - 1], energies[best], energies[best + 1]);
        let denom = l - 2.0 * c + r;
        if denom < 0.0 {
            cycles += (0.5 * (l - r) / denom).clamp(-0.5, 0.5) * FLICKER_STEP;
        }
    }
    let (da, db, explained) = sinusoid_fit(&d, &wts, cycles);
    let ratio_amp = (da * da + db * db).sqrt();
    if !ratio_amp.is_finite() || ratio_amp <= 0.0 {
        return 0.0;
    }
    // Noise in the ratio after removing the sinusoid, and the standard error of
    // a fitted coefficient pair for that noise.
    let energy: f64 = d.iter().zip(&wts).map(|(v, wt)| wt * v * v).sum();
    let resid = ((energy - explained).max(0.0) / (valid - 4.0).max(1.0)).sqrt();
    let std_err = resid * (2.0 / valid).sqrt();
    if ratio_amp < FLICKER_SIGNIFICANCE * std_err {
        return 0.0;
    }
    // Each sample's own sinusoid at that frequency: scene part plus bands.
    let (pa, pb, _) = sinusoid_fit(&lp, &wts, cycles);
    let (ca, cb, _) = sinusoid_fit(&lc, &wts, cycles);
    // The smallest scene part consistent with both lies along the change; what
    // is left of the mean across it belongs to the bands.
    let (ux, uy) = (da / ratio_amp, db / ratio_amp);
    let (mx, my) = (0.5 * (pa + ca), 0.5 * (pb + cb));
    let across = mx * -uy + my * ux;
    let half = 0.5 * ratio_amp;
    let amp = (across * across + half * half).sqrt().min(FLICKER_MAX_GAIN * half);
    amp as f32
}

// Rolling-shutter wobble.

/// Horizontal bands of rows measured on their own. Enough for about 3 bands
/// per cycle of a wobble with 8 cycles over the picture height, as a fast
/// vibration gives; with fewer bands such a wobble averages out inside each band.
const WOBBLE_BANDS: usize = 24;
/// Search radius around the global shift, in analysis pixels.
const WOBBLE_RADIUS: i32 = 8;
const SIDE: usize = 2 * WOBBLE_RADIUS as usize + 1;
/// Bands with a usable shift on both halves needed for a reading.
const WOBBLE_MIN_BANDS: usize = 12;
/// Rise in mean squared difference one pixel away from the best match that
/// a band needs to count as textured.
const WOBBLE_MIN_CURVATURE: f64 = 4.0;

/// Sum of squared differences of two rows of equal length. Written as 16-bit
/// differences multiplied into 32-bit sums, which the compiler turns into
/// pairwise multiply-add vector instructions even on baseline x86-64.
#[inline]
fn row_ssd(a: &[u8], b: &[u8]) -> u32 {
    let n = a.len().min(b.len());
    let (a, b) = (&a[..n], &b[..n]);
    let mut acc = 0u64;
    for (ca, cb) in a.chunks(4096).zip(b.chunks(4096)) {
        // 4096 squares of at most 65025 stay within i32.
        let part: i32 = ca
            .iter()
            .zip(cb)
            .map(|(&x, &y)| {
                let d = x as i16 - y as i16;
                d as i32 * d as i32
            })
            .sum();
        acc += part as u64;
    }
    acc.min(u32::MAX as u64) as u32
}

fn parabola_vertex(l: f64, c: f64, r: f64) -> f64 {
    let denom = l - 2.0 * c + r;
    if denom <= 1e-9 {
        0.0
    } else {
        (0.5 * (l - r) / denom).clamp(-0.5, 0.5)
    }
}

fn median_of(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = v.len() / 2;
    if v.is_empty() {
        0.0
    } else if v.len() % 2 == 0 {
        0.5 * (v[mid - 1] + v[mid])
    } else {
        v[mid]
    }
}

/// Residuals of (t, v) points from a robust straight line (Theil-Sen: the
/// median of the slopes between every pair of points, then the median
/// intercept). A few points far off the line, such as bands where something
/// moved, do not pull the line toward them.
fn robust_line_residuals(t: &[f64], v: &[f64]) -> Vec<f64> {
    let n = t.len();
    let mut slopes = Vec::with_capacity(n * n.saturating_sub(1) / 2);
    for i in 0..n {
        for j in i + 1..n {
            if (t[j] - t[i]).abs() > 1e-9 {
                slopes.push((v[j] - v[i]) / (t[j] - t[i]));
            }
        }
    }
    let b = median_of(&mut slopes);
    let mut icpt: Vec<f64> = t.iter().zip(v).map(|(x, y)| y - b * x).collect();
    let a = median_of(&mut icpt);
    t.iter().zip(v).map(|(x, y)| y - a - b * x).collect()
}

/// The shift of one block of `cur` (rows b0..b1, columns x0..x1) against
/// `prev`, searched around (cx, cy) by walking downhill through the sum of
/// squared differences, refined by a parabola. Err(true) when the best match
/// lies at the edge of the search, Err(false) when the block has too little texture.
#[allow(clippy::too_many_arguments)]
fn block_shift(prev: &Plane, cur: &Plane, b0: usize, b1: usize, x0: usize, x1: usize, cx: i32, cy: i32) -> Result<(f64, f64), bool> {
    let w = cur.w;
    let r = WOBBLE_RADIUS;
    let pixels = ((b1 - b0) * (x1 - x0)) as f64;
    let mut ssd = [u64::MAX; SIDE * SIDE];
    let eval = |i: usize, j: usize, ssd: &mut [u64; SIDE * SIDE]| {
        if ssd[j * SIDE + i] != u64::MAX {
            return;
        }
        let (sx, sy) = (cx + i as i32 - r, cy + j as i32 - r);
        let px0 = (x0 as i32 - sx) as usize;
        let mut acc = 0u64;
        for y in b0..b1 {
            let py = (y as i32 - sy) as usize;
            let c = &cur.px[y * w + x0..y * w + x1];
            let p = &prev.px[py * w + px0..py * w + px0 + (x1 - x0)];
            acc += row_ssd(c, p) as u64;
        }
        ssd[j * SIDE + i] = acc;
    };
    // Walk downhill from the global shift until no neighbour matches better.
    // Block shifts sit within a few pixels of the global one, so this visits
    // far fewer than all the candidates.
    let (mut bi, mut bj) = (r as usize, r as usize);
    loop {
        let mut best = (bi, bj);
        for j in bj - 1..=bj + 1 {
            for i in bi - 1..=bi + 1 {
                eval(i, j, &mut ssd);
                if ssd[j * SIDE + i] < ssd[best.1 * SIDE + best.0] {
                    best = (i, j);
                }
            }
        }
        if best == (bi, bj) {
            break;
        }
        (bi, bj) = best;
        if bi == 0 || bj == 0 || bi == SIDE - 1 || bj == SIDE - 1 {
            return Err(true);
        }
    }
    let at = |i: usize, j: usize| ssd[j * SIDE + i] as f64 / pixels;
    let c = at(bi, bj);
    let (l, rr, u, d) = (at(bi - 1, bj), at(bi + 1, bj), at(bi, bj - 1), at(bi, bj + 1));
    if (l + rr - 2.0 * c) * 0.5 < WOBBLE_MIN_CURVATURE || (u + d - 2.0 * c) * 0.5 < WOBBLE_MIN_CURVATURE {
        return Err(false);
    }
    Ok(((cx + bi as i32 - r) as f64 + parabola_vertex(l, c, rr), (cy + bj as i32 - r) as f64 + parabola_vertex(u, c, d)))
}

/// Rolling-shutter wobble ("jello"), typical of drone propeller vibration or a loose mount: within one frame, horizontal bands of rows are displaced sideways (and up and down) by different amounts in a wavy pattern. Given the previous and current analysis pictures (level 0 of `MotionFrame::levels`) and the global shift between them, returns how far the bands wander from a straight line, as an RMS in analysis pixels. 0 when the bands have too little texture to measure.
///
/// Shifts follow `metrics::shifted_difference`: content that moved right by
/// dx pixels has dx positive. The picture is cut into 24 bands of rows, and
/// each band into a left and a right half. Each half is matched on its own
/// (plus or minus 8 pixels around the global shift, sub-pixel by parabola),
/// and a straight line is fitted through the shifts of each side against band
/// position (a robust fit, so bands where something moved do not bend it): a
/// pan with a rolling shutter makes a straight skew, which is fine. What is
/// left from the line is the wobble. When most bands match
/// beyond the search, the reading is -1 (not measured).
///
/// A rolling shutter displaces whole rows, so real wobble leaves the same
/// residual on both sides, while a subject moving on one side, or matching
/// noise, leaves residuals that do not agree. The reading is therefore the
/// square root of the median product of the two sides' residuals rather than
/// of either side's own: noise averages out instead of adding up, which
/// matters for heavily compressed footage. Wobble runs through the whole
/// height, so the median stays near the mean square of the residual, while
/// something moving through a few bands, even across the whole width (water,
/// a passing train), leaves most bands at zero and does not count.
///
/// Measured on the full analysis picture with narrow bands, because fast
/// vibration makes many cycles over the height; on the half-size level with 8
/// bands, 8 cycles cancel out inside each band.
pub fn wobble(prev: &Plane, cur: &Plane, dx: f32, dy: f32) -> f32 {
    let (w, h) = (cur.w, cur.h);
    if prev.w != w || prev.h != h || w < 64 || h < 64 || prev.px.len() < w * h || cur.px.len() < w * h {
        return 0.0;
    }
    if !dx.is_finite() || !dy.is_finite() {
        return 0.0;
    }
    let (cx, cy) = (dx.round() as i32, dy.round() as i32);
    let r = WOBBLE_RADIUS;
    let (wi, hi) = (w as i32, h as i32);
    let margin = cx.abs() + r + 2;
    if 2 * margin + 32 > wi {
        return 0.0;
    }
    let (x0, x1) = (margin as usize, (wi - margin) as usize);
    let xm = (x0 + x1) / 2;
    // Rows of `cur` whose matching row of `prev` exists for every candidate.
    let ylo = (cy + r).max(0);
    let yhi = (hi + cy - r).min(hi);
    if yhi - ylo < (WOBBLE_BANDS as i32) * 4 {
        return 0.0;
    }
    let (ylo, yhi) = (ylo as usize, yhi as usize);
    let mut pos = Vec::with_capacity(WOBBLE_BANDS);
    let mut left = Vec::with_capacity(WOBBLE_BANDS);
    let mut right = Vec::with_capacity(WOBBLE_BANDS);
    // Bands whose match lay beyond the search.
    let mut beyond = 0usize;
    for band in 0..WOBBLE_BANDS {
        let b0 = ylo + band * (yhi - ylo) / WOBBLE_BANDS;
        let b1 = ylo + (band + 1) * (yhi - ylo) / WOBBLE_BANDS;
        let (l, rt) = match (block_shift(prev, cur, b0, b1, x0, xm, cx, cy), block_shift(prev, cur, b0, b1, xm, x1, cx, cy)) {
            (Ok(l), Ok(rt)) => (l, rt),
            (l, rt) => {
                beyond += (l == Err(true) || rt == Err(true)) as usize;
                continue;
            }
        };
        pos.push((b0 + b1) as f64 * 0.5 / h as f64);
        left.push(l);
        right.push(rt);
    }
    if pos.len() < WOBBLE_MIN_BANDS {
        // Mostly out of reach: more movement than the search covers, which
        // says nothing either way, so the sample is left unmeasured.
        return if beyond * 2 >= WOBBLE_BANDS { -1.0 } else { 0.0 };
    }
    // Residuals of each side from its own straight lines, and their product per band.
    let side = |s: &[(f64, f64)]| {
        let x: Vec<f64> = s.iter().map(|v| v.0).collect();
        let y: Vec<f64> = s.iter().map(|v| v.1).collect();
        (robust_line_residuals(&pos, &x), robust_line_residuals(&pos, &y))
    };
    let ((lx, ly), (rx, ry)) = (side(&left), side(&right));
    let mut p: Vec<f64> = (0..pos.len()).map(|k| lx[k] * rx[k] + ly[k] * ry[k]).collect();
    let median = median_of(&mut p);
    median.max(0.0).sqrt() as f32
}

// White balance cue.

/// Luma range of pixels used for the white balance cue.
const NEUTRAL_LUMA: (u32, u32) = (40, 220);
/// Largest chroma distance from neutral, in codes, of a near-neutral pixel.
const NEUTRAL_CHROMA: i32 = 30;
/// Share of chroma samples that must be near-neutral for a reading.
const NEUTRAL_MIN_SHARE: f32 = 0.02;
/// Smallest luma step, in levels, between a 2 by 2 block and one of its four
/// neighbours for the block to count as textured rather than smooth.
const NEUTRAL_DETAIL: u32 = 3;

/// White balance cue from a frame in full-range YUV 4:2:0 (yuvj420p): y is w*h, u and v are (w/2)*(h/2), all 0..255 with 128 neutral chroma. Average chroma of near-neutral, textured pixels (luma between about 40 and 220, chroma distance from neutral under about 30 codes, a visible luma step to a neighbouring block). Returns (cb - 128, cr - 128) means, or None when fewer than 2% of pixels qualify.
///
/// Each chroma sample is paired with the mean of the 2 by 2 luma block it
/// covers. Odd sizes with rounded-up chroma planes are accepted too.
///
/// Smooth areas are left out. Overcast or hazy sky, fog and haze are near
/// neutral but take their colour from the light and the air, not from the
/// camera's setting: a clip that is mostly grey sky reads cooler than the
/// forest beside it with the same white balance. They are also smooth at this
/// size, where rock, bark, sand, gravel and concrete show texture, so leaving
/// out blocks within `NEUTRAL_DETAIL` levels of all four neighbours drops the
/// sky without looking at its colour (which would bias every reading). A frame
/// that is all sky then has too few textured neutral pixels and gives no reading.
pub fn neutral_chroma(y: &[u8], u: &[u8], v: &[u8], w: usize, h: usize) -> Option<(f32, f32)> {
    let (cw, ch) = if u.len() >= w.div_ceil(2) * h.div_ceil(2) && v.len() >= w.div_ceil(2) * h.div_ceil(2) {
        (w.div_ceil(2), h.div_ceil(2))
    } else {
        (w / 2, h / 2)
    };
    if cw == 0 || ch == 0 || y.len() < w * h || u.len() < cw * ch || v.len() < cw * ch {
        return None;
    }
    // Sum of each 2 by 2 luma block, one per chroma sample.
    let mut blocks = vec![0u32; cw * ch];
    for j in 0..ch {
        let y0 = (2 * j).min(h - 1);
        let y1 = (2 * j + 1).min(h - 1);
        let (ra, rb) = (&y[y0 * w..y0 * w + w], &y[y1 * w..y1 * w + w]);
        for (i, b) in blocks[j * cw..(j + 1) * cw].iter_mut().enumerate() {
            let x0 = (2 * i).min(w - 1);
            let x1 = (2 * i + 1).min(w - 1);
            *b = ra[x0] as u32 + ra[x1] as u32 + rb[x0] as u32 + rb[x1] as u32;
        }
    }
    let step = 4 * NEUTRAL_DETAIL;
    let (mut n, mut su, mut sv) = (0u64, 0i64, 0i64);
    for j in 0..ch {
        let (ur, vr) = (&u[j * cw..j * cw + cw], &v[j * cw..j * cw + cw]);
        for i in 0..cw {
            let luma4 = blocks[j * cw + i];
            if luma4 < 4 * NEUTRAL_LUMA.0 || luma4 > 4 * NEUTRAL_LUMA.1 {
                continue;
            }
            let (cb, cr) = (ur[i] as i32 - 128, vr[i] as i32 - 128);
            if cb * cb + cr * cr >= NEUTRAL_CHROMA * NEUTRAL_CHROMA {
                continue;
            }
            let differs = |k: usize| blocks[k].abs_diff(luma4) >= step;
            let textured = (i > 0 && differs(j * cw + i - 1))
                || (i + 1 < cw && differs(j * cw + i + 1))
                || (j > 0 && differs((j - 1) * cw + i))
                || (j + 1 < ch && differs((j + 1) * cw + i));
            if !textured {
                continue;
            }
            n += 1;
            su += cb as i64;
            sv += cr as i64;
        }
    }
    if n == 0 || (n as f32) < NEUTRAL_MIN_SHARE * (cw * ch) as f32 {
        return None;
    }
    Some((su as f32 / n as f32, sv as f32 / n as f32))
}

/// Plain reading of a chroma difference: warm/cool axis (cr - cb) / 2 and green/magenta axis -(cr + cb) / 2, as (temperature, tint): positive temperature is warmer, positive tint is greener.
pub fn temperature_tint(cb: f32, cr: f32) -> (f32, f32) {
    ((cr - cb) * 0.5, -(cr + cb) * 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{estimate_shift, motion_frame};
    use std::process::Command;
    use std::time::Instant;

    const W: usize = 480;
    const H: usize = 270;

    struct Rng(u32);
    impl Rng {
        fn unit(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            ((self.0 >> 16) & 0x7fff) as f32 / 32767.0
        }
        /// Roughly normal, unit standard deviation.
        fn normal(&mut self) -> f32 {
            (self.unit() + self.unit() + self.unit() + self.unit() - 2.0) / 0.577
        }
    }

    /// A smooth random field that can be sampled anywhere, for sub-pixel shifts.
    struct Field {
        gw: usize,
        gh: usize,
        g: Vec<f32>,
    }

    impl Field {
        fn new(gw: usize, gh: usize, seed: u32) -> Field {
            let mut rng = Rng(seed);
            let raw: Vec<f32> = (0..gw * gh).map(|_| rng.unit() * 255.0).collect();
            // 3x3 box blur so the texture is a couple of pixels wide.
            let mut g = vec![0f32; gw * gh];
            for y in 0..gh {
                for x in 0..gw {
                    let mut acc = 0.0;
                    for dy in -1i32..=1 {
                        for dx in -1i32..=1 {
                            let xx = (x as i32 + dx).clamp(0, gw as i32 - 1) as usize;
                            let yy = (y as i32 + dy).clamp(0, gh as i32 - 1) as usize;
                            acc += raw[yy * gw + xx];
                        }
                    }
                    g[y * gw + x] = acc / 9.0;
                }
            }
            Field { gw, gh, g }
        }

        /// Bilinear sample, with a margin so shifted pictures stay inside.
        fn at(&self, x: f32, y: f32) -> f32 {
            let x = (x + 16.0).clamp(0.0, self.gw as f32 - 2.0);
            let y = (y + 16.0).clamp(0.0, self.gh as f32 - 2.0);
            let (xi, yi) = (x as usize, y as usize);
            let (fx, fy) = (x - xi as f32, y - yi as f32);
            let g = |xx: usize, yy: usize| self.g[yy * self.gw + xx];
            let top = g(xi, yi) * (1.0 - fx) + g(xi + 1, yi) * fx;
            let bot = g(xi, yi + 1) * (1.0 - fx) + g(xi + 1, yi + 1) * fx;
            top * (1.0 - fy) + bot * fy
        }
    }

    fn noisy(px: &[f32], sigma: f32, seed: u32) -> Vec<u8> {
        let mut rng = Rng(seed);
        px.iter().map(|&v| (v + rng.normal() * sigma).round().clamp(0.0, 255.0) as u8).collect()
    }

    // Horizon roll.

    /// Renders a scene with ffmpeg at 1920x1080, rotates it clockwise by
    /// `degrees`, crops away the corners and downsizes with area averaging to
    /// 480x270 grayscale, like the analysis pictures.
    fn rotated_scene(lum: &str, degrees: f32) -> Vec<u8> {
        let src = format!("nullsrc=s=1920x1080:d=1,format=gray,geq=lum='{lum}'");
        let vf = format!("rotate={degrees}*PI/180:fillcolor=gray,crop=1600:900,scale={W}:{H}:flags=area,format=gray");
        let out = Command::new("ffmpeg")
            .args(["-hide_banner", "-v", "error", "-f", "lavfi", "-i", &src, "-vf", &vf])
            .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "gray", "-"])
            .output()
            .expect("ffmpeg must be installed to run the picture tests");
        assert!(out.status.success(), "ffmpeg failed: {}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(out.stdout.len(), W * H);
        out.stdout
    }

    /// A grid of straight lines.
    const LINES: &str = "clip(70+140*(lt(mod(X,113),7)+lt(mod(Y,97),7)),0,255)";
    /// Sky over a facade of windows, with grain.
    const BUILDINGS: &str =
        "if(lt(Y,380),200-0.04*Y,if(lt(mod(X,150),104)*lt(mod(Y-380,120),72),175,70)+18*(random(1)-0.5))";
    /// Sky over gently textured ground: one long horizon line.
    const HORIZON: &str =
        "if(lt(Y,500),205-0.03*Y,95+14*sin(X*0.031+Y*0.017)*cos(Y*0.043-X*0.012)+10*(random(1)-0.5))";

    const ANGLES: [f32; 9] = [-4.0, -2.0, -1.0, -0.5, 0.0, 0.5, 1.0, 2.0, 4.0];
    /// A scene rotated clockwise by `deg` and rendered with exact area
    /// coverage (8 by 8 samples per pixel): the sharpest edges a downsized
    /// picture can have. Kind 0 is a single horizon, 1 a grid of thin lines,
    /// 2 a grid of windows.
    fn sharp_scene(deg: f32, kind: u32) -> Vec<u8> {
        let (s, c) = deg.to_radians().sin_cos();
        let mut out = vec![0u8; W * H];
        for y in 0..H {
            for x in 0..W {
                let mut acc = 0f32;
                for sy in 0..8 {
                    for sx in 0..8 {
                        let px = x as f32 + (sx as f32 + 0.5) / 8.0 - W as f32 / 2.0;
                        let py = y as f32 + (sy as f32 + 0.5) / 8.0 - H as f32 / 2.0;
                        // Scene coordinates of this point before the rotation.
                        let u = px * c + py * s;
                        let v = -px * s + py * c;
                        acc += match kind {
                            0 => {
                                if v < 10.3 {
                                    190.0
                                } else {
                                    90.0
                                }
                            }
                            1 => {
                                if u.rem_euclid(37.0) < 4.0 || v.rem_euclid(31.0) < 4.0 {
                                    200.0
                                } else {
                                    70.0
                                }
                            }
                            _ => {
                                if u.rem_euclid(45.0) < 30.0 && v.rem_euclid(36.0) < 22.0 {
                                    175.0
                                } else {
                                    70.0
                                }
                            }
                        };
                    }
                }
                out[y * W + x] = (acc / 64.0).round() as u8;
            }
        }
        out
    }

    #[test]
    fn picture_roll_of_sharp_edges() {
        let mut worst = 0f32;
        for a in [-4.0f32, -3.0, -2.1, -2.0, -1.0, -0.5, -0.2, 0.0, 0.2, 0.5, 1.0, 2.0, 3.0, 4.0] {
            for kind in 0..3 {
                let (deg, conf) = horizon_roll(&sharp_scene(a, kind), W, H);
                println!("picture roll sharp scene {kind}: truth {a:+.1} read {deg:+.3} confidence {conf:.2}");
                assert!((deg - a).abs() <= 0.15, "kind {kind} at {a}: read {deg}");
                assert!(conf >= 0.6, "kind {kind} at {a}: confidence {conf}");
                worst = worst.max((deg - a).abs());
            }
        }
        println!("picture roll sharp scenes: worst error {worst:.3}");
    }

    fn roll_sweep(name: &str, lum: &str, min_conf: f32) -> (f32, f32) {
        let (mut worst, mut lowest) = (0f32, 1f32);
        for &a in &ANGLES {
            let img = rotated_scene(lum, a);
            let (deg, conf) = horizon_roll(&img, W, H);
            println!("picture roll {name}: truth {a:+.1} read {deg:+.3} error {:+.3} confidence {conf:.2}", deg - a);
            assert!((deg - a).abs() <= 0.3, "{name} at {a}: read {deg}");
            if a != 0.0 {
                assert_eq!(deg.signum(), a.signum(), "{name} at {a}: sign of {deg}");
            }
            assert!(conf >= min_conf, "{name} at {a}: confidence {conf}");
            worst = worst.max((deg - a).abs());
            lowest = lowest.min(conf);
        }
        (worst, lowest)
    }

    #[test]
    fn picture_roll_of_line_grid() {
        let (err, conf) = roll_sweep("lines", LINES, 0.8);
        println!("picture roll lines: worst error {err:.3} lowest confidence {conf:.2}");
    }

    #[test]
    fn picture_roll_of_buildings() {
        let (err, conf) = roll_sweep("buildings", BUILDINGS, 0.6);
        println!("picture roll buildings: worst error {err:.3} lowest confidence {conf:.2}");
    }

    #[test]
    fn picture_roll_of_horizon_over_texture() {
        let (err, conf) = roll_sweep("horizon", HORIZON, 0.35);
        println!("picture roll horizon: worst error {err:.3} lowest confidence {conf:.2}");
    }

    #[test]
    fn picture_roll_is_unsure_without_lines() {
        let mut rng = Rng(7);
        // Fine random noise, like foliage or water at this size.
        let noise: Vec<u8> = (0..W * H).map(|_| (40.0 + rng.unit() * 180.0) as u8).collect();
        // Smoothed random texture.
        let field = Field::new(W + 40, H + 40, 11);
        let tex: Vec<u8> = (0..W * H).map(|k| field.at((k % W) as f32, (k / W) as f32) as u8).collect();
        // A smooth diagonal gradient and a vertical sky gradient with grain.
        let ramp: Vec<u8> = (0..W * H).map(|k| (((k % W) + (k / W)) as f32 * 255.0 / (W + H) as f32) as u8).collect();
        let sky: Vec<f32> = (0..W * H).map(|k| 120.0 + 90.0 * (k / W) as f32 / H as f32).collect();
        let sky = noisy(&sky, 2.0, 3);
        let flat = vec![128u8; W * H];
        for (name, img) in [("noise", &noise), ("texture", &tex), ("ramp", &ramp), ("sky", &sky), ("flat", &flat)] {
            let (deg, conf) = horizon_roll(img, W, H);
            println!("picture roll {name}: read {deg:+.2} confidence {conf:.2}");
            assert!(conf < 0.35, "{name}: confidence {conf}");
        }
    }

    // Light flicker.

    /// A still scene with flicker bands of `amp` at `cycles` and `phase`, plus sensor noise.
    fn banded(base: &[f32], amp: f32, cycles: f32, phase: f32, seed: u32) -> Vec<u8> {
        let px: Vec<f32> = base
            .iter()
            .enumerate()
            .map(|(k, &v)| {
                let t = ((k / W) as f32 + 0.5) / H as f32;
                v * (1.0 + amp * (2.0 * std::f32::consts::PI * cycles * t + phase).sin())
            })
            .collect();
        noisy(&px, 1.5, seed)
    }

    /// A textured scene, optionally with static horizontal stripes (blinds).
    fn scene(blinds: bool) -> Vec<f32> {
        let field = Field::new(W + 40, H + 40, 21);
        (0..W * H)
            .map(|k| {
                let (x, y) = ((k % W) as f32, (k / W) as f32);
                let v = 50.0 + 0.55 * field.at(x, y) + 30.0 * (y / H as f32);
                if blinds && x > 120.0 && x < 400.0 {
                    // Slats: bright and dark bands 54 rows apart, 5 cycles over the height.
                    if (y as usize / 27).is_multiple_of(2) {
                        v * 1.25
                    } else {
                        v * 0.7
                    }
                } else {
                    v
                }
            })
            .collect()
    }

    fn flicker_between(a: &[u8], b: &[u8]) -> f32 {
        flicker_strength(&row_means(a, W, H), &row_means(b, W, H))
    }

    #[test]
    fn picture_flicker_is_measured_whatever_the_phase_step() {
        let base = scene(false);
        let mut worst = 0f32;
        for &amp in &[0.02f32, 0.05] {
            for &cycles in &[1.3f32, 3.0, 8.5] {
                for &step in &[60f32, 90.0, 180.0] {
                    let a = banded(&base, amp, cycles, 0.4, 1);
                    let b = banded(&base, amp, cycles, 0.4 + step.to_radians(), 2);
                    let got = flicker_between(&a, &b);
                    let err = got / amp - 1.0;
                    println!(
                        "picture flicker {amp} at {cycles} cycles, phase step {step}: read {got:.4} ({:+.0}%)",
                        err * 100.0
                    );
                    assert!(err.abs() <= 0.3, "amp {amp} cycles {cycles} step {step}: read {got}");
                    worst = worst.max(err.abs());
                }
            }
        }
        println!("picture flicker: worst relative error {:.0}%", worst * 100.0);
    }

    #[test]
    fn picture_flicker_is_zero_for_a_still_scene_and_blinds() {
        for blinds in [false, true] {
            let base = scene(blinds);
            let a = banded(&base, 0.0, 1.0, 0.0, 5);
            let b = banded(&base, 0.0, 1.0, 0.0, 6);
            let got = flicker_between(&a, &b);
            println!("picture flicker without bands (blinds {blinds}): {got:.5}");
            assert!(got < 0.005, "blinds {blinds}: {got}");
        }
        // A slight exposure change between samples is not flicker either.
        let base = scene(true);
        let a = banded(&base, 0.0, 1.0, 0.0, 7);
        let brighter: Vec<f32> = base.iter().map(|v| v * 1.04).collect();
        let b = banded(&brighter, 0.0, 1.0, 0.0, 8);
        let got = flicker_between(&a, &b);
        println!("picture flicker with an exposure change: {got:.5}");
        assert!(got < 0.005, "{got}");
        // Bands over blinds are still measured.
        let a = banded(&base, 0.03, 4.0, 0.0, 9);
        let b = banded(&base, 0.03, 4.0, 1.6, 10);
        let got = flicker_between(&a, &b);
        println!("picture flicker 0.03 over blinds: {got:.4}");
        assert!((got / 0.03 - 1.0).abs() <= 0.3, "{got}");
    }

    /// The scene moved down by `dy` rows.
    fn moved_down(base: &[f32], dy: f32) -> Vec<f32> {
        (0..W * H)
            .map(|k| {
                let (x, y) = (k % W, k / W);
                let src = (y as f32 - dy).clamp(0.0, (H - 1) as f32);
                let y0 = (src as usize).min(H - 2);
                let f = src - y0 as f32;
                base[y0 * W + x] * (1.0 - f) + base[(y0 + 1) * W + x] * f
            })
            .collect()
    }

    #[test]
    fn picture_flicker_is_not_fooled_by_a_slight_vertical_move() {
        // Blinds that move by a fraction of a row change the row means in a
        // periodic way; lining the rows up first keeps that from reading as flicker.
        for blinds in [false, true] {
            let base = scene(blinds);
            for dy in [0.3f32, 0.5, 1.0, -0.7] {
                let a = banded(&base, 0.0, 1.0, 0.0, 1);
                let b = banded(&moved_down(&base, dy), 0.0, 1.0, 0.0, 2);
                let got = flicker_between(&a, &b);
                println!("picture flicker without bands, blinds {blinds}, moved {dy} rows: {got:.4}");
                assert!(got < 0.005, "blinds {blinds} dy {dy}: {got}");
            }
        }
        // Real bands still count when the picture moved a little too.
        let base = scene(true);
        let a = banded(&base, 0.03, 6.0, 0.0, 3);
        let b = banded(&moved_down(&base, 0.4), 0.03, 6.0, 1.5, 4);
        let got = flicker_between(&a, &b);
        println!("picture flicker 0.03 over blinds moved 0.4 rows: {got:.4}");
        assert!((got / 0.03 - 1.0).abs() <= 0.3, "{got}");
        // And over a plain wall, where moving bands look most like a move.
        let plain = vec![120f32; W * H];
        let a = banded(&plain, 0.03, 4.0, 0.0, 5);
        let b = banded(&plain, 0.03, 4.0, 1.6, 6);
        let got = flicker_between(&a, &b);
        println!("picture flicker 0.03 over a plain wall: {got:.4}");
        assert!((got / 0.03 - 1.0).abs() <= 0.3, "{got}");
    }

    #[test]
    fn picture_flicker_ignores_dark_rows_and_short_input() {
        assert_eq!(flicker_strength(&[100.0; 10], &[100.0; 10]), 0.0);
        let dark = vec![3.0f32; H];
        assert_eq!(flicker_strength(&dark, &dark), 0.0);
        // Black letterbox bars around banded rows.
        let base = scene(false);
        let mut a = banded(&base, 0.04, 5.0, 0.0, 11);
        let mut b = banded(&base, 0.04, 5.0, 2.0, 12);
        for k in (0..W * 30).chain(W * (H - 30)..W * H) {
            a[k] = 0;
            b[k] = 0;
        }
        let got = flicker_between(&a, &b);
        println!("picture flicker 0.04 with black bars: {got:.4}");
        assert!((got / 0.04 - 1.0).abs() <= 0.3, "{got}");
    }

    // Rolling-shutter wobble.

    /// A picture whose content moved by (dx + row_shift(y), dy) from the field.
    fn moved(field: &Field, dx: f32, dy: f32, row_shift: &dyn Fn(f32) -> f32, noise: f32, seed: u32) -> Plane {
        let px: Vec<f32> = (0..W * H)
            .map(|k| {
                let (x, y) = ((k % W) as f32, (k / W) as f32);
                field.at(x - dx - row_shift(y), y - dy)
            })
            .collect();
        Plane { w: W, h: H, px: noisy(&px, noise, seed) }
    }

    /// A sine wobble of `amp` pixels and `cycles` over the height, at `phase` turns.
    fn sine_rows(amp: f32, cycles: f32, phase: f32) -> impl Fn(f32) -> f32 {
        move |y: f32| amp * (2.0 * std::f32::consts::PI * (cycles * y / H as f32 + phase)).sin()
    }

    #[test]
    fn picture_wobble_separates_jello_from_pans_and_skew() {
        let field = Field::new(W + 40, H + 40, 31);
        let none = |_: f32| 0.0;
        let skew = |y: f32| 0.02 * (y - H as f32 / 2.0);
        let slow = sine_rows(1.5, 1.5, 0.0);
        // Fast vibration: 8 cycles over the height, as the feature test clip
        // made with a 6 pixel wobble at 1280 wide (2.25 analysis pixels), the
        // two samples a third of a cycle apart.
        let fast_prev = sine_rows(2.25, 8.0, 0.0);
        let fast = sine_rows(2.25, 8.0, 0.33);
        type Case<'a> = (&'a str, &'a dyn Fn(f32) -> f32, f32, f32, &'a dyn Fn(f32) -> f32);
        let cases: [Case; 7] = [
            ("still", &none, 0.0, 0.0, &none),
            ("shift", &none, 2.3, -1.4, &none),
            ("skew", &none, 1.6, 0.7, &skew),
            ("jello", &none, 0.0, 0.0, &slow),
            ("jello while panning", &none, 3.4, 0.6, &slow),
            ("fast jello", &fast_prev, 0.0, 0.0, &fast),
            ("fast jello while panning", &fast_prev, -2.6, 0.3, &fast),
        ];
        let mut readings = Vec::new();
        for (name, before, dx, dy, f) in cases {
            // Noise of 4 levels, like heavily compressed footage.
            let prev = moved(&field, 0.0, 0.0, before, 4.0, 1);
            let cur = moved(&field, dx, dy, f, 4.0, 2);
            let got = wobble(&prev, &cur, dx, dy);
            println!("picture wobble {name}: {got:.3} px");
            readings.push(got);
        }
        for (k, name) in ["still", "shift", "skew"].iter().enumerate() {
            assert!(readings[k] < 0.1, "{name}: {}", readings[k]);
        }
        for (k, name) in ["jello", "jello while panning", "fast jello", "fast jello while panning"].iter().enumerate() {
            assert!(readings[k + 3] > 0.5, "{name}: {}", readings[k + 3]);
        }
    }

    #[test]
    fn picture_wobble_through_the_motion_pyramid() {
        // Full size frames with jello, measured as the app does, with the
        // global shift from `estimate_shift`.
        let field = Field::new(W + 40, H + 40, 41);
        let full = |dx: f32, f: &dyn Fn(f32) -> f32, seed: u32| {
            let p = moved(&field, dx, 0.0, f, 1.0, seed);
            motion_frame(&p.px, W, H)
        };
        let prev = full(0.0, &|_| 0.0, 1);
        let jello = sine_rows(3.0, 2.0, 0.0);
        let pan = full(4.0, &|_| 0.0, 2);
        let wobbly = full(4.0, &jello, 3);
        for (name, cur, min, max) in [("pan", &pan, 0.0, 0.1), ("jello", &wobbly, 0.5, 9.0)] {
            let (dx, dy, _) = estimate_shift(&prev, cur);
            let got = wobble(&prev.levels[0], &cur.levels[0], dx, dy);
            println!("picture wobble pyramid {name}: shift ({dx:.2}, {dy:.2}) wobble {got:.3} px");
            assert!(got >= min && got <= max, "{name}: {got}");
        }
    }

    #[test]
    fn picture_wobble_ignores_moving_subjects_and_flat_pictures() {
        let field = Field::new(W + 40, H + 40, 51);
        let other = Field::new(W + 40, H + 40, 52);
        // A subject with its own texture over a still scene, `size` pixels
        // across, and a wide strip (water, a passing train) across the whole width.
        let scene = |subject_x: f32, size: (f32, f32), strip: f32, seed: u32| {
            let px: Vec<f32> = (0..W * H)
                .map(|k| {
                    let (x, y) = ((k % W) as f32, (k / W) as f32);
                    let (sx, sy) = (x - subject_x, y - 120.0);
                    if (0.0..size.0).contains(&sx) && (0.0..size.1).contains(&sy) {
                        other.at(sx, sy) * 0.5 + 120.0
                    } else if (190.0..230.0).contains(&y) {
                        other.at(x - strip, y)
                    } else {
                        field.at(x, y)
                    }
                })
                .collect();
            Plane { w: W, h: H, px: noisy(&px, 2.0, seed) }
        };
        for (name, a, b) in [
            ("small subject", scene(180.0, (60.0, 44.0), 0.0, 1), scene(192.0, (60.0, 44.0), 0.0, 2)),
            ("large subject on one side", scene(20.0, (200.0, 120.0), 0.0, 3), scene(26.0, (200.0, 120.0), 0.0, 4)),
            ("strip moving across the width", scene(0.0, (0.0, 0.0), 0.0, 5), scene(0.0, (0.0, 0.0), 3.0, 6)),
        ] {
            let got = wobble(&a, &b, 0.0, 0.0);
            println!("picture wobble with {name}: {got:.3} px");
            assert!(got < 0.3, "{name}: {got}");
        }
        let flat = Plane { w: W, h: H, px: vec![100; W * H] };
        assert_eq!(wobble(&flat, &flat, 0.0, 0.0), 0.0);
        let small = Plane { w: 8, h: 8, px: vec![0; 64] };
        assert_eq!(wobble(&small, &small, 0.0, 0.0), 0.0);
    }

    #[test]
    fn picture_wobble_beyond_the_search_is_not_read_as_none() {
        // Bands displaced by far more than the search reaches: either not
        // measured, or read from the bands still in reach, never as no wobble.
        let field = Field::new(W + 80, H + 80, 57);
        let prev = moved(&field, 0.0, 0.0, &|_| 0.0, 1.0, 1);
        let cur = moved(&field, 0.0, 0.0, &sine_rows(14.0, 3.0, 0.0), 1.0, 2);
        let got = wobble(&prev, &cur, 0.0, 0.0);
        println!("picture wobble far beyond the search: {got:.3}");
        assert!(got < 0.0 || got > 0.7, "{got}");
    }

    // White balance cue.

    /// A grey card scene of varied brightness with a chroma cast, and a share
    /// of strongly coloured pixels on the left.
    fn yuv(cast_cb: f32, cast_cr: f32, saturated: f32) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let field = Field::new(W + 40, H + 40, 61);
        let y: Vec<u8> = (0..W * H).map(|k| (30.0 + 0.8 * field.at((k % W) as f32, (k / W) as f32)) as u8).collect();
        let (cw, ch) = (W / 2, H / 2);
        let mut rng = Rng(62);
        let mut u = vec![0u8; cw * ch];
        let mut v = vec![0u8; cw * ch];
        for k in 0..cw * ch {
            let colourful = ((k % cw) as f32) < saturated * cw as f32;
            let (cb, cr) = if colourful { (-70.0, 60.0) } else { (cast_cb, cast_cr) };
            u[k] = (128.0 + cb + rng.normal()).round().clamp(0.0, 255.0) as u8;
            v[k] = (128.0 + cr + rng.normal()).round().clamp(0.0, 255.0) as u8;
        }
        (y, u, v)
    }

    #[test]
    fn picture_neutral_chroma_reads_casts() {
        let reading = |cb: f32, cr: f32, sat: f32| {
            let (y, u, v) = yuv(cb, cr, sat);
            neutral_chroma(&y, &u, &v, W, H)
        };
        let (cb, cr) = reading(0.0, 0.0, 0.3).unwrap();
        let (t, g) = temperature_tint(cb, cr);
        println!("picture chroma neutral: cb {cb:.2} cr {cr:.2} temperature {t:.2} tint {g:.2}");
        assert!(t.abs() < 1.0 && g.abs() < 1.0);
        let (cb, cr) = reading(-8.0, 10.0, 0.3).unwrap();
        let (t, g) = temperature_tint(cb, cr);
        println!("picture chroma warm: cb {cb:.2} cr {cr:.2} temperature {t:.2} tint {g:.2}");
        assert!(t > 7.0 && g.abs() < 2.0);
        let (cb, cr) = reading(-8.0, -9.0, 0.3).unwrap();
        let (t, g) = temperature_tint(cb, cr);
        println!("picture chroma green: cb {cb:.2} cr {cr:.2} temperature {t:.2} tint {g:.2}");
        assert!(g > 7.0 && t.abs() < 2.0);
        let (cb, cr) = reading(9.0, -6.0, 0.0).unwrap();
        assert!(temperature_tint(cb, cr).0 < -6.0, "cool cast");
        assert_eq!(reading(0.0, 0.0, 1.0), None, "fully saturated scene");
        // Too dark or too bright counts as not neutral either.
        let (_, u, v) = yuv(0.0, 0.0, 0.0);
        assert_eq!(neutral_chroma(&vec![10u8; W * H], &u, &v, W, H), None);
        assert_eq!(neutral_chroma(&vec![250u8; W * H], &u, &v, W, H), None);
        assert_eq!(temperature_tint(-4.0, 6.0), (5.0, -1.0));
    }

    /// A forest scene: textured ground of grey bark and rock (chroma `cast`)
    /// and green leaves, under smooth bluish overcast sky (cb +10, cr -4, the
    /// light, not the camera) that fills the top `sky` share of the frame.
    fn sky_over_forest(sky: f32, cast: (f32, f32), noise: f32) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let field = Field::new(W + 40, H + 40, 63);
        let horizon = sky * H as f32;
        let mut rng = Rng(64);
        let y: Vec<u8> = (0..W * H)
            .map(|k| {
                let (x, row) = ((k % W) as f32, (k / W) as f32);
                let v = if row < horizon { 150.0 + 35.0 * row / H as f32 + 4.0 * (x / 70.0).sin() } else { 40.0 + 0.6 * field.at(x, row) };
                (v + noise * rng.normal()).round().clamp(0.0, 255.0) as u8
            })
            .collect();
        let (cw, ch) = (W / 2, H / 2);
        let mut u = vec![0u8; cw * ch];
        let mut v = vec![0u8; cw * ch];
        for k in 0..cw * ch {
            let (x, row) = ((2 * (k % cw)) as f32, (2 * (k / cw)) as f32);
            let (cb, cr) = if row < horizon {
                (10.0, -4.0)
            } else if field.at(x + 7.0, row + 3.0) > 128.0 {
                (-35.0, -30.0)
            } else {
                cast
            };
            u[k] = (128.0 + cb + 0.7 * rng.normal()).round().clamp(0.0, 255.0) as u8;
            v[k] = (128.0 + cr + 0.7 * rng.normal()).round().clamp(0.0, 255.0) as u8;
        }
        (y, u, v)
    }

    #[test]
    fn picture_neutral_chroma_leaves_out_grey_sky() {
        // The same camera setting with more or less sky in the frame reads the
        // same; before, sky was half the neutral area at 60% sky, and the clip
        // read about 4 codes cooler than its neighbours.
        let read = |sky: f32, cast: (f32, f32)| {
            let (y, u, v) = sky_over_forest(sky, cast, 0.8);
            neutral_chroma(&y, &u, &v, W, H)
        };
        let (cb0, cr0) = read(0.0, (0.0, 0.0)).unwrap();
        let t0 = temperature_tint(cb0, cr0).0;
        for sky in [0.3f32, 0.6, 0.85] {
            let (cb, cr) = read(sky, (0.0, 0.0)).unwrap();
            let t = temperature_tint(cb, cr).0;
            println!("picture chroma with {:.0}% sky: cb {cb:.2} cr {cr:.2} temperature {t:+.2} (no sky {t0:+.2})", sky * 100.0);
            assert!((t - t0).abs() < 1.0, "sky {sky}: {t} against {t0}");
        }
        // A real setting change still shows with sky in the frame.
        let (cb, cr) = read(0.6, (8.0, -6.0)).unwrap();
        let t = temperature_tint(cb, cr).0;
        println!("picture chroma with 60% sky and a cool setting: temperature {t:+.2}");
        assert!(t - t0 < -5.0, "{t}");
        // A frame of nothing but sky gives no reading.
        let (y, u, v) = sky_over_forest(1.0, (0.0, 0.0), 0.8);
        assert_eq!(neutral_chroma(&y, &u, &v, W, H), None);
    }

    #[test]
    fn picture_timing_per_frame() {
        let field = Field::new(W + 40, H + 40, 71);
        let frame = |dx: f32, seed: u32| {
            let px: Vec<f32> = (0..W * H).map(|k| field.at((k % W) as f32 - dx, (k / W) as f32)).collect();
            noisy(&px, 2.0, seed)
        };
        let (a, b) = (frame(0.0, 1), frame(2.0, 2));
        let (ma, mb) = (motion_frame(&a, W, H), motion_frame(&b, W, H));
        let (y, u, v) = yuv(3.0, -2.0, 0.2);
        let rows_a = row_means(&a, W, H);
        let reps = 200;
        // The fastest of many runs, which is what one core does undisturbed;
        // a busy machine only adds to the others.
        let time = |f: &mut dyn FnMut() -> f32| {
            let mut sink = f();
            let mut best = f64::MAX;
            for _ in 0..reps {
                let start = Instant::now();
                sink += f();
                best = best.min(start.elapsed().as_secs_f64() * 1e6);
            }
            std::hint::black_box(sink);
            best
        };
        let roll = time(&mut || horizon_roll(&a, W, H).0);
        let rows = time(&mut || row_means(&b, W, H)[0]);
        let flick = time(&mut || flicker_strength(&rows_a, &row_means(&b, W, H)));
        let wob = time(&mut || wobble(&ma.levels[0], &mb.levels[0], 2.0, 0.0));
        let wb = time(&mut || neutral_chroma(&y, &u, &v, W, H).map(|c| c.0).unwrap_or(0.0));
        // For scale, the existing focus measurement on the same frame.
        let lap = time(&mut || crate::metrics::laplacian_variance(&a, W, H) as f32);
        println!("picture timing reference: Laplacian variance {lap:.0} us");
        println!(
            "picture timing per {W}x{H} frame: roll {roll:.0} us, row means {rows:.0} us, flicker {flick:.0} us (with row means), wobble {wob:.0} us, neutral chroma {wb:.0} us, total {:.0} us",
            roll + flick + wob + wb
        );
    }
}
