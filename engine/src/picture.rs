//! STUB: replaced by the real module.
pub fn horizon_roll(_gray: &[u8], _w: usize, _h: usize) -> (f32, f32) { (0.0, 0.0) }
pub fn row_means(gray: &[u8], w: usize, h: usize) -> Vec<f32> { (0..h).map(|y| gray[y * w..(y + 1) * w].iter().map(|&v| v as f32).sum::<f32>() / w as f32).collect() }
pub fn flicker_strength(_prev_rows: &[f32], _cur_rows: &[f32]) -> f32 { 0.0 }
pub fn wobble(_prev: &crate::metrics::Plane, _cur: &crate::metrics::Plane, _dx: f32, _dy: f32) -> f32 { 0.0 }
pub fn neutral_chroma(_y: &[u8], _u: &[u8], _v: &[u8], _w: usize, _h: usize) -> Option<(f32, f32)> { None }
pub fn temperature_tint(cb: f32, cr: f32) -> (f32, f32) { ((cr - cb) / 2.0, -(cr + cb) / 2.0) }
