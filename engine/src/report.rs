//! A report on the whole shoot: what came out of the footage, and patterns
//! in how it was shot that cost usable moments, from the scores and the
//! camera's own settings. Every finding is plain counting and averaging;
//! one is only made when enough clips back it up.

use crate::project::Project;
use crate::selection::{ClipSelection, SegmentStatus};
use crate::solar::Light;
use crate::tone::ExposureIssue;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Tone {
    /// Something that went well.
    Good,
    /// Something that cost usable footage.
    Warn,
    /// Worth knowing, neither good nor bad.
    Info,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Finding {
    pub tone: Tone,
    /// Short heading, sentence case.
    pub title: String,
    /// One or two sentences: what was found and what it means.
    pub body: String,
    /// A row per group compared, for findings that compare groups: label, clips, value text.
    #[serde(default)]
    pub rows: Vec<(String, usize, String)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct ShootReport {
    pub clips: usize,
    pub clips_analyzed: usize,
    pub footage_seconds: f64,
    pub selects: usize,
    pub selects_seconds: f64,
    pub findings: Vec<Finding>,
}

/// Fewest clips a group needs before it is compared with another.
pub const MIN_GROUP: usize = 3;

fn pct(v: f64) -> String {
    format!("{}%", (v * 100.0).round())
}

fn mean(v: &[f32]) -> f32 {
    if v.is_empty() {
        0.0
    } else {
        v.iter().sum::<f32>() / v.len() as f32
    }
}

fn dur(s: f64) -> String {
    if s < 60.0 {
        format!("{} s", s.round())
    } else if s < 3600.0 {
        format!("{} min", (s / 60.0).round())
    } else {
        format!("{} h {:02} min", (s / 3600.0).floor(), ((s % 3600.0) / 60.0).round())
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

struct ClipFacts {
    name: String,
    duration: f64,
    kept_seconds: f64,
    stability: f32,
    focus: f32,
    /// Share of samples whose main exposure problem is each issue.
    issue_share: [f32; 6],
    iso: Option<f32>,
    shutter: Option<f64>,
    camera: Option<String>,
    fps: f64,
    light: Option<Light>,
    crooked: Vec<f32>,
    flicker: bool,
    jello: bool,
    wind: bool,
    clipped_sound: bool,
    dropout: bool,
    choppy: bool,
    wb_note: bool,
    repeats: usize,
}

fn facts(project: &Project, sels: &[ClipSelection], scenes: &[crate::scene::ClipScene]) -> Vec<ClipFacts> {
    let mut out = Vec::new();
    for ((c, sel), sc) in project.clips.iter().zip(sels).zip(scenes) {
        let (Some(info), Some(a)) = (c.info.as_ref(), c.analysis.as_ref()) else { continue };
        if !a.complete || a.frames.is_empty() {
            continue;
        }
        let Some(scores) = c.scores(project.fallback_profile) else { continue };
        let mut issue_share = [0f32; 6];
        for &i in &scores.issue {
            issue_share[(i as usize).min(5)] += 1.0;
        }
        for v in issue_share.iter_mut() {
            *v /= scores.issue.len().max(1) as f32;
        }
        let segs = &sel.segments;
        let any = |f: &dyn Fn(&crate::selection::Segment) -> bool| segs.iter().filter(|g| g.status != SegmentStatus::BelowThreshold).any(f);
        let sound = a.sound.as_ref().map(|s| &s.summary);
        out.push(ClipFacts {
            name: info.name.clone(),
            duration: info.duration,
            kept_seconds: sel.kept_seconds(),
            stability: mean(&scores.stability),
            focus: mean(&scores.focus),
            issue_share,
            iso: info.camera.as_ref().and_then(|n| n.iso),
            shutter: info.camera.as_ref().and_then(|n| n.shutter),
            camera: info.camera.as_ref().and_then(|n| n.camera.clone()),
            fps: info.rate.fps(),
            light: sc.light,
            crooked: segs.iter().filter(|g| g.status == SegmentStatus::Kept && g.crooked()).filter_map(|g| g.roll).collect(),
            flicker: any(&|g| g.flickers()),
            jello: any(&|g| g.jello()),
            wind: sound.map(|s| s.wind_share >= 0.2).unwrap_or(false),
            clipped_sound: sound.map(|s| s.clipped_events >= 3).unwrap_or(false),
            dropout: sound.map(|s| !s.dropouts.is_empty()).unwrap_or(false),
            choppy: info.camera.as_ref().and_then(|n| n.shutter_warning(info.rate.fps())).is_some(),
            wb_note: sc.wb_note.is_some(),
            repeats: segs.iter().filter(|g| g.similar_to.is_some()).count(),
        });
    }
    out
}

/// Clips grouped by a label, keeping groups with enough clips.
fn grouped<'a>(f: &'a [ClipFacts], label: impl Fn(&ClipFacts) -> Option<String>) -> Vec<(String, Vec<&'a ClipFacts>)> {
    let mut g: Vec<(String, Vec<&ClipFacts>)> = Vec::new();
    for c in f {
        if let Some(l) = label(c) {
            match g.iter_mut().find(|x| x.0 == l) {
                Some(x) => x.1.push(c),
                None => g.push((l, vec![c])),
            }
        }
    }
    g.retain(|x| x.1.len() >= MIN_GROUP);
    g
}

fn shutter_bucket(s: f64, fps: f64) -> String {
    // In terms of the 180 degree rule: twice the frame rate.
    let ratio = (1.0 / s) / (2.0 * fps.max(1.0));
    if ratio < 0.8 {
        "Slower than 180 degrees".into()
    } else if ratio <= 1.6 {
        "Around 180 degrees".into()
    } else {
        "Faster than 180 degrees".into()
    }
}

/// The report for the project as it stands.
pub fn shoot_report(project: &Project) -> ShootReport {
    let scenes = crate::scene::resolve(project);
    let sels = project.selections_with(Some(&scenes));
    let f = facts(project, &sels, &scenes);
    let mut r = ShootReport {
        clips: project.clips.len(),
        clips_analyzed: f.len(),
        footage_seconds: f.iter().map(|c| c.duration).sum(),
        selects: sels.iter().map(|s| s.kept().count()).sum(),
        selects_seconds: sels.iter().map(|s| s.kept_seconds()).sum(),
        findings: Vec::new(),
    };
    if f.is_empty() {
        return r;
    }
    let total: f64 = f.iter().map(|c| c.duration).sum();
    let kept: f64 = f.iter().map(|c| c.kept_seconds).sum();
    let empty = f.iter().filter(|c| c.kept_seconds <= 0.0).count();
    r.findings.push(Finding {
        tone: Tone::Info,
        title: "What came out of the footage".into(),
        body: format!(
            "{} of {} became selects ({}).{}",
            dur(kept),
            dur(total),
            pct(kept / total.max(1e-9)),
            if empty > 0 { format!(" {} gave nothing usable at these settings.", plural(empty, "clip", "clips")) } else { String::new() }
        ),
        rows: vec![],
    });

    // Stability against shutter speed.
    let by_shutter = grouped(&f, |c| c.shutter.map(|s| shutter_bucket(s, c.fps)));
    if by_shutter.len() >= 2 {
        let mut rows: Vec<(String, usize, String, f32)> = by_shutter
            .iter()
            .map(|(l, cs)| {
                let m = mean(&cs.iter().map(|c| c.stability).collect::<Vec<_>>());
                (l.clone(), cs.len(), format!("stability {}", (m * 100.0).round()), m)
            })
            .collect();
        rows.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));
        let spread = rows.first().unwrap().3 - rows.last().unwrap().3;
        if spread >= 0.1 {
            r.findings.push(Finding {
                tone: Tone::Warn,
                title: "Shutter speed and shake".into(),
                body: format!(
                    "Clips shot {} averaged {} points steadier than clips shot {}. Shake shows less with motion blur, so a slower shutter (with an ND filter in bright light) hides small movements.",
                    rows[0].0.to_lowercase(),
                    (spread * 100.0).round(),
                    rows.last().unwrap().0.to_lowercase()
                ),
                rows: rows.into_iter().map(|(a, b, c, _)| (a, b, c)).collect(),
            });
        }
    }

    // Stability by camera.
    let by_camera = grouped(&f, |c| c.camera.clone());
    if by_camera.len() >= 2 {
        let mut rows: Vec<(String, usize, String, f32)> = by_camera
            .iter()
            .map(|(l, cs)| {
                let m = mean(&cs.iter().map(|c| c.stability).collect::<Vec<_>>());
                let kept: f64 = cs.iter().map(|c| c.kept_seconds).sum::<f64>() / cs.iter().map(|c| c.duration).sum::<f64>().max(1e-9);
                (l.clone(), cs.len(), format!("stability {}, {} kept", (m * 100.0).round(), pct(kept)), m)
            })
            .collect();
        rows.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));
        r.findings.push(Finding {
            tone: Tone::Info,
            title: "By camera".into(),
            body: format!("{} gave the steadiest footage; {} the shakiest.", rows[0].0, rows.last().unwrap().0),
            rows: rows.into_iter().map(|(a, b, c, _)| (a, b, c)).collect(),
        });
    }

    // Clipped highlights, and the ISO they were shot at.
    let clipped: Vec<&ClipFacts> = f.iter().filter(|c| c.issue_share[ExposureIssue::ClippedHighlights.code() as usize] >= 0.3).collect();
    if !clipped.is_empty() {
        let low_iso = clipped.iter().filter(|c| c.iso.map(|i| i <= 200.0).unwrap_or(false)).count();
        let extra = if low_iso * 2 >= clipped.len() && low_iso >= 2 {
            format!(" {low_iso} of them were already at ISO 200 or lower, so the light was simply too bright for the aperture and shutter: an ND filter or a smaller aperture keeps the highlights.")
        } else {
            " Clipped highlights cannot be brought back in the grade; expose a little darker in bright scenes.".into()
        };
        r.findings.push(Finding {
            tone: Tone::Warn,
            title: "Clipped highlights".into(),
            body: format!("{} {} clipped highlights for much of their length.{extra}", plural(clipped.len(), "clip", "clips"), if clipped.len() == 1 { "has" } else { "have" }),
            rows: vec![],
        });
    }
    let under: Vec<&ClipFacts> = f.iter().filter(|c| c.issue_share[ExposureIssue::Underexposed.code() as usize] + c.issue_share[ExposureIssue::DeepShadows.code() as usize] >= 0.4).collect();
    if !under.is_empty() {
        let night = under.iter().filter(|c| matches!(c.light, Some(Light::Night | Light::BlueHour))).count();
        r.findings.push(Finding {
            tone: if night * 2 > under.len() { Tone::Info } else { Tone::Warn },
            title: "Dark footage".into(),
            body: format!(
                "{} {} underexposed or lost in the shadows for much of their length.{}",
                plural(under.len(), "clip", "clips"),
                if under.len() == 1 { "is" } else { "are" },
                if night > 0 { format!(" {night} of them were shot at night or in the blue hour, where that may be what you wanted.") } else { " Brightening it in the grade brings up noise.".into() }
            ),
            rows: vec![],
        });
    }

    // Light: share of footage against share of selects.
    let lit: Vec<&ClipFacts> = f.iter().filter(|c| c.light.is_some()).collect();
    if lit.len() >= MIN_GROUP {
        let mut rows = Vec::new();
        let foot: f64 = lit.iter().map(|c| c.duration).sum::<f64>().max(1e-9);
        let sel: f64 = lit.iter().map(|c| c.kept_seconds).sum::<f64>().max(1e-9);
        let mut best: Option<(Light, f64)> = None;
        for l in [Light::GoldenHour, Light::BlueHour, Light::Daylight, Light::HarshMidday, Light::Night] {
            let cs: Vec<&&ClipFacts> = lit.iter().filter(|c| c.light == Some(l)).collect();
            if cs.is_empty() {
                continue;
            }
            let fs = cs.iter().map(|c| c.duration).sum::<f64>() / foot;
            let ss = cs.iter().map(|c| c.kept_seconds).sum::<f64>() / sel;
            rows.push((l.words().to_string(), cs.len(), format!("{} of footage, {} of selects", pct(fs), pct(ss))));
            let lift = if fs > 0.0 { ss / fs } else { 0.0 };
            if cs.len() >= 2 && best.map_or(true, |b| lift > b.1) {
                best = Some((l, lift));
            }
        }
        let body = match best {
            Some((l, lift)) if lift >= 1.2 => format!("{} footage made it into the selects more often than its share of the shoot: it was {:.1} times as likely to be kept.", l.words(), lift),
            _ => "The light was worked out from when and where each clip was recorded.".into(),
        };
        r.findings.push(Finding { tone: Tone::Info, title: "Light".into(), body, rows });
    }

    let crooked: Vec<f32> = f.iter().flat_map(|c| c.crooked.iter().copied()).collect();
    if !crooked.is_empty() {
        let right = crooked.iter().filter(|r| **r > 0.0).count();
        let lean = if right * 4 >= crooked.len() * 3 {
            " Almost all lean down to the right, which points at the camera or gimbal rather than the moment: check its level or recalibrate it."
        } else if right * 4 <= crooked.len() {
            " Almost all lean down to the left, which points at the camera or gimbal rather than the moment: check its level or recalibrate it."
        } else {
            ""
        };
        r.findings.push(Finding {
            tone: Tone::Warn,
            title: "Crooked horizons".into(),
            body: format!("{} {} a horizon at least 1 degree off level, {:.1} degrees on average. Each is easy to fix with a small rotation in the edit.{lean}", plural(crooked.len(), "select", "selects"), if crooked.len() == 1 { "has" } else { "have" }, mean(&crooked.iter().map(|r| r.abs()).collect::<Vec<_>>())),
            rows: vec![],
        });
    }
    let flick = f.iter().filter(|c| c.flicker).count();
    if flick > 0 {
        r.findings.push(Finding {
            tone: Tone::Warn,
            title: "Light flicker".into(),
            body: format!("{} {} flicker bands from lights on mains power. A shutter of 1/50 or 1/100 avoids them where the mains runs at 50 Hz (Europe, most of Asia, Africa and Australia), 1/60 or 1/120 where it runs at 60 Hz (most of the Americas).", plural(flick, "clip", "clips"), if flick == 1 { "shows" } else { "show" }),
            rows: vec![],
        });
    }
    let jello = f.iter().filter(|c| c.jello).count();
    if jello > 0 {
        let drone = f.iter().filter(|c| c.jello && c.camera.as_deref().map(|n| n.contains("DJI")).unwrap_or(false)).count();
        r.findings.push(Finding {
            tone: Tone::Warn,
            title: "Jello and vibration".into(),
            body: format!(
                "{} {} the picture wobble within the frame.{}",
                plural(jello, "clip", "clips"),
                if jello == 1 { "shows" } else { "show" },
                if drone > 0 { format!(" {drone} of them are drone clips: check the propellers for damage and balance, and the gimbal dampers.") } else { " Check the mount for loose parts and add dampening.".into() }
            ),
            rows: vec![],
        });
    }
    let choppy = f.iter().filter(|c| c.choppy).count();
    if choppy > 0 {
        r.findings.push(Finding {
            tone: Tone::Warn,
            title: "Shutter too fast".into(),
            body: format!("{} recorded with a shutter far faster than the frame rate suits, so movement in them looks choppy. An ND filter lets you keep the shutter near twice the frame rate in bright light.", plural(choppy, "clip was", "clips were")),
            rows: vec![],
        });
    }
    let (wind, clip, drop) = (f.iter().filter(|c| c.wind).count(), f.iter().filter(|c| c.clipped_sound).count(), f.iter().filter(|c| c.dropout).count());
    if wind + clip + drop > 0 {
        let mut bits = Vec::new();
        if wind > 0 {
            bits.push(format!("wind noise in {}", plural(wind, "clip", "clips")));
        }
        if clip > 0 {
            bits.push(format!("clipped sound in {}", plural(clip, "clip", "clips")));
        }
        if drop > 0 {
            bits.push(format!("sound dropping out in {}", plural(drop, "clip", "clips")));
        }
        let mut advice = Vec::new();
        if wind > 0 {
            advice.push("a furry windshield fixes wind");
        }
        if clip > 0 {
            advice.push("lower the recording level or use a 32-bit float recorder for loud sound");
        }
        if drop > 0 {
            advice.push("dropouts usually mean a wireless mic losing its link or a loose cable");
        }
        r.findings.push(Finding {
            tone: Tone::Warn,
            title: "Sound".into(),
            body: format!("Found {}. For next time: {}.", bits.join(", "), advice.join("; ")),
            rows: vec![],
        });
    }
    let wb = f.iter().filter(|c| c.wb_note).count();
    if wb > 0 {
        r.findings.push(Finding {
            tone: Tone::Info,
            title: "White balance".into(),
            body: format!("{} {} a different white balance from the rest of {} scene. Matching them before grading saves time; setting white balance by hand rather than automatic keeps a scene consistent.", plural(wb, "clip", "clips"), if wb == 1 { "has" } else { "have" }, if wb == 1 { "its" } else { "their" }),
            rows: vec![],
        });
    }
    let repeats: usize = f.iter().map(|c| c.repeats).sum();
    if repeats > 0 {
        r.findings.push(Finding {
            tone: Tone::Info,
            title: "Repeated takes".into(),
            body: format!("{} look like a better select in another clip, such as a second take from the same spot. Set aside repeated takes keeps only the best of each.", plural(repeats, "select", "selects")),
            rows: vec![],
        });
    }
    let soft: Vec<&ClipFacts> = f.iter().filter(|c| c.focus < 0.35).collect();
    if soft.len() >= 2 {
        r.findings.push(Finding {
            tone: Tone::Warn,
            title: "Soft footage".into(),
            body: format!("{} scored low on focus throughout, such as {}. Check them by eye: plain subjects like sky and fog also score low.", plural(soft.len(), "clip", "clips"), soft.iter().take(3).map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")),
            rows: vec![],
        });
    }
    let hand: usize = project.clips.iter().map(|c| c.marks.len()).sum();
    if hand > 0 {
        r.findings.push(Finding {
            tone: Tone::Info,
            title: "Your own calls".into(),
            body: format!("You made {} by hand. Fit settings to my edits, under Learn from your edits, learns from them.", plural(hand, "call", "calls")),
            rows: vec![],
        });
    }
    if r.findings.iter().all(|x| x.tone != Tone::Warn) && f.len() >= MIN_GROUP {
        r.findings.insert(1, Finding {
            tone: Tone::Good,
            title: "No shooting problems found".into(),
            body: "Nothing in the scores or the camera settings points at a habit that cost usable footage.".into(),
            rows: vec![],
        });
    }
    r
}

fn esc(s: &str) -> String {
    crate::export::xml::escape(s)
}

/// The report as a page to keep or share.
pub fn report_html(name: &str, r: &ShootReport) -> String {
    let mut items = String::new();
    for f in &r.findings {
        let (cls, word, icon) = match f.tone {
            Tone::Good => ("good", "Good", "<path d=\"M2.5 7.5l3 3 6-7\"/>"),
            Tone::Warn => ("warn", "Worth fixing", "<path d=\"M7 1.8l5.6 9.9H1.4z\"/><path d=\"M7 5.6v2.6M7 10.1v.1\"/>"),
            Tone::Info => ("info", "Note", "<path d=\"M3.2 7h7.6\"/>"),
        };
        let rows = if f.rows.is_empty() {
            String::new()
        } else {
            let tr: String = f.rows.iter().map(|(a, n, v)| format!("<tr><td>{}</td><td class=\"num\">{}</td><td>{}</td></tr>", esc(a), n, esc(v))).collect();
            format!("<table><thead><tr><th>Group</th><th class=\"num\">Clips</th><th>Result</th></tr></thead><tbody>{tr}</tbody></table>")
        };
        items.push_str(&format!(
            "<article><span class=\"pill {cls}\"><svg viewBox=\"0 0 14 14\" aria-hidden=\"true\">{icon}</svg>{word}</span><h2>{}</h2><p>{}</p>{rows}</article>",
            esc(&f.title),
            esc(&f.body)
        ));
    }
    format!(
        r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} shoot report</title>
<style>
:root {{ --ground:#FAF8F3; --card:#FFFFFF; --line:#E4E0D6; --ink:#1A2432; --muted:#5E6878; }}
* {{ box-sizing:border-box; }}
body {{ margin:0; background:var(--ground); color:var(--ink); font:16px/1.6 Inter, system-ui, sans-serif; padding:32px 16px 48px; }}
main {{ max-width:760px; margin:0 auto; }}
h1 {{ font-family:"Averia Serif Libre", Georgia, serif; font-weight:400; font-size:30px; margin:0 0 4px; }}
.sub {{ color:var(--muted); margin:0 0 20px; font-family:"Inter Tight", Inter, sans-serif; font-variant-numeric:tabular-nums; }}
article {{ background:var(--card); border:1px solid var(--line); border-radius:12px; padding:16px 18px; margin-bottom:14px; position:relative; }}
article::before {{ content:""; position:absolute; left:0; right:0; top:0; height:3px; border-radius:12px 12px 0 0; background:linear-gradient(90deg,#C9A227,rgba(201,162,39,0)); }}
h2 {{ font:600 18px/1.4 "Inter Tight", Inter, sans-serif; margin:8px 0 4px; }}
p {{ margin:0; }}
.pill {{ display:inline-flex; align-items:center; gap:6px; font-size:11.5px; letter-spacing:.06em; text-transform:uppercase; border-radius:999px; padding:2px 10px; }}
.pill svg {{ width:12px; height:12px; fill:none; stroke:currentColor; stroke-width:1.7; stroke-linecap:round; stroke-linejoin:round; }}
.good {{ color:#0F5C4E; background:#E7F1EE; }} .warn {{ color:#9A5B00; background:#FDF2E2; }} .info {{ color:#7A8798; background:#EFF2F4; }}
table {{ width:100%; border-collapse:collapse; margin-top:10px; font-size:14.5px; }}
th {{ text-align:left; font-size:11.5px; letter-spacing:.08em; text-transform:uppercase; color:var(--muted); background:#FBFCFD; padding:8px 10px; border-bottom:1px solid var(--line); }}
td {{ padding:8px 10px; border-bottom:1px solid var(--line); }}
.num {{ text-align:right; font-family:"Inter Tight", Inter, sans-serif; font-variant-numeric:tabular-nums; }}
</style></head><body><main>
<h1>{title}</h1>
<p class="sub">{clips} clips, {footage} of footage, {selects} selects ({sel_dur})</p>
{items}
</main></body></html>
"#,
        title = esc(name),
        clips = r.clips,
        footage = dur(r.footage_seconds),
        selects = r.selects,
        sel_dur = dur(r.selects_seconds),
    )
}
