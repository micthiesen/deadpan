use super::*;
use deadpan_store::registers::{RegisterName, RegisterValue};

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
