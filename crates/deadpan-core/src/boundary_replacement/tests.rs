use super::*;
use crate::*;
use serde_json::json;
use std::sync::Arc;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn rate() -> FrameRate {
    FrameRate::new(30_000, 1001).unwrap()
}
fn hold(duration: i64) -> HoldRecipe {
    HoldRecipe {
        duration: frames(duration),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
        picture_context: None,
    }
}
fn span(end: i64) -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: end,
            time_base,
        },
    )
    .unwrap()
}
fn target(value: &str) -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: node(value),
        repeats: vec![],
    }
}
fn request(document: &ProjectDocument, next: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(next),
        command,
    }
}
fn accepted(document: &ProjectDocument, target: &ScopedNodeTarget) -> Box<AcceptedGeneration> {
    let NodeKind::Hold { recipe } = &document.nodes()[&target.node].kind else {
        panic!()
    };
    let HoldVideo::Generated { accepted } = &recipe.video else {
        panic!()
    };
    accepted.clone()
}
fn entry(document: &ProjectDocument, target: ScopedNodeTarget) -> BoundaryReplacement {
    BoundaryReplacement {
        accepted: accepted(document, &target),
        target,
    }
}
fn wrapped(base: &CommandRequest, replacements: Vec<BoundaryReplacement>) -> CommandRequest {
    CommandRequest {
        command: Command::WithBoundaryReplacements {
            edit: BoundaryReplacementEdit::new(base.command.clone(), replacements).unwrap(),
        },
        ..base.clone()
    }
}
fn set_fallback(document: &mut ProjectDocument, replacement: &BoundaryReplacement) {
    let NodeKind::Hold { recipe } = &mut document
        .nodes
        .get_mut(&replacement.target.node)
        .unwrap()
        .kind
    else {
        panic!()
    };
    recipe.video = fallback(&replacement.accepted.fallback);
}

fn fixture() -> ProjectDocument {
    let mut document = ProjectDocument::new(
        ProjectId::new("boundary").unwrap(),
        revision("before"),
        PresentationBasis {
            width: 640,
            height: 480,
            frame_rate: rate(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let object = |digit: char| {
        GeneratedObjectRef::new(
            GeneratedContentId::new(digit.to_string().repeat(64)).unwrap(),
            100,
        )
        .unwrap()
    };
    let artifact = GeneratedArtifact {
        sampled_asset: AssetId::new("sampled").unwrap(),
        sampled_object: object('a'),
        native_asset: AssetId::new("native").unwrap(),
        native_object: object('b'),
        provenance: object('c'),
        sampling: BridgeSamplingMap::new(
            rate(),
            FrameRate::new(24, 1).unwrap(),
            frames(5),
            frames(4),
            BridgeInterpolation::EncodedSrgbRgb8LinearHalfUp,
        )
        .unwrap()
        .into(),
        content_aspect: Some([640, 480]),
    };
    for (id, reference, count) in [
        (&artifact.sampled_asset, &artifact.sampled_object, 4),
        (&artifact.native_asset, &artifact.native_object, 5),
    ] {
        document.assets.insert(
            id.clone(),
            AssetRecord {
                label: "generated".into(),
                content_hash: reference.content().to_string(),
                video: Some(span(48_000)),
                audio: None,
                still_image: false,
                frame_count: Some(frames(count)),
                source_qualification: None,
            },
        );
    }
    let mut recipe = hold(4);
    recipe.video = HoldVideo::Generated {
        accepted: Box::new(AcceptedGeneration {
            artifact,
            fallback: HoldFallback::Background,
        }),
    };
    recipe.audio = HoldAudio::Tone {
        frequency_hz: 440,
        level: GainDb::new(-6000).unwrap(),
    };
    recipe.picture_context = Some(
        CapturedFraming::new(vec![CapturedCanvas {
            width: 640,
            height: 480,
            fit: CapturedFit::Fill,
            layers: vec![],
        }])
        .unwrap(),
    );
    document.nodes.extend([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("lead"), node("held"), node("tail")]),
        ),
        (node("lead"), BeatNode::hold("Lead", hold(4))),
        (node("held"), BeatNode::hold("Accepted", recipe)),
        (node("tail"), BeatNode::hold("Tail", hold(4))),
    ]);
    let audio = AssetId::new("sound").unwrap();
    document.assets.insert(
        audio.clone(),
        AssetRecord {
            label: "Sound".into(),
            content_hash: "d".repeat(64),
            video: None,
            audio: Some(span(1000)),
            still_image: false,
            frame_count: None,
            source_qualification: Some(SourceQualificationId::new("e".repeat(64)).unwrap()),
        },
    );
    document.sounds.insert(
        SoundId::new("sound-event").unwrap(),
        SoundEvent {
            owner: node("root"),
            label: "Independent sound".into(),
            source: SourceAudio {
                asset: audio,
                span: span(1000),
            },
            mapping: SourceAudioMapping::natural_rate(span(1000), rate()).unwrap(),
            offset: AudioSample(10_000),
            gain_millidecibels: 0,
            start_edge: AudioEdgePolicy::Hard,
            end_edge: AudioEdgePolicy::Hard,
            overflow: SoundOverflowPolicy::Reject,
        },
    );
    document.validate().unwrap();
    document
}

fn insert(next: &str) -> Command {
    Command::InsertTime {
        at: ProjectFrame(4),
        hold: hold(2),
        id: node("inserted"),
        identities: SplitIdentities::default(),
        timing: AudioTimingId {
            allocation: revision(next),
            ordinal: 0,
        },
    }
}
fn rename() -> Command {
    Command::Rename {
        node: node("lead"),
        label: "changed".into(),
    }
}

fn assert_net(
    before: &ProjectDocument,
    base: &CommandRequest,
    effective: &CommandRequest,
    replacements: &[BoundaryReplacement],
) -> ProjectDocument {
    let (base_edit, mut expected) = apply_with_result(before, base).unwrap();
    for replacement in replacements {
        set_fallback(&mut expected, replacement);
    }
    let (transaction, after) = apply_with_result(before, effective).unwrap();
    assert_eq!(
        after, expected,
        "only final Hold provider values may differ from the base"
    );
    assert_eq!(transaction.description, base_edit.description);
    assert_eq!(transaction.forward.apply(before).unwrap(), after);
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *before);
    assert_eq!(effective.command.base_command(), &base.command);
    let wire = serde_json::to_vec(effective).unwrap();
    assert_eq!(
        serde_json::from_slice::<CommandRequest>(&wire).unwrap(),
        *effective
    );
    let validated = ValidatedDocument::new(Arc::new(before.clone())).unwrap();
    assert_eq!(
        apply_validated(&validated, effective)
            .unwrap()
            .1
            .document()
            .as_ref(),
        &after
    );
    after
}

#[test]
fn ordinary_insertion_preserves_allocation_timing_and_sound_transforms_once() {
    let before = fixture();
    let base = request(&before, "insert", insert("insert"));
    let replacement = entry(&before, target("held"));
    let effective = wrapped(&base, vec![replacement.clone()]);
    let after = assert_net(&before, &base, &effective, &[replacement]);
    assert!(after.nodes().contains_key(&node("inserted")));
    assert_ne!(after.audio_bindings(), before.audio_bindings());
    // Ripple keeps the original sound recipe and appends one clock transform.
    assert_eq!(after.sounds(), before.sounds());
    assert!(before.sound_routes().is_empty());
    let route = &after.sound_routes()[&SoundId::new("sound-event").unwrap()];
    assert_eq!(route.edits.len(), 1);
    assert_eq!(
        route.edits[0].operation,
        RootSoundOperation::Insert {
            at: ProjectFrame(4),
            duration: frames(2),
        }
    );
    assert_eq!(after.revision_id(), &revision("insert"));
}

#[test]
fn compound_keeps_leaf_revisions_frozen_capture_and_register_writes() {
    let before = fixture();
    let first = request(&before, "leaf", insert("leaf"));
    let (_, staged) = apply_with_result(&before, &first).unwrap();
    let slice = CapturedEditSlice::capture(
        &staged,
        &node("root"),
        FrameRange::new(ProjectFrame(6), ProjectFrame(10)).unwrap(),
        AudioTimingId {
            allocation: revision("capture"),
            ordinal: 0,
        },
    )
    .unwrap();
    let captured = Arc::new(RegisterValue::Edited {
        slice: Arc::new(slice),
    });
    let transaction = ResolvedTransaction::new(
        7,
        BTreeMap::new(),
        vec![
            ResolvedStep::Edit {
                edit: LeafEdit::new(first.new_revision.clone(), first.command.clone()).unwrap(),
            },
            ResolvedStep::Yank {
                name: RegisterName::new('a').unwrap(),
                value: captured.clone(),
            },
        ],
    )
    .unwrap();
    let base = request(&before, "outer", Command::Compound { transaction });
    let original = replay_compound::<EditError>(&before, &base, |_| Ok(())).unwrap();
    assert_eq!(
        original.register_writes[&RegisterName::new('a').unwrap()],
        captured
    );
    let replacement = entry(&before, target("held"));
    let effective = wrapped(&base, vec![replacement.clone()]);
    let after = assert_net(&before, &base, &effective, &[replacement]);
    let Command::Compound { transaction } = effective.command.base_command() else {
        panic!()
    };
    assert_eq!(transaction.expected_bank_version(), 7);
    assert_eq!(
        transaction.steps()[0].edit().unwrap().new_revision,
        revision("leaf")
    );
    let ResolvedStep::Yank { value, .. } = &transaction.steps()[1] else {
        panic!()
    };
    assert_eq!(value, &captured);
    assert_eq!(after.revision_id(), &revision("outer"));
}

fn nested() -> ProjectDocument {
    let mut document = fixture();
    let repeat = |name: &str, child: &str| {
        let mut beat = BeatNode::sequence(name, vec![]);
        beat.kind = NodeKind::Repeat {
            child: node(child),
            iterations: IterationOrder::new(revision(name), 2).unwrap(),
            gap: None,
            escalation: None,
        };
        beat
    };
    document.nodes.extend([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("lead"), node("outer"), node("tail")]),
        ),
        (node("outer"), repeat("outer", "body")),
        (
            node("body"),
            BeatNode::sequence("body", vec![node("inner")]),
        ),
        (node("inner"), repeat("inner", "inside")),
        (
            node("inside"),
            BeatNode::sequence("inside", vec![node("held")]),
        ),
    ]);
    document.validate().unwrap();
    document
}
fn scoped(outer: bool, inner: bool) -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: node("held"),
        repeats: [("outer", outer), ("inner", inner)]
            .into_iter()
            .map(|(name, play)| RepeatEditStep {
                repeat: node(name),
                branch: if play {
                    RepeatEditBranch::Play {
                        iteration: IterationId {
                            allocation: revision(name),
                            ordinal: 1,
                        },
                    }
                } else {
                    RepeatEditBranch::Default
                },
            })
            .collect(),
    }
}
fn isolate(before: &ProjectDocument, selected: ScopedNodeTarget) -> CommandRequest {
    let edit = ScopedNodeEdit::Rename {
        label: "isolated".into(),
    };
    let requirements = before.scoped_edit_requirements(&selected, &edit).unwrap();
    request(
        before,
        "isolation",
        Command::EditScoped {
            target: selected,
            edit,
            identities: OccurrenceIdentities {
                nodes: (0..requirements.nodes)
                    .map(|i| node(&format!("fresh-{i}")))
                    .collect(),
                marks: (0..requirements.marks)
                    .map(|i| MarkId::new(format!("fresh-{i}")).unwrap())
                    .collect(),
            },
        },
    )
}

#[test]
fn exclusive_scopes_keep_direct_and_compound_isolation_proofs_and_inverse() {
    let before = nested();
    for compound in [false, true] {
        let selected = scoped(true, true);
        let isolated = isolate(&before, selected.clone());
        let base = if compound {
            request(
                &before,
                "outer-revision",
                Command::Compound {
                    transaction: ResolvedTransaction::new(
                        0,
                        BTreeMap::new(),
                        vec![ResolvedStep::Edit {
                            edit: LeafEdit::new(
                                isolated.new_revision.clone(),
                                isolated.command.clone(),
                            )
                            .unwrap(),
                        }],
                    )
                    .unwrap(),
                },
            )
        } else {
            isolated
        };
        let (_, staged) = apply_with_result(&before, &base).unwrap();
        let proof = derive_scoped_isolation(&before, &base, &staged).unwrap();
        let final_target = proof.map_forward(&selected).unwrap();
        let replacement = entry(&staged, final_target.clone());
        let effective = wrapped(&base, vec![replacement.clone()]);
        let after = assert_net(&before, &base, &effective, &[replacement]);
        let complete = derive_scoped_isolation(&before, &effective, &after).unwrap();
        assert_eq!(complete.record(), proof.record());
        assert_eq!(complete.map_forward(&selected).unwrap(), final_target);
        assert_eq!(complete.map_backward(&final_target).unwrap(), selected);
        assert_eq!(
            accepted(&after, &scoped(false, false)),
            accepted(&before, &scoped(false, false))
        );
    }
}

#[test]
fn literal_defaults_are_valid_but_shared_play_addresses_refuse() {
    let before = nested();
    let base = request(&before, "rename", rename());
    let replacement = entry(&before, scoped(false, false));
    assert_net(
        &before,
        &base,
        &wrapped(&base, vec![replacement.clone()]),
        &[replacement],
    );
    for selected in [scoped(true, false), scoped(false, true), scoped(true, true)] {
        selected.validate(&before).unwrap(); // Valid presentation does not prove ownership.
        let effective = wrapped(&base, vec![entry(&before, selected)]);
        assert_eq!(
            apply(&before, &effective).unwrap_err().code,
            EditErrorCode::InvalidCommand
        );
    }
    assert_eq!(before, nested());
}

#[test]
fn private_inner_branch_under_shared_outer_play_still_refuses() {
    let before = nested();
    let isolated = isolate(&before, scoped(false, true));
    let prepared = prepare_scoped_edit(&before, &isolated).unwrap();
    let mut shared_outer = prepared.target.clone();
    shared_outer.repeats[0].branch = scoped(true, false).repeats[0].branch.clone();
    shared_outer.validate(&prepared.document).unwrap();
    let base = request(&prepared.document, "rename", rename());
    let effective = wrapped(&base, vec![entry(&prepared.document, shared_outer)]);
    assert_eq!(
        apply(&prepared.document, &effective).unwrap_err().code,
        EditErrorCode::InvalidCommand
    );
    let canonical = entry(&prepared.document, prepared.target);
    assert_net(
        &prepared.document,
        &base,
        &wrapped(&base, vec![canonical.clone()]),
        &[canonical],
    );
}

#[test]
fn owned_gap_and_default_can_be_replaced_together() {
    let mut before = nested();
    before
        .nodes
        .insert(node("gap-held"), before.nodes[&node("held")].clone());
    before.gap_overrides.insert(
        node("inner"),
        PlayOverrides::try_from(vec![PlayOverride {
            iteration: IterationId {
                allocation: revision("inner"),
                ordinal: 1,
            },
            root: node("gap-held"),
        }])
        .unwrap(),
    );
    before.validate().unwrap();
    let mut gap = scoped(false, true);
    gap.node = node("gap-held");
    gap.validate(&before).unwrap();
    let replacements = vec![entry(&before, gap), entry(&before, scoped(false, false))];
    let base = request(&before, "rename", rename());
    let after = assert_net(
        &before,
        &base,
        &wrapped(&base, replacements.clone()),
        &replacements,
    );
    for name in ["gap-held", "held"] {
        let NodeKind::Hold { recipe } = &after.nodes()[&node(name)].kind else {
            panic!()
        };
        assert_eq!(recipe.video, HoldVideo::Background);
    }
}

#[test]
fn exact_fallback_marker_is_allowed_but_mismatched_provider_or_target_refuses() {
    let before = fixture();
    let replacement = entry(&before, target("held"));
    let base = request(&before, "rename", rename());
    let mut wrong_accepted = replacement.clone();
    wrong_accepted.accepted.artifact.content_aspect = Some([320, 240]);
    let mut wrong_fallback = replacement.clone();
    wrong_fallback.accepted.fallback = HoldFallback::Freeze {
        asset: AssetId::new("sampled").unwrap(),
        timestamp: span(48_000).start(),
    };
    let mut missing = replacement.clone();
    missing.target.node = node("absent");
    let mut non_hold = replacement.clone();
    non_hold.target.node = node("root");
    let mut wrong_ancestry = replacement.clone();
    wrong_ancestry.target.repeats = scoped(false, false).repeats;
    for entry in [
        wrong_accepted,
        wrong_fallback.clone(),
        missing,
        non_hold,
        wrong_ancestry,
    ] {
        assert_eq!(
            apply(&before, &wrapped(&base, vec![entry]))
                .unwrap_err()
                .code,
            EditErrorCode::InvalidCommand
        );
    }
    let mut fallback_document = before.clone();
    set_fallback(&mut fallback_document, &replacement);
    fallback_document.validate().unwrap();
    let fallback_base = request(&fallback_document, "rename", rename());
    assert_net(
        &fallback_document,
        &fallback_base,
        &wrapped(&fallback_base, vec![replacement.clone()]),
        std::slice::from_ref(&replacement),
    );
    assert!(
        apply(
            &fallback_document,
            &wrapped(&fallback_base, vec![wrong_fallback])
        )
        .is_err()
    );
    let mut other = fallback_document.clone();
    let NodeKind::Hold { recipe } = &mut other.nodes.get_mut(&node("held")).unwrap().kind else {
        panic!()
    };
    recipe.video = HoldVideo::Accepted {
        asset: replacement.accepted.artifact.sampled_asset.clone(),
        frames: FrameRange::new(ProjectFrame(0), ProjectFrame(4)).unwrap(),
    };
    other.validate().unwrap();
    assert!(
        apply(
            &other,
            &wrapped(&request(&other, "rename", rename()), vec![replacement])
        )
        .is_err()
    );
    assert_eq!(before, fixture());
}

#[test]
fn constructor_and_wire_refuse_empty_duplicate_noncanonical_and_nested_entries() {
    let before = fixture();
    let one = entry(&before, target("held"));
    assert!(BoundaryReplacementEdit::new(rename(), vec![]).is_err());
    assert!(BoundaryReplacementEdit::new(rename(), vec![one.clone(), one.clone()]).is_err());
    let mut alias = one.clone();
    alias.target.repeats = scoped(false, false).repeats;
    assert!(BoundaryReplacementEdit::new(rename(), vec![one.clone(), alias]).is_err());
    let mut earlier = one.clone();
    earlier.target.node = node("aaa");
    assert!(BoundaryReplacementEdit::new(rename(), vec![one.clone(), earlier]).is_err());
    let envelope = Command::WithBoundaryReplacements {
        edit: BoundaryReplacementEdit::new(rename(), vec![one.clone()]).unwrap(),
    };
    assert!(BoundaryReplacementEdit::new(envelope.clone(), vec![one.clone()]).is_err());
    assert!(AtomicCommand::new(envelope.clone()).is_err());
    assert!(LeafEdit::new(revision("leaf"), envelope.clone()).is_err());
    let nested = json!({"command":"with_boundary_replacements","edit":{
        "command": envelope, "replacements":[one.clone()]}});
    assert!(
        serde_json::from_value::<Command>(nested)
            .unwrap_err()
            .to_string()
            .contains("cannot be nested")
    );
    let hidden = json!({"command":"compound","transaction":{"expected_bank_version":0,"inputs":{},
        "steps":[{"type":"edit","edit":{"new_revision":"leaf","command":envelope}}]}});
    assert!(
        serde_json::from_value::<Command>(hidden)
            .unwrap_err()
            .to_string()
            .contains("forbidden in Compound")
    );
    for replacements in [json!([]), json!([one.clone(), one])] {
        assert!(
            serde_json::from_value::<Command>(json!({"command":"with_boundary_replacements",
            "edit":{"command":rename(),"replacements":replacements}}))
            .is_err()
        );
    }
    // The raw discriminator refuses a leaf wrapper before decoding its invalid edit.
    assert!(
        serde_json::from_value::<AtomicCommand>(
            json!({"command":"with_boundary_replacements","edit":null})
        )
        .unwrap_err()
        .to_string()
        .contains("forbidden in Compound")
    );
}

#[test]
fn complete_wrapper_budget_applies_to_native_typed_input() {
    let before = fixture();
    let replacement = entry(&before, target("held"));
    let empty = Command::Rename {
        node: node("lead"),
        label: String::new(),
    };
    let overhead = serde_json::to_vec(&empty).unwrap().len();
    let base = Command::Rename {
        node: node("lead"),
        label: "x".repeat(MAX_COMPOUND_WIRE_BYTES - overhead),
    };
    assert_eq!(
        crate::compound::wire::size(&base, MAX_COMPOUND_WIRE_BYTES).unwrap(),
        MAX_COMPOUND_WIRE_BYTES
    );
    assert_eq!(
        BoundaryReplacementEdit::new(base, vec![replacement.clone()])
            .unwrap_err()
            .code,
        EditErrorCode::LimitExceeded
    );
    let mut deep = replacement;
    deep.target.repeats = vec![scoped(false, false).repeats[0].clone(); MAX_DOCUMENT_DEPTH + 1];
    assert_eq!(
        BoundaryReplacementEdit::new(rename(), vec![deep])
            .unwrap_err()
            .code,
        EditErrorCode::LimitExceeded
    );
}

#[test]
fn typed_count_bound_precedes_entry_validation() {
    let one = entry(&fixture(), target("held"));
    assert_eq!(
        BoundaryReplacementEdit::new(rename(), vec![one; MAX_DOCUMENT_NODES + 1])
            .unwrap_err()
            .code,
        EditErrorCode::LimitExceeded
    );
}

#[test]
fn streaming_ingress_keeps_collection_depth_duplicate_and_value_limits() {
    let prefix = r#"{"command":"with_boundary_replacements","edit":{"replacements":["#;
    let excess = format!("{prefix}{}{{\"unread", "null,".repeat(MAX_DOCUMENT_NODES));
    assert!(
        serde_json::from_str::<Command>(&excess)
            .unwrap_err()
            .to_string()
            .contains("document node limit")
    );
    let duplicate =
        r#"{"command":"with_boundary_replacements","edit":{"command":null,"command":null}}"#;
    assert!(
        serde_json::from_str::<Command>(duplicate)
            .unwrap_err()
            .to_string()
            .contains("duplicate command")
    );
    let deep = format!(
        "{{\"command\":\"with_boundary_replacements\",\"edit\":{}0{}}}",
        "[".repeat(70),
        "]".repeat(70)
    );
    assert!(
        serde_json::from_str::<Command>(&deep)
            .unwrap_err()
            .to_string()
            .contains("depth limit")
    );
    let many = format!(
        "{{\"command\":\"with_boundary_replacements\",\"edit\":[{}false]}}",
        "false,".repeat(1_000_000)
    );
    assert!(
        serde_json::from_str::<Command>(&many)
            .unwrap_err()
            .to_string()
            .contains("value limit")
    );
    // Wrapping does not reset the Compound leaf count or delay refusal until
    // an attacker-controlled excess leaf has been buffered.
    let excess_steps = format!(
        r#"{{"command":"with_boundary_replacements","edit":{{"command":{{"command":"compound","transaction":{{"steps":[{}{{"unread"#,
        "null,".repeat(MAX_COMPOUND_STEPS)
    );
    assert!(
        serde_json::from_str::<Command>(&excess_steps)
            .unwrap_err()
            .to_string()
            .contains("1024 expanded steps")
    );
}
