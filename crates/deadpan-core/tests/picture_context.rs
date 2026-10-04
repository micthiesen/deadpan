use std::collections::BTreeMap;

use deadpan_core::*;
use serde_json::{Value, json};

fn id(s: &str) -> NodeId {
    NodeId::new(s).unwrap()
}
fn context() -> CapturedFraming {
    CapturedFraming::capture(
        None,
        CapturedCanvas {
            width: 1920,
            height: 1080,
            fit: CapturedFit::Fill,
            layers: vec![
                Some(FramingPose {
                    scale: ExactRatio::integer(2),
                    ..Default::default()
                }),
                None,
            ],
        },
    )
    .unwrap()
}
fn recipe() -> HoldRecipe {
    HoldRecipe {
        duration: FrameDuration::new(8).unwrap(),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
        picture_context: None,
    }
}
fn empty() -> ProjectDocument {
    ProjectDocument::new(
        ProjectId::new("picture_context").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("picture_context"),
    )
    .unwrap()
}
fn request(doc: &ProjectDocument, command: Command, revision: &str) -> CommandRequest {
    CommandRequest {
        project_id: doc.project_id().clone(),
        expected_revision: doc.revision_id().clone(),
        new_revision: RevisionId::new(revision).unwrap(),
        command,
    }
}
fn insertion() -> Command {
    Command::Insert {
        parent: id("picture_context"),
        index: 0,
        subtree: Subtree {
            root: id("hold"),
            nodes: BTreeMap::from([(id("hold"), BeatNode::hold("hold", recipe()))]),
            overrides: BTreeMap::new(),
            gap_overrides: BTreeMap::new(),
        },
    }
}
fn edit(doc: &ProjectDocument, command: Command, revision: &str) -> ProjectDocument {
    let tx = apply(doc, &request(doc, command, revision)).unwrap();
    let next = tx.forward.apply(doc).unwrap();
    assert_eq!(tx.inverse.apply(&next).unwrap(), *doc);
    assert_eq!(
        ProjectDocument::from_json(&next.to_json().unwrap()).unwrap(),
        next
    );
    next
}
fn hold(doc: &ProjectDocument, name: &str) -> HoldRecipe {
    let NodeKind::Hold { recipe } = &doc.nodes()[&id(name)].kind else {
        panic!("Hold expected")
    };
    recipe.clone()
}

#[test]
fn context_set_clear_undo_and_occurrence_isolation_preserve_live_framing_and_audio() {
    let doc = edit(&empty(), insertion(), "insert");
    let doc = edit(
        &doc,
        Command::SetFraming {
            node: id("hold"),
            framing: Some(Framing::static_pose(FramingPose::identity()).unwrap()),
        },
        "camera",
    );
    let captured = context();
    let set = edit(
        &doc,
        Command::SetHoldPictureContext {
            node: id("hold"),
            context: Some(captured.clone()),
        },
        "capture",
    );
    assert_eq!(hold(&set, "hold").picture_context, Some(captured.clone()));
    assert_eq!(
        set.nodes()[&id("hold")].framing,
        doc.nodes()[&id("hold")].framing
    );
    assert_eq!(set.audio_bindings(), doc.audio_bindings());
    let cleared = edit(
        &set,
        Command::SetHoldPictureContext {
            node: id("hold"),
            context: None,
        },
        "clear",
    );
    assert!(hold(&cleared, "hold").picture_context.is_none());
    assert!(
        !serde_json::to_value(hold(&cleared, "hold"))
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("picture_context")
    );
    let repeated = edit(
        &set,
        Command::WrapRepeat {
            node: id("hold"),
            id: id("repeat"),
            plays: 3,
            gap: None,
            anchor_policy: WrapAnchorPolicy::default(),
        },
        "repeat",
    );
    let NodeKind::Repeat { iterations, .. } = &repeated.nodes()[&id("repeat")].kind else {
        panic!("Repeat expected")
    };
    let play = iterations.at(1).unwrap();
    let isolated = edit(
        &repeated,
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("hold"),
                repeats: vec![RepeatInstance {
                    node: id("repeat"),
                    iteration: play,
                }],
            },
            edit: OccurrenceEdit::SetHoldPictureContext { context: None },
            identities: OccurrenceIdentities {
                nodes: vec![id("isolated")],
                marks: vec![],
            },
        },
        "isolate",
    );
    assert_eq!(hold(&isolated, "hold").picture_context, Some(captured));
    assert!(hold(&isolated, "isolated").picture_context.is_none());
}

#[test]
fn typed_and_serialized_context_bounds_reject_invalid_canvas_pose_and_totals() {
    let mut value = context();
    value.canvases[0].width = 3;
    assert!(value.validate().is_err());
    assert!(
        serde_json::from_value::<CapturedFraming>(serde_json::to_value(&value).unwrap()).is_err()
    );
    value = context();
    value.canvases[0].layers = vec![Some(FramingPose::identity()); MAX_CAPTURED_POSES + 1];
    assert!(value.validate().is_err());
    assert!(
        serde_json::from_value::<CapturedFraming>(serde_json::to_value(&value).unwrap()).is_err()
    );
    let doc = edit(&empty(), insertion(), "insert");
    assert!(
        apply(
            &doc,
            &request(
                &doc,
                Command::SetHoldPictureContext {
                    node: id("hold"),
                    context: Some(value)
                },
                "invalid"
            )
        )
        .is_err()
    );
    let stages = vec![
        CapturedCanvas {
            width: 16,
            height: 16,
            fit: CapturedFit::Fit,
            layers: vec![None; 257]
        };
        2
    ];
    assert!(CapturedFraming::new(stages.clone()).is_err());
    assert!(serde_json::from_value::<CapturedFraming>(json!({"canvases": stages})).is_err());
}

#[test]
fn unchanged_recapture_is_exactly_constant_after_pose_or_implicit_fill_clip() {
    let first = context();
    let mut current = first.clone();
    for _ in 0..1000 {
        current = CapturedFraming::capture(
            Some(&current),
            CapturedCanvas {
                width: 1920,
                height: 1080,
                fit: CapturedFit::Fit,
                layers: vec![None],
            },
        )
        .unwrap();
        assert_eq!(current, first);
    }
    let first = CapturedFraming::capture(
        None,
        CapturedCanvas {
            width: 1920,
            height: 1080,
            fit: CapturedFit::Fill,
            layers: vec![],
        },
    )
    .unwrap();
    let current = CapturedFraming::capture(
        Some(&first),
        CapturedCanvas {
            width: 1920,
            height: 1080,
            fit: CapturedFit::Fit,
            layers: vec![Some(FramingPose::identity())],
        },
    )
    .unwrap();
    assert_eq!(
        current.canvases[0].layers,
        vec![None, Some(FramingPose::identity())]
    );
    assert_eq!(current.canvases[0].fit, CapturedFit::Fill);
}

#[test]
fn document_budget_counts_hold_and_gap_contexts_at_the_exact_boundary() {
    let admitted = document_at_context_limit();
    admitted.validate().unwrap();
    let mut wire = serde_json::to_value(&admitted).unwrap();
    wire["nodes"]["repeat"]["kind"]["gap"]["picture_context"]["canvases"][0]["layers"]
        .as_array_mut()
        .unwrap()
        .push(Value::Null);
    for json in [
        wire.to_string(),
        wire.to_string()
            .replace("picture_context", "picture_con\\u0074ext"),
    ] {
        assert!(
            ProjectDocument::from_json(&json)
                .unwrap_err()
                .to_string()
                .contains("aggregate captured framing record limit")
        );
    }
}

fn document_at_context_limit() -> ProjectDocument {
    let initial = empty();
    let mut nodes = BTreeMap::new();
    let mut children = Vec::new();
    let full = CapturedFraming::new(vec![CapturedCanvas {
        width: 16,
        height: 16,
        fit: CapturedFit::Fit,
        layers: vec![None; MAX_CAPTURED_SCOPES],
    }])
    .unwrap();
    let full_count = full.record_count().unwrap();
    let count = MAX_CAPTURED_FRAMING_RECORDS / full_count;
    for i in 0..count {
        let name = id(&format!("hold-{i}"));
        let mut hold = recipe();
        hold.picture_context = Some(full.clone());
        nodes.insert(name.clone(), BeatNode::hold("Retained view", hold));
        children.push(name);
    }
    let remainder = MAX_CAPTURED_FRAMING_RECORDS - count * full_count;
    assert!(remainder >= 2 && remainder - 2 < MAX_CAPTURED_SCOPES);
    let mut gap = recipe();
    gap.picture_context = Some(
        CapturedFraming::new(vec![CapturedCanvas {
            width: 16,
            height: 16,
            fit: CapturedFit::Fit,
            layers: vec![None; remainder - 2],
        }])
        .unwrap(),
    );
    let mut repeat = BeatNode::sequence("Repeat", vec![]);
    repeat.kind = NodeKind::Repeat {
        child: id("repeat-child"),
        iterations: IterationOrder::new(initial.revision_id().clone(), 2).unwrap(),
        gap: Some(gap),
        escalation: None,
    };
    nodes.insert(id("repeat-child"), BeatNode::hold("Child", recipe()));
    nodes.insert(id("repeat"), repeat);
    children.push(id("repeat"));
    nodes.insert(initial.root().clone(), BeatNode::sequence("Root", children));
    let mut wire = serde_json::to_value(initial).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn every_typed_payload_checks_context_before_resolving_its_target() {
    let doc = empty();
    let mut oversized = context();
    oversized.canvases[0].layers = vec![None; MAX_CAPTURED_SCOPES + 1];
    let mut hold = recipe();
    hold.picture_context = Some(oversized.clone());
    let subtree = Subtree {
        root: id("incoming"),
        nodes: BTreeMap::from([(id("incoming"), BeatNode::hold("incoming", hold.clone()))]),
        overrides: BTreeMap::new(),
        gap_overrides: BTreeMap::new(),
    };
    let play = IterationOrder::new(doc.revision_id().clone(), 2)
        .unwrap()
        .at(0)
        .unwrap();
    let mut commands = vec![
        Command::InsertTime {
            at: ProjectFrame(-1),
            hold: hold.clone(),
            id: id("pause"),
            identities: SplitIdentities::default(),
            timing: AudioTimingId {
                allocation: RevisionId::new("next").unwrap(),
                ordinal: 0,
            },
        },
        Command::Insert {
            parent: id("missing"),
            index: 0,
            subtree: subtree.clone(),
        },
        Command::SetPlayOverride {
            node: id("missing"),
            iteration: play.clone(),
            subtree: subtree.clone(),
        },
        Command::WrapRepeat {
            node: id("missing"),
            id: id("repeat"),
            plays: 2,
            gap: Some(hold.clone()),
            anchor_policy: WrapAnchorPolicy::default(),
        },
        Command::SetRepeat {
            node: id("missing"),
            plays: 2,
            gap: Some(hold.clone()),
        },
        Command::SetHoldPictureContext {
            node: id("missing"),
            context: Some(oversized.clone()),
        },
    ];
    for edit in [
        OccurrenceEdit::Insert {
            index: 0,
            subtree: subtree.clone(),
        },
        OccurrenceEdit::SetPlayOverride {
            iteration: play,
            subtree,
        },
        OccurrenceEdit::WrapRepeat {
            id: id("repeat"),
            plays: 2,
            gap: Some(hold.clone()),
            anchor_policy: WrapAnchorPolicy::default(),
        },
        OccurrenceEdit::SetRepeat {
            plays: 2,
            gap: Some(hold),
        },
        OccurrenceEdit::SetHoldPictureContext {
            context: Some(oversized),
        },
    ] {
        commands.push(Command::EditOccurrence {
            instance: InstancePath {
                node: id("missing"),
                repeats: vec![],
            },
            edit,
            identities: OccurrenceIdentities::default(),
        });
    }
    for command in commands {
        let error = apply(&doc, &request(&doc, command, "next")).unwrap_err();
        assert!(error.to_string().contains("scope limit"), "{error}");
    }
}

#[test]
fn aggregate_preflight_covers_subtrees_patches_and_repeated_isolation() {
    let full = document_at_context_limit();
    let mut incoming = full.nodes().clone();
    incoming.insert(
        id("extra"),
        BeatNode::hold(
            "extra",
            HoldRecipe {
                picture_context: Some(context()),
                ..recipe()
            },
        ),
    );
    let base = empty();
    let error = apply(
        &base,
        &request(
            &base,
            Command::Insert {
                parent: id("missing"),
                index: 0,
                subtree: Subtree {
                    root: full.root().clone(),
                    nodes: incoming,
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
            "oversized",
        ),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("aggregate captured framing record limit")
    );

    // Replacing a context at the exact limit must not charge the retired
    // before-value twice. Its forward and inverse patches both remain legal.
    let captured = hold(&full, "hold-0").picture_context;
    let mut changed = captured.clone().unwrap();
    changed.canvases[0].fit = CapturedFit::Fill;
    let tx = apply(
        &full,
        &request(
            &full,
            Command::SetHoldPictureContext {
                node: id("hold-0"),
                context: Some(changed),
            },
            "replacement",
        ),
    )
    .unwrap();
    let replaced = tx.forward.apply(&full).unwrap();
    assert_eq!(tx.inverse.apply(&replaced).unwrap(), full);

    let mut oversized_patch = tx.forward.clone();
    let after = oversized_patch
        .nodes
        .get_mut(&id("hold-0"))
        .unwrap()
        .after
        .as_mut()
        .unwrap();
    let NodeKind::Hold { recipe } = &mut after.kind else {
        panic!("hold");
    };
    recipe
        .picture_context
        .as_mut()
        .unwrap()
        .canvases
        .push(CapturedCanvas {
            width: 16,
            height: 16,
            fit: CapturedFit::Fit,
            layers: vec![],
        });
    assert!(
        oversized_patch
            .apply(&full)
            .unwrap_err()
            .to_string()
            .contains("aggregate captured framing record limit")
    );

    // Before-values are untrusted too, even when they would later conflict.
    let mut invalid_before = tx.forward;
    let before = invalid_before
        .nodes
        .get_mut(&id("hold-0"))
        .unwrap()
        .before
        .as_mut()
        .unwrap();
    let NodeKind::Hold { recipe } = &mut before.kind else {
        panic!("hold");
    };
    recipe.picture_context.as_mut().unwrap().canvases[0]
        .layers
        .push(None);
    assert!(
        invalid_before
            .apply(&full)
            .unwrap_err()
            .to_string()
            .contains("scope limit")
    );

    let wrapped = edit(
        &full,
        Command::WrapRepeat {
            node: id("hold-0"),
            id: id("isolation-repeat"),
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::default(),
        },
        "wrap",
    );
    let NodeKind::Repeat { iterations, .. } = &wrapped.nodes()[&id("isolation-repeat")].kind else {
        panic!("repeat");
    };
    let error = apply(
        &wrapped,
        &request(
            &wrapped,
            Command::EditOccurrence {
                instance: InstancePath {
                    node: id("hold-0"),
                    repeats: vec![RepeatInstance {
                        node: id("isolation-repeat"),
                        iteration: iterations.at(0).unwrap(),
                    }],
                },
                edit: OccurrenceEdit::Rename {
                    label: "isolated".into(),
                },
                identities: OccurrenceIdentities {
                    nodes: vec![id("copy")],
                    marks: vec![],
                },
            },
            "isolate",
        ),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("aggregate captured framing record limit")
    );
    assert_eq!(hold(&wrapped, "hold-0").picture_context, captured);
    // Isolation alone temporarily exceeds the document budget, but clearing
    // the copied context returns to the exact limit and must remain editable.
    let cleared = edit(
        &wrapped,
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("hold-0"),
                repeats: vec![RepeatInstance {
                    node: id("isolation-repeat"),
                    iteration: iterations.at(0).unwrap(),
                }],
            },
            edit: OccurrenceEdit::SetHoldPictureContext { context: None },
            identities: OccurrenceIdentities {
                nodes: vec![id("cleared-copy")],
                marks: vec![],
            },
        },
        "clear-at-limit",
    );
    assert!(hold(&cleared, "cleared-copy").picture_context.is_none());
    assert_eq!(hold(&cleared, "hold-0").picture_context, captured);
}
