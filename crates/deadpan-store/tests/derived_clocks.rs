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
