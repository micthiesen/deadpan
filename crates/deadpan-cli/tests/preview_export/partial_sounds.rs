//! Command-created partial attachment copies through the public Render path.
//! Start from the full Original Source, with its audio moved outside the beat.

use super::*;
use deadpan_core::{
    BeatSound, CapturedEditSlice, EqualizerRecipe, MarkId, OccurrenceIdentities,
    SlicePasteIdentities,
};

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
        "--limited",
    ])?;
    Ok(serde_json::from_value(result["audio"]["samples"].clone())?)
}

fn same_pcm(expected: &[[f64; 2]], actual: &[[f64; 2]], context: &str) {
    assert_eq!(expected.len(), 256);
    assert_eq!(actual.len(), expected.len());
    assert!(expected.iter().flatten().any(|value| value.abs() > 0.001));
    for (index, (expected, actual)) in expected.iter().zip(actual).enumerate() {
        for channel in 0..2 {
            assert!(actual[channel].is_finite());
            assert!(
                (expected[channel] - actual[channel]).abs() < 1e-6,
                "{context} sample {index} channel {channel}: {} != {}",
                expected[channel],
                actual[channel]
            );
        }
    }
}

fn copy(
    project: &mut Project,
    parent: Option<&NodeId>,
    start: i64,
    end: i64,
    capture: &str,
) -> Result {
    project.apply(|project, document, revision| {
        let slice = CapturedEditSlice::capture(
            document,
            parent.unwrap_or(document.root()),
            FrameRange::new(ProjectFrame(start), ProjectFrame(end))?,
            AudioTimingId {
                allocation: RevisionId::new(capture)?,
                ordinal: 0,
            },
        )
        .map_err(|error| format!("capture {start}..{end} ({capture}): {error}"))?;
        let required = slice.identity_requirements()?;
        Ok(Command::SpliceSlice {
            parent: document.root().clone(),
            index: root_children(document)?.len(),
            identities: SlicePasteIdentities {
                authored: OccurrenceIdentities {
                    nodes: project.fresh(required.nodes)?,
                    marks: (0..required.marks)
                        .map(|index| MarkId::new(format!("{revision}-mark-{index}")))
                        .collect::<std::result::Result<_, _>>()?,
                },
                aliases: project.fresh(required.aliases)?,
            },
            slice,
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })?;
    Ok(())
}

/// A copied pre-master bus keeps its samples. Master gain belongs to its new
/// project surroundings: the Original's final transient now precedes this copy.
pub fn partial_copy_mastering_context(dir: &Path) -> Result<Value> {
    let mut project = Project::create(dir, "partial-copy-mastering")?;
    place_click_sound(&mut project)?;
    let owner = root_children(&project.document()?)?[0].clone();
    let eq = EqualizerRecipe::new(
        [3000, -6000, 3000].map(|db| GainDb::new(db).unwrap()),
        None,
        None,
    )?;
    project.apply(|_, document, _| {
        let source = document.sounds()[&SoundId::new("click")?].source.clone();
        Ok(Command::SetBeatSound {
            owner: owner.clone(),
            id: SoundId::new("attached")?,
            event: BeatSound {
                label: "Mixed Original and attached sound".into(),
                mapping: SourceAudioMapping::natural_rate(
                    source.span,
                    document.presentation_basis().frame_rate,
                )?,
                source,
                offset: AudioSample(0),
                treatments: AudioTreatments::default()
                    .with_clip_gain(ClipGain::new(GainDb::new(-12000)?, false, vec![], vec![])?)?
                    .with_equalizer(Some(eq))?,
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        })
    })?;
    project.apply(|_, _, _| {
        Ok(Command::DeleteSound {
            id: SoundId::new("click")?,
        })
    })?;
    project.apply(|_, _, _| {
        Ok(Command::SetAudioTreatments {
            node: owner,
            treatments: AudioTreatments::default().with_equalizer(Some(eq))?,
        })
    })?;
    #[derive(serde::Deserialize)]
    struct Block {
        samples: Vec<[f32; 2]>,
        #[serde(default)]
        gain: Vec<f64>,
    }
    let read = |project: &Project, start: i64, mode: &str| -> Result<Block> {
        Ok(serde_json::from_value(
            success(&[
                "inspect-audio",
                project.path(),
                "--samples",
                &start.to_string(),
                &(start + 256).to_string(),
                mode,
            ])?["audio"]
                .clone(),
        )?)
    };
    let expected = read(&project, boundary(2)? + 512, "--authored-bus")?;
    let before = read(&project, boundary(2)? + 512, "--limited")?;
    let preceding = read(&project, boundary(120)? - 256, "--authored-bus")?;
    let preceding_peak = preceding
        .samples
        .iter()
        .flatten()
        .map(|v| v.abs())
        .fold(0.0, f32::max);
    assert!(
        preceding_peak > 0.9,
        "the preceding Original transient must exercise limiting"
    );
    copy(&mut project, None, 2, 6, "mixed-copy-capture")?;
    let actual = read(&project, boundary(120)? + 512, "--authored-bus")?;
    let after = read(&project, boundary(120)? + 512, "--limited")?;
    assert_eq!(expected.samples.len(), 256);
    assert_eq!(
        actual
            .samples
            .iter()
            .flatten()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>(),
        expected
            .samples
            .iter()
            .flatten()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>(),
        "the complete shared-EQ bus must retain its PCM"
    );
    assert!(actual.samples.iter().flatten().any(|v| v.abs() > 0.1));
    let mut prediction_error = 0.0_f64;
    for (raw, limited) in [(&expected, &before), (&actual, &after)] {
        assert_eq!(limited.samples.len(), raw.samples.len());
        assert_eq!(limited.gain.len(), raw.samples.len());
        for ((input, output), gain) in raw.samples.iter().zip(&limited.samples).zip(&limited.gain) {
            assert!(gain.is_finite() && (0.0..=1.0).contains(gain));
            for channel in 0..2 {
                let predicted = (f64::from(input[channel]) * gain) as f32;
                prediction_error =
                    prediction_error.max(f64::from((predicted - output[channel]).abs()));
                assert_eq!(
                    output[channel].to_bits(),
                    predicted.to_bits(),
                    "limited PCM must equal authored PCM times reported master gain"
                );
            }
        }
    }
    assert!(before.gain.iter().all(|gain| *gain == 1.0));
    let after_min_gain = after.gain.iter().copied().fold(1.0, f64::min);
    assert!(after_min_gain > 0.0 && after_min_gain < 0.99);
    let difference = before
        .samples
        .iter()
        .flatten()
        .zip(after.samples.iter().flatten())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f32::max);
    assert!(
        difference > 0.001,
        "the mastering-context distinction must change PCM magnitude materially"
    );
    Ok(json!({
        "package":project.package,"frames_compared":256,"channels":2,
        "max_authored_difference":0.0,"max_limited_difference":difference,
        "before_min_gain":1.0,"after_min_gain":after_min_gain,
        "max_gain_prediction_error":prediction_error,"preceding_original_peak":preceding_peak,
    }))
}

fn copied_project(dir: &Path) -> Result<Project> {
    let mut project = Project::create(dir, "partial-attached-sound-copies")?;
    place_click_sound(&mut project)?;
    let owner = root_children(&project.document()?)?[0].clone();
    project.apply(|_, document, _| {
        let NodeKind::Source { source } = &document.nodes()[&owner].kind else {
            return Err("fixture owner is not a Source".into());
        };
        Ok(Command::SetSourceAudioMapping {
            node: owner.clone(),
            mapping: source.audio_mapping,
            // Keep the Original's audio outside this allocation so its own
            // timing and edge policies cannot supply the attachment oracle.
            offset: AudioSample(480_000),
        })
    })?;
    let eq = EqualizerRecipe::new(
        [3000, -6000, 3000].map(|db| GainDb::new(db).unwrap()),
        None,
        None,
    )?;
    project.apply(|_, document, _| {
        let source = document.sounds()[&SoundId::new("click")?].source.clone();
        Ok(Command::SetBeatSound {
            owner: owner.clone(),
            id: SoundId::new("attached")?,
            event: BeatSound {
                label: "Partial copied sound".into(),
                mapping: SourceAudioMapping::natural_rate(
                    source.span,
                    document.presentation_basis().frame_rate,
                )?,
                source,
                offset: AudioSample(0),
                treatments: AudioTreatments::default()
                    .with_clip_gain(ClipGain::new(GainDb::new(-12000)?, false, vec![], vec![])?)?
                    .with_equalizer(Some(eq))?,
                start_edge: AudioEdgePolicy::Hard,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        })
    })?;
    project.apply(|_, _, _| {
        Ok(Command::DeleteSound {
            id: SoundId::new("click")?,
        })
    })?;
    project.apply(|_, _, _| {
        Ok(Command::SetAudioTreatments {
            node: owner.clone(),
            treatments: AudioTreatments::default().with_equalizer(Some(eq))?,
        })
    })?;

    let first = pcm(&project, boundary(2)? + 512)?;
    copy(&mut project, None, 2, 6, "partial-export-capture")?;
    same_pcm(&first, &pcm(&project, boundary(120)? + 512)?, "first paste");
    let second = pcm(&project, boundary(121)? + 512)?;
    let copied_group = root_children(&project.document()?)?
        .last()
        .ok_or("copied group")?
        .clone();
    copy(
        &mut project,
        Some(&copied_group),
        121,
        124,
        "partial-export-recapture",
    )?;
    same_pcm(&second, &pcm(&project, boundary(124)? + 512)?, "recopy");
    project.apply(|_, _, revision| {
        Ok(Command::DeleteRipple {
            node: owner.clone(),
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })?;
    same_pcm(&first, &pcm(&project, 512)?, "first after deletion");
    same_pcm(
        &second,
        &pcm(&project, boundary(4)? + 512)?,
        "recopy after deletion",
    );
    assert_eq!(project.document()?.beat_sounds().len(), 2);
    Ok(project)
}

pub fn partial_attached_sound_copies(dir: &Path) -> Result<Fixture> {
    let project = copied_project(dir)?;
    project.finish(
        vec!["Partial attached-sound copy and recopy"],
        (0..4)
            .map(|frame| (frame, original(frame + 2)))
            .chain((4..7).map(|frame| (frame, original(frame - 1))))
            .collect(),
        vec![
            "Real registered stereo WAV with event and shared-owner EQ; partial copy 2..6, recopy 121..124, delete original owner. Independent pre-copy PCM windows match after both pastes and deletion within 1e-6.".into(),
        ],
    )
}

pub fn split_attached_sounds(dir: &Path) -> Result<Fixture> {
    let mut project = copied_project(dir)?;
    let expected = [0, 1, 2]
        .map(|frame| pcm(&project, boundary(frame).unwrap() + 512))
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    for at in [1, 3] {
        project.apply(|project, document, revision| {
            let count = document
                .insert_time_target(ProjectFrame(at))?
                .split
                .ok_or("expected split")?
                .required_ids;
            Ok(Command::InsertTime {
                at: ProjectFrame(at),
                hold: HoldRecipe {
                    duration: FrameDuration::new(1)?,
                    video: HoldVideo::Background,
                    picture_context: None,
                    audio: HoldAudio::Silence,
                },
                id: project.fresh(1)?.remove(0),
                identities: SplitIdentities {
                    nodes: project.fresh(count)?,
                },
                timing: AudioTimingId {
                    allocation: revision.clone(),
                    ordinal: 0,
                },
            })
        })?;
    }
    let first_shift = boundary(2)? - boundary(1)?;
    let second_shift = boundary(4)? - boundary(3)?;
    for ((frame, shift), expected) in [(0, 0), (1, first_shift), (2, first_shift + second_shift)]
        .into_iter()
        .zip(expected)
    {
        same_pcm(
            &expected,
            &pcm(&project, boundary(frame)? + 512 + shift)?,
            "split attached sound",
        );
    }
    assert_eq!(project.document()?.beat_sounds().len(), 4);
    let mut fixture = project.finish(
        vec!["Split copied attached sounds with silent pauses"],
        vec![(0, original(2)), (1, Expected::Background), (2, original(3)), (3, Expected::Background), (4, original(4)), (5, original(5)), (6, original(3)), (7, original(4)), (8, original(5))],
        vec!["Two silent one-frame pauses split a copied and shared-EQ sound owner. Three independently read pre-edit PCM windows retain their chronological integral sample shifts within 1e-6.".into()],
    )?;
    fixture.audio = vec![(boundary(1)? + 512, false), (boundary(3)? + 512, false)];
    Ok(fixture)
}

pub fn partial_repeat_attached_sounds(dir: &Path) -> Result<Fixture> {
    partial_repeat_sound_fixture(dir, false)
}

pub fn partial_existing_repeat_attached_sounds(dir: &Path) -> Result<Fixture> {
    partial_repeat_sound_fixture(dir, true)
}

pub fn occurrence_pauses(dir: &Path) -> Result<Fixture> {
    use deadpan_core::{OccurrencePauseIdentities, PauseInsertionTarget};
    let mut project = copied_project(dir)?;
    project.apply(|_, document, revision| {
        Ok(Command::RepeatSelection {
            parent: document.root().clone(),
            selection: deadpan_core::SliceCaptureSelection::Child {
                node: root_children(document)?[0].clone(),
            },
            plays: 2,
            identities: deadpan_core::RepeatSelectionIdentities {
                repeat: NodeId::new("repeat")?,
                group: None,
                split: SplitIdentities::default(),
            },
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })?;
    project.apply(|_, _, revision| {
        Ok(Command::SetRepeatGaps {
            node: NodeId::new("repeat")?,
            gap: Some(HoldRecipe {
                duration: FrameDuration::new(2)?,
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
                picture_context: None,
            }),
            branches: vec![],
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })?;
    let before = [0, 2, 6, 10]
        .map(|frame| pcm(&project, boundary(frame).unwrap() + 512))
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    for at in [2, 6] {
        project.apply(|project, document, revision| {
            let PauseInsertionTarget::Occurrence(target) =
                document.pause_insertion_target(ProjectFrame(at))?
            else {
                return Err("expected occurrence".into());
            };
            Ok(Command::InsertOccurrenceTime {
                at: ProjectFrame(at),
                within: None,
                id: project.fresh(1)?.remove(0),
                hold: HoldRecipe {
                    duration: FrameDuration::new(1)?,
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                    picture_context: None,
                },
                timing: AudioTimingId {
                    allocation: revision.clone(),
                    ordinal: 0,
                },
                ai: false,
                all_plays: false,
                identities: OccurrencePauseIdentities {
                    isolation: OccurrenceIdentities {
                        nodes: project.fresh(target.isolation_nodes)?,
                        marks: (0..target.isolation_marks)
                            .map(|n| MarkId::new(format!("{revision}-mark-{n}")))
                            .collect::<std::result::Result<_, _>>()?,
                    },
                    split: SplitIdentities {
                        nodes: project.fresh(target.split_nodes)?,
                    },
                    container: if target.container {
                        Some(project.fresh(1)?.remove(0))
                    } else {
                        None
                    },
                    gap: if target.gap {
                        Some(project.fresh(1)?.remove(0))
                    } else {
                        None
                    },
                },
            })
        })?;
    }
    for (expected, frame) in before.iter().zip([0, 3, 8, 12]) {
        same_pcm(
            expected,
            &pcm(&project, boundary(frame)? + 512)?,
            "one-play/gap pause retains attached PCM",
        );
    }
    let mut fixture = project.finish(
        vec!["One-play and one-gap pauses"],
        vec![(0, original(2)), (1, original(3)), (2, Expected::Background), (3, original(4)), (4, original(5)), (5, Expected::Background), (6, Expected::Background), (7, Expected::Background), (8, original(2)), (9, original(3)), (10, original(4)), (11, original(5)), (12, original(3)), (13, original(4)), (14, original(5))],
        vec!["A copied Source with event and shared-owner EQ repeats twice. One silent frame splits only the first play; another splits only its gap. Four pre-edit limited PCM windows retain their samples within 1e-6.".into()],
    )?;
    fixture.audio = [2, 5, 6, 7]
        .map(|frame| Ok((boundary(frame)? + 512, false)))
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    Ok(fixture)
}

fn partial_repeat_sound_fixture(dir: &Path, existing: bool) -> Result<Fixture> {
    let mut project = copied_project(dir)?;
    if existing {
        project.apply(|_, document, revision| {
            Ok(Command::RepeatSelection {
                parent: document.root().clone(),
                selection: deadpan_core::SliceCaptureSelection::Child {
                    node: root_children(document)?[0].clone(),
                },
                plays: 2,
                identities: deadpan_core::RepeatSelectionIdentities {
                    repeat: NodeId::new("existing-repeat")?,
                    group: None,
                    split: SplitIdentities::default(),
                },
                timing: AudioTimingId {
                    allocation: revision.clone(),
                    ordinal: 0,
                },
            })
        })?;
    }
    let selected = pcm(&project, boundary(1)? + 512)?;
    let repeat = NodeId::new("partial-repeat")?;
    project.apply(|project, document, revision| {
        let parent = if existing {
            document.root().clone()
        } else {
            root_children(document)?[0].clone()
        };
        let selection = deadpan_core::SliceCaptureSelection::Range {
            range: FrameRange::new(ProjectFrame(1), ProjectFrame(3))?,
        };
        let plan = document.repeat_selection(&parent, &selection, 2)?;
        Ok(Command::RepeatSelection {
            parent,
            selection,
            plays: 2,
            identities: deadpan_core::RepeatSelectionIdentities {
                repeat: repeat.clone(),
                group: Some(project.fresh(1)?.remove(0)),
                split: SplitIdentities {
                    nodes: project.fresh(plan.required_split_ids)?,
                },
            },
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })?;
    same_pcm(
        &selected,
        &pcm(&project, boundary(1)? + 512)?,
        "first partial play",
    );
    same_pcm(
        &selected,
        &pcm(&project, boundary(3)? + 512)?,
        "second partial play",
    );
    project.apply(|_, _, revision| {
        Ok(Command::SetRepeatPlays {
            node: repeat.clone(),
            plays: 3,
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })?;
    let born = pcm(&project, boundary(5)? + 512)?;
    project.apply(|_, _, revision| {
        Ok(Command::SetRepeatGaps {
            node: repeat.clone(),
            gap: Some(HoldRecipe {
                duration: FrameDuration::new(1)?,
                video: HoldVideo::Background,
                picture_context: None,
                audio: HoldAudio::Silence,
            }),
            branches: vec![],
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })?;
    same_pcm(
        &selected,
        &pcm(&project, boundary(4)? + 512)?,
        "old play after gap",
    );
    same_pcm(
        &born,
        &pcm(&project, boundary(7)? + 512)?,
        "new play after gap",
    );
    assert_eq!(project.document()?.beat_sounds().len(), 4);
    let final_document = project.document()?;
    success(&[
        "project",
        "undo",
        project.path(),
        "--expected",
        final_document.revision_id().as_str(),
    ])?;
    let undone = project.document()?;
    assert_eq!(undone.duration()?.frames(), if existing { 15 } else { 11 });
    success(&[
        "project",
        "redo",
        project.path(),
        "--expected",
        undone.revision_id().as_str(),
    ])?;
    let restored = project.document()?;
    assert_ne!(restored.revision_id(), final_document.revision_id());
    let mut expected_restored = serde_json::to_value(&final_document)?;
    expected_restored["revision_id"] = serde_json::to_value(restored.revision_id())?;
    assert_eq!(serde_json::to_value(&restored)?, expected_restored);
    same_pcm(
        &born,
        &pcm(&project, boundary(7)? + 512)?,
        "new play after durable redo",
    );
    let mut expected = vec![
        (0, original(2)),
        (1, original(3)),
        (2, original(4)),
        (3, Expected::Background),
        (4, original(3)),
        (5, original(4)),
        (6, Expected::Background),
        (7, original(3)),
        (8, original(4)),
        (9, original(5)),
    ];
    if existing {
        expected.extend((10..14).map(|frame| (frame, original(frame - 8))));
    }
    let tail = if existing { 14 } else { 10 };
    expected.extend((tail..tail + 3).map(|frame| (frame, original(frame - tail + 3))));
    let mut fixture = project.finish(
        vec!["Partial Repeat with attached sound"],
        expected,
        vec!["Repeat the middle two frames of a copied sound owner, add a new play and silent gaps, then durable Undo/Redo. Independent old/new-play PCM is retained through the gap edit.".into()],
    )?;
    fixture.name = if existing {
        "partial-existing-repeat-attached-sounds"
    } else {
        "partial-repeat-attached-sounds"
    };
    if existing {
        fixture.notes.push("The selected range lies inside an existing two-play Repeat; exact Split aliases preserve that Repeat's concrete play clocks.".into());
    }
    fixture.audio = vec![(boundary(3)? + 512, false), (boundary(6)? + 512, false)];
    Ok(fixture)
}
