//! Explicit range intent stays relative; finished geometry and captures stay exact.
#[path = "range_set/fixture.rs"]
mod fixture;
use deadpan_core::*;
use fixture::*;
use std::{collections::BTreeMap, num::NonZeroU32};

fn offsets(start: i64, end: i64) -> SemanticRangeOffsets {
    SemanticRangeOffsets {
        start: SemanticFrameOffset(start),
        end: SemanticFrameOffset(end),
    }
}
fn context(cursor: i64) -> SemanticContext {
    SemanticContext {
        original: None,
        scoped: None,
        hold_duration: None,
        shared_hold_duration: None,
        mixed_hold_duration: None,
        selected_hold_duration: None,
        parent: node("root"),
        cursor: ProjectFrame(cursor),
        selected_child: Some(node("source")),
        visual_selection: None,
    }
}
fn run(
    document: &ProjectDocument,
    context: &SemanticContext,
    instructions: Vec<SemanticInstruction>,
) -> Result<SemanticPlan, EditError> {
    plan_semantic(
        document,
        context,
        &SemanticProgram::new(instructions)?,
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 19,
        },
        revision("range-final"),
        |request| {
            Ok(match request {
                SemanticAllocationRequest::Yank { step_index } => SemanticAllocation::Yank {
                    capture_revision: revision(&format!("range-capture-{step_index}")),
                },
                SemanticAllocationRequest::DeleteRange {
                    step_index,
                    required_nodes,
                } => SemanticAllocation::DeleteRange {
                    new_revision: revision(&format!("range-delete-{step_index}")),
                    identities: SplitIdentities {
                        nodes: (0..required_nodes)
                            .map(|n| node(&format!("range-split-{step_index}-{n}")))
                            .collect(),
                    },
                },
                SemanticAllocationRequest::PasteEditedSet {
                    step_index,
                    requirements,
                } => {
                    let prefix = format!("range-paste-{step_index}");
                    SemanticAllocation::PasteEditedSet {
                        new_revision: revision(&prefix),
                        identities: EditSetPasteIdentities {
                            group: node(&format!("{prefix}-group")),
                            components: requirements
                                .components
                                .iter()
                                .enumerate()
                                .map(|(member, r)| EditSetComponentIdentities {
                                    nodes: (0..r.nodes)
                                        .map(|n| node(&format!("{prefix}-{member}-node-{n}")))
                                        .collect(),
                                    aliases: (0..r.aliases)
                                        .map(|n| node(&format!("{prefix}-{member}-alias-{n}")))
                                        .collect(),
                                })
                                .collect(),
                            marks: (0..requirements.marks)
                                .map(|n| MarkId::new(format!("{prefix}-mark-{n}")).unwrap())
                                .collect(),
                        },
                    }
                }
                other => panic!("unexpected allocation: {other:?}"),
            })
        },
        |_, _| panic!("no Original register"),
    )
}

#[test]
fn relative_pairs_normalize_once_and_finished_selection_survives_motion() {
    let document = fixture();
    let input = vec![
        offsets(8, 12),
        offsets(-8, -4),
        offsets(-6, -2),
        offsets(-2, 0),
        offsets(8, 12),
    ];
    let selected = document
        .select_semantic_ranges(&context(10), input.clone())
        .unwrap();
    assert_eq!(selected.cursor, ProjectFrame(22));
    assert_eq!(
        selected.visual_selection,
        Some(SemanticVisualSelection::Ranges {
            ranges: vec![range(2, 10), range(18, 22)]
        })
    );
    assert_eq!(
        selected
            .visual_selection
            .as_ref()
            .unwrap()
            .effective_selector(),
        SemanticSelector::VisualSelection
    );
    let plan = run(
        &document,
        &context(10),
        vec![
            SemanticInstruction::SelectRanges { ranges: input },
            SemanticInstruction::MoveFrames {
                forward: false,
                count: NonZeroU32::new(20).unwrap(),
            },
            SemanticInstruction::Yank {
                selector: SemanticSelector::VisualSelection,
                register: RegisterName::unnamed(),
            },
        ],
    )
    .unwrap();
    assert_eq!(plan.context.cursor, ProjectFrame(2));
    assert_eq!(
        plan.trace[2].resolved_ranges,
        vec![range(2, 10), range(18, 22)]
    );
    let RegisterValue::EditedSet { set } = plan.register_writes[&RegisterName::unnamed()].as_ref()
    else {
        panic!()
    };
    assert_eq!(set.duration(), frames(12));
    assert_eq!(plan.document, document);
}

#[test]
fn every_input_is_checked_before_merging_and_no_end_is_clamped() {
    let document = fixture();
    for input in [
        vec![],
        vec![offsets(0, 0)],
        vec![offsets(3, 2)],
        vec![offsets(-11, 2)],
        vec![offsets(-10, 14), offsets(14, 15)],
        vec![offsets(0, i64::MAX)],
        vec![offsets(0, 1); 65],
    ] {
        assert!(
            document
                .resolve_semantic_ranges(&context(10), &input)
                .is_err(),
            "{input:?}"
        );
    }
    assert_eq!(
        document
            .resolve_semantic_ranges(&context(24), &[offsets(-24, 0)])
            .unwrap()
            .ranges,
        vec![range(0, 24)]
    );
    for ranges in [
        vec![range(2, 6), range(6, 10)],
        vec![range(10, 14), range(2, 6)],
        vec![range(2, 6), range(2, 6)],
    ] {
        assert!(
            document
                .resolve_semantic_range_selection(&context(0), &ranges)
                .is_err()
        );
        assert!(
            CapturedEditSet::capture_ranges(
                &document,
                &node("root"),
                &ranges,
                timing("range-bad", 0)
            )
            .is_err()
        );
    }
}

#[test]
fn reverse_cuts_capture_entry_snapshot_and_one_compound_undo_restores_every_frame() {
    let document = fixture();
    let plan = run(
        &document,
        &context(0),
        vec![SemanticInstruction::Cut {
            selector: SemanticSelector::Ranges {
                ranges: vec![offsets(18, 22), offsets(2, 6), offsets(10, 14)],
            },
            register: RegisterName::unnamed(),
        }],
    )
    .unwrap();
    assert_eq!(plan.document.duration().unwrap(), frames(12));
    assert_eq!(plan.context.cursor, ProjectFrame(2));
    assert_eq!(
        plan.trace[0].resolved_ranges,
        vec![range(2, 6), range(10, 14), range(18, 22)]
    );
    assert!(plan.trace[0].resolved_range.is_none());
    let request = plan.request.unwrap();
    let transaction = apply(&document, &request).unwrap();
    let after = transaction.forward.apply(&document).unwrap();
    assert_eq!(after, plan.document);
    assert_eq!(transaction.inverse.apply(&after).unwrap(), document);
    let RegisterValue::EditedSet { set } = plan.register_writes[&RegisterName::unnamed()].as_ref()
    else {
        panic!()
    };
    set.validate_capture(&document).unwrap();
    let empty = destination(&document, "range-empty");
    let pasted = paste_set(&empty, set, "range-union");
    let flat = edit(
        &pasted,
        "range-ungroup",
        Command::Ungroup {
            node: node("range-union-group"),
        },
    );
    check_oracle(
        &flat,
        &document,
        &[range(2, 6), range(10, 14), range(18, 22)],
    );
}

#[test]
fn one_range_keeps_set_authority_and_unsupported_consumers_refuse() {
    let document = fixture();
    let selected = document
        .select_semantic_ranges(&context(0), vec![offsets(2, 6)])
        .unwrap();
    for instruction in [
        SemanticInstruction::Duplicate {
            selector: SemanticSelector::VisualSelection,
        },
        SemanticInstruction::Paste {
            register: RegisterName::unnamed(),
            before: false,
        },
    ] {
        assert!(run(&document, &selected, vec![instruction]).is_err());
    }
    let plan = run(
        &document,
        &selected,
        vec![SemanticInstruction::Yank {
            selector: SemanticSelector::VisualSelection,
            register: RegisterName::unnamed(),
        }],
    )
    .unwrap();
    assert!(matches!(
        plan.register_writes[&RegisterName::unnamed()].as_ref(),
        RegisterValue::EditedSet { .. }
    ));
}

#[test]
fn range_wire_is_strict_and_forged_captures_cannot_change_shared_recipes_or_scope() {
    for json in [
        r#"{"type":"ranges","ranges":[]}"#,
        r#"{"type":"ranges","ranges":[{"start":0,"end":1,"extra":2}]}"#,
        r#"{"type":"ranges","ranges":[{"start":0.0,"end":1}]}"#,
        r#"{"type":"ranges","ranges":[{"start":0,"end":0}]}"#,
    ] {
        assert!(
            serde_json::from_str::<SemanticSelector>(json).is_err(),
            "{json}"
        );
    }
    let document = fixture();
    let set = CapturedEditSet::capture_ranges(
        &document,
        &node("root"),
        &[range(2, 6), range(18, 22)],
        timing("forge-capture", 0),
    )
    .unwrap();
    for mode in 0..5 {
        let mut wire = serde_json::to_value(&set).unwrap();
        match mode {
            0 => wire["components"][1]["nodes"]["source"]["label"] = "forged".into(),
            1 => wire["components"][1]["parent"] = "wrong".into(),
            2 => wire["components"][1]["capture_timing"]["ordinal"] = 0.into(),
            3 => wire["components"].as_array_mut().unwrap().reverse(),
            4 => wire["components"][1]["revision_id"] = "wrong".into(),
            _ => unreachable!(),
        }
        assert!(
            CapturedEditSet::from_json(&wire.to_string()).is_err(),
            "{mode}"
        );
    }
    let mut altered = serde_json::to_value(&set).unwrap();
    for component in altered["components"].as_array_mut().unwrap() {
        component["nodes"]["source"]["label"] = "same forged recipe".into();
    }
    let altered = CapturedEditSet::from_json(&altered.to_string()).unwrap();
    assert!(altered.validate_capture(&document).is_err());
}

#[test]
fn nested_scope_uses_absolute_boundaries_and_requires_entry_for_partial_groups() {
    let document = fixture();
    let document = edit(
        &document,
        "range-nested-group",
        Command::Group {
            parent: node("root"),
            start: 0,
            end: 1,
            id: node("group"),
            label: "Ordinary group".into(),
        },
    );
    let document = edit(
        &document,
        "range-nested-prefix",
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("prefix"),
                nodes: BTreeMap::from([(node("prefix"), hold(3))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    );
    let document = edit(
        &document,
        "range-nested-tail",
        Command::Insert {
            parent: node("root"),
            index: 2,
            subtree: Subtree {
                root: node("tail"),
                nodes: BTreeMap::from([(node("tail"), hold(4))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    );
    let frozen = document.to_json().unwrap();
    let mut nested = context(13);
    nested.parent = node("group");
    let target = document
        .resolve_semantic_ranges(&nested, &[offsets(-8, -4), offsets(8, 12)])
        .unwrap();
    assert_eq!(target.ranges, vec![range(5, 9), range(21, 25)]);
    assert_eq!(target.total_duration, frames(8));
    let set = CapturedEditSet::capture_ranges(
        &document,
        &node("group"),
        &target.ranges,
        timing("range-nested-capture", 0),
    )
    .unwrap();
    set.validate_capture(&document).unwrap();
    assert_eq!(set.scope(), &node("group"));
    assert!(
        set.components()
            .iter()
            .all(|component| component.parent() == &node("group"))
    );
    assert_eq!(
        document
            .resolve_semantic_ranges(&nested, &[offsets(-10, 14)])
            .unwrap()
            .ranges,
        vec![range(3, 27)]
    );
    for outside in [offsets(-11, -9), offsets(13, 15)] {
        assert!(
            document
                .resolve_semantic_ranges(&nested, &[outside])
                .is_err()
        );
    }
    let mut outer = context(0);
    outer.selected_child = Some(node("group"));
    assert!(
        document
            .resolve_semantic_ranges(&outer, &[offsets(5, 9), offsets(21, 25)])
            .is_err()
    );
    assert_eq!(
        document
            .resolve_semantic_ranges(&outer, &[offsets(3, 27)])
            .unwrap()
            .ranges,
        vec![range(3, 27)]
    );
    assert_eq!(document.to_json().unwrap(), frozen);
}

#[test]
fn mixed_component_modes_and_stale_capture_sources_refuse() {
    let document = fixture();
    let set = CapturedEditSet::capture_ranges(
        &document,
        &node("root"),
        &[range(2, 6), range(18, 22)],
        timing("range-mode-capture", 0),
    )
    .unwrap();
    let child = CapturedEditSlice::capture_selection(
        &document,
        &node("root"),
        &SliceCaptureSelection::Child {
            node: node("source"),
        },
        timing("range-mode-capture", 1),
    )
    .unwrap();
    let mut wire = serde_json::to_value(&set).unwrap();
    wire["components"][1] = serde_json::to_value(child).unwrap();
    let error = CapturedEditSet::from_json(&wire.to_string()).unwrap_err();
    assert!(
        error.message.contains("all whole-child or all same-scope"),
        "{error:?}"
    );
    let later = edit(
        &document,
        "range-later-revision",
        Command::Rename {
            node: node("root"),
            label: "Same content, later revision".into(),
        },
    );
    assert_eq!(
        set.validate_capture(&later).unwrap_err().code,
        EditErrorCode::RevisionConflict
    );
    let foreign = ProjectDocument::new(
        ProjectId::new("another-project").unwrap(),
        document.revision_id().clone(),
        document.presentation_basis().clone(),
        node("root"),
    )
    .unwrap();
    assert_eq!(
        set.validate_capture(&foreign).unwrap_err().code,
        EditErrorCode::ProjectConflict
    );
    set.validate_capture(&document).unwrap();
}

#[test]
fn under_or_overallocated_second_cut_discards_private_prior_cut_and_capture() {
    let document = fixture();
    let frozen = document.to_json().unwrap();
    let bank = BTreeMap::from([(
        RegisterName::unnamed(),
        std::sync::Arc::new(RegisterValue::Edited {
            slice: std::sync::Arc::new(capture(&document, range(0, 1), 0)),
        }),
    )]);
    let retained = bank.clone();
    let program = SemanticProgram::new(vec![SemanticInstruction::Cut {
        selector: SemanticSelector::Ranges {
            ranges: vec![offsets(2, 6), offsets(10, 14), offsets(18, 22)],
        },
        register: RegisterName::unnamed(),
    }])
    .unwrap();
    for over in [false, true] {
        let mut cuts = 0usize;
        let result = plan_semantic(
            &document,
            &context(0),
            &program,
            SemanticRegisterBank {
                entries: &bank,
                version: 37,
            },
            revision("range-pool-final"),
            |request| {
                Ok(match request {
                    SemanticAllocationRequest::Yank { .. } => SemanticAllocation::Yank {
                        capture_revision: revision("range-pool-capture"),
                    },
                    SemanticAllocationRequest::DeleteRange {
                        step_index,
                        required_nodes,
                    } => {
                        cuts += 1;
                        assert!(required_nodes > 0);
                        let count = if cuts == 2 {
                            if over {
                                required_nodes + 1
                            } else {
                                required_nodes - 1
                            }
                        } else {
                            required_nodes
                        };
                        SemanticAllocation::DeleteRange {
                            new_revision: revision(&format!("range-pool-delete-{step_index}")),
                            identities: SplitIdentities {
                                nodes: (0..count)
                                    .map(|n| node(&format!("range-pool-{step_index}-{n}")))
                                    .collect(),
                            },
                        }
                    }
                    other => panic!("unexpected allocation {other:?}"),
                })
            },
            |_, _| panic!("no Original register"),
        );
        assert_eq!(
            cuts, 2,
            "failure happens after one successfully staged range deletion"
        );
        assert_eq!(result.unwrap_err().code, EditErrorCode::InvalidCommand);
        assert_eq!(document.to_json().unwrap(), frozen);
        assert_eq!(bank, retained);
    }
}

#[test]
fn paste_rejects_under_or_overallocated_component_pool_without_publishing_any_member() {
    let document = fixture();
    let set = CapturedEditSet::capture_ranges(
        &document,
        &node("root"),
        &[range(2, 6), range(18, 22)],
        timing("range-paste-pool-capture", 0),
    )
    .unwrap();
    let empty = destination(&document, "range-paste-pool-empty");
    let frozen = empty.to_json().unwrap();
    for over in [false, true] {
        let mut identities = set_pools(&set, "range-paste-pool");
        if over {
            identities.components[1]
                .nodes
                .push(node("range-paste-extra"));
        } else {
            assert!(identities.components[1].nodes.pop().is_some());
        }
        let request = CommandRequest {
            project_id: empty.project_id().clone(),
            expected_revision: empty.revision_id().clone(),
            new_revision: revision("range-paste-pool-leaf"),
            command: Command::SpliceEditSet {
                parent: node("root"),
                index: 0,
                set: set.clone(),
                identities,
                timing: timing("range-paste-pool-leaf", 0),
            },
        };
        assert_eq!(
            apply(&empty, &request).unwrap_err().code,
            EditErrorCode::InvalidCommand
        );
        assert_eq!(empty.to_json().unwrap(), frozen);
    }
    let pasted = paste_set(&empty, &set, "range-paste-pool-valid");
    assert_eq!(pasted.duration().unwrap(), frames(8));
    assert_eq!(empty.to_json().unwrap(), frozen);
}

#[test]
fn native_visual_copy_and_cut_dispatch_matches_explicit_visual_selectors() {
    let document = fixture();
    let prefix = vec![
        SemanticInstruction::SelectRanges {
            ranges: vec![offsets(2, 6), offsets(-8, -4)],
        },
        SemanticInstruction::MoveFrames {
            forward: true,
            count: NonZeroU32::new(1).unwrap(),
        },
    ];
    let mut native = prefix.clone();
    native.extend([
        SemanticInstruction::YankSelection {
            register: RegisterName::new('a').unwrap(),
        },
        SemanticInstruction::CutSelection {
            register: RegisterName::new('b').unwrap(),
        },
    ]);
    let mut explicit = prefix;
    explicit.extend([
        SemanticInstruction::Yank {
            selector: SemanticSelector::VisualSelection,
            register: RegisterName::new('a').unwrap(),
        },
        SemanticInstruction::Cut {
            selector: SemanticSelector::VisualSelection,
            register: RegisterName::new('b').unwrap(),
        },
    ]);
    let native = run(&document, &context(10), native).unwrap();
    let explicit = run(&document, &context(10), explicit).unwrap();
    assert_eq!(native.document, explicit.document);
    assert_eq!(native.register_writes, explicit.register_writes);
    assert_eq!(native.context, explicit.context);
    assert_eq!(native.document.duration().unwrap(), frames(16));
    assert!(native.context.visual_selection.is_none());
    for letter in ['a', 'b'] {
        let RegisterValue::EditedSet { set } =
            native.register_writes[&RegisterName::new(letter).unwrap()].as_ref()
        else {
            panic!("native Visual must capture a union")
        };
        assert_eq!(set.duration(), frames(8));
        assert_eq!(
            set.components()
                .iter()
                .map(CapturedEditSlice::range)
                .collect::<Vec<_>>(),
            [range(2, 6), range(12, 16)]
        );
        set.validate_capture(&document).unwrap();
    }
}
