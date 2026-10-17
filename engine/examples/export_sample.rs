//! Analyse every clip in a folder and export the selects timeline.
//! cargo run --release --example export_sample -- <folder> <output base> <rate num/den> <df 0|1> <full|selects>
use selects_engine::analyze::{analyze_clip, AnalyzeOptions};
use selects_engine::probe::{collect_video_files, probe};
use selects_engine::project::{Project, TimelineLayout};
use selects_engine::rate::Rate;
use std::sync::atomic::AtomicBool;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let folder = std::path::PathBuf::from(&args[1]);
    let base = std::path::PathBuf::from(&args[2]);
    let (n, d) = args[3].split_once('/').unwrap();
    let rate = Rate::new(n.parse().unwrap(), d.parse().unwrap());
    let mut p = Project::new();
    for f in collect_video_files(&[folder]) {
        let info = probe(&f).map_err(|e| e.to_string());
        let id = p.add_clip(&f.to_string_lossy(), info).unwrap();
        if let Some(info) = p.clip(id).unwrap().info.clone() {
            let a = analyze_clip(&info, &AnalyzeOptions::default(), &AtomicBool::new(false), |_| {}).unwrap();
            p.clip_mut(id).unwrap().analysis = Some(a);
        }
    }
    p.selection.min_length = 1.0;
    p.export.timeline_rate = Some(rate);
    p.export.drop_frame = Some(args[4] == "1");
    p.export.layout = if args[5] == "selects" { TimelineLayout::SelectsOnly } else { TimelineLayout::FullClipsBelow };
    let w = selects_engine::export::write_all(&p, &base).unwrap();
    println!("{} selects, warnings: {:?}", w.selects, w.warnings);
}
