//! Exercise command-created clocks through the ordinary authored audio bus.
use super::*;

struct ClockProvider {
    fixture: FixtureProvider,
    revision: RevisionId,
}

impl ClockProvider {
    fn new(document: &ProjectDocument) -> Self {
        Self {
            fixture: FixtureProvider::new(),
            revision: document.revision_id().clone(),
        }
    }
}

impl AudioSourceProvider for ClockProvider {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        assert_eq!(revision, &self.revision);
        self.fixture.source(
            project,
            &RevisionId::new("definition-revision").unwrap(),
            asset,
            cancelled,
        )
    }

    fn source_for_context(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        expected: &AssetRecord,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        assert_eq!(revision, &self.revision);
        assert_eq!(
            expected.source_qualification.as_ref().unwrap().as_str(),
            "c".repeat(64)
        );
        self.fixture.source_for_context(
            project,
            &RevisionId::new("definition-revision").unwrap(),
            asset,
            expected,
            cancelled,
        )
    }
}

fn edit(document: &ProjectDocument, revision: &str, command: Command) -> ProjectDocument {
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(revision).unwrap(),
        command,
    };
    let edit = apply(document, &request).unwrap();
    let after = edit.forward.apply(document).unwrap();
    assert_eq!(edit.inverse.apply(&after).unwrap(), *document);
    ProjectDocument::from_json(&after.to_json().unwrap()).unwrap()
}

fn insert_prefix(document: &ProjectDocument) -> ProjectDocument {
    let NodeKind::Hold { recipe } = hold(1, HoldAudio::Silence).kind else {
        unreachable!()
    };
    edit(
        document,
        "moved",
        Command::InsertTime {
            at: ProjectFrame(0),
            hold: recipe,
            id: id("inserted"),
            identities: SplitIdentities::default(),
            timing: AudioTimingId {
                allocation: RevisionId::new("moved").unwrap(),
                ordinal: 0,
            },
        },
    )
}

fn remove_prefix(document: &ProjectDocument) -> ProjectDocument {
    edit(
        document,
        "returned",
        Command::DeleteRipple {
            node: id("inserted"),
            timing: AudioTimingId {
                allocation: RevisionId::new("returned").unwrap(),
                ordinal: 0,
            },
        },
    )
}

fn ntsc(edge: AudioEdgePolicy) -> ProjectDocument {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let base = document(
        rate,
        &["lead", "owner", "tail"],
        [
            ("lead", hold(2, HoldAudio::Silence)),
            ("owner", picture_only(rate, 4)),
            // Keep the event's historical edge separate from current Hold gates.
            ("tail", picture_only(rate, 2)),
        ],
        BTreeMap::new(),
    );
    let mut wire = serde_json::to_value(base).unwrap();
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("c".repeat(64));
    let mut recipe = voice_recipe(rate, 6408);
    recipe.mapping = SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::ZERO,
        frames: recipe.mapping.duration_frames(FrameDuration::ZERO).unwrap(),
        selection: ExactFrameRange {
            start: ExactRatio::ZERO,
            end: ExactRatio::integer(4),
        },
    };
    wire["beat_sounds"] = serde_json::json!({"owner": {"effect": BeatSound {
        label: "Effect".into(), source: recipe.source, mapping: recipe.mapping, offset: recipe.offset,
        gain_millidecibels: 0, start_edge: edge, end_edge: edge, overflow: SoundOverflowPolicy::Reject,
    }}});
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn repeated_with_gap(child: &str, allocation: &str, gap: i64) -> BeatNode {
    BeatNode {
        kind: NodeKind::Repeat {
            child: id(child),
            iterations: IterationOrder::new(RevisionId::new(allocation).unwrap(), 2).unwrap(),
            gap: Some(HoldRecipe {
                duration: FrameDuration::new(gap).unwrap(),
                video: HoldVideo::Background,
                picture_context: None,
                audio: HoldAudio::Silence,
            }),
            escalation: None,
        },
        ..BeatNode::sequence("Repeated sound owner", Vec::new())
    }
}

fn nested_ntsc_copy_document() -> ProjectDocument {
    let mut wire = serde_json::to_value(ntsc(AudioEdgePolicy::Hard)).unwrap();
    wire["nodes"]["inner_repeat"] =
        serde_json::to_value(repeated_with_gap("owner", "inner-plays", 1)).unwrap();
    wire["nodes"]["inner_stage"] =
        serde_json::to_value(retime("inner_repeat", 6, 0..9, PitchPolicy::Preserve)).unwrap();
    wire["nodes"]["outer_stage"] =
        serde_json::to_value(retime("inner_stage", 4, 0..6, PitchPolicy::Preserve)).unwrap();
    wire["nodes"]["repeat"] =
        serde_json::to_value(repeated_with_gap("outer_stage", "outer-plays", 1)).unwrap();
    wire["nodes"]["destination_hold"] = serde_json::to_value(hold(1, HoldAudio::Silence)).unwrap();
    wire["nodes"]["root"]["kind"]["children"] =
        serde_json::json!(["lead", "repeat", "tail", "destination_hold"]);
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn frame_sample_boundary(frame: i64) -> i64 {
    ExactRatio::new(i128::from(frame) * 48_000 * 1001, 30_000)
        .unwrap()
        .round_even()
        .unwrap() as i64
}

fn sound_id(value: &str) -> SoundId {
    SoundId::new(value).unwrap()
}

type StereoPcm = Vec<[f32; 2]>;

fn render_nested_occurrences(
    document: &ProjectDocument,
    first_frame: i64,
) -> (StereoPcm, StereoPcm, StereoPcm) {
    let mut renderer = StageAudio::new(compile(document, false));
    let mut provider = ClockProvider::new(document);
    let mut render = |start: i64, end: i64| {
        let mut output = Vec::with_capacity(usize::try_from(end - start).unwrap());
        let mut cursor = start;
        while cursor < end {
            let count = u32::try_from((end - cursor).min(1024)).unwrap();
            output.extend(bus(&mut renderer, &mut provider, cursor, count).samples);
            cursor += i64::from(count);
        }
        output
    };
    let first_start = frame_sample_boundary(first_frame);
    let first_end = frame_sample_boundary(first_frame + 4);
    let gap_end = frame_sample_boundary(first_frame + 5);
    let second_end = frame_sample_boundary(first_frame + 9);
    (
        render(first_start, first_end),
        render(first_end, gap_end),
        render(gap_end, second_end),
    )
}

fn local_sample_boundary(start_frame: i64, local_frame: ExactRatio) -> i64 {
    ExactRatio::integer(start_frame)
        .checked_add(local_frame)
        .unwrap()
        .checked_mul(ExactRatio::new(8008, 5).unwrap())
        .unwrap()
        .round_even()
        .unwrap() as i64
        - frame_sample_boundary(start_frame)
}

fn assert_pcm_equal_outside_current_gate_context(
    expected: &[[f32; 2]],
    actual: &[[f32; 2]],
    expected_start: i64,
    actual_start: i64,
) {
    let hold_edges = [
        ExactRatio::ZERO,
        ExactRatio::new(16, 9).unwrap(),
        ExactRatio::new(20, 9).unwrap(),
        ExactRatio::integer(4),
    ];
    let mut compared = 0;
    let mut compared_nonzero = false;
    for (index, (expected, actual)) in expected.iter().zip(actual).enumerate() {
        let index = i64::try_from(index).unwrap();
        let near_old_edge = hold_edges
            .iter()
            .any(|edge| (index - local_sample_boundary(expected_start, *edge)).abs() <= 96);
        let near_current_edge = hold_edges
            .iter()
            .any(|edge| (index - local_sample_boundary(actual_start, *edge)).abs() <= 96);
        if !near_old_edge && !near_current_edge {
            assert_eq!(expected, actual, "sample offset {index}");
            compared += 1;
            compared_nonzero |= *expected != [0.0; 2];
        }
    }
    assert!(
        compared > 512,
        "too few samples compared outside Hold gates"
    );
    assert!(compared_nonzero, "comparison skipped all audible samples");
}

fn assert_inner_gap_silence(samples: &[[f32; 2]], start_frame: i64) {
    let gap_start = usize::try_from(local_sample_boundary(
        start_frame,
        ExactRatio::new(16, 9).unwrap(),
    ))
    .unwrap();
    let gap_end = usize::try_from(local_sample_boundary(
        start_frame,
        ExactRatio::new(20, 9).unwrap(),
    ))
    .unwrap();
    assert!(gap_start < gap_end && gap_end <= samples.len());
    assert!(
        samples[gap_start..gap_end]
            .iter()
            .all(|sample| *sample == [0.0; 2])
    );
}

fn slice_identities(slice: &CapturedEditSlice, prefix: &str) -> SlicePasteIdentities {
    let required = slice.identity_requirements().unwrap();
    SlicePasteIdentities {
        authored: OccurrenceIdentities {
            nodes: (0..required.nodes)
                .map(|index| id(&format!("{prefix}-node-{index}")))
                .collect(),
            marks: (0..required.marks)
                .map(|index| MarkId::new(format!("{prefix}-mark-{index}")).unwrap())
                .collect(),
        },
        aliases: (0..required.aliases)
            .map(|index| id(&format!("{prefix}-alias-{index}")))
            .collect(),
    }
}

fn capture_repeat_slice(
    document: &ProjectDocument,
    parent: &str,
    repeat: &NodeId,
    name: &str,
) -> CapturedEditSlice {
    CapturedEditSlice::capture_selection(
        document,
        &id(parent),
        &SliceCaptureSelection::Child {
            node: repeat.clone(),
        },
        AudioTimingId {
            allocation: RevisionId::new(name).unwrap(),
            ordinal: 0,
        },
    )
    .unwrap()
}

fn paste_repeat_slice(
    document: &ProjectDocument,
    parent: &str,
    index: usize,
    slice: &CapturedEditSlice,
    name: &str,
) -> ProjectDocument {
    edit(
        document,
        name,
        Command::SpliceSlice {
            parent: id(parent),
            index,
            slice: slice.clone(),
            identities: slice_identities(slice, name),
            timing: AudioTimingId {
                allocation: RevisionId::new(name).unwrap(),
                ordinal: 0,
            },
        },
    )
}

#[test]
fn command_created_clocks_keep_ntsc_pcm_and_edge_progress_through_move_and_return() {
    for edge in [AudioEdgePolicy::Hard, AudioEdgePolicy::Automatic] {
        let before = ntsc(edge);
        let original = bus(
            &mut StageAudio::new(compile(&before, false)),
            &mut FixtureProvider::new(),
            3203,
            6407,
        );
        assert_ne!(original.samples[6406], [0.0; 2]);
        let moved = insert_prefix(&before);
        let mut provider = ClockProvider::new(&moved);
        let mut renderer = StageAudio::new(compile(&moved, false));
        let actual = bus(&mut renderer, &mut provider, 4805, 6406);
        assert_eq!(actual.samples.len(), 6406);
        for (index, (actual, expected)) in actual.samples.iter().zip(&original.samples).enumerate()
        {
            assert_eq!(actual, expected, "{edge:?} moved sample {index}");
        }
        assert!(provider.fixture.context_calls > 0);

        let returned = remove_prefix(&moved);
        let mut expected = original.samples;
        expected[6406] = [0.0; 2];
        let plan = compile(&returned, false);
        let mut provider = ClockProvider::new(&returned);
        let full = bus(
            &mut StageAudio::new(plan.clone()),
            &mut provider,
            3203,
            6407,
        );
        assert_eq!(full.samples, expected);
        // The picture-only Original imposes no Hold mask. Bus suppression is
        // the intersection across contributions, not this event's route gaps.
        assert!(full.suppressed.is_empty());
        for (offset, count) in [(6300, 107), (0, 256), (1590, 37), (3200, 201)] {
            let mut cold = StageAudio::new(plan.clone());
            assert_eq!(
                bus(&mut cold, &mut provider, 3203 + offset, count).samples,
                expected[offset as usize..offset as usize + count as usize]
            );
        }
    }
}

#[test]
fn sound_clock_current_coincident_hold_owns_the_automatic_fade_boundary() {
    let before = ntsc(AudioEdgePolicy::Hard);
    let raw = bus(
        &mut StageAudio::new(compile(&before, false)),
        &mut FixtureProvider::new(),
        3203,
        6407,
    );
    let mut wire = serde_json::to_value(ntsc(AudioEdgePolicy::Automatic)).unwrap();
    wire["nodes"]["tail"] = serde_json::to_value(hold(2, HoldAudio::Silence)).unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let moved = insert_prefix(&before);
    let mut provider = ClockProvider::new(&moved);
    let mut renderer = StageAudio::new(compile(&moved, false));
    let actual = bus(&mut renderer, &mut provider, 4805 + 6310, 96);
    for (index, sample) in actual.samples.iter().enumerate() {
        // The current Hold starts at B(7)=11211. The old virtual edge moved
        // to 11212; exact coincidence combines policy but the current Hold
        // owns its physical label, so its final gain is 1/192, not 3/192.
        let gain = (2 * (95 - index) + 1) as f32 / 192.0;
        assert_eq!(
            *sample,
            raw.samples[6310 + index].map(|value| value * gain),
            "current Hold ramp sample {index}"
        );
    }
}

#[test]
fn saved_clocks_keep_complete_nested_preserve_while_current_gain_and_source_admission_stay_live() {
    let before = saved(&occurrence_document(), "owner", 3840);
    let moved = insert_prefix(&before);
    let plan = compile(&moved, false);
    let mut provider = ClockProvider::new(&moved);
    let mut renderer = StageAudio::new(plan.clone());
    let passage = stretch(&stretch(&raw_voice(3840), 2560, 3, 2), 1280, 2, 1);
    let expected: Vec<_> = passage
        .iter()
        .copied()
        .chain(passage.iter().copied())
        .collect();
    assert_eq!(
        bus(&mut renderer, &mut provider, 18, 2560).samples,
        expected
    );
    assert!(provider.fixture.context_calls > 0);
    for (offset, count) in [(2440, 120), (0, 128), (1240, 128)] {
        let mut cold = StageAudio::new(plan.clone());
        assert_eq!(
            bus(&mut cold, &mut provider, 18 + offset, count).samples,
            expected[offset as usize..offset as usize + count as usize]
        );
    }
    provider.fixture.unavailable = true;
    assert!(matches!(
        renderer.prepare_authored_bus(
            &mut provider,
            AudioSample(18),
            128,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Preparation(
            PreparationError::SourceUnavailable(_)
        ))
    ));

    // Owner gain is applied after the retained raw processing graph.
    let mut wire = serde_json::to_value(&moved).unwrap();
    wire["nodes"]["owner"]["audio_treatments"] = serde_json::to_value(trim(
        0,
        vec![GainRange::new(ExactRatio::ZERO, ExactRatio::integer(1920)).unwrap()],
    ))
    .unwrap();
    let muted = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut expected = expected;
    expected[..640].fill([0.0; 2]);
    expected[1280..1920].fill([0.0; 2]);
    assert_eq!(
        bus(
            &mut StageAudio::new(compile(&muted, false)),
            &mut ClockProvider::new(&muted),
            18,
            2560
        )
        .samples,
        expected
    );
}

#[test]
fn sound_clocks_translate_processed_ntsc_extent_through_nested_preserve() {
    let mut wire = serde_json::to_value(ntsc(AudioEdgePolicy::Hard)).unwrap();
    wire["nodes"]["inner"] =
        serde_json::to_value(retime("owner", 6, 0..4, PitchPolicy::Preserve)).unwrap();
    wire["nodes"]["outer"] =
        serde_json::to_value(retime("inner", 4, 0..6, PitchPolicy::Preserve)).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["lead", "outer", "tail"]);
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut provider = ClockProvider::new(&before);
    let original = bus(
        &mut StageAudio::new(compile(&before, false)),
        &mut provider,
        3203,
        6407,
    );
    assert!(original.samples.iter().any(|sample| *sample != [0.0; 2]));
    let moved = insert_prefix(&before);
    provider.revision = moved.revision_id().clone();
    assert_eq!(
        bus(
            &mut StageAudio::new(compile(&moved, false)),
            &mut provider,
            4805,
            6406
        )
        .samples,
        original.samples[..6406]
    );
    let returned = remove_prefix(&moved);
    provider.revision = returned.revision_id().clone();
    let plan = compile(&returned, false);
    let mut expected = original.samples;
    expected[6406] = [0.0; 2];
    let full = bus(
        &mut StageAudio::new(plan.clone()),
        &mut provider,
        3203,
        6407,
    );
    assert_eq!(full.samples, expected);
    assert!(full.suppressed.is_empty());
    for (offset, count) in [(6300, 107), (0, 128), (1601, 128), (3180, 97)] {
        assert_eq!(
            bus(
                &mut StageAudio::new(plan.clone()),
                &mut provider,
                3203 + offset,
                count
            )
            .samples,
            expected[offset as usize..offset as usize + count as usize]
        );
    }
}

#[test]
fn copied_nested_sound_clocks_keep_old_pcm_labels_and_terminal_clip_across_recopy() {
    let before = nested_ntsc_copy_document();
    // Create the destination Sequence before sound clocks exist. Grouping is
    // intentionally outside the retained-clock edit path exercised below.
    let grouped = edit(
        &before,
        "group-destination",
        Command::Group {
            parent: id("root"),
            start: 3,
            end: 4,
            id: id("destination-sequence"),
            label: "Destination".into(),
        },
    );
    let original = render_nested_occurrences(&grouped, 2);
    assert_eq!(original.1, vec![[0.0; 2]; original.1.len()]);
    assert_ne!(original.0.last().copied().unwrap(), [0.0; 2]);
    assert_ne!(original.2.last().copied().unwrap(), [0.0; 2]);

    let moved = insert_prefix(&grouped);
    let returned = remove_prefix(&moved);
    let clipped = render_nested_occurrences(&returned, 2);
    assert_eq!(
        &clipped.0[..clipped.0.len() - 1],
        &original.0[..original.0.len() - 1]
    );
    assert_eq!(
        &clipped.2[..clipped.2.len() - 1],
        &original.2[..original.2.len() - 1]
    );
    assert_eq!(clipped.0.last().copied().unwrap(), [0.0; 2]);
    assert_eq!(clipped.2.last().copied().unwrap(), [0.0; 2]);
    assert_eq!(clipped.1, vec![[0.0; 2]; clipped.1.len()]);

    // A returned-document reference uses the copy's gain so the f32 oracle
    // observes the same per-contribution multiply and f64 accumulation order.
    let mut source_event = returned.beat_sounds()[&id("owner")][&sound_id("effect")].clone();
    source_event.gain_millidecibels = -6000;
    let gained_reference = edit(
        &returned,
        "reference-gain",
        Command::SetBeatSound {
            owner: id("owner"),
            id: sound_id("effect"),
            event: source_event,
        },
    );
    let reference = render_nested_occurrences(&gained_reference, 2);
    assert_eq!(reference.1, vec![[0.0; 2]; reference.1.len()]);
    assert_eq!(reference.0.last().copied().unwrap(), [0.0; 2]);
    assert_eq!(reference.2.last().copied().unwrap(), [0.0; 2]);

    // The copied subtree gets fresh node/Repeat IDs in the destination scope.
    let first_slice = capture_repeat_slice(&returned, "root", &id("repeat"), "capture-first");
    let first_copy = paste_repeat_slice(
        &returned,
        "destination-sequence",
        1,
        &first_slice,
        "paste-first",
    );
    let first_copy_repeat = first_copy
        .children(&id("destination-sequence"))
        .nth(1)
        .unwrap()
        .clone();
    assert_ne!(first_copy_repeat, id("repeat"));
    let first_copy_owner = first_copy
        .beat_sounds()
        .keys()
        .find(|owner| *owner != &id("owner"))
        .unwrap()
        .clone();
    let mut copied_event = first_copy.beat_sounds()[&first_copy_owner][&sound_id("effect")].clone();
    copied_event.gain_millidecibels = -6000;
    let first_copy = edit(
        &first_copy,
        "first-copy-gain",
        Command::SetBeatSound {
            owner: first_copy_owner,
            id: sound_id("effect"),
            event: copied_event,
        },
    );

    // The copy begins at frame 14, a different NTSC phase from frame 2.
    // Render bounded per-stage blocks, then compare away from current Hold
    // envelopes while checking both transported terminal labels explicitly.
    let first_actual = render_nested_occurrences(&first_copy, 14);
    assert_eq!(first_actual.1, vec![[0.0; 2]; first_actual.1.len()]);
    assert_inner_gap_silence(&first_actual.0, 14);
    assert_inner_gap_silence(&first_actual.2, 19);
    for (expected, actual, old_frame, new_frame) in [
        (&reference.0, &first_actual.0, 2, 14),
        (&reference.2, &first_actual.2, 7, 19),
    ] {
        assert_eq!(expected.len(), actual.len());
        assert_eq!(actual.last().copied().unwrap(), [0.0; 2]);
        assert_pcm_equal_outside_current_gate_context(expected, actual, old_frame, new_frame);
    }

    // Recopying that fresh subtree at frame 23 crosses to the opposite rounded
    // allocation length. The already-clipped terminal PCM stays absent.
    let second_slice = capture_repeat_slice(
        &first_copy,
        "destination-sequence",
        &first_copy_repeat,
        "capture-second",
    );
    let second_copy = paste_repeat_slice(
        &first_copy,
        "destination-sequence",
        2,
        &second_slice,
        "paste-second",
    );
    let second_copy_repeat = second_copy
        .children(&id("destination-sequence"))
        .nth(2)
        .unwrap()
        .clone();
    let second_actual = render_nested_occurrences(&second_copy, 23);
    assert_eq!(second_actual.1, vec![[0.0; 2]; second_actual.1.len()]);
    assert_inner_gap_silence(&second_actual.0, 23);
    assert_inner_gap_silence(&second_actual.2, 28);
    for (expected, actual, old_frame, new_frame) in [
        (&reference.0, &second_actual.0, 2, 23),
        (&reference.2, &second_actual.2, 7, 28),
    ] {
        assert_eq!(actual.len() + 1, expected.len());
        assert_pcm_equal_outside_current_gate_context(expected, actual, old_frame, new_frame);
    }

    // A later phase restores the full frame allocation, but cannot restore the
    // terminal label clipped from each occurrence earlier in
    // the journal.
    let third_slice = capture_repeat_slice(
        &second_copy,
        "destination-sequence",
        &second_copy_repeat,
        "capture-third",
    );
    let third_copy = paste_repeat_slice(
        &second_copy,
        "destination-sequence",
        3,
        &third_slice,
        "paste-third",
    );
    let third_actual = render_nested_occurrences(&third_copy, 32);
    assert_eq!(third_actual.1, vec![[0.0; 2]; third_actual.1.len()]);
    assert_inner_gap_silence(&third_actual.0, 32);
    assert_inner_gap_silence(&third_actual.2, 37);
    for (expected, actual, old_frame, new_frame) in [
        (&reference.0, &third_actual.0, 2, 32),
        (&reference.2, &third_actual.2, 7, 37),
    ] {
        assert_eq!(actual.len(), expected.len());
        assert_eq!(actual.last().copied().unwrap(), [0.0; 2]);
        assert_pcm_equal_outside_current_gate_context(expected, actual, old_frame, new_frame);
    }
}

#[test]
fn neutral_groups_outside_retained_sound_scopes_preserve_pcm_and_allow_later_timing_edits() {
    let before = nested_ntsc_copy_document();
    let inserted = insert_prefix(&before);
    let clocked = remove_prefix(&inserted);
    let initial_pcm = render_nested_occurrences(&clocked, 2);
    assert_eq!(initial_pcm.1, vec![[0.0; 2]; initial_pcm.1.len()]);
    assert_eq!(initial_pcm.0.last().copied().unwrap(), [0.0; 2]);
    assert_eq!(initial_pcm.2.last().copied().unwrap(), [0.0; 2]);

    let original_sounds = clocked.beat_sounds().clone();
    let original_clocks = clocked.audio_bindings().sound_clocks().clone();
    let original_timings = clocked.audio_bindings().timings().clone();
    let grouped = edit(
        &clocked,
        "neutral-group",
        Command::Group {
            parent: id("root"),
            start: 1,
            end: 2,
            id: id("clock-wrapper"),
            label: "Clock wrapper".into(),
        },
    );
    assert_eq!(grouped.beat_sounds(), &original_sounds);
    assert_eq!(grouped.audio_bindings().sound_clocks(), &original_clocks);
    assert_eq!(grouped.audio_bindings().timings(), &original_timings);
    let grouped_pcm = render_nested_occurrences(&grouped, 2);
    assert_eq!(grouped_pcm, initial_pcm);

    let hold_recipe = match hold(1, HoldAudio::Silence).kind {
        NodeKind::Hold { recipe } => recipe,
        _ => unreachable!(),
    };
    let shifted = edit(
        &grouped,
        "after-group-insert",
        Command::InsertTime {
            at: ProjectFrame(0),
            hold: hold_recipe,
            id: id("after-group-hold"),
            identities: SplitIdentities { nodes: vec![] },
            timing: AudioTimingId {
                allocation: RevisionId::new("after-group-insert").unwrap(),
                ordinal: 0,
            },
        },
    );
    assert_eq!(
        shifted.audio_bindings().sound_clocks()[&id("owner")][&sound_id("effect")]
            .clocks()
            .len(),
        original_clocks[&id("owner")][&sound_id("effect")]
            .clocks()
            .len()
            + 1
    );
    let shifted_pcm = render_nested_occurrences(&shifted, 3);
    assert_eq!(shifted_pcm.0.len() + 1, initial_pcm.0.len());
    assert_eq!(shifted_pcm.2.len() + 1, initial_pcm.2.len());
    assert_inner_gap_silence(&shifted_pcm.0, 3);
    assert_inner_gap_silence(&shifted_pcm.2, 8);
    assert_pcm_equal_outside_current_gate_context(&initial_pcm.0, &shifted_pcm.0, 2, 3);
    assert_pcm_equal_outside_current_gate_context(&initial_pcm.2, &shifted_pcm.2, 7, 8);

    let returned = edit(
        &shifted,
        "after-group-delete",
        Command::DeleteRipple {
            node: id("after-group-hold"),
            timing: AudioTimingId {
                allocation: RevisionId::new("after-group-delete").unwrap(),
                ordinal: 0,
            },
        },
    );
    let after_timing_clocks = returned.audio_bindings().sound_clocks().clone();
    let after_timing_layouts = returned.audio_bindings().timings().clone();
    let returned_pcm = render_nested_occurrences(&returned, 2);
    assert_eq!(returned_pcm, initial_pcm);
    assert_eq!(returned_pcm.0.last().copied().unwrap(), [0.0; 2]);
    assert_eq!(returned_pcm.2.last().copied().unwrap(), [0.0; 2]);

    let ungrouped = edit(
        &returned,
        "neutral-ungroup",
        Command::Ungroup {
            node: id("clock-wrapper"),
        },
    );
    assert_eq!(ungrouped.beat_sounds(), &original_sounds);
    assert_eq!(
        ungrouped.audio_bindings().sound_clocks(),
        &after_timing_clocks
    );
    assert_eq!(ungrouped.audio_bindings().timings(), &after_timing_layouts);
    assert_eq!(render_nested_occurrences(&ungrouped, 2), returned_pcm);

    // The identity-based grouping path with no endpoint Splits is also a
    // neutral wrapper. Its caller-owned timing ID is validated but not stored.
    let selection = SliceCaptureSelection::Child { node: id("repeat") };
    let plan = ungrouped.group_selection(&id("root"), &selection).unwrap();
    assert_eq!(plan.required_split_ids, 0);
    let selected_group = edit(
        &ungrouped,
        "group-selection",
        Command::GroupSelection {
            parent: id("root"),
            selection,
            label: "Selected clock wrapper".into(),
            identities: GroupSelectionIdentities {
                group: id("selected-clock-wrapper"),
                split: SplitIdentities { nodes: vec![] },
            },
            timing: AudioTimingId {
                allocation: RevisionId::new("group-selection").unwrap(),
                ordinal: 0,
            },
        },
    );
    assert_eq!(selected_group.beat_sounds(), &original_sounds);
    assert_eq!(
        selected_group.audio_bindings().sound_clocks(),
        &after_timing_clocks
    );
    assert_eq!(
        selected_group.audio_bindings().timings(),
        &after_timing_layouts
    );
    assert_eq!(render_nested_occurrences(&selected_group, 2), returned_pcm);
}

#[test]
fn retained_sound_and_current_bus_share_the_pcm_residency_cap() {
    let moved = insert_prefix(&ntsc(AudioEdgePolicy::Hard));
    let mut renderer = StageAudio::with_limits(
        compile(&moved, false),
        StageLimits {
            maximum_resident_frames: 1280,
            ..Default::default()
        },
    )
    .unwrap();
    let mut provider = ClockProvider::new(&moved);
    assert!(matches!(
        renderer.prepare_authored_bus(
            &mut provider,
            AudioSample(4805),
            256,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit(_))
    ));
    assert_eq!(provider.fixture.calls, 0);
    // A failed reservation leaves the current plan and controller usable.
    assert_eq!(renderer.plan().metadata().revision_id, *moved.revision_id());
    assert_eq!(
        bus(&mut renderer, &mut provider, 4805, 32).samples.len(),
        32
    );
}

/// `yab` then `p` carries a beat's own sound with its retained clock: at the
/// same 30000/1001 sample phase the pasted copy renders bit-identical decoded
/// PCM. `yib` then `p` pastes the same picture time without the sound, so the
/// copy (a picture-only Original) is exact digital silence. The original
/// occurrence is unchanged by either paste.
#[test]
fn beat_object_copies_carry_the_beat_sound_only_with_its_attachments() {
    let mut wire = serde_json::to_value(ntsc(AudioEdgePolicy::Hard)).unwrap();
    // Pad so the copy starts at frame 12: 10 frames after the owner's frame 2,
    // which is an exact multiple of 8008 samples on the NTSC grid.
    wire["nodes"]["pad"] = serde_json::to_value(hold(4, HoldAudio::Silence)).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["lead", "owner", "tail", "pad"]);
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let render = |document: &ProjectDocument, start: i64, end: i64| {
        let mut renderer = StageAudio::new(compile(document, false));
        let mut provider = ClockProvider::new(document);
        let mut output = Vec::new();
        let mut cursor = start;
        while cursor < end {
            let count = u32::try_from((end - cursor).min(1024)).unwrap();
            output.extend(bus(&mut renderer, &mut provider, cursor, count).samples);
            cursor += i64::from(count);
        }
        output
    };
    let (owner_start, owner_end) = (frame_sample_boundary(2), frame_sample_boundary(6));
    let (copy_start, copy_end) = (frame_sample_boundary(12), frame_sample_boundary(16));
    assert_eq!(copy_start - owner_start, 16_016);
    assert_eq!(copy_end - copy_start, owner_end - owner_start);
    let original = render(&before, owner_start, owner_end);
    assert!(original.iter().any(|sample| *sample != [0.0; 2]));

    for attachments in [SliceAttachments::Owned, SliceAttachments::Excluded] {
        let name = format!("{attachments:?}").to_lowercase();
        let slice = CapturedEditSlice::capture_selection_with(
            &before,
            &id("root"),
            &SliceCaptureSelection::Child { node: id("owner") },
            attachments,
            AudioTimingId {
                allocation: RevisionId::new(format!("capture-{name}")).unwrap(),
                ordinal: 0,
            },
        )
        .unwrap();
        slice.validate_capture(&before).unwrap();
        let pasted = paste_repeat_slice(&before, "root", 4, &slice, &format!("paste-{name}"));
        assert_eq!(pasted.duration().unwrap().frames(), 16);
        assert_eq!(render(&pasted, owner_start, owner_end), original);
        let copy = render(&pasted, copy_start, copy_end);
        assert_eq!(copy.len(), original.len());
        match attachments {
            SliceAttachments::Owned => {
                assert_eq!(pasted.beat_sounds().len(), 2);
                assert_eq!(copy, original, "the ab copy renders the same sound");
            }
            SliceAttachments::Excluded => {
                assert_eq!(pasted.beat_sounds().len(), 1);
                assert!(
                    copy.iter().all(|sample| *sample == [0.0; 2]),
                    "the ib copy has no sound of its own"
                );
            }
        }
    }
}
