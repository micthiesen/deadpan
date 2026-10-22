//! Explicit node selector counterfactual. Synthetic structural metadata only.
//! Public controls precede the original final positive Node selector admission.

#[path = "explicit_node_selectors/positive.rs"]
mod positive;

use deadpan_core::*;
use serde_json::json;
use std::collections::BTreeMap;

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
fn source(label: &str, start: i64) -> BeatNode {
    let mut beat = hold(label, 8);
    beat.kind = NodeKind::Source {
        source: SourceNode {
            duration: frames(8),
            edit_window: None,
            video: SourceVideo::Stream {
                asset: AssetId::new("explicit-node-original").unwrap(),
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
fn edit(
    document: &ProjectDocument,
    name: &str,
    command: Command,
) -> (ProjectDocument, EditTransaction) {
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(name),
        command,
    };
    let encoded = serde_json::to_string(&request).unwrap();
    assert_eq!(
        serde_json::from_str::<CommandRequest>(&encoded).unwrap(),
        request
    );
    let transaction = apply(document, &request).unwrap();
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    (after, transaction)
}
fn insert(
    document: &ProjectDocument,
    label: &str,
    index: usize,
    root: &str,
    nodes: BTreeMap<NodeId, BeatNode>,
) -> ProjectDocument {
    edit(
        document,
        label,
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
    .0
}
fn fixture() -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("explicit-node-counterfactual").unwrap(),
        revision("explicit-node-initial"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let asset = edit(
        &empty,
        "explicit-node-asset",
        Command::AddAsset {
            id: AssetId::new("explicit-node-original").unwrap(),
            asset: AssetRecord {
                label: "Synthetic structural Original".into(),
                content_hash: "a".repeat(64),
                video: Some(span(0, 32)),
                audio: None,
                still_image: false,
                frame_count: Some(frames(32)),
                source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()),
            },
        },
    )
    .0;
    let lead = insert(
        &asset,
        "explicit-node-lead",
        0,
        "lead",
        BTreeMap::from([(node("lead"), hold("Lead", 3))]),
    );
    let group = insert(
        &lead,
        "explicit-node-group",
        1,
        "group",
        BTreeMap::from([
            (
                node("group"),
                BeatNode::sequence(
                    "Group",
                    vec![
                        node("empty-a"),
                        node("source-a"),
                        node("held"),
                        node("source-b"),
                        node("empty-b"),
                    ],
                ),
            ),
            (node("empty-a"), BeatNode::sequence("Empty A", vec![])),
            (node("source-a"), source("First source", 0)),
            (node("held"), hold("Held", 2)),
            (node("source-b"), source("Second source", 16)),
            (node("empty-b"), BeatNode::sequence("Empty B", vec![])),
        ]),
    );
    let tail = insert(
        &group,
        "explicit-node-tail",
        2,
        "tail",
        BTreeMap::from([(node("tail"), hold("Tail", 3))]),
    );
    edit(
        &tail,
        "explicit-node-mark",
        Command::SetMark {
            id: MarkId::new("source-b-owned-mark").unwrap(),
            owner: node("source-b"),
            label: "Owned source marker".into(),
            boundary: BoundaryAnchor {
                coordinate: Anchor::Local {
                    node: node("source-b"),
                    position: ExactRatio::integer(2),
                },
                bias: InsertionBias::Right,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
    )
    .0
}
fn context(parent: &str, cursor: i64, selected: &str) -> SemanticContext {
    SemanticContext {
        original: None,
        scoped: None,
        hold_duration: None,
        shared_hold_duration: None,
        mixed_hold_duration: None,
        selected_hold_duration: None,
        parent: node(parent),
        cursor: ProjectFrame(cursor),
        selected_child: Some(node(selected)),
        visual_selection: None,
    }
}
fn capture(
    document: &ProjectDocument,
    parent: &str,
    target: &str,
    capture_id: &str,
) -> CapturedEditSlice {
    let selection = SliceCaptureSelection::Child { node: node(target) };
    let captured = CapturedEditSlice::capture_selection(
        document,
        &node(parent),
        &selection,
        timing(capture_id),
    )
    .unwrap();
    captured.validate_capture(document).unwrap();
    assert_eq!(captured.selection(), &selection);
    assert_eq!(captured.attachments(), SliceAttachments::Owned);
    assert_eq!(captured.parent(), &node(parent));
    assert_eq!(
        CapturedEditSlice::from_json(&captured.to_json().unwrap()).unwrap(),
        captured
    );
    captured
}
fn captured(plan: &SemanticPlan) -> &CapturedEditSlice {
    let RegisterValue::Edited { slice } =
        plan.register_writes[&RegisterName::new('a').unwrap()].as_ref()
    else {
        panic!("edited capture expected");
    };
    slice
}
fn yank(
    document: &ProjectDocument,
    context: &SemanticContext,
    selector: SemanticSelector,
    capture_id: &str,
) -> SemanticPlan {
    let bank = BTreeMap::new();
    let mut allocations = Vec::new();
    let program = SemanticProgram::new(vec![SemanticInstruction::Yank {
        selector,
        register: RegisterName::new('a').unwrap(),
    }])
    .unwrap();
    let plan = plan_semantic(
        document,
        context,
        &program,
        SemanticRegisterBank {
            entries: &bank,
            version: 9,
        },
        revision(&format!("{capture_id}-final")),
        |request| {
            assert!(matches!(
                request,
                SemanticAllocationRequest::Yank { step_index: 0 }
            ));
            allocations.push(request);
            Ok(SemanticAllocation::Yank {
                capture_revision: revision(capture_id),
            })
        },
        |_, _| panic!("whole-node Edit capture never resolves Original registers"),
    )
    .unwrap();
    assert_eq!(
        allocations,
        vec![SemanticAllocationRequest::Yank { step_index: 0 }]
    );
    assert!(bank.is_empty());
    assert_eq!(plan.document, *document);
    assert_eq!(plan.context, *context);
    assert_eq!(plan.register_writes.len(), 2);
    assert_eq!(
        plan.register_writes[&RegisterName::new('a').unwrap()],
        plan.register_writes[&RegisterName::unnamed()],
    );
    let Command::Compound { transaction } = &plan.request.as_ref().unwrap().command else {
        panic!("bank-only Compound expected");
    };
    assert_eq!(transaction.expected_bank_version(), 9);
    assert_eq!(transaction.steps().len(), 1);
    assert!(matches!(transaction.steps()[0], ResolvedStep::Yank { .. }));
    assert!(transaction.steps()[0].edit().is_none());
    plan
}

#[test]
fn explicit_node_selector_admits_after_nested_child_and_empty_identity_controls() {
    let before = fixture();
    before.validate().unwrap();
    assert_eq!(before.duration().unwrap(), frames(24));
    let immutable_before = before.to_json().unwrap();
    let mut requested = context("root", 23, "tail");
    requested.visual_selection = Some(SemanticVisualSelection::Time {
        anchor: ProjectFrame(0),
        head: ProjectFrame(3),
        extending: false,
    });
    let immutable_requested = requested.clone();

    // Geometry and capture use the actual parent, independently of requested
    // Root navigation, its Tail selection and the unrelated Lead Visual range.
    let geometry = before
        .sequence_children(&node("group"), &node("source-b"), &node("source-b"))
        .unwrap();
    assert_eq!((geometry.first, geometry.end), (3, 4));
    assert_eq!(geometry.range, range(13, 21));
    let direct = capture(&before, "group", "source-b", "node-source-b-capture");
    assert_eq!(direct.range(), range(13, 21));
    assert_eq!(direct.duration(), frames(8));
    let payload = serde_json::to_value(&direct).unwrap();
    assert_eq!(
        payload["nodes"]["source-b"],
        serde_json::to_value(&before.nodes()[&node("source-b")]).unwrap()
    );
    assert_eq!(
        payload["parts"],
        json!([{
            "root":"source-b", "mapping":range(0,8),
            "source_start":ProjectFrame(13),
        }])
    );
    assert_eq!(
        payload["assets"],
        serde_json::to_value(before.assets()).unwrap()
    );
    assert_eq!(payload["marks"].as_object().unwrap().len(), 1);
    assert_eq!(
        payload["marks"]["source-b-owned-mark"],
        serde_json::to_value(&before.marks()[&MarkId::new("source-b-owned-mark").unwrap()])
            .unwrap()
    );

    // Existing SelectedBeat controls use their own valid Group context.
    // They do not claim that Root already offers explicit descendant selection.
    let group_context = context("group", 3, "source-b");
    let control = yank(
        &before,
        &group_context,
        SemanticSelector::SelectedBeat,
        "node-source-b-capture",
    );
    assert_eq!(captured(&control), &direct);
    assert_eq!(control.trace[0].resolved_parent, Some(node("group")));
    assert_eq!(control.trace[0].resolved_range, Some(range(13, 21)));
    assert_eq!(
        control.trace[0].resolved_selection,
        Some(SliceCaptureSelection::Child {
            node: node("source-b")
        })
    );

    // Existing Root selectors intentionally name different content.
    let tail = yank(
        &before,
        &requested,
        SemanticSelector::SelectedBeat,
        "node-tail-capture",
    );
    assert_eq!(
        captured(&tail).selection(),
        &SliceCaptureSelection::Child { node: node("tail") }
    );
    assert_eq!(captured(&tail).range(), range(21, 24));
    let lead = yank(
        &before,
        &requested,
        SemanticSelector::VisualSelection,
        "node-lead-capture",
    );
    assert_eq!(captured(&lead).range(), range(0, 3));
    assert_ne!(captured(&lead).selection(), direct.selection());

    // Public whole-child deletion has no split identity pool.
    let delete_revision = "node-raw-source-b-delete";
    let raw = Command::DeleteRipple {
        node: node("source-b"),
        timing: timing(delete_revision),
    };
    let (deleted, transaction) = edit(&before, delete_revision, raw);
    assert_eq!(transaction.duration_delta, -8);
    assert_eq!(deleted.duration().unwrap(), frames(16));
    assert!(!deleted.nodes().contains_key(&node("source-b")));
    assert_eq!(transaction.inverse.apply(&deleted).unwrap(), before);
    assert_eq!(
        before.nodes()[&node("source-a")],
        deleted.nodes()[&node("source-a")]
    );

    // An actual existing semantic Child Cut proves the zero-split allocator,
    // closed register payload, replay and one atomic inverse.
    let bank = BTreeMap::new();
    let mut allocations = Vec::new();
    let cut = plan_semantic(
        &before,
        &group_context,
        &SemanticProgram::new(vec![SemanticInstruction::Cut {
            selector: SemanticSelector::SelectedBeat,
            register: RegisterName::new('a').unwrap(),
        }])
        .unwrap(),
        SemanticRegisterBank {
            entries: &bank,
            version: 9,
        },
        revision("node-semantic-cut-final"),
        |request| {
            assert_eq!(
                request,
                SemanticAllocationRequest::Cut {
                    step_index: 0,
                    required_split_ids: 0,
                }
            );
            allocations.push(request);
            Ok(SemanticAllocation::Cut {
                new_revision: revision("node-semantic-cut-leaf"),
                capture_revision: revision("node-source-b-capture"),
                split_identities: SplitIdentities { nodes: vec![] },
            })
        },
        |_, _| panic!("no Original register"),
    )
    .unwrap();
    assert_eq!(allocations.len(), 1);
    assert_eq!(captured(&cut), &direct);
    assert_eq!(cut.document.duration().unwrap(), frames(16));
    assert_eq!(cut.context.parent, node("group"));
    assert_eq!(cut.context.cursor, ProjectFrame(13));
    let compound = cut.request.as_ref().unwrap();
    let Command::Compound { transaction } = &compound.command else {
        panic!("one Compound expected");
    };
    assert_eq!(transaction.steps().len(), 1);
    let ResolvedStep::Cut { delete, .. } = &transaction.steps()[0] else {
        panic!("one Cut leaf expected");
    };
    assert!(
        matches!(delete.command.as_command(), Command::DeleteRipple { node: selected, .. }
                     if selected == &node("source-b"))
    );
    let replayed = replay_compound::<EditError>(&before, compound, |_| Ok(())).unwrap();
    assert_eq!(replayed.document, cut.document);
    assert_eq!(replayed.register_writes, cut.register_writes);
    assert_eq!(replayed.edit.duration_delta, -8);
    assert_eq!(
        replayed.edit.inverse.apply(&replayed.document).unwrap(),
        before
    );

    // Zero time does not erase either empty child's authored identity.
    let empty_a = capture(&before, "group", "empty-a", "node-empty-a-capture");
    let empty_b = capture(&before, "group", "empty-b", "node-empty-b-capture");
    assert_eq!(empty_a.range(), range(3, 3));
    assert_eq!(empty_b.range(), range(21, 21));
    assert_eq!(empty_a.duration(), FrameDuration::ZERO);
    assert_eq!(empty_b.duration(), FrameDuration::ZERO);
    assert_ne!(empty_a.selection(), empty_b.selection());
    assert_ne!(empty_a, empty_b);
    for (name, expected, direct) in [
        ("empty-a", range(3, 3), &empty_a),
        ("empty-b", range(21, 21), &empty_b),
    ] {
        let capture_id = format!("node-{name}-capture");
        let equivalent = yank(
            &before,
            &context("group", 3, name),
            SemanticSelector::SelectedBeat,
            &capture_id,
        );
        assert_eq!(captured(&equivalent), direct);
        assert_eq!(equivalent.trace[0].resolved_range, Some(expected));
        let delete_id = format!("node-{name}-delete");
        let (after, transaction) = edit(
            &before,
            &delete_id,
            Command::DeleteRipple {
                node: node(name),
                timing: timing(&delete_id),
            },
        );
        assert_eq!(transaction.duration_delta, 0);
        assert_eq!(after.duration().unwrap(), frames(24));
        assert!(!after.nodes().contains_key(&node(name)));
        assert!(!transaction.forward.nodes.is_empty());
        assert_eq!(transaction.inverse.apply(&after).unwrap(), before);
        let other = if name == "empty-a" {
            "empty-b"
        } else {
            "empty-a"
        };
        assert_eq!(after.nodes()[&node(other)], before.nodes()[&node(other)]);
    }
    assert_eq!(before.to_json().unwrap(), immutable_before);
    assert_eq!(requested, immutable_requested);

    // No production Node variant is referenced by Rust, so this compiles before
    // implementation. The sole intended old-code runtime failure is admission.
    let mut wire = serde_json::to_value(
        SemanticProgram::new(vec![SemanticInstruction::Yank {
            selector: SemanticSelector::SelectedBeat,
            register: RegisterName::new('a').unwrap(),
        }])
        .unwrap(),
    )
    .unwrap();
    wire["instructions"][0]["selector"] = json!({"type":"node","node":"source-b"});
    assert_eq!(
        wire["instructions"][0]["selector"],
        json!({"type":"node","node":"source-b"})
    );
    let admitted = serde_json::from_value::<SemanticProgram>(wire);
    assert!(
        admitted.is_ok(),
        "Explicit Node selector admission must follow all public controls: {admitted:?}"
    );
    let admitted = admitted.unwrap();
    let plan = yank(
        &before,
        &requested,
        match &admitted.instructions()[0] {
            SemanticInstruction::Yank { selector, .. } => selector.clone(),
            _ => panic!("the admitted program is the exact Yank witness"),
        },
        "node-source-b-capture",
    );
    assert_eq!(captured(&plan), &direct);
    assert_eq!(plan.context, immutable_requested);
    assert_eq!(plan.trace[0].resolved_parent, Some(node("group")));
    assert_eq!(plan.trace[0].resolved_range, Some(range(13, 21)));
}
