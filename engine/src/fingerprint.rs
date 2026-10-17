//! Picture fingerprints used to find where a finished video's shots came from.
//!
//! A difference hash: the picture is shrunk to 9 by 8 cells, and each bit says
//! whether a cell is darker than its right-hand neighbour. Any grade that keeps
//! brighter things brighter (a log to Rec. 709 conversion, contrast, exposure,
//! most looks) leaves those comparisons, and so the hash, unchanged.
//!
//! Finished videos are often cropped from the source: letterboxed to 2.39:1 or
//! reframed to 9:16 for phones. Each source frame therefore carries three
//! hashes: the whole picture, a centred 2.39:1 band and a centred 9:16 column.

/// A rectangle of the picture in pixels: x0, y0, x1, y1 (exclusive).
pub type Region = (usize, usize, usize, usize);

pub const HASH_FULL: usize = 0;
pub const HASH_WIDE: usize = 1;
pub const HASH_TALL: usize = 2;

/// Mean luma of each cell when `region` is divided into `cols` by `rows`.
fn cell_means(gray: &[u8], w: usize, region: Region, cols: usize, rows: usize) -> Vec<f32> {
    let (x0, y0, x1, y1) = region;
    let rw = (x1 - x0).max(1);
    let rh = (y1 - y0).max(1);
    let mut out = vec![0f32; cols * rows];
    for r in 0..rows {
        let ya = y0 + r * rh / rows;
        let yb = (y0 + (r + 1) * rh / rows).max(ya + 1).min(y1.max(ya + 1));
        for c in 0..cols {
            let xa = x0 + c * rw / cols;
            let xb = (x0 + (c + 1) * rw / cols).max(xa + 1).min(x1.max(xa + 1));
            let mut sum: u64 = 0;
            let mut n: u64 = 0;
            for y in ya..yb {
                let row = &gray[y * w..y * w + w];
                for &v in &row[xa..xb] {
                    sum += v as u64;
                    n += 1;
                }
            }
            out[r * cols + c] = if n > 0 { sum as f32 / n as f32 } else { 0.0 };
        }
    }
    out
}

/// 64-bit difference hash of one region of a grayscale picture.
pub fn dhash(gray: &[u8], w: usize, h: usize, region: Region) -> u64 {
    if w < 9 || h < 8 || gray.len() < w * h {
        return 0;
    }
    let (x0, y0, x1, y1) = region;
    let region = (x0.min(w - 1), y0.min(h - 1), x1.min(w).max(x0 + 1), y1.min(h).max(y0 + 1));
    let cells = cell_means(gray, w, region, 9, 8);
    let mut bits: u64 = 0;
    for r in 0..8 {
        for c in 0..8 {
            if cells[r * 9 + c] < cells[r * 9 + c + 1] {
                bits |= 1 << (r * 8 + c);
            }
        }
    }
    bits
}

/// The centred band of a picture with the given width to height ratio.
pub fn centre_crop(w: usize, h: usize, aspect: f64) -> Region {
    let a = w as f64 / h.max(1) as f64;
    if a < aspect {
        let ch = ((w as f64 / aspect).round() as usize).clamp(1, h);
        let y0 = (h - ch) / 2;
        (0, y0, w, y0 + ch)
    } else {
        let cw = ((h as f64 * aspect).round() as usize).clamp(1, w);
        let x0 = (w - cw) / 2;
        (x0, 0, x0 + cw, h)
    }
}

/// Whole picture, centred 2.39:1 band and centred 9:16 column.
pub fn frame_hashes(gray: &[u8], w: usize, h: usize) -> [u64; 3] {
    [
        dhash(gray, w, h, (0, 0, w, h)),
        dhash(gray, w, h, centre_crop(w, h, 2.39)),
        dhash(gray, w, h, centre_crop(w, h, 9.0 / 16.0)),
    ]
}

/// Number of differing bits.
pub fn distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient_scene(w: usize, h: usize) -> Vec<u8> {
        let mut g = vec![0u8; w * h];
        for y in 0..h {
            for x in 0..w {
                let v = ((x * 7 + y * 3) % 97) as f32 + 60.0 * ((x as f32 / 23.0).sin() + (y as f32 / 17.0).cos());
                g[y * w + x] = (v + 80.0).clamp(0.0, 255.0) as u8;
            }
        }
        g
    }

    #[test]
    fn a_monotonic_grade_keeps_the_hash() {
        let (w, h) = (160, 90);
        let g = gradient_scene(w, h);
        // A strong contrast curve, like turning log footage into Rec. 709.
        let graded: Vec<u8> = g
            .iter()
            .map(|&v| {
                let x = v as f32 / 255.0;
                (255.0 * (x * x * (3.0 - 2.0 * x)).powf(0.8)) as u8
            })
            .collect();
        let a = frame_hashes(&g, w, h);
        let b = frame_hashes(&graded, w, h);
        assert!(distance(a[0], b[0]) <= 4, "{}", distance(a[0], b[0]));
    }

    #[test]
    fn different_pictures_have_distant_hashes() {
        let (w, h) = (160, 90);
        let g = gradient_scene(w, h);
        let flipped: Vec<u8> = (0..w * h).map(|i| g[(i / w) * w + (w - 1 - i % w)]).collect();
        assert!(distance(dhash(&g, w, h, (0, 0, w, h)), dhash(&flipped, w, h, (0, 0, w, h))) > 16);
    }

    #[test]
    fn centre_crops_have_the_right_shape() {
        assert_eq!(centre_crop(1920, 1080, 2.39), (0, 138, 1920, 941));
        assert_eq!(centre_crop(1920, 1080, 9.0 / 16.0), (656, 0, 1264, 1080));
    }
}
