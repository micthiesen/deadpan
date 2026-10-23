//! Explicit group correspondence, exact historical aliases and bounded work.
use super::*;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn node(duration: i64, kind: FrozenAudioKind) -> FrozenAudioNode {
    FrozenAudioNode {
        duration: FrameDuration::new(duration).unwrap(),
        edges: AudioEdgePolicies::default(),
        editorial_edges: Default::default(),
        kind,
    }
}

fn sequence(duration: i64, children: &[&str]) -> FrozenAudioNode {
    node(
        duration,
        FrozenAudioKind::Sequence {
            children: children.iter().map(|child| id(child)).collect(),
        },
    )
}

fn admit(nodes: BTreeMap<NodeId, FrozenAudioNode>) -> FrozenAudioLayout {
    FrozenAudioLayout::from_renamed_parts(
        id("root"),
        FrameRate::new(30_000, 1001).unwrap(),
        nodes,
        BTreeMap::new(),
        BTreeMap::new(),
        BTreeMap::new(),
        BTreeMap::new(),
    )
    .unwrap()
}

fn before() -> FrozenAudioLayout {
    admit(BTreeMap::from([
        (id("root"), sequence(5, &["top"])),
        (id("top"), sequence(5, &["empty", "a", "b"])),
        (id("empty"), sequence(0, &[])),
        (
            id("a"),
            node(2, FrozenAudioKind::Source { placement: None }),
        ),
        (
            id("b"),
            node(
                3,
                FrozenAudioKind::Hold {
                    audio: ReferenceAudibility::Silence,
                },
            ),
        ),
    ]))
}

fn grouped(nested: bool) -> FrozenAudioLayout {
    let mut nodes = before().nodes().clone();
    nodes.insert(id("top"), sequence(5, &["empty", "fresh"]));
    nodes.insert(
        id("fresh"),
        sequence(5, if nested { &["a", "inner"] } else { &["a", "b"] }),
    );
    if nested {
        nodes.insert(id("inner"), sequence(3, &["b"]));
    }
    admit(nodes)
}

fn groups(names: &[&str]) -> BTreeSet<NodeId> {
    names.iter().map(|name| id(name)).collect()
}

fn prove(
    old: &FrozenAudioLayout,
    live: &FrozenAudioLayout,
    introduced: &BTreeSet<NodeId>,
    budget: usize,
) -> Result<SoundClockCorrespondence, DocumentError> {
    old.sound_owner_clock_correspondence_with_groups(
        live,
        (&id("top"), &id("top")),
        (&id("a"), &id("a")),
        &crate::SoundClockRepeatMap::default(),
        introduced,
        budget,
    )
}

#[test]
fn introduced_group_keeps_old_top_children_and_real_live_paths_without_new_alias() {
    let old = before();
    let live = grouped(false);
    let old_wire = old.to_json().unwrap();
    let live_wire = live.to_json().unwrap();
    let proof = prove(&old, &live, &groups(&["fresh"]), MAX_DOCUMENT_NODES).unwrap();

    for name in ["top", "empty", "a", "b"] {
        assert_eq!(proof.historical_node(&id(name)), Some(&id(name)));
        let path = InstancePath {
            node: id(name),
            repeats: vec![],
        };
        assert_eq!(proof.remap_instance(&path).unwrap(), path);
    }
    assert_eq!(proof.node_pairs().count(), 4);
    assert_eq!(proof.historical_node(&id("fresh")), None);
    assert!(
        proof
            .remap_instance(&InstancePath {
                node: id("fresh"),
                repeats: vec![]
            })
            .is_err()
    );
    assert_eq!(
        proof.inner.live_path.parents.get(&id("a")),
        Some(&id("fresh"))
    );
    assert_eq!(
        proof.inner.live_path.parents.get(&id("fresh")),
        Some(&id("top"))
    );
    assert_eq!(
        proof.inner.historical_path.parents.get(&id("a")),
        Some(&id("top"))
    );
    assert_eq!(old.to_json().unwrap(), old_wire);
    assert_eq!(live.to_json().unwrap(), live_wire);
    // Journal/plan callers reject this owner mismatch separately.
    assert_ne!(proof.historical_node(&id("b")), Some(&id("a")));
}

#[test]
fn nested_and_empty_introduced_groups_keep_all_old_zero_duration_children() {
    let old = before();
    let mut nodes = grouped(true).nodes().clone();
    nodes.insert(id("top"), sequence(5, &["empty", "fresh-empty", "fresh"]));
    nodes.insert(id("fresh-empty"), sequence(0, &[]));
    let live = admit(nodes);
    let proof = prove(
        &old,
        &live,
        &groups(&["fresh", "inner", "fresh-empty"]),
        MAX_DOCUMENT_NODES,
    )
    .unwrap();
    for name in ["top", "empty", "a", "b"] {
        assert_eq!(proof.historical_node(&id(name)), Some(&id(name)));
    }
    for name in ["fresh", "inner", "fresh-empty"] {
        assert_eq!(proof.historical_node(&id(name)), None);
    }
    assert_eq!(
        proof.inner.live_path.parents.get(&id("fresh-empty")),
        Some(&id("top"))
    );
    assert_eq!(
        proof.inner.live_path.parents.get(&id("b")),
        Some(&id("inner"))
    );
    assert_eq!(
        proof.inner.live_path.parents.get(&id("inner")),
        Some(&id("fresh"))
    );

    // A new empty group cannot stand in for an old empty Sequence.
    let mut nodes = live.nodes().clone();
    nodes.remove(&id("empty"));
    nodes.insert(id("top"), sequence(5, &["fresh-empty", "fresh"]));
    let missing_old_empty = admit(nodes);
    assert!(
        prove(
            &old,
            &missing_old_empty,
            &groups(&["fresh", "inner", "fresh-empty"]),
            MAX_DOCUMENT_NODES
        )
        .is_err()
    );
}

#[test]
fn all_introduced_groups_must_be_fresh_existing_ordinary_internal_nonowners() {
    let old = before();
    let mut nodes = grouped(false).nodes().clone();
    nodes.insert(id("root"), sequence(5, &["top", "exterior"]));
    nodes.insert(id("exterior"), sequence(0, &[]));
    let live = admit(nodes);
    for extra in ["missing", "root", "top", "a", "b", "empty", "exterior"] {
        assert!(
            prove(&old, &live, &groups(&["fresh", extra]), MAX_DOCUMENT_NODES).is_err(),
            "{extra}"
        );
    }
    assert!(
        old.sound_owner_clock_correspondence_with_groups(
            &live,
            (&id("top"), &id("top")),
            (&id("a"), &id("fresh")),
            &crate::SoundClockRepeatMap::default(),
            &groups(&["fresh"]),
            MAX_DOCUMENT_NODES
        )
        .is_err()
    );

    // A known historical Sequence cannot be relabelled as introduced.
    let mut nodes = old.nodes().clone();
    nodes.insert(id("root"), sequence(5, &["top", "fresh"]));
    nodes.insert(id("fresh"), sequence(0, &[]));
    let known_historical = admit(nodes);
    assert!(
        prove(
            &known_historical,
            &live,
            &groups(&["fresh"]),
            MAX_DOCUMENT_NODES
        )
        .is_err()
    );
    assert!(
        prove(
            &old,
            &grouped(true),
            &groups(&["fresh"]),
            MAX_DOCUMENT_NODES
        )
        .is_err()
    );
}

#[test]
fn introduction_never_changes_existing_order_or_cardinality() {
    let old = before();
    let mut nodes = grouped(false).nodes().clone();
    nodes.insert(id("fresh"), sequence(5, &["b", "a"]));
    let reordered = admit(nodes);
    assert!(prove(&old, &reordered, &groups(&["fresh"]), MAX_DOCUMENT_NODES).is_err());

    let mut nodes = grouped(false).nodes().clone();
    nodes.remove(&id("b"));
    nodes.insert(id("fresh"), sequence(2, &["a"]));
    nodes.insert(id("top"), sequence(2, &["empty", "fresh"]));
    nodes.insert(id("root"), sequence(2, &["top"]));
    let deleted = admit(nodes);
    assert!(prove(&old, &deleted, &groups(&["fresh"]), MAX_DOCUMENT_NODES).is_err());

    // Duplicate leaf identities are rejected by layout admission itself.
    let mut nodes = grouped(false).nodes().clone();
    nodes.insert(id("fresh"), sequence(7, &["a", "a", "b"]));
    nodes.insert(id("top"), sequence(7, &["empty", "fresh"]));
    nodes.insert(id("root"), sequence(7, &["top"]));
    assert!(
        FrozenAudioLayout::from_renamed_parts(
            id("root"),
            old.rate(),
            nodes,
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new(),
            BTreeMap::new()
        )
        .is_err()
    );
}

#[test]
fn group_work_is_bounded_and_empty_relation_retains_exact_api_behavior() {
    let old = before();
    let live = grouped(true);
    let introduced = groups(&["fresh", "inner"]);
    let proof = prove(&old, &live, &introduced, MAX_DOCUMENT_NODES).unwrap();
    assert_eq!(
        prove(&old, &live, &introduced, proof.work() - 1)
            .unwrap_err()
            .code,
        DocumentErrorCode::LimitExceeded
    );
    assert_eq!(
        prove(&old, &live, &introduced, proof.work())
            .unwrap()
            .work(),
        proof.work()
    );
    assert_eq!(
        prove(&old, &live, &introduced, 1).unwrap_err().code,
        DocumentErrorCode::LimitExceeded
    );

    let exact = old
        .sound_owner_clock_correspondence_with_repeats(
            &old,
            &id("top"),
            &id("top"),
            (&id("a"), &id("a")),
            &crate::SoundClockRepeatMap::default(),
            MAX_DOCUMENT_NODES,
        )
        .unwrap();
    let empty = prove(&old, &old, &BTreeSet::new(), MAX_DOCUMENT_NODES).unwrap();
    assert_eq!(exact.work(), empty.work());
    assert_eq!(
        exact.node_pairs().collect::<Vec<_>>(),
        empty.node_pairs().collect::<Vec<_>>()
    );
    assert!(prove(&old, &live, &BTreeSet::new(), MAX_DOCUMENT_NODES).is_err());
}

fn unary_grouped(chain: usize) -> FrozenAudioLayout {
    assert!(chain > 0);
    let mut nodes = before().nodes().clone();
    nodes.insert(id("top"), sequence(5, &["empty", "fresh-0", "b"]));
    for index in 0..chain {
        let name = format!("fresh-{index}");
        let child = if index + 1 == chain {
            "a".to_owned()
        } else {
            format!("fresh-{}", index + 1)
        };
        nodes.insert(id(&name), sequence(2, &[&child]));
    }
    admit(nodes)
}

fn unary_groups(chain: usize) -> BTreeSet<NodeId> {
    (0..chain)
        .map(|index| id(&format!("fresh-{index}")))
        .collect()
}

fn prove_unary(
    old: &FrozenAudioLayout,
    live: &FrozenAudioLayout,
    introduced: &BTreeSet<NodeId>,
    budget: usize,
) -> Result<SoundClockCorrespondence, DocumentError> {
    old.sound_owner_clock_correspondence_with_groups(
        live,
        (&id("a"), &id("fresh-0")),
        (&id("a"), &id("a")),
        &crate::SoundClockRepeatMap::default(),
        introduced,
        budget,
    )
}

#[test]
fn introduced_scope_is_unaliased_while_its_complete_owner_keeps_the_exact_path() {
    let old = before();
    for chain in [1, 2, 4] {
        let live = unary_grouped(chain);
        let old_wire = old.to_json().unwrap();
        let live_wire = live.to_json().unwrap();
        let proof = prove_unary(&old, &live, &unary_groups(chain), MAX_DOCUMENT_NODES).unwrap();
        assert_eq!(
            proof
                .node_pairs()
                .map(|(live, old)| (live.clone(), old.clone()))
                .collect::<Vec<_>>(),
            vec![(id("a"), id("a"))]
        );
        let owner = InstancePath {
            node: id("a"),
            repeats: vec![],
        };
        assert_eq!(proof.remap_instance(&owner).unwrap(), owner);
        assert_eq!(proof.historical_node(&id("top")), None);
        assert_eq!(
            proof.inner.live_path.parents.get(&id("fresh-0")),
            Some(&id("top"))
        );
        for index in 0..chain {
            let group = id(&format!("fresh-{index}"));
            assert_eq!(proof.historical_node(&group), None);
            let child = if index + 1 == chain {
                id("a")
            } else {
                id(&format!("fresh-{}", index + 1))
            };
            assert_eq!(proof.inner.live_path.parents.get(&child), Some(&group));
        }
        assert_eq!(
            proof.inner.historical_path.parents.get(&id("a")),
            Some(&id("top"))
        );
        assert_eq!(old.to_json().unwrap(), old_wire);
        assert_eq!(live.to_json().unwrap(), live_wire);
        assert_eq!(
            prove_unary(&old, &live, &unary_groups(chain), proof.work() - 1)
                .unwrap_err()
                .code,
            DocumentErrorCode::LimitExceeded
        );
        assert!(prove_unary(&old, &live, &unary_groups(chain), proof.work()).is_ok());
    }
}

#[test]
fn capture_time_group_context_stays_aliased_only_in_its_actual_reference() {
    let old = before();
    let live = unary_grouped(2);
    let early = prove_unary(&old, &live, &unary_groups(2), MAX_DOCUMENT_NODES).unwrap();
    assert_eq!(early.historical_node(&id("fresh-0")), None);
    let captured = live
        .sound_owner_clock_correspondence_with_groups(
            &live,
            (&id("fresh-0"), &id("fresh-0")),
            (&id("a"), &id("a")),
            &crate::SoundClockRepeatMap::default(),
            &BTreeSet::new(),
            MAX_DOCUMENT_NODES,
        )
        .unwrap();
    assert_eq!(
        captured.historical_node(&id("fresh-0")),
        Some(&id("fresh-0"))
    );
    assert_eq!(
        captured.historical_node(&id("fresh-1")),
        Some(&id("fresh-1"))
    );
    assert_eq!(captured.historical_node(&id("a")), Some(&id("a")));
    // A group known in this layout must not be relabelled as newly introduced.
    assert!(prove_unary(&live, &live, &unary_groups(2), MAX_DOCUMENT_NODES).is_err());
}

#[test]
fn introduced_scope_refuses_unlisted_nonunary_and_temporal_chains() {
    let old = before();
    let live = unary_grouped(2);
    assert!(prove_unary(&old, &live, &unary_groups(1), MAX_DOCUMENT_NODES).is_err());
    assert!(prove_unary(&old, &live, &BTreeSet::new(), MAX_DOCUMENT_NODES).is_err());

    // Even a zero-duration extra sibling is outside the unary mode.
    let mut nodes = unary_grouped(1).nodes().clone();
    nodes.insert(id("extra-zero"), sequence(0, &[]));
    nodes.insert(id("fresh-0"), sequence(2, &["a", "extra-zero"]));
    let siblings = admit(nodes);
    assert!(prove_unary(&old, &siblings, &unary_groups(1), MAX_DOCUMENT_NODES).is_err());

    let mut nodes = unary_grouped(1).nodes().clone();
    nodes.insert(id("fresh-0"), sequence(2, &["processor"]));
    nodes.insert(
        id("processor"),
        node(
            2,
            FrozenAudioKind::Retime {
                child: id("a"),
                mapping: FrameRange::new(crate::ProjectFrame(0), crate::ProjectFrame(2)).unwrap(),
                pitch: PitchPolicy::FollowSpeed,
                purpose: RetimePurpose::Edit,
            },
        ),
    );
    let temporal = admit(nodes);
    assert!(prove_unary(&old, &temporal, &unary_groups(1), MAX_DOCUMENT_NODES).is_err());
    assert!(
        prove_unary(
            &old,
            &temporal,
            &groups(&["fresh-0", "processor"]),
            MAX_DOCUMENT_NODES
        )
        .is_err()
    );
}

#[test]
fn introduced_scope_requires_the_recorded_source_or_hold_owner_at_both_ends() {
    let old = before();
    let live = unary_grouped(1);
    // Historical Top cannot masquerade as the historical owner scope.
    assert!(
        old.sound_owner_clock_correspondence_with_groups(
            &live,
            (&id("top"), &id("fresh-0")),
            (&id("a"), &id("a")),
            &crate::SoundClockRepeatMap::default(),
            &unary_groups(1),
            MAX_DOCUMENT_NODES
        )
        .is_err()
    );
    // Explicitly asking for another existing owner does not retarget the chain.
    assert!(
        old.sound_owner_clock_correspondence_with_groups(
            &live,
            (&id("a"), &id("fresh-0")),
            (&id("a"), &id("b")),
            &crate::SoundClockRepeatMap::default(),
            &unary_groups(1),
            MAX_DOCUMENT_NODES
        )
        .is_err()
    );

    // Equal kind and duration cannot discover a replacement independent owner.
    let mut nodes = live.nodes().clone();
    nodes.insert(
        id("other-source"),
        node(2, FrozenAudioKind::Source { placement: None }),
    );
    nodes.insert(id("fresh-0"), sequence(2, &["other-source"]));
    nodes.insert(id("top"), sequence(7, &["empty", "a", "fresh-0", "b"]));
    nodes.insert(id("root"), sequence(7, &["top"]));
    let wrong_child = admit(nodes);
    assert!(prove_unary(&old, &wrong_child, &unary_groups(1), MAX_DOCUMENT_NODES).is_err());

    // The same bounded mode also admits a complete Hold owner.
    let mut old_nodes = old.nodes().clone();
    old_nodes.insert(
        id("a"),
        node(
            2,
            FrozenAudioKind::Hold {
                audio: ReferenceAudibility::Silence,
            },
        ),
    );
    let hold_old = admit(old_nodes);
    let mut live_nodes = live.nodes().clone();
    live_nodes.insert(
        id("a"),
        node(
            2,
            FrozenAudioKind::Hold {
                audio: ReferenceAudibility::Silence,
            },
        ),
    );
    let hold_live = admit(live_nodes);
    assert!(prove_unary(&hold_old, &hold_live, &unary_groups(1), MAX_DOCUMENT_NODES).is_ok());
}

#[test]
fn introduced_scope_never_skips_an_actual_project_root_or_unused_scope() {
    let old = before();
    let live = unary_grouped(1);
    assert!(
        old.sound_owner_clock_correspondence_with_groups(
            &live,
            (&id("a"), &id("root")),
            (&id("a"), &id("a")),
            &crate::SoundClockRepeatMap::default(),
            &groups(&["root", "fresh-0"]),
            MAX_DOCUMENT_NODES
        )
        .is_err()
    );
    assert!(
        prove_unary(
            &old,
            &live,
            &groups(&["fresh-0", "root"]),
            MAX_DOCUMENT_NODES
        )
        .is_err()
    );

    let mut nodes = live.nodes().clone();
    nodes.insert(id("exterior-new"), sequence(0, &[]));
    nodes.insert(id("root"), sequence(5, &["top", "exterior-new"]));
    let exterior = admit(nodes);
    assert!(
        prove_unary(
            &old,
            &exterior,
            &groups(&["fresh-0", "exterior-new"]),
            MAX_DOCUMENT_NODES
        )
        .is_err()
    );
}

#[test]
fn introduced_scope_checks_exact_stored_child_edge_and_derived_clock() {
    let old = before();
    // Defense in depth: these deliberately corrupted layouts cannot be made by
    // capture/from_json. Unique Arc mutation is confined to this private test.
    let mut live = unary_grouped(1);
    Arc::get_mut(&mut live.inner)
        .unwrap()
        .index
        .parents
        .insert(id("a"), (id("fresh-0"), ExactRatio::integer(1)));
    assert!(prove_unary(&old, &live, &unary_groups(1), MAX_DOCUMENT_NODES).is_err());

    let mut live = unary_grouped(1);
    Arc::get_mut(&mut live.inner)
        .unwrap()
        .derived_clocks
        .insert(
            id("fresh-0"),
            FrozenRetimeClock {
                extent: ExactRatio::integer(2),
                child_start: ExactRatio::ZERO,
                child_end: ExactRatio::integer(2),
            },
        );
    assert!(prove_unary(&old, &live, &unary_groups(1), MAX_DOCUMENT_NODES).is_err());
}

#[test]
fn unary_scope_retains_repeat_membership_through_its_real_wrapper_edges() {
    let mut old_nodes = before().nodes().clone();
    old_nodes.remove(&id("top"));
    old_nodes.remove(&id("empty"));
    old_nodes.remove(&id("b"));
    let iterations =
        IterationOrder::new(crate::RevisionId::new("unary-plays").unwrap(), 2).unwrap();
    old_nodes.insert(id("root"), sequence(4, &["repeat"]));
    old_nodes.insert(
        id("repeat"),
        node(
            4,
            FrozenAudioKind::Repeat {
                child: id("a"),
                iterations: iterations.clone(),
                gap_duration: FrameDuration::ZERO,
                gap_audio: ReferenceAudibility::Silence,
            },
        ),
    );
    let old = admit(old_nodes);
    let mut live_nodes = old.nodes().clone();
    live_nodes.insert(id("fresh-0"), sequence(2, &["fresh-1"]));
    live_nodes.insert(id("fresh-1"), sequence(2, &["a"]));
    live_nodes.insert(
        id("repeat"),
        node(
            4,
            FrozenAudioKind::Repeat {
                child: id("fresh-0"),
                iterations,
                gap_duration: FrameDuration::ZERO,
                gap_audio: ReferenceAudibility::Silence,
            },
        ),
    );
    let live = admit(live_nodes);
    let repeats = crate::SoundClockRepeatMap::new(vec![crate::SoundClockRepeatStep::Shared {
        live_repeat: id("repeat"),
        historical_repeat: id("repeat"),
    }])
    .unwrap();
    let proof = old
        .sound_owner_clock_correspondence_with_groups(
            &live,
            (&id("a"), &id("fresh-0")),
            (&id("a"), &id("a")),
            &repeats,
            &unary_groups(2),
            MAX_DOCUMENT_NODES,
        )
        .unwrap();
    for ordinal in [0, 1] {
        let instance = InstancePath {
            node: id("a"),
            repeats: vec![crate::RepeatInstance {
                node: id("repeat"),
                iteration: IterationId {
                    allocation: crate::RevisionId::new("unary-plays").unwrap(),
                    ordinal,
                },
            }],
        };
        assert_eq!(proof.remap_instance(&instance).unwrap(), instance);
    }
    assert_eq!(proof.historical_node(&id("repeat")), Some(&id("repeat")));
    assert_eq!(proof.historical_node(&id("fresh-0")), None);
    assert_eq!(proof.historical_node(&id("fresh-1")), None);
}

#[test]
fn unary_scope_keeps_explicit_owner_aliases_after_an_exact_copy_rename() {
    let old = before();
    let mut nodes = unary_grouped(2).nodes().clone();
    let source = nodes.remove(&id("a")).unwrap();
    nodes.insert(id("copied-owner"), source);
    nodes.insert(id("fresh-1"), sequence(2, &["copied-owner"]));
    let live = admit(nodes);
    let proof = old
        .sound_owner_clock_correspondence_with_groups(
            &live,
            (&id("a"), &id("fresh-0")),
            (&id("a"), &id("copied-owner")),
            &crate::SoundClockRepeatMap::default(),
            &unary_groups(2),
            MAX_DOCUMENT_NODES,
        )
        .unwrap();
    assert_eq!(proof.historical_node(&id("copied-owner")), Some(&id("a")));
    assert_eq!(proof.historical_node(&id("fresh-0")), None);
    assert_eq!(
        proof
            .remap_instance(&InstancePath {
                node: id("copied-owner"),
                repeats: vec![]
            })
            .unwrap(),
        InstancePath {
            node: id("a"),
            repeats: vec![]
        }
    );
}
