use super::*;
use deadpan_core::{
    AudioTimingId, CapturedEditSlice, CommandRequest, FrameRange, OccurrenceIdentities,
    ProjectFrame, SlicePasteIdentities,
};

fn paste(
    document: &ProjectDocument,
    slice: &CapturedEditSlice,
    name: &str,
) -> Result<CommandRequest> {
    let count = slice.identity_requirements()?;
    assert_eq!(count.marks, 0);
    Ok(CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(name),
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
                allocation: revision(name),
                ordinal: 0,
            },
        },
    })
}

#[test]
fn historical_slice_restores_only_source_metadata_admitted_by_its_stored_revision() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    let initial = store.snapshot()?;
    let registration = request(&store, &original, "registered", "camera", Some("clip"))?;
    store.register_source(&registration, &decoded, None, limits(), &active())?;
    let unrelated = retain(&mut store, "cfr-bframes.mp4")?;
    let unrelated_decoded = decode(&store, &unrelated)?;
    let unrelated_registration = request(&store, &unrelated, "catalog-only", "unselected", None)?;
    store.register_source(
        &unrelated_registration,
        &unrelated_decoded,
        None,
        limits(),
        &active(),
    )?;
    let registered = store.snapshot()?;
    let slice = CapturedEditSlice::capture(
        &registered,
        registered.root(),
        FrameRange::new(ProjectFrame(1), ProjectFrame(9))?,
        AudioTimingId {
            allocation: revision("copy"),
            ordinal: 0,
        },
    )?;
    store.undo(registered.revision_id(), revision("undo-catalog"))?;
    store.undo(&revision("undo-catalog"), revision("undo-registration"))?;
    let before = store.snapshot()?;
    assert!(before.assets().is_empty());
    let saved_counts = counts(&path)?;
    assert_eq!(saved_counts, (5, 2, 2));
    let record = registered.assets()[&id("camera")].clone();
    let add = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision("direct-add"),
        command: Command::AddAsset {
            id: id("camera"),
            asset: record.clone(),
        },
    };
    assert_eq!(
        store.commit(&add).unwrap_err().code(),
        "SourceAdmissionUnavailable"
    );
    for name in ["wrong-receipt", "wrong-history", "unselected-asset"] {
        let mut wire = serde_json::to_value(&slice)?;
        match name {
            "wrong-receipt" => {
                wire["assets"]["camera"]["source_qualification"] =
                    serde_json::json!("f".repeat(64));
            }
            "wrong-history" => wire["revision_id"] = serde_json::json!(initial.revision_id()),
            _ => {
                // This exact record was admitted in the named revision, but it
                // belongs to the independent catalog, not the selected contents.
                wire["assets"]["unselected"] =
                    serde_json::to_value(&registered.assets()[&id("unselected")])?;
            }
        }
        let forged: CapturedEditSlice = serde_json::from_value(wire)?;
        let command = paste(&before, &forged, name)?;
        deadpan_core::apply(&before, &command)?;
        assert_eq!(
            store.preview(&command).unwrap_err().code(),
            "InvalidCommand"
        );
        assert_eq!(store.commit(&command).unwrap_err().code(), "InvalidCommand");
        assert_eq!(store.snapshot()?, before);
        assert_eq!(counts(&path)?, saved_counts);
    }
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let command = paste(&before, &slice, "restored")?;
    let preview = store.preview(&command)?;
    assert_eq!(preview.duration_delta, 8);
    assert_eq!(counts(&path)?, saved_counts);
    assert_eq!(store.commit(&command)?.edit, preview);
    let restored = store.snapshot()?;
    assert_eq!(restored.duration()?.frames(), 8);
    assert_eq!(restored.assets()[&id("camera")], record);
    assert_eq!(restored.assets().len(), 1);
    assert_eq!(counts(&path)?, (6, 3, 2));
    store.validate()?;
    drop(store);
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    let receipt = reader.registered_source(restored.revision_id(), &id("camera"))?;
    assert_eq!(Some(receipt.id()), record.source_qualification.as_ref());
    reader.validate()?;
    Ok(())
}
