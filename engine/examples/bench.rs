//! Analyse every clip in a folder with N clips at a time and report speed.
//! cargo run --release --example bench -- <folder> [clips at once]
use selects_engine::analyze::{analyze_clip, AnalyzeOptions};
use selects_engine::probe::{collect_video_files, probe};
use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

fn main() {
    let folder = std::path::PathBuf::from(std::env::args().nth(1).unwrap());
    let workers: usize = std::env::args().nth(2).map(|s| s.parse().unwrap()).unwrap_or(2);
    let files = collect_video_files(&[folder]);
    let infos: Vec<_> = files.iter().filter_map(|f| probe(f).ok()).collect();
    let footage: f64 = infos.iter().map(|i| i.duration).sum();
    let queue = Mutex::new(infos.clone());
    let decoders = Mutex::new(BTreeMap::<String, usize>::new());
    let start = std::time::Instant::now();
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let Some(info) = queue.lock().unwrap().pop() else { break };
                let a = analyze_clip(&info, &AnalyzeOptions::default(), &AtomicBool::new(false), |_| {}).unwrap();
                *decoders.lock().unwrap().entry(a.decoder).or_default() += 1;
            });
        }
    });
    let t = start.elapsed().as_secs_f64();
    let used: Vec<String> = decoders.into_inner().unwrap().into_iter().map(|(d, n)| format!("{n} {d}")).collect();
    println!(
        "{} clips, {:.0} s of footage in {:.1} s with {workers} at once: {:.1}x real time (decoded: {})",
        infos.len(),
        footage,
        t,
        footage / t,
        used.join(", ")
    );
}
