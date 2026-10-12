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

pub fn partial_attached_sound_copies(dir: &Path) -> Result<Fixture> {
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
