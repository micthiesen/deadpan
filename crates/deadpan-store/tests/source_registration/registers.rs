use super::*;
use deadpan_store::registers::{RegisterName, RegisterValue};
use std::{collections::BTreeMap, sync::Arc};

#[test]
fn qualified_range_repeat_stages_exact_wrapper_capture_and_reopens_with_one_undo() -> Result {
    use deadpan_core::{
        ProjectFrame, RepeatSelectionIdentities, SemanticAllocation, SemanticAllocationRequest,
        SemanticContext, SemanticInstruction, SemanticProgram, SemanticRegisterBank,
        SemanticSelector, SemanticVisualSelection, SliceCaptureSelection, SplitIdentities,
        plan_semantic,
    };
    use std::num::NonZeroU32;

    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    let registration = request(
        &store,
        &original,
        "registered-repeat",
        "camera",
        Some("clip"),
    )?;
    store.register_source(&registration, &decoded, None, limits(), &active())?;
    let before = store.snapshot()?;
    let before_counts = counts(&path)?;
    let bank = store.registers()?;
    let body = SemanticProgram::new(vec![
        SemanticInstruction::Repeat {
            selector: SemanticSelector::VisualSelection,
            plays: NonZeroU32::new(3).unwrap(),
        },
        SemanticInstruction::Yank {
            selector: SemanticSelector::SelectedBeat,
            register: RegisterName::new('a')?,
        },
    ])?;
    let plan = plan_semantic(
        &before,
        &SemanticContext {
            parent: before.root().clone(),
            cursor: ProjectFrame(1),
            selected_child: None,
            visual_selection: Some(SemanticVisualSelection::Time {
                anchor: ProjectFrame(3),
                head: ProjectFrame(1),
                extending: true,
            }),
        },
        &body,
        SemanticRegisterBank {
            entries: &bank.entries,
            version: bank.version,
        },
        revision("repeat-capture-outer"),
        |allocation| match allocation {
            SemanticAllocationRequest::Repeat {
                required_split_ids,
                needs_group,
                ..
            } => {
                assert!(needs_group);
                assert!(required_split_ids > 0);
                Ok(SemanticAllocation::Repeat {
                    new_revision: revision("repeat-capture-stage"),
                    identities: RepeatSelectionIdentities {
                        repeat: NodeId::new("selected-repeat")?,
                        group: Some(NodeId::new("selected-group")?),
                        split: SplitIdentities {
                            nodes: (0..required_split_ids)
                                .map(|index| NodeId::new(format!("repeat-split-{index}")))
                                .collect::<std::result::Result<_, _>>()?,
                        },
                    },
                })
            }
            SemanticAllocationRequest::Yank { .. } => Ok(SemanticAllocation::Yank {
                capture_revision: revision("repeat-copy"),
            }),
            _ => unreachable!("repeat then capture"),
        },
        |_, _| unreachable!("no Original paste"),
    )?;
    let preview = store.preview_compound(plan.request.as_ref().unwrap())?;
    assert_eq!(store.snapshot()?, before);
    assert_eq!(counts(&path)?, before_counts);
    let result = store.commit_compound(plan.request.as_ref().unwrap(), None)?;
    assert_eq!(result.register_bank, preview.register_bank);
    let after = store.snapshot()?;
    assert_eq!(after.duration()?.frames(), before.duration()?.frames() + 4);
    assert_eq!(after.assets(), before.assets());
    assert_eq!(counts(&path)?.1, before_counts.1 + 1);
    let RegisterValue::Edited { slice } =
        result.register_bank.entries[&RegisterName::new('a')?].as_ref()
    else {
        panic!("staged wrapper copy")
    };
    assert_eq!(
        slice.selection(),
        &SliceCaptureSelection::Child {
            node: NodeId::new("selected-repeat")?
        }
    );
    assert_eq!(slice.revision_id(), &revision("repeat-capture-stage"));
    assert_eq!(slice.duration().frames(), 6);
    slice.validate_capture(&store.capture_snapshot_at(slice.revision_id())?)?;
    store.undo(after.revision_id(), revision("repeat-source-undo"))?;
    let mut expected = serde_json::to_value(before)?;
    expected["revision_id"] = serde_json::json!("repeat-source-undo");
    assert_eq!(serde_json::to_value(store.snapshot()?)?, expected);
    store.checkpoint()?;
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.registers()?, result.register_bank);
    slice.validate_capture(&store.capture_snapshot_at(slice.revision_id())?)?;
    store.redo(
        &revision("repeat-source-undo"),
        revision("repeat-source-redo"),
    )?;
    let mut expected = serde_json::to_value(after)?;
    expected["revision_id"] = serde_json::json!("repeat-source-redo");
    assert_eq!(serde_json::to_value(store.snapshot()?)?, expected);
    store.validate()?;
    Ok(())
}

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

    // A cached qualified mapping remains an equality check on every paste.
    // The second leaf uses the same register and duration with different PTS.
    let mut repeated = make(expected.clone())?;
    let Command::Compound { transaction } = &repeated.command else {
        unreachable!()
    };
    let second = ResolvedStep::Paste {
        name: a,
        edit: LeafEdit::new(
            revision("second-paste-stage"),
            Command::SpliceSource {
                parent: before.root().clone(),
                index: 1,
                source: source(4..10)?,
                id: NodeId::new("second-pasted")?,
                label: "Wrong repeated mapping".into(),
                timing: AudioTimingId {
                    allocation: revision("second-paste-stage"),
                    ordinal: 0,
                },
            },
        )?,
    };
    repeated.command = Command::Compound {
        transaction: ResolvedTransaction::new(
            bank.version,
            transaction.inputs().clone(),
            vec![transaction.steps()[0].clone(), second],
        )?,
    };
    deadpan_core::apply(&before, &repeated)?;
    for error in [
        store.preview_compound(&repeated).unwrap_err(),
        store.commit_compound(&repeated, None).unwrap_err(),
    ] {
        assert!(
            error.to_string().contains("qualified register ordinals"),
            "{error}"
        );
    }
    assert_eq!(store.snapshot()?, before);
    assert_eq!(store.registers()?, bank);
    assert_eq!(counts(&path)?, counts_before);

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

#[test]
fn typed_selectors_capture_qualified_source_and_staged_range_with_one_history_entry() -> Result {
    use deadpan_core::{
        FrameRange, ProjectFrame, SemanticAllocation, SemanticAllocationRequest, SemanticContext,
        SemanticInstruction, SemanticMotion, SemanticProgram, SemanticRegisterBank,
        SemanticSelector, SliceCaptureSelection, SplitIdentities, plan_semantic,
    };
    use std::num::NonZeroU32;

    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    let registration = request(&store, &original, "registered", "camera", Some("clip"))?;
    store.register_source(&registration, &decoded, None, limits(), &active())?;
    let before = store.snapshot()?;
    let counts_before = counts(&path)?;
    let bank = store.registers()?;
    let body = SemanticProgram::new(vec![
        SemanticInstruction::Yank {
            selector: SemanticSelector::SelectedBeat,
            register: RegisterName::new('a')?,
        },
        SemanticInstruction::Cut {
            selector: SemanticSelector::Motion {
                motion: SemanticMotion::Frames {
                    forward: true,
                    count: NonZeroU32::new(2).unwrap(),
                },
            },
            register: RegisterName::new('b')?,
        },
        SemanticInstruction::Yank {
            selector: SemanticSelector::Motion {
                motion: SemanticMotion::Scope { end: true },
            },
            register: RegisterName::new('c')?,
        },
    ])?;
    let plan = plan_semantic(
        &before,
        &SemanticContext {
            parent: before.root().clone(),
            cursor: ProjectFrame(3),
            selected_child: Some(NodeId::new("clip")?),
            visual_selection: None,
        },
        &body,
        SemanticRegisterBank {
            entries: &bank.entries,
            version: bank.version,
        },
        revision("typed-source"),
        |allocation| match allocation {
            SemanticAllocationRequest::Yank { step_index } => Ok(SemanticAllocation::Yank {
                capture_revision: revision(&format!("typed-yank-{step_index}")),
            }),
            SemanticAllocationRequest::Cut {
                step_index,
                required_split_ids,
            } => Ok(SemanticAllocation::Cut {
                new_revision: revision(&format!("typed-cut-{step_index}")),
                capture_revision: revision(&format!("typed-cut-capture-{step_index}")),
                split_identities: SplitIdentities {
                    nodes: (0..required_split_ids)
                        .map(|index| NodeId::new(format!("typed-split-{index}")))
                        .collect::<std::result::Result<_, _>>()?,
                },
            }),
            _ => unreachable!("capture-only macro"),
        },
        |_, _| unreachable!("no Original paste"),
    )?;
    let request = plan.request.as_ref().unwrap();
    let preview = store.preview_compound(request)?;
    assert_eq!(store.snapshot()?, before);
    assert_eq!(store.registers()?, bank);
    assert_eq!(counts(&path)?, counts_before);
    let result = store.commit_compound(request, None)?;
    assert!(result.committed.is_some());
    assert_eq!(result.register_bank, preview.register_bank);
    let after = store.snapshot()?;
    assert_eq!(after.duration()?.frames(), before.duration()?.frames() - 2);
    assert_eq!(counts(&path)?.1, counts_before.1 + 1);
    let bank = store.registers()?;
    let RegisterValue::Edited { slice: child } = bank.entries[&RegisterName::new('a')?].as_ref()
    else {
        panic!("child capture")
    };
    assert_eq!(
        child.selection(),
        &SliceCaptureSelection::Child {
            node: NodeId::new("clip")?
        }
    );
    child.validate_capture(&before)?;
    let RegisterValue::Edited { slice: staged } = bank.entries[&RegisterName::new('c')?].as_ref()
    else {
        panic!("staged motion capture")
    };
    assert_ne!(staged.revision_id(), before.revision_id());
    assert_ne!(staged.revision_id(), after.revision_id());
    assert_eq!(
        staged.range(),
        FrameRange::new(ProjectFrame(3), ProjectFrame(after.duration()?.frames()))?
    );
    let historical = store.capture_snapshot_at(staged.revision_id())?;
    assert_eq!(historical.assets(), before.assets());
    staged.validate_capture(&historical)?;
    store.undo(after.revision_id(), revision("undo-typed-source"))?;
    let mut expected = serde_json::to_value(&before)?;
    expected["revision_id"] = serde_json::json!("undo-typed-source");
    assert_eq!(serde_json::to_value(store.snapshot()?)?, expected);
    store.checkpoint()?;
    drop(store);
    let mut reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(reopened.registers()?, bank);
    reopened.redo(
        &revision("undo-typed-source"),
        revision("redo-typed-source"),
    )?;
    let mut expected = serde_json::to_value(&after)?;
    expected["revision_id"] = serde_json::json!("redo-typed-source");
    assert_eq!(serde_json::to_value(reopened.snapshot()?)?, expected);
    assert_eq!(reopened.registers()?, bank);
    reopened.validate()?;
    Ok(())
}

#[test]
fn counted_semantic_original_paste_keeps_exact_mapping_through_undo_and_reopen() -> Result {
    use deadpan_core::{
        ProjectFrame, SemanticAllocation, SemanticAllocationRequest, SemanticContext,
        SemanticInstruction, SemanticProgram, SemanticRegisterBank, plan_semantic,
    };
    use deadpan_media::source_import_timing::derive_source_moment;
    use std::num::NonZeroU32;

    let scratch = tempfile::tempdir()?;
    let (path, mut store) = project(scratch.path())?;
    let original = retain(&mut store, "offset-bframes.mp4")?;
    let decoded = decode(&store, &original)?;
    let register = request(&store, &original, "register", "camera", None)?;
    let registered = store.register_source(&register, &decoded, None, limits(), &active())?;
    let before = store.snapshot()?;
    let a = RegisterName::new('a')?;
    let b = RegisterName::new('b')?;
    let original = RegisterValue::Original {
        revision: before.revision_id().clone(),
        asset: id("camera"),
        qualification: registered.qualification.clone(),
        ordinals: 3..9,
    };
    let bank = store.save_register(
        before.project_id(),
        before.revision_id(),
        a,
        original.clone(),
    )?;
    let body = Arc::new(SemanticProgram::new(vec![SemanticInstruction::Paste {
        register: a,
        before: false,
    }])?);
    let bank = store.save_macro(
        before.project_id(),
        before.revision_id(),
        bank.version,
        b,
        body,
    )?;
    let receipt = store.source_qualification(&registered.qualification)?;
    let expected = derive_source_moment(
        receipt.snapshot().video().unwrap().index(),
        receipt.snapshot().audio(),
        3..9,
        before.presentation_basis().frame_rate,
    )?
    .source_node(id("camera"));
    let program = SemanticProgram::new(vec![SemanticInstruction::Call {
        register: b,
        count: NonZeroU32::new(2).unwrap(),
    }])?;
    let plan = plan_semantic(
        &before,
        &SemanticContext {
            parent: before.root().clone(),
            cursor: ProjectFrame(0),
            selected_child: None,
            visual_selection: None,
        },
        &program,
        SemanticRegisterBank {
            entries: &bank.entries,
            version: bank.version,
        },
        revision("semantic-original"),
        |request| {
            let SemanticAllocationRequest::PasteOriginal {
                step_index,
                required_split_ids,
            } = request
            else {
                unreachable!()
            };
            assert_eq!(required_split_ids, 0);
            Ok(SemanticAllocation::PasteOriginal {
                new_revision: revision(&format!("paste-{step_index}")),
                node: NodeId::new(format!("pasted-{step_index}"))?,
                split_identities: deadpan_core::SplitIdentities { nodes: vec![] },
            })
        },
        |document, value| {
            assert_eq!(value, &original);
            assert_eq!(document.presentation_basis(), before.presentation_basis());
            Ok(expected.clone())
        },
    )?;
    let request = plan.request.as_ref().unwrap();
    let preview = store.preview_compound(request)?;
    assert_eq!(preview.register_bank, bank);
    let saved = store.commit_compound(request, None)?;
    assert!(saved.committed.is_some());
    assert_eq!(saved.register_bank, bank);
    let after = store.snapshot()?;
    assert_eq!(after.duration()?.frames(), expected.duration.frames() * 2);
    let NodeKind::Sequence { children } = &after.nodes()[after.root()].kind else {
        unreachable!()
    };
    assert_eq!(children.len(), 2);
    for child in children {
        let NodeKind::Source { source } = &after.nodes()[child].kind else {
            panic!("Original Source")
        };
        assert_eq!(source, &expected);
    }
    store.undo(after.revision_id(), revision("undo-semantic-original"))?;
    let mut expected_undo = serde_json::to_value(&before)?;
    expected_undo["revision_id"] = serde_json::json!("undo-semantic-original");
    assert_eq!(serde_json::to_value(store.snapshot()?)?, expected_undo);
    assert_eq!(store.registers()?, bank);
    store.checkpoint()?;
    drop(store);
    let mut reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    reopened.redo(
        &revision("undo-semantic-original"),
        revision("redo-semantic-original"),
    )?;
    let mut expected_redo = serde_json::to_value(&after)?;
    expected_redo["revision_id"] = serde_json::json!("redo-semantic-original");
    assert_eq!(serde_json::to_value(reopened.snapshot()?)?, expected_redo);
    assert_eq!(reopened.registers()?, bank);
    reopened.validate()?;
    let before_replace = reopened.snapshot()?;
    let program =
        SemanticProgram::new(vec![SemanticInstruction::ReplaceSelection { register: a }])?;
    let plan = plan_semantic(
        &before_replace,
        &SemanticContext {
            parent: before_replace.root().clone(),
            cursor: ProjectFrame(0),
            selected_child: None,
            visual_selection: Some(deadpan_core::SemanticVisualSelection::Time {
                anchor: ProjectFrame(2),
                head: ProjectFrame(1),
                extending: false,
            }),
        },
        &program,
        SemanticRegisterBank {
            entries: &bank.entries,
            version: bank.version,
        },
        revision("semantic-original-replacement"),
        |allocation| {
            let SemanticAllocationRequest::PasteOriginal {
                required_split_ids, ..
            } = allocation
            else {
                unreachable!()
            };
            assert!(
                required_split_ids > 0,
                "the qualified Source needs interior endpoint splits"
            );
            Ok(SemanticAllocation::PasteOriginal {
                new_revision: revision("original-replacement-stage"),
                node: NodeId::new("replacement-source")?,
                split_identities: deadpan_core::SplitIdentities {
                    nodes: (0..required_split_ids)
                        .map(|index| NodeId::new(format!("replacement-split-{index}")))
                        .collect::<std::result::Result<_, _>>()?,
                },
            })
        },
        |_, value| {
            assert_eq!(value, &original);
            Ok(expected.clone())
        },
    )?;
    assert!(plan.context.visual_selection.is_none());
    assert_eq!(plan.context.cursor, ProjectFrame(1));
    assert_eq!(
        plan.trace[0].removed_range,
        Some(deadpan_core::FrameRange::new(
            ProjectFrame(1),
            ProjectFrame(2)
        )?)
    );
    assert_eq!(
        plan.trace[0].resolved_range,
        Some(deadpan_core::FrameRange::new(
            ProjectFrame(1),
            ProjectFrame(1 + expected.duration.frames())
        )?)
    );
    let request = plan.request.as_ref().unwrap();
    let preview = reopened.preview_compound(request)?;
    assert_eq!(reopened.snapshot()?, before_replace);
    assert_eq!(preview.register_bank, bank);
    let committed = reopened.commit_compound(request, None)?;
    assert!(committed.committed.is_some());
    assert_eq!(committed.register_bank, bank);
    let replaced = reopened.snapshot()?;
    assert_eq!(
        replaced.duration()?.frames(),
        before_replace.duration()?.frames() - 1 + expected.duration.frames()
    );
    let NodeKind::Source { source } = &replaced.nodes()[&NodeId::new("replacement-source")?].kind
    else {
        panic!("replacement must use the exact qualified Source")
    };
    assert_eq!(source, &expected);
    reopened.undo(
        replaced.revision_id(),
        revision("undo-original-replacement"),
    )?;
    let mut expected_undo = serde_json::to_value(&before_replace)?;
    expected_undo["revision_id"] = serde_json::json!("undo-original-replacement");
    assert_eq!(serde_json::to_value(reopened.snapshot()?)?, expected_undo);
    reopened.checkpoint()?;
    drop(reopened);
    let mut reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    reopened.redo(
        &revision("undo-original-replacement"),
        revision("redo-original-replacement"),
    )?;
    let mut expected_redo = serde_json::to_value(&replaced)?;
    expected_redo["revision_id"] = serde_json::json!("redo-original-replacement");
    assert_eq!(serde_json::to_value(reopened.snapshot()?)?, expected_redo);
    assert_eq!(reopened.registers()?, bank);
    reopened.validate()?;
    Ok(())
}
