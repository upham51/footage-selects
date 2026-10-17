//! Final Cut Pro 7 XML (xmeml version 4), the interchange format read by
//! Premiere Pro and DaVinci Resolve.
//!
//! Structure follows Apple's "Final Cut Pro XML Interchange Format":
//! sequence > media > video > track > clipitem, with every clipitem time in
//! sequence frames. `start`/`end` are timeline positions, `in`/`out` are
//! positions in the media counted from its first frame. Rates are written as
//! an integer timebase plus an NTSC flag (23.976 is timebase 24, NTSC TRUE).

use super::xml::Xml;
use super::{file_url, Placed, Tier, Timeline};
use crate::project::TimelineLayout;
use crate::rate::{frames_to_timecode, Rate};

fn rate(x: &mut Xml, r: Rate) {
    x.open("rate", &[]);
    x.text("timebase", r.timebase());
    x.text("ntsc", if r.ntsc() { "TRUE" } else { "FALSE" });
    x.close();
}

fn timecode(x: &mut Xml, r: Rate, frames: i64, drop: bool) {
    let drop = drop && r.supports_drop_frame();
    x.open("timecode", &[]);
    rate(x, r);
    x.text("string", frames_to_timecode(frames, r, drop));
    x.text("frame", frames);
    x.text("displayformat", if drop { "DF" } else { "NDF" });
    x.close();
}

/// FCP 7 label names, which Premiere Pro maps to its label colors.
pub fn label_for(tier: Tier) -> &'static str {
    match tier {
        Tier::Strong => "Forest",
        Tier::Good => "Cerulean",
        Tier::Fair => "Mango",
    }
}

/// Marker color as Premiere Pro writes it (ARGB as a decimal integer).
pub fn ppro_color(tier: Tier) -> u32 {
    match tier {
        Tier::Strong => 0xFF2EA043, // green
        Tier::Good => 0xFF1F7AE0,   // blue
        Tier::Fair => 0xFFE3B341,   // yellow
    }
}

fn file_element(x: &mut Xml, tl: &Timeline, si: usize, written: &mut [bool]) {
    let id = format!("file-{}", si + 1);
    if written[si] {
        x.empty("file", &[("id", id)]);
        return;
    }
    written[si] = true;
    let s = &tl.sources[si];
    let native = s.native_rate();
    x.open("file", &[("id", id)]);
    x.text("name", &s.info.name);
    x.text("pathurl", file_url(&s.info.path, true));
    rate(x, native);
    x.text("duration", s.frames_native);
    timecode(x, native, s.tc_frames, s.tc_drop);
    x.open("media", &[]);
    x.open("video", &[]);
    x.open("samplecharacteristics", &[]);
    rate(x, native);
    x.text("width", s.info.width);
    x.text("height", s.info.height);
    x.text("anamorphic", "FALSE");
    x.text("pixelaspectratio", "square");
    x.text("fielddominance", "none");
    x.close();
    x.close();
    if s.info.has_audio {
        x.open("audio", &[]);
        x.open("samplecharacteristics", &[]);
        x.text("depth", 16);
        x.text("samplerate", if s.info.audio_sample_rate > 0 { s.info.audio_sample_rate } else { 48000 });
        x.close();
        x.text("channelcount", s.info.audio_channels.max(1));
        x.close();
    }
    x.close();
    x.close();
}

#[allow(clippy::too_many_arguments)]
fn clipitem(
    x: &mut Xml,
    tl: &Timeline,
    p: &Placed,
    id: &str,
    written: &mut [bool],
    audio_track: Option<u32>,
) {
    let s = &tl.sources[p.source];
    x.open("clipitem", &[("id", id.to_string())]);
    let name = match &p.select {
        Some(m) => format!("{} (select {})", s.info.name, m.number),
        None => s.info.name.clone(),
    };
    x.text("name", name);
    x.text("enabled", "TRUE");
    x.text("duration", s.frames_timeline);
    rate(x, tl.rate);
    x.text("start", p.record_in);
    x.text("end", p.record_out());
    x.text("in", p.source_in);
    x.text("out", p.source_out());
    file_element(x, tl, p.source, written);
    if let Some(track) = audio_track {
        x.open("sourcetrack", &[]);
        x.text("mediatype", "audio");
        x.text("trackindex", track);
        x.close();
    }
    if let Some(m) = &p.select {
        x.open("labels", &[]);
        x.text("label2", label_for(m.tier));
        x.close();
        x.open("comments", &[]);
        x.text("mastercomment1", m.marker_note());
        x.close();
    }
    x.close();
}

fn audio_tracks(x: &mut Xml, tl: &Timeline, items: &[Placed], written: &mut [bool]) {
    // The clips that carry sound: the full clips when they are on track 1,
    // otherwise the selects. Stereo sources use two mono tracks, as Premiere Pro does.
    let channels = items
        .iter()
        .map(|p| tl.sources[p.source].info.audio_channels.min(2))
        .max()
        .unwrap_or(0);
    for ch in 1..=channels.max(1) {
        x.open("track", &[]);
        for (i, p) in items.iter().enumerate() {
            let s = &tl.sources[p.source];
            if !s.info.has_audio || s.info.audio_channels < ch {
                continue;
            }
            clipitem(x, tl, p, &format!("clipitem-a{ch}-{}", i + 1), written, Some(ch));
        }
        x.close();
    }
}

pub fn write(tl: &Timeline) -> String {
    let mut x = Xml::new("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE xmeml>\n");
    let mut written = vec![false; tl.sources.len()];
    x.open("xmeml", &[("version", "4".into())]);
    x.open("sequence", &[("id", "sequence-1".into())]);
    x.text("name", &tl.name);
    x.text("duration", tl.duration);
    rate(&mut x, tl.rate);
    timecode(&mut x, tl.rate, tl.start_frame, tl.drop_frame);
    x.text("in", -1);
    x.text("out", -1);
    x.open("media", &[]);
    x.open("video", &[]);
    x.open("format", &[]);
    x.open("samplecharacteristics", &[]);
    rate(&mut x, tl.rate);
    x.text("width", tl.width);
    x.text("height", tl.height);
    x.text("anamorphic", "FALSE");
    x.text("pixelaspectratio", "square");
    x.text("fielddominance", "none");
    x.close();
    x.close();
    // Track 1: full clips, or empty when exporting selects only.
    x.open("track", &[]);
    for (i, p) in tl.base.iter().enumerate() {
        clipitem(&mut x, tl, p, &format!("clipitem-v1-{}", i + 1), &mut written, None);
    }
    x.text("enabled", "TRUE");
    x.text("locked", "FALSE");
    x.close();
    // Track 2: the selects.
    x.open("track", &[]);
    for (i, p) in tl.selects.iter().enumerate() {
        clipitem(&mut x, tl, p, &format!("clipitem-v2-{}", i + 1), &mut written, None);
    }
    x.text("enabled", "TRUE");
    x.text("locked", "FALSE");
    x.close();
    x.close(); // video
    x.open("audio", &[]);
    x.open("format", &[]);
    x.open("samplecharacteristics", &[]);
    x.text("depth", 16);
    x.text("samplerate", 48000);
    x.close();
    x.close();
    let sound = match tl.layout {
        TimelineLayout::FullClipsBelow => &tl.base,
        TimelineLayout::SelectsOnly => &tl.selects,
    };
    audio_tracks(&mut x, tl, sound, &mut written);
    x.close(); // audio
    x.close(); // media
    // One marker per select on the timeline, named and noted with its scores.
    for p in &tl.selects {
        let m = p.select.as_ref().unwrap();
        x.open("marker", &[]);
        x.text("comment", m.marker_note());
        x.text("name", m.marker_name());
        x.text("in", p.record_in);
        x.text("out", -1);
        x.text("pproColor", ppro_color(m.tier));
        x.close();
    }
    x.close(); // sequence
    x.close(); // xmeml
    x.finish()
}
