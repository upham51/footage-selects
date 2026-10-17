//! Where the time goes for each clip, stage by stage.
//! cargo run --release --example stages -- clip.mov [more clips]
//!
//! 1. ffmpeg on its own: decode, keep 4 frames a second, downsize, send
//!    the small gray frames through the pipe (timed by the analysis itself,
//!    with the engine's math switched off by collecting frames first).
//! 2. The engine's math on those frames: Laplacian, histogram, block matching,
//!    picture fingerprint.
//! 3. The full analysis as the app runs it, with the decoder it settled on.
use selects_engine::analyze::{analysis_size, analyze_clip, decoded_frames, AnalyzeOptions};
use selects_engine::metrics::{estimate_shift, exposure_stats, laplacian_variance, motion_frame};
use selects_engine::fingerprint::frame_hashes;
use selects_engine::probe::probe;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

fn main() {
    let opts = AnalyzeOptions::default();
    for p in std::env::args().skip(1) {
        let info = probe(p.as_ref()).unwrap();
        let (w, h) = analysis_size(&info, opts.width);
        println!(
            "== {p}\n   {} {} {}x{} {:.3} fps, {:.2} s, rotation {}",
            info.codec, info.pix_fmt, info.width, info.height, info.avg_fps, info.duration, info.rotation
        );

        let t0 = Instant::now();
        let (frames, decoder) = decoded_frames(&info, &opts).unwrap();
        let ff = t0.elapsed().as_secs_f64();
        let n = frames.len().max(1) as f64;

        let t0 = Instant::now();
        for f in &frames {
            std::hint::black_box(laplacian_variance(f, w as usize, h as usize));
        }
        let lap = t0.elapsed().as_secs_f64();
        let t0 = Instant::now();
        for f in &frames {
            std::hint::black_box(exposure_stats(f));
        }
        let hist = t0.elapsed().as_secs_f64();
        let t0 = Instant::now();
        let mut prev = None;
        for f in &frames {
            let m = motion_frame(f, w as usize, h as usize);
            if let Some(p) = &prev {
                std::hint::black_box(estimate_shift(p, &m));
            }
            prev = Some(m);
        }
        let motion = t0.elapsed().as_secs_f64();
        let t0 = Instant::now();
        for f in &frames {
            std::hint::black_box(frame_hashes(f, w as usize, h as usize));
        }
        let fp = t0.elapsed().as_secs_f64();

        let t0 = Instant::now();
        let a = analyze_clip(&info, &opts, &AtomicBool::new(false), |_| {}).unwrap();
        let full = t0.elapsed().as_secs_f64();

        let ms = |s: f64| s * 1000.0 / n;
        println!("   ffmpeg decode + downsize + pipe ({decoder:?} path): {ff:.2} s for {} samples", frames.len());
        println!("   Laplacian      {:.3} ms a sample", ms(lap));
        println!("   histogram      {:.3} ms a sample", ms(hist));
        println!("   block matching {:.3} ms a sample", ms(motion));
        println!("   fingerprint    {:.3} ms a sample", ms(fp));
        println!(
            "   full analysis  {full:.2} s ({}), {:.1}x real time for this clip alone",
            a.decoder,
            info.duration / full
        );
    }
}
