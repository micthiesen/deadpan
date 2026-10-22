use super::*;
use std::{num::NonZeroU32, sync::Arc};

fn selector(name: &str) -> SemanticSelector {
    SemanticSelector::Node { node: node(name) }
}

fn run(
    document: &ProjectDocument,
    entry: &SemanticContext,
    instructions: Vec<SemanticInstruction>,
    bank: &BTreeMap<RegisterName, Arc<RegisterValue>>,
) -> (
    Result<SemanticPlan, EditError>,
    Vec<SemanticAllocationRequest>,
) {
    let program = SemanticProgram::new(instructions).unwrap();
    assert_eq!(
        serde_json::from_str::<SemanticProgram>(&serde_json::to_string(&program).unwrap()).unwrap(),
        program
    );
    let mut allocations = Vec::new();
    let result = plan_semantic(
        document,
        entry,
        &program,
        SemanticRegisterBank {
            entries: bank,
            version: 17,
        },
        revision("explicit-positive-final"),
        |request| {
            allocations.push(request.clone());
            match request {
                SemanticAllocationRequest::Yank { step_index } => Ok(SemanticAllocation::Yank {
                    capture_revision: revision(&format!("explicit-positive-capture-{step_index}")),
                }),
                SemanticAllocationRequest::Cut {
                    step_index,
                    required_split_ids,
                } => {
                    assert_eq!(required_split_ids, 0);
                    Ok(SemanticAllocation::Cut {
                        new_revision: revision(&format!("explicit-positive-leaf-{step_index}")),
                        capture_revision: revision(&format!(
                            "explicit-positive-capture-{step_index}"
                        )),
                        split_identities: SplitIdentities { nodes: vec![] },
                    })
                }
                SemanticAllocationRequest::Repeat {
                    step_index,
                    required_split_ids,
                    needs_group,
                } => {
                    assert_eq!(required_split_ids, 0);
                    assert!(!needs_group);
                    Ok(SemanticAllocation::Repeat {
                        new_revision: revision(&format!("explicit-positive-leaf-{step_index}")),
                        identities: RepeatSelectionIdentities {
                            repeat: node(&format!("explicit-positive-repeat-{step_index}")),
                            group: None,
                            split: SplitIdentities { nodes: vec![] },
                        },
                    })
                }
                SemanticAllocationRequest::ParameterEdit { step_index } => {
                    Ok(SemanticAllocation::ParameterEdit {
                        new_revision: revision(&format!("explicit-positive-leaf-{step_index}")),
                    })
                }
                SemanticAllocationRequest::Sound { step_index } => Ok(SemanticAllocation::Sound {
                    new_revision: revision(&format!("explicit-positive-leaf-{step_index}")),
                    id: SoundId::new(format!("explicit-positive-sound-{step_index}")).unwrap(),
                }),
                SemanticAllocationRequest::PasteEdited {
                    step_index,
                    requirements,
                    required_split_ids,
                } => {
                    assert_eq!(required_split_ids, 0);
                    Ok(SemanticAllocation::PasteEdited {
                        new_revision: revision(&format!("explicit-positive-leaf-{step_index}")),
                        identities: SlicePasteIdentities {
                            authored: OccurrenceIdentities {
                                nodes: (0..requirements.nodes)
                                    .map(|index| {
                                        node(&format!("explicit-paste-{step_index}-{index}"))
                                    })
                                    .collect(),
                                marks: (0..requirements.marks)
                                    .map(|index| {
                                        MarkId::new(format!("explicit-paste-{step_index}-{index}"))
                                            .unwrap()
                                    })
                                    .collect(),
                            },
                            aliases: (0..requirements.aliases)
                                .map(|index| {
                                    node(&format!("explicit-paste-alias-{step_index}-{index}"))
                                })
                                .collect(),
                        },
                        split_identities: SplitIdentities { nodes: vec![] },
                    })
                }
                other => panic!("unexpected allocation: {other:?}"),
            }
        },
        |_, _| panic!("no Original register resolution"),
    );
    (result, allocations)
}

fn cut(name: &str) -> SemanticInstruction {
    SemanticInstruction::Cut {
        selector: selector(name),
        register: RegisterName::new('a').unwrap(),
    }
}

fn repeat(name: &str) -> SemanticInstruction {
    SemanticInstruction::Repeat {
        selector: selector(name),
        plays: NonZeroU32::new(2).unwrap(),
        escalation: None,
    }
}

fn assert_replay(before: &ProjectDocument, plan: &SemanticPlan, delta: i64) {
    let replay =
        replay_compound::<EditError>(before, plan.request.as_ref().unwrap(), |_| Ok(())).unwrap();
    assert_eq!(replay.document, plan.document);
    assert_eq!(replay.register_writes, plan.register_writes);
    assert_eq!(replay.edit.duration_delta, delta);
    assert_eq!(
        replay.edit.inverse.apply(&replay.document).unwrap(),
        *before
    );
}

#[test]
fn node_cut_and_repeat_use_actual_parent_and_preserve_navigation_scope() {
    let before = fixture();
    let entry = context("root", 23, "tail");
    let (result, allocations) = run(&before, &entry, vec![cut("source-b")], &BTreeMap::new());
    let plan = result.unwrap();
    assert_eq!(
        allocations,
        vec![SemanticAllocationRequest::Cut {
            step_index: 0,
            required_split_ids: 0
        }]
    );
    assert_eq!(plan.context.parent, node("root"));
    assert_eq!(plan.context.selected_child, Some(node("group")));
    assert_eq!(plan.context.cursor, ProjectFrame(13));
    assert_eq!(plan.context.visual_selection, None);
    assert_eq!(plan.document.duration().unwrap(), frames(16));
    assert!(!plan.document.nodes().contains_key(&node("source-b")));
    assert_eq!(
        captured(&plan),
        &capture(&before, "group", "source-b", "explicit-positive-capture-0")
    );
    assert_eq!(
        plan.trace[0].capture.as_ref().unwrap().scope,
        vec![node("group")]
    );
    assert_replay(&before, &plan, -8);

    // Direct-child deletion must choose the surviving next child, not retain
    // the removed identity as a navigation selection.
    let (result, _) = run(&before, &entry, vec![cut("lead")], &BTreeMap::new());
    let direct = result.unwrap();
    assert_eq!(direct.context.parent, node("root"));
    assert_eq!(direct.context.selected_child, Some(node("group")));
    assert_replay(&before, &direct, -3);

    let (result, allocations) = run(&before, &entry, vec![repeat("source-b")], &BTreeMap::new());
    let repeated = result.unwrap();
    assert_eq!(
        allocations,
        vec![SemanticAllocationRequest::Repeat {
            step_index: 0,
            required_split_ids: 0,
            needs_group: false
        }]
    );
    assert_eq!(repeated.document.duration().unwrap(), frames(32));
    assert_eq!(repeated.context.parent, node("root"));
    assert_eq!(repeated.context.selected_child, Some(node("group")));
    assert_eq!(repeated.trace[0].resolved_parent, Some(node("group")));
    assert!(
        matches!(&repeated.document.nodes()[&node("explicit-positive-repeat-0")].kind,
        NodeKind::Repeat { child, .. } if child == &node("source-b"))
    );
    assert_replay(&before, &repeated, 8);
}

#[test]
fn empty_nodes_retain_distinct_identity_and_refuse_positive_repeat_before_allocation() {
    let before = fixture();
    let entry = context("root", 23, "tail");
    for (name, at) in [("empty-a", 3), ("empty-b", 21)] {
        let selected = before.select_semantic_node(&entry, node(name)).unwrap();
        assert_eq!(selected.parent, node("root"));
        assert_eq!(selected.selected_child, Some(node("group")));
        assert_eq!(selected.cursor, ProjectFrame(at));
        assert_eq!(
            selected.visual_selection,
            Some(SemanticVisualSelection::Node { node: node(name) })
        );
        let copied = yank(
            &before,
            &entry,
            selector(name),
            &format!("explicit-empty-{name}"),
        );
        assert_eq!(captured(&copied).range(), range(at, at));
        assert_eq!(
            captured(&copied).selection(),
            &SliceCaptureSelection::Child { node: node(name) }
        );
        let (result, _) = run(&before, &entry, vec![cut(name)], &BTreeMap::new());
        let removed = result.unwrap();
        assert_eq!(removed.document.duration().unwrap(), frames(24));
        assert!(!removed.document.nodes().contains_key(&node(name)));
        assert_replay(&before, &removed, 0);
        let (result, allocations) = run(&before, &entry, vec![repeat(name)], &BTreeMap::new());
        assert!(result.is_err());
        assert!(allocations.is_empty());
    }
}

#[test]
fn finished_node_survives_motion_and_explicit_replacement_discards_stale_visual() {
    let before = fixture();
    let entry = context("root", 23, "tail");
    let (result, allocations) = run(
        &before,
        &entry,
        vec![
            SemanticInstruction::SelectNode {
                node: node("source-b"),
            },
            SemanticInstruction::MoveScope { end: true },
            SemanticInstruction::FinishSelection,
        ],
        &BTreeMap::new(),
    );
    let plan = result.unwrap();
    assert!(allocations.is_empty());
    assert!(plan.request.is_none());
    assert_eq!(plan.document, before);
    assert_eq!(plan.context.cursor, ProjectFrame(24));
    let visual = SemanticVisualSelection::Node {
        node: node("source-b"),
    };
    assert_eq!(plan.context.visual_selection, Some(visual.clone()));
    assert_eq!(visual.effective_selector(), selector("source-b"));
    let copied = yank(
        &before,
        &plan.context,
        visual.effective_selector(),
        "finished-node-copy",
    );
    assert_eq!(captured(&copied).range(), range(13, 21));

    let mut stale = entry.clone();
    stale.visual_selection = Some(SemanticVisualSelection::Node {
        node: node("missing"),
    });
    assert!(
        before
            .resolve_semantic_node(&stale, &node("source-b"))
            .is_err()
    );
    let replacement = before
        .select_semantic_node(&stale, node("source-b"))
        .unwrap();
    assert_eq!(replacement.cursor, ProjectFrame(21));
    assert_eq!(replacement.selected_child, Some(node("group")));
    assert_eq!(replacement.visual_selection, Some(visual));
}

#[test]
fn identity_reresolves_after_staged_duration_rename_and_move_and_never_falls_back() {
    let before = fixture();
    let entry = context("group", 3, "held");
    let (result, allocations) = run(
        &before,
        &entry,
        vec![
            SemanticInstruction::SetHoldDuration {
                length: PauseLength::Frames {
                    frames: NonZeroU32::new(4).unwrap(),
                },
            },
            SemanticInstruction::Yank {
                selector: selector("source-b"),
                register: RegisterName::new('a').unwrap(),
            },
        ],
        &BTreeMap::new(),
    );
    let plan = result.unwrap();
    assert_eq!(
        allocations,
        vec![
            SemanticAllocationRequest::ParameterEdit { step_index: 0 },
            SemanticAllocationRequest::Yank { step_index: 1 }
        ]
    );
    assert_eq!(captured(&plan).range(), range(15, 23));
    assert_eq!(plan.trace[1].resolved_range, Some(range(15, 23)));
    assert_replay(&before, &plan, 2);
    let renamed = edit(
        &before,
        "explicit-renamed",
        Command::Rename {
            node: node("source-b"),
            label: "Same label as any other beat".into(),
        },
    )
    .0;
    assert_eq!(
        renamed
            .resolve_semantic_node(&entry, &node("source-b"))
            .unwrap()
            .range,
        range(13, 21)
    );
    let moved = edit(
        &renamed,
        "explicit-moved-inside",
        Command::Move {
            node: node("source-b"),
            parent: node("group"),
            index: 0,
        },
    )
    .0;
    assert_eq!(
        moved
            .resolve_semantic_node(&entry, &node("source-b"))
            .unwrap()
            .range,
        range(3, 11)
    );
    let outside = edit(
        &moved,
        "explicit-moved-outside",
        Command::Move {
            node: node("source-b"),
            parent: node("root"),
            index: 3,
        },
    )
    .0;
    assert!(
        outside
            .resolve_semantic_node(&entry, &node("source-b"))
            .is_err()
    );
    let deleted = edit(
        &before,
        "explicit-deleted",
        Command::DeleteRipple {
            node: node("source-b"),
            timing: timing("explicit-deleted"),
        },
    )
    .0;
    let (result, allocations) = run(
        &deleted,
        &entry,
        vec![SemanticInstruction::Yank {
            selector: selector("source-b"),
            register: RegisterName::new('a').unwrap(),
        }],
        &BTreeMap::new(),
    );
    assert!(result.is_err());
    assert!(allocations.is_empty());
}

#[test]
fn scope_and_nonordinary_authority_refuse_without_allocations_or_fallback() {
    let before = fixture();
    for (entry, name) in [
        (context("root", 23, "tail"), "root"),
        (context("group", 3, "held"), "group"),
        (context("group", 3, "held"), "lead"),
        (context("group", 3, "held"), "tail"),
        (context("group", 3, "held"), "root"),
        (context("root", 23, "tail"), "foreign-or-retired"),
    ] {
        assert!(before.resolve_semantic_node(&entry, &node(name)).is_err());
        let (result, allocations) = run(&before, &entry, vec![cut(name)], &BTreeMap::new());
        assert!(result.is_err(), "{name}");
        assert!(allocations.is_empty());
    }
    let mut authority = context("root", 23, "tail");
    authority.original = Some(SemanticOriginalContext {
        asset: AssetId::new("explicit-node-original").unwrap(),
        qualification: SourceQualificationId::new("b".repeat(64)).unwrap(),
        ordinal: SourceFrameId(0),
    });
    assert!(
        before
            .resolve_semantic_node(&authority, &node("source-b"))
            .is_err()
    );
    assert!(
        before
            .select_semantic_node(&authority, node("source-b"))
            .is_err()
    );
}

#[test]
fn later_named_call_failure_does_not_publish_earlier_capture_or_cut() {
    let before = fixture();
    let entry = context("root", 23, "tail");
    let named = RegisterName::new('n').unwrap();
    let bank = BTreeMap::from([(
        named,
        Arc::new(RegisterValue::Macro {
            program: SemanticProgram::new(vec![SemanticInstruction::Yank {
                selector: selector("source-b"),
                register: RegisterName::new('a').unwrap(),
            }])
            .unwrap()
            .into(),
        }),
    )]);
    let frozen_bank = bank.clone();
    let frozen_document = before.to_json().unwrap();
    let (result, allocations) = run(
        &before,
        &entry,
        vec![
            cut("source-b"),
            SemanticInstruction::Call {
                register: named,
                count: NonZeroU32::new(1).unwrap(),
            },
        ],
        &bank,
    );
    assert!(result.is_err());
    assert_eq!(
        allocations,
        vec![SemanticAllocationRequest::Cut {
            step_index: 0,
            required_split_ids: 0
        }]
    );
    assert_eq!(before.to_json().unwrap(), frozen_document);
    assert_eq!(bank, frozen_bank);
}

#[test]
fn node_wire_is_strict_and_role_edits_do_not_convert_whole_identity_to_a_time_range() {
    let valid = json!({"type":"node", "node":"source-b"});
    assert_eq!(
        serde_json::from_value::<SemanticSelector>(valid).unwrap(),
        selector("source-b")
    );
    for invalid in [
        r#"{"type":"node"}"#,
        r#"{"type":"node","node":null}"#,
        r#"{"type":"node","node":""}"#,
        r#"{"type":"node","node":"source-b","parent":"group"}"#,
        r#"{"type":"node","node":"source-b","node":"tail"}"#,
    ] {
        assert!(
            serde_json::from_str::<SemanticSelector>(invalid).is_err(),
            "{invalid}"
        );
    }
    for invalid in [
        r#"{"type":"select_node"}"#,
        r#"{"type":"select_node","node":"source-b","range":{"start":13,"end":21}}"#,
    ] {
        assert!(
            serde_json::from_str::<SemanticInstruction>(invalid).is_err(),
            "{invalid}"
        );
    }
    // Synthetic, validated linked A/V makes lack of source audio an impossible
    // explanation for the identity refusal. The ordinary time-range controls
    // below execute both role consumers successfully on this same source.
    let silent = fixture();
    let asset_id = AssetId::new("explicit-node-original").unwrap();
    let mut asset = silent.assets()[&asset_id].clone();
    asset.audio = Some(span(0, 32));
    let mut beat = silent.nodes()[&node("source-b")].clone();
    let NodeKind::Source { source } = &mut beat.kind else {
        unreachable!()
    };
    source.audio = Some(SourceAudio {
        asset: asset_id.clone(),
        span: span(16, 24),
    });
    source.audio_mapping = SourceAudioMapping::Duration {
        frames: ExactRatio::integer(8),
    };
    source.link = LinkRelation::Linked;
    let mut wire = serde_json::to_value(&silent).unwrap();
    wire["assets"][asset_id.as_str()] = serde_json::to_value(asset).unwrap();
    wire["nodes"]["source-b"] = serde_json::to_value(beat).unwrap();
    let before = ProjectDocument::from_json(&serde_json::to_string(&wire).unwrap()).unwrap();
    let entry = before
        .select_semantic_node(&context("group", 13, "source-b"), node("source-b"))
        .unwrap();
    for instruction in [
        SemanticInstruction::DeleteRole {
            role: MediaRole::Audio,
        },
        SemanticInstruction::RoleRepeat {
            role: MediaRole::Audio,
            plays: NonZeroU32::new(2).unwrap(),
            trim: true,
            extend: false,
        },
    ] {
        let mut time = context("group", 13, "source-b");
        let (anchor, head) = if matches!(instruction, SemanticInstruction::RoleRepeat { .. }) {
            (15, 19)
        } else {
            (13, 21)
        };
        time.visual_selection = Some(SemanticVisualSelection::Time {
            anchor: ProjectFrame(anchor),
            head: ProjectFrame(head),
            extending: false,
        });
        let (control, controls) = run(&before, &time, vec![instruction.clone()], &BTreeMap::new());
        let control = control.unwrap();
        assert!(!controls.is_empty());
        assert_eq!(
            control.document.duration().unwrap(),
            before.duration().unwrap()
        );
        assert_replay(&before, &control, 0);
        let (result, allocations) = run(&before, &entry, vec![instruction], &BTreeMap::new());
        let error = result.unwrap_err();
        assert_eq!(error.code, EditErrorCode::InvalidCommand);
        assert_eq!(
            error.message,
            "a role edit acts on a Visual time range, not a group object"
        );
        assert!(allocations.is_empty());
    }
}

#[test]
fn finished_node_paste_replaces_nested_whole_and_empty_children_at_actual_parent() {
    let before = fixture();
    let copied = Arc::new(RegisterValue::Edited {
        slice: Arc::new(capture(&before, "root", "lead", "explicit-paste-input")),
    });
    let bank = BTreeMap::from([(RegisterName::new('a').unwrap(), copied)]);
    for (target, removed, delta) in [("source-b", range(13, 21), -5), ("empty-a", range(3, 3), 3)] {
        for before_key in [false, true] {
            let entry = before
                .select_semantic_node(&context("root", 23, "tail"), node(target))
                .unwrap();
            let (result, allocations) = run(
                &before,
                &entry,
                vec![SemanticInstruction::Paste {
                    register: RegisterName::new('a').unwrap(),
                    before: before_key,
                }],
                &bank,
            );
            let plan = result.unwrap();
            assert_eq!(allocations.len(), 1);
            assert!(matches!(
                allocations[0],
                SemanticAllocationRequest::PasteEdited {
                    required_split_ids: 0,
                    ..
                }
            ));
            assert!(!plan.document.nodes().contains_key(&node(target)));
            assert_eq!(plan.document.duration().unwrap().frames(), 24 + delta);
            assert_eq!(plan.context.parent, node("root"));
            assert_eq!(plan.context.selected_child, Some(node("group")));
            assert_eq!(plan.context.visual_selection, None);
            assert_eq!(plan.trace[0].resolved_parent, Some(node("group")));
            assert_eq!(plan.trace[0].removed_range, Some(removed));
            assert_eq!(
                plan.trace[0].resolved_selection,
                Some(SliceCaptureSelection::Child { node: node(target) })
            );
            assert_replay(&before, &plan, delta);
        }
    }
}

#[test]
fn whole_composites_are_addressable_but_repeat_and_retime_interiors_are_not() {
    let before = fixture();
    for command in [
        Command::WrapRepeat {
            node: node("group"),
            id: node("wrapper"),
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::default(),
        },
        Command::WrapRetime {
            node: node("group"),
            id: node("wrapper"),
            duration: frames(36),
            pitch: PitchPolicy::Preserve,
        },
    ] {
        let wrapped = edit(&before, "explicit-composite", command).0;
        let entry = context("root", 0, "lead");
        let whole = wrapped
            .resolve_semantic_node(&entry, &node("wrapper"))
            .unwrap();
        assert_eq!(whole.parent, node("root"));
        assert_eq!(whole.navigation_child, node("wrapper"));
        assert_eq!(whole.range, range(3, 39));
        let copied = yank(
            &wrapped,
            &entry,
            selector("wrapper"),
            "explicit-whole-composite",
        );
        assert_eq!(captured(&copied).range(), whole.range);
        assert!(
            wrapped
                .resolve_semantic_node(&entry, &node("group"))
                .is_err()
        );
        assert!(
            wrapped
                .resolve_semantic_node(&entry, &node("source-b"))
                .is_err()
        );
        let (result, allocations) = run(&wrapped, &entry, vec![cut("source-b")], &BTreeMap::new());
        assert!(result.is_err());
        assert!(allocations.is_empty());
    }
}
