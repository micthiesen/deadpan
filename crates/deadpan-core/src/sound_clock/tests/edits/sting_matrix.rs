//! A sting attached to a beat follows that beat's identity through every
//! whole-beat structural edit, and refuses readably where its own timing would
//! change. `edit` checks each exact inverse (Undo) and the wire roundtrip.
use super::*;

fn ids(prefix: &str, count: usize) -> Vec<NodeId> {
    (0..count).map(|i| node(&format!("{prefix}-{i}"))).collect()
}

fn refusal(document: &ProjectDocument, name: &str, command: Command) -> String {
    let snapshot = document.to_json().unwrap();
    let error = apply(document, &request(document, name, command)).unwrap_err();
    assert_eq!(document.to_json().unwrap(), snapshot);
    error.message
}

fn move_owner(document: &ProjectDocument, name: &str, parent: &str, index: usize) -> Command {
    let start = document.source_splice_boundary(&node("top"), 0).unwrap().0;
    Command::MoveRange {
        source_revision: document.revision_id().clone(),
        source_parent: node("top"),
        range: range(start, start + 4),
        destination: MoveRangeDestination::Seam {
            parent: node(parent),
            index,
        },
        identities: SplitIdentities { nodes: vec![] },
        timing: timing(name),
    }
}

#[test]
fn a_sting_moves_with_its_beat_inside_and_out_of_its_group() {
    let before = fixture();
    let sting = before.beat_sounds[&node("owner")].clone();
    // Reorder inside the enclosing group, then lift the beat out to the root.
    let within = edit(&before, "within", move_owner(&before, "within", "top", 2));
    assert_eq!(within.beat_sounds[&node("owner")], sting);
    assert_eq!(journal(&within), vec![timing("within")]);
    let lifted = edit(&within, "lifted", {
        let start = within.source_splice_boundary(&node("top"), 1).unwrap().0;
        Command::MoveRange {
            source_revision: within.revision_id().clone(),
            source_parent: node("top"),
            range: range(start, start + 4),
            destination: MoveRangeDestination::Seam {
                parent: node("root"),
                index: 0,
            },
            identities: SplitIdentities { nodes: vec![] },
            timing: timing("lifted"),
        }
    });
    assert_eq!(lifted.beat_sounds[&node("owner")], sting);
    assert_eq!(journal(&lifted), vec![timing("within"), timing("lifted")]);
    // Each pre-edit placement is retained exactly, in chronological order.
    assert_eq!(
        lifted.audio_bindings.timings[&timing("within")],
        FrozenAudioLayout::capture(&before).unwrap()
    );
    assert_eq!(
        lifted.audio_bindings.timings[&timing("lifted")],
        FrozenAudioLayout::capture(&within).unwrap()
    );
    lifted.validate().unwrap();
}

#[test]
fn a_sting_follows_split_group_ungroup_and_delete_of_its_beat() {
    let before = fixture();
    let sting = before.beat_sounds[&node("owner")].clone();
    let split = edit(
        &before,
        "split",
        Command::Split {
            node: node("owner"),
            at: frames(2),
            identities: SplitIdentities {
                nodes: ids("split", 3),
            },
        },
    );
    // Both fragments keep the complete recipe behind delivery crops.
    assert_eq!(split.beat_sounds.len(), 2);
    assert!(split.beat_sounds.values().all(|events| *events == sting));

    let grouped = edit(
        &before,
        "grouped",
        Command::GroupSelection {
            parent: node("top"),
            selection: SliceCaptureSelection::Child {
                node: node("owner"),
            },
            label: "Sting beat".into(),
            identities: GroupSelectionIdentities {
                group: node("wrapper"),
                split: SplitIdentities::default(),
            },
            timing: timing("grouped"),
        },
    );
    assert_eq!(grouped.beat_sounds, before.beat_sounds);
    let ungrouped = edit(
        &grouped,
        "ungrouped",
        Command::Ungroup {
            node: node("top"),
            wrappers: vec![],
        },
    );
    assert_eq!(ungrouped.beat_sounds, before.beat_sounds);

    // Deleting the beat deletes its sting; `edit` proves Undo restores both.
    let deleted = edit(
        &before,
        "deleted",
        Command::DeleteRipple {
            node: node("owner"),
            timing: timing("deleted"),
        },
    );
    assert!(deleted.beat_sounds.is_empty());
    assert!(deleted.audio_bindings.sound_clocks.is_empty());
}

#[test]
fn a_repeated_sting_plays_in_every_play_until_kept_on_the_first() {
    let before = fixture();
    let repeated = edit(
        &before,
        "repeated",
        Command::RepeatSelection {
            parent: node("top"),
            selection: SliceCaptureSelection::Child {
                node: node("owner"),
            },
            plays: 3,
            identities: RepeatSelectionIdentities {
                repeat: node("repeat"),
                group: None,
                split: SplitIdentities::default(),
            },
            timing: timing("repeated"),
        },
    );
    // The owner is the repeated child, so its sting sounds in every play.
    assert_eq!(repeated.beat_sounds, before.beat_sounds);
    let first = edit(
        &repeated,
        "first",
        Command::KeepFirstPlayAttachments {
            node: node("repeat"),
            identities: OccurrenceIdentities {
                nodes: vec![node("first-owner")],
                marks: vec![],
            },
        },
    );
    assert!(!first.beat_sounds.contains_key(&node("owner")));
    assert_eq!(
        first.beat_sounds[&node("first-owner")],
        before.beat_sounds[&node("owner")]
    );
}

#[test]
fn timing_changes_inside_a_sting_beat_refuse_readably() {
    let before = fixture();
    for (name, command) in [
        (
            "longer",
            Command::SetHoldDuration {
                node: node("owner"),
                duration: frames(6),
            },
        ),
        (
            "slower",
            Command::WrapRetime {
                node: node("owner"),
                id: node("speed"),
                duration: frames(8),
                pitch: PitchPolicy::Preserve,
            },
        ),
    ] {
        let message = refusal(&before, name, command);
        assert!(
            message.starts_with(
                "this edit changes the timing inside a beat that carries an attached sound"
            ),
            "{message}"
        );
    }
    // Legacy whole-node commands remain historical replay grammar only.
    let message = refusal(
        &before,
        "legacy",
        Command::WrapRepeat {
            node: node("owner"),
            id: node("repeat"),
            plays: 2,
            gap: None,
            anchor_policy: Default::default(),
        },
    );
    assert!(message.contains("remove beat sounds"), "{message}");
    // A Sequence's own sound has no exact redistribution onto its children.
    let mut sequence = before.clone();
    let events = sequence.beat_sounds.remove(&node("owner")).unwrap();
    sequence.beat_sounds.insert(node("top"), events);
    sequence.validate().unwrap();
    let message = refusal(
        &sequence,
        "ungroup",
        Command::Ungroup {
            node: node("top"),
            wrappers: vec![],
        },
    );
    assert!(
        message.contains("owned by the removed Sequence"),
        "{message}"
    );
}

/// After a pause cuts a beat, its pieces are Partition fragments. A sting
/// attached to one fragment follows it through move, deletion, Repeat and
/// grouping; refining that fragment with another Split still refuses.
#[test]
fn a_sting_on_a_fragment_follows_whole_beat_edits_and_its_split_refuses() {
    let mut before = fixture();
    let events = before.beat_sounds.remove(&node("owner")).unwrap();
    let count = before
        .insert_time_target(ProjectFrame(7))
        .unwrap()
        .split
        .map_or(0, |split| split.required_ids);
    let paused = edit(
        &before,
        "paused",
        Command::InsertTime {
            at: ProjectFrame(7),
            hold: HoldRecipe {
                duration: frames(1),
                video: HoldVideo::Background,
                picture_context: None,
                audio: HoldAudio::Silence,
            },
            id: node("pause"),
            identities: SplitIdentities {
                nodes: ids("p", count),
            },
            timing: timing("paused"),
        },
    );
    let fragment = node("p-0");
    assert!(matches!(
        paused.nodes[&fragment].kind,
        NodeKind::Retime {
            purpose: RetimePurpose::Partition,
            ..
        }
    ));
    let mut attached = paused.clone();
    attached
        .beat_sounds
        .insert(fragment.clone(), events.clone());
    attached.validate().unwrap();
    let moved = edit(
        &attached,
        "moved",
        Command::MoveRange {
            source_revision: attached.revision_id().clone(),
            source_parent: node("top"),
            range: range(5, 7),
            destination: MoveRangeDestination::Seam {
                parent: node("root"),
                index: 0,
            },
            identities: SplitIdentities { nodes: vec![] },
            timing: timing("moved"),
        },
    );
    assert_eq!(moved.beat_sounds[&fragment], events);
    let deleted = edit(
        &attached,
        "deleted",
        Command::DeleteRipple {
            node: fragment.clone(),
            timing: timing("deleted"),
        },
    );
    assert!(deleted.beat_sounds.is_empty());
    let repeated = edit(
        &attached,
        "repeated",
        Command::RepeatSelection {
            parent: node("top"),
            selection: SliceCaptureSelection::Child {
                node: fragment.clone(),
            },
            plays: 2,
            identities: RepeatSelectionIdentities {
                repeat: node("repeat"),
                group: None,
                split: SplitIdentities::default(),
            },
            timing: timing("repeated"),
        },
    );
    assert_eq!(repeated.beat_sounds[&fragment], events);
    let grouped = edit(
        &attached,
        "grouped",
        Command::GroupSelection {
            parent: node("top"),
            selection: SliceCaptureSelection::Child {
                node: fragment.clone(),
            },
            label: "Fragment".into(),
            identities: GroupSelectionIdentities {
                group: node("group"),
                split: SplitIdentities::default(),
            },
            timing: timing("grouped"),
        },
    );
    assert_eq!(grouped.beat_sounds[&fragment], events);
    let message = refusal(
        &attached,
        "split",
        Command::Split {
            node: fragment,
            at: frames(1),
            identities: SplitIdentities { nodes: ids("s", 4) },
        },
    );
    assert!(
        message.starts_with(
            "this edit changes the timing inside a beat that carries an attached sound"
        ),
        "{message}"
    );
}
