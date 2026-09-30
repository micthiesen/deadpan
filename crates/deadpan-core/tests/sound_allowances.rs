use std::collections::BTreeMap;

use deadpan_core::*;
use serde_json::json;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn sound() -> SoundId {
    SoundId::new("effect").unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn hold(value: i64) -> HoldRecipe {
    HoldRecipe {
        duration: frames(value),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
        picture_context: None,
    }
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
    let request = serde_json::from_value(serde_json::to_value(request).unwrap()).unwrap();
    let transaction = apply(document, &request).unwrap();
    let result = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&result).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&result.to_json().unwrap()).unwrap(),
        result
    );
    (result, transaction)
}
fn address(target: &str, repeats: Vec<RepeatInstance>) -> SoundHoldIssuer {
    SoundHoldIssuer::Node {
        instance: InstancePath {
            node: node(target),
            repeats,
        },
    }
}
fn allow(document: &ProjectDocument, issuer: SoundHoldIssuer) -> ProjectDocument {
    edit(
        document,
        Command::SetSoundAllowance {
            sound: sound(),
            issuer,
            allowed: true,
        },
    )
    .0
}
fn allowance_ids(document: &ProjectDocument) -> Vec<NodeId> {
    document
        .sound_allowances()
        .get(&sound())
        .into_iter()
        .flat_map(|values| values.iter().map(|issuer| issuer.instance().node.clone()))
        .collect()
}
fn fixture(repeated: bool) -> ProjectDocument {
    let document = ProjectDocument::new(
        ProjectId::new("allowances").unwrap(),
        RevisionId::new("r").unwrap(),
        PresentationBasis {
            width: 640,
            height: 480,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 48_000,
            time_base,
        },
    )
    .unwrap();
    let document = edit(
        &document,
        Command::ImportSource {
            id: AssetId::new("catalog").unwrap(),
            asset: AssetRecord {
                label: "Effect".into(),
                content_hash: "a".repeat(64),
                video: None,
                audio: Some(span),
                still_image: false,
                frame_count: None,
                source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
            },
            insertion: None,
            primary: None,
        },
    )
    .0;
    let document = edit(
        &document,
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("hold"),
                nodes: BTreeMap::from([(
                    node("hold"),
                    BeatNode::hold("Pause", hold(if repeated { 10 } else { 120 })),
                )]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )
    .0;
    let document = if repeated {
        let inner = edit(
            &document,
            Command::WrapRepeat {
                node: node("hold"),
                id: node("inner"),
                plays: 3,
                gap: Some(hold(5)),
                anchor_policy: Default::default(),
            },
        )
        .0;
        edit(
            &inner,
            Command::WrapRepeat {
                node: node("inner"),
                id: node("outer"),
                plays: 3,
                gap: Some(hold(3)),
                anchor_policy: Default::default(),
            },
        )
        .0
    } else {
        document
    };
    edit(
        &document,
        Command::SetSound {
            id: sound(),
            event: SoundEvent {
                owner: node("root"),
                label: "Effect".into(),
                source: SourceAudio {
                    asset: AssetId::new("catalog").unwrap(),
                    span,
                },
                mapping: SourceAudioMapping::natural_rate(
                    span,
                    document.presentation_basis().frame_rate,
                )
                .unwrap(),
                offset: AudioSample(137),
                gain_millidecibels: 0,
                start_edge: AudioEdgePolicy::Automatic,
                end_edge: AudioEdgePolicy::Automatic,
                overflow: SoundOverflowPolicy::Reject,
            },
        },
    )
    .0
}
fn play(document: &ProjectDocument, repeat: &str, ordinal: u32) -> RepeatInstance {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&node(repeat)].kind else {
        panic!("Repeat")
    };
    RepeatInstance {
        node: node(repeat),
        iteration: iterations.at(ordinal).unwrap(),
    }
}
fn split_ids(prefix: &str) -> SplitIdentities {
    SplitIdentities {
        nodes: (0..32)
            .map(|index| node(&format!("{prefix}-{index}")))
            .collect(),
    }
}

#[test]
fn allowance_is_reversible_guarded_and_independent_of_recipe_parameters() {
    let original = fixture(false);
    let issuer = address("hold", vec![]);
    let (allowed, transaction) = edit(
        &original,
        Command::SetSoundAllowance {
            sound: sound(),
            issuer: issuer.clone(),
            allowed: true,
        },
    );
    assert_eq!(transaction.duration_delta, 0);
    assert!(transaction.changed_ids.contains(&node("root")));
    assert!(transaction.changed_ids.contains(&node("hold")));
    assert_eq!(allowed.sounds(), original.sounds());
    assert_eq!(allowed.sound_routes(), original.sound_routes());
    assert!(allowed.sound_allowances()[&sound()].contains(&issuer));
    let mut stale = transaction.forward.clone();
    stale.sound_allowances.get_mut(&sound()).unwrap().before =
        Some(SoundHoldAllowances::try_from(vec![issuer.clone()]).unwrap());
    assert_eq!(
        stale.apply(&original).unwrap_err().code,
        EditErrorCode::PatchConflict
    );
    let mut event = allowed.sounds()[&sound()].clone();
    event.gain_millidecibels = -3000;
    event.start_edge = AudioEdgePolicy::Hard;
    let parameters = edit(
        &allowed,
        Command::SetSound {
            id: sound(),
            event: event.clone(),
        },
    )
    .0;
    assert_eq!(parameters.sound_allowances(), allowed.sound_allowances());
    let replacement = edit(&parameters, Command::ReplaceSound { id: sound(), event }).0;
    assert_eq!(replacement.sound_allowances(), allowed.sound_allowances());
    let removed = edit(
        &allowed,
        Command::SetSoundAllowance {
            sound: sound(),
            issuer,
            allowed: false,
        },
    )
    .0;
    assert!(removed.sound_allowances().is_empty());
    assert!(!removed.to_json().unwrap().contains("sound_allowances"));
    let deleted = edit(&allowed, Command::DeleteSound { id: sound() }).0;
    assert!(deleted.sound_allowances().is_empty());
    let recreated = edit(
        &deleted,
        Command::SetSound {
            id: sound(),
            event: original.sounds()[&sound()].clone(),
        },
    )
    .0;
    assert!(recreated.sound_allowances().is_empty());
}

#[test]
fn split_and_internal_insertion_split_copy_only_existing_hold_issuers_once() {
    let original = allow(&fixture(false), address("hold", vec![]));
    let split = edit(
        &original,
        Command::Split {
            node: node("hold"),
            at: frames(40),
            identities: split_ids("split"),
        },
    )
    .0;
    assert_eq!(allowance_ids(&split).len(), 2);
    let command = Command::InsertTime {
        at: ProjectFrame(5),
        hold: hold(2),
        id: node("new-pause"),
        identities: split_ids("insert"),
        timing: AudioTimingId {
            allocation: request(&split, Command::DeleteSound { id: sound() }).new_revision,
            ordinal: 0,
        },
    };
    let inserted = edit(&split, command).0;
    assert_eq!(
        allowance_ids(&inserted).len(),
        3,
        "the helper Split copies one issuer once"
    );
    assert!(!inserted.sound_allowances()[&sound()].contains(&address("new-pause", vec![])));
    assert_eq!(inserted.sound_routes()[&sound()].edits.len(), 1);
    assert_eq!(inserted.sounds(), original.sounds());
    assert!(FrozenAudioContext::capture(&inserted).is_err());
    let NodeKind::Sequence { children } = &inserted.nodes()[&node("root")].kind else {
        panic!("root")
    };
    let removed = edit(
        &inserted,
        Command::Delete {
            node: children[0].clone(),
        },
    )
    .0;
    assert_eq!(allowance_ids(&removed).len(), 2);
    for issuer in removed.sound_allowances()[&sound()].iter() {
        issuer.validate(&removed).unwrap();
    }
}

#[test]
fn interior_source_splice_copies_hold_allowances_once_and_ripples_placed_sound_once() {
    let original = allow(&fixture(false), address("hold", vec![]));
    let insert = |document: &ProjectDocument, target: NodeId, name: &str| Command::SpliceSourceAt {
        parent: node("root"),
        target,
        at: frames(5),
        source: SourceNode {
            duration: frames(40),
            video: SourceVideo::Blank,
            audio: Some(original.sounds()[&sound()].source.clone()),
            audio_mapping: original.sounds()[&sound()].mapping,
            video_mapping: SourceVideoMapping::FitBeat,
            audio_offset: AudioSample(0),
            link: LinkRelation::Independent,
        },
        id: node(name),
        label: "Original moment".into(),
        identities: split_ids(name),
        timing: AudioTimingId {
            allocation: request(document, Command::DeleteSound { id: sound() }).new_revision,
            ordinal: 0,
        },
    };
    let first = edit(&original, insert(&original, node("hold"), "first")).0;
    assert_eq!(allowance_ids(&first).len(), 2);
    assert_eq!(first.sounds(), original.sounds());
    let journal = &first.sound_routes()[&sound()];
    assert_eq!(journal.edits.len(), 1);
    assert_eq!(
        journal.edits[0].operation,
        RootSoundOperation::Insert {
            at: ProjectFrame(5),
            duration: frames(40),
        }
    );
    let NodeKind::Sequence { children } = &first.nodes()[&node("root")].kind else {
        panic!("root")
    };
    let second = edit(&first, insert(&first, children[2].clone(), "second")).0;
    assert_eq!(allowance_ids(&second).len(), 3);
    assert_eq!(second.sounds(), original.sounds());
    let journal = &second.sound_routes()[&sound()];
    assert_eq!(journal.edits.len(), 2);
    assert_eq!(
        journal.edits[1].operation,
        RootSoundOperation::Insert {
            at: ProjectFrame(50),
            duration: frames(40),
        }
    );
    assert_eq!(journal.recipe_extent, frames(120));
    assert_eq!(
        journal.compile().unwrap().output_extent(),
        ExactRatio::integer(200)
    );
    for name in ["first", "second"] {
        assert!(!second.sound_allowances()[&sound()].contains(&address(name, vec![])));
    }
    for issuer in second.sound_allowances()[&sound()].iter() {
        issuer.validate(&second).unwrap();
    }
}

#[test]
fn splice_retains_policy_and_deleting_the_issuer_prunes_it_without_removing_surviving_sound() {
    let original = allow(&fixture(false), address("hold", vec![]));
    let source = SourceNode {
        duration: frames(40),
        video: SourceVideo::Blank,
        audio: Some(original.sounds()[&sound()].source.clone()),
        audio_mapping: original.sounds()[&sound()].mapping,
        video_mapping: SourceVideoMapping::FitBeat,
        audio_offset: AudioSample(0),
        link: LinkRelation::Independent,
    };
    let inserted = edit(
        &original,
        Command::SpliceSource {
            parent: node("root"),
            index: 0,
            source,
            id: node("source"),
            label: "Source".into(),
            timing: AudioTimingId {
                allocation: request(&original, Command::DeleteSound { id: sound() }).new_revision,
                ordinal: 0,
            },
        },
    )
    .0;
    assert_eq!(inserted.sound_allowances(), original.sound_allowances());
    let mut event = original.sounds()[&sound()].clone();
    event.offset = AudioSample(0);
    let replaced = edit(&inserted, Command::ReplaceSound { id: sound(), event }).0;
    let deleted = edit(&replaced, Command::Delete { node: node("hold") }).0;
    assert!(deleted.sounds().contains_key(&sound()));
    assert!(deleted.sound_allowances().is_empty());
}

#[test]
fn nested_occurrence_isolation_moves_selected_addresses_and_preserves_other_plays() {
    let original = fixture(true);
    let outer0 = play(&original, "outer", 0);
    let outer1 = play(&original, "outer", 1);
    let inner0 = play(&original, "inner", 0);
    let inner1 = play(&original, "inner", 1);
    let selected = address("hold", vec![outer0.clone(), inner0.clone()]);
    let sibling = address("hold", vec![outer0.clone(), inner1.clone()]);
    let other = address("hold", vec![outer1.clone(), inner0.clone()]);
    let gap = SoundHoldIssuer::RepeatGap {
        instance: InstancePath {
            node: node("inner"),
            repeats: vec![outer0.clone()],
        },
        gap_after: inner0.iteration.clone(),
    };
    let mut allowed = original.clone();
    for issuer in [&selected, &sibling, &other, &gap] {
        allowed = allow(&allowed, issuer.clone());
    }
    let isolated = edit(
        &allowed,
        Command::EditOccurrence {
            instance: selected.instance().clone(),
            edit: OccurrenceEdit::Rename {
                label: "Isolated".into(),
            },
            identities: OccurrenceIdentities {
                nodes: (0..16)
                    .map(|index| node(&format!("isolate-{index}")))
                    .collect(),
                marks: vec![],
            },
        },
    )
    .0;
    let values = &isolated.sound_allowances()[&sound()];
    assert_eq!(values.len(), 4);
    assert!(values.contains(&other));
    assert!(!values.contains(&selected));
    assert!(!values.contains(&sibling));
    assert!(!values.contains(&gap));
    let outer_copy = isolated.overrides()[&node("outer")]
        .get(&outer0.iteration)
        .unwrap();
    let inner_copy_hold = isolated.overrides()[outer_copy]
        .get(&inner0.iteration)
        .unwrap();
    let NodeKind::Repeat {
        child: shared_hold, ..
    } = &isolated.nodes()[outer_copy].kind
    else {
        panic!("inner copy")
    };
    let copied_step = RepeatInstance {
        node: outer_copy.clone(),
        iteration: inner0.iteration.clone(),
    };
    assert!(values.contains(&SoundHoldIssuer::Node {
        instance: InstancePath {
            node: inner_copy_hold.clone(),
            repeats: vec![outer0.clone(), copied_step]
        }
    }));
    assert!(values.contains(&SoundHoldIssuer::Node {
        instance: InstancePath {
            node: shared_hold.clone(),
            repeats: vec![
                outer0.clone(),
                RepeatInstance {
                    node: outer_copy.clone(),
                    iteration: inner1.iteration
                }
            ]
        }
    }));
    assert!(values.contains(&SoundHoldIssuer::RepeatGap {
        instance: InstancePath {
            node: outer_copy.clone(),
            repeats: vec![outer0]
        },
        gap_after: inner0.iteration
    }));
    assert_eq!(isolated.sounds(), original.sounds());
}

#[test]
fn splitting_a_repeat_remaps_its_gap_owner_and_all_enclosing_play_identities() {
    let original = fixture(true);
    let outer = play(&original, "outer", 0);
    let inner = play(&original, "inner", 0);
    let gap = SoundHoldIssuer::RepeatGap {
        instance: InstancePath {
            node: node("inner"),
            repeats: vec![outer.clone()],
        },
        gap_after: inner.iteration.clone(),
    };
    let original = allow(&allow(&original, gap), address("hold", vec![outer, inner]));
    let split = edit(
        &original,
        Command::Split {
            node: node("outer"),
            at: frames(20),
            identities: split_ids("copy"),
        },
    )
    .0;
    let values = &split.sound_allowances()[&sound()];
    assert_eq!(values.len(), 4);
    for issuer in values.iter() {
        issuer.validate(&split).unwrap();
    }
    assert_eq!(
        values
            .iter()
            .filter(|issuer| matches!(issuer, SoundHoldIssuer::RepeatGap { .. }))
            .count(),
        2
    );
    assert_eq!(split.sound_routes(), original.sound_routes());
}

#[test]
fn invalid_scope_and_gap_addresses_fail_without_changing_history() {
    let original = fixture(true);
    let outer = play(&original, "outer", 0);
    let inner = play(&original, "inner", 0);
    let wrong = RepeatInstance {
        node: node("inner"),
        iteration: IterationId {
            allocation: RevisionId::new("retired").unwrap(),
            ordinal: 0,
        },
    };
    for issuer in [
        address("hold", vec![]),
        address("hold", vec![outer.clone(), wrong]),
        address("root", vec![]),
        SoundHoldIssuer::RepeatGap {
            instance: InstancePath {
                node: node("inner"),
                repeats: vec![outer.clone()],
            },
            gap_after: play(&original, "inner", 2).iteration,
        },
        SoundHoldIssuer::RepeatGap {
            instance: InstancePath {
                node: node("hold"),
                repeats: vec![outer.clone(), inner],
            },
            gap_after: outer.iteration,
        },
    ] {
        assert!(
            apply(
                &original,
                &request(
                    &original,
                    Command::SetSoundAllowance {
                        sound: sound(),
                        issuer,
                        allowed: true
                    }
                )
            )
            .is_err()
        );
        assert!(original.sound_allowances().is_empty());
    }
    let issuer = address(
        "hold",
        vec![play(&original, "outer", 0), play(&original, "inner", 0)],
    );
    assert!(
        apply(
            &original,
            &request(
                &original,
                Command::SetSoundAllowance {
                    sound: SoundId::new("missing").unwrap(),
                    issuer,
                    allowed: true
                }
            )
        )
        .is_err()
    );
}

#[test]
fn wire_is_closed_canonical_and_bounded_before_document_traversal() {
    let issuer = address("hold", vec![]);
    assert!(SoundHoldAllowances::try_from(vec![issuer.clone(), issuer.clone()]).is_err());
    assert!(serde_json::from_value::<SoundHoldAllowances>(json!([issuer, issuer])).is_err());
    let too_many: Vec<_> = (0..=MAX_SOUND_ALLOWANCES_PER_EVENT)
        .map(|index| address(&format!("hold-{index}"), vec![]))
        .collect();
    assert_eq!(
        SoundHoldAllowances::try_from(too_many).unwrap_err().code,
        DocumentErrorCode::LimitExceeded
    );
    let repeated = RepeatInstance {
        node: node(&"n".repeat(128)),
        iteration: IterationId {
            allocation: RevisionId::new("a".repeat(128)).unwrap(),
            ordinal: 0,
        },
    };
    let too_large: Vec<SoundHoldIssuer> = (0..32)
        .map(|index| {
            address(
                &format!("hold-{index}"),
                vec![repeated.clone(); MAX_DOCUMENT_DEPTH],
            )
        })
        .collect();
    assert_eq!(
        SoundHoldAllowances::try_from(too_large).unwrap_err().code,
        DocumentErrorCode::LimitExceeded
    );
    assert!(
        SoundHoldAllowances::try_from(vec![address(
            "hold",
            vec![repeated; MAX_DOCUMENT_DEPTH + 1]
        )])
        .is_err()
    );
    let original = fixture(false);
    for (key, value) in [
        ("effect", json!([])),
        ("missing", json!([address("hold", vec![])])),
    ] {
        let mut wire = serde_json::to_value(&original).unwrap();
        wire["sound_allowances"] = json!({key: value});
        assert!(ProjectDocument::from_json(&wire.to_string()).is_err());
    }
    let mut wire = serde_json::to_value(address("hold", vec![])).unwrap();
    wire["definition"] = json!({"type":"node","node":"hold"});
    assert!(serde_json::from_value::<SoundHoldIssuer>(wire).is_err());
}

#[test]
fn document_aggregate_limit_is_shared_by_individually_valid_event_sets() {
    let original = fixture(false);
    let mut wire = serde_json::to_value(&original).unwrap();
    let mut children = vec![node("hold")];
    for index in 1..MAX_SOUND_ALLOWANCES_PER_EVENT {
        let id = node(&format!("hold-{index}"));
        wire["nodes"][id.as_str()] =
            serde_json::to_value(BeatNode::hold("Pause", hold(1))).unwrap();
        children.push(id);
    }
    wire["nodes"]["root"]["kind"]["children"] = json!(children);
    let issuers: Vec<_> = children
        .iter()
        .map(|id| address(id.as_str(), vec![]))
        .collect();
    let event = wire["sounds"]["effect"].clone();
    wire["sounds"] = json!({});
    wire["sound_allowances"] = json!({});
    for index in 0..8 {
        let id = format!("event-{index}");
        wire["sounds"][&id] = event.clone();
        wire["sound_allowances"][&id] = json!(issuers);
    }
    let full = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(
        full.sound_allowances()
            .values()
            .map(SoundHoldAllowances::len)
            .sum::<usize>(),
        MAX_DOCUMENT_SOUND_ALLOWANCES
    );
    wire["sounds"]["one-more"] = event;
    wire["sound_allowances"]["one-more"] = json!([address("hold", vec![])]);
    assert_eq!(
        ProjectDocument::from_json(&wire.to_string())
            .unwrap_err()
            .code,
        DocumentErrorCode::LimitExceeded
    );
}

#[test]
fn audio_setter_retires_only_its_hold_permissions_and_undo_restores_them() {
    let original = fixture(false);
    let mut wire = serde_json::to_value(&original).unwrap();
    wire["nodes"]["other-hold"] =
        serde_json::to_value(BeatNode::hold("Other pause", hold(10))).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = json!(["hold", "other-hold"]);
    let original = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let second = SoundId::new("second").unwrap();
    let original = edit(
        &original,
        Command::SetSound {
            id: second.clone(),
            event: original.sounds()[&sound()].clone(),
        },
    )
    .0;
    let target = address("hold", vec![]);
    let other = address("other-hold", vec![]);
    let allowed = allow(&allow(&original, target.clone()), other.clone());
    let allowed = edit(
        &allowed,
        Command::SetSoundAllowance {
            sound: second.clone(),
            issuer: target,
            allowed: true,
        },
    )
    .0;
    let unchanged = edit(
        &allowed,
        Command::SetHoldAudio {
            node: node("hold"),
            audio: HoldAudio::Silence,
        },
    )
    .0;
    assert_eq!(unchanged.sound_allowances(), allowed.sound_allowances());
    let (room, transaction) = edit(
        &allowed,
        Command::SetHoldAudio {
            node: node("hold"),
            audio: HoldAudio::RoomTone {
                source: allowed.sounds()[&sound()].source.clone(),
            },
        },
    );
    assert_eq!(transaction.duration_delta, 0);
    assert_eq!(room.sounds(), allowed.sounds());
    assert_eq!(room.sound_routes(), allowed.sound_routes());
    assert_eq!(room.sound_allowances().len(), 1);
    assert_eq!(room.sound_allowances()[&sound()].len(), 1);
    assert!(room.sound_allowances()[&sound()].contains(&other));
    assert!(!room.sound_allowances().contains_key(&second));
}

#[test]
fn occurrence_audio_setter_retires_only_the_isolated_hold_permission() {
    let original = fixture(true);
    let outer0 = play(&original, "outer", 0);
    let outer1 = play(&original, "outer", 1);
    let inner0 = play(&original, "inner", 0);
    let inner1 = play(&original, "inner", 1);
    let selected = address("hold", vec![outer0.clone(), inner0.clone()]);
    let sibling = address("hold", vec![outer0.clone(), inner1.clone()]);
    let other = address("hold", vec![outer1, inner0.clone()]);
    let gap = SoundHoldIssuer::RepeatGap {
        instance: InstancePath {
            node: node("inner"),
            repeats: vec![outer0.clone()],
        },
        gap_after: inner0.iteration.clone(),
    };
    let mut allowed = original.clone();
    for issuer in [&selected, &sibling, &other, &gap] {
        allowed = allow(&allowed, issuer.clone());
    }
    let room_audio = HoldAudio::RoomTone {
        source: allowed.sounds()[&sound()].source.clone(),
    };
    let (isolated, transaction) = edit(
        &allowed,
        Command::EditOccurrence {
            instance: selected.instance().clone(),
            edit: OccurrenceEdit::SetHoldAudio {
                audio: room_audio.clone(),
            },
            identities: OccurrenceIdentities {
                nodes: (0..16)
                    .map(|index| node(&format!("audio-isolate-{index}")))
                    .collect(),
                marks: vec![],
            },
        },
    );
    let copied_repeat = isolated.overrides()[&node("outer")]
        .get(&outer0.iteration)
        .unwrap();
    let copied_hold = isolated.overrides()[copied_repeat]
        .get(&inner0.iteration)
        .unwrap();
    let NodeKind::Hold { recipe } = &isolated.nodes()[copied_hold].kind else {
        panic!("isolated Hold")
    };
    assert_eq!(recipe.audio, room_audio);
    assert_eq!(
        isolated.nodes()[&node("hold")],
        allowed.nodes()[&node("hold")]
    );
    let values = &isolated.sound_allowances()[&sound()];
    assert_eq!(values.len(), 3);
    assert!(values.contains(&other));
    assert!(
        values
            .iter()
            .all(|issuer| &issuer.instance().node != copied_hold)
    );
    assert!(values.iter().any(|issuer| matches!(issuer,
        SoundHoldIssuer::RepeatGap { instance, gap_after }
            if &instance.node == copied_repeat && gap_after == &inner0.iteration
    )));
    assert!(values.iter().any(|issuer| matches!(issuer,
        SoundHoldIssuer::Node { instance }
            if instance.repeats.last().is_some_and(|step|
                &step.node == copied_repeat && step.iteration == inner1.iteration)
    )));
    for issuer in values.iter() {
        issuer.validate(&isolated).unwrap();
    }
    assert_eq!(transaction.duration_delta, 0);
    assert_eq!(isolated.duration().unwrap(), allowed.duration().unwrap());
    assert_eq!(isolated.sounds(), allowed.sounds());
    assert_eq!(isolated.sound_routes(), allowed.sound_routes());
}
