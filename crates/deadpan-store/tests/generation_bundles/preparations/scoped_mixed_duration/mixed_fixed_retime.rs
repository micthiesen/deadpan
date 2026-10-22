//! Mixed Default/Play authority survives a fixed stage, generation and history.
use super::nested_fixed_retime::{allocation, content_eq, db_counts, fixed_repeat_initial, set};
use super::*;
use deadpan_core::{
    FrameRange, InstancePath, LeafEdit, ProjectFrame, RegisterName, RepeatInstance, ResolvedStep,
    ResolvedTransaction, SemanticAllocationRequest, SemanticContext, SemanticInstruction,
    SemanticMixedHoldDurationContext, SemanticProgram, SemanticRegisterBank, plan_semantic,
};
use std::num::NonZeroU32;
use std::sync::Arc;
fn representative(document: &ProjectDocument) -> Result<InstancePath> {
    let repeat = |name: &str| -> Result<RepeatInstance> {
        let node = NodeId::new(name)?;
        let NodeKind::Repeat { iterations, .. } = &document.nodes()[&node].kind else {
            return Err("Repeat required".into());
        };
        Ok(RepeatInstance {
            node,
            iteration: iterations.at(1).unwrap(),
        })
    };
    Ok(InstancePath {
        node: NodeId::new("hold")?,
        repeats: vec![repeat("top")?, repeat("outer")?],
    })
}

fn target(instance: &InstancePath, top_play: bool) -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: instance.node.clone(),
        repeats: instance
            .repeats
            .iter()
            .enumerate()
            .map(|(index, repeat)| RepeatEditStep {
                repeat: repeat.node.clone(),
                branch: if (index == 0) == top_play {
                    RepeatEditBranch::Play {
                        iteration: repeat.iteration.clone(),
                    }
                } else {
                    RepeatEditBranch::Default
                },
            })
            .collect(),
    }
}

fn affected_windows(top_play: bool) -> &'static [(i64, i64)] {
    if top_play {
        &[(48, 60), (60, 72)]
    } else {
        &[(12, 24), (48, 60), (84, 96)]
    }
}

#[test]
fn mixed_fixed_retime_macro_keeps_default_authority_generation_and_history_atomic() -> Result {
    for top_play in [false, true] {
        for final_frames in [8, 18] {
            let scratch = tempfile::tempdir()?;
            let package = scratch
                .path()
                .join(format!("mixed-fixed-{top_play}-{final_frames}.deadpan"));
            let mut store = ProjectStore::create(&package, &fixed_repeat_initial()?)?;
            store.set_generation_context_resolver(Arc::new(Relevant));
            let instance = representative(&store.snapshot()?)?;
            let chosen = target(&instance, top_play);
            let mut shared = chosen.clone();
            for step in &mut shared.repeats {
                step.branch = RepeatEditBranch::Default;
            }
            accepted_for_duration(
                &mut store,
                &shared,
                FrameDuration::new(12)?,
                "mixed-fixed-accepted",
                73,
            )?;
            let before = store.snapshot()?;
            let source_request = store
                .generation_request(&RequestId::new("mixed-fixed-accepted-request")?)?
                .unwrap();
            assert_eq!(source_request.target, shared);
            let NodeKind::Hold {
                recipe: accepted_recipe,
            } = &before.nodes()[&chosen.node].kind
            else {
                panic!("accepted Hold")
            };
            let HoldVideo::Generated { accepted } = &accepted_recipe.video else {
                panic!("only a bundle-admitted synthetic provider is accepted here")
            };
            let accepted = accepted.clone();
            let authority = SemanticMixedHoldDurationContext {
                root: NodeId::new("top")?,
                target: chosen.clone(),
                presentation: Some(instance.clone()),
            };
            let visibility = before
                .mixed_hold_duration_visibility(before.root(), &authority)?
                .ok_or("live mixed family must have representative visibility")?;
            assert_eq!(
                visibility.frames,
                FrameRange::new(ProjectFrame(48), ProjectFrame(54))?
            );

            // Saved nested calls retain the literal Default and Play choices.
            // First isolate the family; the next leaf reuses it with no identities.
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
                mixed_hold_duration: Some(Box::new(authority.clone())),
                selected_hold_duration: None,
                parent: before.root().clone(),
                cursor: visibility.frames.start(),
                selected_child: Some(NodeId::new("top")?),
                visual_selection: None,
            };
            let mut allocation_requests = Vec::new();
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
                RevisionId::new("mixed-fixed-final")?,
                |request| {
                    allocation_requests.push(request.clone());
                    allocation(request)
                },
                |_, _| panic!("no Original resolver"),
            )?;
            assert_eq!(
                allocation_requests,
                vec![
                    SemanticAllocationRequest::Isolation {
                        step_index: 0,
                        nodes: if top_play { 6 } else { 4 },
                        marks: 0,
                    },
                    SemanticAllocationRequest::Isolation {
                        step_index: 1,
                        nodes: 0,
                        marks: 0,
                    },
                ]
            );
            assert_eq!(
                plan.context.mixed_hold_duration.as_ref().unwrap().root,
                context.mixed_hold_duration.as_ref().unwrap().root
            );
            assert_eq!(plan.context.cursor, visibility.frames.start());
            let request = plan.request.as_ref().unwrap();
            let Command::Compound { transaction } = &request.command else {
                panic!("nested saved macro compiles to a Compound")
            };
            assert_eq!(transaction.steps().len(), 2);
            let first = transaction.steps()[0].edit().unwrap().request(&before);
            let prepared =
                deadpan_core::prepare_scoped_hold_duration(&before, &first, Some(&instance))?;
            assert_ne!(prepared.target.node, chosen.node);
            assert_eq!(
                prepared.presentation,
                plan.context
                    .mixed_hold_duration
                    .as_ref()
                    .unwrap()
                    .presentation
            );
            assert_eq!(
                prepared.boundary.as_ref().unwrap().frame,
                visibility.frames.start()
            );
            prepared
                .isolation()
                .validate(&before, &first, &prepared.document)?;
            let second = transaction.steps()[1]
                .edit()
                .unwrap()
                .request(&prepared.document);
            let final_prepared = deadpan_core::prepare_scoped_hold_duration(
                &prepared.document,
                &second,
                prepared.presentation.as_ref(),
            )?;
            assert!(final_prepared.isolation().steps().is_empty());
            assert_eq!(final_prepared.target, prepared.target);
            assert_eq!(final_prepared.presentation, prepared.presentation);
            assert_eq!(
                final_prepared.fixed_visibility().unwrap().frames,
                FrameRange::new(
                    ProjectFrame(48),
                    ProjectFrame(if final_frames == 8 { 52 } else { 57 }),
                )?
            );
            content_eq(&final_prepared.document, &plan.document)?;
            let replay =
                deadpan_core::replay_compound::<deadpan_core::EditError>(&before, request, |_| {
                    Ok(())
                })?;
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
            assert_eq!(after, plan.document);
            assert_eq!(after.duration()?.frames(), before.duration()?.frames());
            assert_eq!(
                after.overrides().len(),
                before.overrides().len() + if top_play { 2 } else { 0 }
            );
            assert_eq!(after.gap_overrides(), before.gap_overrides());
            assert_eq!(after.duration()?.frames(), 108);
            assert_eq!(prepared.isolation().steps().len(), 1);
            assert_eq!(
                prepared.isolation().steps()[0].nodes().len(),
                if top_play { 6 } else { 4 }
            );
            assert!(matches!(
                prepared.target.repeats[usize::from(top_play)].branch,
                RepeatEditBranch::Default
            ));
            let outer = NodeId::new("outer")?;
            let NodeKind::Repeat { iterations, .. } = &after.nodes()[&outer].kind else {
                panic!("inner Repeat retained")
            };
            assert_eq!(
                after.overrides()[&outer].get(&iterations.at(0).unwrap()),
                Some(&NodeId::new("owned")?)
            );
            if top_play {
                let cloned_owned = &prepared.isolation().steps()[0].nodes()[&NodeId::new("owned")?];
                assert_eq!(
                    after.nodes()[cloned_owned],
                    before.nodes()[&NodeId::new("owned")?]
                );
            }
            for (node, original) in before.nodes() {
                assert_eq!(&after.nodes()[node], original, "retained template {node}");
            }
            let before_plan = deadpan_plan::RenderPlan::compile(&before)?;
            let after_plan = deadpan_plan::RenderPlan::compile(&after)?;
            for frame in (0..108).filter(|frame| {
                !affected_windows(top_play)
                    .iter()
                    .any(|&(start, end)| (start..end).contains(frame))
            }) {
                assert_eq!(
                    after_plan.picture(ProjectFrame(frame))?.picture,
                    before_plan.picture(ProjectFrame(frame))?.picture,
                    "every unselected provider output at root frame {frame}"
                );
            }
            for &(start, _) in affected_windows(top_play) {
                assert_ne!(
                    after_plan.picture(ProjectFrame(start + 4))?.picture,
                    before_plan.picture(ProjectFrame(start + 4))?.picture,
                    "every affected family changes provider output at {start}"
                );
            }
            let NodeKind::Hold { recipe } = &after.nodes()[&prepared.target.node].kind else {
                panic!("isolated duration target")
            };
            assert_eq!(
                recipe.duration,
                FrameDuration::new(i64::from(final_frames))?
            );
            let current = store
                .generation_request(&source_request.request_id)?
                .unwrap();
            assert_eq!(current.target, source_request.target);
            assert_eq!(current.origin_target, source_request.origin_target);
            assert_eq!(current.origin_revision, source_request.origin_revision);
            assert_eq!(current.binding, source_request.binding);
            assert_eq!(current.provider, source_request.provider);
            assert_eq!(current.constraints, source_request.constraints);
            // The admitted request belongs to the shared Default, while this new
            // preparation belongs to an isolated Play. Its controls therefore come
            // from the immutable accepted artifact, not that other scoped request.
            let expected_extension = PreparationOrigin::AcceptedExtension {
                accepted: Box::new(accepted.artifact.clone()),
                controls: PreparationControls::AcceptedArtifact,
            };
            if final_frames == 8 {
                assert!(committed.generation_preparations.is_empty());
                assert_eq!(recipe.video, HoldVideo::Generated { accepted });
            } else {
                assert_eq!(committed.generation_preparations.len(), 1);
                assert_eq!(recipe.video, HoldVideo::Background);
                let pending = store
                    .generation_preparation(&committed.generation_preparations[0])?
                    .unwrap();
                assert_eq!(pending.target, prepared.target);
                assert_eq!(pending.origin_target, prepared.target);
                assert_eq!(pending.duration, FrameDuration::new(18)?);
                assert_eq!(pending.state, PreparationState::Queued);
                assert_eq!(pending.origin, expected_extension);
                assert_eq!(pending.origin_revision, request.new_revision);
            }
            store.validate_full()?;

            // A request captured before the Compound is stale now. Its rejection
            // must leave both content and all durable history rows untouched.
            let stale_duration = FrameDuration::new(11)?;
            let stale_needs = before.scoped_hold_duration_requirements(&chosen, stale_duration)?;
            let stale = CommandRequest {
                project_id: before.project_id().clone(),
                expected_revision: before.revision_id().clone(),
                new_revision: RevisionId::new("mixed-fixed-stale-request")?,
                command: Command::SetScopedHoldDuration {
                    target: chosen.clone(),
                    duration: stale_duration,
                    identities: OccurrenceIdentities {
                        nodes: (0..stale_needs.nodes)
                            .map(|index| NodeId::new(format!("stale-node-{index}")))
                            .collect::<std::result::Result<_, _>>()?,
                        marks: (0..stale_needs.marks)
                            .map(|index| deadpan_core::MarkId::new(format!("stale-mark-{index}")))
                            .collect::<std::result::Result<_, _>>()?,
                    },
                },
            };
            let row_counts = db_counts(&package)?;
            let before_refusal = store.snapshot()?;
            let request_before_refusal = store.generation_request(&source_request.request_id)?;
            let preparations_before_refusal = committed
                .generation_preparations
                .iter()
                .map(|id| store.generation_preparation(id))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            assert!(store.commit(&stale).is_err());
            assert_eq!(store.snapshot()?, before_refusal);
            assert_eq!(store.registers()?, bank);
            assert_eq!(db_counts(&package)?, row_counts);
            assert_eq!(
                store.generation_request(&source_request.request_id)?,
                request_before_refusal
            );
            assert_eq!(
                committed
                    .generation_preparations
                    .iter()
                    .map(|id| store.generation_preparation(id))
                    .collect::<std::result::Result<Vec<_>, _>>()?,
                preparations_before_refusal
            );

            // A second Compound whose first valid duration leaf is followed by a
            // missing target is also rejected before publishing partial history.
            let mapped_target = prepared.target.clone();
            let late_duration = FrameDuration::new(9)?;
            let late_needs =
                after.scoped_hold_duration_requirements(&mapped_target, late_duration)?;
            assert_eq!((late_needs.nodes, late_needs.marks), (0, 0));
            let late_first = LeafEdit::new(
                RevisionId::new("mixed-fixed-late-valid")?,
                Command::SetScopedHoldDuration {
                    target: mapped_target,
                    duration: late_duration,
                    identities: OccurrenceIdentities {
                        nodes: Vec::new(),
                        marks: Vec::new(),
                    },
                },
            )?;
            let late_second = LeafEdit::new(
                RevisionId::new("mixed-fixed-late-missing")?,
                Command::Rename {
                    node: NodeId::new("missing-target")?,
                    label: "Must not commit".into(),
                },
            )?;
            let late = command(
                &store,
                "mixed-fixed-late-refusal",
                Command::Compound {
                    transaction: ResolvedTransaction::new(
                        bank.version,
                        BTreeMap::new(),
                        vec![
                            ResolvedStep::Edit { edit: late_first },
                            ResolvedStep::Edit { edit: late_second },
                        ],
                    )?,
                },
            )?;
            let row_counts = db_counts(&package)?;
            let before_refusal = store.snapshot()?;
            let request_before_refusal = store.generation_request(&source_request.request_id)?;
            let preparations_before_refusal = committed
                .generation_preparations
                .iter()
                .map(|id| store.generation_preparation(id))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            assert!(store.commit_compound(&late, None).is_err());
            assert_eq!(store.snapshot()?, before_refusal);
            assert_eq!(store.registers()?, bank);
            assert_eq!(db_counts(&package)?, row_counts);
            assert_eq!(
                store.generation_request(&source_request.request_id)?,
                request_before_refusal
            );
            assert_eq!(
                committed
                    .generation_preparations
                    .iter()
                    .map(|id| store.generation_preparation(id))
                    .collect::<std::result::Result<Vec<_>, _>>()?,
                preparations_before_refusal
            );

            drop(store);
            let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
            store.set_generation_context_resolver(Arc::new(Relevant));
            assert_eq!(store.snapshot()?, after);
            assert_eq!(store.registers()?, bank);
            store.undo(
                &store.head_revision()?,
                RevisionId::new("mixed-fixed-undo")?,
            )?;
            content_eq(&store.snapshot()?, &before)?;
            if final_frames == 18 {
                assert_eq!(
                    store
                        .generation_preparation(&committed.generation_preparations[0])?
                        .unwrap()
                        .state,
                    PreparationState::Cancelled
                );
            }
            let redo = store.redo(
                &store.head_revision()?,
                RevisionId::new("mixed-fixed-redo")?,
            )?;
            content_eq(&store.snapshot()?, &after)?;
            if final_frames == 18 {
                assert_eq!(redo.generation_preparations.len(), 1);
                let replayed = store
                    .generation_preparation(&redo.generation_preparations[0])?
                    .unwrap();
                assert_eq!(replayed.target, prepared.target);
                assert_eq!(replayed.origin_target, prepared.target);
                assert_eq!(replayed.duration, FrameDuration::new(18)?);
                assert_eq!(replayed.state, PreparationState::Queued);
                assert_eq!(replayed.origin, expected_extension);
            } else {
                assert!(redo.generation_preparations.is_empty());
            }
            store.validate_full()?;
            drop(store);
            let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
            content_eq(&store.snapshot()?, &after)?;
            assert_eq!(store.registers()?, bank);
            store.validate_full()?;
        }
    }
    Ok(())
}
