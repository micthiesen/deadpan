//! Independent VFR recipe expectations. The two retained files encode picture
//! durations 1001,2002,3003 in a repeating cycle, with a 1001-tick legacy final
//! picture or a 3003-tick corrected terminal. Source time is never an ordinal.

use super::*;

fn media(long_terminal: bool) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures")
        .join(if long_terminal {
            "vfr-long-terminal.mp4"
        } else {
            "vfr.mp4"
        })
}

// Source tick / 1001 at a picture's start, authored by the fixture generator.
fn start(ordinal: i128) -> i128 {
    6 * (ordinal / 3) + [0, 1, 3][usize::try_from(ordinal % 3).unwrap()]
}

/// Select an Original from a rational point in 1001-tick units. Expectations
/// use only this fixture's authored clock and the recipe's manually flattened
/// structure, never the document, retained index or compiled plan.
fn at(numerator: i128, denominator: i128) -> Expected {
    assert!(numerator >= 0 && numerator < 240 * denominator);
    let ordinal = (0..120)
        .rev()
        .find(|i| start(*i) * denominator <= numerator)
        .unwrap();
    original(u64::try_from(ordinal).unwrap())
}

fn center(project_frame: u64) -> Expected {
    at(i128::from(project_frame) * 2 + 1, 2)
}

fn shortened(dir: &Path, name: &'static str) -> Result<Project> {
    let mut project = Project::create_from(dir, name, &media(true))?;
    // Full VFR source is 240 project frames, not its 120 decoded pictures.
    assert_eq!(project.document()?.duration()?.frames(), 240);
    project.delete_range(42, 240)?;
    project.delete_range(0, 12)?;
    assert_eq!(project.document()?.duration()?.frames(), 30);
    Ok(project)
}

fn finish(project: Project, expected: Vec<Expected>, rows: Vec<&'static str>) -> Result<Fixture> {
    let frames = expected.len();
    let fixture = project.finish(
        rows,
        expected
            .into_iter()
            .enumerate()
            .map(|(i, picture)| (u64::try_from(i).unwrap(), picture))
            .collect(),
        Vec::new(),
    )?;
    assert_eq!(fixture.frames, u64::try_from(frames)?);
    Ok(fixture)
}

fn full(dir: &Path, long: bool) -> Result<Fixture> {
    let name = if long {
        "vfr-long-terminal"
    } else {
        "vfr-legacy-terminal"
    };
    let project = Project::create_from(dir, name, &media(long))?;
    // Both have 240 frames of union A/V. In the legacy file the final two
    // project pictures explicitly hold ordinal 119 while the audio continues.
    finish(
        project,
        (0..240).map(center).collect(),
        vec!["VFR Original and A/V endpoints"],
    )
}

fn freeze(dir: &Path) -> Result<Fixture> {
    let mut project = shortened(dir, "vfr-freeze")?;
    project.run_semantic(
        json!([{"type":"insert_pause","length":{"unit":"frames","frames":15}}]),
        15,
        None,
    )?;
    let expected = (0..45)
        .map(|f| {
            if f < 15 {
                center(12 + f)
            } else if f < 30 {
                center(26)
            } else {
                center(12 + f - 15)
            }
        })
        .collect();
    let mut fixture = finish(
        project,
        expected,
        vec!["VFR freeze and linked audio resume"],
    )?;
    fixture.audio = vec![(28_672, false), (40_000, false), (52_736, true)];
    Ok(fixture)
}

fn repeated(dir: &Path, nested: bool) -> Result<Fixture> {
    let name = if nested {
        "vfr-nested-retime-repeat"
    } else {
        "vfr-repeat-gaps"
    };
    let mut project = shortened(dir, name)?;
    project.split_root(12)?;
    project.split_root(24)?;
    if nested {
        project.apply(|_, document, _| {
            Ok(Command::WrapRetime {
                node: root_child_at(document, 12)?.0,
                id: node("slow")?,
                duration: FrameDuration::new(24)?,
                pitch: PitchPolicy::Preserve,
            })
        })?;
    }
    let (plays, length, gap, duration) = if nested {
        (2, 24, 3, 69)
    } else {
        (3, 12, 6, 66)
    };
    project.apply(|_, document, _| {
        Ok(Command::WrapRepeat {
            node: root_child_at(document, 12)?.0,
            id: node("repeat")?,
            plays,
            gap: Some(silent(gap, HoldVideo::Background)?),
            anchor_policy: WrapAnchorPolicy::First,
        })
    })?;
    let repeated_length = plays as u64 * length + (plays as u64 - 1) * gap as u64;
    let expected = (0..duration)
        .map(|f| {
            if f < 12 {
                return center(12 + f);
            }
            if f >= 12 + repeated_length {
                return center(36 + f - 12 - repeated_length);
            }
            let local = (f - 12) % (length + gap as u64);
            if local >= length {
                Expected::Background
            } else if nested {
                at(96 + i128::from(local) * 2 + 1, 4)
            } else {
                center(24 + local)
            }
        })
        .collect();
    let mut fixture = finish(
        project,
        expected,
        vec!["VFR Repeat, gaps and nested Retime"],
    )?;
    if !nested {
        fixture.audio = vec![
            (28_672, true),
            (57_472, true),
            (86_272, true),
            (40_000, false),
            (69_000, false),
        ];
    }
    Ok(fixture)
}

fn retimed(dir: &Path, preserve: bool) -> Result<Fixture> {
    let name = if preserve {
        "vfr-retime-preserve"
    } else {
        "vfr-retime-tape"
    };
    let mut project = shortened(dir, name)?;
    project.split_root(12)?;
    project.split_root(24)?;
    let length = if preserve { 24 } else { 8 };
    project.apply(|_, document, _| {
        Ok(Command::WrapRetime {
            node: root_child_at(document, 12)?.0,
            id: node("speed")?,
            duration: FrameDuration::new(length)?,
            pitch: if preserve {
                PitchPolicy::Preserve
            } else {
                PitchPolicy::FollowSpeed
            },
        })
    })?;
    let expected = (0..18 + length as u64)
        .map(|f| {
            if f < 12 {
                center(12 + f)
            } else if f >= 12 + length as u64 {
                center(36 + f - 12 - length as u64)
            } else {
                let local = i128::from(f - 12);
                // Source point 24 + (n+.5)*12/length, including fractional centers.
                at(
                    48 * i128::from(length) + (2 * local + 1) * 12,
                    2 * i128::from(length),
                )
            }
        })
        .collect();
    finish(
        project,
        expected,
        vec!["VFR half-speed Preserve and 1.5x Tape"],
    )
}

fn reversed(dir: &Path, bounce: bool) -> Result<Fixture> {
    let name = if bounce {
        "vfr-ping-pong"
    } else {
        "vfr-reverse-hiccup"
    };
    let mut project = shortened(dir, name)?;
    let (cursor, length, inserted) = if bounce { (24, 12, 11) } else { (20, 8, 8) };
    project.run_semantic(json!([{"type":"insert_reverse","length":{"unit":"frames","frames":length},"bounce":bounce}]), cursor, None)?;
    let expected = (0..30 + inserted)
        .map(|f| {
            if f < cursor {
                center(u64::try_from(12 + f).unwrap())
            } else if f < cursor + inserted {
                let source = 12 + cursor - 1 - i64::from(bounce) - (f - cursor);
                center(u64::try_from(source).unwrap())
            } else {
                center(u64::try_from(12 + f - inserted).unwrap())
            }
        })
        .collect();
    let mut fixture = finish(project, expected, vec!["VFR reverse and bounce endpoints"])?;
    fixture.audio = if bounce {
        vec![(28_672, true), (46_336, true), (52_000, false)]
    } else {
        vec![(28_672, true), (35_072, true), (50_000, false)]
    };
    Ok(fixture)
}

fn cutaway(dir: &Path) -> Result<Fixture> {
    let mut project = shortened(dir, "vfr-cutaway-bounce")?;
    let asset = project.asset.clone();
    project.apply(|_, document, _| {
        let (beat, start) = root_child_at(document, 10)?;
        let (host, offset) = source_host(document, &beat)?;
        let local = offset + 10 - start;
        let time_base = deadpan_core::SourceTimeBase::new(1, 30_000)?;
        let point = |ticks| SourcePoint {
            time_base,
            ticks: ExactRatio::integer(ticks),
        };
        Ok(Command::SetCutaways {
            node: host,
            cutaways: vec![Cutaway {
                range: FrameRange::new(ProjectFrame(local), ProjectFrame(local + 16))?,
                asset: asset.clone(),
                selection: ExactSourceSpan::new(point(180_180), point(192_192))?,
                fit: CutawayFit::Bounce,
                removed: false,
            }],
        })
    })?;
    let expected = (0..30)
        .map(|f| {
            if !(10..26).contains(&f) {
                center(12 + f)
            } else {
                let n = i128::from(f - 10);
                at(360 + if n < 12 { 2 * n + 1 } else { 47 - 2 * n }, 2)
            }
        })
        .collect();
    finish(
        project,
        expected,
        vec!["VFR picture-only cutaway and bounce"],
    )
}

fn bleep(dir: &Path) -> Result<Fixture> {
    let mut project = shortened(dir, "vfr-bleep")?;
    // The first selected center is inside a two-project-frame VFR picture;
    // using that picture's PTS as the playback origin shifts later pictures.
    project.run_semantic(
        json!([
            {"type":"begin_selection"},
            {"type":"move_frames","forward":true,"count":8},
            {"type":"bleep","register":"b","level":-3000}
        ]),
        14,
        None,
    )?;
    finish(
        project,
        (0..30).map(|f| center(12 + f)).collect(),
        vec!["VFR bleep preserves source clock"],
    )
}

pub fn matrix(root: &Path) -> Result<Vec<Fixture>> {
    Ok(vec![
        full(&root.join("legacy"), false)?,
        full(&root.join("long"), true)?,
        freeze(&root.join("freeze"))?,
        repeated(&root.join("repeat"), false)?,
        repeated(&root.join("nested"), true)?,
        retimed(&root.join("preserve"), true)?,
        retimed(&root.join("tape"), false)?,
        reversed(&root.join("reverse"), false)?,
        reversed(&root.join("bounce"), true)?,
        cutaway(&root.join("cutaway"))?,
        bleep(&root.join("bleep"))?,
    ])
}

/// Inspect plan points against the authored PTS table. This is independent of
/// SourceVideoIndex/select_source_frame, which the real preview/export consume.
pub fn plan_picture(picture: &Value) -> Result<Expected> {
    let ratio = |point: &Value| -> Result<(i128, i128)> {
        assert_eq!(
            point["time_base"],
            json!({"numerator":1,"denominator":30000})
        );
        Ok((
            point["ticks"]["numerator"]
                .as_str()
                .ok_or("ticks")?
                .parse()?,
            point["ticks"]["denominator"]
                .as_str()
                .ok_or("denominator")?
                .parse()?,
        ))
    };
    let ordinal = |n: i128, d: i128| -> i128 {
        (0..120)
            .rev()
            .find(|i| start(*i) * 1001 * d <= n)
            .unwrap_or(-1)
    };
    Ok(match picture["type"].as_str().ok_or("picture type")? {
        "background" => Expected::Background,
        "freeze" => {
            let (n, d) = ratio(&picture["point"])?;
            original(u64::try_from(ordinal(n, d))?)
        }
        "source" => {
            assert_eq!(picture["endpoints"], "hold_adjacent");
            let (n, d) = ratio(&picture["point"])?;
            let (a, ad) = ratio(&picture["selection"]["start"])?;
            let (b, bd) = ratio(&picture["selection"]["end"])?;
            let first = ordinal(a, ad);
            let last = (0..120)
                .rev()
                .find(|i| start(*i) * 1001 * bd < b)
                .ok_or("selected endpoint")?;
            original(u64::try_from(ordinal(n, d).clamp(first, last))?)
        }
        other => return Err(format!("unexpected VFR picture {other}").into()),
    })
}
