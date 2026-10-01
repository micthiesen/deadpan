//! Synthetic admitted bundles test durable acceptance, not decoded video.
use super::*;
use deadpan_core::{
    AudioTimingId, FrameRange, MoveRangeDestination, ProjectFrame, SplitIdentities,
};

fn assert_authored(actual: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = serde_json::to_value(actual.revision_id())?;
    assert_eq!(serde_json::to_value(actual)?, expected);
    Ok(())
}

#[test]
fn moved_accepted_generated_hold_keeps_artifact_and_never_revives_old_request() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("accepted-move.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    let input = ready_for_acceptance(&mut store)?;
    store.accept_generation_bundle(
        &input,
        &unchanged_relevance(&store, &input.new_revision)?,
        media_limits(),
    )?;
    let accepted = store.snapshot()?;
    let hold = NodeId::new("hold")?;
    let group = NodeId::new("destination")?;
    let tail = NodeId::new("tail")?;
    edit_reconciled(
        &mut store,
        "destination",
        Command::Insert {
            parent: accepted.root().clone(),
            index: 1,
            subtree: Subtree {
                root: group.clone(),
                nodes: BTreeMap::from([
                    (
                        group.clone(),
                        BeatNode::sequence("Destination", vec![tail.clone()]),
                    ),
                    (
                        tail,
                        BeatNode::hold(
                            "Tail",
                            HoldRecipe {
                                picture_context: None,
                                duration: FrameDuration::new(5)?,
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                            },
                        ),
                    ),
                ]),
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
    )?;
    // Context relevance is independent from accepted media. Expire the old
    // request while retaining its accepted provider on the same authored Hold.
    let current = store.snapshot()?;
    let next = RevisionId::new("old-request-stale")?;
    let mut relevance = unchanged_relevance(&store, &next)?;
    for observation in &mut relevance.observations {
        observation.after_context = ContextObservation::Unresolved;
    }
    store.commit_reconciled(
        &CommandRequest {
            project_id: current.project_id().clone(),
            expected_revision: current.revision_id().clone(),
            new_revision: next,
            command: Command::Rename {
                node: group.clone(),
                label: "Current destination".into(),
            },
        },
        &relevance,
    )?;
    let retained_request = store
        .generation_request(&input.identity.request_id)?
        .unwrap();
    assert_eq!(retained_request.relevance, deadpan_jobs::Relevance::Stale);
    assert!(store.current_generation_requests()?.is_empty());
    let retained_attempt = store.generation_attempt(&input.identity)?.unwrap();
    let before = store.snapshot()?;
    let destination = MoveRangeDestination::Seam {
        parent: group.clone(),
        index: 1,
    };
    let range = FrameRange::new(ProjectFrame(0), ProjectFrame(12))?;
    let plan = before.range_move(before.root(), range, &destination)?;
    assert_eq!(
        plan.required_ids, 0,
        "whole accepted provider retains its node"
    );
    assert_eq!(
        plan.inserted,
        FrameRange::new(ProjectFrame(5), ProjectFrame(17))?
    );
    let command = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: RevisionId::new("moved")?,
        command: Command::MoveRange {
            source_revision: before.revision_id().clone(),
            source_parent: before.root().clone(),
            range,
            destination,
            identities: SplitIdentities::default(),
            timing: AudioTimingId {
                allocation: RevisionId::new("moved")?,
                ordinal: 0,
            },
        },
    };
    let database = Connection::open(package.join("project.sqlite"))?;
    let counts = || -> Result<(i64, i64)> {
        Ok(database.query_row(
            "SELECT (SELECT count(*) FROM revisions),(SELECT count(*) FROM history)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    };
    let cells = counts()?;
    let preview = store.preview(&command)?;
    assert_eq!(preview.duration_delta, 0);
    assert_eq!(counts()?, cells);
    assert_eq!(
        store.generation_request(&input.identity.request_id)?,
        Some(retained_request.clone())
    );
    assert_eq!(
        store.generation_attempt(&input.identity)?,
        Some(retained_attempt.clone())
    );
    assert_eq!(store.commit(&command)?.edit, preview);
    assert_eq!(counts()?, (cells.0 + 1, cells.1 + 1));
    let after = store.snapshot()?;
    assert_eq!(after.duration()?, before.duration()?);
    assert_eq!(
        after.children(after.root()).cloned().collect::<Vec<_>>(),
        vec![group.clone()]
    );
    assert_eq!(after.children(&group).last(), Some(&hold));
    assert_eq!(after.nodes()[&hold], before.nodes()[&hold]);
    assert_eq!(after.assets(), accepted.assets());
    assert_eq!(
        store.generation_request(&input.identity.request_id)?,
        Some(retained_request.clone())
    );
    store.validate()?;
    drop(store);

    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, after);
    store.undo(after.revision_id(), RevisionId::new("undo-move")?)?;
    let undone = store.snapshot()?;
    assert_authored(&undone, &before)?;
    assert_ne!(undone.revision_id(), before.revision_id());
    assert_eq!(
        store.generation_request(&input.identity.request_id)?,
        Some(retained_request.clone())
    );
    drop(store);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    store.redo(undone.revision_id(), RevisionId::new("redo-move")?)?;
    assert_authored(&store.snapshot()?, &after)?;
    assert_eq!(
        store.generation_request(&input.identity.request_id)?,
        Some(retained_request)
    );
    assert!(store.current_generation_requests()?.is_empty());
    assert_eq!(
        store.generation_attempt(&input.identity)?,
        Some(retained_attempt)
    );
    assert_eq!(counts()?, (cells.0 + 3, cells.1 + 1));
    store.validate()?;
    drop(store);
    ProjectStore::open(&package, AccessMode::ReadOnly)?.validate()?;
    Ok(())
}
