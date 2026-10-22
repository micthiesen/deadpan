//! Selected fixed-stage bodies retain independent accepted providers and atomic history.
use super::nested_fixed_retime::{allocation, content_eq, db_counts, fixed_repeat_initial, set};
use super::*;
use deadpan_core::{
    FrameRange, InstancePath, LeafEdit, ProjectFrame, RegisterName, RepeatInstance, ResolvedStep,
    ResolvedTransaction, SemanticAllocationRequest, SemanticContext, SemanticInstruction,
    SemanticProgram, SemanticRegisterBank, SemanticSelectedHoldDurationContext, plan_semantic,
};
use std::num::NonZeroU32;
use std::sync::Arc;

fn instance(document: &ProjectDocument, top: u32, inner: u32) -> Result<InstancePath> {
    let repeat = |name: &str, play| -> Result<RepeatInstance> {
        let node = NodeId::new(name)?;
        let NodeKind::Repeat { iterations, .. } = &document.nodes()[&node].kind else {
            return Err("Repeat required".into());
        };
        Ok(RepeatInstance {
            node,
            iteration: iterations.at(play).unwrap(),
        })
    };
    Ok(InstancePath {
        node: NodeId::new("hold")?,
        repeats: vec![repeat("top", top)?, repeat("outer", inner)?],
    })
}

fn target(presentation: &InstancePath, axis: usize, shared_other: bool) -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: presentation.node.clone(),
        repeats: presentation
            .repeats
            .iter()
            .enumerate()
            .map(|(index, step)| RepeatEditStep {
                repeat: step.node.clone(),
                branch: if index != axis && shared_other {
                    RepeatEditBranch::Default
                } else {
                    RepeatEditBranch::Play {
                        iteration: step.iteration.clone(),
                    }
                },
            })
            .collect(),
    }
}

fn authority(
    document: &ProjectDocument,
    axis: usize,
    shared_other: bool,
) -> Result<SemanticSelectedHoldDurationContext> {
    let first = instance(document, 1, 1)?;
    let second = instance(
        document,
        if axis == 0 { 2 } else { 1 },
        if axis == 1 { 2 } else { 1 },
    )?;
    Ok(SemanticSelectedHoldDurationContext {
        root: NodeId::new("top")?,
        target: target(&first, axis, shared_other),
        also: vec![target(&second, axis, shared_other)],
        presentation: Some(first),
    })
}

fn affected(axis: usize, shared_other: bool, top: u32, inner: u32) -> bool {
    inner != 0
        && if axis == 0 {
            top >= 1 && (shared_other || inner == 1)
        } else {
            inner >= 1 && (shared_other || top == 1)
        }
}

fn resize_command(
    document: &ProjectDocument,
    target: &ScopedNodeTarget,
    presentation: &InstancePath,
    duration: FrameDuration,
    key: &str,
) -> Result<Command> {
    let mixed = target
        .repeats
        .iter()
        .any(|s| s.branch == RepeatEditBranch::Default);
    let needs = if mixed {
        document.scoped_hold_duration_requirements(target, duration)?
    } else {
        document.occurrence_hold_duration_requirements(presentation, duration)?
    };
    let identities = OccurrenceIdentities {
        nodes: (0..needs.nodes)
            .map(|n| NodeId::new(format!("{key}-node-{n}")))
            .collect::<std::result::Result<_, _>>()?,
        marks: (0..needs.marks)
            .map(|n| deadpan_core::MarkId::new(format!("{key}-mark-{n}")))
            .collect::<std::result::Result<_, _>>()?,
    };
    Ok(if mixed {
        Command::SetScopedHoldDuration {
            target: target.clone(),
            duration,
            identities,
        }
    } else {
        Command::EditOccurrence {
            instance: presentation.clone(),
            edit: deadpan_core::OccurrenceEdit::SetHoldDuration { duration },
            identities,
        }
    })
}

#[test]
fn selected_fixed_retime_macro_keeps_each_accepted_provider_and_history_atomic() -> Result {
    for axis in [0, 1] {
        for shared_other in [false, true] {
            for final_frames in [8, 18] {
                let scratch = tempfile::tempdir()?;
                let package = scratch.path().join(format!(
                    "selected-fixed-{axis}-{shared_other}-{final_frames}.deadpan"
                ));
                let mut store = ProjectStore::create(&package, &fixed_repeat_initial()?)?;
                store.set_generation_context_resolver(Arc::new(Relevant));
                let chosen = authority(&store.snapshot()?, axis, shared_other)?;
                let mut shared = chosen.target.clone();
                for step in &mut shared.repeats {
                    step.branch = RepeatEditBranch::Default;
                }
                accepted_for_duration(
                    &mut store,
                    &shared,
                    FrameDuration::new(12)?,
                    "selected-fixed-accepted",
                    73,
                )?;
                let before = store.snapshot()?;
                let original_intents = store
                    .generation_intents(None, 256)?
                    .into_iter()
                    .map(|intent| (intent.head, intent.birth))
                    .collect::<Vec<_>>();
                let source_request = store
                    .generation_request(&RequestId::new("selected-fixed-accepted-request")?)?
                    .unwrap();
                assert_eq!(source_request.target, shared);
                let NodeKind::Hold { recipe } = &before.nodes()[&chosen.target.node].kind else {
                    panic!("accepted Hold")
                };
                let HoldVideo::Generated { accepted } = &recipe.video else {
                    panic!("bundle-admitted synthetic provider")
                };
                let accepted = accepted.clone();
                let visibility = before
                    .selected_hold_duration_visibility(before.root(), &chosen)?
                    .ok_or("live selected primary")?;
                assert_eq!(
                    visibility.frames,
                    FrameRange::new(ProjectFrame(48), ProjectFrame(54))?
                );

                let a = RegisterName::new('a')?;
                let b = RegisterName::new('b')?;
                let intermediate = if final_frames == 8 { 10 } else { 14 };
                let bank = store.save_macro(
                    before.project_id(),
                    before.revision_id(),
                    store.register_version()?,
                    b,
                    Arc::new(SemanticProgram::new(vec![
                        set(intermediate),
                        set(final_frames),
                    ])?),
                )?;
                let bank = store.save_macro(
                    before.project_id(),
                    before.revision_id(),
                    bank.version,
                    a,
                    Arc::new(SemanticProgram::new(vec![SemanticInstruction::Call {
                        register: b,
                        count: NonZeroU32::new(1).unwrap(),
                    }])?),
                )?;
                let context = SemanticContext {
                    original: None,
                    scoped: None,
                    hold_duration: None,
                    shared_hold_duration: None,
                    mixed_hold_duration: None,
                    selected_hold_duration: Some(Box::new(chosen.clone())),
                    parent: before.root().clone(),
                    cursor: visibility.frames.start(),
                    selected_child: Some(NodeId::new("top")?),
                    visual_selection: None,
                };
                let mut allocations = Vec::new();
                let plan = plan_semantic(
                    &before,
                    &context,
                    &SemanticProgram::new(vec![SemanticInstruction::Call {
                        register: a,
                        count: NonZeroU32::new(1).unwrap(),
                    }])?,
                    SemanticRegisterBank {
                        entries: &bank.entries,
                        version: bank.version,
                    },
                    RevisionId::new("selected-fixed-final")?,
                    |request| {
                        allocations.push(request.clone());
                        allocation(request)
                    },
                    |_, _| panic!("no Original resolver"),
                )?;
                let first_pools = match (axis, shared_other) {
                    (0, false) => [10, 10],
                    (0, true) => [6, 6],
                    (1, false) => [10, 4],
                    (1, true) => [4, 4],
                    _ => unreachable!(),
                };
                assert_eq!(
                    allocations,
                    first_pools
                        .into_iter()
                        .chain([0, 0])
                        .enumerate()
                        .map(|(step_index, nodes)| {
                            SemanticAllocationRequest::Isolation {
                                step_index,
                                nodes,
                                marks: 0,
                            }
                        })
                        .collect::<Vec<_>>()
                );
                assert_eq!(plan.context.cursor, visibility.frames.start());
                let mapped = plan.context.selected_hold_duration.as_ref().unwrap();
                assert_eq!(mapped.root, chosen.root);
                let request = plan.request.as_ref().unwrap();
                let Command::Compound { transaction } = &request.command else {
                    panic!("nested selected macro compiles to Compound")
                };
                assert_eq!(transaction.steps().len(), 4);
                let proof =
                    deadpan_core::derive_scoped_isolation(&before, request, &plan.document)?;
                assert_eq!(proof.map_forward(&chosen.target)?, mapped.target);
                assert_eq!(proof.map_forward(&chosen.also[0])?, mapped.also[0]);
                assert_eq!(
                    proof.map_instance_forward(chosen.presentation.as_ref().unwrap())?,
                    *mapped.presentation.as_ref().unwrap()
                );
                let original_second = instance(
                    &before,
                    if axis == 0 { 2 } else { 1 },
                    if axis == 1 { 2 } else { 1 },
                )?;
                let mapped_second = proof.map_instance_forward(&original_second)?;
                assert!(mapped.also[0].matches_instance(&plan.document, &mapped_second)?);
                let selected_targets = [&mapped.target, &mapped.also[0]];
                assert_ne!(selected_targets[0].node, selected_targets[1].node);
                for (old, new) in [&chosen.target, &chosen.also[0]]
                    .into_iter()
                    .zip(selected_targets)
                {
                    assert_eq!(
                        old.repeats
                            .iter()
                            .filter(|s| matches!(s.branch, RepeatEditBranch::Default))
                            .count(),
                        new.repeats
                            .iter()
                            .filter(|s| matches!(s.branch, RepeatEditBranch::Default))
                            .count()
                    );
                }
                let final_visibility = plan
                    .document
                    .selected_hold_duration_visibility(plan.document.root(), mapped)?
                    .unwrap();
                assert_eq!(
                    final_visibility.frames,
                    FrameRange::new(
                        ProjectFrame(48),
                        ProjectFrame(if final_frames == 8 { 52 } else { 57 })
                    )?
                );
                let replay = deadpan_core::replay_compound::<deadpan_core::EditError>(
                    &before,
                    request,
                    |_| Ok(()),
                )?;
                assert_eq!(replay.document, plan.document);
                assert_eq!(replay.edit.inverse.apply(&plan.document)?, before);
                let preview = store.preview_compound(request)?;
                assert_eq!(preview.register_bank, bank);
                assert_eq!(
                    preview.edit.unwrap().forward.apply_stored(&before)?,
                    plan.document
                );
                assert_eq!(store.snapshot()?, before);
                let outcome = store.commit_compound(request, None)?;
                assert_eq!(outcome.register_bank, bank);
                let committed = outcome.committed.unwrap();
                let after = store.snapshot()?;
                let heads = store.generation_intents(None, 256)?;
                assert_eq!(
                    heads.len(),
                    original_intents.len() + committed.generation_preparations.len()
                );
                for id in &committed.generation_preparations {
                    let head = heads
                        .iter()
                        .find(|intent| &intent.head.activation_id == id)
                        .unwrap();
                    assert!(selected_targets.contains(&&head.head.target));
                    assert_eq!(head.birth.origin_target, head.head.target);
                    assert_ne!(head.head.target.node, chosen.target.node);
                }
                assert_eq!(after, plan.document);
                assert_eq!(after.duration()?.frames(), 108);
                assert_eq!(after.gap_overrides(), before.gap_overrides());
                for (node, old) in before.nodes() {
                    assert_eq!(&after.nodes()[node], old, "retained template {node}");
                }
                let before_plan = deadpan_plan::RenderPlan::compile(&before)?;
                let after_plan = deadpan_plan::RenderPlan::compile(&after)?;
                for top in 0..3 {
                    for inner in 0..3 {
                        let start = i64::from(top) * 36 + i64::from(inner) * 12;
                        if affected(axis, shared_other, top, inner) {
                            assert_ne!(
                                after_plan.picture(ProjectFrame(start + 4))?.picture,
                                before_plan.picture(ProjectFrame(start + 4))?.picture
                            );
                        } else {
                            for frame in start..start + 12 {
                                assert_eq!(
                                    after_plan.picture(ProjectFrame(frame))?.picture,
                                    before_plan.picture(ProjectFrame(frame))?.picture,
                                    "every unrelated provider output at {frame}"
                                );
                            }
                        }
                    }
                }
                let current = store
                    .generation_request(&source_request.request_id)?
                    .unwrap();
                assert_eq!(current.target, source_request.target);
                assert_eq!(current.origin_target, source_request.origin_target);
                assert_eq!(current.origin_revision, source_request.origin_revision);
                assert_eq!(current.binding, source_request.binding);
                assert_eq!(current.provider, source_request.provider);
                assert_eq!(current.constraints, source_request.constraints);
                let expected_extension = PreparationOrigin::AcceptedExtension {
                    accepted: Box::new(accepted.artifact.clone()),
                    controls: PreparationControls::AcceptedArtifact,
                };
                assert_eq!(
                    committed.generation_preparations.len(),
                    if final_frames == 8 { 0 } else { 2 }
                );
                for target in selected_targets {
                    let NodeKind::Hold { recipe } = &after.nodes()[&target.node].kind else {
                        panic!("selected cloned Hold")
                    };
                    assert_eq!(
                        recipe.duration,
                        FrameDuration::new(i64::from(final_frames))?
                    );
                    if final_frames == 8 {
                        assert_eq!(
                            recipe.video,
                            HoldVideo::Generated {
                                accepted: accepted.clone()
                            }
                        );
                    } else {
                        assert_eq!(recipe.video, HoldVideo::Background);
                        let candidates = committed
                            .generation_preparations
                            .iter()
                            .map(|id| store.generation_preparation(id))
                            .collect::<std::result::Result<Vec<_>, _>>()?;
                        let matches = candidates
                            .into_iter()
                            .flatten()
                            .filter(|p| &p.target == target)
                            .collect::<Vec<_>>();
                        assert_eq!(
                            matches.len(),
                            1,
                            "one final preparation per selected target"
                        );
                        let pending = &matches[0];
                        assert_eq!(&pending.origin_target, target);
                        assert_eq!(pending.duration, FrameDuration::new(18)?);
                        assert_eq!(pending.state, PreparationState::Queued);
                        assert_eq!(pending.origin, expected_extension);
                        assert_eq!(pending.origin_revision, request.new_revision);
                    }
                }
                store.validate_full()?;
                // A pre-Compound request and a valid first leaf followed by an invalid
                // target each leave snapshots, durable rows and preparation state intact.
                let stale_duration = FrameDuration::new(11)?;
                let stale = CommandRequest {
                    project_id: before.project_id().clone(),
                    expected_revision: before.revision_id().clone(),
                    new_revision: RevisionId::new("selected-fixed-stale")?,
                    command: resize_command(
                        &before,
                        &chosen.target,
                        chosen.presentation.as_ref().unwrap(),
                        stale_duration,
                        "stale",
                    )?,
                };
                let late = command(
                    &store,
                    "selected-fixed-late",
                    Command::Compound {
                        transaction: ResolvedTransaction::new(
                            bank.version,
                            BTreeMap::new(),
                            vec![
                                ResolvedStep::Edit {
                                    edit: LeafEdit::new(
                                        RevisionId::new("selected-fixed-late-valid")?,
                                        resize_command(
                                            &after,
                                            &mapped.target,
                                            mapped.presentation.as_ref().unwrap(),
                                            FrameDuration::new(9)?,
                                            "late",
                                        )?,
                                    )?,
                                },
                                ResolvedStep::Edit {
                                    edit: LeafEdit::new(
                                        RevisionId::new("selected-fixed-late-invalid")?,
                                        Command::Rename {
                                            node: NodeId::new("missing-target")?,
                                            label: "Must not commit".into(),
                                        },
                                    )?,
                                },
                            ],
                        )?,
                    },
                )?;
                for (invalid, compound) in [(&stale, false), (&late, true)] {
                    let rows = db_counts(&package)?;
                    let preparations = committed
                        .generation_preparations
                        .iter()
                        .map(|id| store.generation_preparation(id))
                        .collect::<std::result::Result<Vec<_>, _>>()?;
                    let old_request = store.generation_request(&source_request.request_id)?;
                    if compound {
                        assert!(store.commit_compound(invalid, None).is_err());
                    } else {
                        assert!(store.commit(invalid).is_err());
                    }
                    assert_eq!(store.snapshot()?, after);
                    assert_eq!(store.registers()?, bank);
                    assert_eq!(db_counts(&package)?, rows);
                    assert_eq!(
                        store.generation_request(&source_request.request_id)?,
                        old_request
                    );
                    assert_eq!(
                        committed
                            .generation_preparations
                            .iter()
                            .map(|id| store.generation_preparation(id))
                            .collect::<std::result::Result<Vec<_>, _>>()?,
                        preparations
                    );
                }
                drop(store);
                let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
                store.set_generation_context_resolver(Arc::new(Relevant));
                assert_eq!(store.snapshot()?, after);
                assert_eq!(store.registers()?, bank);
                store.undo(
                    &store.head_revision()?,
                    RevisionId::new("selected-fixed-undo")?,
                )?;
                content_eq(&store.snapshot()?, &before)?;
                assert_eq!(
                    store
                        .generation_intents(None, 256)?
                        .into_iter()
                        .map(|intent| (intent.head, intent.birth))
                        .collect::<Vec<_>>(),
                    original_intents,
                    "Undo never transfers a cloned intent onto the shared original Hold"
                );
                assert_eq!(
                    store
                        .generation_request(&source_request.request_id)?
                        .unwrap()
                        .target,
                    source_request.target
                );
                let db = rusqlite::Connection::open(package.join("project.sqlite"))?;
                for id in &committed.generation_preparations {
                    assert_eq!(
                        store.generation_preparation(id)?.unwrap().state,
                        PreparationState::Cancelled
                    );
                    let text: String = db.query_row(
                        "SELECT record FROM generation_intent_terminals WHERE activation_id=?1",
                        [id.as_str()],
                        |row| row.get(0),
                    )?;
                    let terminal: deadpan_store::generation_intents::IntentTerminal =
                        serde_json::from_str(&text)?;
                    let original_head = heads
                        .iter()
                        .find(|intent| &intent.head.activation_id == id)
                        .unwrap();
                    assert_eq!(terminal.target, original_head.head.target);
                    assert_eq!(terminal.at_revision, store.head_revision()?);
                    assert_eq!(
                        terminal.phase,
                        deadpan_store::generation_intents::TerminalPhase::Transition
                    );
                    assert_eq!(
                        terminal.reason,
                        deadpan_store::generation_intents::IntentTerminalReason::Deleted
                    );
                }
                drop(db);
                store.validate_full()?;
                drop(store);
                let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
                store.set_generation_context_resolver(Arc::new(Relevant));
                content_eq(&store.snapshot()?, &before)?;
                assert_eq!(
                    store.generation_intents(None, 256)?.len(),
                    original_intents.len()
                );
                let redo = store.redo(
                    &store.head_revision()?,
                    RevisionId::new("selected-fixed-redo")?,
                )?;
                content_eq(&store.snapshot()?, &after)?;
                assert_eq!(
                    redo.generation_preparations.len(),
                    committed.generation_preparations.len()
                );
                for id in &redo.generation_preparations {
                    assert!(
                        !committed.generation_preparations.contains(id),
                        "Redo allocates fresh activation authority"
                    );
                    let pending = store.generation_preparation(id)?.unwrap();
                    assert!(selected_targets.contains(&&pending.target));
                    assert_eq!(pending.origin_target, pending.target);
                    assert_eq!(pending.duration, FrameDuration::new(18)?);
                    assert_eq!(pending.state, PreparationState::Queued);
                    assert_eq!(pending.origin, expected_extension);
                    let intent = store
                        .generation_intents(None, 256)?
                        .into_iter()
                        .find(|intent| &intent.head.activation_id == id)
                        .unwrap();
                    let deadpan_store::generation_intents::IntentAuthorization::Redo {
                        original_activation,
                    } = &intent.birth.receipt.authorization
                    else {
                        panic!("Redo must retain its original immutable authorization")
                    };
                    assert!(
                        committed
                            .generation_preparations
                            .contains(original_activation)
                    );
                    let original_head = heads
                        .iter()
                        .find(|intent| &intent.head.activation_id == original_activation)
                        .unwrap();
                    assert_eq!(intent.head.target, original_head.head.target);
                }
                store.validate_full()?;
                drop(store);
                let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
                content_eq(&store.snapshot()?, &after)?;
                assert_eq!(store.registers()?, bank);
                store.validate_full()?;
            }
        }
    }
    Ok(())
}
