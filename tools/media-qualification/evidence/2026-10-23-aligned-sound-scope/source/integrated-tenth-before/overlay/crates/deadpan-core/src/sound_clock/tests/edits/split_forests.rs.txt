use super::*;

fn split_leaf(document: &ProjectDocument, name: &str) -> ProjectDocument {
    edit(
        document,
        name,
        Command::Split {
            node: node("owner"),
            at: frames(2),
            identities: SplitIdentities {
                nodes: vec![node("left"), node("right"), node("right-full")],
            },
        },
    )
}

fn group_leaf(document: &ProjectDocument) -> ProjectDocument {
    edit(
        document,
        "grouped",
        Command::GroupSelection {
            parent: node("top"),
            selection: SliceCaptureSelection::Child {
                node: node("owner"),
            },
            label: "Split group".into(),
            identities: GroupSelectionIdentities {
                group: node("group"),
                split: SplitIdentities::default(),
            },
            timing: timing("grouped"),
        },
    )
}

fn grouped_fixture(introduced: bool) -> ProjectDocument {
    if introduced {
        group_leaf(&edit(&fixture(), "prefix", insert("prefix", 0, 1)))
    } else {
        edit(&group_leaf(&fixture()), "prefix", insert("prefix", 0, 1))
    }
}

fn saved<'a>(document: &'a ProjectDocument, owner: &NodeId) -> &'a SoundClockJournal {
    &document.audio_bindings.sound_clocks[owner][&sound("effect")]
}

fn pieces(values: &[(&str, &str)]) -> Vec<SoundClockSplitPiece> {
    values
        .iter()
        .map(|(partition, owner)| SoundClockSplitPiece {
            partition: node(partition),
            full_owner: node(owner),
        })
        .collect()
}

fn assert_forest(reference: &SoundClockReference, historical: &str, expected: &[(&str, &str)]) {
    assert_eq!(
        reference.introduced_splits(),
        &[SoundClockSplit::new(node(historical), pieces(expected)).unwrap(),]
    );
}

#[test]
fn plain_source_and_hold_split_keep_complete_broad_scopes_in_every_capture() {
    for source in [false, true] {
        for captured in [false, true] {
            let mut before = fixture();
            if source {
                let mut source_node = hold(4);
                source_node.kind = NodeKind::Source { source: blank(4) };
                before.nodes.insert(node("owner"), source_node);
                before.validate().unwrap();
            }
            if captured {
                before = edit(&before, "prefix-one", insert("prefix-one", 0, 1));
                before = edit(&before, "prefix-two", insert("prefix-two", 0, 1));
            }
            let after = split_leaf(&before, "split");
            assert_eq!(after.duration().unwrap(), before.duration().unwrap());
            assert_eq!(after.beat_sounds.len(), 2);
            for owner in [node("owner"), node("right-full")] {
                assert_eq!(
                    after.beat_sounds[&owner],
                    before.beat_sounds[&node("owner")]
                );
                let journal = saved(&after, &owner);
                assert_eq!(journal.scope(), &node("top"));
                assert_eq!(journal.clocks().len(), if captured { 3 } else { 1 });
                for reference in journal.clocks() {
                    assert_eq!(reference.scope(), &node("top"));
                    assert_eq!(reference.owner(), &node("owner"));
                    assert!(reference.introduced_groups().is_empty());
                    assert_forest(
                        reference,
                        "owner",
                        &[("left", "owner"), ("right", "right-full")],
                    );
                }
            }
        }
    }
}

#[test]
fn split_distinguishes_an_introduced_group_from_a_genuine_historical_group() {
    for introduced in [false, true] {
        let before = grouped_fixture(introduced);
        let old = saved(&before, &node("owner")).clocks()[0].clone();
        let after = split_leaf(&before, "split");
        for owner in [node("owner"), node("right-full")] {
            let journal = saved(&after, &owner);
            assert_eq!(journal.scope(), &node("top"));
            assert_eq!(clock_ids(journal), vec![timing("prefix"), timing("split")]);
            let historical = &journal.clocks()[0];
            assert_eq!(historical.scope(), old.scope());
            assert_eq!(historical.owner(), old.owner());
            assert_eq!(
                historical.introduced_groups().contains(&node("group")),
                introduced
            );
            assert_eq!(
                after.audio_bindings.timings[historical.timing()]
                    .nodes()
                    .contains_key(&node("group")),
                !introduced,
            );
            assert_forest(
                historical,
                "owner",
                &[("left", "owner"), ("right", "right-full")],
            );
            let latest = &journal.clocks()[1];
            assert!(latest.introduced_groups().is_empty());
            assert!(
                after.audio_bindings.timings[latest.timing()]
                    .nodes()
                    .contains_key(&node("group"))
            );
            assert_forest(
                latest,
                "owner",
                &[("left", "owner"), ("right", "right-full")],
            );
        }
    }
}

#[test]
fn partition_refinement_composes_old_forests_and_retains_them_after_a_prefix_move() {
    let before = split_leaf(&grouped_fixture(true), "split");
    let refined = edit(
        &before,
        "refined",
        Command::Split {
            node: node("left"),
            at: frames(1),
            identities: SplitIdentities {
                nodes: vec![node("middle"), node("middle-full")],
            },
        },
    );
    assert_eq!(refined.beat_sounds.len(), 3);
    assert_eq!(refined.duration().unwrap(), before.duration().unwrap());
    for owner in [node("owner"), node("middle-full"), node("right-full")] {
        let journal = saved(&refined, &owner);
        assert_eq!(journal.scope(), &node("top"));
        for reference in &journal.clocks()[..2] {
            assert_forest(
                reference,
                "owner",
                &[
                    ("left", "owner"),
                    ("middle", "middle-full"),
                    ("right", "right-full"),
                ],
            );
        }
        if owner == node("right-full") {
            assert_eq!(journal.clocks().len(), 2);
        } else {
            assert_eq!(journal.clocks().len(), 3);
            let latest = &journal.clocks()[2];
            assert_eq!(latest.timing(), &timing("refined"));
            assert_eq!(latest.owner(), &node("owner"));
            assert_forest(
                latest,
                "left",
                &[("left", "owner"), ("middle", "middle-full")],
            );
        }
    }
    let moved = edit(&refined, "moved-prefix", insert("moved-prefix", 0, 1));
    for owner in moved.beat_sounds.keys() {
        let previous = saved(&refined, owner);
        let current = saved(&moved, owner);
        assert_eq!(current.scope(), previous.scope());
        assert_eq!(current.clocks().len(), previous.clocks().len() + 1);
        assert_eq!(
            &current.clocks()[..previous.clocks().len()],
            previous.clocks()
        );
        let latest = current.clocks().last().unwrap();
        assert_eq!(latest.timing(), &timing("moved-prefix"));
        assert!(latest.introduced_splits().is_empty());
    }
}

#[test]
fn whole_group_copy_renames_complete_live_forests_and_historical_namespaces() {
    for introduced in [false, true] {
        let before = split_leaf(&grouped_fixture(introduced), "split");
        let slice = CapturedEditSlice::capture_selection(
            &before,
            &node("top"),
            &SliceCaptureSelection::Child {
                node: node("group"),
            },
            timing("copied-group"),
        )
        .unwrap();
        assert_eq!(
            CapturedEditSlice::from_json(&slice.to_json().unwrap()).unwrap(),
            slice
        );
        let after = edit(
            &before,
            "pasted-group",
            paste_command(&slice, "pasted-group", 0),
        );
        let copied = after
            .beat_sounds
            .keys()
            .filter(|owner| !before.beat_sounds.contains_key(*owner))
            .cloned()
            .collect::<BTreeSet<_>>();
        assert_eq!(copied.len(), 2);
        let scopes = copied
            .iter()
            .map(|owner| saved(&after, owner).scope().clone())
            .collect::<BTreeSet<_>>();
        assert_eq!(scopes.len(), 1);
        let scope = scopes.first().unwrap();
        assert_ne!(scope, &node("group"));
        assert!(
            matches!(&after.nodes[scope].kind, NodeKind::Sequence { children } if children.len() == 2)
        );
        for owner in &copied {
            let journal = saved(&after, owner);
            assert_eq!(journal.clocks().len(), 3);
            assert_eq!(
                journal.clocks()[0].introduced_groups().contains(scope),
                introduced
            );
            for reference in &journal.clocks()[..2] {
                assert_eq!(reference.introduced_splits().len(), 1);
                let split = &reference.introduced_splits()[0];
                assert_ne!(split.historical_node, node("owner"));
                assert!(
                    after.audio_bindings.timings[reference.timing()]
                        .nodes()
                        .contains_key(&split.historical_node)
                );
                assert_eq!(split.pieces.len(), 2);
                assert_eq!(
                    split
                        .pieces
                        .iter()
                        .map(|piece| piece.full_owner.clone())
                        .collect::<BTreeSet<_>>(),
                    copied
                );
                for piece in &split.pieces {
                    assert_ne!(piece.partition, node("left"));
                    assert_ne!(piece.partition, node("right"));
                    assert!(matches!(&after.nodes[&piece.partition].kind,
                        NodeKind::Retime { child, purpose: RetimePurpose::Partition, .. }
                            if child == &piece.full_owner));
                }
            }
            let latest = journal.clocks().last().unwrap();
            assert!(latest.introduced_splits().is_empty());
            assert!(latest.introduced_groups().is_empty());
        }
    }
}

#[test]
fn partial_fragment_copy_keeps_full_provider_and_drops_exterior_forest_authority() {
    let before = split_leaf(&grouped_fixture(true), "split");
    for (partition, original_owner, mapping) in [
        ("left", "owner", range(0, 2)),
        ("right", "right-full", range(2, 4)),
    ] {
        let slice = CapturedEditSlice::capture_selection(
            &before,
            &node("group"),
            &SliceCaptureSelection::Child {
                node: node(partition),
            },
            timing("copied-fragment"),
        )
        .unwrap();
        assert_eq!(slice.duration(), frames(2));
        let after = edit(
            &before,
            "pasted-fragment",
            paste_command(&slice, "pasted-fragment", 0),
        );
        let copied = after
            .beat_sounds
            .keys()
            .filter(|owner| !before.beat_sounds.contains_key(*owner))
            .collect::<Vec<_>>();
        assert_eq!(copied.len(), 1);
        let owner = copied[0];
        assert_eq!(
            after.beat_sounds[owner],
            before.beat_sounds[&node(original_owner)]
        );
        assert_eq!(after.node_duration(owner).unwrap(), frames(4));
        let parent = after.parent_of(owner).unwrap();
        assert!(matches!(&after.nodes[&parent].kind,
            NodeKind::Retime { child, duration, mapping: actual_mapping, purpose: RetimePurpose::Partition, .. }
                if child == owner && *duration == frames(2) && *actual_mapping == mapping));
        let journal = saved(&after, owner);
        assert_eq!(journal.scope(), owner);
        assert_eq!(journal.clocks().len(), 3);
        for reference in journal.clocks() {
            assert_eq!(reference.scope(), reference.owner());
            assert!(reference.introduced_groups().is_empty());
            assert!(reference.introduced_splits().is_empty());
        }
    }
}

#[test]
fn repeat_wrapping_cannot_discard_a_complete_split_forest_atomically() {
    for introduced in [false, true] {
        let before = split_leaf(&grouped_fixture(introduced), "split");
        for owner in [node("owner"), node("right-full")] {
            let journal = saved(&before, &owner);
            assert_eq!(journal.scope(), &node("top"));
            assert!(
                journal
                    .clocks()
                    .iter()
                    .all(|reference| !reference.introduced_splits().is_empty())
            );
        }
        let snapshot = before.to_json().unwrap();
        let error = apply(
            &before,
            &request(
                &before,
                "wrapped",
                Command::RepeatSelection {
                    parent: node("top"),
                    selection: SliceCaptureSelection::Child {
                        node: node("group"),
                    },
                    plays: 2,
                    identities: RepeatSelectionIdentities {
                        repeat: node("repeat"),
                        group: None,
                        split: SplitIdentities::default(),
                    },
                    timing: timing("wrapped"),
                },
            ),
        )
        .unwrap_err();
        assert!(
            error
                .message
                .contains("cannot narrow a complete retained sound clock forest"),
            "{error}"
        );
        assert_eq!(before.to_json().unwrap(), snapshot);
        before.validate().unwrap();
    }
}
