//! Endpoint grouping preserves complete attached-sound PCM and processing history.
//! Uses the existing FixtureProvider WAV and ClockProvider admission convention.
use super::*;

const OLD_REFUSAL: &str = "group endpoint splits cannot yet preserve beat-owned sound clocks";

fn timing(name: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: RevisionId::new(name).unwrap(),
        ordinal: 0,
    }
}

fn eq(gains: [i32; 3]) -> AudioTreatments {
    AudioTreatments::default()
        .with_equalizer(Some(
            EqualizerRecipe::new(gains.map(|gain| GainDb::new(gain).unwrap()), None, None).unwrap(),
        ))
        .unwrap()
}

fn fixture(event_eq: bool, owner_eq: bool, original: bool) -> ProjectDocument {
    fixture_with_hold(event_eq, owner_eq, original, None)
}

fn fixture_with_hold(
    event_eq: bool,
    owner_eq: bool,
    original: bool,
    hold_audio: Option<HoldAudio>,
) -> ProjectDocument {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let owner = match hold_audio {
        Some(policy) => {
            assert!(!original);
            hold(12, policy)
        }
        None => picture_only(rate, 12),
    };
    // Picture-only neighbors avoid adding current Hold gates at the crop or copy.
    let base = document(
        rate,
        &["lead", "owner", "tail"],
        [
            ("lead", picture_only(rate, 2)),
            ("owner", owner),
            ("tail", picture_only(rate, 5)),
        ],
        BTreeMap::new(),
    );
    let source = audio(0..8197);
    let event = BeatSound {
        label: "Finite independent effect".into(),
        mapping: SourceAudioMapping::natural_rate(source.span, rate).unwrap(),
        source,
        offset: AudioSample(37),
        treatments: if event_eq {
            eq([6000, -6000, 3000])
        } else {
            AudioTreatments::default()
        },
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    let mut wire = serde_json::to_value(base).unwrap();
    // Existing synthetic ClockProvider context marker, not a new host receipt.
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("c".repeat(64));
    wire["beat_sounds"] = serde_json::json!({"owner": {"effect": event}});
    if owner_eq {
        wire["nodes"]["owner"]["audio_treatments"] =
            serde_json::to_value(eq([3000, -1500, 1000])).unwrap();
    }
    if original {
        let source = audio(0..8197);
        wire["nodes"]["owner"]["kind"]["source"]["audio"] = serde_json::to_value(&source).unwrap();
        wire["nodes"]["owner"]["kind"]["source"]["audio_mapping"] =
            serde_json::to_value(SourceAudioMapping::natural_rate(source.span, rate).unwrap())
                .unwrap();
    }
    let before = insert_prefix(&ProjectDocument::from_json(&wire.to_string()).unwrap());
    assert_eq!(before.duration().unwrap().frames(), 20);
    assert_eq!(
        before.source_splice_boundary(before.root(), 2).unwrap(),
        ProjectFrame(3)
    );
    for (frame, sample) in [(2, 3203), (3, 4805), (5, 8008), (8, 12813), (20, 32032)] {
        assert_eq!(frame_sample_boundary(frame), sample);
    }
    assert_eq!(frame_sample_boundary(3) - frame_sample_boundary(2), 1602);
    assert_eq!(frame_sample_boundary(4) - frame_sample_boundary(3), 1601);
    let journal = &before.audio_bindings().sound_clocks()[&id("owner")][&sound_id("effect")];
    assert_eq!(journal.scope(), &id("owner"));
    assert_ne!(journal.scope(), before.root());
    assert_eq!(journal.clocks().len(), 1);
    assert_eq!(journal.clocks()[0].timing(), &timing("moved"));
    before
}

fn selection() -> SliceCaptureSelection {
    SliceCaptureSelection::Range {
        range: FrameRange::new(ProjectFrame(5), ProjectFrame(8)).unwrap(),
    }
}

fn selected_group(
    document: &ProjectDocument,
    name: &str,
    selection: SliceCaptureSelection,
) -> Command {
    let plan = document
        .group_selection(document.root(), &selection)
        .unwrap();
    Command::GroupSelection {
        parent: document.root().clone(),
        selection,
        label: "Neutral endpoints".into(),
        identities: GroupSelectionIdentities {
            group: id(name),
            split: SplitIdentities {
                nodes: (0..plan.required_split_ids)
                    .map(|index| id(&format!("{name}-split-{index}")))
                    .collect(),
            },
        },
        timing: timing(name),
    }
}

fn read(document: &ProjectDocument, start: i64, end: i64, partitions: &[u32]) -> Vec<[f32; 2]> {
    assert!(!partitions.is_empty() && partitions.iter().all(|count| *count > 0));
    let mut reader = StageAudio::new(compile(document, false));
    let mut provider = ClockProvider::new(document);
    let mut output = Vec::with_capacity(usize::try_from(end - start).unwrap());
    let mut at = start;
    let mut index = 0;
    while at < end {
        let count = u32::try_from((end - at).min(i64::from(partitions[index]))).unwrap();
        output.extend(bus(&mut reader, &mut provider, at, count).samples);
        at += i64::from(count);
        index = (index + 1) % partitions.len();
    }
    // Silence needs no source read, and the carrier-only control has no sound
    // journal. Complete audible journal reads exercise historical context.
    if start == 0
        && end == frame_sample_boundary(document.duration().unwrap().frames())
        && !document.audio_bindings().sound_clocks().is_empty()
        && output.iter().any(|sample| *sample != [0.0; 2])
    {
        assert!(provider.fixture.context_calls > 0);
    }
    output
}

fn complete(document: &ProjectDocument) -> Vec<[f32; 2]> {
    read(
        document,
        0,
        frame_sample_boundary(document.duration().unwrap().frames()),
        &[1021],
    )
}

fn assert_complete(document: &ProjectDocument, expected: &[[f32; 2]]) {
    let end = frame_sample_boundary(document.duration().unwrap().frames());
    assert_eq!(expected.len(), usize::try_from(end).unwrap());
    assert_eq!(complete(document), expected, "1021-sample partitions");
    assert_eq!(
        read(document, 0, end, &[37, 509, 1021, 173]),
        expected,
        "different partitions"
    );
    // Each call creates both a fresh StageAudio and a fresh provider.
    // Cover onset, +37 offset, both seams, finite event end, owner end, terminal.
    for (first, last) in [
        (4804, 4844),
        (8007, 8009),
        (12812, 12814),
        (13037, 13041),
        (frame_sample_boundary(15) - 1, frame_sample_boundary(15) + 1),
        (end - 37, end),
    ] {
        assert_eq!(
            read(document, first, last, &[1021]),
            expected[usize::try_from(first).unwrap()..usize::try_from(last).unwrap()],
            "cold [{first},{last})",
        );
    }
}

fn variants() -> [(bool, bool, bool); 4] {
    [
        (false, false, false),
        (true, false, false),
        (true, true, false),
        (true, true, true),
    ]
}

#[test]
fn endpoint_grouping_plain_split_and_aligned_controls_preserve_complete_pcm() {
    // This standalone test must pass against the frozen old implementation.
    for (event_eq, owner_eq, original) in variants() {
        let before = fixture(event_eq, owner_eq, original);
        let expected = complete(&before);
        assert!(
            expected[8000..8020]
                .iter()
                .any(|sample| *sample != [0.0; 2])
        );
        assert!(
            expected[12800..12820]
                .iter()
                .any(|sample| *sample != [0.0; 2])
        );
        let split = edit(
            &before,
            "endpoint-control-split",
            Command::Split {
                node: id("owner"),
                at: FrameDuration::new(2).unwrap(),
                identities: SplitIdentities {
                    nodes: ["control-left", "control-right", "control-copy"]
                        .map(id)
                        .to_vec(),
                },
            },
        );
        assert_eq!(split.beat_sounds().len(), 2);
        assert_complete(&split, &expected);
        let aligned = SliceCaptureSelection::Child { node: id("owner") };
        assert_eq!(
            before
                .group_selection(before.root(), &aligned)
                .unwrap()
                .required_split_ids,
            0
        );
        let grouped = edit(
            &before,
            "endpoint-control-aligned",
            selected_group(&before, "endpoint-control-aligned", aligned),
        );
        assert_eq!(grouped.audio_bindings(), before.audio_bindings());
        assert_eq!(grouped.beat_sounds(), before.beat_sounds());
        assert_complete(&grouped, &expected);
    }
}

fn grouped(document: &ProjectDocument) -> ProjectDocument {
    assert_eq!(
        document
            .group_selection(document.root(), &selection())
            .unwrap()
            .required_split_ids,
        5
    );
    let command = selected_group(document, "endpoint-group", selection());
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new("endpoint-group").unwrap(),
        command,
    };
    let frozen = document.to_json().unwrap();
    let result = apply(document, &request);
    assert_eq!(document.to_json().unwrap(), frozen);
    if let Err(error) = &result {
        assert_eq!(error.code, EditErrorCode::InvalidCommand);
        assert_eq!(error.message, OLD_REFUSAL, "unexpected preflight failure");
    }
    assert!(
        result.is_ok(),
        "endpoint grouping should reuse Split transport: {:?}",
        result.as_ref().err()
    );
    // Existing helper verifies exact inverse equality and JSON round-trip.
    let after = edit(document, "endpoint-group", request.command);
    assert_eq!(after.duration().unwrap(), document.duration().unwrap());
    assert_eq!(
        after.node_duration(&id("endpoint-group")).unwrap().frames(),
        3
    );
    assert_eq!(after.beat_sounds().len(), 3);
    let old = &document.audio_bindings().sound_clocks()[&id("owner")][&sound_id("effect")];
    for (owner, events) in after.beat_sounds() {
        assert_eq!(events, &document.beat_sounds()[&id("owner")]);
        let journal = &after.audio_bindings().sound_clocks()[owner][&sound_id("effect")];
        assert_eq!(journal.clocks().len(), old.clocks().len() + 1);
        assert_eq!(journal.clocks()[0], old.clocks()[0]);
        assert_eq!(journal.clocks()[1].timing(), &timing("endpoint-group"));
        assert_eq!(journal.clocks()[1].owner(), &id("owner"));
        assert_eq!(journal.clocks()[1].scope(), &id("owner"));
    }
    after
}

#[test]
fn endpoint_groups_keep_every_ntsc_sample_through_event_and_shared_eq() {
    for (event_eq, owner_eq, original) in variants() {
        let before = fixture(event_eq, owner_eq, original);
        let expected = complete(&before);
        assert_complete(&grouped(&before), &expected);
    }
}

#[test]
fn endpoint_hold_groups_keep_permitted_sound_and_room_tone_phase() {
    let before = fixture_with_hold(
        true,
        true,
        false,
        Some(HoldAudio::RoomTone {
            source: audio(512..640),
        }),
    );
    let expected = complete(&before);
    let without_event = edit(
        &before,
        "endpoint-room-tone-only",
        Command::DeleteBeatSound {
            owner: id("owner"),
            id: sound_id("effect"),
        },
    );
    let carrier = complete(&without_event);
    for window in [8000..8020, 12800..12820] {
        assert!(
            expected[window.clone()]
                .iter()
                .zip(&carrier[window])
                .any(|(all, hold)| all != hold),
            "the independent event must be audible across each grouping seam",
        );
    }
    let split = edit(
        &before,
        "endpoint-hold-control-split",
        Command::Split {
            node: id("owner"),
            at: FrameDuration::new(2).unwrap(),
            identities: SplitIdentities {
                nodes: ["hold-left", "hold-right", "hold-copy"].map(id).to_vec(),
            },
        },
    );
    assert_complete(&split, &expected);
    let aligned = edit(
        &before,
        "endpoint-hold-control-aligned",
        selected_group(
            &before,
            "endpoint-hold-control-aligned",
            SliceCaptureSelection::Child { node: id("owner") },
        ),
    );
    assert_eq!(aligned.audio_bindings(), before.audio_bindings());
    assert_complete(&aligned, &expected);
    assert_complete(&grouped(&before), &expected);
}

#[test]
fn endpoint_silent_hold_groups_keep_default_independent_sound_suppression() {
    let before = fixture_with_hold(true, true, false, Some(HoldAudio::Silence));
    let expected = complete(&before);
    assert!(expected.iter().all(|sample| *sample == [0.0; 2]));
    assert_complete(&grouped(&before), &expected);
}

#[test]
fn endpoint_groups_keep_independent_root_sound_in_its_original_clock() {
    let before = fixture(true, true, true);
    let source = audio(0..4096);
    let root_sound = deadpan_core::SoundEvent {
        owner: before.root().clone(),
        label: "Independent root sound after the selected interval".into(),
        mapping: SourceAudioMapping::natural_rate(
            source.span,
            before.presentation_basis().frame_rate,
        )
        .unwrap(),
        source,
        offset: AudioSample(frame_sample_boundary(16)),
        treatments: eq([-3000, 1500, -500]),
        start_edge: AudioEdgePolicy::Hard,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    };
    let before = edit(
        &before,
        "endpoint-root-sound",
        Command::SetSound {
            id: sound_id("root-independent"),
            event: root_sound,
        },
    );
    let expected = complete(&before);
    assert!(
        expected[25626..29722]
            .iter()
            .any(|sample| *sample != [0.0; 2])
    );
    let after = grouped(&before);
    assert_eq!(after.sounds(), before.sounds());
    assert_complete(&after, &expected);
}

fn append_copy(
    document: &ProjectDocument,
    name: &str,
    selection: SliceCaptureSelection,
) -> ProjectDocument {
    let slice = CapturedEditSlice::capture_selection(
        document,
        document.root(),
        &selection,
        timing(&format!("{name}-capture")),
    )
    .unwrap();
    slice.validate_capture(document).unwrap();
    let slice = CapturedEditSlice::from_json(&slice.to_json().unwrap()).unwrap();
    edit(
        document,
        name,
        Command::SpliceSlice {
            parent: document.root().clone(),
            index: document.children(document.root()).count(),
            identities: slice_identities(&slice, name),
            slice,
            timing: timing(name),
        },
    )
}

#[test]
fn endpoint_group_control_range_copy_has_exact_pcm_at_same_ntsc_phase() {
    // Independent old-code control for the subsequent copy's literal oracle.
    for (event_eq, owner_eq, original) in variants() {
        let before = fixture(event_eq, owner_eq, original);
        let mut expected = complete(&before);
        let passage = expected[8008..12813].to_vec();
        assert_eq!(frame_sample_boundary(20) - frame_sample_boundary(5), 24024);
        let copy = append_copy(&before, "endpoint-control-copy", selection());
        assert_eq!(copy.duration().unwrap().frames(), 23);
        expected.extend(passage);
        assert_complete(&copy, &expected);
    }
}

#[test]
fn endpoint_group_copy_keeps_full_owner_processing_and_both_cropped_seams() {
    for (event_eq, owner_eq, original) in variants() {
        let before = fixture(event_eq, owner_eq, original);
        let mut expected = complete(&before);
        let passage = expected[8008..12813].to_vec();
        let after = grouped(&before);
        let copy = append_copy(
            &after,
            "endpoint-group-copy",
            SliceCaptureSelection::Child {
                node: id("endpoint-group"),
            },
        );
        assert_eq!(copy.duration().unwrap().frames(), 23);
        expected.extend(passage);
        assert_complete(&copy, &expected);
        for (first, last) in [(32031, 32034), (36835, 36837)] {
            assert_eq!(
                read(&copy, first, last, &[1021]),
                expected[usize::try_from(first).unwrap()..usize::try_from(last).unwrap()],
                "copy seam [{first},{last})",
            );
        }
    }
}
