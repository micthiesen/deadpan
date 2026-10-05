//! Synthetic structure verifies durable copy identity and history, not media.
use deadpan_core::*;
use deadpan_store::{AccessMode, ProjectStore};
use rusqlite::Connection;
use std::{collections::BTreeMap, error::Error, path::Path};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

#[path = "edited_slice/child_capture.rs"]
mod child_capture;
#[path = "edited_slice/move_range.rs"]
mod move_range;
#[path = "edited_slice/placement.rs"]
mod placement;
#[path = "edited_slice/preview.rs"]
mod preview;

fn node(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}
fn timing(name: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: revision(name),
        ordinal: 0,
    }
}
fn recipe(frames: i64) -> HoldRecipe {
    HoldRecipe {
        picture_context: None,
        duration: FrameDuration::new(frames).unwrap(),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    }
}
fn request(document: &ProjectDocument, name: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(name),
        command,
    }
}

fn document() -> Result<ProjectDocument> {
    let empty = ProjectDocument::new(
        ProjectId::new("edited-slice-history")?,
        revision("baseline"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )?;
    let mut wire = serde_json::to_value(empty)?;
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("lead"), node("group"), node("tail")]),
        ),
        (node("lead"), BeatNode::hold("Lead", recipe(2))),
        (
            node("group"),
            BeatNode::sequence("Group", vec![node("voice"), node("repeat")]),
        ),
        (node("voice"), BeatNode::hold("Owned voice", recipe(7))),
        (
            node("repeat"),
            BeatNode {
                label: "Owned repeat".into(),
                framing: None,
                audio_treatments: Default::default(),
                audio_editorial_edges: Default::default(),
                audio_edges: Default::default(),
                kind: NodeKind::Repeat {
                    child: node("echo"),
                    iterations: IterationOrder::new(revision("plays"), 3)?,
                    gap: Some(recipe(1)),
                    escalation: None,
                },
                cutaways: Vec::new(),
                captions: Vec::new(),
            },
        ),
        (node("echo"), BeatNode::hold("Echo", recipe(2))),
        (node("tail"), BeatNode::hold("Tail", recipe(3))),
    ]))?;
    let mut document = ProjectDocument::from_json(&wire.to_string())?;
    for (id, owner, position) in [("owned-mark", "voice", 2), ("root-mark", "root", 1)] {
        let edit = apply(
            &document,
            &request(
                &document,
                id,
                Command::SetMark {
                    id: MarkId::new(id)?,
                    owner: node(owner),
                    label: id.into(),
                    boundary: BoundaryAnchor {
                        coordinate: Anchor::Local {
                            node: node(owner),
                            position: ExactRatio::integer(position),
                        },
                        bias: InsertionBias::Right,
                    },
                    loss_policy: AnchorLossPolicy::DeleteOwned,
                },
            ),
        )?;
        document = edit.forward.apply(&document)?;
    }
    assert_eq!(document.duration()?.frames(), 20);
    Ok(document)
}

fn capture(document: &ProjectDocument) -> Result<CapturedEditSlice> {
    Ok(CapturedEditSlice::capture(
        document,
        &node("group"),
        FrameRange::new(ProjectFrame(3), ProjectFrame(17))?,
        timing("copy"),
    )?)
}

fn paste(
    document: &ProjectDocument,
    slice: &CapturedEditSlice,
    name: &str,
    index: usize,
) -> Result<CommandRequest> {
    let count = slice.identity_requirements()?;
    assert_eq!(
        count.marks, 1,
        "the unselected root mark must not be copied"
    );
    Ok(request(
        document,
        name,
        Command::SpliceSlice {
            parent: node("root"),
            index,
            slice: slice.clone(),
            identities: SlicePasteIdentities {
                authored: OccurrenceIdentities {
                    nodes: (0..count.nodes)
                        .map(|i| node(&format!("{name}-node-{i}")))
                        .collect(),
                    marks: (0..count.marks)
                        .map(|i| MarkId::new(format!("{name}-mark-{i}")))
                        .collect::<std::result::Result<_, _>>()?,
                },
                aliases: (0..count.aliases)
                    .map(|i| node(&format!("{name}-alias-{i}")))
                    .collect(),
            },
            timing: timing(name),
        },
    ))
}

fn authored(path: &Path) -> Result<Vec<String>> {
    let database = Connection::open(path.join("project.sqlite"))?;
    let mut rows = Vec::new();
    for sql in [
        "SELECT json_array(id,parent_id,kind,document) FROM revisions ORDER BY id",
        "SELECT json_array(id,parent_id,revision_id,request,edit) FROM history ORDER BY id",
        "SELECT json_array(singleton,head_revision,cursor,workflow) FROM state",
        "SELECT json_array(position,history_id) FROM redo ORDER BY position",
    ] {
        rows.extend(
            database
                .prepare(sql)?
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?,
        );
    }
    Ok(rows)
}
fn assert_authored(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut value = serde_json::to_value(expected)?;
    value["revision_id"] = serde_json::to_value(actual.revision_id())?;
    assert_eq!(serde_json::to_value(actual)?, value);
    Ok(())
}

#[test]
fn captured_slice_survives_source_deletion_and_independent_pastes_reopen_undo_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("slices.deadpan");
    let baseline = document()?;
    let mut store = ProjectStore::create(&path, &baseline)?;
    let cells = authored(&path)?;
    let slice = capture(&store.snapshot()?)?;
    let bytes = serde_json::to_vec(&slice)?;
    assert_eq!(slice.duration().frames(), 14);
    assert_eq!(slice.revision_id(), baseline.revision_id());
    assert_eq!(authored(&path)?, cells, "copy must not create history");
    assert_eq!(store.snapshot()?, baseline);
    store.commit(&request(
        &baseline,
        "delete-original",
        Command::DeleteRipple {
            node: node("group"),
            timing: timing("delete-original"),
        },
    ))?;
    let removed = store.snapshot()?;
    assert_eq!(removed.duration()?.frames(), 5);
    assert!(!removed.nodes().contains_key(&node("voice")));
    drop(store);

    // The retained value crosses serialization and a writer restart. The current
    // document lacks its original nodes; the historical revision proves capture.
    let slice: CapturedEditSlice = serde_json::from_slice(&bytes)?;
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let first = paste(&removed, &slice, "first", 1)?;
    let stale = paste(&removed, &slice, "stale", 1)?;
    let cells = authored(&path)?;
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    let preview = reader.preview(&first)?;
    assert_eq!(preview.duration_delta, 14);
    assert_eq!(authored(&path)?, cells);
    drop(reader);
    assert_eq!(store.commit(&first)?.edit, preview);
    let once = store.snapshot()?;
    assert_eq!(once.duration()?.frames(), 19);
    assert_eq!(once.node_duration(&node("first-node-0"))?.frames(), 14);
    assert_eq!(once.marks().len(), 2);
    let second = paste(&once, &slice, "second", 2)?;
    store.commit(&second)?;
    let twice = store.snapshot()?;
    assert_eq!(twice.duration()?.frames(), 33);
    assert_eq!(twice.marks().len(), 3);
    let first_mark = &twice.marks()[&MarkId::new("first-mark-0")?];
    let second_mark = &twice.marks()[&MarkId::new("second-mark-0")?];
    assert_ne!(first_mark.owner, second_mark.owner);
    let first_voice = first_mark.owner.clone();
    let second_voice = second_mark.owner.clone();
    let changed = request(
        &twice,
        "rename-first",
        Command::Rename {
            node: first_voice.clone(),
            label: "Changed one copy".into(),
        },
    );
    store.commit(&changed)?;
    let independent = store.snapshot()?;
    assert_eq!(independent.nodes()[&first_voice].label, "Changed one copy");
    assert_eq!(independent.nodes()[&second_voice].label, "Owned voice");
    assert_eq!(
        serde_json::to_vec(&slice)?,
        bytes,
        "pasting cannot change the register"
    );
    store.validate()?;
    drop(store);

    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, independent);
    store.undo(independent.revision_id(), revision("undo-rename"))?;
    assert_authored(&store.snapshot()?, &twice)?;
    store.undo(&revision("undo-rename"), revision("undo-second"))?;
    assert_authored(&store.snapshot()?, &once)?;
    store.undo(&revision("undo-second"), revision("undo-first"))?;
    assert_authored(&store.snapshot()?, &removed)?;
    let cells = authored(&path)?;
    assert!(store.commit(&stale).is_err());
    assert_eq!(
        authored(&path)?,
        cells,
        "undo cannot revive a stale request"
    );
    store.undo(&revision("undo-first"), revision("undo-delete"))?;
    assert_authored(&store.snapshot()?, &baseline)?;
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.redo(&revision("undo-delete"), revision("redo-delete"))?;
    assert_authored(&store.snapshot()?, &removed)?;
    store.redo(&revision("redo-delete"), revision("redo-first"))?;
    assert_authored(&store.snapshot()?, &once)?;
    store.redo(&revision("redo-first"), revision("redo-second"))?;
    assert_authored(&store.snapshot()?, &twice)?;
    store.redo(&revision("redo-second"), revision("redo-rename"))?;
    assert_authored(&store.snapshot()?, &independent)?;
    store.validate()?;
    drop(store);
    ProjectStore::open(&path, AccessMode::ReadOnly)?.validate()?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let saved = database
        .prepare("SELECT request FROM history ORDER BY id")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    assert_eq!(saved.len(), 4);
    let saved: Vec<CommandRequest> = saved
        .iter()
        .map(|wire| serde_json::from_str(wire))
        .collect::<std::result::Result<_, _>>()?;
    assert_eq!(&saved[1..], &[first, second, changed]);
    Ok(())
}

#[test]
fn invalid_slice_identity_and_clock_pools_leave_all_authored_rows_unchanged() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("invalid-slices.deadpan");
    let baseline = document()?;
    let slice = capture(&baseline)?;
    let mut store = ProjectStore::create(&path, &baseline)?;
    let cells = authored(&path)?;
    assert!(slice.identity_requirements()?.timings > 0);
    assert!(slice.identity_requirements()?.aliases > 0);
    for case in 0..5 {
        let mut invalid = paste(&baseline, &slice, &format!("invalid-{case}"), 1)?;
        let Command::SpliceSlice {
            identities,
            timing,
            index,
            ..
        } = &mut invalid.command
        else {
            unreachable!()
        };
        match case {
            0 => identities.authored.nodes.clear(),
            1 => identities.authored.nodes[0] = node("tail"),
            2 => identities.aliases[0] = identities.authored.nodes[0].clone(),
            3 => timing.ordinal = u32::MAX,
            _ => *index = usize::MAX,
        }
        assert!(
            store.preview(&invalid).is_err(),
            "preview accepted case {case}"
        );
        assert!(
            store.commit(&invalid).is_err(),
            "commit accepted case {case}"
        );
        assert_eq!(store.snapshot()?, baseline);
        assert_eq!(authored(&path)?, cells);
    }
    store.validate()?;
    Ok(())
}
