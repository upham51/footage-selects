//! Print raw measurements and scores for clips: cargo run --release --example inspect -- clip.mov
use selects_engine::analyze::{analyze_clip, AnalyzeOptions};
use selects_engine::probe::probe;
use selects_engine::scores::{score_clip_with, ToneContext};
use selects_engine::tone::ToneProfile;
use std::sync::atomic::AtomicBool;
fn main() {
    for p in std::env::args().skip(1) {
        let info = probe(p.as_ref()).unwrap();
        let a = analyze_clip(&info, &AnalyzeOptions::default(), &AtomicBool::new(false), |_| {}).unwrap();
        let profile = info.detected_profile.as_ref().map(|d| d.profile).unwrap_or(ToneProfile::Standard);
        let s = score_clip_with(&a, ToneContext { profile, full_range: info.full_range });
        println!("profile: {}", profile.label());
        println!("== {p} {}x{} {:.2}s frames {} in {:.2}s", a.width, a.height, info.duration, a.frames.len(), a.elapsed);
        for i in 0..a.frames.len() {
            let m = a.frames.get(i);
            println!("{:5.2} lap {:8.1} mean {:5.1} lo {:.3} hi {:.3} p2 {:3} p98 {:3} dx {:6.2} dy {:6.2} ok {} | F {:.2} E {:.2} S {:.2}",
              s.t[i], m.lap, m.mean, m.lo, m.hi, m.p2, m.p98, m.dx, m.dy, m.motion_ok, s.focus[i], s.exposure[i], s.stability[i]);
        }
    }
}
