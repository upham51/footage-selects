//! FCPXML 1.9 for Final Cut Pro (10.5 and later) and DaVinci Resolve.
//!
//! Layout, following Apple's FCPXML reference:
//! resources (formats, assets with media-rep) > library > event > project >
//! sequence > spine. Times are rational seconds such as "1001/24000s".
//! Offsets and durations are whole timeline frames; `start` values are whole
//! frames of the clip's own rate, counted from its start timecode.
//!
//! Full clips sit on the primary storyline. Each select is a connected clip
//! on lane 1 above its source, carrying a marker with its scores. With the
//! selects-only layout the storyline is one gap and the selects connect to it.

use super::xml::Xml;
use super::{file_url, Placed, Tier, Timeline};
use crate::project::TimelineLayout;
use crate::rate::Rate;
use std::collections::HashMap;

/// An exact time in seconds as a fraction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Time {
    pub num: i128,
    pub den: i128,
}

fn gcd(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a.max(1)
}

impl Time {
    pub fn new(num: i128, den: i128) -> Time {
        let g = gcd(num, den);
        Time {
            num: num / g,
            den: den / g,
        }
    }
    /// `frames` frames at `rate`.
    pub fn frames(frames: i64, rate: Rate) -> Time {
        Time::new(frames as i128 * rate.den as i128, rate.num as i128)
    }
    pub fn add(self, o: Time) -> Time {
        Time::new(self.num * o.den + o.num * self.den, self.den * o.den)
    }
    pub fn fcp(self) -> String {
        if self.num == 0 {
            "0s".into()
        } else if self.den == 1 {
            format!("{}s", self.num)
        } else {
            format!("{}/{}s", self.num, self.den)
        }
    }
    /// Parse "1001/24000s" or "3600s".
    pub fn parse(s: &str) -> Option<Time> {
        let s = s.strip_suffix('s')?;
        match s.split_once('/') {
            Some((n, d)) => Some(Time::new(n.parse().ok()?, d.parse().ok()?)),
            None => Some(Time::new(s.parse().ok()?, 1)),
        }
    }
    /// True when this time is a whole number of frames at `rate`.
    pub fn on_frame(self, rate: Rate) -> bool {
        // time * fps must be an integer: num * rate.num / (den * rate.den).
        (self.num * rate.num as i128) % (self.den * rate.den as i128) == 0
    }
}

fn frame_duration(rate: Rate) -> String {
    Time::frames(1, rate).fcp()
}

fn tc_format(drop: bool, rate: Rate) -> &'static str {
    if drop && rate.supports_drop_frame() {
        "DF"
    } else {
        "NDF"
    }
}

fn audio_rate_label(hz: u32) -> &'static str {
    match hz {
        32000 => "32k",
        44100 => "44.1k",
        88200 => "88.2k",
        96000 => "96k",
        _ => "48k",
    }
}

/// Marker kinds give color in Final Cut Pro: a completed to-do is green, a
/// standard marker blue, an open to-do red. The tier word is in the name too.
fn marker_attrs(tier: Tier) -> Option<&'static str> {
    match tier {
        Tier::Strong => Some("1"),
        Tier::Good => None,
        Tier::Fair => Some("0"),
    }
}

pub fn write(tl: &Timeline) -> String {
    let r = tl.rate;
    let mut x = Xml::new("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE fcpxml>\n");
    x.open("fcpxml", &[("version", "1.9".into())]);

    // Resources: the timeline format, one format per distinct source format, and assets.
    x.open("resources", &[]);
    x.empty(
        "format",
        &[
            ("id", "r1".into()),
            ("frameDuration", frame_duration(r)),
            ("width", tl.width.to_string()),
            ("height", tl.height.to_string()),
            ("colorSpace", "1-1-1 (Rec. 709)".into()),
        ],
    );
    let mut formats: HashMap<(Rate, u32, u32), String> = HashMap::new();
    formats.insert((r, tl.width, tl.height), "r1".into());
    let mut next = 2;
    let mut format_of = Vec::with_capacity(tl.sources.len());
    for s in &tl.sources {
        let key = (s.info.rate, s.info.width, s.info.height);
        let id = match formats.get(&key) {
            Some(id) => id.clone(),
            None => {
                let id = format!("r{next}");
                next += 1;
                x.empty(
                    "format",
                    &[
                        ("id", id.clone()),
                        ("frameDuration", frame_duration(s.info.rate)),
                        ("width", s.info.width.to_string()),
                        ("height", s.info.height.to_string()),
                        ("colorSpace", "1-1-1 (Rec. 709)".into()),
                    ],
                );
                formats.insert(key, id.clone());
                id
            }
        };
        format_of.push(id);
    }
    let mut asset_ids = Vec::with_capacity(tl.sources.len());
    for (i, s) in tl.sources.iter().enumerate() {
        let id = format!("r{next}");
        next += 1;
        let native = s.info.rate;
        let mut attrs = vec![
            ("id", id.clone()),
            ("name", s.info.name.clone()),
            ("start", Time::frames(s.tc_frames, native).fcp()),
            ("duration", Time::frames(s.frames_native, native).fcp()),
            ("hasVideo", "1".into()),
            ("format", format_of[i].clone()),
            ("videoSources", "1".into()),
        ];
        if s.info.has_audio {
            attrs.push(("hasAudio", "1".into()));
            attrs.push(("audioSources", "1".into()));
            attrs.push(("audioChannels", s.info.audio_channels.max(1).to_string()));
            attrs.push((
                "audioRate",
                (if s.info.audio_sample_rate > 0 { s.info.audio_sample_rate } else { 48000 }).to_string(),
            ));
        }
        x.open("asset", &attrs);
        x.empty(
            "media-rep",
            &[
                ("kind", "original-media".into()),
                ("src", file_url(&s.info.path, false)),
            ],
        );
        x.close();
        asset_ids.push(id);
    }
    x.close(); // resources

    let tc_start = Time::frames(tl.start_frame, r);
    let total = Time::frames(tl.duration, r);
    x.open("library", &[]);
    x.open("event", &[("name", crate::PRODUCT_NAME.into())]);
    x.open("project", &[("name", tl.name.clone())]);
    x.open(
        "sequence",
        &[
            ("format", "r1".into()),
            ("duration", total.fcp()),
            ("tcStart", tc_start.fcp()),
            ("tcFormat", tc_format(tl.drop_frame, r).into()),
            ("audioLayout", "stereo".into()),
            ("audioRate", audio_rate_label(48000).into()),
        ],
    );
    x.open("spine", &[]);

    // Selects grouped under the spine item they connect to.
    let connected = |x: &mut Xml, p: &Placed, parent_start: Time, parent_record_in: i64, video_only: bool| {
        let s = &tl.sources[p.source];
        let m = p.select.as_ref().unwrap();
        let start = Time::frames(s.tc_frames, s.info.rate).add(Time::frames(p.source_in_native, s.info.rate));
        let offset = parent_start.add(Time::frames(p.record_in - parent_record_in, r));
        let mut attrs = vec![
            ("ref", asset_ids[p.source].clone()),
            ("lane", "1".into()),
            ("offset", offset.fcp()),
            ("name", format!("{} (select {})", s.info.name, m.number)),
            ("start", start.fcp()),
            ("duration", Time::frames(p.duration, r).fcp()),
            ("tcFormat", tc_format(s.tc_drop, s.info.rate).into()),
        ];
        if video_only && s.info.has_audio {
            attrs.push(("srcEnable", "video".into()));
        }
        x.open("asset-clip", &attrs);
        let mut marker = vec![
            ("start", start.fcp()),
            ("duration", Time::frames(1, r).fcp()),
            ("value", m.marker_name()),
            ("note", m.marker_note()),
        ];
        if let Some(c) = marker_attrs(m.tier) {
            marker.push(("completed", c.into()));
        }
        x.empty("marker", &marker);
        x.close();
    };

    match tl.layout {
        TimelineLayout::FullClipsBelow => {
            for base in &tl.base {
                let s = &tl.sources[base.source];
                let start = Time::frames(s.tc_frames, s.info.rate);
                let mut attrs = vec![
                    ("ref", asset_ids[base.source].clone()),
                    ("offset", tc_start.add(Time::frames(base.record_in, r)).fcp()),
                    ("name", s.info.name.clone()),
                    ("start", start.fcp()),
                    ("duration", Time::frames(base.duration, r).fcp()),
                    ("tcFormat", tc_format(s.tc_drop, s.info.rate).into()),
                ];
                if s.info.has_audio {
                    attrs.push(("audioRole", "dialogue".into()));
                }
                let children: Vec<&Placed> = tl
                    .selects
                    .iter()
                    .filter(|p| p.source == base.source && p.record_in >= base.record_in && p.record_in < base.record_out())
                    .collect();
                if children.is_empty() {
                    x.empty("asset-clip", &attrs);
                } else {
                    x.open("asset-clip", &attrs);
                    for p in children {
                        connected(&mut x, p, start, base.record_in, true);
                    }
                    x.close();
                }
            }
        }
        TimelineLayout::SelectsOnly => {
            x.open(
                "gap",
                &[
                    ("name", "Gap".into()),
                    ("offset", tc_start.fcp()),
                    ("start", tc_start.fcp()),
                    ("duration", total.fcp()),
                ],
            );
            for p in &tl.selects {
                connected(&mut x, p, tc_start, 0, false);
            }
            x.close();
        }
    }

    x.close(); // spine
    x.close(); // sequence
    x.close(); // project
    x.close(); // event
    x.close(); // library
    x.close(); // fcpxml
    x.finish()
}
