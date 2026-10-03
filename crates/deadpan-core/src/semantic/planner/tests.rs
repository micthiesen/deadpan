use super::*;
use crate::{
    BeatNode, FrameCut, FrameDuration, HoldAudio, HoldRecipe, HoldVideo,
    MAX_SEMANTIC_PROGRAM_BYTES, MAX_SEMANTIC_PROGRAM_INSTRUCTIONS, ProjectId,
};
use std::num::NonZeroU32;

mod content;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn name(value: char) -> RegisterName {
    RegisterName::new(value).unwrap()
}
fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}
fn hold(frames: i64) -> BeatNode {
    BeatNode::hold(
        "Held",
        HoldRecipe {
            duration: FrameDuration::new(frames).unwrap(),
            picture_context: None,
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}
fn tree(children: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let mut document = ProjectDocument::new(
        ProjectId::new("macro").unwrap(),
        revision("base"),
        crate::basis::default_basis(),
        node("root"),
    )
    .unwrap();
    document.nodes.insert(
        node("root"),
        BeatNode::sequence("Root", children.iter().map(|name| node(name)).collect()),
    );
    document
        .nodes
        .extend(nodes.into_iter().map(|(name, value)| (node(name), value)));
    document.validate().unwrap();
    document
}
fn fixture(frames: i64) -> ProjectDocument {
    tree(&["held"], vec![("held", hold(frames))])
}
fn context(parent: &str, cursor: i64) -> SemanticContext {
    SemanticContext {
        parent: node(parent),
        cursor: ProjectFrame(cursor),
        selected_child: None,
    }
}
fn cut(count: u32, register: char) -> SemanticInstruction {
    SemanticInstruction::CutFrames {
        operation: FrameCut::new(count).unwrap(),
        register: name(register),
    }
}
fn motion(forward: bool, count: u32) -> SemanticInstruction {
    SemanticInstruction::MoveFrames {
        forward,
        count: NonZeroU32::new(count).unwrap(),
    }
}
fn call(register: char, count: u32) -> SemanticInstruction {
    SemanticInstruction::Call {
        register: name(register),
        count: NonZeroU32::new(count).unwrap(),
    }
}
fn program(instructions: Vec<SemanticInstruction>) -> SemanticProgram {
    SemanticProgram::new(instructions).unwrap()
}
fn macro_value(instructions: Vec<SemanticInstruction>) -> Arc<RegisterValue> {
    Arc::new(RegisterValue::Macro {
        program: Arc::new(program(instructions)),
    })
}
fn allocate(request: SemanticAllocationRequest) -> Result<SemanticAllocation, EditError> {
    Ok(match request {
        SemanticAllocationRequest::Cut {
            step_index,
            required_split_ids,
        } => SemanticAllocation::Cut {
            new_revision: revision(&format!("leaf-{step_index}")),
            capture_revision: revision(&format!("capture-{step_index}")),
            split_identities: SplitIdentities {
                nodes: (0..required_split_ids)
                    .map(|n| node(&format!("split-{step_index}-{n}")))
                    .collect(),
            },
        },
        SemanticAllocationRequest::Yank { step_index } => SemanticAllocation::Yank {
            capture_revision: revision(&format!("capture-{step_index}")),
        },
        SemanticAllocationRequest::PasteEdited {
            step_index,
            requirements,
        } => SemanticAllocation::PasteEdited {
            new_revision: revision(&format!("leaf-{step_index}")),
            identities: SlicePasteIdentities {
                authored: crate::OccurrenceIdentities {
                    nodes: (0..requirements.nodes)
                        .map(|n| node(&format!("paste-{step_index}-{n}")))
                        .collect(),
                    marks: (0..requirements.marks)
                        .map(|n| MarkId::new(format!("mark-{step_index}-{n}")).unwrap())
                        .collect(),
                },
                aliases: (0..requirements.aliases)
                    .map(|n| node(&format!("alias-{step_index}-{n}")))
                    .collect(),
            },
        },
        SemanticAllocationRequest::PasteOriginal { step_index } => {
            SemanticAllocation::PasteOriginal {
                new_revision: revision(&format!("leaf-{step_index}")),
                node: node(&format!("original-{step_index}")),
            }
        }
    })
}
fn no_original(_: &ProjectDocument, _: &RegisterValue) -> Result<SourceNode, EditError> {
    panic!("fixture must not resolve Original media")
}
fn plan(
    document: &ProjectDocument,
    context: SemanticContext,
    instructions: Vec<SemanticInstruction>,
    bank: &BTreeMap<RegisterName, Arc<RegisterValue>>,
) -> Result<SemanticPlan, EditError> {
    plan_semantic(
        document,
        &context,
        &program(instructions),
        SemanticRegisterBank {
            entries: bank,
            version: 7,
        },
        revision("outer"),
        allocate,
        no_original,
    )
}

#[test]
fn sequential_cuts_capture_each_staged_document_and_form_one_reversible_compound() {
    let document = fixture(12);
    let original = document.clone();
    let planned = plan(
        &document,
        context("root", 1),
        vec![cut(3, 'a'), cut(2, 'b')],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(document, original);
    assert_eq!(planned.document.duration().unwrap().frames(), 7);
    assert_eq!(planned.context.cursor, ProjectFrame(1));
    assert_eq!(planned.trace[0].resolved_range, Some(range(1, 4)));
    assert_eq!(planned.trace[1].resolved_range, Some(range(1, 3)));
    assert_eq!(planned.trace[0].before_revision, revision("base"));
    assert_eq!(planned.trace[1].before_revision, revision("leaf-0"));
    assert_eq!(planned.trace[0].before_scope, range(0, 12));
    assert_eq!(planned.trace[1].before_scope, range(0, 9));
    let request = planned.request.as_ref().unwrap();
    let Command::Compound { transaction } = &request.command else {
        panic!()
    };
    assert_eq!(transaction.expected_bank_version(), 7);
    assert_eq!(transaction.steps().len(), 2);
    let mut staged = document.clone();
    for step in transaction.steps() {
        let ResolvedStep::Cut { slice, delete, .. } = step else {
            panic!()
        };
        slice.validate_capture(&staged).unwrap();
        assert_eq!(slice.revision_id(), staged.revision_id());
        staged = crate::apply(&staged, &delete.request(&staged))
            .unwrap()
            .forward
            .apply(&staged)
            .unwrap();
    }
    assert_eq!(
        planned.register_writes[&name('a')].capture_revision(),
        Some(document.revision_id())
    );
    assert_eq!(
        planned.register_writes[&name('b')].capture_revision(),
        Some(&revision("leaf-0"))
    );
    assert_eq!(
        planned.register_writes[&name('b')],
        planned.register_writes[&RegisterName::unnamed()]
    );
    let replayed = crate::replay_compound::<EditError>(&document, request, |_| Ok(())).unwrap();
    assert_eq!(replayed.document, planned.document);
    assert_eq!(replayed.register_writes, planned.register_writes);
    assert_eq!(
        replayed.edit.inverse.apply(&replayed.document).unwrap(),
        document
    );
}

#[test]
fn motions_use_shortened_scope_and_cuts_retain_requested_counts_after_clamping() {
    let document = fixture(10);
    let planned = plan(
        &document,
        context("root", 2),
        vec![cut(4, 'a'), motion(true, 50), motion(false, 2), cut(7, 'b')],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(planned.trace[1].after.cursor, ProjectFrame(6));
    assert_eq!(planned.trace[2].after.cursor, ProjectFrame(4));
    assert_eq!(planned.trace[3].resolved_range, Some(range(4, 6)));
    assert_eq!(planned.trace[3].instruction, cut(7, 'b'));
    assert_eq!(planned.document.duration().unwrap().frames(), 4);
    assert_eq!(planned.context.cursor, ProjectFrame(4));
    let NodeKind::Sequence { children } = &planned.document.nodes()[&node("root")].kind else {
        panic!()
    };
    assert_eq!(planned.selected_child.as_ref(), children.last());
}

#[test]
fn nested_scope_clamps_absolute_motion_and_preserves_outside_siblings() {
    let document = tree(
        &["prefix", "group", "tail"],
        vec![
            ("prefix", hold(3)),
            ("group", BeatNode::sequence("Group", vec![node("inner")])),
            ("inner", hold(8)),
            ("tail", hold(5)),
        ],
    );
    let planned = plan(
        &document,
        context("group", 4),
        vec![motion(false, 100), cut(3, 'a'), motion(true, 100)],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(planned.trace[0].after.cursor, ProjectFrame(3));
    assert_eq!(planned.trace[1].resolved_range, Some(range(3, 6)));
    assert_eq!(planned.trace[1].before_scope, range(3, 11));
    assert_eq!(planned.trace[2].before_scope, range(3, 8));
    assert_eq!(planned.context.parent, node("group"));
    assert_eq!(planned.context.cursor, ProjectFrame(8));
    assert_eq!(
        planned.document.nodes()[&node("prefix")],
        document.nodes()[&node("prefix")]
    );
    assert_eq!(
        planned.document.nodes()[&node("tail")],
        document.nodes()[&node("tail")]
    );
    assert_eq!(planned.document.duration().unwrap().frames(), 13);
}

#[test]
fn late_failure_cannot_mutate_inputs_or_return_partial_register_writes() {
    let document = fixture(8);
    let bank = BTreeMap::from([(name('a'), macro_value(vec![motion(true, 1)]))]);
    let original = document.clone();
    let original_bank = bank.clone();
    let error = plan(
        &document,
        context("root", 0),
        vec![cut(2, 'b'), call('b', 1)],
        &bank,
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::InvalidCommand);
    assert!(error.message.contains("copied content"));
    assert_eq!(document, original);
    assert_eq!(bank, original_bank);
    assert!(
        plan(
            &document,
            context("root", 0),
            vec![cut(8, 'b'), cut(1, 'b')],
            &bank
        )
        .is_err()
    );
}

#[test]
fn counted_call_freezes_its_body_while_later_calls_read_staged_register_writes() {
    let document = fixture(9);
    let bank = BTreeMap::from([(name('a'), macro_value(vec![cut(2, 'a')]))]);
    let planned = plan(&document, context("root", 0), vec![call('a', 3)], &bank).unwrap();
    assert_eq!(planned.document.duration().unwrap().frames(), 3);
    assert_eq!(planned.trace.len(), 4);
    assert_eq!(planned.trace[0].depth, 0);
    assert!(planned.trace[1..].iter().all(|item| item.depth == 1));
    let Command::Compound { transaction } = &planned.request.as_ref().unwrap().command else {
        panic!()
    };
    assert_eq!(
        transaction.inputs()[&name('a')].as_ref(),
        Some(&bank[&name('a')])
    );
    assert!(matches!(
        planned.register_writes[&name('a')].as_ref(),
        RegisterValue::Edited { .. }
    ));
    assert!(matches!(
        bank[&name('a')].as_ref(),
        RegisterValue::Macro { .. }
    ));
    let error = plan(
        &document,
        context("root", 0),
        vec![call('a', 2), call('a', 1)],
        &bank,
    )
    .unwrap_err();
    assert!(error.message.contains("copied content"));
}

#[test]
fn nested_calls_resolve_in_execution_order_and_freeze_each_selected_input() {
    let document = fixture(12);
    let bank = BTreeMap::from([
        (name('a'), macro_value(vec![motion(true, 1), call('b', 2)])),
        (name('b'), macro_value(vec![cut(1, 'c')])),
    ]);
    let planned = plan(&document, context("root", 0), vec![call('a', 2)], &bank).unwrap();
    let cuts: Vec<_> = planned
        .trace
        .iter()
        .filter_map(|item| item.resolved_range)
        .collect();
    assert_eq!(
        cuts,
        vec![range(1, 2), range(1, 2), range(2, 3), range(2, 3)]
    );
    assert_eq!(planned.document.duration().unwrap().frames(), 8);
    assert_eq!(planned.context.cursor, ProjectFrame(2));
    let replayed =
        crate::replay_compound::<EditError>(&document, planned.request.as_ref().unwrap(), |_| {
            Ok(())
        })
        .unwrap();
    assert_eq!(replayed.document, planned.document);
}

#[test]
fn recursion_missing_content_and_depth_limits_refuse() {
    let document = fixture(4);
    let empty = BTreeMap::new();
    assert!(
        plan(&document, context("root", 0), vec![call('a', 1)], &empty)
            .unwrap_err()
            .message
            .contains("empty")
    );
    for bank in [
        BTreeMap::from([(name('a'), macro_value(vec![call('a', 1)]))]),
        BTreeMap::from([
            (name('a'), macro_value(vec![call('b', 1)])),
            (name('b'), macro_value(vec![call('a', 1)])),
        ]),
    ] {
        assert!(
            plan(&document, context("root", 0), vec![call('a', 1)], &bank)
                .unwrap_err()
                .message
                .contains("recursive")
        );
    }
    let chain = |length: u8| -> BTreeMap<_, _> {
        (0..length)
            .map(|index| {
                let register = char::from(b'a' + index);
                (
                    name(register),
                    macro_value(vec![if index + 1 == length {
                        motion(true, 1)
                    } else {
                        call(char::from(b'a' + index + 1), 1)
                    }]),
                )
            })
            .collect()
    };
    assert!(
        plan(
            &document,
            context("root", 0),
            vec![call('a', 1)],
            &chain(16)
        )
        .is_ok()
    );
    let error = plan(
        &document,
        context("root", 0),
        vec![call('a', 1)],
        &chain(17),
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::LimitExceeded);
    assert!(error.message.contains("depth"));
}

#[test]
fn fuel_charges_calls_and_clamped_motions_and_rejects_huge_counts_without_allocating() {
    let document = fixture(1);
    let bank = BTreeMap::from([(name('a'), macro_value(vec![motion(false, 1)]))]);
    let mut allocations = 0;
    for count in [4096, u32::MAX] {
        let error = plan_semantic(
            &document,
            &context("root", 0),
            &program(vec![call('a', count)]),
            SemanticRegisterBank {
                entries: &bank,
                version: 0,
            },
            revision("outer"),
            |request| {
                allocations += 1;
                allocate(request)
            },
            no_original,
        )
        .unwrap_err();
        assert_eq!(error.code, EditErrorCode::LimitExceeded);
    }
    assert_eq!(allocations, 0);
    let planned = plan(&document, context("root", 0), vec![call('a', 4095)], &bank).unwrap();
    assert_eq!(planned.trace.len(), MAX_SEMANTIC_INSTRUCTION_FUEL);
    assert!(planned.request.is_none());
    assert_eq!(planned.document, document);
    assert_eq!(planned.context.cursor, ProjectFrame(0));
    let error = plan(
        &document,
        context("root", 0),
        vec![call('a', 4095), motion(false, 1)],
        &bank,
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::LimitExceeded);
}

#[test]
fn motion_only_handles_empty_scope_and_right_bias_without_allocations() {
    for document in [
        tree(&[], vec![]),
        tree(&["a", "b"], vec![("a", hold(2)), ("b", hold(3))]),
    ] {
        let planned = plan_semantic(
            &document,
            &context("root", 0),
            &program(vec![motion(true, 2)]),
            SemanticRegisterBank {
                entries: &BTreeMap::new(),
                version: 0,
            },
            revision("unused"),
            |_| panic!("motion allocated"),
            no_original,
        )
        .unwrap();
        assert!(planned.request.is_none());
        assert_eq!(planned.document, document);
        assert!(planned.register_writes.is_empty());
        assert_eq!(
            planned.selected_child,
            document.nodes().contains_key(&node("b")).then(|| node("b"))
        );
    }
    let empty = tree(&[], vec![]);
    assert_eq!(
        plan(
            &empty,
            context("root", 0),
            vec![cut(1, 'a')],
            &BTreeMap::new()
        )
        .unwrap_err()
        .code,
        EditErrorCode::SelectionUnavailable
    );
}

#[test]
fn fresh_identity_contract_rejects_revision_capture_and_split_reuse() {
    let document = fixture(10);
    for duplicate in ["base", "outer", "leaf-0", "capture-0"] {
        let error = plan_semantic(
            &document,
            &context("root", 1),
            &program(vec![cut(1, 'a'), cut(1, 'a')]),
            SemanticRegisterBank {
                entries: &BTreeMap::new(),
                version: 0,
            },
            revision("outer"),
            |request| {
                let mut allocation = allocate(request)?;
                if matches!(
                    request,
                    SemanticAllocationRequest::Cut { step_index: 1, .. }
                ) && let SemanticAllocation::Cut {
                    capture_revision, ..
                } = &mut allocation
                {
                    *capture_revision = revision(duplicate);
                }
                Ok(allocation)
            },
            no_original,
        )
        .unwrap_err();
        assert_eq!(error.code, EditErrorCode::IdentityConflict, "{duplicate}");
    }
    for duplicate in ["held", "split-0-0"] {
        let error = plan_semantic(
            &document,
            &context("root", 1),
            &program(vec![cut(1, 'a'), cut(1, 'a')]),
            SemanticRegisterBank {
                entries: &BTreeMap::new(),
                version: 0,
            },
            revision("outer"),
            |request| {
                let mut allocation = allocate(request)?;
                if matches!(
                    request,
                    SemanticAllocationRequest::Cut { step_index: 1, .. }
                ) && let SemanticAllocation::Cut {
                    split_identities, ..
                } = &mut allocation
                {
                    split_identities.nodes[0] = node(duplicate);
                }
                Ok(allocation)
            },
            no_original,
        )
        .unwrap_err();
        assert_eq!(error.code, EditErrorCode::IdentityConflict, "{duplicate}");
    }
    let error = plan_semantic(
        &document,
        &context("root", 0),
        &program(vec![cut(1, 'a')]),
        SemanticRegisterBank {
            entries: &BTreeMap::new(),
            version: 0,
        },
        revision("outer"),
        |request| {
            let mut allocation = allocate(request)?;
            if let SemanticAllocation::Cut {
                split_identities, ..
            } = &mut allocation
            {
                split_identities.nodes.push(node("extra"));
            }
            Ok(allocation)
        },
        no_original,
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::InvalidCommand);
}

#[test]
fn semantic_program_wire_is_strict_bounded_and_has_no_capture_provenance() {
    let value = macro_value(vec![motion(true, 2), cut(7, 'a'), call('b', 3)]);
    assert_eq!(value.capture_revision(), None);
    let encoded = serde_json::to_string(&value).unwrap();
    assert_eq!(
        serde_json::from_str::<RegisterValue>(&encoded).unwrap(),
        *value
    );
    for invalid in [
        r#"{"instructions":[]}"#,
        r#"{"instructions":[{"type":"move_frames","forward":true,"count":0}]}"#,
        r#"{"instructions":[{"type":"move_frames","forward":true,"count":1,"cursor":7}]}"#,
        r#"{"instructions":[{"type":"move_frames","forward":true,"count":1,"count":2}]}"#,
        r#"{"instructions":[{"type":"call","register":"\"","count":1}]}"#,
        r#"{"instructions":[{"type":"call","register":"a","count":4294967296}]}"#,
        r#"{"instructions":[{"type":"shell","command":"exit"}]}"#,
        r#"{"instructions":[{"type":"move_frames","forward":true,"count":1}],"instructions":[]}"#,
    ] {
        assert!(
            serde_json::from_str::<SemanticProgram>(invalid).is_err(),
            "{invalid}"
        );
    }
    assert!(SemanticProgram::new(vec![]).is_err());
    assert!(
        SemanticProgram::new(vec![motion(true, 1); MAX_SEMANTIC_PROGRAM_INSTRUCTIONS + 1]).is_err()
    );
    assert!(SemanticProgram::new(vec![call('"', 1)]).is_err());
    let oversized = serde_json::json!({"instructions":vec![motion(true, 1); MAX_SEMANTIC_PROGRAM_INSTRUCTIONS + 1]});
    assert!(serde_json::from_value::<SemanticProgram>(oversized).is_err());
    let instruction = serde_json::to_string(&motion(true, 1)).unwrap();
    let oversized_scalar = format!(
        r#"{{"instructions":[{instruction}],"extra":"{}","unread"#,
        "x".repeat(MAX_SEMANTIC_PROGRAM_BYTES + 1)
    );
    // The byte bound must fire before the incomplete suffix or typed schema.
    let error = serde_json::from_str::<SemanticProgram>(&oversized_scalar).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("resolved transaction byte limit"),
        "{error}"
    );
    let mut incomplete = format!(
        "{{\"instructions\":[{}",
        vec![instruction; MAX_SEMANTIC_PROGRAM_INSTRUCTIONS].join(",")
    );
    incomplete.push_str(",{\"unread");
    // Reject the excess instruction before attempting its incomplete payload.
    assert!(
        serde_json::from_str::<SemanticProgram>(&incomplete)
            .unwrap_err()
            .to_string()
            .contains("1024 expanded steps")
    );
}

#[test]
fn motion_clamps_without_overflowing_an_absolute_end_boundary() {
    let document = fixture(i64::MAX);
    let planned = plan(
        &document,
        context("root", i64::MAX - 1),
        vec![motion(true, u32::MAX), motion(false, 1)],
        &BTreeMap::new(),
    )
    .unwrap();
    assert_eq!(planned.trace[0].after.cursor, ProjectFrame(i64::MAX));
    assert_eq!(planned.context.cursor, ProjectFrame(i64::MAX - 1));
    assert!(planned.request.is_none());
}

#[test]
fn cumulative_document_and_capture_budgets_refuse_before_another_leaf_allocation() {
    let document = fixture(8);
    let bank = BTreeMap::new();
    let size = wire::size(&document, MAX_DOCUMENT_JSON_BYTES).unwrap();
    for (document_bytes, captured_bytes) in [
        (MAX_COMPOUND_DOCUMENT_BYTES - size + 1, 0),
        (0, MAX_COMPOUND_CAPTURE_BYTES - size + 1),
    ] {
        let mut planner = Planner {
            current: document.clone(),
            context: context("root", 0),
            bounds: (ProjectFrame(0), ProjectFrame(8)),
            child_ends: vec![(node("held"), ProjectFrame(8))],
            bank: &bank,
            inputs: BTreeMap::new(),
            writes: BTreeMap::new(),
            steps: Vec::new(),
            trace: Vec::new(),
            calls: Vec::new(),
            nodes: document.nodes().keys().cloned().collect(),
            marks: BTreeSet::new(),
            revisions: BTreeSet::from([revision("base"), revision("outer")]),
            document_bytes,
            captured_bytes,
            allocate: |_: SemanticAllocationRequest| -> Result<SemanticAllocation, EditError> {
                panic!("exhausted work budget reached the leaf allocator")
            },
            resolve_original: no_original,
        };
        let error = planner.execute(&program(vec![cut(1, 'a')])).unwrap_err();
        assert_eq!(error.code, EditErrorCode::LimitExceeded);
        assert_eq!(planner.current, document);
        assert!(planner.writes.is_empty());
        assert!(planner.steps.is_empty());
    }
}

#[test]
fn resolved_edit_limit_is_separate_from_instruction_fuel() {
    let document = fixture(2048);
    let bank = BTreeMap::from([(name('a'), macro_value(vec![cut(1, 'b')]))]);
    let mut allocations = 0;
    let error = plan_semantic(
        &document,
        &context("root", 0),
        &program(vec![call('a', 1025)]),
        SemanticRegisterBank {
            entries: &bank,
            version: 0,
        },
        revision("outer"),
        |request| {
            allocations += 1;
            allocate(request)
        },
        no_original,
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::LimitExceeded);
    assert!(error.message.contains("resolved editing steps"));
    assert_eq!(allocations, 0);
}
