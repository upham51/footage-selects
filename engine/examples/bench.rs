//! Analyse every clip in a folder with N clips at a time and report speed.
//! cargo run --release --example bench -- <folder> [clips at once]
use selects_engine::analyze::{analyze_clip, AnalyzeOptions};
use selects_engine::probe::{collect_video_files, probe};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

fn main() {
    let folder = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let workers: usize = std::env::args().nth(2).map(|s| s.parse().unwrap()).unwrap_or(2);
    let files = collect_video_files(&[folder]);
    let infos: Vec<_> = files.iter().filter_map(|f| probe(f).ok()).collect();
    let footage: f64 = infos.iter().map(|i| i.duration).sum();
    let queue = Mutex::new(infos.clone());
    let start = std::time::Instant::now();
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let Some(info) = queue.lock().unwrap().pop() else { break };
                analyze_clip(&info, &AnalyzeOptions::default(), &AtomicBool::new(false), |_| {}).unwrap();
            });
        }
    });
    let t = start.elapsed().as_secs_f64();
    println!("{} clips, {:.0} s of footage in {:.1} s with {workers} at once: {:.1}x real time", infos.len(), footage, t, footage / t);
}
