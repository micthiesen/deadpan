use super::*;

fn selected_group(
    document: &ProjectDocument,
    parent: &str,
    selection: SliceCaptureSelection,
) -> Command {
    let plan = document.group_selection(&node(parent), &selection).unwrap();
    Command::GroupSelection {
        parent: node(parent),
        selection,
        label: "Sound group".into(),
        identities: GroupSelectionIdentities {
            group: node("group"),
            split: SplitIdentities {
                nodes: (0..plan.required_split_ids)
                    .map(|index| node(&format!("group-split-{index}")))
                    .collect(),
            },
        },
        timing: timing("grouped"),
    }
}

#[test]
fn sound_clocks_survive_direct_and_selected_groups_without_a_new_clock() {
    let before = edit(&fixture(), "shifted", insert("shifted", 0, 2));
    let commands = [
        Command::Group {
            parent: node("root"),
            start: 1,
            end: 3,
            id: node("group"),
            label: "Sound group".into(),
        },
        selected_group(
            &before,
            "root",
            SliceCaptureSelection::Child { node: node("top") },
        ),
        selected_group(
            &before,
            "root",
            SliceCaptureSelection::Children {
                first: node("lead"),
                last: node("top"),
            },
        ),
        selected_group(
            &before,
            "root",
            SliceCaptureSelection::Range {
                range: range(2, 11),
            },
        ),
    ];
    for command in commands {
        let grouped = edit(&before, "grouped", command);
        assert_eq!(grouped.audio_bindings, before.audio_bindings);
        assert_eq!(grouped.beat_sounds, before.beat_sounds);
        assert_eq!(grouped.duration().unwrap(), before.duration().unwrap());
        assert_eq!(
            grouped.source_splice_boundary(&node("top"), 0).unwrap(),
            before.source_splice_boundary(&node("top"), 0).unwrap()
        );
        let ungrouped = edit(
            &grouped,
            "ungrouped",
            Command::Ungroup {
                node: node("group"),
            },
        );
        assert_eq!(ungrouped.nodes, before.nodes);
        assert_eq!(ungrouped.audio_bindings, before.audio_bindings);
        assert_eq!(ungrouped.beat_sounds, before.beat_sounds);

        // The wrapper may remain during a later temporal edit. The old scope
        // still names the same processing subtree below its new ancestry.
        let moved = edit(&grouped, "moved-group", insert("moved-group", 0, 1));
        assert_eq!(
            journal(&moved),
            vec![timing("shifted"), timing("moved-group")]
        );
        assert_eq!(
            moved.audio_bindings.sound_clocks[&node("owner")][&sound("effect")].scope(),
            &node("top")
        );
        let restored = edit(
            &moved,
            "unwrapped-move",
            Command::Ungroup {
                node: node("group"),
            },
        );
        assert_eq!(restored.audio_bindings, moved.audio_bindings);
        assert_eq!(restored.beat_sounds, before.beat_sounds);
    }
}

#[test]
fn sound_clock_scope_interiors_and_removal_refuse_atomically() {
    let before = edit(&fixture(), "shifted", insert("shifted", 0, 2));
    let snapshot = before.to_json().unwrap();
    for command in [
        Command::Group {
            parent: node("top"),
            start: 0,
            end: 1,
            id: node("group"),
            label: "Inside scope".into(),
        },
        selected_group(
            &before,
            "top",
            SliceCaptureSelection::Child {
                node: node("owner"),
            },
        ),
        Command::Ungroup { node: node("top") },
    ] {
        let error = apply(&before, &request(&before, "grouped", command)).unwrap_err();
        assert!(
            error.message.contains("inside a retained beat sound scope"),
            "{error}"
        );
        assert_eq!(before.to_json().unwrap(), snapshot);
    }
}

#[test]
fn ungroup_never_discards_a_sequences_own_sounds_with_or_without_clocks() {
    for clocked in [false, true] {
        let mut before = fixture();
        let events = before.beat_sounds.remove(&node("owner")).unwrap();
        before.beat_sounds.insert(node("top"), events);
        before.validate().unwrap();
        if clocked {
            before = edit(&before, "shifted", insert("shifted", 0, 2));
        }
        let snapshot = before.to_json().unwrap();
        let error = apply(
            &before,
            &request(&before, "ungrouped", Command::Ungroup { node: node("top") }),
        )
        .unwrap_err();
        assert!(
            error
                .message
                .contains("sounds owned by the removed Sequence"),
            "{error}"
        );
        assert_eq!(before.to_json().unwrap(), snapshot);
    }
}

#[test]
fn selected_groups_still_refuse_partial_sound_owners_with_or_without_clocks() {
    for clocked in [false, true] {
        let mut before = fixture();
        let start = if clocked {
            before = edit(&before, "shifted", insert("shifted", 0, 2));
            4
        } else {
            2
        };
        let selection = SliceCaptureSelection::Range {
            range: range(start, start + 1),
        };
        assert!(
            before
                .group_selection(&node("top"), &selection)
                .unwrap()
                .required_split_ids
                > 0
        );
        let command = selected_group(&before, "top", selection);
        let snapshot = before.to_json().unwrap();
        let error = apply(&before, &request(&before, "grouped", command)).unwrap_err();
        assert!(error.message.contains("group endpoint splits"), "{error}");
        assert_eq!(before.to_json().unwrap(), snapshot);
    }
}

#[test]
fn unrelated_and_empty_selected_groups_preserve_existing_sound_clocks() {
    let mut base = fixture();
    base.nodes
        .insert(node("empty"), BeatNode::sequence("Empty", vec![]));
    base.nodes.insert(
        node("unrelated"),
        BeatNode::sequence("Unrelated", vec![node("empty")]),
    );
    let NodeKind::Sequence { children } = &mut base.nodes.get_mut(&node("root")).unwrap().kind
    else {
        unreachable!()
    };
    children.push(node("unrelated"));
    base.validate().unwrap();
    let before = edit(&base, "shifted", insert("shifted", 0, 2));
    for selection in [
        SliceCaptureSelection::Child {
            node: node("empty"),
        },
        SliceCaptureSelection::Children {
            first: node("empty"),
            last: node("empty"),
        },
    ] {
        let command = selected_group(&before, "unrelated", selection);
        let grouped = edit(&before, "grouped", command);
        assert_eq!(grouped.audio_bindings, before.audio_bindings);
        assert_eq!(grouped.beat_sounds, before.beat_sounds);
        assert_eq!(grouped.node_duration(&node("group")).unwrap(), frames(0));
        let ungrouped = edit(
            &grouped,
            "ungrouped",
            Command::Ungroup {
                node: node("group"),
            },
        );
        assert_eq!(ungrouped.nodes, before.nodes);
        assert_eq!(ungrouped.audio_bindings, before.audio_bindings);
    }
}
