//! Reuses the parent's synthetic admitted-bundle fixture. This establishes
//! durable acceptance boundaries, not generated-video decoding or quality.
use super::*;
use deadpan_core::{
    AudioTimingId, CapturedEditSlice, FrameRange, OccurrenceIdentities, ProjectFrame,
    SlicePasteIdentities, SplitIdentities,
};

fn paste(
    document: &ProjectDocument,
    slice: &CapturedEditSlice,
    name: &str,
) -> Result<CommandRequest> {
    let count = slice.identity_requirements()?;
    assert_eq!(count.marks, 0);
    let next = RevisionId::new(name)?;
    Ok(CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: next.clone(),
        command: Command::SpliceSlice {
            parent: document.root().clone(),
            index: 0,
            slice: slice.clone(),
            identities: SlicePasteIdentities {
                authored: OccurrenceIdentities {
                    nodes: (0..count.nodes)
                        .map(|i| NodeId::new(format!("{name}-node-{i}")))
                        .collect::<std::result::Result<_, _>>()?,
                    marks: Vec::new(),
                },
                aliases: (0..count.aliases)
                    .map(|i| NodeId::new(format!("{name}-alias-{i}")))
                    .collect::<std::result::Result<_, _>>()?,
            },
            timing: AudioTimingId {
                allocation: next,
                ordinal: 0,
            },
        },
    })
}

fn authored(package: &Path) -> Result<Vec<String>> {
    let database = Connection::open(package.join("project.sqlite"))?;
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

#[test]
fn historical_slice_reuses_accepted_artifact_after_last_hold_is_deleted() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("historical-slice.deadpan");
    let initial = document()?;
    let mut store = ProjectStore::create(&package, &initial)?;
    let input = ready_for_acceptance(&mut store)?;
    store.accept_generation_bundle(
        &input,
        &unchanged_relevance(&store, &input.new_revision)?,
        media_limits(),
    )?;
    let accepted = store.snapshot()?;
    let slice = CapturedEditSlice::capture(
        &accepted,
        accepted.root(),
        FrameRange::new(ProjectFrame(0), ProjectFrame(12))?,
        AudioTimingId {
            allocation: RevisionId::new("copy")?,
            ordinal: 0,
        },
    )?;
    let wire = serde_json::to_vec(&slice)?;
    edit_reconciled(
        &mut store,
        "deleted",
        Command::DeleteRipple {
            node: NodeId::new("hold")?,
            timing: AudioTimingId {
                allocation: RevisionId::new("deleted")?,
                ordinal: 0,
            },
        },
    )?;
    let before = store.snapshot()?;
    assert_eq!(before.duration()?.frames(), 0);
    assert!(!before.nodes().contains_key(&NodeId::new("hold")?));
    assert_eq!(
        store
            .generation_request(&input.identity.request_id)?
            .unwrap()
            .relevance,
        deadpan_jobs::Relevance::Detached
    );
    let cells = authored(&package)?;
    for (name, change) in [("wrong-artifact", true), ("wrong-history", false)] {
        let mut forged = serde_json::to_value(&slice)?;
        if change {
            let provenance = &mut forged["nodes"]["hold"]["kind"]["recipe"]["video"]["accepted"]["artifact"]
                ["provenance"];
            assert!(provenance.is_object());
            *provenance = serde_json::to_value(object(b"never accepted"))?;
        } else {
            forged["revision_id"] = serde_json::to_value(initial.revision_id())?;
        }
        let forged: CapturedEditSlice = serde_json::from_value(forged)?;
        let command = paste(&before, &forged, name)?;
        deadpan_core::apply(&before, &command)?;
        assert_eq!(
            store.preview(&command).unwrap_err().code(),
            "InvalidCommand"
        );
        assert_eq!(store.commit(&command).unwrap_err().code(), "InvalidCommand");
        assert_eq!(authored(&package)?, cells);
        assert_eq!(store.snapshot()?, before);
    }
    drop(store);
    let slice: CapturedEditSlice = serde_json::from_slice(&wire)?;
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let command = paste(&before, &slice, "pasted")?;
    let preview = store.preview(&command)?;
    assert_eq!(preview.duration_delta, 12);
    assert_eq!(authored(&package)?, cells);
    assert_eq!(store.commit(&command)?.edit, preview);
    let pasted = store.snapshot()?;
    assert_eq!(pasted.duration()?.frames(), 12);
    assert_eq!(pasted.assets(), accepted.assets());
    let provider = |document: &ProjectDocument| {
        document.nodes().values().find_map(|node| match &node.kind {
            NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Generated { .. }) => {
                Some(recipe.video.clone())
            }
            _ => None,
        })
    };
    assert_eq!(provider(&pasted), provider(&accepted));
    assert_eq!(
        store
            .generation_request(&input.identity.request_id)?
            .unwrap()
            .relevance,
        deadpan_jobs::Relevance::Detached
    );
    store.undo(pasted.revision_id(), RevisionId::new("undo-paste")?)?;
    assert_eq!(store.snapshot()?.nodes(), before.nodes());
    store.redo(
        &RevisionId::new("undo-paste")?,
        RevisionId::new("redo-paste")?,
    )?;
    assert_eq!(store.snapshot()?.nodes(), pasted.nodes());
    store.validate()?;
    drop(store);
    let reader = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(provider(&reader.snapshot()?), provider(&accepted));
    reader.validate()?;
    Ok(())
}

fn placement(
    document: &ProjectDocument,
    slice: &CapturedEditSlice,
    name: &str,
    replace: bool,
) -> Result<CommandRequest> {
    let mut request = paste(document, slice, name)?;
    let Command::SpliceSlice {
        parent,
        slice,
        identities,
        timing,
        ..
    } = request.command
    else {
        unreachable!()
    };
    let target = NodeId::new("destination")?;
    let at = FrameDuration::new(2)?;
    let range = FrameRange::new(ProjectFrame(1), ProjectFrame(4))?;
    let required = if replace {
        document
            .slice_replacement(&parent, range, &slice)?
            .required_ids
    } else {
        document
            .slice_splice_interior(&parent, &target, at, &slice)?
            .required_ids
    };
    let split_identities = SplitIdentities {
        nodes: (0..required)
            .map(|i| NodeId::new(format!("{name}-split-{i}")))
            .collect::<std::result::Result<_, _>>()?,
    };
    request.command = if replace {
        Command::ReplaceSlice {
            parent,
            range,
            slice,
            identities,
            split_identities,
            timing,
        }
    } else {
        Command::SpliceSliceAt {
            parent,
            target,
            at,
            slice,
            identities,
            split_identities,
            timing,
        }
    };
    Ok(request)
}

fn assert_authored(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = serde_json::to_value(actual.revision_id())?;
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

#[test]
fn interior_and_replacement_reuse_accepted_artifacts_without_reviving_generation() -> Result {
    for replace in [false, true] {
        let scratch = tempfile::tempdir()?;
        let package = scratch.path().join("historical-placement.deadpan");
        let mut store = ProjectStore::create(&package, &document()?)?;
        let input = ready_for_acceptance(&mut store)?;
        store.accept_generation_bundle(
            &input,
            &unchanged_relevance(&store, &input.new_revision)?,
            media_limits(),
        )?;
        let accepted = store.snapshot()?;
        let slice = CapturedEditSlice::capture(
            &accepted,
            accepted.root(),
            FrameRange::new(ProjectFrame(0), ProjectFrame(12))?,
            AudioTimingId {
                allocation: RevisionId::new("copy")?,
                ordinal: 0,
            },
        )?;
        edit_reconciled(
            &mut store,
            "deleted",
            Command::DeleteRipple {
                node: NodeId::new("hold")?,
                timing: AudioTimingId {
                    allocation: RevisionId::new("deleted")?,
                    ordinal: 0,
                },
            },
        )?;
        let empty = store.snapshot()?;
        let target = NodeId::new("destination")?;
        edit_reconciled(
            &mut store,
            "destination",
            Command::Insert {
                parent: empty.root().clone(),
                index: 0,
                subtree: Subtree {
                    root: target.clone(),
                    nodes: BTreeMap::from([(
                        target,
                        BeatNode::hold(
                            "Destination",
                            HoldRecipe {
                                picture_context: None,
                                duration: FrameDuration::new(5)?,
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                            },
                        ),
                    )]),
                    overrides: Default::default(),
                    gap_overrides: Default::default(),
                },
            },
        )?;
        let before = store.snapshot()?;
        let cells = authored(&package)?;
        let mut forged = serde_json::to_value(&slice)?;
        forged["nodes"]["hold"]["kind"]["recipe"]["video"]["accepted"]["artifact"]["provenance"] =
            serde_json::to_value(object(b"never accepted"))?;
        let forged: CapturedEditSlice = serde_json::from_value(forged)?;
        let invalid = placement(&before, &forged, "forged", replace)?;
        deadpan_core::apply(&before, &invalid)?;
        assert_eq!(
            store.preview(&invalid).unwrap_err().code(),
            "InvalidCommand"
        );
        assert_eq!(store.commit(&invalid).unwrap_err().code(), "InvalidCommand");
        assert_eq!(store.snapshot()?, before);
        assert_eq!(authored(&package)?, cells);
        let command = placement(&before, &slice, "placed", replace)?;
        let preview = store.preview(&command)?;
        assert_eq!(preview.duration_delta, if replace { 9 } else { 12 });
        assert_eq!(authored(&package)?, cells);
        assert_eq!(store.commit(&command)?.edit, preview);
        let after = store.snapshot()?;
        assert_eq!(after.duration()?.frames(), if replace { 14 } else { 17 });
        assert_eq!(after.assets(), accepted.assets());
        let expected_provider = match &accepted.nodes()[&NodeId::new("hold")?].kind {
            NodeKind::Hold { recipe } => &recipe.video,
            _ => unreachable!(),
        };
        assert!(after.nodes().values().any(|node| {
            matches!(&node.kind, NodeKind::Hold { recipe } if &recipe.video == expected_provider)
        }));
        assert_eq!(
            store
                .generation_request(&input.identity.request_id)?
                .unwrap()
                .relevance,
            deadpan_jobs::Relevance::Detached
        );
        store.undo(after.revision_id(), RevisionId::new("undo-placement")?)?;
        assert_authored(&store.snapshot()?, &before)?;
        store.redo(
            &RevisionId::new("undo-placement")?,
            RevisionId::new("redo-placement")?,
        )?;
        assert_authored(&store.snapshot()?, &after)?;
        store.validate()?;
        drop(store);
        let reader = ProjectStore::open(&package, AccessMode::ReadOnly)?;
        assert_authored(&reader.snapshot()?, &after)?;
        assert_eq!(
            reader
                .generation_request(&input.identity.request_id)?
                .unwrap()
                .relevance,
            deadpan_jobs::Relevance::Detached
        );
        reader.validate()?;
    }
    Ok(())
}
