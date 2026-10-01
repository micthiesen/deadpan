use super::*;
use deadpan_store::slice_preview::SliceViewIdentities;
use std::sync::atomic::AtomicBool;

#[test]
fn move_preview_seals_only_a_current_validated_command_and_leaves_history_unchanged() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("move-view.deadpan");
    let baseline = document()?;
    let mut store = ProjectStore::create(&path, &baseline)?;
    let selected = FrameRange::new(ProjectFrame(2), ProjectFrame(17))?;
    let command = request(
        &baseline,
        "move-view",
        Command::MoveRange {
            source_revision: baseline.revision_id().clone(),
            source_parent: node("root"),
            range: selected,
            destination: MoveRangeDestination::Seam {
                parent: node("root"),
                index: 3,
            },
            identities: SplitIdentities { nodes: vec![] },
            timing: timing("move-view"),
        },
    );
    let cells = authored(&path)?;
    let admitted = store.preview_edit_slice(&command)?;
    assert_eq!(admitted.capture_revision(), baseline.revision_id());
    assert_eq!(
        admitted.placement_base().map(AsRef::as_ref),
        Some(&baseline)
    );
    assert_eq!(
        **admitted.document(),
        store.preview(&command)?.forward.apply(&baseline)?
    );
    assert_eq!(admitted.document().duration()?, baseline.duration()?);
    assert_eq!(
        admitted.document().nodes(),
        store.preview(&command)?.forward.apply(&baseline)?.nodes()
    );
    assert_eq!(authored(&path)?, cells);
    for kind in 0..4 {
        let mut forged = command.clone();
        let Command::MoveRange {
            source_revision,
            destination,
            identities,
            ..
        } = &mut forged.command
        else {
            unreachable!()
        };
        let expected = match kind {
            0 => {
                *source_revision = revision("stale-source");
                "RevisionConflict"
            }
            1 => {
                forged.expected_revision = revision("stale-destination");
                "RevisionConflict"
            }
            2 => {
                identities.nodes.push(node("root"));
                "IdentityConflict"
            }
            _ => {
                *destination = MoveRangeDestination::Interior {
                    parent: node("group"),
                    target: node("voice"),
                    at: FrameDuration::new(3)?,
                };
                "InvalidCommand"
            }
        };
        assert_eq!(
            store.preview_edit_slice(&forged).err().unwrap().code(),
            expected
        );
        assert_eq!(authored(&path)?, cells);
    }
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(
        reader.preview_edit_slice(&command).err().unwrap().code(),
        "ProjectReadOnly"
    );
    drop(reader);
    store.commit(&command)?;
    assert_eq!(store.snapshot()?, **admitted.document());
    let committed = store.snapshot()?;
    store.undo(committed.revision_id(), revision("undo-view"))?;
    let restored = store.snapshot()?;
    assert_authored(&restored, &baseline)?;
    let mut stale = command;
    stale.expected_revision = restored.revision_id().clone();
    stale.new_revision = revision("after-undo");
    if let Command::MoveRange { timing, .. } = &mut stale.command {
        timing.allocation = stale.new_revision.clone();
    }
    assert_eq!(
        store.preview_edit_slice(&stale).err().unwrap().code(),
        "RevisionConflict"
    );
    drop(store);
    assert!(admitted.check_live(&AtomicBool::new(false)).is_err());
    Ok(())
}

fn view_ids(
    document: &ProjectDocument,
    slice: &CapturedEditSlice,
    name: &str,
) -> Result<SliceViewIdentities> {
    let Command::SpliceSlice { identities, .. } = paste(document, slice, name, 0)?.command else {
        unreachable!()
    };
    Ok(SliceViewIdentities {
        empty_revision: revision(&format!("{name}-empty")),
        view_revision: revision(name),
        root: node(&format!("{name}-root")),
        paste: identities,
    })
}

#[test]
fn standalone_view_preserves_only_owned_content_without_history_and_revokes_on_close() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("view.deadpan");
    let baseline = document()?;
    let store = ProjectStore::create(&path, &baseline)?;
    let slice = capture(&baseline)?;
    let before = authored(&path)?;
    let view = store.view_edit_slice(&slice, view_ids(&baseline, &slice, "view")?)?;
    assert!(view.placement_base().is_none());
    assert_eq!(view.capture_revision(), baseline.revision_id());
    assert_eq!(view.document().duration()?.frames(), 14);
    assert!(view.document().sounds().is_empty());
    assert!(view.document().sound_routes().is_empty());
    assert!(view.sources().is_empty());
    assert_eq!(view.document().marks().len(), 1);
    assert!(
        !view
            .document()
            .nodes()
            .values()
            .any(|node| matches!(node.label.as_str(), "Lead" | "Tail" | "Group"))
    );
    let root = &view.document().nodes()[view.document().root()];
    assert_eq!(root.framing, None);
    assert_eq!(root.audio_treatments, Default::default());
    let active = AtomicBool::new(false);
    let old_handle = store.original_import_handle()?;
    assert!(view.matches_originals(&old_handle));
    view.check_live(&active)?;
    assert_eq!(store.snapshot()?, baseline);
    assert_eq!(authored(&path)?, before);
    drop(store);
    assert!(
        view.matches_originals(&old_handle),
        "identity does not establish liveness"
    );
    assert!(view.check_live(&active).is_err());
    assert!(view.generated().check_live(&active).is_err());
    let reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert!(!view.matches_originals(&reopened.original_import_handle()?));
    assert_eq!(authored(&path)?, before);
    Ok(())
}

#[test]
fn all_placement_factories_seal_the_exact_preview_and_base_without_writes() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("placements.deadpan");
    let baseline = document()?;
    let store = ProjectStore::create(&path, &baseline)?;
    let slice = capture(&baseline)?;
    let before = authored(&path)?;
    for mode in 0..3 {
        let name = format!("placement-{mode}");
        let mut command = paste(&baseline, &slice, &name, 0)?;
        if mode != 0 {
            let Command::SpliceSlice {
                parent,
                slice,
                identities,
                timing,
                ..
            } = command.command
            else {
                unreachable!()
            };
            let range = FrameRange::new(ProjectFrame(0), ProjectFrame(1))?;
            let at = FrameDuration::new(1)?;
            let required = if mode == 1 {
                baseline
                    .slice_splice_interior(&parent, &node("lead"), at, &slice)?
                    .required_ids
            } else {
                baseline
                    .slice_replacement(&parent, range, &slice)?
                    .required_ids
            };
            let split_identities = SplitIdentities {
                nodes: (0..required)
                    .map(|i| node(&format!("{name}-split-{i}")))
                    .collect(),
            };
            command.command = if mode == 1 {
                Command::SpliceSliceAt {
                    parent,
                    target: node("lead"),
                    at,
                    slice,
                    identities,
                    split_identities,
                    timing,
                }
            } else {
                Command::ReplaceSlice {
                    parent,
                    range,
                    slice,
                    identities,
                    split_identities,
                    timing,
                }
            };
        }
        let view = store.preview_edit_slice(&command)?;
        assert_eq!(view.placement_base().map(AsRef::as_ref), Some(&baseline));
        assert_eq!(
            **view.document(),
            store.preview(&command)?.forward.apply(&baseline)?
        );
        assert_eq!(authored(&path)?, before);
        assert_eq!(store.snapshot()?, baseline);
    }
    Ok(())
}

#[test]
fn factories_reject_non_slice_forged_capture_reused_ids_and_read_only_owner() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("rejections.deadpan");
    let baseline = document()?;
    let store = ProjectStore::create(&path, &baseline)?;
    let slice = capture(&baseline)?;
    let cells = authored(&path)?;
    let other = request(
        &baseline,
        "rename",
        Command::Rename {
            node: node("voice"),
            label: "new".into(),
        },
    );
    assert_eq!(
        store.preview_edit_slice(&other).err().unwrap().code(),
        "InvalidCommand"
    );
    let mut wire = serde_json::to_value(&slice)?;
    wire["nodes"]["voice"]["label"] = serde_json::json!("forged");
    let forged: CapturedEditSlice = serde_json::from_value(wire)?;
    assert_eq!(
        store
            .view_edit_slice(&forged, view_ids(&baseline, &forged, "forged")?)
            .err()
            .unwrap()
            .code(),
        "InvalidCommand"
    );
    let mut ids = view_ids(&baseline, &slice, "reuse")?;
    ids.empty_revision = baseline.revision_id().clone();
    assert_eq!(
        store.view_edit_slice(&slice, ids).err().unwrap().code(),
        "RevisionReused"
    );
    let reader = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(
        reader
            .view_edit_slice(&slice, view_ids(&baseline, &slice, "reader")?)
            .err()
            .unwrap()
            .code(),
        "ProjectReadOnly"
    );
    assert_eq!(authored(&path)?, cells);
    Ok(())
}
