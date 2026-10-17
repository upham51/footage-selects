//! CMX3600 edit decision list of the selects.
//!
//! An EDL holds one video track, so it lists the selects only, at the same
//! record positions as track 2 of the XML timelines. Each event has the
//! source clip name and path as comments and an Avid-style locator (* LOC)
//! carrying the select's color and scores.
//!
//! Source timecode is the clip's own timecode when the clip runs at the
//! timeline rate. When it does not, a CMX3600 list cannot describe it exactly
//! (different source and record lengths would read as a speed change), so
//! source times are written at the timeline rate and the event says so.

use super::{Tier, Timeline};
use crate::rate::frames_to_timecode;

fn locator_color(t: Tier) -> &'static str {
    match t {
        Tier::Strong => "GREEN",
        Tier::Good => "BLUE",
        Tier::Fair => "YELLOW",
    }
}

/// Comment text cannot contain line breaks.
fn one_line(s: &str) -> String {
    s.replace(['\r', '\n'], " ")
}

pub fn write(tl: &Timeline) -> String {
    let r = tl.rate;
    let drop = tl.drop_frame && r.supports_drop_frame();
    let mut out = String::new();
    let title: String = one_line(&tl.name).chars().take(70).collect();
    out.push_str(&format!("TITLE: {title}\r\n"));
    out.push_str(if drop { "FCM: DROP FRAME\r\n" } else { "FCM: NON-DROP FRAME\r\n" });
    out.push_str("\r\n");
    for (i, p) in tl.selects.iter().enumerate() {
        let s = &tl.sources[p.source];
        let m = p.select.as_ref().unwrap();
        let (src_in, src_out) = super::source_timecodes_at_timeline_rate(tl, p);
        let rec_in = frames_to_timecode(tl.start_frame + p.record_in, r, drop);
        let rec_out = frames_to_timecode(tl.start_frame + p.record_out(), r, drop);
        out.push_str(&format!(
            "{:03}  AX       V     C        {src_in} {src_out} {rec_in} {rec_out}\r\n",
            (i + 1) % 1000
        ));
        out.push_str(&format!("* FROM CLIP NAME: {}\r\n", one_line(&s.info.name)));
        out.push_str(&format!("* SOURCE FILE: {}\r\n", one_line(&s.info.path)));
        if !s.same_rate_as(r) {
            out.push_str(&format!(
                "* NOTE: SOURCE IS {} FPS. SOURCE TIMECODE IS GIVEN AT THE TIMELINE RATE OF {} FPS.\r\n",
                s.native_rate().label(),
                r.label()
            ));
        }
        out.push_str(&format!(
            "* LOC: {rec_in} {:<7} {}. {}\r\n",
            locator_color(m.tier),
            m.marker_name(),
            m.marker_note()
        ));
        out.push_str("\r\n");
    }
    out
}
