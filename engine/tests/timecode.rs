//! Frame rate and timecode maths, including drop frame.

use selects_engine::probe::choose_rate;
use selects_engine::rate::{frames_to_timecode, timecode_to_frames, Rate, TIMELINE_RATES};

const R23976: Rate = Rate::new(24000, 1001);
const R24: Rate = Rate::new(24, 1);
const R25: Rate = Rate::new(25, 1);
const R2997: Rate = Rate::new(30000, 1001);
const R30: Rate = Rate::new(30, 1);
const R5994: Rate = Rate::new(60000, 1001);

#[test]
fn timebase_and_ntsc_flags() {
    assert_eq!((R23976.timebase(), R23976.ntsc()), (24, true));
    assert_eq!((R24.timebase(), R24.ntsc()), (24, false));
    assert_eq!((R25.timebase(), R25.ntsc()), (25, false));
    assert_eq!((R2997.timebase(), R2997.ntsc()), (30, true));
    assert_eq!((R30.timebase(), R30.ntsc()), (30, false));
    assert_eq!((R5994.timebase(), R5994.ntsc()), (60, true));
    // Drop frame only exists for 29.97 and 59.94.
    for r in TIMELINE_RATES {
        assert_eq!(r.supports_drop_frame(), *r == R2997 || *r == R5994, "{r}");
    }
    assert_eq!(R23976.label(), "23.976");
    assert_eq!(R2997.label(), "29.97");
    assert_eq!(R5994.label(), "59.94");
}

#[test]
fn one_hour_in_frames_at_each_rate() {
    let hour = |r: Rate, df: bool| timecode_to_frames("01:00:00:00", r, df).unwrap();
    assert_eq!(hour(R23976, false), 86_400);
    assert_eq!(hour(R24, false), 86_400);
    assert_eq!(hour(R25, false), 90_000);
    assert_eq!(hour(R2997, false), 108_000);
    assert_eq!(hour(R2997, true), 107_892);
    assert_eq!(hour(R30, false), 108_000);
    assert_eq!(hour(R5994, false), 216_000);
    assert_eq!(hour(R5994, true), 215_784);
    // Drop frame forced on for a rate without drop frame has no effect.
    assert_eq!(hour(R25, true), 90_000);
}

#[test]
fn drop_frame_skips_frame_numbers_at_each_minute_except_tenths() {
    // 29.97: frames ;00 and ;01 do not exist at the start of minutes 1-9, 11-19, ...
    assert_eq!(frames_to_timecode(1799, R2997, true), "00:00:59;29");
    assert_eq!(frames_to_timecode(1800, R2997, true), "00:01:00;02");
    assert_eq!(frames_to_timecode(17_981, R2997, true), "00:09:59;29");
    assert_eq!(frames_to_timecode(17_982, R2997, true), "00:10:00;00");
    assert_eq!(frames_to_timecode(107_892, R2997, true), "01:00:00;00");
    // 59.94: four numbers are skipped.
    assert_eq!(frames_to_timecode(3599, R5994, true), "00:00:59;59");
    assert_eq!(frames_to_timecode(3600, R5994, true), "00:01:00;04");
    assert_eq!(frames_to_timecode(35_964, R5994, true), "00:10:00;00");
    assert_eq!(frames_to_timecode(215_784, R5994, true), "01:00:00;00");
    // Non drop frame counts every number.
    assert_eq!(frames_to_timecode(1800, R2997, false), "00:01:00:00");
    assert_eq!(frames_to_timecode(86_400, R23976, false), "01:00:00:00");
    assert_eq!(frames_to_timecode(90_000 + 24, R25, false), "01:00:00:24");
    // Labels that drop frame never uses are rejected or normalised consistently.
    assert_eq!(timecode_to_frames("00:01:00;02", R2997, true), Some(1800));
    assert_eq!(timecode_to_frames("00:00:00:24", R24, false), None);
}

#[test]
fn drop_frame_timecode_tracks_the_wall_clock() {
    // One real hour at 29.97 is 107,892.1 frames; DF labels it 01:00:00;00.
    let frames = R2997.seconds_to_frames(3600.0);
    assert_eq!(frames, 107_892);
    assert_eq!(frames_to_timecode(frames, R2997, true), "01:00:00;00");
    // NDF at 29.97 drifts 3.6 s behind per hour.
    assert_eq!(frames_to_timecode(frames, R2997, false), "00:59:56:12");
    let frames = R5994.seconds_to_frames(3600.0);
    assert_eq!(frames_to_timecode(frames, R5994, true), "01:00:00;00");
}

#[test]
fn every_frame_round_trips_at_every_rate() {
    for &r in TIMELINE_RATES {
        for df in [false, true] {
            if df && !r.supports_drop_frame() {
                continue;
            }
            // Dense near minute boundaries, sparse elsewhere, over a full 24 hours.
            let mut checked = 0;
            let mut f: i64 = 0;
            // Timecode wraps at 24 hours, which is fewer frames with drop frame.
            let last = timecode_to_frames(&format!("23:59:59{}{:02}", if df { ';' } else { ':' }, r.timebase() - 1), r, df).unwrap();
            while f <= last {
                let tc = frames_to_timecode(f, r, df);
                assert_eq!(timecode_to_frames(&tc, r, df), Some(f), "{r} df={df} frame {f} -> {tc}");
                assert_eq!(tc.contains(';'), df, "{tc}");
                checked += 1;
                f += if f % (r.timebase() as i64 * 60) < 10 { 1 } else { 997 };
            }
            assert!(checked > 1000);
        }
    }
}

#[test]
fn seconds_and_frames_convert_exactly_at_ntsc_rates() {
    assert_eq!(R23976.seconds_to_frames(1001.0 / 24.0), 1000);
    assert!((R23976.frames_to_seconds(24) - 1.001).abs() < 1e-12);
    assert!((R2997.frames_to_seconds(30) - 1.001).abs() < 1e-12);
    assert_eq!(R5994.seconds_to_frames(10.01), 600);
    assert_eq!(R25.seconds_to_frames(2.0), 50);
}

#[test]
fn measured_rates_snap_to_standard_rates() {
    assert_eq!(Rate::snap(23.976), R23976);
    assert_eq!(Rate::snap(23.98), R23976);
    assert_eq!(Rate::snap(24.0), R24);
    assert_eq!(Rate::snap(29.97), R2997);
    assert_eq!(Rate::snap(29.99), R30);
    assert_eq!(Rate::snap(59.94), R5994);
    assert_eq!(Rate::snap(119.88), Rate::new(120000, 1001));
    // Phone footage: nominal rate is a timebase, the average is what matters.
    let (r, _, vfr) = choose_rate(Some(600.0), Some(29.96));
    assert_eq!(r, R2997);
    assert!(vfr);
    let (r, _, vfr) = choose_rate(Some(30000.0 / 1001.0), Some(30000.0 / 1001.0));
    assert_eq!(r, R2997);
    assert!(!vfr);
    let (r, _, _) = choose_rate(Some(25.0), Some(24.99));
    assert_eq!(r, R25);
}
