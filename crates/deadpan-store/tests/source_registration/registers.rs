use super::*;
use deadpan_store::registers::{RegisterName, RegisterValue};
use std::{collections::BTreeMap, sync::Arc};

#[test]
fn original_register_keeps_qualified_identity_across_undo_reopen_and_checkpoint() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    let register = request(&store, &original, "register", "camera", None)?;
    let outcome = store.register_source(&register, &decoded, None, limits(), &active())?;
    let captured = store.snapshot()?;
    let value = RegisterValue::Original {
        revision: captured.revision_id().clone(),
        asset: id("camera"),
        qualification: outcome.qualification.clone(),
        ordinals: 3..9,
    };
    let counts_before = counts(&path)?;
    let bank = store.save_register(
        captured.project_id(),
        captured.revision_id(),
        RegisterName::new('a')?,
        value.clone(),
    )?;
    assert_eq!(bank.entries[&RegisterName::unnamed()].as_ref(), &value);
    assert_eq!(counts(&path)?, counts_before);
    for ordinals in [0..0, std::ops::Range { start: 9, end: 3 }, 0..u64::MAX] {
        let invalid = RegisterValue::Original {
            ordinals,
            revision: captured.revision_id().clone(),
            asset: id("camera"),
            qualification: outcome.qualification.clone(),
        };
        assert!(
            store
                .save_register(
                    captured.project_id(),
                    captured.revision_id(),
                    RegisterName::new('b')?,
                    invalid
                )
                .is_err()
        );
        assert_eq!(store.registers()?, bank);
    }
    let invalid = RegisterValue::Original {
        revision: captured.revision_id().clone(),
        asset: id("camera"),
        qualification: SourceQualificationId::new("f".repeat(64))?,
        ordinals: 3..9,
    };
    assert!(
        store
            .save_register(
                captured.project_id(),
                captured.revision_id(),
                RegisterName::new('b')?,
                invalid
            )
            .is_err()
    );
    store.undo(captured.revision_id(), revision("undo-registration"))?;
    assert!(store.snapshot()?.assets().is_empty());
    assert_eq!(store.registers()?, bank);
    let current = store.snapshot()?;
    let second = store.save_register(
        current.project_id(),
        current.revision_id(),
        RegisterName::new('b')?,
        value,
    )?;
    assert_eq!(
        second.entries[&RegisterName::new('a')?],
        second.entries[&RegisterName::new('b')?]
    );
    store.checkpoint()?;
    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(reopened.registers()?, second);
    assert!(reopened.snapshot()?.assets().is_empty());
    Ok(())
}

#[test]
fn compound_original_paste_checks_exact_ordinals_and_retains_intermediate_yank() -> Result {
    use deadpan_core::{AudioTimingId, LeafEdit, ResolvedStep, ResolvedTransaction};
    use deadpan_media::source_import_timing::derive_source_moment;

    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    let register = request(&store, &original, "register", "camera", None)?;
    let registered = store.register_source(&register, &decoded, None, limits(), &active())?;
    let before = store.snapshot()?;
    let value = RegisterValue::Original {
        revision: before.revision_id().clone(),
        asset: id("camera"),
        qualification: registered.qualification.clone(),
        ordinals: 3..9,
    };
    let a = RegisterName::new('a')?;
    let b = RegisterName::new('b')?;
    let bank = store.save_register(before.project_id(), before.revision_id(), a, value)?;
    let receipt = store.source_qualification(&registered.qualification)?;
    let source = |ordinals| -> Result<deadpan_core::SourceNode> {
        Ok(derive_source_moment(
            receipt.snapshot().video().unwrap().index(),
            receipt.snapshot().audio(),
            ordinals,
            before.presentation_basis().frame_rate,
        )?
        .source_node(id("camera")))
    };
    let expected = source(3..9)?;
    let make = |source| -> Result<CommandRequest> {
        Ok(CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision("compound"),
            command: Command::Compound {
                transaction: ResolvedTransaction::new(
                    bank.version,
                    BTreeMap::from([(a, Some(Arc::clone(&bank.entries[&a])))]),
                    vec![
                        ResolvedStep::Paste {
                            name: a,
                            edit: LeafEdit::new(
                                revision("paste-stage"),
                                Command::SpliceSource {
                                    parent: before.root().clone(),
                                    index: 0,
                                    source,
                                    id: NodeId::new("pasted")?,
                                    label: "Exact Original".into(),
                                    timing: AudioTimingId {
                                        allocation: revision("paste-stage"),
                                        ordinal: 0,
                                    },
                                },
                            )?,
                        },
                        ResolvedStep::Yank {
                            name: b,
                            value: Arc::new(RegisterValue::Original {
                                revision: revision("paste-stage"),
                                asset: id("camera"),
                                qualification: registered.qualification.clone(),
                                ordinals: 3..9,
                            }),
                        },
                    ],
                )?,
            },
        })
    };
    let counts_before = counts(&path)?;
    let wrong = make(source(4..10)?)?;
    // The same source and frame count are insufficient: the measured interval
    // must agree with the frozen register's exact half-open ordinal selection.
    deadpan_core::apply(&before, &wrong)?;
    for error in [
        store.preview(&wrong).unwrap_err(),
        store.commit(&wrong).unwrap_err(),
    ] {
        assert!(
            error.to_string().contains("qualified register ordinals"),
            "{error}"
        );
    }
    assert_eq!(store.snapshot()?, before);
    assert_eq!(store.registers()?, bank);
    assert_eq!(counts(&path)?, counts_before);
    let db = Connection::open(path.join("project.sqlite"))?;
    assert_eq!(
        db.query_row("SELECT count(*) FROM transaction_steps", [], |row| row
            .get::<_, i64>(0))?,
        0
    );
    drop(db);

    let good = make(expected.clone())?;
    let preview = store.preview_compound(&good)?;
    let saved = store.commit(&good)?;
    assert_eq!(saved.edit, preview.edit.unwrap());
    let saved_bank = saved.register_bank.unwrap();
    assert_eq!(saved_bank.version, bank.version + 1);
    let after = store.snapshot()?;
    let NodeKind::Source { source: actual } = &after.nodes()[&NodeId::new("pasted")?].kind else {
        panic!("expected Original Source")
    };
    assert_eq!(actual, &expected);
    assert_eq!(
        saved_bank.entries[&b].capture_revision(),
        Some(&revision("paste-stage"))
    );
    assert!(store.snapshot_at(&revision("paste-stage")).is_err());
    assert_eq!(
        store
            .capture_snapshot_at(&revision("paste-stage"))?
            .assets(),
        before.assets()
    );
    store.undo(after.revision_id(), revision("undone"))?;
    assert_eq!(store.registers()?, saved_bank);
    store.checkpoint()?;
    drop(store);
    let reopened = ProjectStore::open(&path, AccessMode::ReadOnly)?;
    assert_eq!(reopened.registers()?, saved_bank);
    assert_eq!(
        reopened
            .capture_snapshot_at(&revision("paste-stage"))?
            .assets(),
        before.assets()
    );
    Ok(())
}
