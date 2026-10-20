//! Real retained Original A/V through one existing nested Hold resize.
use super::*;
use deadpan_core::{InstancePath, MarkId, OccurrenceEdit, OccurrenceIdentities, RepeatInstance};

// Before the edit, each seven-frame body is Original 28, silent freeze 3,
// Original 29..32. The four bodies begin at 16, 25, 34, 43. Their suffixes begin
// at 20, 29, 38, 47. The Original click at 48000 lies 1554 retained samples into
// each suffix; this follows B(16) + 3155 - B(17), without consulting a plan.
const BODY_STARTS: [i64; 4] = [16, 25, 34, 43];
const CLICK_IN_SUFFIX: i64 = 1554;

fn boundary(frame: i64) -> Result<i64> {
    Ok(i64::try_from(
        ExactRatio::new(i128::from(frame) * 8008, 5)?.round_even()?,
    )?)
}

fn pcm(project: &Project, start: i64) -> Result<Vec<[f64; 2]>> {
    let result = success(&[
        "inspect-audio",
        project.path(),
        "--samples",
        &start.to_string(),
        &(start + 256).to_string(),
        "--authored-bus",
    ])?;
    Ok(serde_json::from_value(result["audio"]["samples"].clone())?)
}

fn same_pcm(expected: &[[f64; 2]], actual: &[[f64; 2]], label: &str) {
    assert_eq!(expected.len(), 256);
    assert_eq!(actual.len(), expected.len());
    assert!(expected.iter().flatten().any(|value| value.abs() > 0.001));
    for (sample, (expected, actual)) in expected.iter().zip(actual).enumerate() {
        for channel in 0..2 {
            assert!(actual[channel].is_finite());
            assert!(
                (actual[channel] - expected[channel]).abs() < 1e-6,
                "{label} sample {sample} channel {channel}: {} != {}",
                actual[channel],
                expected[channel]
            );
        }
    }
}

/// Every expected picture, changed Hold, translated Source allocation and
/// silent interval follows the authored body above, independently of the plan.
fn layout(selected: usize, frames: i64) -> (Vec<Expected>, Vec<i64>, Vec<std::ops::Range<i64>>) {
    let mut pictures: Vec<_> = (12..28).map(original).collect();
    let mut suffixes = Vec::new();
    let mut silence = Vec::new();
    for outer in 0..2 {
        for inner in 0..2 {
            let index = outer * 2 + inner;
            pictures.push(original(28));
            let start = pictures.len() as i64;
            let count = if index == selected { frames } else { 3 };
            pictures.extend(std::iter::repeat_n(original(28), count as usize));
            silence.push(start..start + count);
            suffixes.push(pictures.len() as i64);
            pictures.extend((29..32).map(original));
            if inner == 0 {
                let start = pictures.len() as i64;
                pictures.extend([Expected::Background; 2]);
                silence.push(start..start + 2);
            }
        }
        if outer == 0 {
            let start = pictures.len() as i64;
            pictures.extend([Expected::Background; 2]);
            silence.push(start..start + 2);
        }
    }
    pictures.extend((32..42).map(original));
    (pictures, suffixes, silence)
}

fn build(dir: &Path, grow: bool) -> Result<Fixture> {
    let name = if grow {
        "scoped-hold-duration-grow"
    } else {
        "scoped-hold-duration-shrink"
    };
    let mut project = Project::create(dir, name)?;
    project.shorten()?;
    project.split_root(16)?;
    project.split_root(20)?;
    insert_freeze(&mut project, 17, 3, "duration-hold")?;
    project.apply(|_, document, _| {
        Ok(Command::Group {
            parent: document.root().clone(),
            start: 1,
            end: 4,
            id: node("body")?,
            label: "Original and existing Hold".into(),
        })
    })?;
    for (child, repeat) in [("body", "inner"), ("inner", "outer")] {
        project.apply(|_, _, _| {
            Ok(Command::WrapRepeat {
                node: node(child)?,
                id: node(repeat)?,
                plays: 2,
                gap: Some(silent(2, HoldVideo::Background)?),
                anchor_policy: WrapAnchorPolicy::First,
            })
        })?;
    }
    let before = project.document()?;
    assert_eq!(before.duration()?.frames(), 60);
    let expected_pcm: Vec<_> = BODY_STARTS
        .into_iter()
        .map(|start| pcm(&project, boundary(start + 4)? + CLICK_IN_SUFFIX))
        .collect::<Result<_>>()?;
    let (selected, frames, outer, inner) = if grow { (0, 5, 0, 0) } else { (3, 1, 1, 1) };
    let iteration = |repeat: &str, ordinal| -> Result<RepeatInstance> {
        let NodeKind::Repeat { iterations, .. } = &before.nodes()[&node(repeat)?].kind else {
            return Err("Repeat".into());
        };
        Ok(RepeatInstance {
            node: node(repeat)?,
            iteration: iterations.at(ordinal).ok_or("play")?,
        })
    };
    let instance = InstancePath {
        node: node("duration-hold")?,
        repeats: vec![iteration("outer", outer)?, iteration("inner", inner)?],
    };
    project.apply(|project, document, revision| {
        let required = document
            .occurrence_hold_duration_requirements(&instance, FrameDuration::new(frames)?)?;
        Ok(Command::EditOccurrence {
            instance,
            edit: OccurrenceEdit::SetHoldDuration {
                duration: FrameDuration::new(frames)?,
            },
            identities: OccurrenceIdentities {
                nodes: project.fresh(required.nodes)?,
                marks: (0..required.marks)
                    .map(|n| MarkId::new(format!("{revision}-mark-{n}")))
                    .collect::<std::result::Result<_, _>>()?,
            },
        })
    })?;
    let (pictures, suffixes, silence) = layout(selected, frames);
    let expected_frames = if grow { 62 } else { 58 };
    assert_eq!(pictures.len(), expected_frames);
    assert_eq!(
        project.document()?.duration()?.frames(),
        expected_frames as i64
    );
    let mut clicks = Vec::new();
    for (index, (suffix, expected)) in suffixes.iter().zip(&expected_pcm).enumerate() {
        let actual_start = boundary(*suffix)? + CLICK_IN_SUFFIX;
        same_pcm(
            expected,
            &pcm(&project, actual_start)?,
            &format!(
                "{name} Source suffix {index}, old origin {}, new origin {suffix}",
                BODY_STARTS[index] + 4
            ),
        );
        clicks.push((actual_start, true));
    }
    let quiet: Vec<_> = silence
        .iter()
        .map(|range| Ok((boundary(range.start)? + 512, false)))
        .collect::<Result<_>>()?;
    let mut fixture = project.finish(
        vec!["Hold (insert time)", "Repeat operator"],
        pictures.into_iter().enumerate().map(|(frame, expected)| (frame as u64, expected)).collect(),
        vec![format!("Real cfr-bframes.mp4 A/V at 30000/1001. Existing three-frame freeze at each nested 2x2 play. Explicit Play/Play resize of the {} Hold to {frames} frames; literal final picture layout {expected_frames} frames. Four decoded Original click windows preserve their old per-owner sample origins within 1e-6; current click windows {clicks:?}; silent windows {quiet:?}. Separate PCM regressions cover 44.1 kHz and initially unbound suffixes.", if grow { "first/first" } else { "last/last" })],
    )?;
    fixture.audio = clicks.into_iter().chain(quiet).collect();
    Ok(fixture)
}

pub fn grow(dir: &Path) -> Result<Fixture> {
    build(dir, true)
}
pub fn shrink(dir: &Path) -> Result<Fixture> {
    build(dir, false)
}
