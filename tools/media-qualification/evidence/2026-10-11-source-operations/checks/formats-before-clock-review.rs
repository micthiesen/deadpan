//! Combined structural edits over independently authored source clocks.
//!
//! The oracle uses the retained fixture generators' timestamp patterns, never
//! the media index, document mapping or render plan. All files already have
//! separate decoder/color qualification; this covers their composition with
//! cuts, nested Retime/Repeat, silent gaps and an inserted freeze.

use super::*;

#[derive(Clone, Copy)]
enum Clock {
    Regular,
    /// Picture positions 4 and 7 are omitted on the 30000/1001 grid.
    MissingPictures,
    /// The same grid and omissions, rounded to the Matroska millisecond clock.
    MillisecondMissing,
    /// H.264 VFR durations repeat 1, 2, 3 project frames.
    AlternatingVfr,
}

#[derive(Clone, Copy)]
pub struct Case {
    pub name: &'static str,
    pub rate: [u32; 2],
    clock: Clock,
}

impl Case {
    const fn regular(name: &'static str, rate: [u32; 2]) -> Self {
        Self {
            name,
            rate,
            clock: Clock::Regular,
        }
    }

    /// Original picture at an exact point in units of this project's frames.
    fn at(self, numerator: i128, denominator: i128) -> Expected {
        assert!(numerator >= 0 && denominator > 0);
        let ordinal = match self.clock {
            Clock::Regular => numerator / denominator,
            Clock::MissingPictures | Clock::MillisecondMissing => {
                (0_i128..12)
                    .filter(|n| ![4, 7].contains(n))
                    .take_while(|n| match self.clock {
                        Clock::MillisecondMissing => {
                            let pts_ms = (n * 1001 + 15) / 30;
                            pts_ms * 30_000 * denominator <= numerator * 1001 * 1000
                        }
                        _ => n * denominator <= numerator,
                    })
                    .count() as i128
                    - 1
            }
            Clock::AlternatingVfr => {
                (0_i128..120)
                    .take_while(|n| {
                        let start = 6 * (n / 3) + [0, 1, 3][usize::try_from(n % 3).unwrap()];
                        start * denominator <= numerator
                    })
                    .count() as i128
                    - 1
            }
        };
        original(u64::try_from(ordinal).unwrap())
    }

    pub fn plan_picture(self, picture: &Value) -> Result<Expected> {
        if picture["type"] == "background" {
            return Ok(Expected::Background);
        }
        assert!(picture["type"] == "source" || picture["type"] == "freeze");
        let point = &picture["point"];
        let part = |value: &Value| -> Result<i128> {
            Ok(value.as_str().ok_or("rational string")?.parse()?)
        };
        let time_base = &point["time_base"];
        let numerator = part(&point["ticks"]["numerator"])?
            * i128::from(time_base["numerator"].as_u64().ok_or("time base")?)
            * i128::from(self.rate[0]);
        let denominator = part(&point["ticks"]["denominator"])?
            * i128::from(time_base["denominator"].as_u64().ok_or("time base")?)
            * i128::from(self.rate[1]);
        Ok(self.at(numerator, denominator))
    }
}

pub const CASES: &[Case] = &[
    Case::regular("cfr-bframes.mp4", [30000, 1001]),
    Case {
        name: "vfr-long-terminal.mp4",
        rate: [30000, 1001],
        clock: Clock::AlternatingVfr,
    },
    Case::regular("h264-sdr-10-full.mp4", [30000, 1001]),
    Case::regular("hevc-sdr-8-limited.mp4", [30000, 1001]),
    Case::regular("hevc-sdr-10-full.mp4", [30000, 1001]),
    Case::regular("vp9-sdr-10-full.mp4", [30000, 1001]),
    Case {
        name: "vp9-vfr.webm",
        rate: [30000, 1001],
        clock: Clock::MillisecondMissing,
    },
    Case::regular("av1-sdr-8-limited.mp4", [30000, 1001]),
    Case::regular("av1-sdr-10-limited.mp4", [30000, 1001]),
    Case {
        name: "av1-vfr.mp4",
        rate: [30000, 1001],
        clock: Clock::MissingPictures,
    },
    Case {
        name: "av1-vfr.webm",
        rate: [30000, 1001],
        clock: Clock::MillisecondMissing,
    },
    Case::regular("av1-grain.mp4", [30000, 1001]),
    Case::regular("av1-pq.mp4", [30000, 1001]),
    Case::regular("av1-hlg.mp4", [30000, 1001]),
    Case::regular("prores-hq.mov", [30000, 1001]),
    Case {
        name: "prores-vfr.mov",
        rate: [30000, 1001],
        clock: Clock::MissingPictures,
    },
    Case::regular("fields-tff.mp4", [50, 1]),
    Case::regular("fields-bff.mp4", [60000, 1001]),
    Case::regular("telecine-tff.mp4", [60000, 1001]),
    Case::regular("aperture-fractional.mp4", [30000, 1001]),
];

pub fn build(directory: &Path, case: Case, pitch: PitchPolicy) -> Result<Fixture> {
    let media = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures")
        .join(case.name);
    let mut project = Project::create_from(directory, case.name, &media)?;
    let initial = project.document()?;
    let rate = initial.presentation_basis().frame_rate;
    assert_eq!([rate.numerator(), rate.denominator()], case.rate);
    let duration = initial.duration()?.frames();
    assert!(duration >= 12);
    if duration > 12 {
        project.delete_range(12, duration)?;
    }
    project.split_root(4)?;
    project.split_root(8)?;
    project.apply(|_, document, _| {
        Ok(Command::WrapRetime {
            node: root_child_at(document, 4)?.0,
            id: node("slow")?,
            duration: FrameDuration::new(6)?,
            pitch,
        })
    })?;
    project.apply(|_, _, _| {
        Ok(Command::WrapRepeat {
            node: node("slow")?,
            id: node("repeat")?,
            plays: 2,
            gap: Some(silent(3, HoldVideo::Background)?),
            anchor_policy: WrapAnchorPolicy::First,
        })
    })?;
    project.run_semantic(
        json!([{"type":"insert_pause","length":{"unit":"frames","frames":3}}]),
        22,
        None,
    )?;
    // A[0,4), slow B[4,10), gap[10,13), slow B[13,19), C[19,22),
    // freeze[22,25), final C[25,26). A/B/C cover Original [0,4)/[4,8)/[8,12).
    let expected = (0_u64..26)
        .map(|frame| {
            let f = i128::from(frame);
            let picture = match frame {
                0..4 => case.at(2 * f + 1, 2),
                4..10 => case.at(12 + 2 * (f - 4) + 1, 3),
                10..13 => Expected::Background,
                13..19 => case.at(12 + 2 * (f - 13) + 1, 3),
                19..22 => case.at(16 + 2 * (f - 19) + 1, 2),
                22..25 => case.at(21, 2),
                25 => case.at(23, 2),
                _ => unreachable!(),
            };
            (frame, picture)
        })
        .collect();
    let fixture = project.finish(
        vec!["Source-format cuts, nested Retime/Repeat, gap and freeze"],
        expected,
        vec![format!(
            "{}; {pitch:?}; independent generator clock",
            case.name
        )],
    )?;
    assert_eq!(fixture.frames, 26);
    Ok(fixture)
}
