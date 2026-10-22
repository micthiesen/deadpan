//! Named-range resolver/capture controls before semantic selector admission.
//! Synthetic structural source metadata only; no media or audible claim.

use super::*;

#[path = "named_mark_roles.rs"]
mod roles;

fn frame_range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

fn source_span(start: i64, end: i64) -> SourceSpan {
    let time_base = SourceTimeBase::new(1001, 30_000).unwrap();
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

fn source_beat(label: &str, start: i64) -> BeatNode {
    let mut beat = hold(8);
    beat.label = label.into();
    beat.kind = NodeKind::Source {
        source: SourceNode {
            edit_window: None,
            duration: duration(8),
            video: SourceVideo::Stream {
                asset: AssetId::new("named-range-original").unwrap(),
                span: source_span(start, start + 8),
            },
            video_mapping: SourceVideoMapping::FitBeat,
            audio: None,
            audio_mapping: SourceAudioMapping::FitBeat,
            audio_offset: AudioSample(0),
            link: LinkRelation::Independent,
        },
    };
    beat
}

fn named_range_fixture() -> ProjectDocument {
    let with_asset = edit(
        &empty(),
        Command::AddAsset {
            id: AssetId::new("named-range-original").unwrap(),
            asset: AssetRecord {
                source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
                label: "Synthetic named-range Original".into(),
                content_hash: "a".repeat(64),
                video: Some(source_span(0, 32)),
                audio: None,
                still_image: false,
                frame_count: Some(duration(32)),
            },
        },
    )
    .0;
    let inserted = edit(
        &with_asset,
        Command::Insert {
            parent: node("root"),
            index: 0,
            subtree: Subtree {
                root: node("group"),
                nodes: BTreeMap::from([
                    (
                        node("group"),
                        BeatNode::sequence(
                            "Group",
                            vec![node("first"), node("held"), node("second")],
                        ),
                    ),
                    (node("first"), source_beat("First source", 0)),
                    (node("held"), hold(2)),
                    (node("second"), source_beat("Second source", 16)),
                ]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )
    .0;
    // These Edit marks are owned by their source beats. Their biased boundary
    // sides are included in the intended partial-endpoint capture.
    let with_start = put(
        &inserted,
        "named-range-start",
        "first",
        local("first", 2),
        InsertionBias::Right,
        AnchorLossPolicy::KeepUnresolved,
    );
    put(
        &with_start,
        "named-range-end",
        "second",
        local("second", 4),
        InsertionBias::Left,
        AnchorLossPolicy::KeepUnresolved,
    )
}

fn named_range() -> BoundarySelector {
    BoundarySelector::MarkRange {
        start: named("named-range-start"),
        end: named("named-range-end"),
    }
}

fn resolve_named_range(document: &ProjectDocument, expected_end: i64) -> FrameRange {
    let selector_wire = serde_json::to_value(named_range()).unwrap();
    assert_eq!(
        selector_wire,
        json!({
            "type":"mark_range",
            "start":{"id":"named-range-start"},
            "end":{"id":"named-range-end"},
        })
    );
    let selector = serde_json::from_value::<BoundarySelector>(selector_wire).unwrap();
    let resolved = select(document, selector).unwrap();
    assert_eq!(resolved.project_id, *document.project_id());
    assert_eq!(resolved.revision_id, *document.revision_id());
    assert_eq!(resolved.role, MediaRole::Linked);
    let ResolvedSelectionKind::Range { start, end, frames } = resolved.selection else {
        panic!("two named Edit marks resolve a range");
    };
    assert_eq!(start.exact_frame, ExactRatio::integer(2));
    assert_eq!(start.frame, ProjectFrame(2));
    assert_eq!(end.exact_frame, ExactRatio::integer(expected_end));
    assert_eq!(end.frame, ProjectFrame(expected_end));
    let start_mark = start.mark.as_ref().unwrap();
    let end_mark = end.mark.as_ref().unwrap();
    assert_eq!(start_mark.id, mark_id("named-range-start"));
    assert_eq!(start_mark.bindings, vec![0]);
    assert_eq!(end_mark.id, mark_id("named-range-end"));
    assert_eq!(end_mark.bindings, vec![0]);
    assert_eq!(frames, frame_range(2, expected_end));
    frames
}

fn visual_program() -> SemanticProgram {
    SemanticProgram::new(vec![SemanticInstruction::Yank {
        selector: SemanticSelector::VisualSelection,
        register: RegisterName::new('a').unwrap(),
    }])
    .unwrap()
}

fn capture_and_visual_yank(
    document: &ProjectDocument,
    expected_end: i64,
    held_frames: i64,
    label: &str,
) -> CapturedEditSlice {
    let snapshot = document.to_json().unwrap();
    let range = resolve_named_range(document, expected_end);
    let timing = AudioTimingId {
        allocation: RevisionId::new(format!("{label}-capture")).unwrap(),
        ordinal: 0,
    };
    let direct =
        CapturedEditSlice::capture(document, &node("group"), range, timing.clone()).unwrap();
    direct.validate_capture(document).unwrap();
    assert_eq!(direct.duration(), duration(expected_end - 2));
    assert_eq!(direct.range(), range);
    assert_eq!(direct.parent(), &node("group"));
    assert_eq!(direct.attachments(), SliceAttachments::Owned);
    assert_eq!(direct.capture_timing(), &timing);
    assert_eq!(direct.selection(), &SliceCaptureSelection::Range { range });
    let direct_wire = serde_json::to_value(&direct).unwrap();
    assert_eq!(
        direct_wire["parts"],
        json!([
            {"root":"first", "mapping":frame_range(2, 8), "source_start":ProjectFrame(0)},
            {"root":"held", "mapping":frame_range(0, held_frames), "source_start":ProjectFrame(8)},
            {"root":"second", "mapping":frame_range(0, 4), "source_start":ProjectFrame(8 + held_frames)},
        ])
    );
    // Full source recipes and owned marks survive the endpoint windows.
    for name in ["first", "held", "second"] {
        assert_eq!(
            direct_wire["nodes"][name],
            serde_json::to_value(&document.nodes()[&node(name)]).unwrap()
        );
    }
    assert_eq!(
        direct_wire["assets"],
        serde_json::to_value(document.assets()).unwrap()
    );
    assert_eq!(
        direct_wire["marks"],
        serde_json::to_value(document.marks()).unwrap()
    );
    assert_eq!(direct_wire["marks"].as_object().unwrap().len(), 2);
    assert_eq!(
        CapturedEditSlice::from_json(&direct.to_json().unwrap()).unwrap(),
        direct
    );

    // The existing semantic Visual selector consumes the same public range.
    // Use the same capture identity so the entire closed payload must agree.
    let context = SemanticContext {
        original: None,
        scoped: None,
        hold_duration: None,
        shared_hold_duration: None,
        mixed_hold_duration: None,
        selected_hold_duration: None,
        parent: node("group"),
        cursor: ProjectFrame(0),
        selected_child: Some(node("held")),
        visual_selection: Some(SemanticVisualSelection::Time {
            anchor: range.start(),
            head: range.end(),
            extending: false,
        }),
    };
    let program = visual_program();
    let visual_wire = serde_json::to_value(&program).unwrap();
    assert_eq!(visual_wire["instructions"][0]["type"], json!("yank"));
    assert_eq!(
        visual_wire["instructions"][0]["selector"],
        json!({"type":"visual_selection"})
    );
    assert_eq!(
        serde_json::from_value::<SemanticProgram>(visual_wire).unwrap(),
        program
    );
    let bank = BTreeMap::new();
    let mut allocations = Vec::new();
    let planned = plan_semantic(
        document,
        &context,
        &program,
        SemanticRegisterBank {
            entries: &bank,
            version: 7,
        },
        RevisionId::new(format!("{label}-visual-yank")).unwrap(),
        |request| {
            assert!(matches!(
                request,
                SemanticAllocationRequest::Yank { step_index: 0 }
            ));
            allocations.push(request);
            Ok(SemanticAllocation::Yank {
                capture_revision: timing.allocation.clone(),
            })
        },
        |_, _| panic!("Visual Yank never resolves an Original register"),
    )
    .unwrap();
    assert_eq!(allocations.len(), 1);
    assert!(bank.is_empty());
    assert_eq!(planned.document, *document);
    assert_eq!(planned.context, context);
    assert_eq!(planned.selected_child, Some(node("held")));
    assert_eq!(planned.register_writes.len(), 2);
    let named_register = RegisterName::new('a').unwrap();
    let RegisterValue::Edited { slice } = planned.register_writes[&named_register].as_ref() else {
        panic!("Visual Yank captures edited content");
    };
    assert_eq!(slice.as_ref(), &direct);
    assert_eq!(
        planned.register_writes[&named_register],
        planned.register_writes[&RegisterName::unnamed()]
    );
    assert_eq!(planned.trace.len(), 1);
    let trace = &planned.trace[0];
    assert_eq!(trace.resolved_parent, Some(node("group")));
    assert_eq!(trace.resolved_range, Some(range));
    assert_eq!(
        trace.resolved_selection,
        Some(SliceCaptureSelection::Range { range })
    );
    let provenance = trace.capture.as_ref().unwrap();
    assert_eq!(provenance.timing, timing);
    assert_eq!(provenance.parent, node("group"));
    assert_eq!(provenance.bounds, frame_range(0, 16 + held_frames));
    assert_eq!(provenance.scope, vec![node("group")]);
    assert_eq!(provenance.scope_labels, vec!["Group"]);
    let request = planned.request.as_ref().unwrap();
    let Command::Compound { transaction } = &request.command else {
        panic!("semantic Yank has a bank-only Compound request");
    };
    assert_eq!(transaction.expected_bank_version(), 7);
    assert_eq!(transaction.steps().len(), 1);
    assert!(matches!(transaction.steps()[0], ResolvedStep::Yank { .. }));
    assert!(transaction.steps()[0].edit().is_none());
    let replayed = replay_compound::<EditError>(document, request, |_| Ok(())).unwrap();
    assert_eq!(replayed.register_writes, planned.register_writes);
    // Generic replay reserves an outer revision. The semantic bank-only
    // result above deliberately preserves the original authored revision.
    let mut expected_replay = serde_json::to_value(document).unwrap();
    expected_replay["revision_id"] = serde_json::to_value(&request.new_revision).unwrap();
    assert_eq!(
        replayed.document,
        ProjectDocument::from_json(&expected_replay.to_string()).unwrap()
    );
    assert_eq!(replayed.edit.duration_delta, 0);
    assert_eq!(
        replayed.edit.inverse.apply(&replayed.document).unwrap(),
        *document
    );
    assert_eq!(document.to_json().unwrap(), snapshot);
    direct
}

#[test]
fn named_mark_range_semantic_yank_admits_after_public_resolver_and_visual_controls() {
    let before = named_range_fixture();
    before.validate().unwrap();
    assert_eq!(before.duration().unwrap(), duration(18));
    assert_eq!(
        before.marks()[&mark_id("named-range-start")]
            .boundary
            .coordinate,
        local("first", 2)
    );
    assert_eq!(
        before.marks()[&mark_id("named-range-end")]
            .boundary
            .coordinate,
        local("second", 4)
    );
    let before_capture = capture_and_visual_yank(&before, 14, 2, "named-before");
    assert_eq!(before_capture.duration(), duration(12));

    // Existing ordinary duration editing moves the second source's Edit
    // position, while both named marks retain their local source addresses.
    let (after, resized) = edit(
        &before,
        Command::SetHoldDuration {
            node: node("held"),
            duration: duration(4),
        },
    );
    assert_eq!(resized.duration_delta, 2);
    assert_eq!(after.duration().unwrap(), duration(20));
    assert_eq!(after.marks(), before.marks());
    assert_eq!(after.assets(), before.assets());
    for name in ["root", "group", "first", "second"] {
        assert_eq!(after.nodes()[&node(name)], before.nodes()[&node(name)]);
    }
    let after_capture = capture_and_visual_yank(&after, 16, 4, "named-after");
    assert_eq!(after_capture.duration(), duration(14));
    assert_eq!(
        after_capture.range().start(),
        before_capture.range().start()
    );
    assert_eq!(resized.inverse.apply(&after).unwrap(), before);
    assert_eq!(resized.forward.apply(&before).unwrap(), after);
    for document in [&before, &after] {
        assert_eq!(
            ProjectDocument::from_json(&document.to_json().unwrap()).unwrap(),
            *document
        );
    }

    // Use the existing actual wire tags and NamedMarkTarget field shapes.
    // No resolved timestamps, project/revision or captured result is saved
    // in this program. The sole old-code failure is selector admission.
    let mut wire = serde_json::to_value(visual_program()).unwrap();
    wire["instructions"][0]["selector"] = serde_json::to_value(named_range()).unwrap();
    let admitted = serde_json::from_value::<SemanticProgram>(wire);
    assert!(
        admitted.is_ok(),
        "A semantic Yank must admit the named mark_range selector after public resolver and Visual controls pass: {admitted:?}"
    );
    let (planned, allocations) = plan_named(&before, &context(), admitted.unwrap(), "admission");
    assert_eq!(
        allocations,
        vec![SemanticAllocationRequest::Yank { step_index: 0 }]
    );
    assert_eq!(planned.document, before);
    assert_eq!(planned.context, context());
    assert_eq!(captured(&planned).range(), before_capture.range());
    assert_eq!(captured(&planned).duration(), before_capture.duration());
    assert_eq!(planned.trace[0].resolved_range, Some(frame_range(2, 14)));
}

fn context() -> SemanticContext {
    SemanticContext {
        original: None,
        scoped: None,
        hold_duration: None,
        shared_hold_duration: None,
        mixed_hold_duration: None,
        selected_hold_duration: None,
        parent: node("group"),
        cursor: ProjectFrame(0),
        selected_child: Some(node("held")),
        visual_selection: None,
    }
}

fn selector() -> SemanticSelector {
    SemanticSelector::MarkRange {
        start: named("named-range-start"),
        end: named("named-range-end"),
    }
}

fn select_instruction() -> SemanticInstruction {
    SemanticInstruction::SelectMarkRange {
        start: named("named-range-start"),
        end: named("named-range-end"),
    }
}

fn yank_named() -> SemanticInstruction {
    SemanticInstruction::Yank {
        selector: selector(),
        register: RegisterName::new('a').unwrap(),
    }
}

fn allocate_named(
    request: SemanticAllocationRequest,
    tag: &str,
) -> Result<SemanticAllocation, EditError> {
    let revision =
        |step: usize, kind: &str| RevisionId::new(format!("named-{tag}-{kind}-{step}")).unwrap();
    Ok(match request {
        SemanticAllocationRequest::Yank { step_index } => SemanticAllocation::Yank {
            capture_revision: revision(step_index, "capture"),
        },
        SemanticAllocationRequest::ParameterEdit { step_index } => {
            SemanticAllocation::ParameterEdit {
                new_revision: revision(step_index, "edit"),
            }
        }
        SemanticAllocationRequest::Cut {
            step_index,
            required_split_ids,
        } => SemanticAllocation::Cut {
            new_revision: revision(step_index, "edit"),
            capture_revision: revision(step_index, "capture"),
            split_identities: SplitIdentities {
                nodes: (0..required_split_ids)
                    .map(|index| node(&format!("named-{tag}-split-{step_index}-{index}")))
                    .collect(),
            },
        },
        SemanticAllocationRequest::Repeat {
            step_index,
            required_split_ids,
            needs_group,
        } => SemanticAllocation::Repeat {
            new_revision: revision(step_index, "edit"),
            identities: RepeatSelectionIdentities {
                repeat: node(&format!("named-{tag}-repeat-{step_index}")),
                group: needs_group.then(|| node(&format!("named-{tag}-group-{step_index}"))),
                split: SplitIdentities {
                    nodes: (0..required_split_ids)
                        .map(|index| node(&format!("named-{tag}-split-{step_index}-{index}")))
                        .collect(),
                },
            },
        },
        other => panic!("unexpected named range allocation: {other:?}"),
    })
}

fn plan_named(
    document: &ProjectDocument,
    context: &SemanticContext,
    program: SemanticProgram,
    tag: &str,
) -> (SemanticPlan, Vec<SemanticAllocationRequest>) {
    let bank = BTreeMap::new();
    let mut allocations = Vec::new();
    let planned = plan_semantic(
        document,
        context,
        &program,
        SemanticRegisterBank {
            entries: &bank,
            version: 7,
        },
        RevisionId::new(format!("named-{tag}-final")).unwrap(),
        |request| {
            allocations.push(request.clone());
            allocate_named(request, tag)
        },
        |_, _| panic!("named Edit selectors never resolve Original registers"),
    )
    .unwrap();
    (planned, allocations)
}

fn captured(plan: &SemanticPlan) -> &CapturedEditSlice {
    let RegisterValue::Edited { slice } =
        plan.register_writes[&RegisterName::new('a').unwrap()].as_ref()
    else {
        panic!("expected an edited named range capture");
    };
    slice
}

#[test]
fn named_mark_selection_is_finished_typed_intent_and_rechecks_after_resize_and_calls() {
    let before = named_range_fixture();
    let original_context = context();
    let selected = before
        .select_semantic_mark_range(
            &original_context,
            named("named-range-start"),
            named("named-range-end"),
        )
        .unwrap();
    assert_eq!(original_context, context());
    assert_eq!(selected.cursor, ProjectFrame(14));
    assert_eq!(selected.selected_child, original_context.selected_child);
    assert_eq!(
        selected
            .visual_selection
            .as_ref()
            .unwrap()
            .effective_selector(),
        selector()
    );
    assert_eq!(
        before
            .resolve_semantic_mark_range(
                &selected,
                &named("named-range-start"),
                &named("named-range-end")
            )
            .unwrap(),
        frame_range(2, 14)
    );
    assert_eq!(
        serde_json::to_value(&selected.visual_selection).unwrap(),
        json!({
            "type":"mark_range", "start":{"id":"named-range-start"}, "end":{"id":"named-range-end"}
        })
    );
    let (selection_only, allocations) = plan_named(
        &before,
        &context(),
        SemanticProgram::new(vec![
            select_instruction(),
            SemanticInstruction::FinishSelection,
        ])
        .unwrap(),
        "selection-only",
    );
    assert!(allocations.is_empty());
    assert!(selection_only.request.is_none());
    assert!(selection_only.register_writes.is_empty());
    assert_eq!(selection_only.document, before);
    assert_eq!(selection_only.context, selected);

    let resize = SemanticInstruction::SetHoldDuration {
        length: PauseLength::Frames {
            frames: std::num::NonZeroU32::new(4).unwrap(),
        },
    };
    let body = SemanticProgram::new(vec![
        resize,
        select_instruction(),
        SemanticInstruction::Yank {
            selector: SemanticSelector::VisualSelection,
            register: RegisterName::new('a').unwrap(),
        },
    ])
    .unwrap();
    let bank = BTreeMap::from([(
        RegisterName::new('b').unwrap(),
        std::sync::Arc::new(RegisterValue::Macro {
            program: std::sync::Arc::new(body.clone()),
        }),
    )]);
    let original_bank = bank.clone();
    let mut allocations = Vec::new();
    let planned = plan_semantic(
        &before,
        &context(),
        &SemanticProgram::new(vec![SemanticInstruction::Call {
            register: RegisterName::new('b').unwrap(),
            count: std::num::NonZeroU32::new(1).unwrap(),
        }])
        .unwrap(),
        SemanticRegisterBank {
            entries: &bank,
            version: 7,
        },
        RevisionId::new("named-call-final").unwrap(),
        |request| {
            allocations.push(request.clone());
            allocate_named(request, "call")
        },
        |_, _| panic!("no Original callback"),
    )
    .unwrap();
    assert_eq!(bank, original_bank);
    assert_eq!(
        allocations,
        vec![
            SemanticAllocationRequest::ParameterEdit { step_index: 0 },
            SemanticAllocationRequest::Yank { step_index: 1 }
        ]
    );
    assert_eq!(planned.document.duration().unwrap(), duration(20));
    assert_eq!(planned.context.cursor, ProjectFrame(16));
    assert_eq!(planned.context.visual_selection, selected.visual_selection);
    assert_eq!(
        planned
            .document
            .resolve_semantic_mark_range(
                &planned.context,
                &named("named-range-start"),
                &named("named-range-end")
            )
            .unwrap(),
        frame_range(2, 16)
    );
    assert_eq!(captured(&planned).range(), frame_range(2, 16));
    assert_eq!(captured(&planned).duration(), duration(14));
    assert!(
        planned
            .trace
            .iter()
            .any(|trace| trace.instruction == select_instruction()
                && trace.resolved_range == Some(frame_range(2, 16)))
    );
    let replayed =
        replay_compound::<EditError>(&before, planned.request.as_ref().unwrap(), |_| Ok(()))
            .unwrap();
    assert_eq!(replayed.document, planned.document);
    assert_eq!(replayed.register_writes, planned.register_writes);
    assert_eq!(
        replayed.edit.inverse.apply(&replayed.document).unwrap(),
        before
    );
}

#[test]
fn named_mark_range_cut_and_repeat_use_actual_range_and_atomic_inverse() {
    let before = named_range_fixture();
    for (tag, instruction, expected_delta) in [
        (
            "cut",
            SemanticInstruction::Cut {
                selector: selector(),
                register: RegisterName::new('a').unwrap(),
            },
            -12,
        ),
        (
            "repeat",
            SemanticInstruction::Repeat {
                selector: selector(),
                plays: std::num::NonZeroU32::new(3).unwrap(),
                escalation: None,
            },
            24,
        ),
    ] {
        let (planned, allocations) = plan_named(
            &before,
            &context(),
            SemanticProgram::new(vec![instruction.clone()]).unwrap(),
            tag,
        );
        assert_eq!(allocations.len(), 1);
        let required = before
            .range_deletion(&node("group"), frame_range(2, 14))
            .unwrap()
            .required_ids;
        match &allocations[0] {
            SemanticAllocationRequest::Cut {
                required_split_ids, ..
            } => assert_eq!(*required_split_ids, required),
            SemanticAllocationRequest::Repeat {
                required_split_ids,
                needs_group,
                ..
            } => {
                let plan = before
                    .repeat_selection(
                        &node("group"),
                        &SliceCaptureSelection::Range {
                            range: frame_range(2, 14),
                        },
                        3,
                    )
                    .unwrap();
                assert_eq!(*required_split_ids, plan.required_split_ids);
                assert_eq!(*needs_group, plan.needs_group);
            }
            other => panic!("unexpected mutation allocation: {other:?}"),
        }
        assert_eq!(planned.trace[0].instruction, instruction);
        assert_eq!(
            planned.document.duration().unwrap().frames(),
            18 + expected_delta
        );
        assert_eq!(planned.context.cursor, ProjectFrame(2));
        assert!(planned.context.visual_selection.is_none());
        let replayed =
            replay_compound::<EditError>(&before, planned.request.as_ref().unwrap(), |_| Ok(()))
                .unwrap();
        assert_eq!(replayed.document, planned.document);
        assert_eq!(replayed.edit.duration_delta, expected_delta);
        assert_eq!(
            replayed.edit.inverse.apply(&replayed.document).unwrap(),
            before
        );
        assert_eq!(
            ProjectDocument::from_json(&planned.document.to_json().unwrap()).unwrap(),
            planned.document
        );
    }
}

#[test]
fn named_mark_range_rejections_allocate_nothing_and_never_fall_back() {
    let before = named_range_fixture();
    let mut cases = Vec::new();
    cases.push((
        before.clone(),
        context(),
        named("missing"),
        named("named-range-end"),
        "does not exist",
    ));
    cases.push((
        before.clone(),
        context(),
        named("named-range-end"),
        named("named-range-start"),
        "range end must follow",
    ));
    let collapsed = put(
        &before,
        "collapsed-end",
        "first",
        Anchor::Local {
            node: node("first"),
            position: ratio(9, 4),
        },
        InsertionBias::Left,
        AnchorLossPolicy::KeepUnresolved,
    );
    cases.push((
        collapsed,
        context(),
        named("named-range-start"),
        named("collapsed-end"),
        "quantization",
    ));
    let unresolved = edit(
        &before,
        Command::Delete {
            node: node("first"),
        },
    )
    .0;
    cases.push((
        unresolved,
        context(),
        named("named-range-start"),
        named("named-range-end"),
        "bound fragments",
    ));
    let wrapped = edit(
        &before,
        Command::WrapRepeat {
            node: node("first"),
            id: node("ambiguous-repeat"),
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        },
    )
    .0;
    let ambiguous = put(
        &wrapped,
        "ambiguous-start",
        "first",
        local("first", 2),
        InsertionBias::Right,
        AnchorLossPolicy::KeepUnresolved,
    );
    cases.push((
        ambiguous,
        context(),
        named("ambiguous-start"),
        named("named-range-end"),
        "occurrence",
    ));
    let original_mark = put(
        &before,
        "original-clock",
        "root",
        Anchor::Source {
            asset: AssetId::new("named-range-original").unwrap(),
            moment: SourceMoment::Timestamp {
                stream: SourceStream::Video,
                timestamp: source_span(2, 3).start(),
            },
        },
        InsertionBias::Right,
        AnchorLossPolicy::KeepUnresolved,
    );
    cases.push((
        original_mark,
        context(),
        named("original-clock"),
        named("named-range-end"),
        "Original clock",
    ));
    let outside = edit(
        &before,
        Command::Insert {
            parent: node("root"),
            index: 1,
            subtree: Subtree {
                root: node("outside"),
                nodes: BTreeMap::from([(node("outside"), hold(3))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )
    .0;
    let outside = put(
        &outside,
        "outside-end",
        "outside",
        local("outside", 1),
        InsertionBias::Left,
        AnchorLossPolicy::KeepUnresolved,
    );
    cases.push((
        outside,
        context(),
        named("named-range-start"),
        named("outside-end"),
        "outside the current Sequence",
    ));
    let mut occurrence = named("named-range-start");
    occurrence.occurrence = Some(InstancePath {
        node: node("absent"),
        repeats: vec![],
    });
    cases.push((
        before,
        context(),
        occurrence,
        named("named-range-end"),
        "occurrence",
    ));
    for (document, mut context, start, end, message) in cases {
        // A usable-looking unrelated selection must not rescue a bad mark.
        context.visual_selection = Some(SemanticVisualSelection::Time {
            anchor: ProjectFrame(0),
            head: ProjectFrame(1),
            extending: false,
        });
        let snapshot = document.to_json().unwrap();
        let program = SemanticProgram::new(vec![SemanticInstruction::Yank {
            selector: SemanticSelector::MarkRange {
                start: start.clone(),
                end: end.clone(),
            },
            register: RegisterName::new('a').unwrap(),
        }])
        .unwrap();
        let bank = BTreeMap::new();
        let error = plan_semantic(
            &document,
            &context,
            &program,
            SemanticRegisterBank {
                entries: &bank,
                version: 0,
            },
            RevisionId::new("named-invalid-final").unwrap(),
            |_| panic!("invalid named selector allocated"),
            |_, _| panic!("no Original callback"),
        )
        .unwrap_err();
        assert_eq!(error.code, EditErrorCode::SelectionUnavailable, "{error:?}");
        assert!(
            error
                .message
                .to_lowercase()
                .contains(&message.to_lowercase()),
            "expected {message:?}: {error:?}"
        );
        assert_eq!(document.to_json().unwrap(), snapshot);
        assert!(bank.is_empty());
        assert!(
            document
                .select_semantic_mark_range(&context, start, end)
                .is_err()
        );
    }
}

#[test]
fn named_mark_range_late_failure_publishes_neither_staged_edit_nor_capture() {
    let before = named_range_fixture();
    let bank = BTreeMap::from([(
        RegisterName::new('z').unwrap(),
        std::sync::Arc::new(RegisterValue::Macro {
            program: std::sync::Arc::new(visual_program()),
        }),
    )]);
    let snapshot = before.to_json().unwrap();
    let original_bank = bank.clone();
    let program = SemanticProgram::new(vec![
        yank_named(),
        SemanticInstruction::SetHoldDuration {
            length: PauseLength::Frames {
                frames: std::num::NonZeroU32::new(4).unwrap(),
            },
        },
        SemanticInstruction::Yank {
            selector: SemanticSelector::MarkRange {
                start: named("missing-late"),
                end: named("named-range-end"),
            },
            register: RegisterName::new('a').unwrap(),
        },
    ])
    .unwrap();
    let mut allocations = Vec::new();
    let error = plan_semantic(
        &before,
        &context(),
        &program,
        SemanticRegisterBank {
            entries: &bank,
            version: 7,
        },
        RevisionId::new("named-late-final").unwrap(),
        |request| {
            allocations.push(request.clone());
            allocate_named(request, "late")
        },
        |_, _| panic!("no Original callback"),
    )
    .unwrap_err();
    assert_eq!(error.code, EditErrorCode::SelectionUnavailable);
    assert!(error.message.contains("missing-late"));
    assert_eq!(
        allocations,
        vec![
            SemanticAllocationRequest::Yank { step_index: 0 },
            SemanticAllocationRequest::ParameterEdit { step_index: 1 }
        ]
    );
    assert_eq!(before.to_json().unwrap(), snapshot);
    assert_eq!(bank, original_bank);
}

#[test]
fn named_mark_range_wire_retains_only_addresses_and_rejects_unknown_fields() {
    let program = SemanticProgram::new(vec![select_instruction(), yank_named()]).unwrap();
    let value = serde_json::to_value(&program).unwrap();
    assert_eq!(
        serde_json::from_value::<SemanticProgram>(value.clone()).unwrap(),
        program
    );
    assert_eq!(value["instructions"][0]["type"], json!("select_mark_range"));
    for path in [0, 1] {
        let mut invalid = value.clone();
        let target = if path == 0 {
            &mut invalid["instructions"][0]
        } else {
            &mut invalid["instructions"][1]["selector"]
        };
        target["frames"] = json!({"start":2,"end":14});
        assert!(serde_json::from_value::<SemanticProgram>(invalid).is_err());
    }
    let mut invalid = value;
    invalid["instructions"][1]["selector"]["start"]["fallback"] = json!(true);
    assert!(serde_json::from_value::<SemanticProgram>(invalid).is_err());
}

#[test]
fn named_mark_selection_replaces_invalid_prior_selection_and_navigation_retains_finished_intent() {
    let before = named_range_fixture();
    let mut invalid_prior = context();
    invalid_prior.visual_selection = Some(SemanticVisualSelection::MarkRange {
        start: named("missing-old-selection"),
        end: named("named-range-end"),
    });
    assert!(
        before
            .resolve_semantic_mark_range(
                &invalid_prior,
                &named("named-range-start"),
                &named("named-range-end")
            )
            .is_err()
    );
    let replaced = before
        .select_semantic_mark_range(
            &invalid_prior,
            named("named-range-start"),
            named("named-range-end"),
        )
        .unwrap();
    assert_eq!(replaced.cursor, ProjectFrame(14));
    assert_eq!(
        replaced
            .visual_selection
            .as_ref()
            .unwrap()
            .effective_selector(),
        selector()
    );
    assert!(
        matches!(&invalid_prior.visual_selection, Some(SemanticVisualSelection::MarkRange { start, .. }) if start.id == mark_id("missing-old-selection"))
    );
    let (moved, allocations) = plan_named(
        &before,
        &replaced,
        SemanticProgram::new(vec![SemanticInstruction::MoveFrames {
            forward: false,
            count: std::num::NonZeroU32::new(1).unwrap(),
        }])
        .unwrap(),
        "finished-navigation",
    );
    assert!(allocations.is_empty());
    assert_eq!(moved.context.cursor, ProjectFrame(13));
    assert_eq!(moved.context.visual_selection, replaced.visual_selection);
    let (plain, allocations) = plan_named(
        &before,
        &moved.context,
        SemanticProgram::new(vec![SemanticInstruction::BeginSelection]).unwrap(),
        "replace-with-time",
    );
    assert!(allocations.is_empty());
    assert!(matches!(
        plain.context.visual_selection,
        Some(SemanticVisualSelection::Time {
            anchor: ProjectFrame(13),
            head: ProjectFrame(13),
            extending: true
        })
    ));
}
