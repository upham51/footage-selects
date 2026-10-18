//! STUB: replaced by the real module.
use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MusicInfo { pub path: String, pub name: String, pub duration: f64, pub sample_rate: u32, pub channels: u32, pub codec: String }
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct BeatGrid { pub tempo: f64, pub beats: Vec<f64>, pub downbeats: Vec<usize>, pub strength: Vec<f32>, pub confidence: f32 }
pub const ANALYSIS_RATE: u32 = 22050;
pub fn probe_music(_path: &std::path::Path) -> Result<MusicInfo, String> { Err("stub".into()) }
pub fn decode_mono(_path: &std::path::Path, _rate: u32, _cancel: &std::sync::atomic::AtomicBool) -> Result<Vec<f32>, String> { Err("stub".into()) }
pub fn analyze_music(_samples: &[f32], _rate: u32) -> BeatGrid { BeatGrid::default() }
pub fn beat_grid(_path: &std::path::Path, _cancel: &std::sync::atomic::AtomicBool) -> Result<(MusicInfo, BeatGrid), String> { Err("stub".into()) }
pub fn cut_points(grid: &BeatGrid, every: usize, max_seconds: f64) -> Vec<f64> {
    let first = grid.downbeats.first().copied().unwrap_or(0);
    grid.beats.iter().skip(first).step_by(every.max(1)).copied().filter(|&t| t <= max_seconds).collect()
}
