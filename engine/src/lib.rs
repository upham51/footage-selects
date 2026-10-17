//! The engine: probing, per-frame scoring, selection, project files
//! and timeline export. No machine learning and no network access; every score
//! is classical image arithmetic on downsized grayscale frames.

/// The app's name, read at build time from `productName` in src-tauri/tauri.conf.json.
pub const PRODUCT_NAME: &str = env!("PRODUCT_NAME");
/// The app's version, from the same file.
pub const PRODUCT_VERSION: &str = env!("PRODUCT_VERSION");

pub mod analyze;
pub mod export;
pub mod ffmpeg;
pub mod metrics;
pub mod probe;
pub mod rate;
pub mod project;
pub mod scores;
pub mod selection;
