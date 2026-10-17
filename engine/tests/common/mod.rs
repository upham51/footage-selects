//! Synthetic test footage generated with ffmpeg's built-in test sources.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

pub fn fixtures_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("synthetic-footage");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_ffmpeg(args: &[&str], out: &Path) {
    if out.exists() {
        return;
    }
    let tmp = out.with_extension(format!(
        "tmp.{}",
        out.extension().unwrap().to_string_lossy()
    ));
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-v", "error", "-y"])
        .args(args)
        .arg(&tmp)
        .status()
        .expect("ffmpeg must be installed to run the synthetic footage tests");
    assert!(status.success(), "ffmpeg failed for {}", out.display());
    std::fs::rename(&tmp, out).unwrap();
}

/// A still texture with structure in both directions, standing in for a real scene.
fn texture() -> PathBuf {
    let out = fixtures_dir().join("texture.png");
    run_ffmpeg(
        &[
            "-f", "lavfi", "-i",
            "nullsrc=s=1600x900:d=1,format=gray,geq=lum='clip(128+45*sin(X*0.043+3*sin(Y*0.021))+35*cos(Y*0.061+X*0.017)+28*sin((X-Y)*0.11)+40*(random(1)-0.5),0,255)'",
            "-vf", "gblur=sigma=0.8", "-frames:v", "1", "-update", "1",
        ],
        &out,
    );
    out
}

/// Texture blended with ffmpeg's moving test pattern, then `vf` applied.
fn lavfi_clip(name: &str, seconds: f32, vf: &str) -> PathBuf {
    let out = fixtures_dir().join(name);
    let tex = texture();
    let d = format!("{seconds}");
    let graph = format!("[0:v]format=yuv420p[t];[1:v]format=yuv420p[p];[t][p]blend=all_mode=average,{vf}[v]");
    run_ffmpeg(
        &[
            "-loop", "1", "-framerate", "30", "-t", &d, "-i", tex.to_str().unwrap(),
            "-f", "lavfi", "-t", &d, "-i", "testsrc2=s=1600x900:r=30",
            "-filter_complex", &graph, "-map", "[v]", "-c:v", "libx264", "-preset", "ultrafast",
            "-pix_fmt", "yuv420p", "-g", "30",
        ],
        &out,
    );
    out
}

pub struct Footage {
    pub sharp: PathBuf,
    pub blurred: PathBuf,
    pub well_exposed: PathBuf,
    pub dark: PathBuf,
    pub blown_out: PathBuf,
    pub steady: PathBuf,
    pub panning: PathBuf,
    /// Moves 100 px/s on a 1280 px frame, fast enough that a one-frame timing error would look like shake.
    pub fast_pan: PathBuf,
    pub shaky: PathBuf,
    /// 3 s sharp, 3 s blurred, 3 s sharp.
    pub sharp_blur_sharp: PathBuf,
    pub with_audio_25p: PathBuf,
    pub timecode_2997_df: PathBuf,
    pub variable_rate: PathBuf,
    pub rotated: PathBuf,
}

const CROP_STATIC: &str = "crop=1280:720:160:90";

fn generate() -> Footage {
    let sharp = lavfi_clip("sharp.mp4", 3.0, CROP_STATIC);
    let blurred = lavfi_clip("blurred.mp4", 3.0, &format!("{CROP_STATIC},gblur=sigma=5"));
    let well_exposed = sharp.clone();
    let dark = lavfi_clip("dark.mp4", 3.0, &format!("{CROP_STATIC},lutyuv=y=val*0.12"));
    let blown_out = lavfi_clip(
        "blown.mp4",
        3.0,
        &format!("{CROP_STATIC},lutyuv=y='clip(val*2.6+70,0,255)'"),
    );
    let steady = sharp.clone();
    let panning = lavfi_clip("panning.mp4", 4.0, "crop=1280:720:'20+t*60':90");
    let fast_pan = lavfi_clip("fast_pan.mp4", 3.0, "crop=1280:720:'10+t*100':90");
    let shaky = lavfi_clip(
        "shaky.mp4",
        4.0,
        "crop=1280:720:'160+90*(random(1)-0.5)':'90+70*(random(2)-0.5)'",
    );

    // Sharp for 3 s, blurred from 3 s to 6 s, sharp again to 9 s.
    let sbs = lavfi_clip(
        "sharp_blur_sharp.mp4",
        9.0,
        &format!("{CROP_STATIC},split[s][b];[b]gblur=sigma=6[bl];[s][bl]overlay=enable='between(t,3,6)'"),
    );

    let with_audio_25p = fixtures_dir().join("audio_25p.mov");
    run_ffmpeg(
        &[
            "-f", "lavfi", "-i", "testsrc2=s=960x540:r=25:d=2", "-f", "lavfi", "-i",
            "sine=frequency=440:sample_rate=48000:d=2", "-c:v", "libx264", "-preset",
            "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-ac", "2", "-timecode",
            "10:00:00:00",
        ],
        &with_audio_25p,
    );

    let timecode_2997_df = fixtures_dir().join("tc_2997_df.mov");
    run_ffmpeg(
        &[
            "-f", "lavfi", "-i", "testsrc2=s=640x360:r=30000/1001:d=2", "-c:v", "libx264",
            "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-timecode", "01:00:00;00",
        ],
        &timecode_2997_df,
    );

    // Drop every third frame of a 60 fps source and keep the original timestamps.
    let variable_rate = fixtures_dir().join("vfr.mp4");
    run_ffmpeg(
        &[
            "-f", "lavfi", "-i", "testsrc2=s=640x360:r=60:d=3", "-vf",
            "select='not(eq(mod(n,3),2))'", "-fps_mode", "passthrough", "-c:v", "libx264",
            "-preset", "ultrafast", "-pix_fmt", "yuv420p",
        ],
        &variable_rate,
    );

    let rotated = fixtures_dir().join("rotated.mp4");
    if !rotated.exists() {
        let src = lavfi_clip("rotate_src.mp4", 2.0, "crop=1280:720:160:90");
        run_ffmpeg(
            &[
                "-display_rotation", "90", "-i", src.to_str().unwrap(), "-c", "copy",
            ],
            &rotated,
        );
    }

    Footage {
        sharp,
        blurred,
        well_exposed,
        dark,
        blown_out,
        steady,
        panning,
        fast_pan,
        shaky,
        sharp_blur_sharp: sbs,
        with_audio_25p,
        timecode_2997_df,
        variable_rate,
        rotated,
    }
}

pub fn footage() -> &'static Footage {
    static F: OnceLock<Footage> = OnceLock::new();
    F.get_or_init(generate)
}
