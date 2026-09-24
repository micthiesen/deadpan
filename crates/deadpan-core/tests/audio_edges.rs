use std::collections::BTreeMap;

use deadpan_core::*;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn empty() -> ProjectDocument {
    ProjectDocument::new_automatic(
        ProjectId::new("edge-project").unwrap(),
        RevisionId::new("initial").unwrap(),
        id("root"),
    )
    .unwrap()
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
    let request = request(document, command);
    let request = serde_json::from_str(&serde_json::to_string(&request).unwrap()).unwrap();
    let transaction = apply(document, &request).unwrap();
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    after
}

#[test]
fn policy_edits_preserve_time_basis_marks_and_unselected_edges() {
    let initial = empty();
    let after = edit(
        &initial,
        Command::SetAudioEdge {
            node: id("root"),
            edge: AudioBoundaryKind::NodeStart,
            policy: AudioEdgePolicy::Hard,
        },
    );
    assert_eq!(after.duration().unwrap(), FrameDuration::ZERO);
    assert_eq!(after.presentation_basis(), initial.presentation_basis());
    assert_eq!(after.basis_state(), initial.basis_state());
    assert_eq!(
        after.basis_state().rate_origin,
        FrameRateOrigin::Provisional
    );
    assert_eq!(after.marks(), initial.marks());
    assert_eq!(after.overrides(), initial.overrides());
    assert_eq!(
        after.nodes()[&id("root")].audio_edges,
        AudioEdgePolicies {
            node_start: AudioEdgePolicy::Hard,
            ..Default::default()
        }
    );
    let restored = edit(
        &after,
        Command::SetAudioEdge {
            node: id("root"),
            edge: AudioBoundaryKind::NodeStart,
            policy: AudioEdgePolicy::Automatic,
        },
    );
    assert_eq!(restored.nodes(), initial.nodes());
    assert_eq!(
        apply(
            &after,
            &request(
                &initial,
                Command::Rename {
                    node: id("root"),
                    label: "stale".into()
                }
            )
        )
        .unwrap_err()
        .code,
        EditErrorCode::RevisionConflict
    );
}

#[test]
fn source_and_gap_choices_are_kind_checked_and_retained_when_not_audible() {
    let media = edit(
        &empty(),
        Command::AddAsset {
            id: AssetId::new("still").unwrap(),
            asset: AssetRecord {
                label: "Picture".into(),
                content_hash: "a".repeat(64),
                video: None,
                audio: None,
                still_image: true,
                frame_count: None,
                source_qualification: None,
            },
        },
    );
    let mut document = edit(
        &media,
        Command::Insert {
            parent: id("root"),
            index: 0,
            subtree: Subtree {
                root: id("source"),
                overrides: BTreeMap::new(),
                nodes: BTreeMap::from([(
                    id("source"),
                    BeatNode {
                        framing: None,
                        label: "Unvoiced source".into(),
                        kind: NodeKind::Source {
                            source: SourceNode {
                                duration: FrameDuration::new(3).unwrap(),
                                video: SourceVideo::Still {
                                    asset: AssetId::new("still").unwrap(),
                                },
                                video_mapping: SourceVideoMapping::FitBeat,
                                audio: None,
                                audio_mapping: SourceAudioMapping::FitBeat,
                                audio_offset: AudioSample(0),
                                link: LinkRelation::Independent,
                            },
                        },
                        audio_edges: Default::default(),
                    },
                )]),
            },
        },
    );
    document = edit(
        &document,
        Command::WrapRepeat {
            node: id("source"),
            id: id("repeat"),
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        },
    );
    let before_duration = document.duration().unwrap();
    for (node, edges) in [
        (
            "source",
            [
                AudioBoundaryKind::SourcePlacementStart,
                AudioBoundaryKind::SourcePlacementEnd,
            ],
        ),
        (
            "repeat",
            [
                AudioBoundaryKind::RepeatGapStart,
                AudioBoundaryKind::RepeatGapEnd,
            ],
        ),
    ] {
        for edge in edges {
            document = edit(
                &document,
                Command::SetAudioEdge {
                    node: id(node),
                    edge,
                    policy: AudioEdgePolicy::Hard,
                },
            );
            assert_eq!(
                document.nodes()[&id(node)].audio_edges.get(edge),
                AudioEdgePolicy::Hard
            );
        }
    }
    assert_eq!(document.duration().unwrap(), before_duration);
    for (node, edge) in [
        ("source", AudioBoundaryKind::RepeatGapStart),
        ("repeat", AudioBoundaryKind::SourcePlacementEnd),
        ("root", AudioBoundaryKind::RepeatGapEnd),
    ] {
        for policy in [AudioEdgePolicy::Automatic, AudioEdgePolicy::Hard] {
            assert_eq!(
                apply(
                    &document,
                    &request(
                        &document,
                        Command::SetAudioEdge {
                            node: id(node),
                            edge,
                            policy
                        }
                    )
                )
                .unwrap_err()
                .code,
                EditErrorCode::WrongNodeKind
            );
        }
    }
    let with_gap = edit(
        &document,
        Command::SetRepeat {
            node: id("repeat"),
            plays: 3,
            gap: Some(HoldRecipe {
                duration: FrameDuration::new(2).unwrap(),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            }),
        },
    );
    assert_eq!(
        with_gap.nodes()[&id("repeat")].audio_edges,
        document.nodes()[&id("repeat")].audio_edges
    );
    let without_gap = edit(
        &with_gap,
        Command::SetRepeat {
            node: id("repeat"),
            plays: 2,
            gap: None,
        },
    );
    assert_eq!(
        without_gap.nodes()[&id("repeat")].audio_edges,
        document.nodes()[&id("repeat")].audio_edges
    );
}

#[test]
fn current_wire_defaults_an_absent_object_but_requires_complete_known_policies_when_present() {
    let mut initial = serde_json::to_value(empty()).unwrap();
    assert!(initial["nodes"]["root"].get("audio_edges").is_none());
    assert_eq!(
        ProjectDocument::from_json(&initial.to_string()).unwrap(),
        empty()
    );
    initial["nodes"]["root"]["audio_edges"] =
        serde_json::to_value(AudioEdgePolicies::default()).unwrap();
    assert_eq!(
        ProjectDocument::from_json(&initial.to_string()).unwrap(),
        empty()
    );
    let mut mutations = Vec::new();
    let mut null = initial.clone();
    null["nodes"]["root"]["audio_edges"] = serde_json::Value::Null;
    mutations.push(null);
    for name in [
        "node_start",
        "node_end",
        "source_placement_start",
        "source_placement_end",
        "repeat_gap_start",
        "repeat_gap_end",
    ] {
        let mut missing = initial.clone();
        missing["nodes"]["root"]["audio_edges"]
            .as_object_mut()
            .unwrap()
            .remove(name);
        mutations.push(missing);
    }
    for (name, value) in [
        ("node_start", serde_json::Value::Null),
        ("node_start", serde_json::json!("soft")),
        ("invented_edge", serde_json::json!("automatic")),
        ("source_placement_start", serde_json::json!("hard")),
        ("repeat_gap_end", serde_json::json!("hard")),
    ] {
        let mut invalid = initial.clone();
        invalid["nodes"]["root"]["audio_edges"][name] = value;
        mutations.push(invalid);
    }
    for invalid in mutations {
        assert!(ProjectDocument::from_json(&invalid.to_string()).is_err());
    }
    let node = initial["nodes"]["root"].to_string();
    let duplicate = node.replacen(
        "\"node_start\":\"automatic\"",
        "\"node_start\":\"automatic\",\"node_start\":\"hard\"",
        1,
    );
    assert!(serde_json::from_str::<BeatNode>(&duplicate).is_err());
}

#[test]
fn migrating_automatic_edges_does_not_grow_legacy_document_request_or_patch_json() {
    let initial = empty();
    let request = request(
        &initial,
        Command::Insert {
            parent: id("root"),
            index: 0,
            subtree: Subtree {
                root: id("group"),
                nodes: BTreeMap::from([
                    (id("group"), BeatNode::sequence("Group", vec![id("hold")])),
                    (
                        id("hold"),
                        BeatNode::hold(
                            "Hold",
                            HoldRecipe {
                                duration: FrameDuration::new(3).unwrap(),
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                            },
                        ),
                    ),
                ]),
                overrides: BTreeMap::new(),
            },
        },
    );
    let transaction = apply(&initial, &request).unwrap();
    let after = transaction.forward.apply(&initial).unwrap();
    for document in [&initial, &after] {
        let current_json = document.to_json().unwrap();
        assert!(!current_json.contains("audio_edges"));
        let old_json = current_json.replacen(
            &format!("\"schema_version\": {DOCUMENT_SCHEMA_VERSION}"),
            "\"schema_version\": 10",
            1,
        );
        assert_ne!(old_json, current_json);
        let upgraded = legacy_v10::Document::from_json(&old_json)
            .unwrap()
            .upgrade()
            .unwrap();
        assert_eq!(&upgraded, document);
        assert_eq!(upgraded.to_json().unwrap().len(), old_json.len());
    }
    let request_json = serde_json::to_string(&request).unwrap();
    assert!(!request_json.contains("audio_edges"));
    assert_eq!(
        serde_json::to_string(&legacy_v10::upgrade_request(&request_json).unwrap()).unwrap(),
        request_json
    );
    let edit_json = serde_json::to_string(&transaction).unwrap();
    assert!(!edit_json.contains("audio_edges"));
    assert!(legacy_v10::matches_edit(&edit_json, &transaction).unwrap());
    // Explicit choices still retain the full object and round trip as authored.
    let hard = edit(
        &after,
        Command::SetAudioEdge {
            node: id("hold"),
            edge: AudioBoundaryKind::NodeEnd,
            policy: AudioEdgePolicy::Hard,
        },
    );
    assert!(hard.to_json().unwrap().contains("audio_edges"));
}

#[test]
fn ungroup_cannot_silently_drop_an_explicit_edge_exception() {
    let grouped = edit(
        &empty(),
        Command::Insert {
            parent: id("root"),
            index: 0,
            subtree: Subtree {
                root: id("group"),
                nodes: BTreeMap::from([(id("group"), BeatNode::sequence("Group", vec![]))]),
                overrides: BTreeMap::new(),
            },
        },
    );
    let hard = edit(
        &grouped,
        Command::SetAudioEdge {
            node: id("group"),
            edge: AudioBoundaryKind::NodeEnd,
            policy: AudioEdgePolicy::Hard,
        },
    );
    assert_eq!(
        apply(
            &hard,
            &request(&hard, Command::Ungroup { node: id("group") })
        )
        .unwrap_err()
        .code,
        EditErrorCode::InvalidCommand
    );
    let automatic = edit(
        &hard,
        Command::SetAudioEdge {
            node: id("group"),
            edge: AudioBoundaryKind::NodeEnd,
            policy: AudioEdgePolicy::Automatic,
        },
    );
    let ungrouped = edit(&automatic, Command::Ungroup { node: id("group") });
    assert_eq!(ungrouped.nodes(), empty().nodes());
}
