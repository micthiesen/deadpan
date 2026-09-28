use std::collections::BTreeMap;

use deadpan_core::*;
use serde_json::json;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn sound() -> SoundId {
    SoundId::new("effect").unwrap()
}

fn span(start: i64, end: i64) -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 44_100).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base,
        },
        SourceTimestamp {
            ticks: end,
            time_base,
        },
    )
    .unwrap()
}

fn request(document: &ProjectDocument, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(format!("{}x", document.revision_id())).unwrap(),
        command,
    }
}

fn edit(document: &ProjectDocument, command: Command) -> (ProjectDocument, EditTransaction) {
    let request = request(document, command);
    let request = serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap();
    let transaction = apply(document, &request).unwrap();
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    assert_eq!(transaction.forward.inverse(), transaction.inverse);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    (after, transaction)
}

fn asset() -> AssetRecord {
    AssetRecord {
        label: "Effect catalog source".into(),
        content_hash: "a".repeat(64),
        audio: Some(span(-44_100, 88_200)),
        video: None,
        frame_count: None,
        still_image: false,
        source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
    }
}

fn fixture() -> ProjectDocument {
    let document = ProjectDocument::new(
        ProjectId::new("sound-events").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 640,
            height: 480,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let (document, _) = edit(
        &document,
        Command::ImportSource {
            id: AssetId::new("catalog").unwrap(),
            asset: asset(),
            insertion: None,
            primary: None,
        },
    );
    edit(
        &document,
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("hold"),
                nodes: BTreeMap::from([(
                    node("hold"),
                    BeatNode {
                        audio_treatments: Default::default(),
                        label: "Picture time".into(),
                        framing: None,
                        audio_edges: Default::default(),
                        kind: NodeKind::Hold {
                            recipe: HoldRecipe {
                                duration: FrameDuration::new(100).unwrap(),
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                                picture_context: None,
                            },
                        },
                    },
                )]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )
    .0
}

fn event(document: &ProjectDocument) -> SoundEvent {
    let source = SourceAudio {
        asset: AssetId::new("catalog").unwrap(),
        span: span(0, 44_100),
    };
    let natural =
        SourceAudioMapping::natural_rate(source.span, document.presentation_basis().frame_rate)
            .unwrap();
    SoundEvent {
        owner: node("root"),
        label: "Clang".into(),
        source,
        mapping: SourceAudioMapping::Placement {
            start: ExactRatio::new(1, 7).unwrap(),
            frames: natural
                .duration_frames(document.duration().unwrap())
                .unwrap(),
        },
        offset: AudioSample(13),
        gain_millidecibels: -6000,
        start_edge: AudioEdgePolicy::Automatic,
        end_edge: AudioEdgePolicy::Hard,
        overflow: SoundOverflowPolicy::Reject,
    }
}

fn place(document: &ProjectDocument, event: SoundEvent) -> (ProjectDocument, EditTransaction) {
    edit(document, Command::SetSound { id: sound(), event })
}

#[test]
fn upsert_and_delete_are_guarded_reversible_nonstructural_edits() {
    let before = fixture();
    assert!(!before.to_json().unwrap().contains("\"sounds\""));
    let original = event(&before);
    let (placed, transaction) = place(&before, original.clone());
    assert_eq!(placed.sounds()[&sound()], original);
    assert_eq!(placed.nodes(), before.nodes());
    assert_eq!(placed.duration().unwrap(), before.duration().unwrap());
    assert_eq!(placed.presentation_basis(), before.presentation_basis());
    assert_eq!(transaction.duration_delta, 0);
    assert_eq!(transaction.changed_ids, [node("root")]);
    assert!(transaction.forward.nodes.is_empty());
    assert_eq!(transaction.forward.sounds[&sound()].before, None);
    let mut updated = original.clone();
    updated.offset = AudioSample(48_013);
    updated.gain_millidecibels = 24_000;
    let (changed, update) = place(&placed, updated.clone());
    assert_eq!(update.forward.sounds[&sound()].before, Some(original));
    assert_eq!(update.forward.sounds[&sound()].after, Some(updated));
    assert_eq!(changed.sounds().len(), 1);
    let mut broken = update.forward.clone();
    broken.sounds.get_mut(&sound()).unwrap().before = None;
    assert_eq!(
        broken.apply(&placed).unwrap_err().code,
        EditErrorCode::PatchConflict
    );
    let (removed, deletion) = edit(&changed, Command::DeleteSound { id: sound() });
    assert!(removed.sounds().is_empty());
    assert_eq!(removed.nodes(), before.nodes());
    assert_eq!(deletion.changed_ids, [node("root")]);
    assert_eq!(
        apply(
            &removed,
            &request(&removed, Command::DeleteSound { id: sound() })
        )
        .unwrap_err()
        .code,
        EditErrorCode::SelectionUnavailable
    );
}

#[test]
fn selection_and_sample_offset_preserve_the_complete_source_phase() {
    let document = fixture();
    let mut selected = event(&document);
    let start = ExactRatio::integer(-10);
    let frames = selected
        .mapping
        .duration_frames(document.duration().unwrap())
        .unwrap();
    selected.mapping = SourceAudioMapping::SelectedPlacement {
        start,
        frames,
        selection: ExactFrameRange {
            start: ExactRatio::ZERO,
            end: ExactRatio::integer(10),
        },
    };
    let (placed, _) = place(&document, selected.clone());
    assert_eq!(placed.sounds()[&sound()].mapping, selected.mapping);
    let interval = selected
        .mapping
        .selection_frames_with_offset(
            document.duration().unwrap(),
            selected.offset,
            document.presentation_basis().frame_rate,
        )
        .unwrap();
    assert_eq!(interval.start, ExactRatio::new(65, 8008).unwrap());
    assert_eq!(
        interval.end.checked_sub(interval.start).unwrap(),
        ExactRatio::integer(10)
    );
    assert_eq!(
        selected
            .mapping
            .duration_frames(document.duration().unwrap())
            .unwrap(),
        ExactRatio::new(30_000, 1001).unwrap()
    );
    selected.offset = AudioSample(-1);
    assert!(
        apply(
            &document,
            &request(
                &document,
                Command::SetSound {
                    id: sound(),
                    event: selected
                }
            )
        )
        .is_err()
    );
}

#[test]
fn invalid_source_rate_owner_extent_and_gain_are_rejected_atomically() {
    let document = fixture();
    let original = event(&document);
    let mut invalid = Vec::new();
    let mut event = original.clone();
    event.owner = node("hold");
    invalid.push(event);
    let mut event = original.clone();
    event.source.asset = AssetId::new("missing").unwrap();
    invalid.push(event);
    let mut event = original.clone();
    event.source.span = span(-44_101, 0);
    invalid.push(event);
    let mut event = original.clone();
    event.mapping = SourceAudioMapping::FitBeat;
    invalid.push(event);
    let mut event = original.clone();
    event.mapping = SourceAudioMapping::Duration {
        frames: ExactRatio::integer(30),
    };
    invalid.push(event);
    let mut event = original.clone();
    event.offset = AudioSample(-48_000);
    invalid.push(event);
    let mut event = original.clone();
    event.offset = AudioSample(160_160);
    invalid.push(event);
    let mut event = original.clone();
    event.offset = AudioSample(i64::MAX);
    invalid.push(event);
    let mut event = original.clone();
    event.gain_millidecibels = -96_001;
    invalid.push(event);
    let mut event = original.clone();
    event.gain_millidecibels = 24_001;
    invalid.push(event);
    let mut event = original.clone();
    event.mapping = SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::ZERO,
        frames: ExactRatio::new(30_000, 1001).unwrap(),
        selection: ExactFrameRange {
            start: ExactRatio::ONE,
            end: ExactRatio::ONE,
        },
    };
    invalid.push(event);
    for event in invalid {
        assert!(
            apply(
                &document,
                &request(
                    &document,
                    Command::SetSound {
                        id: sound(),
                        event: event.clone()
                    }
                )
            )
            .is_err(),
            "accepted {event:?}"
        );
        let mut wire = serde_json::to_value(&document).unwrap();
        wire["sounds"] = json!({"effect": event});
        assert!(ProjectDocument::from_json(&wire.to_string()).is_err());
    }
    assert!(document.sounds().is_empty());
    let mut unqualified = serde_json::to_value(&document).unwrap();
    unqualified["assets"]["catalog"]
        .as_object_mut()
        .unwrap()
        .remove("source_qualification");
    let unqualified = ProjectDocument::from_json(&unqualified.to_string()).unwrap();
    assert!(
        apply(
            &unqualified,
            &request(
                &unqualified,
                Command::SetSound {
                    id: sound(),
                    event: original
                }
            )
        )
        .is_err()
    );
}

#[test]
fn exact_terminal_boundary_fits_but_one_sample_of_overflow_does_not() {
    let document = fixture();
    let mut event = event(&document);
    let frames = event
        .mapping
        .duration_frames(document.duration().unwrap())
        .unwrap();
    event.mapping = SourceAudioMapping::Placement {
        start: ExactRatio::integer(100).checked_sub(frames).unwrap(),
        frames,
    };
    event.offset = AudioSample(0);
    place(&document, event.clone());
    event.offset = AudioSample(1);
    assert!(
        apply(
            &document,
            &request(&document, Command::SetSound { id: sound(), event })
        )
        .is_err()
    );
}

#[test]
fn temporal_and_occurrence_edits_fail_before_transforming_retained_context() {
    let document = fixture();
    let (document, _) = place(&document, event(&document));
    let commands = [
        Command::SetHoldDuration {
            node: node("hold"),
            duration: FrameDuration::new(101).unwrap(),
        },
        Command::Split {
            node: node("root"),
            at: FrameDuration::new(50).unwrap(),
            identities: SplitIdentities { nodes: vec![] },
        },
        Command::WrapRetime {
            node: node("hold"),
            id: node("retime"),
            duration: FrameDuration::new(100).unwrap(),
            pitch: PitchPolicy::Preserve,
        },
        Command::EditOccurrence {
            instance: InstancePath {
                node: node("hold"),
                repeats: vec![],
            },
            edit: OccurrenceEdit::Delete,
            identities: OccurrenceIdentities::default(),
        },
    ];
    for command in commands {
        let failure = apply(&document, &request(&document, command)).unwrap_err();
        assert_eq!(failure.code, EditErrorCode::InvalidCommand);
        assert!(failure.message.contains("sound intervals and sample phase"));
    }
    assert!(
        FrozenAudioContext::capture(&document)
            .unwrap_err()
            .message
            .contains("sound events")
    );
    assert!(
        capture_unbound_audio_bindings(
            &document,
            AudioTimingId {
                allocation: RevisionId::new("capture").unwrap(),
                ordinal: 0,
            }
        )
        .unwrap_err()
        .message
        .contains("sound events")
    );
    let (renamed, _) = edit(
        &document,
        Command::Rename {
            node: node("hold"),
            label: "New label".into(),
        },
    );
    assert_eq!(renamed.sounds(), document.sounds());
    let (edge, _) = edit(
        &document,
        Command::SetAudioEdge {
            node: node("root"),
            edge: AudioBoundaryKind::NodeStart,
            policy: AudioEdgePolicy::Hard,
        },
    );
    assert_eq!(edge.sounds(), document.sounds());
    let (catalog, _) = edit(
        &document,
        Command::ImportSource {
            id: AssetId::new("second").unwrap(),
            asset: asset(),
            insertion: None,
            primary: None,
        },
    );
    assert_eq!(catalog.sounds(), document.sounds());
    let (removed, _) = edit(&document, Command::DeleteSound { id: sound() });
    edit(
        &removed,
        Command::SetHoldDuration {
            node: node("hold"),
            duration: FrameDuration::new(101).unwrap(),
        },
    );
}

#[test]
fn bounded_live_inventory_allows_replacement_and_rejects_unknown_wire_fields() {
    let mut document = fixture();
    let event = event(&document);
    for index in 0..MAX_DOCUMENT_SOUNDS {
        document = edit(
            &document,
            Command::SetSound {
                id: SoundId::new(format!("effect{index}")).unwrap(),
                event: event.clone(),
            },
        )
        .0;
    }
    assert_eq!(document.sounds().len(), 64);
    let failure = apply(
        &document,
        &request(
            &document,
            Command::SetSound {
                id: sound(),
                event: event.clone(),
            },
        ),
    )
    .unwrap_err();
    assert_eq!(failure.code, EditErrorCode::LimitExceeded);
    let mut replacement = event.clone();
    replacement.gain_millidecibels = -96_000;
    document = edit(
        &document,
        Command::SetSound {
            id: SoundId::new("effect0").unwrap(),
            event: replacement,
        },
    )
    .0;
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["sounds"]["effect64"] = json!(event);
    assert!(ProjectDocument::from_json(&wire.to_string()).is_err());
    for field in [
        "owner",
        "source",
        "mapping",
        "offset",
        "gain_millidecibels",
        "start_edge",
        "end_edge",
        "overflow",
    ] {
        let mut wire = serde_json::to_value(&event).unwrap();
        wire.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<SoundEvent>(wire).is_err());
    }
    let mut wire = serde_json::to_value(&event).unwrap();
    wire["future"] = json!(true);
    assert!(serde_json::from_value::<SoundEvent>(wire).is_err());
    assert!(serde_json::from_value::<SoundOverflowPolicy>(json!("trim")).is_err());
}

#[test]
fn frozen_command_vocabularies_reject_sound_authoring() {
    let document = fixture();
    let request = serde_json::to_string(&request(
        &document,
        Command::SetSound {
            id: sound(),
            event: event(&document),
        },
    ))
    .unwrap();
    macro_rules! check {
        ($($module:ident),+ $(,)?) => { $(assert!($module::upgrade_request(&request).is_err(), stringify!($module));)+ };
    }
    check!(
        legacy_v1, legacy_v2, legacy_v3, legacy_v4, legacy_v5, legacy_v6, legacy_v7, legacy_v8,
        legacy_v9, legacy_v10, legacy_v11, legacy_v12, legacy_v13, legacy_v14, legacy_v15,
        legacy_v16, legacy_v17, legacy_v18, legacy_v19, legacy_v20, legacy_v21, legacy_v22,
        legacy_v23, legacy_v24, legacy_v25, legacy_v26, legacy_v27
    );
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["schema_version"] = json!(27);
    let old = legacy_v27::Document::from_json(&wire.to_string()).unwrap();
    assert!(old.upgrade().unwrap().sounds().is_empty());
    let (placed, transaction) = place(&document, event(&document));
    wire = serde_json::to_value(&placed).unwrap();
    wire["schema_version"] = json!(27);
    assert!(legacy_v27::Document::from_json(&wire.to_string()).is_err());
    assert!(
        legacy_v27::matches_edit(&serde_json::to_string(&transaction).unwrap(), &transaction)
            .is_err()
    );
}
