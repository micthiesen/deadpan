use super::*;

fn cells(store: &ProjectStore) -> Vec<String> {
    let mut result = timeline(store);
    for query in [
        "SELECT json_array(singleton,version) FROM register_state",
        "SELECT json_array(name,content_id) FROM registers ORDER BY name",
        "SELECT json_array(id,capture_revision,capture_step,value) FROM register_contents ORDER BY id",
        "SELECT json_array(owner_revision,ordinal,step_revision,document) FROM transaction_steps ORDER BY owner_revision,ordinal",
    ] {
        result.extend(
            store
                .connection
                .prepare(query)
                .unwrap()
                .query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .map(Result::unwrap),
        );
    }
    result
}

#[test]
fn readonly_macro_preview_matches_save_without_changing_default_history_or_redo() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("macro-preview.deadpan");
    let initial = document();
    let mut writer = ProjectStore::create(&path, &initial).unwrap();
    writer.commit(&cut(&initial, "last", "cut")).unwrap();
    writer.undo(&revision("cut"), revision("undone")).unwrap();
    let bank = save(&mut writer, 'a', capture(&initial, "first"));
    let reader = ProjectStore::open(&path, crate::AccessMode::ReadOnly).unwrap();
    let (document, inspected) = reader.snapshot_with_registers().unwrap();
    assert_eq!(inspected, bank);
    assert_eq!(document, writer.snapshot().unwrap());
    let before = cells(&writer);
    let changes = reader.connection.total_changes();
    let preview = reader
        .preview_macro(
            document.project_id(),
            document.revision_id(),
            bank.version,
            name('a'),
            macro_program(),
        )
        .unwrap();
    assert_eq!(preview.version, bank.version + 1);
    assert_eq!(preview.entries[&name('"')], bank.entries[&name('"')]);
    assert!(matches!(
        preview.entries[&name('a')].as_ref(),
        RegisterValue::Macro { .. }
    ));
    assert_eq!(reader.connection.total_changes(), changes);
    assert!(reader.connection.is_autocommit());
    assert_eq!(cells(&writer), before);
    assert_eq!(reader.history_availability().unwrap(), (false, true));
    assert_eq!(
        reader.snapshot_with_registers().unwrap(),
        (document.clone(), bank)
    );
    let saved = writer
        .save_macro(
            document.project_id(),
            document.revision_id(),
            inspected.version,
            name('a'),
            macro_program(),
        )
        .unwrap();
    assert_eq!(saved, preview);
    assert_eq!(reader.snapshot_with_registers().unwrap(), (document, saved));
    writer
        .redo(&revision("undone"), revision("redone"))
        .unwrap();
}

#[test]
fn preview_and_save_reject_the_same_stale_context_names_and_exhausted_version_without_writes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("macro-preview-refusal.deadpan");
    let initial = document();
    let mut writer = ProjectStore::create(&path, &initial).unwrap();
    let bank = save(&mut writer, 'a', capture(&initial, "first"));
    let reader = ProjectStore::open(&path, crate::AccessMode::ReadOnly).unwrap();
    for (project, revision, version, name) in [
        (
            ProjectId::new("other").unwrap(),
            initial.revision_id().clone(),
            bank.version,
            name('a'),
        ),
        (
            initial.project_id().clone(),
            revision("stale"),
            bank.version,
            name('a'),
        ),
        (
            initial.project_id().clone(),
            initial.revision_id().clone(),
            0,
            name('a'),
        ),
        (
            initial.project_id().clone(),
            initial.revision_id().clone(),
            bank.version,
            RegisterName::unnamed(),
        ),
    ] {
        let before = cells(&writer);
        let preview = reader
            .preview_macro(&project, &revision, version, name, macro_program())
            .unwrap_err();
        let saved = writer
            .save_macro(&project, &revision, version, name, macro_program())
            .unwrap_err();
        assert_eq!(preview.to_string(), saved.to_string());
        assert_eq!(cells(&writer), before);
    }
    writer
        .connection
        .execute("UPDATE register_state SET version=?1", [i64::MAX])
        .unwrap();
    crate::registers::reseal_for_test(&writer.connection).unwrap();
    let before = cells(&writer);
    let preview = reader
        .preview_macro(
            initial.project_id(),
            initial.revision_id(),
            i64::MAX as u64,
            name('a'),
            macro_program(),
        )
        .unwrap_err();
    let saved = writer
        .save_macro(
            initial.project_id(),
            initial.revision_id(),
            i64::MAX as u64,
            name('a'),
            macro_program(),
        )
        .unwrap_err();
    assert_eq!(preview.to_string(), saved.to_string());
    assert!(preview.to_string().contains("versions are exhausted"));
    assert_eq!(cells(&writer), before);
}

#[test]
fn snapshot_and_macro_preview_reject_an_over_capacity_bank_before_deserialization_or_writes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("macro-preview-capacity.deadpan");
    let initial = document();
    let mut writer = ProjectStore::create(&path, &initial).unwrap();
    save(&mut writer, 'a', capture(&initial, "first"));
    let bank = save(&mut writer, 'b', capture(&initial, "last"));
    let reader = ProjectStore::open(&path, crate::AccessMode::ReadOnly).unwrap();
    writer
        .connection
        .pragma_update(None, "ignore_check_constraints", true)
        .unwrap();
    writer
        .connection
        .execute(
            "UPDATE register_contents SET value=CAST(zeroblob(?1) AS TEXT)",
            [(MAX_REGISTER_BYTES / 2 + 1) as i64],
        )
        .unwrap();
    let changes = writer.connection.total_changes();
    let preview = reader
        .preview_macro(
            initial.project_id(),
            initial.revision_id(),
            bank.version,
            name('c'),
            macro_program(),
        )
        .unwrap_err();
    let saved = writer
        .save_macro(
            initial.project_id(),
            initial.revision_id(),
            bank.version,
            name('c'),
            macro_program(),
        )
        .unwrap_err();
    assert_eq!(preview.to_string(), saved.to_string());
    assert!(preview.to_string().contains("aggregate 64 MiB"));
    assert!(
        reader
            .snapshot_with_registers()
            .unwrap_err()
            .to_string()
            .contains("aggregate 64 MiB")
    );
    assert_eq!(writer.connection.total_changes(), changes);
    assert_eq!(writer.register_version().unwrap(), bank.version);
    assert_eq!(writer.snapshot().unwrap(), initial);
}

#[test]
fn readonly_snapshot_pairs_document_and_bank_during_atomic_compound_writes() {
    use std::sync::mpsc;
    use std::time::Duration;

    const COMMITS: u64 = 16;
    const WAIT: Duration = Duration::from_secs(20);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("coherent-registers.deadpan");
    drop(ProjectStore::create(&path, &document()).unwrap());
    let reader = ProjectStore::open(&path, crate::AccessMode::ReadOnly).unwrap();
    let (start, requests) = mpsc::sync_channel(1);
    let (finished, replies) = mpsc::sync_channel(1);
    let writer = std::thread::spawn(move || {
        let mut writer = ProjectStore::open(&path, crate::AccessMode::ReadWrite).unwrap();
        for version in 1..=COMMITS {
            if requests.recv_timeout(WAIT).is_err() {
                return;
            }
            let current = writer.snapshot().unwrap();
            let transaction = ResolvedTransaction::new(
                version - 1,
                BTreeMap::new(),
                vec![
                    ResolvedStep::Yank {
                        name: name('a'),
                        value: Arc::new(RegisterValue::Edited {
                            slice: capture(&current, "first"),
                        }),
                    },
                    ResolvedStep::Edit {
                        edit: LeafEdit::new(
                            revision(&format!("coherent-leaf-{version}")),
                            Command::Rename {
                                node: node("first"),
                                label: format!("First {version}"),
                            },
                        )
                        .unwrap(),
                    },
                ],
            )
            .unwrap();
            writer
                .commit_compound(
                    &CommandRequest {
                        project_id: current.project_id().clone(),
                        expected_revision: current.revision_id().clone(),
                        new_revision: revision(&format!("coherent-{version}")),
                        command: Command::Compound { transaction },
                    },
                    None,
                )
                .unwrap();
            if finished.send(version).is_err() {
                return;
            }
        }
    });
    for version in 1..=COMMITS {
        start.send(()).unwrap();
        let (snapshot, bank) = reader.snapshot_with_registers().unwrap();
        let expected = if bank.version == 0 {
            revision("initial")
        } else {
            revision(&format!("coherent-{}", bank.version))
        };
        assert_eq!(snapshot.revision_id(), &expected);
        assert_eq!(replies.recv_timeout(WAIT).unwrap(), version);
        let (snapshot, bank) = reader.snapshot_with_registers().unwrap();
        assert_eq!(bank.version, version);
        assert_eq!(
            snapshot.revision_id(),
            &revision(&format!("coherent-{version}"))
        );
    }
    writer.join().unwrap();
}
