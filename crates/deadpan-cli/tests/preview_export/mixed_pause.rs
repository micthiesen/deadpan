//! Real Original A/V through a mixed Repeat pause, independently of its plan.

use super::*;
use deadpan_core::{
    InstancePath, MarkId, OccurrenceIdentities, OccurrencePauseIdentities, RepeatEditBranch,
    RepeatEditStep, RepeatInstance, ScopedNodeTarget,
};

const SOURCE_STARTS: [i64; 6] = [16, 22, 28, 34, 40, 46];
const LOCAL_CUT: i64 = 1;
const PAUSE_FRAMES: i64 = 3;
// The Original click starts at sample 48,000. Original frame 28 begins at
// B(28) = 44,845, so each retained four-frame Source hears it 3,155 samples in.
const CLICK_LOCAL_SAMPLE: i64 = 3155;

#[derive(Debug)]
struct SourceAllocation {
    before: i64,
    after: i64,
    paused: bool,
}

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

/// Before the pause: Original 12..28, then three outer plays of an inner
/// two-play Original 28..32 loop. Every gap is two silent frames, and Original
/// 32..42 follows. Six Source starts and every expected picture are authored
/// here; neither the changed document nor a render plan supplies the oracle.
fn layout(
    outer_default: bool,
) -> (
    Vec<Expected>,
    Vec<SourceAllocation>,
    Vec<std::ops::Range<i64>>,
) {
    let mut pictures: Vec<_> = (12..28).map(original).collect();
    let mut allocations = Vec::new();
    let mut silence = Vec::new();
    let mut background = |pictures: &mut Vec<Expected>, frames: i64| {
        let start = pictures.len() as i64;
        pictures.extend(std::iter::repeat_n(Expected::Background, frames as usize));
        silence.push(start..start + frames);
    };
    for outer in 0..3 {
        for inner in 0..2 {
            let paused = if outer_default {
                inner == 1
            } else {
                outer == 2
            };
            allocations.push(SourceAllocation {
                before: SOURCE_STARTS[outer * 2 + inner],
                after: pictures.len() as i64,
                paused,
            });
            if paused {
                pictures.push(original(28));
                background(&mut pictures, PAUSE_FRAMES);
                pictures.extend((29..32).map(original));
            } else {
                pictures.extend((28..32).map(original));
            }
            if inner == 0 {
                background(&mut pictures, 2);
            }
        }
        if outer != 2 {
            background(&mut pictures, 2);
        }
    }
    pictures.extend((32..42).map(original));
    (pictures, allocations, silence)
}

fn build(dir: &Path, outer_default: bool) -> Result<Fixture> {
    let name = if outer_default {
        "mixed-default-play-pause"
    } else {
        "mixed-play-default-pause"
    };
    let mut project = Project::create(dir, name)?;
    project.shorten()?;
    project.split_root(16)?;
    let saved = project.split_root(20)?;
    let source = root_child_at(&saved, 16)?.0;
    project.apply(|_, _, _| {
        Ok(Command::WrapRepeat {
            node: source.clone(),
            id: node("inner")?,
            plays: 2,
            gap: Some(silent(2, HoldVideo::Background)?),
            anchor_policy: WrapAnchorPolicy::First,
        })
    })?;
    let before = project.apply(|_, _, _| {
        Ok(Command::WrapRepeat {
            node: node("inner")?,
            id: node("outer")?,
            plays: 3,
            gap: Some(silent(2, HoldVideo::Background)?),
            anchor_policy: WrapAnchorPolicy::First,
        })
    })?;
    assert_eq!(before.duration()?.frames(), 60);
    let expected_pcm: Vec<_> = SOURCE_STARTS
        .into_iter()
        .map(|start| pcm(&project, boundary(start)? + CLICK_LOCAL_SAMPLE))
        .collect::<Result<_>>()?;
    let iteration = |repeat: &str, play| -> Result<RepeatInstance> {
        let NodeKind::Repeat { iterations, .. } = &before.nodes()[&node(repeat)?].kind else {
            return Err(format!("{repeat} must be a Repeat").into());
        };
        Ok(RepeatInstance {
            node: node(repeat)?,
            iteration: iterations.at(play).ok_or("authored play exists")?,
        })
    };
    // Both cases enter through the last concrete Source, at old Edit 47.
    // Default/Play changes the second inner play in all three outer defaults.
    // Play/Default changes both inner defaults only in the third outer play.
    let outer = iteration("outer", 2)?;
    let inner = iteration("inner", 1)?;
    let within = InstancePath {
        node: source.clone(),
        repeats: vec![outer.clone(), inner.clone()],
    };
    let target = ScopedNodeTarget {
        node: source,
        repeats: vec![
            RepeatEditStep {
                repeat: outer.node,
                branch: if outer_default {
                    RepeatEditBranch::Default
                } else {
                    RepeatEditBranch::Play {
                        iteration: outer.iteration,
                    }
                },
            },
            RepeatEditStep {
                repeat: inner.node,
                branch: if outer_default {
                    RepeatEditBranch::Play {
                        iteration: inner.iteration,
                    }
                } else {
                    RepeatEditBranch::Default
                },
            },
        ],
    };
    project.apply(|project, document, revision| {
        let at = ProjectFrame(47);
        let required = document.pause_insertion_scoped(at, &target, &within)?;
        Ok(Command::InsertOccurrenceTime {
            at,
            within: Some(within),
            target: Some(target),
            hold: silent(PAUSE_FRAMES, HoldVideo::Background)?,
            id: project.fresh(1)?.remove(0),
            identities: OccurrencePauseIdentities {
                isolation: OccurrenceIdentities {
                    nodes: project.fresh(required.isolation_nodes)?,
                    marks: (0..required.isolation_marks)
                        .map(|index| MarkId::new(format!("{revision}-mark-{index}")))
                        .collect::<std::result::Result<_, _>>()?,
                },
                split: SplitIdentities {
                    nodes: project.fresh(required.split_nodes)?,
                },
                container: if required.container {
                    Some(project.fresh(1)?.remove(0))
                } else {
                    None
                },
                gap: if required.gap {
                    Some(project.fresh(1)?.remove(0))
                } else {
                    None
                },
            },
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
            ai: false,
            all_plays: false,
        })
    })?;
    let (pictures, allocations, silence) = layout(outer_default);
    let expected_frames = if outer_default { 69 } else { 66 };
    assert_eq!(pictures.len(), expected_frames);
    assert_eq!(
        project.document()?.duration()?.frames(),
        expected_frames as i64
    );
    let mut clicks = Vec::new();
    for (allocation, expected) in allocations.iter().zip(&expected_pcm) {
        let old = boundary(allocation.before)? + CLICK_LOCAL_SAMPLE;
        let actual_start = if allocation.paused {
            // The Source's suffix resumes at the old local cut, on its own
            // old sample support. Round the two absolute cut origins once;
            // do not add a rounded three-frame duration to the click onset.
            boundary(allocation.after + LOCAL_CUT + PAUSE_FRAMES)?
                + (old - boundary(allocation.before + LOCAL_CUT)?)
        } else {
            boundary(allocation.after)? + (old - boundary(allocation.before)?)
        };
        same_pcm(
            expected,
            &pcm(&project, actual_start)?,
            &format!("{name} Source {allocation:?}"),
        );
        clicks.push((actual_start, true));
    }
    let quiet: Vec<_> = silence
        .iter()
        .map(|range| Ok((boundary(range.start)? + 512, false)))
        .collect::<Result<_>>()?;
    let mut fixture = project.finish(
        vec!["Hold (insert time)", "Repeat operator"],
        pictures
            .into_iter()
            .enumerate()
            .map(|(frame, expected)| (frame as u64, expected))
            .collect(),
        vec![format!(
            "Real cfr-bframes.mp4 Original A/V at 30000/1001. Three outer plays, each with two inner plays and two-frame silent gaps. A three-frame explicit {} pause enters through the last Source at old Edit 47, local 1; final duration {expected_frames} frames. All six old click PCM windows retain their samples within 1e-6; authored new click sample windows {clicks:?}; authored silent windows {quiet:?}. The separate decoded-PCM oracle covers 44.1 kHz input.",
            if outer_default { "Default/Play" } else { "Play/Default" },
        )],
    )?;
    fixture.audio = clicks.into_iter().chain(quiet).collect();
    Ok(fixture)
}

pub fn default_play(dir: &Path) -> Result<Fixture> {
    build(dir, true)
}

pub fn play_default(dir: &Path) -> Result<Fixture> {
    build(dir, false)
}
