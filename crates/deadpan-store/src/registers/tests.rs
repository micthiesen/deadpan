use super::*;
use deadpan_core::*;

mod children;
mod macro_preview;

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn timing(value: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: revision(value),
        ordinal: 0,
    }
}
fn name(value: char) -> RegisterName {
    RegisterName::new(value).unwrap()
}

fn document() -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("register-project").unwrap(),
        revision("initial"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1_001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let hold = |frames| {
        BeatNode::hold(
            "Pause",
            HoldRecipe {
                duration: FrameDuration::new(frames).unwrap(),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
                picture_context: None,
            },
        )
    };
    let mut json = serde_json::to_value(empty).unwrap();
    json["nodes"] = serde_json::to_value(BTreeMap::from([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("first"), node("empty"), node("last")]),
        ),
        (node("first"), hold(8)),
        (node("empty"), BeatNode::sequence("Empty", Vec::new())),
        (node("last"), hold(4)),
    ]))
    .unwrap();
    ProjectDocument::from_json(&json.to_string()).unwrap()
}
fn capture(doc: &ProjectDocument, child: &str) -> Arc<CapturedEditSlice> {
    Arc::new(
        CapturedEditSlice::capture_selection(
            doc,
            &node("root"),
            &SliceCaptureSelection::Child { node: node(child) },
            timing("copy"),
        )
        .unwrap(),
    )
}
fn save(store: &mut ProjectStore, slot: char, slice: Arc<CapturedEditSlice>) -> RegisterBank {
    let head = store.snapshot().unwrap();
    store
        .save_register(
            head.project_id(),
            head.revision_id(),
            name(slot),
            RegisterValue::Edited { slice },
        )
        .unwrap()
}
fn cut(doc: &ProjectDocument, child: &str, next: &str) -> CommandRequest {
    CommandRequest {
        project_id: doc.project_id().clone(),
        expected_revision: doc.revision_id().clone(),
        new_revision: revision(next),
        command: Command::DeleteRipple {
            node: node(child),
            timing: timing(next),
        },
    }
}
fn timeline(store: &ProjectStore) -> Vec<String> {
    let mut result = Vec::new();
    for query in [
        "SELECT json_array(id,parent_id,kind,document) FROM revisions ORDER BY id",
        "SELECT json_array(id,parent_id,revision_id,request,edit) FROM history ORDER BY id",
        "SELECT json_array(singleton,head_revision,cursor,workflow) FROM state",
        "SELECT json_array(position,history_id) FROM redo ORDER BY position",
    ] {
        result.extend(
            store
                .connection
                .prepare(query)
                .unwrap()
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .map(Result::unwrap),
        );
    }
    result
}

#[test]
fn named_copy_reopens_with_shared_default_and_preserves_timeline_redo() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("register.deadpan");
    let initial = document();
    let mut store = ProjectStore::create(&path, &initial).unwrap();
    store.commit(&cut(&initial, "last", "edit")).unwrap();
    store.undo(&revision("edit"), revision("undone")).unwrap();
    let head = store.snapshot().unwrap();
    let before = timeline(&store);
    let first = capture(&initial, "first");
    let a = save(&mut store, 'a', Arc::clone(&first));
    assert_eq!(a.version, 1);
    assert!(Arc::ptr_eq(&a.entries[&name('a')], &a.entries[&name('"')]));
    let b = save(&mut store, 'b', capture(&head, "last"));
    assert_eq!(b.version, 2);
    assert_eq!(b.entries[&name('a')], a.entries[&name('a')]);
    assert_eq!(timeline(&store), before);
    assert_eq!(store.history_availability().unwrap(), (false, true));
    assert_eq!(store.register_version().unwrap(), 2);
    let checkpoint = store.checkpoint().unwrap();
    let connection = Connection::open(checkpoint).unwrap();
    assert_eq!(read_bank(&connection).unwrap(), b);
    drop(store);
    let mut store = ProjectStore::open(&path, crate::AccessMode::ReadWrite).unwrap();
    let reopened = store.registers().unwrap();
    assert_eq!(reopened, b);
    assert!(Arc::ptr_eq(
        &reopened.entries[&name('b')],
        &reopened.entries[&name('"')]
    ));
    store.redo(&revision("undone"), revision("redone")).unwrap();
    assert_eq!(store.registers().unwrap(), b);
}

#[test]
fn whole_child_cut_is_atomic_and_undo_keeps_copy_including_empty_children() {
    for child in ["first", "empty"] {
        let directory = tempfile::tempdir().unwrap();
        let initial = document();
        let mut store =
            ProjectStore::create(&directory.path().join("cut.deadpan"), &initial).unwrap();
        let slice = capture(&initial, child);
        let (outcome, bank) = store
            .cut_to_register(
                &cut(&initial, child, "cut"),
                name('c'),
                Arc::clone(&slice),
                None,
            )
            .unwrap();
        assert_eq!(
            bank.entries[&name('c')].as_ref(),
            &RegisterValue::Edited { slice }
        );
        assert!(!store.snapshot().unwrap().nodes().contains_key(&node(child)));
        assert_eq!(outcome.revision_id, revision("cut"));
        store.undo(&revision("cut"), revision("undo-cut")).unwrap();
        assert!(store.snapshot().unwrap().nodes().contains_key(&node(child)));
        assert_eq!(store.registers().unwrap(), bank);
        store.validate().unwrap();
    }
}

#[test]
fn duplicate_names_and_content_ids_fail_before_loading_a_bank() {
    for duplicate_contents in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("duplicate.deadpan");
        let initial = document();
        let mut store = ProjectStore::create(&path, &initial).unwrap();
        save(&mut store, 'a', capture(&initial, "first"));
        save(&mut store, 'b', capture(&initial, "last"));
        // A hostile package can rebuild a table without its uniqueness
        // constraint while preserving valid values, references and hashes.
        store
            .connection
            .execute_batch(if duplicate_contents {
                "PRAGMA foreign_keys=OFF;
            CREATE TABLE contents_copy AS SELECT * FROM register_contents;
            DROP TABLE register_contents;
            ALTER TABLE contents_copy RENAME TO register_contents;
            INSERT INTO register_contents SELECT * FROM register_contents LIMIT 1;
            PRAGMA foreign_keys=ON;"
            } else {
                "PRAGMA foreign_keys=OFF;
            CREATE TABLE slots_copy AS SELECT * FROM registers;
            DROP TABLE registers;
            ALTER TABLE slots_copy RENAME TO registers;
            INSERT INTO registers SELECT 'a',content_id FROM registers WHERE name='b';
            PRAGMA foreign_keys=ON;"
            })
            .unwrap();
        let error = check_stored_sizes(&store.connection).unwrap_err();
        assert!(error.to_string().contains("duplicate register"), "{error}");
        assert!(store.registers().is_err());
        assert!(store.checkpoint().is_err());
        drop(store);
        assert!(ProjectStore::open(&path, crate::AccessMode::ReadOnly).is_err());
    }
}

#[test]
fn selected_range_cut_matches_exact_parent_and_interval() {
    let directory = tempfile::tempdir().unwrap();
    let initial = document();
    let mut store =
        ProjectStore::create(&directory.path().join("range.deadpan"), &initial).unwrap();
    let range = FrameRange::new(ProjectFrame(2), ProjectFrame(6)).unwrap();
    let slice = Arc::new(
        CapturedEditSlice::capture(&initial, &node("root"), range, timing("capture-range"))
            .unwrap(),
    );
    let target = initial.range_deletion(&node("root"), range).unwrap();
    let request = CommandRequest {
        project_id: initial.project_id().clone(),
        expected_revision: initial.revision_id().clone(),
        new_revision: revision("range-cut"),
        command: Command::DeleteRange {
            parent: node("root"),
            range,
            identities: SplitIdentities {
                nodes: (0..target.required_ids)
                    .map(|i| node(&format!("part-{i}")))
                    .collect(),
            },
            timing: timing("range-cut"),
        },
    };
    let mismatched = Arc::new(
        CapturedEditSlice::capture(
            &initial,
            &node("root"),
            FrameRange::new(ProjectFrame(1), ProjectFrame(5)).unwrap(),
            timing("wrong-range"),
        )
        .unwrap(),
    );
    let before = timeline(&store);
    assert!(
        store
            .cut_to_register(&request, name('a'), mismatched, None)
            .is_err()
    );
    assert_eq!(timeline(&store), before);
    assert_eq!(store.register_version().unwrap(), 0);
    let (_, bank) = store
        .cut_to_register(&request, name('a'), slice, None)
        .unwrap();
    assert_eq!(bank.version, 1);
    assert_eq!(store.snapshot().unwrap().duration().unwrap().frames(), 8);
}

#[test]
fn failed_cut_mismatch_and_post_bank_write_failure_preserve_everything() {
    let directory = tempfile::tempdir().unwrap();
    let initial = document();
    let mut store =
        ProjectStore::create(&directory.path().join("rollback.deadpan"), &initial).unwrap();
    let bank = save(&mut store, 'a', capture(&initial, "last"));
    let before = timeline(&store);
    assert!(
        store
            .cut_to_register(
                &cut(&initial, "first", "mismatch"),
                name('a'),
                capture(&initial, "last"),
                None
            )
            .is_err()
    );
    assert_eq!(timeline(&store), before);
    assert_eq!(store.registers().unwrap(), bank);
    store.connection.execute_batch("CREATE TRIGGER reject_history BEFORE INSERT ON history BEGIN SELECT RAISE(ABORT,'injected history write failure'); END;").unwrap();
    assert!(
        store
            .cut_to_register(
                &cut(&initial, "first", "rollback"),
                name('b'),
                capture(&initial, "first"),
                None
            )
            .is_err()
    );
    assert_eq!(timeline(&store), before);
    assert_eq!(store.registers().unwrap(), bank);
}

#[test]
fn stale_wrong_project_missing_capture_and_unavailable_original_preserve_bank() {
    let directory = tempfile::tempdir().unwrap();
    let initial = document();
    let mut store =
        ProjectStore::create(&directory.path().join("invalid.deadpan"), &initial).unwrap();
    let bank = save(&mut store, 'a', capture(&initial, "first"));
    let value = bank.entries[&name('a')].as_ref().clone();
    assert!(
        store
            .save_register(
                &ProjectId::new("other").unwrap(),
                initial.revision_id(),
                name('b'),
                value.clone()
            )
            .is_err()
    );
    assert!(
        store
            .save_register(initial.project_id(), &revision("stale"), name('b'), value)
            .is_err()
    );
    let never_stored = deadpan_core::apply(&initial, &cut(&initial, "last", "not-committed"))
        .unwrap()
        .forward
        .apply(&initial)
        .unwrap();
    assert!(
        store
            .save_register(
                initial.project_id(),
                initial.revision_id(),
                name('b'),
                RegisterValue::Edited {
                    slice: capture(&never_stored, "first")
                }
            )
            .is_err()
    );
    assert!(
        store
            .save_register(
                initial.project_id(),
                initial.revision_id(),
                name('b'),
                RegisterValue::Original {
                    revision: initial.revision_id().clone(),
                    asset: AssetId::new("missing").unwrap(),
                    qualification: SourceQualificationId::new("a".repeat(64)).unwrap(),
                    ordinals: 0..1,
                }
            )
            .is_err()
    );
    assert_eq!(store.registers().unwrap(), bank);
    assert_eq!(store.snapshot().unwrap(), initial);
}

#[test]
fn names_are_closed_and_all_slots_deduplicate_one_payload() {
    for invalid in ['A', '0', '@', '\0', 'é'] {
        assert!(RegisterName::new(invalid).is_err());
        assert!(
            serde_json::from_str::<RegisterName>(&serde_json::to_string(&invalid).unwrap())
                .is_err()
        );
    }
    let directory = tempfile::tempdir().unwrap();
    let initial = document();
    let mut store =
        ProjectStore::create(&directory.path().join("aliases.deadpan"), &initial).unwrap();
    for letter in 'a'..='z' {
        let bank = save(&mut store, letter, capture(&initial, "first"));
        for value in bank.entries.values() {
            assert!(Arc::ptr_eq(value, &bank.entries[&name('"')]));
        }
    }
    assert_eq!(store.registers().unwrap().entries.len(), 27);
    assert_eq!(
        store
            .connection
            .query_row("SELECT count(*) FROM register_contents", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    let bank = save(&mut store, '"', capture(&initial, "last"));
    assert_eq!(bank.version, 27);
    assert_eq!(
        store
            .connection
            .query_row("SELECT count(*) FROM register_contents", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    // Replace the only reference to the second payload: GC retains the shared one.
    save(&mut store, '"', capture(&initial, "first"));
    assert_eq!(
        store
            .connection
            .query_row("SELECT count(*) FROM register_contents", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn corrupt_hash_provenance_name_or_orphaned_content_is_rejected_on_reopen() {
    for mutation in [
        "UPDATE register_contents SET value='{}'",
        "UPDATE register_contents SET capture_revision='missing'",
        "UPDATE registers SET name='A' WHERE name='a'",
        "DELETE FROM registers",
        "UPDATE register_state SET version=-1",
        "DELETE FROM register_contents",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("corrupt.deadpan");
        let initial = document();
        let mut store = ProjectStore::create(&path, &initial).unwrap();
        save(&mut store, 'a', capture(&initial, "first"));
        drop(store);
        let connection = Connection::open(path.join("project.sqlite")).unwrap();
        connection
            .pragma_update(None, "foreign_keys", false)
            .unwrap();
        connection
            .pragma_update(None, "ignore_check_constraints", true)
            .unwrap();
        connection.execute_batch(mutation).unwrap();
        drop(connection);
        for mode in [crate::AccessMode::ReadOnly, crate::AccessMode::ReadWrite] {
            assert!(ProjectStore::open(&path, mode).is_err(), "{mutation}");
        }
    }
}

#[test]
fn aggregate_bytes_are_bounded_before_json_allocation() {
    let directory = tempfile::tempdir().unwrap();
    let initial = document();
    let mut store =
        ProjectStore::create(&directory.path().join("bounded.deadpan"), &initial).unwrap();
    save(&mut store, 'a', capture(&initial, "first"));
    save(&mut store, 'b', capture(&initial, "last"));
    store
        .connection
        .pragma_update(None, "ignore_check_constraints", true)
        .unwrap();
    // Each row is individually below 64 MiB, but their unique aggregate is not.
    store
        .connection
        .execute(
            "UPDATE register_contents SET value=CAST(zeroblob(?1) AS TEXT)",
            [(MAX_REGISTER_BYTES / 2 + 1) as i64],
        )
        .unwrap();
    let error = store.registers().unwrap_err();
    assert!(error.to_string().contains("aggregate 64 MiB"), "{error}");
    store
        .connection
        .execute(
            "UPDATE register_contents SET value=CAST(zeroblob(?1) AS TEXT)",
            [(MAX_REGISTER_BYTES + 1) as i64],
        )
        .unwrap();
    let error = store.registers().unwrap_err();
    assert!(error.to_string().contains("field type or size"), "{error}");
}

#[test]
fn exhausted_version_rejects_cut_before_commit() {
    let directory = tempfile::tempdir().unwrap();
    let initial = document();
    let mut store =
        ProjectStore::create(&directory.path().join("exhausted.deadpan"), &initial).unwrap();
    save(&mut store, 'a', capture(&initial, "last"));
    store
        .connection
        .execute("UPDATE register_state SET version=?1", [i64::MAX])
        .unwrap();
    crate::registers::reseal_for_test(&store.connection).unwrap();
    let bank = store.registers().unwrap();
    let before = timeline(&store);
    assert!(
        store
            .cut_to_register(
                &cut(&initial, "first", "cut"),
                name('b'),
                capture(&initial, "first"),
                None
            )
            .is_err()
    );
    assert_eq!(store.registers().unwrap(), bank);
    assert_eq!(timeline(&store), before);
}

#[test]
fn canonical_but_forged_edited_payload_does_not_supply_its_own_provenance() {
    let directory = tempfile::tempdir().unwrap();
    let initial = document();
    let mut store =
        ProjectStore::create(&directory.path().join("forged.deadpan"), &initial).unwrap();
    let bank = save(&mut store, 'a', capture(&initial, "first"));
    let mut json = serde_json::to_value(bank.entries[&name('a')].as_ref()).unwrap();
    json["slice"]["nodes"]["first"]["label"] = "Not the captured label".into();
    let forged: RegisterValue = serde_json::from_value(json).unwrap();
    assert!(
        store
            .save_register(
                initial.project_id(),
                initial.revision_id(),
                name('b'),
                forged.clone()
            )
            .is_err()
    );
    assert_eq!(store.registers().unwrap(), bank);
    // Bypass all write APIs with internally consistent hash/provenance columns.
    // Reopening still has to recapture the historical selection, not trust JSON.
    let bytes = canonical(&forged).unwrap();
    let hash = digest(&bytes);
    store
        .connection
        .pragma_update(None, "foreign_keys", false)
        .unwrap();
    store
        .connection
        .execute(
            "UPDATE register_contents SET id=?1,value=?2",
            params![hash, std::str::from_utf8(&bytes).unwrap()],
        )
        .unwrap();
    store
        .connection
        .execute("UPDATE registers SET content_id=?1", [hash])
        .unwrap();
    assert!(store.registers().is_err());
}

#[test]
fn malformed_row_types_and_excess_slot_counts_fail_before_loading_values() {
    let directory = tempfile::tempdir().unwrap();
    let initial = document();
    let mut store =
        ProjectStore::create(&directory.path().join("row-types.deadpan"), &initial).unwrap();
    save(&mut store, 'a', capture(&initial, "first"));
    store
        .connection
        .pragma_update(None, "foreign_keys", false)
        .unwrap();
    store.connection.execute_batch("ALTER TABLE register_contents RENAME TO old_contents;
        CREATE TABLE register_contents(id,capture_revision,capture_step,value);
        INSERT INTO register_contents SELECT id,capture_revision,capture_step,CAST(value AS BLOB) FROM old_contents;").unwrap();
    let error = store.registers().unwrap_err();
    assert!(error.to_string().contains("field type or size"), "{error}");
    store
        .connection
        .execute("UPDATE register_contents SET value=CAST(value AS TEXT)", [])
        .unwrap();
    store
        .connection
        .pragma_update(None, "ignore_check_constraints", true)
        .unwrap();
    for index in 0..28 {
        store
            .connection
            .execute(
                "INSERT INTO registers SELECT ?1,id FROM register_contents",
                [format!("extra-{index}")],
            )
            .unwrap();
    }
    let error = store.registers().unwrap_err();
    assert!(error.to_string().contains("row count"), "{error}");
}

fn macro_program() -> Arc<SemanticProgram> {
    Arc::new(
        SemanticProgram::new(vec![
            SemanticInstruction::MoveFrames {
                forward: true,
                count: std::num::NonZeroU32::new(2).unwrap(),
            },
            SemanticInstruction::CutFrames {
                operation: FrameCut::new(1).unwrap(),
                register: name('b'),
            },
            SemanticInstruction::Call {
                register: name('c'),
                count: std::num::NonZeroU32::new(3).unwrap(),
            },
        ])
        .unwrap(),
    )
}

#[test]
fn macro_save_reopens_and_checkpoints_exact_body_without_copy_alias_or_history_changes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("macro.deadpan");
    let initial = document();
    let mut store = ProjectStore::create(&path, &initial).unwrap();
    store.commit(&cut(&initial, "last", "edit")).unwrap();
    store.undo(&revision("edit"), revision("undone")).unwrap();
    let head = store.snapshot().unwrap();
    let before = timeline(&store);
    let program = macro_program();
    let bank = store
        .save_macro(
            head.project_id(),
            head.revision_id(),
            0,
            name('a'),
            program.clone(),
        )
        .unwrap();
    assert_eq!(bank.version, 1);
    assert_eq!(bank.entries.len(), 1);
    assert_eq!(
        bank.entries[&name('a')].as_ref(),
        &RegisterValue::Macro { program }
    );
    assert_eq!(bank.entries[&name('a')].capture_revision(), None);
    let no_provenance: bool = store
        .connection
        .query_row(
            "SELECT capture_revision IS NULL AND capture_step IS NULL FROM register_contents",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(no_provenance);
    assert_eq!(timeline(&store), before);
    assert_eq!(store.history_availability().unwrap(), (false, true));
    let checkpoint = store.checkpoint().unwrap();
    assert_eq!(
        read_bank(&Connection::open(checkpoint).unwrap()).unwrap(),
        bank
    );
    drop(store);
    let mut store = ProjectStore::open(&path, crate::AccessMode::ReadWrite).unwrap();
    assert_eq!(store.registers().unwrap(), bank);
    store.redo(&revision("undone"), revision("redone")).unwrap();
    assert_eq!(store.registers().unwrap(), bank);
}

#[test]
fn macro_and_copy_replace_named_type_both_ways_and_macro_never_changes_unnamed() {
    let directory = tempfile::tempdir().unwrap();
    let initial = document();
    let path = directory.path().join("macro-types.deadpan");
    let mut store = ProjectStore::create(&path, &initial).unwrap();
    let copied = save(&mut store, 'a', capture(&initial, "first"));
    let program = macro_program();
    let macros = store
        .save_register(
            initial.project_id(),
            initial.revision_id(),
            name('a'),
            RegisterValue::Macro {
                program: program.clone(),
            },
        )
        .unwrap();
    assert_eq!(macros.entries[&name('"')], copied.entries[&name('"')]);
    assert!(matches!(
        macros.entries[&name('a')].as_ref(),
        RegisterValue::Macro { .. }
    ));
    let macros = store
        .save_macro(
            initial.project_id(),
            initial.revision_id(),
            macros.version,
            name('b'),
            program,
        )
        .unwrap();
    assert_eq!(macros.entries[&name('"')], copied.entries[&name('"')]);
    let replaced = save(&mut store, 'a', capture(&initial, "last"));
    assert!(matches!(
        replaced.entries[&name('a')].as_ref(),
        RegisterValue::Edited { .. }
    ));
    assert_eq!(replaced.entries[&name('a')], replaced.entries[&name('"')]);
    assert_eq!(replaced.entries[&name('b')], macros.entries[&name('b')]);
    store.validate().unwrap();
    drop(store);
    assert_eq!(
        ProjectStore::open(&path, crate::AccessMode::ReadOnly)
            .unwrap()
            .registers()
            .unwrap(),
        replaced
    );
}

#[test]
fn macro_save_stale_workspace_bank_or_unnamed_target_changes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let initial = document();
    let mut store =
        ProjectStore::create(&directory.path().join("stale-macro.deadpan"), &initial).unwrap();
    let bank = save(&mut store, 'a', capture(&initial, "first"));
    let before = timeline(&store);
    for (project, revision, version, slot) in [
        (
            ProjectId::new("other").unwrap(),
            initial.revision_id().clone(),
            bank.version,
            'b',
        ),
        (
            initial.project_id().clone(),
            revision("stale"),
            bank.version,
            'b',
        ),
        (
            initial.project_id().clone(),
            initial.revision_id().clone(),
            0,
            'b',
        ),
        (
            initial.project_id().clone(),
            initial.revision_id().clone(),
            bank.version,
            '"',
        ),
    ] {
        assert!(
            store
                .save_macro(&project, &revision, version, name(slot), macro_program())
                .is_err()
        );
        assert_eq!(store.registers().unwrap(), bank);
        assert_eq!(timeline(&store), before);
    }
    assert!(
        store
            .save_register(
                initial.project_id(),
                initial.revision_id(),
                name('"'),
                RegisterValue::Macro {
                    program: macro_program()
                },
            )
            .is_err()
    );
    assert_eq!(store.registers().unwrap(), bank);
    assert_eq!(timeline(&store), before);
}

#[test]
fn macro_provenance_unnamed_or_row_corruption_fails_read_checkpoint_and_reopen() {
    for mutation in [
        "UPDATE register_contents SET capture_revision='initial'",
        "UPDATE register_contents SET capture_step='initial'",
        "UPDATE register_contents SET capture_revision='missing'",
        "UPDATE registers SET name='\"'",
        "DELETE FROM registers",
        "UPDATE register_contents SET id=lower(hex(randomblob(32)))",
        "UPDATE register_state SET version=0",
        "UPDATE register_contents SET value=json_set(value,'$.asset','forged')",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("corrupt-macro.deadpan");
        let initial = document();
        let mut store = ProjectStore::create(&path, &initial).unwrap();
        store
            .save_macro(
                initial.project_id(),
                initial.revision_id(),
                0,
                name('a'),
                macro_program(),
            )
            .unwrap();
        store
            .connection
            .pragma_update(None, "foreign_keys", false)
            .unwrap();
        store
            .connection
            .pragma_update(None, "ignore_check_constraints", true)
            .unwrap();
        store.connection.execute_batch(mutation).unwrap();
        assert!(store.registers().is_err(), "{mutation}");
        assert!(store.checkpoint().is_err(), "{mutation}");
        drop(store);
        for mode in [crate::AccessMode::ReadOnly, crate::AccessMode::ReadWrite] {
            assert!(ProjectStore::open(&path, mode).is_err(), "{mutation}");
        }
    }
}

#[test]
fn forged_macro_program_rejects_even_with_valid_canonical_hash_and_no_provenance() {
    let valid = serde_json::to_value(RegisterValue::Macro {
        program: macro_program(),
    })
    .unwrap();
    let mut zero_count = valid.clone();
    zero_count["program"]["instructions"][0]["count"] = 0.into();
    let mut extra_field = valid.clone();
    extra_field["program"]["asset"] = "forged-media".into();
    let mut oversized_body = valid.clone();
    oversized_body["program"]["instructions"] = serde_json::Value::Array(vec![
        valid["program"]["instructions"][0].clone(); MAX_SEMANTIC_PROGRAM_INSTRUCTIONS + 1
    ]);
    let mut oversized_bytes = valid.clone();
    oversized_bytes["program"]["padding"] = "x".repeat(MAX_MACRO_REGISTER_BYTES).into();
    for (forged, expected_error) in [
        (zero_count, None),
        (extra_field, None),
        (oversized_body, None),
        (
            oversized_bytes,
            Some("register type and capture provenance disagree"),
        ),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("forged-macro.deadpan");
        let initial = document();
        let mut store = ProjectStore::create(&path, &initial).unwrap();
        store
            .save_macro(
                initial.project_id(),
                initial.revision_id(),
                0,
                name('a'),
                macro_program(),
            )
            .unwrap();
        let json = serde_json::to_string(&forged).unwrap();
        let id = digest(json.as_bytes());
        store
            .connection
            .pragma_update(None, "foreign_keys", false)
            .unwrap();
        store
            .connection
            .execute(
                "UPDATE register_contents SET id=?1,value=?2",
                params![id, json],
            )
            .unwrap();
        store
            .connection
            .execute("UPDATE registers SET content_id=?1", [id])
            .unwrap();
        let error = store.registers().unwrap_err();
        if let Some(expected) = expected_error {
            // Raw byte admission must reject before typed unknown-field decoding.
            assert!(error.to_string().contains(expected), "{error}");
        }
        assert!(store.checkpoint().is_err());
        drop(store);
        assert!(ProjectStore::open(&path, crate::AccessMode::ReadOnly).is_err());
    }
}

#[test]
fn sql_enforces_macro_and_copy_provenance_and_raw_macro_types_are_checked() {
    let directory = tempfile::tempdir().unwrap();
    let initial = document();
    let mut store =
        ProjectStore::create(&directory.path().join("macro-sql.deadpan"), &initial).unwrap();
    store
        .save_macro(
            initial.project_id(),
            initial.revision_id(),
            0,
            name('a'),
            macro_program(),
        )
        .unwrap();
    assert!(
        store
            .connection
            .execute(
                "UPDATE register_contents SET capture_revision='initial'",
                []
            )
            .is_err()
    );
    store.connection.execute_batch("PRAGMA foreign_keys=OFF;
        CREATE TABLE contents_copy AS SELECT id,capture_revision,capture_step,CAST(value AS BLOB) AS value FROM register_contents;
        DROP TABLE register_contents; ALTER TABLE contents_copy RENAME TO register_contents;").unwrap();
    let error = store.registers().unwrap_err();
    assert!(error.to_string().contains("field type or size"), "{error}");

    let mut copy_store =
        ProjectStore::create(&directory.path().join("copy-sql.deadpan"), &initial).unwrap();
    save(&mut copy_store, 'a', capture(&initial, "first"));
    assert!(
        copy_store
            .connection
            .execute("UPDATE register_contents SET capture_revision=NULL", [])
            .is_err()
    );
}
