use std::collections::BTreeMap;
use std::path::Path;

use deadpan_core::*;
use deadpan_store::{AccessMode, ProjectStore};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn mark_id(value: &str) -> MarkId {
    MarkId::new(value).unwrap()
}
fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}
fn recipe(frames: i64) -> HoldRecipe {
    HoldRecipe {
        duration: duration(frames),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    }
}
fn play(node: &str, ordinal: u32) -> IterationId {
    IterationId {
        allocation: revision(node),
        ordinal,
    }
}
fn selected() -> InstancePath {
    InstancePath {
        node: id("hold"),
        repeats: ["outer", "inner"]
            .map(|node| RepeatInstance {
                node: id(node),
                iteration: play(node, 1),
            })
            .to_vec(),
    }
}
fn initial() -> Result<ProjectDocument> {
    let empty = ProjectDocument::new(
        ProjectId::new("project")?,
        revision("initial"),
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30_000, 1001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )?;
    let repeat = |child: &str, name: &str, count, gap| -> Result<BeatNode> {
        Ok(BeatNode {
            label: name.into(),
            kind: NodeKind::Repeat {
                child: id(child),
                iterations: IterationOrder::new(revision(name), count)?,
                gap: Some(recipe(gap)),
            },
        })
    };
    let mark = |owner: &str, coordinate| Mark {
        owner: id(owner),
        label: owner.into(),
        boundary: BoundaryAnchor {
            coordinate,
            bias: InsertionBias::Right,
        },
        loss_policy: AnchorLossPolicy::KeepUnresolved,
        state: MarkState::Bound,
    };
    let mut wire = serde_json::to_value(empty)?;
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (id("root"), BeatNode::sequence("Root", vec![id("outer")])),
        (id("outer"), repeat("inner", "outer", 2, 2)?),
        (id("inner"), repeat("hold", "inner", 3, 1)?),
        (id("hold"), BeatNode::hold("Pause", recipe(4))),
    ]))?;
    wire["marks"] = serde_json::to_value(BTreeMap::from([
        (
            mark_id("owned"),
            mark(
                "hold",
                Anchor::Local {
                    node: id("hold"),
                    position: ExactRatio::integer(2),
                },
            ),
        ),
        (
            mark_id("selected"),
            mark(
                "root",
                Anchor::Occurrence {
                    instance: selected(),
                    position: ExactRatio::integer(3),
                },
            ),
        ),
    ]))?;
    Ok(ProjectDocument::from_json(&wire.to_string())?)
}
fn request(before: &ProjectDocument, name: &str, nodes: usize, marks: usize) -> CommandRequest {
    CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision(name),
        command: Command::EditOccurrence {
            instance: selected(),
            edit: OccurrenceEdit::SetHoldDuration {
                duration: duration(7),
            },
            identities: OccurrenceIdentities {
                nodes: (0..nodes)
                    .map(|index| id(&format!("{name}-{index}")))
                    .collect(),
                marks: (0..marks)
                    .map(|index| mark_id(&format!("{name}-{index}")))
                    .collect(),
            },
        },
    }
}
#[derive(Debug, PartialEq, Eq)]
struct StoredState {
    revisions: i64,
    edits: i64,
    redos: i64,
    cursor: Option<i64>,
    head: String,
}
fn stored(path: &Path) -> Result<StoredState> {
    let connection = Connection::open(path.join("project.sqlite"))?;
    Ok(connection.query_row(
        "SELECT (SELECT COUNT(*) FROM revisions), (SELECT COUNT(*) FROM history),
         (SELECT COUNT(*) FROM redo), cursor, head_revision FROM state WHERE singleton=1",
        [],
        |row| {
            Ok(StoredState {
                revisions: row.get(0)?,
                edits: row.get(1)?,
                redos: row.get(2)?,
                cursor: row.get(3)?,
                head: row.get(4)?,
            })
        },
    )?)
}
fn assert_authored_equal(actual: &ProjectDocument, expected: &ProjectDocument) {
    assert_eq!(actual.project_id(), expected.project_id());
    assert_eq!(actual.root(), expected.root());
    assert_eq!(actual.nodes(), expected.nodes());
    assert_eq!(actual.overrides(), expected.overrides());
    assert_eq!(actual.marks(), expected.marks());
    assert_eq!(actual.assets(), expected.assets());
    assert_eq!(actual.duration().unwrap(), expected.duration().unwrap());
}

#[test]
fn occurrence_isolation_and_mark_clones_are_one_durable_reversible_edit() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("occurrence.deadpan");
    let initial = initial()?;
    let mut writer = ProjectStore::create(&path, &initial)?;
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    let before_state = stored(&path)?;
    let request = request(&initial, "edit", 3, 2);
    let preview = reader.preview(&request)?;
    assert_eq!(preview, writer.preview(&request)?);
    assert_eq!(preview.duration_delta, 3);
    assert_eq!(reader.snapshot()?, initial);
    assert_eq!(writer.snapshot()?, initial);
    assert_eq!(stored(&path)?, before_state);
    let outcome = writer.commit(&request)?;
    assert_eq!(outcome.edit, preview);
    let edited = writer.snapshot()?;
    assert_eq!(edited, preview.forward.apply(&initial)?);
    assert_eq!(reader.snapshot()?, edited);
    assert_eq!(edited.duration()?.frames(), 33);
    assert_eq!(edited.nodes().len(), 7);
    assert_eq!(edited.overrides().len(), 2);
    assert_eq!(edited.marks().len(), 4);
    let cloned_inner = edited.overrides()[&id("outer")]
        .get(&play("outer", 1))
        .unwrap();
    let cloned_hold = edited.overrides()[cloned_inner]
        .get(&play("inner", 1))
        .unwrap();
    assert_eq!(edited.nodes()[&id("hold")], initial.nodes()[&id("hold")]);
    assert_eq!(edited.nodes()[&id("inner")], initial.nodes()[&id("inner")]);
    assert_eq!(
        edited.marks()[&mark_id("owned")],
        initial.marks()[&mark_id("owned")]
    );
    assert_eq!(edited.marks()[&mark_id("edit-1")].owner, *cloned_hold);
    let selected_mark = &edited.marks()[&mark_id("selected")];
    assert_eq!(selected_mark.state, MarkState::Bound);
    let Anchor::Occurrence { instance, position } = &selected_mark.boundary.coordinate else {
        panic!("selected mark lost its occurrence coordinate")
    };
    assert_eq!(instance.node, *cloned_hold);
    assert_eq!(instance.repeats[1].node, *cloned_inner);
    assert_eq!(*position, ExactRatio::integer(3));
    instance.validate(&edited)?;
    assert_eq!(
        stored(&path)?,
        StoredState {
            revisions: 2,
            edits: 1,
            redos: 0,
            cursor: Some(1),
            head: "edit".into()
        }
    );
    assert_eq!(
        writer.commit(&request).unwrap_err().code(),
        "RevisionConflict"
    );
    assert_eq!(writer.snapshot()?, edited);
    writer.validate()?;
    drop(reader);
    drop(writer);

    let mut reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(reopened.snapshot()?, edited);
    reopened.undo(edited.revision_id(), revision("undo"))?;
    assert_authored_equal(&reopened.snapshot()?, &initial);
    assert_eq!(
        stored(&path)?,
        StoredState {
            revisions: 3,
            edits: 1,
            redos: 1,
            cursor: None,
            head: "undo".into()
        }
    );
    drop(reopened);
    let mut reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    reopened.redo(&revision("undo"), revision("redo"))?;
    assert_authored_equal(&reopened.snapshot()?, &edited);
    assert_eq!(
        stored(&path)?,
        StoredState {
            revisions: 4,
            edits: 1,
            redos: 0,
            cursor: Some(1),
            head: "redo".into()
        }
    );
    reopened.validate()?;
    drop(reopened);
    assert_authored_equal(
        &ProjectStore::open(&path, AccessMode::ReadOnly)?.snapshot()?,
        &edited,
    );
    Ok(())
}

#[test]
fn insufficient_identity_pools_and_stale_requests_leave_no_partial_clones_or_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("failed-occurrence.deadpan");
    let initial = initial()?;
    let mut writer = ProjectStore::create(&path, &initial)?;
    let baseline = stored(&path)?;
    for (name, nodes, marks) in [
        ("no-nodes", 0, 2),
        ("short-nodes", 2, 2),
        ("short-marks", 3, 1),
    ] {
        let request = request(&initial, name, nodes, marks);
        assert!(writer.preview(&request).is_err());
        assert!(writer.commit(&request).is_err());
        assert_eq!(writer.snapshot()?, initial, "partial clone from {name}");
        assert_eq!(stored(&path)?, baseline, "partial history from {name}");
    }
    let mut stale = request(&initial, "stale", 3, 2);
    stale.expected_revision = revision("missing");
    assert_eq!(
        writer.preview(&stale).unwrap_err().code(),
        "RevisionConflict"
    );
    assert_eq!(
        writer.commit(&stale).unwrap_err().code(),
        "RevisionConflict"
    );
    assert_eq!(stored(&path)?, baseline);
    writer.validate()?;
    drop(writer);
    let mut reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(reopened.snapshot()?, initial);
    // A rejected request did not consume its revision or any supplied identity.
    reopened.commit(&request(&initial, "short-nodes", 3, 2))?;
    assert_eq!(reopened.snapshot()?.duration()?.frames(), 33);
    assert_eq!(stored(&path)?.edits, 1);
    reopened.validate()?;
    Ok(())
}
