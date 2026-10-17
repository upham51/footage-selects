//! Finished videos traced back to their source clips.
//!
//! Source clips are made with ffmpeg, analysed like real footage, then cut
//! into a finished video with a strong grade, a slowed shot, a letterboxed
//! shot, a black gap, a title card and a shot from a clip that is not in the
//! project. A second, vertical video reframes two sources to 9:16.

use selects_engine::analyze::{analyze_clip, AnalyzeOptions, ClipAnalysis};
use selects_engine::matching::{match_reference, ClipPrints, MatchReport};
use selects_engine::probe::probe;
use selects_engine::reference::{fingerprint_reference, REFERENCE_SAMPLE_RATE};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::OnceLock;

fn dir() -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("matching-footage");
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn codec() -> Vec<&'static str> {
    static X: OnceLock<bool> = OnceLock::new();
    let x264 = *X.get_or_init(|| {
        Command::new("ffmpeg")
            .args(["-hide_banner", "-encoders"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("libx264"))
            .unwrap_or(false)
    });
    if x264 {
        vec!["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p"]
    } else {
        vec!["-c:v", "mpeg4", "-q:v", "2", "-pix_fmt", "yuv420p"]
    }
}

fn ffmpeg(args: &[&str], out: &Path) {
    if out.exists() {
        return;
    }
    let tmp = out.with_extension("tmp.mp4");
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-v", "error", "-y"])
        .args(args)
        .args(codec())
        .arg(&tmp)
        .status()
        .expect("ffmpeg must be installed");
    assert!(status.success(), "ffmpeg failed for {}", out.display());
    std::fs::rename(&tmp, out).unwrap();
}

/// A large still picture to pan across; `k` changes its pattern.
fn texture(k: u32) -> PathBuf {
    let out = dir().join(format!("texture{k}.png"));
    if out.exists() {
        return out;
    }
    let (a, b, c) = (0.011 + 0.004 * k as f64, 0.017 + 0.003 * k as f64, 0.029 - 0.002 * k as f64);
    let expr = format!(
        "clip(128+55*sin(X*{a}+2*sin(Y*{b}))+45*cos(Y*{c}+X*{b})+30*sin((X+{k}*97-Y)*{a}*3),0,255)"
    );
    let graph = format!("nullsrc=s=2400x1400,format=gray,geq=lum='{expr}',gblur=sigma=2");
    let tmp = out.with_extension("tmp.png");
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-v", "error", "-y", "-f", "lavfi", "-i", &graph, "-frames:v", "1", "-update", "1"])
        .arg(&tmp)
        .status()
        .unwrap();
    assert!(status.success());
    std::fs::rename(&tmp, &out).unwrap();
    out
}

/// 10 s at 30 fps: a 640x360 window panning across texture `k`.
fn pan_clip(name: &str, k: u32, vx: f64, vy: f64) -> PathBuf {
    let out = dir().join(name);
    let tex = texture(k);
    let vf = format!("crop=640:360:'40+{vx}*t':'60+{vy}*t'");
    ffmpeg(
        &["-loop", "1", "-framerate", "30", "-t", "10", "-i", tex.to_str().unwrap(), "-vf", &vf, "-g", "30"],
        &out,
    );
    out
}

/// 10 s at 30 fps: ffmpeg's moving test pattern with a slow pan, so every moment differs.
fn testsrc_clip(name: &str) -> PathBuf {
    let out = dir().join(name);
    ffmpeg(
        &["-f", "lavfi", "-i", "testsrc2=s=1280x720:r=30:d=10", "-vf", "crop=640:360:'40*t':'20*t'", "-g", "30"],
        &out,
    );
    out
}

struct Sources {
    a: PathBuf,
    d: PathBuf,
    e: PathBuf,
    g: PathBuf,
    foreign: PathBuf,
}

fn sources() -> &'static Sources {
    static S: OnceLock<Sources> = OnceLock::new();
    S.get_or_init(|| Sources {
        a: testsrc_clip("a_testsrc.mp4"),
        d: pan_clip("d_pan.mp4", 1, 60.0, 8.0),
        e: pan_clip("e_pan.mp4", 2, -20.0, 45.0),
        g: pan_clip("g_pan.mp4", 3, 70.0, -25.0),
        foreign: pan_clip("f_foreign.mp4", 4, 50.0, 30.0),
    })
}

const GRADE: &str = "eq=contrast=1.5:gamma=0.8:saturation=1.4,curves=preset=strong_contrast";

fn finished_landscape(s: &Sources) -> PathBuf {
    let out = dir().join("finished_landscape.mp4");
    let norm = "scale=640:360,setsar=1,format=yuv420p";
    let graph = format!(
        "[0:v]setpts=PTS-STARTPTS,fps=24,{norm},{GRADE}[p0];\
         [1:v]fps=24,{norm}[p1];\
         [2:v]setpts=2.5*(PTS-STARTPTS),fps=24,{norm},{GRADE}[p2];\
         [3:v]fps=24,drawbox=x=100:y=150:w=440:h=60:color=white:t=fill,{norm}[p3];\
         [4:v]setpts=PTS-STARTPTS,fps=24,crop=640:268,pad=640:360:0:46:black,{norm},{GRADE}[p4];\
         [5:v]setpts=PTS-STARTPTS,fps=24,{norm},{GRADE}[p5];\
         [6:v]setpts=PTS-STARTPTS,fps=24,{norm},{GRADE}[p6];\
         [p0][p1][p2][p3][p4][p5][p6]concat=n=7:v=1:a=0[v]"
    );
    let st = |p: &PathBuf| p.to_str().unwrap().to_string();
    let (a, d, e, g, f) = (st(&s.a), st(&s.d), st(&s.e), st(&s.g), st(&s.foreign));
    ffmpeg(
        &[
            "-ss", "1", "-t", "3", "-i", &a,
            "-f", "lavfi", "-i", "color=c=black:s=640x360:r=24:d=1",
            "-ss", "1", "-t", "2", "-i", &d,
            "-f", "lavfi", "-i", "color=c=0x335577:s=640x360:r=24:d=1.5",
            "-ss", "0.5", "-t", "3", "-i", &e,
            "-ss", "1", "-t", "3", "-i", &f,
            "-ss", "2", "-t", "3", "-i", &g,
            "-filter_complex", &graph, "-map", "[v]", "-r", "24",
        ],
        &out,
    );
    out
}

fn finished_vertical(s: &Sources) -> PathBuf {
    let out = dir().join("finished_vertical_v2.mp4");
    let norm = "crop=ih*9/16:ih,scale=360:640,setsar=1,format=yuv420p";
    let graph = format!(
        "[0:v]setpts=PTS-STARTPTS,fps=24,{norm},{GRADE}[p0];\
         [1:v]setpts=PTS-STARTPTS,fps=24,{norm},{GRADE}[p1];\
         [p0][p1]concat=n=2:v=1:a=0[v]"
    );
    ffmpeg(
        &[
            "-ss", "5", "-t", "3", "-i", s.g.to_str().unwrap(),
            "-ss", "5", "-t", "2", "-i", s.d.to_str().unwrap(),
            "-filter_complex", &graph, "-map", "[v]", "-r", "24",
        ],
        &out,
    );
    out
}

fn analyse(path: &Path) -> (ClipAnalysis, f64) {
    let info = probe(path).unwrap();
    let opts = AnalyzeOptions { hardware_decode: false, ..Default::default() };
    let a = analyze_clip(&info, &opts, &AtomicBool::new(false), |_| {}).unwrap();
    assert!(a.frames.fp.len() == a.frames.len(), "fingerprints were measured");
    (a, info.duration)
}

struct Project {
    clips: Vec<(u64, ClipAnalysis, f64)>,
}

impl Project {
    fn prints(&self) -> Vec<ClipPrints<'_>> {
        self.clips
            .iter()
            .map(|(id, a, dur)| ClipPrints { clip_id: *id, t: &a.frames.t, fp: &a.frames.fp, duration: *dur })
            .collect()
    }
}

fn project() -> &'static Project {
    static P: OnceLock<Project> = OnceLock::new();
    P.get_or_init(|| {
        let s = sources();
        // The foreign clip is deliberately left out.
        let clips = [(1, &s.a), (4, &s.d), (5, &s.e), (7, &s.g)]
            .iter()
            .map(|(id, p)| {
                let (a, d) = analyse(p);
                (*id, a, d)
            })
            .collect();
        Project { clips }
    })
}

/// The shot of `report` covering finished time `t`.
fn shot_at(report: &MatchReport, t: f64) -> &selects_engine::matching::MatchedShot {
    report
        .shots
        .iter()
        .find(|s| s.ref_start <= t && t < s.ref_end)
        .unwrap_or_else(|| panic!("no shot covers {t} s: {:#?}", report.shots))
}

fn expect(report: &MatchReport, t: f64, clip: u64, start: f64, end: f64, speed: f64, crop: &str) {
    let shot = shot_at(report, t);
    let s = shot
        .source
        .as_ref()
        .unwrap_or_else(|| panic!("shot at {t} s should come from clip {clip}: {shot:#?}"));
    assert_eq!(s.clip_id, clip, "shot at {t} s: {s:#?}");
    assert!((s.start - start).abs() <= 0.5, "start of shot at {t} s: {} not {start}", s.start);
    assert!((s.end - end).abs() <= 0.5, "end of shot at {t} s: {} not {end}", s.end);
    assert!((s.speed - speed).abs() <= 0.1 * speed, "speed of shot at {t} s: {} not {speed}", s.speed);
    assert_eq!(s.crop, crop, "crop of shot at {t} s");
}

#[test]
fn a_graded_finished_video_is_traced_back_to_its_clips() {
    let p = project();
    let video = finished_landscape(sources());
    let r = fingerprint_reference(&video, REFERENCE_SAMPLE_RATE, &AtomicBool::new(false)).unwrap();
    let report = match_reference(&r, &p.prints());
    for s in &report.shots {
        eprintln!("{:.2}-{:.2} {:?}", s.ref_start, s.ref_end, s.source.as_ref().map(|x| (x.clip_id, (x.start * 100.0).round() / 100.0, (x.end * 100.0).round() / 100.0, (x.speed * 1000.0).round() / 1000.0, x.confidence, x.crop.clone())));
    }

    // 0 to 3 s: clip 1 from 1 s, normal speed.
    expect(&report, 1.5, 1, 1.0, 4.0, 1.0, "full");
    // 3 to 4 s: black, no shot covers it or it is unmatched.
    if let Some(s) = report.shots.iter().find(|s| s.ref_start <= 3.5 && 3.5 < s.ref_end) {
        assert!(s.source.is_none(), "black gap matched: {s:#?}");
    }
    // 4 to 9 s: clip 4 from 1 s to 3 s slowed 2.5 times.
    expect(&report, 6.5, 4, 1.0, 3.0, 0.4, "full");
    // 9 to 10.5 s: title card.
    if let Some(s) = report.shots.iter().find(|s| s.ref_start <= 9.75 && 9.75 < s.ref_end) {
        assert!(s.source.is_none(), "title card matched: {s:#?}");
    }
    // 10.5 to 13.5 s: clip 5 from 0.5 s, letterboxed.
    expect(&report, 12.0, 5, 0.5, 3.5, 1.0, "2.39:1");
    // 13.5 to 16.5 s: a clip that is not in the project.
    let foreign = shot_at(&report, 15.0);
    assert!(foreign.source.is_none(), "foreign footage matched: {foreign:#?}");
    // 16.5 to 19.5 s: clip 7 from 2 s.
    expect(&report, 18.0, 7, 2.0, 5.0, 1.0, "full");

    let matched: Vec<_> = report.matched().collect();
    assert_eq!(matched.len(), 4, "{:#?}", report.shots);
}

#[test]
fn a_vertical_reframe_is_traced_back_to_its_clips() {
    let p = project();
    let video = finished_vertical(sources());
    let r = fingerprint_reference(&video, REFERENCE_SAMPLE_RATE, &AtomicBool::new(false)).unwrap();
    let report = match_reference(&r, &p.prints());
    expect(&report, 1.5, 7, 5.0, 8.0, 1.0, "9:16");
    expect(&report, 4.0, 4, 5.0, 7.0, 1.0, "9:16");
    assert_eq!(report.matched().count(), 2, "{:#?}", report.shots);
}

#[test]
fn matching_two_hours_of_footage_is_fast() {
    // 170 clips of 42 s at 4 samples a second (28,560 frames) against a
    // 5 minute finished video at 6 a second (1,800 samples), random hashes.
    let mut s: u64 = 0x1234_5678;
    let mut next = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    let clips: Vec<(Vec<f32>, Vec<[u64; 3]>)> = (0..170)
        .map(|_| {
            // Neighbouring frames of a clip differ by a few bits, as real footage does.
            let t = (0..168).map(|i| i as f32 / 4.0).collect();
            let mut h = [next(), next(), next()];
            let fp = (0..168)
                .map(|_| {
                    for v in h.iter_mut() {
                        *v ^= 1 << (next() % 64);
                        *v ^= 1 << (next() % 64);
                    }
                    h
                })
                .collect();
            (t, fp)
        })
        .collect();
    let prints: Vec<ClipPrints> = clips
        .iter()
        .enumerate()
        .map(|(i, (t, fp))| ClipPrints { clip_id: i as u64, t, fp, duration: 42.0 })
        .collect();
    // The finished video reuses 2 s stretches of the source, so matching has real work to do.
    let mut hash = Vec::new();
    let mut t = Vec::new();
    for i in 0..1800usize {
        let clip = (i / 12) % 170;
        let frame = ((i % 12) as f64 / 6.0 * 4.0) as usize + 20;
        hash.push(clips[clip].1[frame][0]);
        t.push(i as f32 / 6.0);
    }
    let r = selects_engine::reference::ReferencePrint {
        path: String::new(),
        name: String::new(),
        duration: 300.0,
        t,
        usable: vec![true; hash.len()],
        hash,
        sample_rate: 6.0,
    };
    let start = std::time::Instant::now();
    let report = match_reference(&r, &prints);
    let took = start.elapsed().as_secs_f64();
    eprintln!("matched {} shots in {took:.2} s", report.shots.len());
    assert!(report.matched().count() >= 140, "{} matched", report.matched().count());
    assert!(took < 15.0, "took {took} s");
}
