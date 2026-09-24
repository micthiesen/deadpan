use std::collections::BTreeMap;

use deadpan_core::*;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn mark_id(value: &str) -> MarkId {
    MarkId::new(value).unwrap()
}
fn local(host: &str, position: i64) -> Anchor {
    Anchor::Local {
        node: node(host),
        position: ExactRatio::integer(position),
    }
}
fn hold(frames: i64) -> BeatNode {
    BeatNode::hold(
        "Pause",
        HoldRecipe {
            duration: FrameDuration::new(frames).unwrap(),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}
fn binding(owner: &str, coordinate: Anchor) -> MarkFragment {
    MarkFragment {
        owner: node(owner),
        coordinate,
        state: MarkState::Bound,
    }
}
fn logical(bindings: Vec<MarkFragment>, loss_policy: AnchorLossPolicy) -> Mark {
    let mut bindings = bindings.into_iter();
    let primary = bindings.next().unwrap();
    Mark {
        owner: primary.owner,
        label: "One logical mark".into(),
        boundary: BoundaryAnchor {
            coordinate: primary.coordinate,
            bias: InsertionBias::Right,
        },
        state: primary.state,
        loss_policy,
        fragments: bindings.collect(),
    }
}
fn fixture() -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("fragments").unwrap(),
        RevisionId::new("r").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(empty).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("group")]),
        ),
        (
            node("group"),
            BeatNode::sequence("Group", vec![node("a"), node("b"), node("c")]),
        ),
        (node("a"), hold(4)),
        (node("b"), hold(5)),
        (node("c"), hold(6)),
    ]))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn with_marks(
    document: &ProjectDocument,
    marks: BTreeMap<MarkId, Mark>,
) -> Result<ProjectDocument, DocumentError> {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["marks"] = serde_json::to_value(marks).unwrap();
    ProjectDocument::from_json(&wire.to_string())
}
fn with_mark(document: &ProjectDocument, mark: Mark) -> ProjectDocument {
    with_marks(document, BTreeMap::from([(mark_id("logical"), mark)])).unwrap()
}
fn request(document: &ProjectDocument, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(format!("{}x", document.revision_id())).unwrap(),
        command,
    }
}
fn edit(document: &ProjectDocument, command: Command) -> ProjectDocument {
    let transaction = apply(document, &request(document, command)).unwrap();
    let encoded = serde_json::to_string(&transaction).unwrap();
    let decoded: EditTransaction = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, transaction);
    let after = decoded.forward.apply(document).unwrap();
    assert_eq!(decoded.inverse.apply(&after).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    after
}
fn stored(document: &ProjectDocument) -> &Mark {
    &document.marks()[&mark_id("logical")]
}

#[test]
fn deleting_only_one_owner_promotes_the_next_binding_and_last_loss_removes_the_mark() {
    let document = with_mark(
        &fixture(),
        logical(
            vec![
                binding("a", local("b", 1)),
                binding("b", local("c", 2)),
                binding(
                    "root",
                    Anchor::Sequence {
                        frame: ProjectFrame(2),
                    },
                ),
            ],
            AnchorLossPolicy::DeleteOwned,
        ),
    );
    let after = edit(&document, Command::Delete { node: node("a") });
    assert_eq!(stored(&after).owner, node("b"));
    assert_eq!(stored(&after).boundary.coordinate, local("c", 2));
    assert_eq!(stored(&after).binding_count(), 2);
    assert_eq!(stored(&after).label, stored(&document).label);
    assert_eq!(
        stored(&after).boundary.bias,
        stored(&document).boundary.bias
    );
    let after = edit(&after, Command::Delete { node: node("c") });
    assert_eq!(stored(&after).owner, node("root"));
    assert_eq!(stored(&after).binding_count(), 1);
    let after = edit(&after, Command::Delete { node: node("b") });
    assert!(!after.marks().contains_key(&mark_id("logical")));
}

#[test]
fn unresolved_binding_retains_its_last_identity_without_hiding_bound_siblings_or_rebinding() {
    let document = with_mark(
        &fixture(),
        logical(
            vec![binding("a", local("b", 1)), binding("root", local("c", 2))],
            AnchorLossPolicy::KeepUnresolved,
        ),
    );
    let after = edit(&document, Command::Delete { node: node("a") });
    assert_eq!(
        stored(&after).state,
        MarkState::Unresolved {
            reason: MarkLossReason::OwnerMissing
        }
    );
    assert_eq!(stored(&after).boundary, stored(&document).boundary);
    assert_eq!(stored(&after).fragments[0], binding("root", local("c", 2)));
    let reused = edit(
        &after,
        Command::Insert {
            parent: node("group"),
            index: 0,
            subtree: Subtree {
                root: node("a"),
                nodes: BTreeMap::from([(node("a"), hold(4))]),
                overrides: BTreeMap::new(),
            },
        },
    );
    assert_eq!(stored(&reused), stored(&after));
    let rebound = edit(
        &reused,
        Command::SetMark {
            id: mark_id("logical"),
            owner: node("root"),
            label: "Explicit replacement".into(),
            boundary: BoundaryAnchor {
                coordinate: local("a", 2),
                bias: InsertionBias::Left,
            },
            loss_policy: AnchorLossPolicy::KeepUnresolved,
        },
    );
    assert_eq!(stored(&rebound).state, MarkState::Bound);
    assert!(stored(&rebound).fragments.is_empty());
    assert_eq!(stored(&rebound).boundary.coordinate, local("a", 2));
}

#[test]
fn each_binding_uses_the_shared_bias_and_retains_exact_local_content_coordinates() {
    for bias in [InsertionBias::Left, InsertionBias::Right] {
        let mut mark = logical(
            vec![
                binding("root", local("group", 4)),
                binding(
                    "a",
                    Anchor::Local {
                        node: node("group"),
                        position: ExactRatio::new(9, 2).unwrap(),
                    },
                ),
                binding("b", local("c", 1)),
            ],
            AnchorLossPolicy::KeepUnresolved,
        );
        mark.boundary.bias = bias;
        let document = with_mark(&fixture(), mark);
        let after = edit(
            &document,
            Command::Insert {
                parent: node("group"),
                index: 1,
                subtree: Subtree {
                    root: node("inserted"),
                    nodes: BTreeMap::from([(node("inserted"), hold(2))]),
                    overrides: BTreeMap::new(),
                },
            },
        );
        let bindings: Vec<_> = stored(&after).bindings().collect();
        assert_eq!(
            bindings[0].coordinate,
            local("group", if bias == InsertionBias::Left { 4 } else { 6 })
        );
        assert_eq!(
            bindings[1].coordinate,
            Anchor::Local {
                node: node("group"),
                position: ExactRatio::new(13, 2).unwrap()
            }
        );
        assert_eq!(bindings[2].coordinate, local("c", 1));
        assert!(
            bindings
                .iter()
                .all(|binding| binding.state == MarkState::Bound)
        );
    }
}

#[test]
fn moving_an_owner_does_not_retarget_a_binding_hosted_elsewhere() {
    let document = with_mark(
        &fixture(),
        logical(
            vec![binding("a", local("b", 1)), binding("b", local("a", 1))],
            AnchorLossPolicy::KeepUnresolved,
        ),
    );
    let moved = edit(
        &document,
        Command::Move {
            node: node("a"),
            parent: node("root"),
            index: 1,
        },
    );
    assert_eq!(stored(&moved), stored(&document));
    let deleted = edit(&moved, Command::Delete { node: node("b") });
    let bindings: Vec<_> = stored(&deleted).bindings().collect();
    assert_eq!(
        bindings[0].state,
        MarkState::Unresolved {
            reason: MarkLossReason::HostMissing
        }
    );
    assert_eq!(
        bindings[1].state,
        MarkState::Unresolved {
            reason: MarkLossReason::OwnerMissing
        }
    );
}

#[test]
fn retained_copy_ownership_depends_on_authored_nodes_not_visible_matching_plays() {
    let repeated = |child| BeatNode {
        framing: None,
        label: "Repeated pair".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: node(child),
            iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 2).unwrap(),
            gap: None,
        },
    };
    let partition = |child, start, end| BeatNode {
        framing: None,
        label: "Retained partition".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: node(child),
            duration: FrameDuration::new(end - start).unwrap(),
            mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            pitch: PitchPolicy::Preserve,
            purpose: RetimePurpose::Partition,
        },
    };
    let mut wire = serde_json::to_value(fixture()).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("left"), node("right")]),
        ),
        (node("left"), partition("repeat-left", 0, 4)),
        (node("right"), partition("repeat-right", 4, 18)),
        (node("repeat-left"), repeated("pair-left")),
        (node("repeat-right"), repeated("pair-right")),
        (
            node("pair-left"),
            BeatNode::sequence("Pair", vec![node("a-left"), node("b-left")]),
        ),
        (
            node("pair-right"),
            BeatNode::sequence("Pair", vec![node("a-right"), node("b-right")]),
        ),
        (node("a-left"), hold(4)),
        (node("b-left"), hold(5)),
        (node("a-right"), hold(4)),
        (node("b-right"), hold(5)),
    ]))
    .unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let document = with_mark(
        &document,
        logical(
            vec![
                binding("a-left", local("b-left", 1)),
                binding("a-right", local("b-right", 1)),
            ],
            AnchorLossPolicy::DeleteOwned,
        ),
    );
    // Left retains no visible B, and the right's first A is hidden. Both are
    // legitimate authored-node bindings; no play-visibility correspondence exists.
    document.validate().unwrap();
    let after = edit(&document, Command::Delete { node: node("left") });
    assert_eq!(
        stored(&after).bindings().collect::<Vec<_>>(),
        [binding("a-right", local("b-right", 1))]
    );
    let after = edit(
        &after,
        Command::SetRepeat {
            node: node("repeat-right"),
            plays: 3,
            gap: None,
        },
    );
    assert_eq!(
        stored(&after).bindings().collect::<Vec<_>>(),
        [binding("a-right", local("b-right", 1))]
    );
}

fn repeated_fixture() -> ProjectDocument {
    let document = edit(
        &fixture(),
        Command::Move {
            node: node("c"),
            parent: node("root"),
            index: 1,
        },
    );
    edit(
        &document,
        Command::WrapRepeat {
            node: node("group"),
            id: node("repeat"),
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        },
    )
}
fn selected(document: &ProjectDocument) -> InstancePath {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&node("repeat")].kind else {
        unreachable!()
    };
    InstancePath {
        node: node("a"),
        repeats: vec![RepeatInstance {
            node: node("repeat"),
            iteration: iterations.at(1).unwrap(),
        }],
    }
}
fn isolation(document: &ProjectDocument, mark_ids: Vec<MarkId>) -> Command {
    Command::EditOccurrence {
        instance: selected(document),
        edit: OccurrenceEdit::Rename {
            label: "Isolated A".into(),
        },
        identities: OccurrenceIdentities {
            nodes: vec![node("copied-group"), node("copied-a"), node("copied-b")],
            marks: mark_ids,
        },
    }
}

#[test]
fn occurrence_copy_uses_one_id_for_only_owned_bindings_and_relocates_concrete_bindings_once() {
    let base = repeated_fixture();
    let mut path = selected(&base);
    path.node = node("b");
    let unresolved = MarkFragment {
        owner: node("a"),
        coordinate: local("missing", 1),
        state: MarkState::Unresolved {
            reason: MarkLossReason::HostMissing,
        },
    };
    let mark = logical(
        vec![
            binding("a", local("b", 1)),
            binding("b", local("c", 1)),
            binding("root", local("a", 1)),
            binding(
                "a",
                Anchor::Sequence {
                    frame: ProjectFrame(20),
                },
            ),
            binding(
                "a",
                Anchor::Occurrence {
                    instance: path.clone(),
                    position: ExactRatio::ONE,
                },
            ),
            unresolved.clone(),
        ],
        AnchorLossPolicy::KeepUnresolved,
    );
    let document = with_mark(&base, mark);
    let before = document.to_json().unwrap();
    assert!(apply(&document, &request(&document, isolation(&document, vec![]))).is_err());
    assert_eq!(document.to_json().unwrap(), before);
    let after = edit(&document, isolation(&document, vec![mark_id("copy")]));
    assert_eq!(after.marks().len(), 2);
    assert_eq!(stored(&after).binding_count(), 6);
    let original: Vec<_> = stored(&after).bindings().collect();
    assert_eq!(original[0], binding("a", local("b", 1)));
    assert_eq!(original[2], binding("root", local("a", 1)));
    assert_eq!(
        original[3],
        binding(
            "a",
            Anchor::Sequence {
                frame: ProjectFrame(20)
            }
        )
    );
    path.node = node("copied-b");
    assert_eq!(
        original[4],
        binding(
            "copied-a",
            Anchor::Occurrence {
                instance: path,
                position: ExactRatio::ONE
            }
        )
    );
    assert_eq!(original[5], unresolved.clone());
    let copied: Vec<_> = after.marks()[&mark_id("copy")].bindings().collect();
    assert_eq!(
        copied,
        vec![
            binding("copied-a", local("copied-b", 1)),
            binding("copied-b", local("c", 1)),
            MarkFragment {
                owner: node("copied-a"),
                ..unresolved
            },
        ]
    );
    assert_eq!(
        after.marks()[&mark_id("copy")].boundary.bias,
        stored(&document).boundary.bias
    );
    // Reusing the already isolated occurrence does not allocate another mark.
    let command = Command::EditOccurrence {
        instance: InstancePath {
            node: node("copied-a"),
            repeats: selected(&document).repeats,
        },
        edit: OccurrenceEdit::Rename {
            label: "Renamed again".into(),
        },
        identities: OccurrenceIdentities {
            nodes: vec![],
            marks: vec![],
        },
    };
    let again = edit(&after, command);
    assert_eq!(again.marks(), after.marks());
}

#[test]
fn every_fragment_is_validated_even_when_the_primary_is_unresolved() {
    let primary = MarkFragment {
        owner: node("missing"),
        coordinate: local("missing", 1),
        state: MarkState::Unresolved {
            reason: MarkLossReason::OwnerMissing,
        },
    };
    let good = logical(
        vec![primary.clone(), binding("root", local("a", 1))],
        AnchorLossPolicy::KeepUnresolved,
    );
    with_mark(&fixture(), good.clone()).validate().unwrap();
    for fragment in [
        binding("missing", local("a", 1)),
        binding("root", local("missing", 1)),
        binding("root", local("a", 5)),
        MarkFragment {
            owner: node("root"),
            coordinate: local("missing", -1),
            state: MarkState::Unresolved {
                reason: MarkLossReason::HostMissing,
            },
        },
    ] {
        let invalid = logical(
            vec![primary.clone(), fragment],
            AnchorLossPolicy::KeepUnresolved,
        );
        assert!(with_marks(&fixture(), BTreeMap::from([(mark_id("logical"), invalid)])).is_err());
    }
    let mut invalid = good.clone();
    invalid.loss_policy = AnchorLossPolicy::DeleteOwned;
    assert!(with_marks(&fixture(), BTreeMap::from([(mark_id("logical"), invalid)])).is_err());
    let mut wire = serde_json::to_value(good).unwrap();
    wire["fragments"][0]["bias"] = serde_json::json!("left");
    assert!(serde_json::from_value::<Mark>(wire).is_err());
    let old = logical(
        vec![binding("root", local("a", 1))],
        AnchorLossPolicy::KeepUnresolved,
    );
    assert!(!serde_json::to_string(&old).unwrap().contains("fragments"));
}

#[test]
fn owned_source_bindings_copy_without_inventing_a_timeline_host() {
    let clock = SourceTimeBase::new(1, 48_000).unwrap();
    let asset = AssetId::new("original").unwrap();
    let base = edit(
        &repeated_fixture(),
        Command::AddAsset {
            id: asset.clone(),
            asset: AssetRecord {
                label: "Original audio".into(),
                content_hash: "a".repeat(64),
                video: None,
                audio: Some(
                    SourceSpan::new(
                        SourceTimestamp {
                            ticks: 0,
                            time_base: clock,
                        },
                        SourceTimestamp {
                            ticks: 100,
                            time_base: clock,
                        },
                    )
                    .unwrap(),
                ),
                still_image: false,
                frame_count: None,
                source_qualification: None,
            },
        },
    );
    let coordinate = |sample| Anchor::Source {
        asset: asset.clone(),
        moment: SourceMoment::AudioSample {
            sample,
            sample_rate: 48_000,
        },
    };
    let document = with_mark(
        &base,
        logical(
            vec![
                binding("a", coordinate(5)),
                binding("root", coordinate(9)),
                binding("b", coordinate(10)),
            ],
            AnchorLossPolicy::KeepUnresolved,
        ),
    );
    let after = edit(&document, isolation(&document, vec![mark_id("copy")]));
    assert_eq!(stored(&after), stored(&document));
    assert_eq!(
        after.marks()[&mark_id("copy")]
            .bindings()
            .collect::<Vec<_>>(),
        [
            binding("copied-a", coordinate(5)),
            binding("copied-b", coordinate(10)),
        ]
    );
    // The only authored content is Hold/Sequence/Repeat, yet original-clock
    // bindings stay valid independently of timeline source usage.
    assert!(
        after
            .nodes()
            .values()
            .all(|node| !matches!(node.kind, NodeKind::Source { .. }))
    );
}

#[test]
fn per_mark_and_document_limits_include_every_primary_and_fragment_before_copying() {
    let document = repeated_fixture();
    let group = |count: usize| {
        logical(
            (0..count)
                .map(|index| {
                    binding(
                        "root",
                        Anchor::Local {
                            node: node("root"),
                            position: ExactRatio::new(index as i128, 1000).unwrap(),
                        },
                    )
                })
                .collect(),
            AnchorLossPolicy::KeepUnresolved,
        )
    };
    let maximum = group(MAX_MARK_BINDINGS);
    with_mark(&document, maximum.clone()).validate().unwrap();
    let invalid = group(MAX_MARK_BINDINGS + 1);
    assert_eq!(
        with_marks(&document, BTreeMap::from([(mark_id("logical"), invalid)]))
            .unwrap_err()
            .code,
        DocumentErrorCode::LimitExceeded
    );
    let mut marks = BTreeMap::new();
    let mut remaining = MAX_DOCUMENT_MARK_BINDINGS;
    let mut count = 0;
    while remaining > 0 {
        let size = remaining.min(MAX_MARK_BINDINGS);
        marks.insert(mark_id(&format!("limit-{count}")), group(size));
        remaining -= size;
        count += 1;
    }
    // One owned binding would require one new copy, beyond the shared limit.
    marks.get_mut(&mark_id("limit-0")).unwrap().owner = node("a");
    let maximum = with_marks(&document, marks.clone()).unwrap();
    let before = maximum.to_json().unwrap();
    let error = apply(
        &maximum,
        &request(&maximum, isolation(&maximum, vec![mark_id("copy")])),
    )
    .unwrap_err();
    assert!(error.to_string().contains("mark binding limit"));
    assert_eq!(maximum.to_json().unwrap(), before);
    marks.insert(mark_id("over-limit"), group(1));
    assert_eq!(
        with_marks(&document, marks).unwrap_err().code,
        DocumentErrorCode::LimitExceeded
    );
}
