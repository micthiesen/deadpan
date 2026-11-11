//! Actual decoded PCM for authored Retime changes over retained sampling clocks.
use super::*;

fn edit(
    document: &ProjectDocument,
    command: Command,
    name: &str,
) -> (ProjectDocument, EditTransaction) {
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(name).unwrap(),
        command,
    };
    let transaction = apply(document, &request).unwrap();
    let changed = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&changed).unwrap(), *document);
    (changed, transaction)
}

fn record(
    document: &ProjectDocument,
    provider: &mut FixtureProvider,
    faded: bool,
    pieces: &[u32],
) -> Vec<[f32; 2]> {
    provider.revisions.insert(document.revision_id().clone());
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(document).unwrap()));
    let total = usize::try_from(renderer.plan().audio_duration().unwrap().0).unwrap();
    let mut output = Vec::new();
    for requested in pieces.iter().copied().cycle() {
        if output.len() == total {
            break;
        }
        let count = requested.min(u32::try_from(total - output.len()).unwrap());
        let start = AudioSample(i64::try_from(output.len()).unwrap());
        let samples = if faded {
            renderer
                .read_edge_faded(provider, start, count, TIMEOUT, &AtomicBool::new(false))
                .unwrap()
                .samples
        } else {
            renderer
                .read(provider, start, count, TIMEOUT, &AtomicBool::new(false))
                .unwrap()
                .samples
        };
        output.extend(samples);
    }
    assert!(output.iter().flatten().any(|sample| sample.abs() > 0.01));
    output
}

/// Prefix pause, a shifted stage over a 4-frame source and a 2-frame suffix
/// at an exact 1,600-sample frame.
fn shifted_document(frames: i64, pitch: PitchPolicy) -> ProjectDocument {
    let rate = FrameRate::new(30, 1).unwrap();
    document_with_asset(
        rate,
        &["prefix", "rate", "suffix"],
        [
            ("prefix", hold(1)),
            ("source", source(rate, 4, 100..6500)),
            ("rate", retime("source", frames, 0..4, pitch)),
            ("suffix", source(rate, 2, 100..3300)),
        ],
        BTreeMap::new(),
        audio(0, 8197).span,
    )
}

#[test]
fn shifted_stages_keep_their_bound_pcm_through_insert_split_and_move() {
    const FRAME: usize = 1_600;
    for (frames, pitch) in [
        (4, PitchPolicy::Shift { semitones: 3 }),
        (3, PitchPolicy::Shift { semitones: -2 }),
    ] {
        let before = shifted_document(frames, pitch);
        let mut provider = FixtureProvider::new();
        let original = record(&before, &mut provider, false, &[256]);
        let stage = FRAME..FRAME * (1 + frames as usize);
        let end = FRAME * (3 + frames as usize);
        assert_eq!(original.len(), end);

        // A pause after the stage: everything before it is unchanged and the
        // suffix resumes two frames later on its own clock.
        let (paused, _) = edit(
            &before,
            Command::InsertTime {
                at: ProjectFrame(1 + frames),
                hold: HoldRecipe {
                    picture_context: None,
                    duration: duration(2),
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                },
                id: id("pause"),
                identities: SplitIdentities {
                    nodes: (0..8).map(|n| id(&format!("pause-{n}"))).collect(),
                },
                timing: AudioTimingId {
                    allocation: RevisionId::new("paused").unwrap(),
                    ordinal: 0,
                },
            },
            "paused",
        );
        let after = record(&paused, &mut provider, false, &[113, 256]);
        assert_eq!(after[..stage.end], original[..stage.end], "{pitch:?}");
        assert_eq!(
            after[stage.end + 2 * FRAME..],
            original[stage.end..],
            "{pitch:?}"
        );

        // Splitting the suffix changes no sample.
        let (split, _) = edit(
            &before,
            Command::Split {
                node: id("suffix"),
                at: duration(1),
                identities: SplitIdentities {
                    nodes: (0..8).map(|n| id(&format!("split-{n}"))).collect(),
                },
            },
            "split",
        );
        assert_eq!(record(&split, &mut provider, false, &[97, 256]), original);

        // Moving the prefix pause after the stage moves the stage's
        // processed output one frame earlier, sample for sample.
        let range = FrameRange::new(ProjectFrame(0), ProjectFrame(1)).unwrap();
        let destination = MoveRangeDestination::Seam {
            parent: id("root"),
            index: 2,
        };
        let query = before.range_move(&id("root"), range, &destination).unwrap();
        let (moved, _) = edit(
            &before,
            Command::MoveRange {
                source_revision: before.revision_id().clone(),
                source_parent: id("root"),
                range,
                destination,
                identities: SplitIdentities {
                    nodes: (0..query.required_ids)
                        .map(|n| id(&format!("move-{n}")))
                        .collect(),
                },
                timing: AudioTimingId {
                    allocation: RevisionId::new("moved").unwrap(),
                    ordinal: 0,
                },
            },
            "moved",
        );
        let shifted = record(&moved, &mut provider, false, &[256]);
        assert_eq!(
            shifted[..stage.end - FRAME],
            original[stage.clone()],
            "{pitch:?}"
        );
        assert_ne!(
            original[stage.clone()],
            record(
                &shifted_document(frames, PitchPolicy::Preserve),
                &mut provider,
                false,
                &[256]
            )[stage.clone()],
            "the shift is heard"
        );
    }
}

#[test]
fn edit_time_preserve_bounds_match_default_preparation_work_and_output() {
    let limits = StageLimits::default();
    assert_eq!(
        deadpan_core::MAX_PRESERVE_INPUT_SAMPLES,
        i128::from(limits.maximum_input_frames)
    );
    assert_eq!(
        deadpan_core::MAX_PRESERVE_OUTPUT_SAMPLES,
        i128::from(limits.maximum_output_frames)
    );
    assert_eq!(
        deadpan_core::MAX_PRESERVE_WORK_SAMPLES,
        i128::from(limits.maximum_prepared_frames)
    );
}
