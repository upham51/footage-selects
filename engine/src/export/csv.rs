//! A spreadsheet of the selects, one row each.
//!
//! Source timecode is given twice. At the timeline rate it matches the EDL; at
//! the clip's own rate it matches what an editor shows for the clip itself, so a
//! 120 fps clip can have frame numbers above 23 there.

use super::{pct, Timeline};
use crate::rate::frames_to_timecode;

pub const HEADER: &[&str] = &[
    "Select",
    "Clip",
    "File",
    "Source in (timeline rate)",
    "Source out (timeline rate)",
    "Source in (clip rate)",
    "Source out (clip rate)",
    "Source in (seconds)",
    "Source out (seconds)",
    "Length (seconds)",
    "Timeline in",
    "Timeline out",
    "Track",
    "Focus",
    "Exposure",
    "Stability",
    "Overall",
    "Strength",
    "Clip frame rate",
    "Timeline frame rate",
    "Stars",
    "Kept by hand",
    "Scene",
    "Light",
    "Camera",
    "Keywords",
    "Problems",
    "Extra before (seconds)",
    "Extra after (seconds)",
];

pub fn field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) || s.starts_with([' ', '=', '+', '-', '@']) {
        // Quote, and neutralise leading characters spreadsheets treat as formulas.
        let body = s.replace('"', "\"\"");
        if s.starts_with(['=', '+', '-', '@']) {
            format!("\"'{body}\"")
        } else {
            format!("\"{body}\"")
        }
    } else {
        s.to_string()
    }
}

pub fn write(tl: &Timeline) -> String {
    let r = tl.rate;
    let drop = tl.drop_frame && r.supports_drop_frame();
    // A byte order mark lets Excel open the file as UTF-8.
    let mut out = String::from("\u{feff}");
    out.push_str(&HEADER.join(","));
    out.push_str("\r\n");
    for p in &tl.selects {
        let s = &tl.sources[p.source];
        let m = p.select.as_ref().unwrap();
        let native = s.native_rate();
        let src_in = s.tc_frames + native.seconds_to_frames(r.frames_to_seconds(p.source_in));
        let src_out = s.tc_frames + native.seconds_to_frames(r.frames_to_seconds(p.source_out()));
        let (tl_in, tl_out) = super::source_timecodes_at_timeline_rate(tl, p);
        let row = [
            m.number.to_string(),
            s.info.name.clone(),
            s.info.path.clone(),
            tl_in,
            tl_out,
            frames_to_timecode(src_in, native, s.tc_drop),
            frames_to_timecode(src_out, native, s.tc_drop),
            format!("{:.3}", r.frames_to_seconds(p.source_in)),
            format!("{:.3}", r.frames_to_seconds(p.source_out())),
            format!("{:.3}", r.frames_to_seconds(p.duration)),
            frames_to_timecode(tl.start_frame + p.record_in, r, drop),
            frames_to_timecode(tl.start_frame + p.record_out(), r, drop),
            "V2".to_string(),
            pct(m.focus).to_string(),
            pct(m.exposure).to_string(),
            pct(m.stability).to_string(),
            pct(m.score).to_string(),
            m.tier.word().to_string(),
            native.label(),
            r.label(),
            m.stars.to_string(),
            if m.manual { "Yes".into() } else { "No".into() },
            m.group.clone().unwrap_or_default(),
            m.light.map(|l| l.words().to_string()).unwrap_or_default(),
            match m.camera {
                crate::selection::CameraMove::Still if m.camera_move => "Moving".into(),
                c => c.words().to_string(),
            },
            m.keyword_line(),
            m.problems.join("; "),
            format!("{:.3}", r.frames_to_seconds(m.lead)),
            format!("{:.3}", r.frames_to_seconds(m.tail)),
        ];
        out.push_str(&row.iter().map(|f| field(f)).collect::<Vec<_>>().join(","));
        out.push_str("\r\n");
    }
    out
}
