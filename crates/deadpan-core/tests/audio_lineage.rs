use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicU64, Ordering},
};

use deadpan_core::*;
use serde_json::json;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn rev(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}
fn span() -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 48000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 48000,
            time_base,
        },
    )
    .unwrap()
}
fn source() -> BeatNode {
    BeatNode {
        label: "Original speech".into(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: duration(12),
                video: SourceVideo::Stream {
                    asset: AssetId::new("original").unwrap(),
                    span: span(),
                },
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(SourceAudio {
                    asset: AssetId::new("original").unwrap(),
                    span: span(),
                }),
                audio_mapping: SourceAudioMapping::FitBeat,
                audio_offset: AudioSample(0),
                link: LinkRelation::Linked,
            },
        },
        audio_edges: Default::default(),
    }
}
fn hold(frames: i64) -> BeatNode {
    BeatNode::hold(
        "Silence",
        HoldRecipe {
            duration: duration(frames),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}
fn sequence(children: &[&str]) -> BeatNode {
    BeatNode::sequence("Sequence", children.iter().map(|name| id(name)).collect())
}
fn repeat(child: &str, plays: u32) -> BeatNode {
    BeatNode {
        label: "Repeat".into(),
        kind: NodeKind::Repeat {
            child: id(child),
            iterations: IterationOrder::new(rev("plays"), plays).unwrap(),
            gap: None,
        },
        audio_edges: Default::default(),
    }
}
fn retime(child: &str, input: i64, output: i64) -> BeatNode {
    BeatNode {
        label: "Preserve".into(),
        kind: NodeKind::Retime {
            child: id(child),
            mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(input)).unwrap(),
            duration: duration(output),
            pitch: PitchPolicy::Preserve,
            purpose: RetimePurpose::Edit,
        },
        audio_edges: Default::default(),
    }
}
fn tree(children: &[&str], entries: Vec<(&str, BeatNode)>) -> ProjectDocument {
    let mut nodes: BTreeMap<_, _> = entries
        .into_iter()
        .map(|(key, node)| (id(key), node))
        .collect();
    nodes.insert(id("root"), sequence(children));
    let asset = AssetRecord {
        label: "Original".into(),
        content_hash: "a".repeat(64),
        video: Some(span()),
        audio: Some(span()),
        still_image: false,
        frame_count: Some(duration(30)),
        source_qualification: None,
    };
    ProjectDocument::from_json(
        &json!({
            "schema_version": DOCUMENT_SCHEMA_VERSION,
            "project_id":"lineage", "revision_id":"initial",
            "presentation_basis":{"width":16,"height":16,"frame_rate":{"numerator":30000,"denominator":1001},"color_policy":"sdr_rec709"},
            "basis_state":{"rate_origin":"explicit","geometry_origin":"explicit","primary":null},
            "root":"root", "assets":{"original":asset}, "marks":{}, "overrides":{}, "nodes":nodes
        })
        .to_string(),
    )
    .unwrap()
}
fn with_lineage(document: &ProjectDocument) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["audio_lineage"] = json!(
        document
            .nodes()
            .keys()
            .map(|owner| {
                (
                    owner.clone(),
                    AudioLineageId {
                        allocation: rev("retained"),
                        origin: owner.clone(),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>()
    );
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn request(document: &ProjectDocument, command: Command) -> CommandRequest {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: rev(&format!("edit-{}", NEXT.fetch_add(1, Ordering::Relaxed))),
        command,
    }
}
fn edit(document: &ProjectDocument, command: Command) -> (ProjectDocument, EditTransaction) {
    let transaction = apply(document, &request(document, command)).unwrap();
    let wire = serde_json::to_string(&transaction).unwrap();
    assert_eq!(
        serde_json::from_str::<EditTransaction>(&wire).unwrap(),
        transaction
    );
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    assert_eq!(
        ProjectDocument::from_json(&after.to_json().unwrap()).unwrap(),
        after
    );
    // Lineage supplies no asset admission or new media ownership.
    assert_eq!(after.assets(), document.assets());
    assert!(
        after
            .audio_lineage()
            .keys()
            .all(|key| after.nodes().contains_key(key))
    );
    (after, transaction)
}
fn split(
    document: &ProjectDocument,
    target: &str,
    at: i64,
    prefix: &str,
) -> (ProjectDocument, EditTransaction) {
    edit(
        document,
        Command::Split {
            node: id(target),
            at: duration(at),
            identities: SplitIdentities {
                nodes: (0..32).map(|i| id(&format!("{prefix}-{i}"))).collect(),
            },
        },
    )
}
fn children<'a>(document: &'a ProjectDocument, name: &str) -> &'a [NodeId] {
    let NodeKind::Sequence { children } = &document.nodes()[&id(name)].kind else {
        panic!("expected Sequence")
    };
    children
}
fn context(document: &ProjectDocument, node: &NodeId) -> NodeId {
    let NodeKind::Retime {
        child,
        purpose: RetimePurpose::Partition,
        ..
    } = &document.nodes()[node].kind
    else {
        panic!("expected transparent Partition")
    };
    child.clone()
}
fn token<'a>(document: &'a ProjectDocument, owner: &str) -> &'a AudioLineageId {
    &document.audio_lineage()[&id(owner)]
}
fn iteration(document: &ProjectDocument, owner: &str, ordinal: u32) -> IterationId {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&id(owner)].kind else {
        panic!("expected Repeat")
    };
    iterations.at(ordinal).unwrap()
}
fn subtree(name: &str, frames: i64) -> Subtree {
    Subtree {
        root: id(name),
        nodes: BTreeMap::from([(id(name), hold(frames))]),
        overrides: BTreeMap::new(),
    }
}

#[test]
fn split_seeds_unchanged_complete_contexts_in_patch_and_changed_ids() {
    let before = tree(
        &["processing"],
        vec![
            ("processing", retime("group", 16, 20)),
            ("group", sequence(&["speech", "pause"])),
            ("speech", source()),
            ("pause", hold(4)),
        ],
    );
    assert!(before.audio_lineage().is_empty());
    let (after, tx) = split(&before, "processing", 7, "cut");
    let sides = children(&after, "root");
    let left = context(&after, &sides[0]);
    let right = context(&after, &sides[1]);
    assert_eq!(left, id("processing"));
    let mapping = [
        ("processing", right),
        ("group", id("cut-3")),
        ("speech", id("cut-4")),
        ("pause", id("cut-5")),
    ];
    for (original, copy) in mapping {
        let expected = AudioLineageId {
            allocation: tx.forward.to_revision.clone(),
            origin: id(original),
        };
        assert_eq!(token(&after, original), &expected);
        assert_eq!(after.audio_lineage()[&copy], expected);
        assert_eq!(after.nodes()[&id(original)], before.nodes()[&id(original)]);
        assert!(!tx.forward.nodes.contains_key(&id(original)));
        assert!(tx.forward.audio_lineage.contains_key(&id(original)));
        assert!(tx.changed_ids.contains(&id(original)));
    }
    assert_eq!(after.duration().unwrap(), before.duration().unwrap());
    assert_eq!(after.audio_lineage().len(), 8);
    // Equal ordinary sources alone establish no family.
    let unrelated = tree(&["a", "b"], vec![("a", source()), ("b", source())]);
    assert!(unrelated.audio_lineage().is_empty());
}

#[test]
fn shallow_refinement_reuses_allocation_and_root_split_retains_payload_identity() {
    let before = tree(&["speech"], vec![("speech", source())]);
    let (first, _) = split(&before, "speech", 3, "first");
    let first_token = token(&first, "speech").clone();
    let right = children(&first, "root")[1].clone();
    let (second, tx) = split(&first, right.as_str(), 4, "second");
    assert_eq!(children(&second, "root").len(), 3);
    for side in children(&second, "root") {
        let local_context = context(&second, side);
        assert!(matches!(
            second.nodes()[&local_context].kind,
            NodeKind::Source { .. }
        ));
        assert_eq!(second.audio_lineage()[&local_context], first_token);
    }
    assert!(tx.forward.audio_lineage.values().all(|change| {
        change
            .after
            .as_ref()
            .is_none_or(|value| value.allocation == first_token.allocation)
    }));
    let (root_split, root_tx) = split(&second, "root", 5, "root-cut");
    let root_token = AudioLineageId {
        allocation: root_tx.forward.to_revision.clone(),
        origin: id("root"),
    };
    assert_eq!(token(&root_split, "root"), &root_token);
    for side in children(&root_split, "root") {
        let payload = context(&root_split, side);
        assert_eq!(root_split.audio_lineage()[&payload], root_token);
        assert_eq!(children(&root_split, payload.as_str()).len(), 3);
    }
    assert_eq!(root_split.duration().unwrap(), before.duration().unwrap());
}

#[test]
fn picture_labels_edges_and_transparent_grouping_preserve_audio_families() {
    let before = with_lineage(&tree(
        &["speech", "pause"],
        vec![("speech", source()), ("pause", hold(4))],
    ));
    let commands = [
        Command::Rename {
            node: id("speech"),
            label: "A different label".into(),
        },
        Command::SetSourceVideoMapping {
            node: id("speech"),
            mapping: SourceVideoMapping::Duration {
                frames: ExactRatio::integer(10),
                endpoints: EndpointPolicy::HoldAdjacent,
            },
        },
        Command::SetHoldProvider {
            node: id("pause"),
            video: HoldVideo::Freeze {
                asset: AssetId::new("original").unwrap(),
                timestamp: span().start(),
            },
        },
        Command::SetAudioEdge {
            node: id("speech"),
            edge: AudioBoundaryKind::NodeStart,
            policy: AudioEdgePolicy::Hard,
        },
        Command::Group {
            parent: id("root"),
            start: 0,
            end: 2,
            id: id("group"),
            label: "Group".into(),
        },
    ];
    let mut current = before.clone();
    for command in commands {
        let (next, tx) = edit(&current, command);
        assert_eq!(next.audio_lineage(), before.audio_lineage());
        assert!(tx.forward.audio_lineage.is_empty());
        current = next;
    }
    let (after, tx) = edit(&current, Command::Ungroup { node: id("group") });
    assert_eq!(after.audio_lineage(), before.audio_lineage());
    assert!(tx.forward.audio_lineage.is_empty());
}

#[test]
fn real_audio_edits_detach_only_the_changed_live_contribution_and_its_ancestors() {
    let before = with_lineage(&tree(
        &["processing"],
        vec![
            ("processing", retime("group", 16, 20)),
            ("group", sequence(&["speech", "pause"])),
            ("speech", source()),
            ("pause", hold(4)),
        ],
    ));
    let (copied, _) = split(&before, "processing", 7, "cut");
    for command in [
        Command::SetSourceAudioMapping {
            node: id("speech"),
            mapping: SourceAudioMapping::FitBeat,
            offset: AudioSample(1),
        },
        Command::SetSourceAudioMapping {
            node: id("speech"),
            mapping: SourceAudioMapping::Duration {
                frames: ExactRatio::new(35, 3).unwrap(),
            },
            offset: AudioSample(0),
        },
        Command::SetHoldDuration {
            node: id("pause"),
            duration: duration(5),
        },
    ] {
        let changed = match &command {
            Command::SetHoldDuration { .. } => "pause",
            _ => "speech",
        };
        let untouched = if changed == "pause" {
            "speech"
        } else {
            "pause"
        };
        let (after, _) = edit(&copied, command);
        for owner in ["root", "processing", "group", changed] {
            assert!(
                !after.audio_lineage().contains_key(&id(owner)),
                "changed raw contribution retained {owner}"
            );
        }
        assert_eq!(token(&after, untouched), token(&copied, untouched));
        for owner in ["cut-2", "cut-3", "cut-4", "cut-5"] {
            assert_eq!(token(&after, owner), token(&copied, owner));
        }
    }
    // Reasserting exactly the existing audio recipe is not a new contribution.
    let (no_change, tx) = edit(
        &copied,
        Command::SetSourceAudioMapping {
            node: id("speech"),
            mapping: SourceAudioMapping::FitBeat,
            offset: AudioSample(0),
        },
    );
    assert_eq!(no_change.audio_lineage(), copied.audio_lineage());
    assert!(tx.forward.audio_lineage.is_empty());
}

#[test]
fn moving_a_context_preserves_its_local_lineage_and_invalidates_both_processing_parents() {
    let before = with_lineage(&tree(
        &["left", "right"],
        vec![
            ("left", retime("left-group", 12, 20)),
            ("left-group", sequence(&["speech", "left-pause"])),
            ("speech", source()),
            ("left-pause", hold(12)),
            ("right", retime("right-group", 4, 8)),
            ("right-group", sequence(&["right-pause"])),
            ("right-pause", hold(4)),
        ],
    ));
    // Both Retime selections remain in bounds while the same source enters a
    // different processing ancestor and changes the old ancestor's input.
    let (after, _) = edit(
        &before,
        Command::Move {
            node: id("speech"),
            parent: id("right-group"),
            index: 1,
        },
    );
    for owner in ["root", "left", "left-group", "right", "right-group"] {
        assert!(!after.audio_lineage().contains_key(&id(owner)));
    }
    for owner in ["speech", "left-pause", "right-pause"] {
        assert_eq!(token(&after, owner), token(&before, owner));
    }
}

#[test]
fn deleting_original_physical_owner_preserves_the_copy_and_historical_origin_name() {
    let before = tree(&["speech"], vec![("speech", source())]);
    let (copied, _) = split(&before, "speech", 3, "cut");
    let historical = token(&copied, "speech").clone();
    let sides = children(&copied, "root");
    let left = sides[0].clone();
    let right_context = context(&copied, &sides[1]);
    let (after, _) = edit(&copied, Command::Delete { node: left });
    assert!(!after.nodes().contains_key(&id("speech")));
    assert!(!after.audio_lineage().contains_key(&id("speech")));
    assert_eq!(after.audio_lineage()[&right_context], historical);
    assert_eq!(historical.origin, id("speech"));
}

#[test]
fn nested_billion_play_isolation_preserves_compact_families_then_detaches_only_edited_branch() {
    let before = tree(
        &["outer"],
        vec![
            ("outer", repeat("group", 1_000_000_000)),
            ("group", sequence(&["inner", "sibling"])),
            ("inner", repeat("speech", 2)),
            ("speech", source()),
            ("sibling", hold(4)),
        ],
    );
    let outer_play = iteration(&before, "outer", 999_999_999);
    let inner_play = iteration(&before, "inner", 1);
    let (isolated, _) = edit(
        &before,
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("speech"),
                repeats: vec![
                    RepeatInstance {
                        node: id("outer"),
                        iteration: outer_play.clone(),
                    },
                    RepeatInstance {
                        node: id("inner"),
                        iteration: inner_play.clone(),
                    },
                ],
            },
            edit: OccurrenceEdit::Rename {
                label: "Selected speech".into(),
            },
            identities: OccurrenceIdentities {
                nodes: (0..5).map(|i| id(&format!("isolated-{i}"))).collect(),
                marks: vec![],
            },
        },
    );
    assert_eq!(isolated.nodes().len(), before.nodes().len() + 5);
    for (owner, plays) in [("outer", 1_000_000_000), ("inner", 2), ("isolated-1", 2)] {
        let NodeKind::Repeat { iterations, .. } = &isolated.nodes()[&id(owner)].kind else {
            panic!("expected Repeat");
        };
        assert_eq!(iterations.len(), plays);
        assert_eq!(iterations.segment_count(), 1);
    }
    assert_eq!(isolated.nodes()[&id("outer")], before.nodes()[&id("outer")]);
    assert_eq!(iteration(&isolated, "isolated-1", 1), inner_play);
    for (original, copies) in [
        ("group", vec!["isolated-0"]),
        ("inner", vec!["isolated-1"]),
        ("speech", vec!["isolated-2", "isolated-4"]),
        ("sibling", vec!["isolated-3"]),
    ] {
        for copy in copies {
            assert_eq!(token(&isolated, original), token(&isolated, copy));
        }
    }
    let (changed, _) = edit(
        &isolated,
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("isolated-4"),
                repeats: vec![
                    RepeatInstance {
                        node: id("outer"),
                        iteration: outer_play,
                    },
                    RepeatInstance {
                        node: id("isolated-1"),
                        iteration: inner_play,
                    },
                ],
            },
            edit: OccurrenceEdit::SetSourceAudioMapping {
                mapping: SourceAudioMapping::FitBeat,
                offset: AudioSample(-1),
            },
            identities: OccurrenceIdentities::default(),
        },
    );
    for owner in ["isolated-0", "isolated-1", "isolated-4"] {
        assert!(!changed.audio_lineage().contains_key(&id(owner)));
    }
    for owner in [
        "group",
        "inner",
        "speech",
        "sibling",
        "isolated-2",
        "isolated-3",
    ] {
        assert_eq!(token(&changed, owner), token(&isolated, owner));
    }
}

#[test]
fn clearing_replacing_and_shrinking_overrides_prunes_only_removed_live_owners() {
    let before = tree(
        &["repeat"],
        vec![("repeat", repeat("base", 3)), ("base", hold(4))],
    );
    let play = iteration(&before, "repeat", 2);
    let (overridden, _) = edit(
        &before,
        Command::SetPlayOverride {
            node: id("repeat"),
            iteration: play.clone(),
            subtree: subtree("override", 4),
        },
    );
    let retained = with_lineage(&overridden);
    for command in [
        Command::ClearPlayOverride {
            node: id("repeat"),
            iteration: play.clone(),
        },
        Command::SetPlayOverride {
            node: id("repeat"),
            iteration: play,
            subtree: subtree("replacement", 4),
        },
        Command::SetRepeat {
            node: id("repeat"),
            plays: 2,
            gap: None,
        },
    ] {
        let (after, _) = edit(&retained, command);
        assert!(!after.nodes().contains_key(&id("override")));
        for owner in ["root", "repeat", "override"] {
            assert!(!after.audio_lineage().contains_key(&id(owner)));
        }
        assert_eq!(token(&after, "base"), token(&retained, "base"));
        assert!(!after.audio_lineage().contains_key(&id("replacement")));
    }
}

#[test]
fn lineage_wire_and_patch_guards_reject_unknown_duplicate_dangling_and_oversized_entries() {
    let before = with_lineage(&tree(&["speech"], vec![("speech", source())]));
    let lineage = json!({"allocation":"old-revision","origin":"removed-owner"});
    let mut historical = serde_json::to_value(&before).unwrap();
    historical["audio_lineage"]["speech"] = lineage.clone();
    assert!(ProjectDocument::from_json(&historical.to_string()).is_ok());
    for bad in [
        json!({"allocation":"old-revision","origin":"removed-owner","media":"original"}),
        json!({"allocation":"old-revision"}),
        json!({"allocation":"", "origin":"speech"}),
    ] {
        let mut wire = historical.clone();
        wire["audio_lineage"]["speech"] = bad;
        assert!(ProjectDocument::from_json(&wire.to_string()).is_err());
    }
    let mut dangling = historical.clone();
    dangling["audio_lineage"]["missing"] = lineage.clone();
    assert!(ProjectDocument::from_json(&dangling.to_string()).is_err());
    let mut base = serde_json::to_value(&before).unwrap();
    base.as_object_mut().unwrap().remove("audio_lineage");
    let base = base.to_string();
    let duplicate = format!(
        "{},\"audio_lineage\":{{\"speech\":{lineage},\"speech\":{lineage}}}}}",
        base.strip_suffix('}').unwrap()
    );
    assert!(ProjectDocument::from_json(&duplicate).is_err());
    let oversize: BTreeMap<_, _> = (0..=MAX_DOCUMENT_NODES)
        .map(|i| (format!("owner-{i}"), lineage.clone()))
        .collect();
    let mut oversized = historical;
    oversized["audio_lineage"] = json!(oversize);
    let error = ProjectDocument::from_json(&oversized.to_string()).unwrap_err();
    assert!(error.message.contains("identity map exceeds"));

    let (_, tx) = edit(
        &before,
        Command::Rename {
            node: id("speech"),
            label: "renamed".into(),
        },
    );
    let mut patch = tx.forward.clone();
    let mut patch_wire = serde_json::to_value(&patch).unwrap();
    patch_wire.as_object_mut().unwrap().remove("audio_lineage");
    let patch_wire = patch_wire.to_string();
    let change = json!({"before":null,"after":lineage});
    let duplicate = format!(
        "{},\"audio_lineage\":{{\"speech\":{change},\"speech\":{change}}}}}",
        patch_wire.strip_suffix('}').unwrap()
    );
    assert!(serde_json::from_str::<DocumentPatch>(&duplicate).is_err());
    patch.audio_lineage.insert(
        id("speech"),
        ValueChange {
            before: Some(AudioLineageId {
                allocation: rev("wrong"),
                origin: id("speech"),
            }),
            after: None,
        },
    );
    assert_eq!(
        patch.apply(&before).unwrap_err().code,
        EditErrorCode::PatchConflict
    );
    patch.audio_lineage = BTreeMap::from([(
        id("missing"),
        ValueChange {
            before: None,
            after: Some(token(&before, "speech").clone()),
        },
    )]);
    assert!(patch.apply(&before).is_err());
    patch.audio_lineage = (0..=MAX_DOCUMENT_NODES)
        .map(|i| {
            (
                id(&format!("owner-{i}")),
                ValueChange {
                    before: None,
                    after: Some(token(&before, "speech").clone()),
                },
            )
        })
        .collect();
    assert_eq!(
        patch.apply(&before).unwrap_err().code,
        EditErrorCode::InvalidCommand
    );
}

#[test]
fn identity_exhaustion_is_atomic_before_any_lineage_is_seeded() {
    let before = tree(
        &["repeat"],
        vec![
            ("repeat", repeat("group", 3)),
            ("group", sequence(&["speech", "pause"])),
            ("speech", source()),
            ("pause", hold(4)),
        ],
    );
    let input = before.to_json().unwrap();
    for command in [
        Command::Split {
            node: id("group"),
            at: duration(1),
            identities: SplitIdentities {
                nodes: vec![id("only-one")],
            },
        },
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("speech"),
                repeats: vec![RepeatInstance {
                    node: id("repeat"),
                    iteration: iteration(&before, "repeat", 1),
                }],
            },
            edit: OccurrenceEdit::Rename {
                label: "never committed".into(),
            },
            identities: OccurrenceIdentities {
                nodes: vec![id("only-one")],
                marks: vec![],
            },
        },
    ] {
        assert!(apply(&before, &request(&before, command)).is_err());
        assert_eq!(before.to_json().unwrap(), input);
        assert!(before.audio_lineage().is_empty());
    }
    let (after, tx) = split(&before, "group", 1, "enough");
    let families: BTreeSet<_> = after.audio_lineage().values().collect();
    assert_eq!(families.len(), 3);
    assert_eq!(
        tx.inverse.apply(&after).unwrap().audio_lineage(),
        before.audio_lineage()
    );
}
