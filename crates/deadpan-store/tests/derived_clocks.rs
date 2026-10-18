use std::collections::BTreeMap;

use deadpan_core::*;
use deadpan_store::{AccessMode, ProjectStore, takes};

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}
fn duration(n: i64) -> FrameDuration {
    FrameDuration::new(n).unwrap()
}
fn document() -> ProjectDocument {
    let doc = ProjectDocument::new(
        ProjectId::new("clocks").unwrap(),
        revision("initial"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut nodes = BTreeMap::from([(
        id("root"),
        BeatNode::sequence("Root", vec![id("first"), id("last")]),
    )]);
    for (owner, child) in [("first", "left"), ("last", "right")] {
        let mut node = BeatNode::sequence("Retime", vec![]);
        node.kind = NodeKind::Retime {
            child: id(child),
            duration: duration(1),
            mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(2)).unwrap(),
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Edit,
        };
        nodes.insert(id(owner), node);
        nodes.insert(
            id(child),
            BeatNode::hold(
                "Hold",
                HoldRecipe {
                    duration: duration(2),
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                    picture_context: None,
                },
            ),
        );
    }
    let mut wire = serde_json::to_value(doc).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    wire["derived_clocks"] = serde_json::to_value(
        [("first", 1), ("last", 5)]
            .into_iter()
            .map(|(name, numerator)| {
                (
                    id(name),
                    DerivedRetimeClock {
                        extent: ExactRatio::new(numerator, 2).unwrap(),
                        child_start: ExactRatio::ZERO,
                        child_end: ExactRatio::integer(2),
                        preserve_input: None,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>(),
    )
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn take(store: &mut ProjectStore, action: takes::Action) -> takes::Outcome {
    let catalog = store.take_catalog().unwrap();
    store
        .apply_take(&takes::Request {
            project_id: catalog.project_id,
            expected_revision: catalog.revision_id,
            expected_version: catalog.version,
            action,
        })
        .unwrap()
}

#[test]
fn clocks_survive_sqlite_reopen_takes_and_durable_navigation() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("clocks.deadpan");
    let initial = document();
    let store = ProjectStore::create(&path, &initial).unwrap();
    assert_eq!(store.snapshot().unwrap(), initial);
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
    let request = CommandRequest {
        project_id: initial.project_id().clone(),
        expected_revision: initial.revision_id().clone(),
        new_revision: revision("refused"),
        command: Command::SetHoldDuration {
            node: id("right"),
            duration: duration(3),
        },
    };
    assert!(store.commit(&request).is_err());
    assert_eq!(store.snapshot().unwrap(), initial);
    let saved = takes::TakeId::new("saved").unwrap();
    take(
        &mut store,
        takes::Action::Create {
            id: saved.clone(),
            name: takes::TakeName::new("Exact clocks").unwrap(),
        },
    );
    let outcome = take(
        &mut store,
        takes::Action::Restore {
            id: saved,
            expected_snapshot: revision("initial"),
            new_revision: revision("restored"),
        },
    );
    assert_eq!(outcome.commit.unwrap().edit.duration_delta, 0);
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
    assert_eq!(
        store
            .undo(&revision("restored"), revision("undone"))
            .unwrap()
            .edit
            .duration_delta,
        0
    );
    assert_eq!(
        store
            .redo(&revision("undone"), revision("redone"))
            .unwrap()
            .edit
            .duration_delta,
        0
    );
    drop(store);
    let store = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
    let restored = store.snapshot().unwrap();
    assert_eq!(restored.derived_clocks(), initial.derived_clocks());
    assert_eq!(restored.duration().unwrap(), duration(3));
    assert_eq!(store.snapshot_at(&revision("initial")).unwrap(), initial);
}

#[test]
fn owned_preserve_input_survives_sqlite_reopen_take_restore_and_undo_redo() {
    let mut wire = serde_json::to_value(document()).unwrap();
    for (owner, child) in [("first", "left"), ("last", "right")] {
        wire["nodes"][owner]["kind"]["pitch"] =
            serde_json::to_value(PitchPolicy::Preserve).unwrap();
        let input = PreserveInputProjection::new(vec![PreserveInputRun {
            destination: ExactFrameRange::new(ExactRatio::ZERO, ExactRatio::integer(2)).unwrap(),
            source: ExactFrameRange::new(ExactRatio::ZERO, ExactRatio::integer(2)).unwrap(),
            provider: PreserveInputProvider::Current {
                target: InstancePath {
                    node: id(child),
                    repeats: vec![],
                },
            },
        }])
        .unwrap();
        wire["derived_clocks"][owner]["preserve_input"] = serde_json::to_value(input).unwrap();
    }
    let initial = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("owned.deadpan");
    drop(ProjectStore::create(&path, &initial).unwrap());
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
    assert_eq!(store.snapshot().unwrap(), initial);
    let saved = takes::TakeId::new("owned").unwrap();
    take(
        &mut store,
        takes::Action::Create {
            id: saved.clone(),
            name: takes::TakeName::new("Owned input").unwrap(),
        },
    );
    let restored = take(
        &mut store,
        takes::Action::Restore {
            id: saved,
            expected_snapshot: revision("initial"),
            new_revision: revision("restored"),
        },
    );
    assert_eq!(restored.commit.unwrap().edit.duration_delta, 0);
    let changed = store.snapshot().unwrap();
    assert_eq!(changed.derived_clocks(), initial.derived_clocks());
    assert_eq!(changed.revision_id(), &revision("restored"));
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
    assert_eq!(store.snapshot().unwrap(), changed);
    store
        .undo(&revision("restored"), revision("undone"))
        .unwrap();
    assert_eq!(
        store.snapshot().unwrap().derived_clocks(),
        initial.derived_clocks()
    );
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
    store.redo(&revision("undone"), revision("redone")).unwrap();
    assert_eq!(
        store.snapshot().unwrap().derived_clocks(),
        changed.derived_clocks()
    );
    assert_eq!(store.snapshot_at(&revision("initial")).unwrap(), initial);
    assert_eq!(store.snapshot_at(&revision("restored")).unwrap(), changed);
}

#[test]
fn pauses_inside_follow_speed_commit_reopen_and_navigate_history_exactly() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("retimed-pauses.deadpan");
    let mut initial = document();
    // An ordinary 2x-slow speed change: two child frames over four.
    let mut wire = serde_json::to_value(&initial).unwrap();
    wire["derived_clocks"] = serde_json::json!({});
    wire["nodes"]["first"]["kind"]["duration"] = serde_json::json!(4);
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["first"]);
    wire["nodes"].as_object_mut().unwrap().remove("last");
    wire["nodes"].as_object_mut().unwrap().remove("right");
    initial = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut store = ProjectStore::create(&path, &initial).unwrap();
    let mut head = initial.revision_id().clone();
    for (name, at, frames) in [("one", 1, 1), ("two", 3, 3)] {
        let request = CommandRequest {
            project_id: initial.project_id().clone(),
            expected_revision: head.clone(),
            new_revision: revision(name),
            command: Command::InsertTime {
                at: ProjectFrame(at),
                hold: HoldRecipe {
                    duration: duration(frames),
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                    picture_context: None,
                },
                id: id(&format!("{name}-pause")),
                identities: SplitIdentities {
                    nodes: (0..8).map(|n| id(&format!("{name}-{n}"))).collect(),
                },
                timing: AudioTimingId {
                    allocation: revision(name),
                    ordinal: 0,
                },
            },
        };
        store.commit(&request).unwrap();
        head = revision(name);
        // Reopening replays and validates the stored history.
        drop(store);
        store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
    }
    let edited = store.snapshot().unwrap();
    assert!(!edited.derived_clocks().is_empty());
    assert_eq!(
        edited.exact_durations().unwrap()[edited.root()],
        ExactRatio::integer(8)
    );
    store.undo(&revision("two"), revision("undo-two")).unwrap();
    store
        .undo(&revision("undo-two"), revision("undo-one"))
        .unwrap();
    let mut undone = store.snapshot().unwrap();
    assert_eq!(undone.duration().unwrap(), duration(4));
    assert!(undone.derived_clocks().is_empty());
    store
        .redo(&revision("undo-one"), revision("redo-one"))
        .unwrap();
    store
        .redo(&revision("redo-one"), revision("redo-two"))
        .unwrap();
    drop(store);
    let store = ProjectStore::open(&path, AccessMode::ReadOnly).unwrap();
    let redone = store.snapshot().unwrap();
    assert_eq!(redone.derived_clocks(), edited.derived_clocks());
    assert_eq!(redone.nodes(), edited.nodes());
    assert_eq!(redone.audio_bindings(), edited.audio_bindings());
    undone = store.snapshot_at(&revision("undo-one")).unwrap();
    assert_eq!(undone.nodes(), initial.nodes());
}

#[test]
fn nested_follow_speed_pauses_reopen_and_restore_the_complete_snapshot() {
    let hold = |frames| {
        BeatNode::hold(
            "Silence",
            HoldRecipe {
                duration: duration(frames),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
                picture_context: None,
            },
        )
    };
    let speed = |child, frames, end| {
        let mut node = BeatNode::sequence("Speed", vec![]);
        node.kind = NodeKind::Retime {
            child: id(child),
            duration: duration(frames),
            mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(end)).unwrap(),
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Edit,
        };
        node
    };
    let same_contents = |actual: &ProjectDocument, expected: &ProjectDocument| {
        let mut wire = serde_json::to_value(actual).unwrap();
        wire["revision_id"] = serde_json::to_value(expected.revision_id()).unwrap();
        assert_eq!(
            ProjectDocument::from_json(&wire.to_string()).unwrap(),
            *expected
        );
    };
    for outer_frames in [24, 18, 12] {
        let mut wire = serde_json::to_value(document()).unwrap();
        wire["derived_clocks"] = serde_json::json!({});
        wire["nodes"] = serde_json::to_value(BTreeMap::from([
            (
                id("root"),
                BeatNode::sequence("Root", vec![id("outer"), id("tail")]),
            ),
            (id("outer"), speed("outer-group", outer_frames, 16)),
            (
                id("outer-group"),
                BeatNode::sequence("Outer", vec![id("a"), id("inner"), id("b")]),
            ),
            (id("a"), hold(2)),
            (id("inner"), speed("inner-group", 12, 18)),
            (
                id("inner-group"),
                BeatNode::sequence("Inner", vec![id("c"), id("middle"), id("d")]),
            ),
            (id("c"), hold(2)),
            (id("middle"), hold(14)),
            (id("d"), hold(2)),
            (id("b"), hold(2)),
            (id("tail"), hold(4)),
        ]))
        .unwrap();
        let initial = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let scratch = tempfile::tempdir().unwrap();
        let path = scratch.path().join("nested-pauses.deadpan");
        let mut store = ProjectStore::create(&path, &initial).unwrap();
        let mut snapshots = vec![initial];
        for (at, frames, name) in [(7, 1, "one"), (9, 2, "two"), (outer_frames + 4, 1, "three")] {
            let before = snapshots.last().unwrap();
            let request = CommandRequest {
                project_id: before.project_id().clone(),
                expected_revision: before.revision_id().clone(),
                new_revision: revision(name),
                command: Command::InsertTime {
                    at: ProjectFrame(at),
                    hold: HoldRecipe {
                        duration: duration(frames),
                        video: HoldVideo::Background,
                        audio: HoldAudio::Silence,
                        picture_context: None,
                    },
                    id: id(&format!("{name}-pause")),
                    identities: SplitIdentities {
                        nodes: (0..before.nodes().len() + 4)
                            .map(|n| id(&format!("{name}-{n}")))
                            .collect(),
                    },
                    timing: AudioTimingId {
                        allocation: revision(name),
                        ordinal: 0,
                    },
                },
            };
            let expected = apply(before, &request)
                .unwrap()
                .forward
                .apply(before)
                .unwrap();
            store.commit(&request).unwrap();
            drop(store);
            store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
            assert_eq!(store.snapshot().unwrap(), expected);
            assert_eq!(store.snapshot_at(before.revision_id()).unwrap(), *before);
            snapshots.push(expected);
        }
        assert_eq!(snapshots[3].duration().unwrap(), duration(outer_frames + 8));
        let mut head = revision("three");
        for index in (0..3).rev() {
            let next = revision(&format!("undo-{index}"));
            store.undo(&head, next.clone()).unwrap();
            drop(store);
            store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
            same_contents(&store.snapshot().unwrap(), &snapshots[index]);
            head = next;
        }
        for (index, expected) in snapshots.iter().enumerate().skip(1) {
            let next = revision(&format!("redo-{index}"));
            store.redo(&head, next.clone()).unwrap();
            drop(store);
            store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
            same_contents(&store.snapshot().unwrap(), expected);
            head = next;
        }
        for expected in snapshots {
            assert_eq!(store.snapshot_at(expected.revision_id()).unwrap(), expected);
        }
    }
}
