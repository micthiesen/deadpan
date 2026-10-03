use super::*;
use deadpan_core::*;
fn request(doc: &ProjectDocument, revision: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: doc.project_id().clone(),
        expected_revision: doc.revision_id().clone(),
        new_revision: RevisionId::new(revision).unwrap(),
        command,
    }
}

fn fixture(path: &Path) -> Result<ProjectDocument> {
    let initial = ProjectDocument::new(
        ProjectId::new("framing-store")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    let mut store = ProjectStore::create(path, &initial)?;
    for (revision, at, length, name) in
        [("pause-one", 0, 8, "hold"), ("pause-two", 3, 2, "inserted")]
    {
        let before = store.snapshot()?;
        store.commit(&request(
            &before,
            revision,
            Command::InsertTime {
                at: ProjectFrame(at),
                hold: HoldRecipe {
                    picture_context: None,
                    duration: FrameDuration::new(length)?,
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                },
                id: NodeId::new(name)?,
                identities: SplitIdentities {
                    nodes: (0..before.nodes().len() + 4)
                        .map(|i| NodeId::new(format!("{revision}-copy-{i}")).unwrap())
                        .collect(),
                },
                timing: AudioTimingId {
                    allocation: RevisionId::new(revision)?,
                    ordinal: 0,
                },
            },
        ))?;
    }
    store.commit(&request(
        &store.snapshot()?,
        "rename",
        Command::Rename {
            node: NodeId::new("inserted")?,
            label: "Retained pause".into(),
        },
    ))?;
    store.undo(&RevisionId::new("rename")?, RevisionId::new("undo")?)?;
    store.redo(&RevisionId::new("undo")?, RevisionId::new("redo")?)?;
    store.undo(&RevisionId::new("redo")?, RevisionId::new("pending")?)?;
    let current = store.snapshot()?;
    store.validate()?;
    drop(store);
    Ok(current)
}

fn framing() -> Framing {
    Framing::creep(
        FramingPose::identity(),
        FramingPose::new(
            ExactRatio::new(1, 3).unwrap(),
            ExactRatio::new(2, 3).unwrap(),
            ExactRatio::new(27, 20).unwrap(),
        )
        .unwrap(),
        FramingCurve::Smoothstep,
    )
    .unwrap()
}

#[test]
fn current_history_keeps_insert_time_redo_and_persists_framing() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("framing.deadpan");
    let expected = fixture(&path)?;
    let db = Connection::open(path.join("project.sqlite"))?;
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, expected);
    store.redo(expected.revision_id(), RevisionId::new("redo-migrated")?)?;
    let base = store.snapshot()?;
    let change = request(
        &base,
        "framing",
        Command::SetFraming {
            node: NodeId::new("inserted")?,
            framing: Some(framing()),
        },
    );
    store.commit(&change)?;
    let framed = store.snapshot()?;
    assert_eq!(framed.audio_bindings(), base.audio_bindings());
    assert_eq!(
        framed.nodes()[&NodeId::new("inserted")?].framing,
        Some(framing())
    );
    let count = docs(&db)?.len();
    assert!(store.commit(&change).is_err());
    assert_eq!(docs(&db)?.len(), count);
    store.undo(framed.revision_id(), RevisionId::new("undo-framing")?)?;
    assert_eq!(store.snapshot()?.nodes(), base.nodes());
    store.redo(
        &RevisionId::new("undo-framing")?,
        RevisionId::new("redo-framing")?,
    )?;
    store.validate()?;
    let final_doc = store.snapshot()?;
    drop(store);
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)?.snapshot()?,
        final_doc
    );
    Ok(())
}
