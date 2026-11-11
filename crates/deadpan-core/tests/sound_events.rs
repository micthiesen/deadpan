use std::collections::BTreeMap;

use deadpan_core::*;
use serde_json::json;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn sound() -> SoundId {
    SoundId::new("effect").unwrap()
}

#[test]
fn independent_treatments_share_the_document_record_budget() {
    let document = fixture();
    let range = GainRange::new(ExactRatio::ZERO, ExactRatio::integer(64)).unwrap();
    let curve = GainEnvelope::new(
        GainClock::OwnerOutput,
        range,
        GainDb::UNITY,
        (1..=64)
            .map(|at| {
                GainSegment::new(
                    ExactRatio::integer(at),
                    GainDb::new(-3000).unwrap(),
                    GainCurve::Cubic {
                        control1: GainDb::UNITY,
                        control2: GainDb::new(-3000).unwrap(),
                    },
                )
                .unwrap()
            })
            .collect(),
    )
    .unwrap();
    let recipe = AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::UNITY, false, vec![curve; 16], vec![]).unwrap(),
    );
    assert!(recipe.record_count() * 64 > MAX_GAIN_RECORDS);
    let mut event = event(&document);
    event.treatments = recipe;
    let mut wire = serde_json::to_value(&document).unwrap();
    wire["sounds"] = serde_json::to_value(
        (0..64)
            .map(|i| (SoundId::new(format!("sound{i}")).unwrap(), event.clone()))
            .collect::<BTreeMap<_, _>>(),
    )
    .unwrap();
    let error = ProjectDocument::from_json(&wire.to_string()).unwrap_err();
    assert_eq!(error.code, DocumentErrorCode::LimitExceeded);
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
                        audio_editorial_edges: Default::default(),
                        audio_edges: Default::default(),
                        kind: NodeKind::Hold {
                            recipe: HoldRecipe {
                                duration: FrameDuration::new(100).unwrap(),
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                                picture_context: None,
                            },
                        },
                        cutaways: Vec::new(),
                        captions: Vec::new(),
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
        treatments: deadpan_core::AudioTreatments::default()
            .with_trim(deadpan_core::GainDb::new(-6000).unwrap())
            .unwrap(),
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
    updated.treatments = updated
        .treatments
        .with_trim(deadpan_core::GainDb::new(24_000).unwrap())
        .unwrap();
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
    for invalid_gain in [-96_001, 24_001] {
        let mut wire = serde_json::to_value(&original).unwrap();
        wire["treatments"]["clip_gain"]["trim"] = serde_json::json!(invalid_gain);
        assert!(serde_json::from_value::<SoundEvent>(wire).is_err());
    }
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
        assert!(
            failure
                .message
                .contains("move time under a placed sound effect")
        );
    }
    assert!(
        FrozenAudioContext::capture(&document)
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
    replacement.treatments = replacement
        .treatments
        .with_trim(deadpan_core::GainDb::new(-96_000).unwrap())
        .unwrap();
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

/// Split the 100-frame Hold into [0,40), [40,50), [50,95) and [95,100) and
/// place a sound over frames 60..90 (inside the third piece).
fn speed_fixture() -> (ProjectDocument, [NodeId; 4]) {
    let mut document = fixture();
    for (index, at) in [40, 50, 95].into_iter().enumerate() {
        let NodeKind::Sequence { children } = &document.nodes()[document.root()].kind else {
            panic!("root sequence")
        };
        let durations = document.durations().unwrap();
        let mut start = 0;
        let mut target = None;
        for child in children {
            let length = durations[child].frames();
            if at < start + length {
                target = Some(child.clone());
                break;
            }
            start += length;
        }
        let target = target.unwrap();
        let mut count = 3;
        let mut pending = vec![target.clone()];
        while let Some(next) = pending.pop() {
            count += 1;
            pending.extend(document.children(&next).cloned());
        }
        document = edit(
            &document,
            Command::Split {
                node: target,
                at: FrameDuration::new(at - start).unwrap(),
                identities: SplitIdentities {
                    nodes: (0..count)
                        .map(|n| node(&format!("split{index}-{n}")))
                        .collect(),
                },
            },
        )
        .0;
    }
    let NodeKind::Sequence { children } = &document.nodes()[document.root()].kind else {
        panic!("root sequence")
    };
    let children: [NodeId; 4] = children.clone().try_into().unwrap();
    let mut placed = event(&document);
    if let SourceAudioMapping::Placement { start, .. } = &mut placed.mapping {
        *start = ExactRatio::integer(60);
    }
    let (document, _) = place(&document, placed);
    (document, children)
}

fn last_operation(document: &ProjectDocument) -> Option<RootSoundOperation> {
    document.sound_routes()[&sound()]
        .edits
        .last()
        .map(|edit| edit.operation)
}

#[test]
fn speed_changes_move_placed_sounds_by_the_time_they_add_or_remove() {
    let (document, children) = speed_fixture();
    assert_eq!(
        document.duration().unwrap(),
        FrameDuration::new(100).unwrap()
    );
    let speed = |target: &NodeId, duration: i64, pitch: PitchPolicy| Command::WrapRetime {
        node: target.clone(),
        id: node("speed"),
        duration: FrameDuration::new(duration).unwrap(),
        pitch,
    };
    // Before the sound: [0,40) at double speed removes its last 20 frames, so
    // the sound at 60..90 moves to 40..70.
    let (faster, _) = edit(&document, speed(&children[0], 20, PitchPolicy::Preserve));
    assert_eq!(faster.duration().unwrap(), FrameDuration::new(80).unwrap());
    assert_eq!(
        last_operation(&faster),
        Some(RootSoundOperation::Delete {
            range: FrameRange::new(ProjectFrame(20), ProjectFrame(40)).unwrap(),
        })
    );
    assert_eq!(faster.sounds(), document.sounds());
    // After the sound: slowing [95,100) inserts its extra frames at 100, so
    // the sound stays at 60..90.
    let (slower, _) = edit(&document, speed(&children[3], 10, PitchPolicy::FollowSpeed));
    assert_eq!(slower.duration().unwrap(), FrameDuration::new(105).unwrap());
    assert_eq!(
        last_operation(&slower),
        Some(RootSoundOperation::Insert {
            at: ProjectFrame(100),
            duration: FrameDuration::new(5).unwrap(),
        })
    );
    // Over the sound: speeding [50,95) up to 30 frames removes 80..95, which
    // cuts the sound's 80..90 tail like a deletion there would.
    let (overlapped, transaction) = edit(&document, speed(&children[2], 30, PitchPolicy::Preserve));
    assert_eq!(
        last_operation(&overlapped),
        Some(RootSoundOperation::Delete {
            range: FrameRange::new(ProjectFrame(80), ProjectFrame(95)).unwrap(),
        })
    );
    assert!(overlapped.sounds().contains_key(&sound()));
    // Undo restores the sound and its route exactly.
    let undone = transaction.inverse.apply(&overlapped).unwrap();
    assert_eq!(undone.sounds(), document.sounds());
    assert_eq!(undone.sound_routes(), document.sound_routes());
    // A later speed change of the new stage is recorded the same way.
    let (changed, _) = edit(
        &faster,
        Command::SetRetime {
            node: node("speed"),
            duration: FrameDuration::new(40).unwrap(),
            pitch: PitchPolicy::Preserve,
        },
    );
    assert_eq!(
        last_operation(&changed),
        Some(RootSoundOperation::Insert {
            at: ProjectFrame(20),
            duration: FrameDuration::new(20).unwrap(),
        })
    );
}
