//! Footage Selects engine: probing, per-frame scoring, selection, project files
//! and timeline export. No machine learning and no network access; every score
//! is classical image arithmetic on downsized grayscale frames.

pub mod analyze;
pub mod ffmpeg;
pub mod metrics;
pub mod probe;
pub mod rate;
pub mod project;
pub mod scores;
pub mod selection;
