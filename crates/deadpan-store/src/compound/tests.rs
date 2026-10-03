use super::*;
use deadpan_core::*;
use std::{collections::BTreeMap, sync::Arc};

fn node(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}
fn name(letter: char) -> RegisterName {
    RegisterName::new(letter).unwrap()
}
fn timing(id: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: revision(id),
        ordinal: 0,
    }
}
fn request(doc: &ProjectDocument, id: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: doc.project_id().clone(),
        expected_revision: doc.revision_id().clone(),
        new_revision: revision(id),
        command,
    }
}
fn document() -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("compound-test").unwrap(),
        revision("initial"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(30_000, 1001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let mut json = serde_json::to_value(empty).unwrap();
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
    json["nodes"] = serde_json::to_value(BTreeMap::from([
        (
            node("root"),
            BeatNode::sequence("Root", vec![node("a"), node("b")]),
        ),
        (node("a"), hold(4)),
        (node("b"), hold(3)),
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
            timing("capture"),
        )
        .unwrap(),
    )
}
fn paste(slice: &CapturedEditSlice, id: &str, index: usize) -> Command {
    let count = slice.identity_requirements().unwrap();
    Command::SpliceSlice {
        parent: node("root"),
        index,
        slice: slice.clone(),
        timing: timing(id),
        identities: SlicePasteIdentities {
            authored: OccurrenceIdentities {
                nodes: (0..count.nodes)
                    .map(|i| node(&format!("{id}-node-{i}")))
                    .collect(),
                marks: (0..count.marks)
                    .map(|i| MarkId::new(format!("{id}-mark-{i}")).unwrap())
                    .collect(),
            },
            aliases: (0..count.aliases)
                .map(|i| node(&format!("{id}-alias-{i}")))
                .collect(),
        },
    }
}
fn compound(
    doc: &ProjectDocument,
    id: &str,
    version: u64,
    inputs: BTreeMap<RegisterName, Option<Arc<RegisterValue>>>,
    steps: Vec<ResolvedStep>,
) -> CommandRequest {
    request(
        doc,
        id,
        Command::Compound {
            transaction: ResolvedTransaction::new(version, inputs, steps).unwrap(),
        },
    )
}
fn repeat_cut_paste(doc: &ProjectDocument) -> (CommandRequest, Arc<CapturedEditSlice>) {
    let repeat = LeafEdit::new(
        revision("repeat-stage"),
        Command::WrapRepeat {
            node: node("a"),
            id: node("repeat"),
            plays: 3,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        },
    )
    .unwrap();
    let repeated = apply(doc, &repeat.request(doc))
        .unwrap()
        .forward
        .apply(doc)
        .unwrap();
    let slice = capture(&repeated, "repeat");
    let steps = vec![
        ResolvedStep::Edit { edit: repeat },
        ResolvedStep::Cut {
            name: name('a'),
            slice: Arc::clone(&slice),
            delete: LeafEdit::new(
                revision("cut-stage"),
                Command::DeleteRipple {
                    node: node("repeat"),
                    timing: timing("cut-stage"),
                },
            )
            .unwrap(),
        },
        ResolvedStep::Paste {
            name: name('a'),
            edit: LeafEdit::new(revision("paste-stage"), paste(&slice, "paste-stage", 1)).unwrap(),
        },
    ];
    (compound(doc, "compound", 0, BTreeMap::new(), steps), slice)
}
fn cells(store: &ProjectStore) -> Vec<String> {
    let mut result = Vec::new();
    for query in [
        "SELECT json_array(id,parent_id,kind,document) FROM revisions ORDER BY id",
        "SELECT json_array(id,parent_id,revision_id,request,edit) FROM history ORDER BY id",
        "SELECT json_array(singleton,head_revision,cursor,workflow) FROM state",
        "SELECT json_array(position,history_id) FROM redo ORDER BY position",
        "SELECT json_array(owner_revision,ordinal,step_revision,document) FROM transaction_steps ORDER BY owner_revision,ordinal",
        "SELECT json_array(singleton,version) FROM register_state",
        "SELECT json_array(id,capture_revision,capture_step,value) FROM register_contents ORDER BY id",
        "SELECT json_array(name,content_id) FROM registers ORDER BY name",
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
fn repeat_cut_paste_has_one_undo_and_reopened_checkpoint_survives_branch_and_bank_gc() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("compound.deadpan");
    let initial = document();
    let mut store = ProjectStore::create(&path, &initial).unwrap();
    let (command, slice) = repeat_cut_paste(&initial);
    let unchanged = cells(&store);
    let preview = store.preview_compound(&command).unwrap();
    assert_eq!(cells(&store), unchanged);
    assert_eq!(preview.register_bank.version, 1);
    let saved = store.commit(&command).unwrap();
    assert_eq!(saved.edit, preview.edit.unwrap());
    let bank = saved.register_bank.unwrap();
    assert_eq!(
        bank.entries[&name('a')].as_ref(),
        &RegisterValue::Edited {
            slice: Arc::clone(&slice)
        }
    );
    assert_eq!(store.snapshot().unwrap().duration().unwrap().frames(), 15);
    assert_eq!(
        store
            .connection
            .query_row("SELECT count(*) FROM history", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        store
            .connection
            .query_row("SELECT count(*) FROM transaction_steps", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        store
            .connection
            .query_row(
                "SELECT count(*) FROM transaction_steps WHERE document IS NOT NULL",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    assert!(store.snapshot_at(&revision("repeat-stage")).is_err());
    slice
        .validate_capture(
            &store
                .capture_snapshot_at(&revision("repeat-stage"))
                .unwrap(),
        )
        .unwrap();
    store
        .undo(&revision("compound"), revision("undone"))
        .unwrap();
    assert_eq!(
        store.snapshot().unwrap().duration().unwrap(),
        initial.duration().unwrap()
    );
    assert_eq!(store.registers().unwrap(), bank);
    drop(store);
    let mut store = ProjectStore::open(&path, crate::AccessMode::ReadWrite).unwrap();
    let current = store.snapshot().unwrap();
    let replacement = capture(&current, "b");
    store
        .save_register(
            current.project_id(),
            current.revision_id(),
            name('a'),
            RegisterValue::Edited { slice: replacement },
        )
        .unwrap();
    // Retained history owns the checkpoint after its last register slot is gone.
    let pasted = store
        .commit(&request(
            &current,
            "new-branch",
            paste(&slice, "new-branch", 2),
        ))
        .unwrap();
    assert_eq!(pasted.edit.duration_delta, 12);
    assert_eq!(store.history_availability().unwrap(), (true, false));
    store.validate().unwrap();
    for id in ["repeat-stage", "cut-stage", "paste-stage"] {
        let head = store.snapshot().unwrap();
        assert!(matches!(
            store.commit(&request(
                &head,
                id,
                Command::DeleteRipple {
                    node: node("a"),
                    timing: timing(id)
                }
            )),
            Err(StoreError::RevisionReused(_))
        ));
    }
    let checkpoint = store.checkpoint().unwrap();
    let restored_path = directory.path().join("restored.deadpan");
    drop(ProjectStore::create(&restored_path, &initial).unwrap());
    Connection::open(checkpoint)
        .unwrap()
        .backup(
            rusqlite::MAIN_DB,
            restored_path.join("project.sqlite"),
            None,
        )
        .unwrap();
    let restored = ProjectStore::open(&restored_path, crate::AccessMode::ReadOnly).unwrap();
    assert_eq!(restored.snapshot().unwrap(), store.snapshot().unwrap());
    assert_eq!(restored.registers().unwrap(), store.registers().unwrap());
    slice
        .validate_capture(
            &restored
                .capture_snapshot_at(&revision("repeat-stage"))
                .unwrap(),
        )
        .unwrap();
    restored.validate().unwrap();
    drop(store);
    ProjectStore::open(&path, crate::AccessMode::ReadOnly)
        .unwrap()
        .validate()
        .unwrap();
}

#[test]
fn late_failure_rolls_back_checkpoints_registers_timeline_and_redo() {
    let directory = tempfile::tempdir().unwrap();
    let initial = document();
    let mut store =
        ProjectStore::create(&directory.path().join("rollback.deadpan"), &initial).unwrap();
    store
        .commit(&request(
            &initial,
            "prior",
            Command::DeleteRipple {
                node: node("b"),
                timing: timing("prior"),
            },
        ))
        .unwrap();
    store.undo(&revision("prior"), revision("undone")).unwrap();
    let current = store.snapshot().unwrap();
    let (command, _) = repeat_cut_paste(&current);
    store.connection.execute_batch("CREATE TRIGGER fail_history BEFORE INSERT ON history BEGIN SELECT RAISE(ABORT,'late failure'); END;").unwrap();
    let unchanged = cells(&store);
    assert!(store.commit(&command).is_err());
    assert_eq!(cells(&store), unchanged);
    assert_eq!(store.history_availability().unwrap(), (false, true));
    store
        .connection
        .execute_batch("DROP TRIGGER fail_history")
        .unwrap();
    store.commit(&command).unwrap();
    store.validate().unwrap();
}

#[test]
fn typed_requests_over_replay_limits_are_rejected_without_writes() {
    let initial = document();
    let children: Vec<_> = (0..80_000).map(|i| node(&format!("h{i}"))).collect();
    let hold = BeatNode::hold(
        "Hold",
        HoldRecipe {
            duration: FrameDuration::new(1).unwrap(),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
            picture_context: None,
        },
    );
    let mut nodes: BTreeMap<_, _> = children
        .iter()
        .map(|id| (id.clone(), hold.clone()))
        .collect();
    nodes.insert(node("bulk"), BeatNode::sequence("Bulk", children));
    let insert = Command::Insert {
        parent: node("root"),
        index: 2,
        subtree: Subtree {
            root: node("bulk"),
            nodes,
            overrides: BTreeMap::new(),
            gap_overrides: BTreeMap::new(),
        },
    };
    for command in [
        insert.clone(),
        Command::Compound {
            transaction: ResolvedTransaction::new(
                0,
                BTreeMap::new(),
                vec![ResolvedStep::Edit {
                    edit: LeafEdit::new(revision("large-leaf"), insert).unwrap(),
                }],
            )
            .unwrap(),
        },
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("bounded.deadpan");
        let mut store = ProjectStore::create(&path, &initial).unwrap();
        let request = request(&initial, "large", command);
        // This is a valid typed edit below the existing request, document and
        // patch byte limits, but it exceeds the reader's scalar/container cap.
        let edit = deadpan_core::apply(&initial, &request).unwrap();
        assert!(serde_json::to_vec(&edit).unwrap().len() < MAX_DOCUMENT_JSON_BYTES);
        drop(edit);
        let wire = serde_json::to_string(&request).unwrap();
        assert!(wire.len() < MAX_DOCUMENT_JSON_BYTES);
        assert!(
            serde_json::from_str::<CommandRequest>(&wire)
                .unwrap_err()
                .to_string()
                .contains("value limit")
        );
        drop(wire);
        let before = cells(&store);
        assert!(
            store.commit(&request).is_err(),
            "a saved request must be readable by history replay"
        );
        assert_eq!(cells(&store), before);
        store.validate().unwrap();
        drop(store);
        assert_eq!(
            ProjectStore::open(&path, crate::AccessMode::ReadOnly)
                .unwrap()
                .snapshot()
                .unwrap(),
            initial
        );
    }
}

#[test]
fn register_only_compound_preserves_history_and_rejects_stale_or_forged_inputs() {
    let directory = tempfile::tempdir().unwrap();
    let initial = document();
    let mut store = ProjectStore::create(&directory.path().join("yank.deadpan"), &initial).unwrap();
    store
        .commit(&request(
            &initial,
            "prior",
            Command::DeleteRipple {
                node: node("b"),
                timing: timing("prior"),
            },
        ))
        .unwrap();
    store.undo(&revision("prior"), revision("undone")).unwrap();
    let current = store.snapshot().unwrap();
    let history = |store: &ProjectStore| {
        [
            "SELECT json_array(id,parent_id,kind,document) FROM revisions ORDER BY id",
            "SELECT json_array(id,parent_id,revision_id,request,edit) FROM history ORDER BY id",
            "SELECT json_array(singleton,head_revision,cursor,workflow) FROM state",
            "SELECT json_array(position,history_id) FROM redo ORDER BY position",
        ]
        .into_iter()
        .flat_map(|query| {
            store
                .connection
                .prepare(query)
                .unwrap()
                .query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .map(Result::unwrap)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
    };
    let before_history = history(&store);
    let value = Arc::new(RegisterValue::Edited {
        slice: capture(&current, "a"),
    });
    let command = compound(
        &current,
        "unused",
        0,
        BTreeMap::new(),
        vec![ResolvedStep::Yank {
            name: name('b'),
            value: Arc::clone(&value),
        }],
    );
    assert!(store.preview(&command).is_err());
    assert!(store.commit(&command).is_err());
    let saved = store.commit_compound(&command, None).unwrap();
    assert!(saved.committed.is_none());
    assert_eq!(store.snapshot().unwrap(), current);
    assert_eq!(saved.register_bank.version, 1);
    assert_eq!(store.history_availability().unwrap(), (false, true));
    assert_eq!(history(&store), before_history);
    let unchanged = cells(&store);
    assert!(store.commit_compound(&command, None).is_err());
    let forged = compound(
        &current,
        "unused",
        1,
        BTreeMap::from([(name('b'), None)]),
        vec![ResolvedStep::Yank {
            name: name('b'),
            value,
        }],
    );
    assert!(store.commit_compound(&forged, None).is_err());
    assert_eq!(cells(&store), unchanged);
    store.validate().unwrap();
}

#[test]
fn hostile_checkpoint_mutations_fail_open_and_checkpoint_validation() {
    for sql in [
        "UPDATE transaction_steps SET document=json_set(document,'$.nodes.repeat.label','forged') WHERE step_revision='repeat-stage'",
        "UPDATE transaction_steps SET document=NULL WHERE step_revision='repeat-stage'",
        "DELETE FROM transaction_steps WHERE step_revision='cut-stage'",
        "INSERT INTO transaction_steps SELECT owner_revision,3,'extra',NULL FROM transaction_steps LIMIT 1",
        "UPDATE transaction_steps SET step_revision='initial' WHERE step_revision='cut-stage'",
        "UPDATE transaction_steps SET document=json_set(document,'$.revision_id','wrong') WHERE step_revision='repeat-stage'",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("hostile.deadpan");
        let initial = document();
        let mut store = ProjectStore::create(&path, &initial).unwrap();
        store.commit(&repeat_cut_paste(&initial).0).unwrap();
        store
            .connection
            .pragma_update(None, "foreign_keys", false)
            .unwrap();
        store.connection.execute_batch(sql).unwrap();
        assert!(store.validate().is_err(), "{sql}");
        assert!(store.checkpoint().is_err(), "{sql}");
        drop(store);
        assert!(
            ProjectStore::open(&path, crate::AccessMode::ReadOnly).is_err(),
            "{sql}"
        );
    }
}

#[test]
fn future_checkpoint_dependency_is_rejected_before_history_can_launder_it() {
    let directory = tempfile::tempdir().unwrap();
    let initial = document();
    let mut store =
        ProjectStore::create(&directory.path().join("future.deadpan"), &initial).unwrap();
    let (command, slice) = repeat_cut_paste(&initial);
    store.commit(&command).unwrap();
    let mut forged = command.clone();
    let Command::Compound { transaction } = &mut forged.command else {
        unreachable!()
    };
    // An external input cannot borrow a checkpoint from its own future steps,
    // even if no leaf happens to read that input.
    *transaction = ResolvedTransaction::new(
        0,
        BTreeMap::from([(name('z'), Some(Arc::new(RegisterValue::Edited { slice })))]),
        transaction.steps().to_vec(),
    )
    .unwrap();
    store
        .connection
        .execute(
            "UPDATE history SET request=?1",
            [serde_json::to_string(&forged).unwrap()],
        )
        .unwrap();
    assert!(
        store
            .validate()
            .unwrap_err()
            .to_string()
            .contains("earlier committed")
    );
}

#[test]
fn planned_macro_multi_cut_has_one_undo_and_final_copy_survives_reopen_and_body_replacement() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("semantic-macro.deadpan");
    let initial = document();
    let mut store = ProjectStore::create(&path, &initial).unwrap();
    let program = Arc::new(
        SemanticProgram::new(vec![
            SemanticInstruction::MoveFrames {
                forward: true,
                count: std::num::NonZeroU32::new(1).unwrap(),
            },
            SemanticInstruction::CutFrames {
                operation: FrameCut::new(1).unwrap(),
                register: name('b'),
            },
        ])
        .unwrap(),
    );
    let bank = store
        .save_macro(
            initial.project_id(),
            initial.revision_id(),
            0,
            name('a'),
            program,
        )
        .unwrap();
    let invocation = SemanticProgram::new(vec![SemanticInstruction::Call {
        register: name('a'),
        count: std::num::NonZeroU32::new(2).unwrap(),
    }])
    .unwrap();
    let plan = plan_semantic(
        &initial,
        &SemanticContext {
            parent: node("root"),
            cursor: ProjectFrame(0),
            selected_child: None,
            visual_selection: None,
        },
        &invocation,
        SemanticRegisterBank {
            entries: &bank.entries,
            version: bank.version,
        },
        revision("macro-run"),
        |allocation| {
            let SemanticAllocationRequest::Cut {
                step_index,
                required_split_ids,
            } = allocation
            else {
                unreachable!()
            };
            Ok(SemanticAllocation::Cut {
                new_revision: revision(&format!("macro-step-{step_index}")),
                capture_revision: revision(&format!("macro-capture-{step_index}")),
                split_identities: SplitIdentities {
                    nodes: (0..required_split_ids)
                        .map(|index| node(&format!("macro-split-{step_index}-{index}")))
                        .collect(),
                },
            })
        },
        |_, _| unreachable!("this macro has no Original paste"),
    )
    .unwrap();
    let request = plan.request.as_ref().unwrap();
    let Command::Compound { transaction } = &request.command else {
        unreachable!()
    };
    assert_eq!(
        transaction.inputs()[&name('a')],
        bank.entries.get(&name('a')).cloned()
    );
    let before = cells(&store);
    let preview = store.preview_compound(request).unwrap();
    assert_eq!(cells(&store), before);
    let saved = store.commit_compound(request, None).unwrap();
    let final_bank = saved.register_bank;
    assert_eq!(final_bank, preview.register_bank);
    assert_eq!(store.snapshot().unwrap(), plan.document);
    assert_eq!(plan.document.duration().unwrap().frames(), 5);
    assert_eq!(plan.context.cursor, ProjectFrame(2));
    assert_eq!(final_bank.entries[&name('a')], bank.entries[&name('a')]);
    assert_eq!(
        final_bank.entries[&name('b')],
        final_bank.entries[&name('"')]
    );
    assert_eq!(final_bank.version, 2);
    let RegisterValue::Edited { slice } = final_bank.entries[&name('b')].as_ref() else {
        unreachable!()
    };
    assert_eq!(slice.revision_id(), &revision("macro-step-0"));
    assert_eq!(
        store
            .connection
            .query_row("SELECT count(*) FROM history", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        store
            .connection
            .query_row("SELECT count(*) FROM transaction_steps", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    store
        .undo(&revision("macro-run"), revision("macro-undo"))
        .unwrap();
    let mut expected_undo = serde_json::to_value(&initial).unwrap();
    expected_undo["revision_id"] = "macro-undo".into();
    assert_eq!(
        serde_json::to_value(store.snapshot().unwrap()).unwrap(),
        expected_undo
    );
    assert_eq!(store.registers().unwrap(), final_bank);
    let checkpoint = store.checkpoint().unwrap();
    assert_eq!(
        crate::registers::read_bank(&Connection::open(checkpoint).unwrap()).unwrap(),
        final_bank
    );
    drop(store);
    let mut store = ProjectStore::open(&path, crate::AccessMode::ReadWrite).unwrap();
    assert_eq!(store.registers().unwrap(), final_bank);
    // History replays frozen input programs even after the named macro becomes
    // an incompatible copy and its content-addressed row is collected.
    let head = store.snapshot().unwrap();
    store
        .save_register(
            head.project_id(),
            head.revision_id(),
            name('a'),
            RegisterValue::Edited {
                slice: capture(&head, "a"),
            },
        )
        .unwrap();
    store
        .redo(head.revision_id(), revision("macro-redo"))
        .unwrap();
    let mut expected_redo = serde_json::to_value(&plan.document).unwrap();
    expected_redo["revision_id"] = "macro-redo".into();
    assert_eq!(
        serde_json::to_value(store.snapshot().unwrap()).unwrap(),
        expected_redo
    );
    assert_eq!(
        store.registers().unwrap().entries[&name('b')],
        final_bank.entries[&name('b')]
    );
    store.validate().unwrap();
    drop(store);
    ProjectStore::open(&path, crate::AccessMode::ReadOnly)
        .unwrap()
        .validate()
        .unwrap();
}

#[test]
fn compound_macro_inputs_are_frozen_and_invalid_programs_fail_historical_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("macro-input.deadpan");
    let initial = document();
    let mut store = ProjectStore::create(&path, &initial).unwrap();
    let program = Arc::new(
        SemanticProgram::new(vec![SemanticInstruction::MoveFrames {
            forward: true,
            count: std::num::NonZeroU32::new(1).unwrap(),
        }])
        .unwrap(),
    );
    let bank = store
        .save_macro(
            initial.project_id(),
            initial.revision_id(),
            0,
            name('m'),
            program.clone(),
        )
        .unwrap();
    let steps = vec![ResolvedStep::Edit {
        edit: LeafEdit::new(
            revision("macro-leaf"),
            Command::DeleteRipple {
                node: node("b"),
                timing: timing("macro-leaf"),
            },
        )
        .unwrap(),
    }];
    let good = compound(
        &initial,
        "macro-outer",
        bank.version,
        BTreeMap::from([(name('m'), bank.entries.get(&name('m')).cloned())]),
        steps.clone(),
    );
    let changed = Arc::new(RegisterValue::Macro {
        program: Arc::new(
            SemanticProgram::new(vec![SemanticInstruction::MoveFrames {
                forward: false,
                count: std::num::NonZeroU32::new(1).unwrap(),
            }])
            .unwrap(),
        ),
    });
    let forged = compound(
        &initial,
        "forged-outer",
        bank.version,
        BTreeMap::from([(name('m'), Some(changed))]),
        steps,
    );
    let before = cells(&store);
    assert!(store.commit_compound(&forged, None).is_err());
    assert_eq!(cells(&store), before);
    store.commit_compound(&good, None).unwrap();
    store.validate().unwrap();
    let mut forged = serde_json::to_value(&good).unwrap();
    forged["command"]["transaction"]["inputs"]["m"]["program"]["instructions"][0]["count"] =
        0.into();
    store
        .connection
        .execute(
            "UPDATE history SET request=?1",
            [serde_json::to_string(&forged).unwrap()],
        )
        .unwrap();
    assert!(store.validate().is_err());
    assert!(store.checkpoint().is_err());
    drop(store);
    assert!(ProjectStore::open(&path, crate::AccessMode::ReadOnly).is_err());
}
