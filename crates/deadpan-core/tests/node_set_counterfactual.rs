//! Whole-node-set public counterfactual using synthetic structural metadata.
//! Existing public controls precede the final positive Nodes admission.
//! Baseline results are retained separately from later positive coverage.

use deadpan_core::*;
use serde_json::json;
use std::{collections::BTreeMap, sync::Arc};

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}

fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}

fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

fn timing(value: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: revision(value),
        ordinal: 0,
    }
}

fn register(value: char) -> RegisterName {
    RegisterName::new(value).unwrap()
}

fn hold(label: &str, count: i64) -> BeatNode {
    BeatNode::hold(
        label,
        HoldRecipe {
            duration: frames(count),
            picture_context: None,
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}

fn span(start: i64, end: i64) -> SourceSpan {
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

fn source(start: i64) -> BeatNode {
    // The two selected Sources intentionally share a label. IDs select them.
    let mut beat = hold("Selected source", 8);
    beat.kind = NodeKind::Source {
        source: SourceNode {
            duration: frames(8),
            edit_window: None,
            video: SourceVideo::Stream {
                asset: AssetId::new("node-set-synthetic-original").unwrap(),
                span: span(start, start + 8),
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

fn edit(document: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(name),
        command,
    };
    assert_eq!(
        serde_json::from_str::<CommandRequest>(&serde_json::to_string(&request).unwrap()).unwrap(),
        request
    );
    let transaction = apply(document, &request).unwrap();
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    after
}

fn insert(
    document: &ProjectDocument,
    name: &str,
    index: usize,
    root: &str,
    nodes: BTreeMap<NodeId, BeatNode>,
) -> ProjectDocument {
    edit(
        document,
        name,
        Command::Insert {
            parent: node("root"),
            index,
            subtree: Subtree {
                root: node(root),
                nodes,
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )
}

fn fixture() -> ProjectDocument {
    let document = ProjectDocument::new(
        ProjectId::new("node-set-counterfactual").unwrap(),
        revision("node-set-initial"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let document = edit(
        &document,
        "node-set-asset",
        Command::AddAsset {
            id: AssetId::new("node-set-synthetic-original").unwrap(),
            asset: AssetRecord {
                label: "Synthetic structural Original, no actual media".into(),
                content_hash: "a".repeat(64),
                video: Some(span(0, 32)),
                audio: None,
                still_image: false,
                frame_count: Some(frames(32)),
                source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
            },
        },
    );
    let document = insert(
        &document,
        "node-set-lead",
        0,
        "lead",
        BTreeMap::from([(node("lead"), hold("Unselected lead", 3))]),
    );
    let document = insert(
        &document,
        "node-set-outer",
        1,
        "outer",
        BTreeMap::from([
            (
                node("outer"),
                BeatNode::sequence(
                    "Outer navigation group",
                    vec![
                        node("empty-a"),
                        node("empty-b"),
                        node("source-a"),
                        node("gap"),
                        node("inner"),
                    ],
                ),
            ),
            (
                node("empty-a"),
                BeatNode::sequence("Same empty label", vec![]),
            ),
            (
                node("empty-b"),
                BeatNode::sequence("Same empty label", vec![]),
            ),
            (node("source-a"), source(0)),
            (node("gap"), hold("Untouched gap", 2)),
            (
                node("inner"),
                BeatNode::sequence("Inner actual parent", vec![node("source-b")]),
            ),
            (node("source-b"), source(16)),
        ]),
    );
    let document = insert(
        &document,
        "node-set-tail",
        2,
        "tail",
        BTreeMap::from([(node("tail"), hold("Unselected tail", 3))]),
    );
    // Independent owned marks keep the admission witness focused. Shared
    // logical mark bindings across capsules need a separate future regression.
    [("source-a", "owned-a", 2), ("source-b", "owned-b", 4)]
        .into_iter()
        .fold(document, |document, (owner, id, at)| {
            edit(
                &document,
                &format!("node-set-mark-{id}"),
                Command::SetMark {
                    id: MarkId::new(id).unwrap(),
                    owner: node(owner),
                    label: format!("Independent {id}"),
                    boundary: BoundaryAnchor {
                        coordinate: Anchor::Local {
                            node: node(owner),
                            position: ExactRatio::integer(at),
                        },
                        bias: InsertionBias::Right,
                    },
                    loss_policy: AnchorLossPolicy::DeleteOwned,
                },
            )
        })
}

fn requested_context() -> SemanticContext {
    SemanticContext {
        original: None,
        scoped: None,
        hold_duration: None,
        shared_hold_duration: None,
        mixed_hold_duration: None,
        selected_hold_duration: None,
        parent: node("root"),
        cursor: ProjectFrame(23),
        selected_child: Some(node("tail")),
        visual_selection: Some(SemanticVisualSelection::Time {
            anchor: ProjectFrame(0),
            head: ProjectFrame(3),
            extending: false,
        }),
    }
}

fn capture(
    document: &ProjectDocument,
    parent: &str,
    selected: &str,
    capture_id: &str,
) -> CapturedEditSlice {
    let selection = SliceCaptureSelection::Child {
        node: node(selected),
    };
    let captured = CapturedEditSlice::capture_selection(
        document,
        &node(parent),
        &selection,
        timing(capture_id),
    )
    .unwrap();
    captured.validate_capture(document).unwrap();
    assert_eq!(captured.parent(), &node(parent));
    assert_eq!(captured.selection(), &selection);
    assert_eq!(captured.attachments(), SliceAttachments::Owned);
    assert_eq!(
        CapturedEditSlice::from_json(&captured.to_json().unwrap()).unwrap(),
        captured
    );
    captured
}

fn edited(plan: &SemanticPlan, name: char) -> &CapturedEditSlice {
    let RegisterValue::Edited { slice } = plan.register_writes[&register(name)].as_ref() else {
        panic!("one ordinary existing Edited capture expected");
    };
    slice
}

fn single_yank(
    document: &ProjectDocument,
    entry: &SemanticContext,
    selected: &str,
    capture_id: &str,
) -> SemanticPlan {
    let bank = BTreeMap::new();
    let mut allocations = Vec::new();
    let program = SemanticProgram::new(vec![SemanticInstruction::Yank {
        selector: SemanticSelector::Node {
            node: node(selected),
        },
        register: register('a'),
    }])
    .unwrap();
    let plan = plan_semantic(
        document,
        entry,
        &program,
        SemanticRegisterBank {
            entries: &bank,
            version: 41,
        },
        revision(&format!("{capture_id}-final")),
        |request| {
            assert_eq!(request, SemanticAllocationRequest::Yank { step_index: 0 });
            allocations.push(request);
            Ok(SemanticAllocation::Yank {
                capture_revision: revision(capture_id),
            })
        },
        |_, _| panic!("no Original register is used"),
    )
    .unwrap();
    assert_eq!(allocations.len(), 1);
    assert_eq!(plan.document, *document);
    assert_eq!(plan.context, *entry);
    assert_eq!(plan.register_writes.len(), 2);
    assert_eq!(
        plan.register_writes[&register('a')],
        plan.register_writes[&RegisterName::unnamed()]
    );
    assert!(bank.is_empty());
    let Command::Compound { transaction } = &plan.request.as_ref().unwrap().command else {
        panic!("bank-only Compound expected");
    };
    assert_eq!(transaction.expected_bank_version(), 41);
    assert_eq!(transaction.steps().len(), 1);
    assert!(matches!(transaction.steps()[0], ResolvedStep::Yank { .. }));
    assert!(transaction.steps()[0].edit().is_none());
    plan
}

#[test]
fn explicit_nodes_selector_admits_only_after_public_single_node_controls() {
    let before = fixture();
    before.validate().unwrap();
    assert_eq!(before.duration().unwrap(), frames(24));
    let frozen_document = before.to_json().unwrap();
    let requested = requested_context();
    let frozen_context = requested.clone();
    let untouched_gap = before.nodes()[&node("gap")].clone();
    let bank: BTreeMap<RegisterName, Arc<RegisterValue>> = BTreeMap::new();

    // This literal structural order agrees with sibling paths, including two
    // different zero-duration children at the same Edit boundary. It is an
    // oracle for future Nodes canonicalization, not a current set API.
    let canonical = [
        ("empty-a", "outer", range(3, 3), None),
        ("empty-b", "outer", range(3, 3), None),
        ("source-a", "outer", range(3, 11), Some("owned-a")),
        ("source-b", "inner", range(13, 21), Some("owned-b")),
    ];
    let mut separate_capsules = BTreeMap::new();
    for (selected, parent, expected_range, owned_mark) in &canonical {
        let target = before
            .resolve_semantic_node(&requested, &node(selected))
            .unwrap();
        assert_eq!(target.parent, node(parent));
        assert_eq!(target.navigation_child, node("outer"));
        assert_eq!(target.range, *expected_range);
        let geometry = before
            .sequence_children(&node(parent), &node(selected), &node(selected))
            .unwrap();
        assert_eq!(geometry.range, *expected_range);
        if parent == &"outer" {
            let expected_slot = match *selected {
                "empty-a" => 0,
                "empty-b" => 1,
                "source-a" => 2,
                _ => unreachable!(),
            };
            assert_eq!(
                (geometry.first, geometry.end),
                (expected_slot, expected_slot + 1)
            );
        } else {
            assert_eq!((geometry.first, geometry.end), (0, 1));
        }
        let capture_id = format!("node-set-public-capture-{selected}");
        let direct = capture(&before, parent, selected, &capture_id);
        assert_eq!(direct.range(), *expected_range);
        assert_eq!(direct.duration(), expected_range.duration());
        let payload = serde_json::to_value(&direct).unwrap();
        assert_eq!(payload["nodes"].as_object().unwrap().len(), 1);
        assert_eq!(
            payload["nodes"][*selected],
            serde_json::to_value(&before.nodes()[&node(selected)]).unwrap()
        );
        assert_eq!(
            payload["parts"],
            json!([{
                "root": selected,
                "mapping": range(0, expected_range.duration().frames()),
                "source_start": expected_range.start(),
            }])
        );
        assert!(payload["nodes"].get("gap").is_none());
        assert!(payload["nodes"].get("tail").is_none());
        let marks = payload["marks"].as_object().unwrap();
        match owned_mark {
            Some(mark) => {
                assert_eq!(marks.len(), 1);
                assert_eq!(
                    marks[*mark],
                    serde_json::to_value(&before.marks()[&MarkId::new(*mark).unwrap()]).unwrap()
                );
                assert_eq!(
                    payload["assets"],
                    serde_json::to_value(before.assets()).unwrap()
                );
            }
            None => {
                assert!(marks.is_empty());
                assert!(payload["assets"].as_object().unwrap().is_empty());
            }
        }
        let control = single_yank(&before, &requested, selected, &capture_id);
        assert_eq!(edited(&control, 'a'), &direct);
        assert_eq!(control.trace[0].resolved_parent, Some(node(parent)));
        assert_eq!(control.trace[0].resolved_range, Some(*expected_range));
        assert_eq!(
            control.trace[0].resolved_selection,
            Some(SliceCaptureSelection::Child {
                node: node(selected)
            })
        );
        separate_capsules.insert(*selected, direct);
    }
    assert_eq!(
        separate_capsules["empty-a"].range(),
        separate_capsules["empty-b"].range()
    );
    assert_ne!(
        separate_capsules["empty-a"].selection(),
        separate_capsules["empty-b"].selection()
    );
    assert_eq!(
        separate_capsules["source-a"].duration().frames()
            + separate_capsules["source-b"].duration().frames(),
        16
    );
    assert_eq!(range(3, 21).duration(), frames(18));
    assert_eq!(before.nodes()[&node("gap")], untouched_gap);

    // Independent ordinary Cut instructions deliberately use separate named
    // registers. This proves the existing descending atomic reducer and its
    // inverse, not a union register or a future multi-selection feature.
    let descending = [
        ("source-b", 'b', 16, "inner", range(13, 21)),
        ("source-a", 'a', 8, "outer", range(3, 11)),
        ("empty-b", 'f', 8, "outer", range(3, 3)),
        ("empty-a", 'e', 8, "outer", range(3, 3)),
    ];
    let program = SemanticProgram::new(
        descending
            .iter()
            .map(|(selected, name, _, _, _)| SemanticInstruction::Cut {
                selector: SemanticSelector::Node {
                    node: node(selected),
                },
                register: register(*name),
            })
            .collect(),
    )
    .unwrap();
    let mut allocations = Vec::new();
    let plan = plan_semantic(
        &before,
        &requested,
        &program,
        SemanticRegisterBank {
            entries: &bank,
            version: 41,
        },
        revision("node-set-descending-final"),
        |request| {
            let SemanticAllocationRequest::Cut {
                step_index,
                required_split_ids,
            } = request.clone()
            else {
                panic!("whole-child Cut is the only expected allocation");
            };
            assert_eq!(step_index, allocations.len());
            assert_eq!(required_split_ids, 0);
            allocations.push(request);
            Ok(SemanticAllocation::Cut {
                new_revision: revision(&format!("node-set-descending-leaf-{step_index}")),
                capture_revision: revision(&format!("node-set-descending-capture-{step_index}")),
                split_identities: SplitIdentities { nodes: vec![] },
            })
        },
        |_, _| panic!("no Original register is used"),
    )
    .unwrap();
    assert_eq!(allocations.len(), 4);
    assert_eq!(plan.document.duration().unwrap(), frames(8));
    assert_eq!(plan.context.parent, node("root"));
    assert_eq!(plan.context.selected_child, Some(node("outer")));
    assert_eq!(plan.context.cursor, ProjectFrame(3));
    assert_eq!(plan.context.visual_selection, None);
    assert_eq!(plan.document.nodes()[&node("gap")], untouched_gap);
    assert_eq!(
        plan.document.nodes()[&node("lead")],
        before.nodes()[&node("lead")]
    );
    assert_eq!(
        plan.document.nodes()[&node("tail")],
        before.nodes()[&node("tail")]
    );
    assert_eq!(
        plan.document
            .children(&node("root"))
            .cloned()
            .collect::<Vec<_>>(),
        vec![node("lead"), node("outer"), node("tail")]
    );
    assert_eq!(
        plan.document
            .children(&node("outer"))
            .cloned()
            .collect::<Vec<_>>(),
        vec![node("gap"), node("inner")]
    );
    assert!(plan.document.children(&node("inner")).next().is_none());
    assert!(plan.document.marks().is_empty());
    assert_eq!(plan.register_writes.len(), 5);
    assert_eq!(
        plan.register_writes[&RegisterName::unnamed()],
        plan.register_writes[&register('e')]
    );
    for (selected, name, _, parent, expected_range) in &descending {
        let slice = edited(&plan, *name);
        assert_eq!(
            slice.selection(),
            &SliceCaptureSelection::Child {
                node: node(selected)
            }
        );
        assert_eq!(slice.parent(), &node(parent));
        assert_eq!(slice.range(), *expected_range);
    }
    let request = plan.request.as_ref().unwrap();
    assert_eq!(
        serde_json::from_str::<CommandRequest>(&serde_json::to_string(request).unwrap()).unwrap(),
        *request
    );
    let Command::Compound { transaction } = &request.command else {
        panic!("the existing semantic cuts must use one Compound");
    };
    assert_eq!(transaction.steps().len(), 4);
    let mut visited = 0usize;
    let replay = replay_compound::<EditError>(&before, request, |visit| {
        let (selected, name, expected_extent, _, _) = descending[visited];
        let ResolvedStep::Cut { name: actual_name, slice, delete } = visit.step else {
            panic!("every control step is one ordinary Cut");
        };
        assert_eq!(*actual_name, register(name));
        slice.validate_capture(visit.before).unwrap();
        assert_eq!(slice.selection(), &SliceCaptureSelection::Child { node: node(selected) });
        assert!(matches!(delete.command.as_command(), Command::DeleteRipple { node: deleted, .. } if deleted == &node(selected)));
        assert_eq!(visit.after.duration().unwrap(), frames(expected_extent));
        assert_eq!(visit.after.nodes()[&node("gap")], untouched_gap);
        assert!(visit.before.nodes().contains_key(&node(selected)));
        assert!(!visit.after.nodes().contains_key(&node(selected)));
        let ordinary = apply(visit.before, visit.request.expect("a Cut has an ordinary leaf request")).unwrap();
        assert_eq!(ordinary.forward.apply(visit.before).unwrap(), *visit.after);
        assert_eq!(ordinary.inverse.apply(visit.after).unwrap(), *visit.before);
        visited += 1;
        Ok(())
    }).unwrap();
    assert_eq!(visited, 4);
    assert_eq!(replay.document, plan.document);
    assert_eq!(replay.register_writes, plan.register_writes);
    assert_eq!(replay.edit.duration_delta, -16);
    assert_eq!(replay.edit.inverse.apply(&replay.document).unwrap(), before);
    assert_eq!(
        ProjectDocument::from_json(&replay.document.to_json().unwrap()).unwrap(),
        replay.document
    );

    // A same-register loop has a different, intentionally insufficient result:
    // it leaves only the final ordinary capture, never a union of both Sources.
    let overwrite = SemanticProgram::new(vec![
        SemanticInstruction::Yank {
            selector: SemanticSelector::Node {
                node: node("source-a"),
            },
            register: register('a'),
        },
        SemanticInstruction::Yank {
            selector: SemanticSelector::Node {
                node: node("source-b"),
            },
            register: register('a'),
        },
    ])
    .unwrap();
    let overwritten = plan_semantic(
        &before,
        &requested,
        &overwrite,
        SemanticRegisterBank {
            entries: &bank,
            version: 41,
        },
        revision("node-set-overwrite-final"),
        |request| {
            let SemanticAllocationRequest::Yank { step_index } = request else {
                panic!("only ordinary Yank allocations expected");
            };
            Ok(SemanticAllocation::Yank {
                capture_revision: revision(&format!("node-set-overwrite-capture-{step_index}")),
            })
        },
        |_, _| panic!("no Original register is used"),
    )
    .unwrap();
    assert_eq!(overwritten.document, before);
    assert_eq!(overwritten.context, requested);
    assert_eq!(overwritten.register_writes.len(), 2);
    assert_eq!(
        edited(&overwritten, 'a').selection(),
        &SliceCaptureSelection::Child {
            node: node("source-b")
        }
    );
    assert_eq!(edited(&overwritten, 'a').duration(), frames(8));
    assert_ne!(edited(&overwritten, 'a').duration(), frames(16));

    assert_eq!(before.to_json().unwrap(), frozen_document);
    assert_eq!(requested, frozen_context);
    assert!(bank.is_empty());

    // No future Nodes API is referenced above. Its strict wire admission is
    // intentionally the sole final old-code positive assertion after controls.
    let mut wire = serde_json::to_value(
        SemanticProgram::new(vec![SemanticInstruction::Yank {
            selector: SemanticSelector::Node {
                node: node("source-a"),
            },
            register: register('a'),
        }])
        .unwrap(),
    )
    .unwrap();
    wire["instructions"][0]["selector"] = json!({
        "type": "nodes",
        "nodes": ["source-b", "empty-b", "source-a", "empty-a"],
    });
    assert_eq!(
        wire["instructions"][0]["selector"]["nodes"],
        json!(["source-b", "empty-b", "source-a", "empty-a"])
    );
    let admitted = serde_json::from_value::<SemanticProgram>(wire);
    assert!(
        admitted.is_ok(),
        "Strict Nodes admission must follow all existing public controls: {admitted:?}"
    );

    // Postimplementation coverage begins here. The retained old-code witness
    // ends at the admission assertion above; its frozen source is unchanged.
    let admitted = admitted.unwrap();
    let union = plan_semantic(
        &before,
        &requested,
        &admitted,
        SemanticRegisterBank {
            entries: &bank,
            version: 41,
        },
        revision("node-set-union-final"),
        |request| {
            assert_eq!(request, SemanticAllocationRequest::Yank { step_index: 0 });
            Ok(SemanticAllocation::Yank {
                capture_revision: revision("node-set-union-capture"),
            })
        },
        |_, _| panic!("no Original register is used"),
    )
    .unwrap();
    assert_eq!(union.document, before);
    assert_eq!(union.context, requested);
    assert_eq!(union.register_writes.len(), 2);
    assert_eq!(
        union.register_writes[&register('a')],
        union.register_writes[&RegisterName::unnamed()]
    );
    let RegisterValue::EditedSet { set } = union.register_writes[&register('a')].as_ref() else {
        panic!("one complete union must occupy the register");
    };
    assert_eq!(set.duration(), frames(16));
    assert_eq!(set.components().len(), 4);
    for (index, (selected, parent, expected_range, _)) in canonical.iter().enumerate() {
        let direct = CapturedEditSlice::capture_selection(
            &before,
            &node(parent),
            &SliceCaptureSelection::Child {
                node: node(selected),
            },
            AudioTimingId {
                allocation: revision("node-set-union-capture"),
                ordinal: u32::try_from(index).unwrap(),
            },
        )
        .unwrap();
        assert_eq!(set.components()[index], direct);
        assert_eq!(union.trace[0].resolved_nodes[index].node, node(selected));
        assert_eq!(union.trace[0].resolved_nodes[index].parent, node(parent));
        assert_eq!(union.trace[0].resolved_ranges[index], *expected_range);
    }
    assert!(union.trace[0].resolved_range.is_none());
    assert!(union.trace[0].resolved_parent.is_none());
    assert!(union.trace[0].resolved_selection.is_none());
    set.validate_capture(&before).unwrap();
    assert_eq!(before.to_json().unwrap(), frozen_document);
    assert_eq!(requested, frozen_context);
    assert!(bank.is_empty());
}
