//! A full-size still of the best frame of every select, a small copy of
//! each, and a contact sheet that lays them all out on one page.
//!
//! The best frame is the sample inside the select with the highest ranking
//! score (the same weighted focus, exposure and stability the per-clip limit
//! uses), so it is the sharpest, best exposed, steadiest moment. Stills are
//! written as JPEG by the ffmpeg that ships with the app. Log footage is
//! written as recorded, so it looks flat until it is graded.

use crate::export::{select_keywords, Tier};
use crate::ffmpeg;
use crate::project::Project;
use crate::rate::{frames_to_timecode, timecode_to_frames};
use crate::selection::{ClipSelection, Segment};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

/// Width of the small copies the contact sheet shows.
pub const THUMB_WIDTH: u32 = 640;

/// The moment inside `seg` with the best ranking score, in seconds.
pub fn best_moment(project: &Project, sel: &ClipSelection, seg: &Segment) -> f64 {
    let sm = &sel.smoothed;
    let s = &project.selection;
    let mut best: Option<(f32, f64)> = None;
    // Stay a little inside the ends, where cuts and blur from a starting move sit.
    let margin = ((seg.end - seg.start) * 0.1).min(0.5);
    for i in 0..sm.len() {
        let t = sm.t[i] as f64;
        if t < seg.start + margin || t > seg.end - margin {
            continue;
        }
        let v = s.rank_score(sm.focus[i], sm.exposure[i], sm.stability[i]);
        if best.map_or(true, |b| v > b.0 + 1e-6) {
            best = Some((v, t));
        }
    }
    best.map(|b| b.1).unwrap_or(0.5 * (seg.start + seg.end))
}

/// One still that was written.
#[derive(Clone, Debug)]
pub struct StillEntry {
    pub number: usize,
    pub clip: String,
    pub file: String,
    pub thumb: String,
    pub source_tc: String,
    pub at: f64,
    pub length: f64,
    pub tier: Tier,
    pub stars: u8,
    pub manual: bool,
    pub focus: f32,
    pub exposure: f32,
    pub stability: f32,
    pub keywords: Vec<String>,
    pub problems: Vec<String>,
    pub group: Option<String>,
    pub log: bool,
}

pub struct StillsWritten {
    pub folder: PathBuf,
    pub sheet: PathBuf,
    pub written: usize,
    /// Clip name and plain reason for stills that could not be written.
    pub failed: Vec<(String, String)>,
    pub cancelled: bool,
}

fn safe_name(s: &str) -> String {
    let out: String = s.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' { c } else { '_' }).collect();
    out.trim().chars().take(60).collect()
}

fn write_still(path: &str, t: f64, full: &Path, thumb: &Path) -> Result<(), String> {
    let filter = format!("[0:v]setsar=1,split=2[a][b];[a]format=yuvj420p[full];[b]scale={THUMB_WIDTH}:-2:flags=bicubic,format=yuvj420p[small]");
    let out = ffmpeg::command("ffmpeg")
        .args(["-hide_banner", "-nostdin", "-v", "error", "-y"])
        .args(["-ss", &format!("{:.4}", t.max(0.0))])
        .args(["-i", path])
        .args(["-filter_complex", &filter])
        .args(["-map", "[full]", "-frames:v", "1", "-c:v", "mjpeg", "-q:v", "2"])
        .arg(full)
        .args(["-map", "[small]", "-frames:v", "1", "-c:v", "mjpeg", "-q:v", "4"])
        .arg(thumb)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("The video reader did not start ({e})."))?;
    if !out.status.success() || !full.exists() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(crate::probe::friendly_ffmpeg_error(err.lines().last().unwrap_or("")));
    }
    Ok(())
}

/// Write a still of every select into `<parent>/<name> stills`, with small
/// copies in a `small` folder and `Contact sheet.html` beside them.
pub fn export_stills(project: &Project, parent: &Path, name: &str, cancel: &AtomicBool, mut progress: impl FnMut(usize, usize)) -> Result<StillsWritten, String> {
    let (selections, _) = project.select_all();
    let scenes = crate::scene::resolve(project);
    let mut jobs: Vec<(usize, &Segment)> = Vec::new();
    for (ci, sel) in selections.iter().enumerate() {
        if project.clips[ci].info.is_none() {
            continue;
        }
        for g in sel.kept() {
            jobs.push((ci, g));
        }
    }
    if jobs.is_empty() {
        return Err("There are no selects yet, so there are no stills to write. Analyze footage or lower a threshold first.".into());
    }
    // A name typed as "Selects.xml" names the folder and the sheet "Selects".
    let name = crate::export::strip_known_extensions(name.trim());
    let name = if safe_name(name).is_empty() { "Selects" } else { name };
    let folder = parent.join(format!("{} stills", safe_name(name)));
    let small = folder.join("small");
    std::fs::create_dir_all(&small).map_err(|e| format!("The folder for the stills could not be made ({e}). Nothing was written. Choose a folder you can write to."))?;
    let total = jobs.len();
    let mut entries = Vec::new();
    let mut failed = Vec::new();
    let mut cancelled = false;
    for (k, (ci, g)) in jobs.into_iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        progress(k, total);
        let clip = &project.clips[ci];
        let info = clip.info.as_ref().unwrap();
        let t = best_moment(project, &selections[ci], g);
        let tc_drop = info.timecode.as_deref().map(crate::rate::is_drop_frame_string).unwrap_or(false);
        let tc0 = info.timecode.as_deref().and_then(|tc| timecode_to_frames(tc, info.rate, tc_drop)).unwrap_or(0);
        let source_tc = frames_to_timecode(tc0 + info.rate.seconds_to_frames(t), info.rate, tc_drop);
        let stem = Path::new(&info.name).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let file = format!("{:03} {} {}.jpg", k + 1, safe_name(&stem), source_tc.replace([':', ';'], "-"));
        match write_still(&clip.path, t, &folder.join(&file), &small.join(&file)) {
            Ok(()) => {
                let tier = Tier::from_score(g.score);
                let scene = &scenes[ci];
                entries.push(StillEntry {
                    number: k + 1,
                    clip: info.name.clone(),
                    file: file.clone(),
                    thumb: format!("small/{file}"),
                    source_tc,
                    at: t,
                    length: g.end - g.start,
                    tier,
                    stars: g.stars,
                    manual: g.manual,
                    focus: g.focus,
                    exposure: g.exposure,
                    stability: g.stability,
                    keywords: select_keywords(g, tier, scene.light, scene.group.as_deref()),
                    problems: g.problem_notes(),
                    group: scene.group.clone(),
                    log: clip.resolved_profile(project.fallback_profile).profile.is_log(),
                });
            }
            Err(e) => failed.push((info.name.clone(), e)),
        }
    }
    progress(total, total);
    let sheet = folder.join("Contact sheet.html");
    std::fs::write(&sheet, contact_sheet(name, &entries)).map_err(|e| format!("The contact sheet could not be written ({e}). The stills themselves were written to {}.", folder.display()))?;
    Ok(StillsWritten { written: entries.len(), folder, sheet, failed, cancelled })
}

fn esc(s: &str) -> String {
    crate::export::xml::escape(s)
}

/// A printable page of every still, grouped by scene.
pub fn contact_sheet(name: &str, entries: &[StillEntry]) -> String {
    let mut groups: Vec<(String, Vec<&StillEntry>)> = Vec::new();
    for e in entries {
        let g = e.group.clone().unwrap_or_else(|| "Selects".into());
        match groups.iter_mut().find(|x| x.0 == g) {
            Some(x) => x.1.push(e),
            None => groups.push((g, vec![e])),
        }
    }
    let any_log = entries.iter().any(|e| e.log);
    let mut body = String::new();
    for (g, list) in &groups {
        body.push_str(&format!("<section><h2 class=\"eyebrow\">{} <span class=\"count\">{} {}</span></h2><div class=\"grid\">", esc(g), list.len(), if list.len() == 1 { "select" } else { "selects" }));
        for e in list {
            let mut title = format!("Select {} &middot; {}", e.number, e.tier.word());
            if e.stars > 0 {
                title.push_str(&format!(" &middot; {} {}", e.stars, if e.stars == 1 { "star" } else { "stars" }));
            }
            if e.manual {
                title.push_str(" &middot; kept by hand");
            }
            let problems = if e.problems.is_empty() {
                String::new()
            } else {
                format!("<p class=\"warn\">{}</p>", e.problems.iter().map(|p| esc(p)).collect::<Vec<_>>().join(". "))
            };
            let tags: String = e.keywords.iter().skip(1).map(|k| format!("<span class=\"tag\">{}</span>", esc(k))).collect();
            body.push_str(&format!(
                "<figure><a href=\"{file}\"><img src=\"{thumb}\" alt=\"Select {n} from {clip}\" loading=\"lazy\"></a><figcaption><div class=\"title\">{title}</div><div class=\"clip\">{clip}</div><div class=\"nums\">{tc} &middot; {len:.1} s &middot; focus {f} &middot; exposure {x} &middot; stability {s}</div><div class=\"tags\">{tags}</div>{problems}</figcaption></figure>",
                file = esc(&e.file),
                thumb = esc(&e.thumb),
                n = e.number,
                clip = esc(&e.clip),
                tc = esc(&e.source_tc),
                len = e.length,
                f = (e.focus * 100.0).round(),
                x = (e.exposure * 100.0).round(),
                s = (e.stability * 100.0).round(),
            ));
        }
        body.push_str("</div></section>");
    }
    let log_note = if any_log { "<p class=\"note\">Log footage is shown as recorded, before any grade, so it looks flat.</p>" } else { "" };
    format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} contact sheet</title>
<style>
:root {{ --ground:#FAF8F3; --card:#FFFFFF; --line:#E4E0D6; --ink:#1A2432; --muted:#5E6878; --navy:#2C4A6E; --gold:#C9A227; --amber:#9A5B00; --amber-bg:#FDF2E2; }}
* {{ box-sizing:border-box; }}
body {{ margin:0; background:var(--ground); color:var(--ink); font:16px/1.6 Inter, system-ui, sans-serif; padding:32px 16px 48px; }}
header {{ max-width:1200px; margin:0 auto 24px; }}
h1 {{ font-family:"Averia Serif Libre", Georgia, serif; font-weight:400; font-size:30px; margin:0 0 4px; }}
.sub, .note {{ color:var(--muted); font-size:14.5px; margin:0; }}
section {{ max-width:1200px; margin:0 auto 28px; background:var(--card); border:1px solid var(--line); border-radius:12px; padding:18px 18px 8px; position:relative; }}
section::before {{ content:""; position:absolute; left:0; right:0; top:0; height:3px; border-radius:12px 12px 0 0; background:linear-gradient(90deg,#C9A227,rgba(201,162,39,0)); }}
.eyebrow {{ font:600 12px/1.4 "Inter Tight", Inter, sans-serif; letter-spacing:.08em; text-transform:uppercase; color:var(--muted); margin:0 0 14px; }}
.count {{ letter-spacing:normal; text-transform:none; font-weight:400; }}
.grid {{ display:grid; grid-template-columns:repeat(auto-fill, minmax(260px, 1fr)); gap:16px; }}
figure {{ margin:0 0 12px; break-inside:avoid; }}
img {{ width:100%; aspect-ratio:16/9; object-fit:contain; background:#EFF2F4; border-radius:8px; border:1px solid var(--line); display:block; }}
figcaption {{ padding-top:8px; font-size:13.5px; }}
.title {{ font-weight:600; }}
.clip {{ color:var(--muted); overflow-wrap:anywhere; }}
.nums {{ font-family:"Inter Tight", Inter, sans-serif; font-variant-numeric:tabular-nums; color:var(--muted); font-size:12.5px; }}
.tags {{ margin-top:4px; }}
.tag {{ display:inline-block; font-size:11.5px; padding:1px 8px; margin:2px 4px 0 0; border:1px solid var(--line); border-radius:999px; color:var(--muted); }}
.warn {{ margin:6px 0 0; color:var(--amber); background:var(--amber-bg); border-radius:8px; padding:4px 8px; font-size:12.5px; }}
a {{ color:var(--navy); }}
@media print {{ body {{ background:#fff; padding:0; }} section {{ border:none; padding:0; }} section::before {{ display:none; }} .grid {{ grid-template-columns:repeat(3, 1fr); }} }}
</style></head><body>
<header><h1>{title}</h1><p class="sub">{n} {selects}. Click a picture to open the full-size still. Print this page to save it as a PDF.</p>{log_note}</header>
{body}
</body></html>
"#,
        title = esc(name),
        n = entries.len(),
        selects = if entries.len() == 1 { "select" } else { "selects" },
    )
}
