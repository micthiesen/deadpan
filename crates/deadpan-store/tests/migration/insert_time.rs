use super::*;
use deadpan_core::*;
use std::collections::BTreeMap;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn bound_initial() -> Result<ProjectDocument> {
    let empty = ProjectDocument::new(
        ProjectId::new("pause-project")?,
        RevisionId::new("initial")?,
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
        (node("root"), BeatNode::sequence("Root", vec![node("hold")])),
        (
            node("hold"),
            BeatNode::hold(
                "Original hold",
                HoldRecipe {
                    picture_context: None,
                    duration: FrameDuration::new(6)?,
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                },
            ),
        ),
    ]))?;
    let unbound = ProjectDocument::from_json(&wire.to_string())?;
    let captured = capture_unbound_audio_bindings(
        &unbound,
        AudioTimingId {
            allocation: RevisionId::new("old-timing")?,
            ordinal: 0,
        },
    )?;
    wire["audio_bindings"] = serde_json::to_value(&captured)?;
    wire["audio_bindings"]["bindings"]["hold"]["resume"] = serde_json::to_value(AudioResume {
        local_boundary: ExactRatio::ONE,
        phase: AudioLocalPhase {
            constant: ExactRatio::new(1, 7)?,
            terms: vec![AudioPhaseTerm {
                placement: captured.bindings()[&node("hold")].lattice.clone(),
                from_local: ExactRatio::ZERO,
                to_local: ExactRatio::ONE,
            }],
        },
    })?;
    Ok(ProjectDocument::from_json(&wire.to_string())?)
}

fn command(document: &ProjectDocument, revision: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(revision).unwrap(),
        command,
    }
}

fn pause(document: &ProjectDocument, revision: &str, at: i64, frames: i64) -> CommandRequest {
    command(
        document,
        revision,
        Command::InsertTime {
            at: ProjectFrame(at),
            hold: HoldRecipe {
                picture_context: None,
                duration: FrameDuration::new(frames).unwrap(),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            },
            id: node(&format!("{revision}-hold")),
            identities: SplitIdentities {
                nodes: (0..document.nodes().len() + 4)
                    .map(|i| node(&format!("{revision}-split-{i}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: RevisionId::new(revision).unwrap(),
                ordinal: 0,
            },
        },
    )
}

#[test]
fn atomic_insert_time_history_survives_reopen_undo_redo_and_rejected_requests() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("insert-time.deadpan");
    let initial = bound_initial()?;
    let mut store = ProjectStore::create(&path, &initial)?;
    let request = pause(&initial, "first-pause", 2, 1);
    let preview = store.preview(&request)?;
    assert_eq!(preview.duration_delta, 1);
    assert_eq!(store.snapshot()?, initial);
    store.commit(&request)?;
    let first = store.snapshot()?;
    assert_eq!(first.duration()?.frames(), 7);
    assert_eq!(first, preview.forward.apply(&initial)?);
    assert!(first.audio_bindings().timings().len() > initial.audio_bindings().timings().len());
    store.commit(&pause(&first, "second-pause", 4, 2))?;
    let second = store.snapshot()?;
    let database = Connection::open(path.join("project.sqlite"))?;
    let stable = contents(&database)?;
    for rejected in [request, pause(&second, "zero-pause", 0, 0)] {
        assert!(store.preview(&rejected).is_err());
        assert!(store.commit(&rejected).is_err());
        assert_eq!(store.snapshot()?, second);
        assert_eq!(contents(&database)?, stable);
    }
    assert_eq!(history_json(&database)?.len(), 2);
    assert_eq!(docs(&database)?.len(), 3);
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.snapshot()?, second);
    store.undo(second.revision_id(), RevisionId::new("undo-second")?)?;
    let undone = store.snapshot()?;
    assert_eq!(undone.nodes(), first.nodes());
    assert_eq!(undone.audio_bindings(), first.audio_bindings());
    assert_ne!(undone.revision_id(), first.revision_id());
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    store.redo(undone.revision_id(), RevisionId::new("redo-second")?)?;
    let redone = store.snapshot()?;
    assert_eq!(redone.nodes(), second.nodes());
    assert_eq!(redone.audio_bindings(), second.audio_bindings());
    assert_ne!(redone.revision_id(), second.revision_id());
    store.validate()?;
    drop(store);
    assert_eq!(
        ProjectStore::open(&path, AccessMode::ReadOnly)?.snapshot()?,
        redone
    );
    Ok(())
}
