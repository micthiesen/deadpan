//! Shared fixed-window semantic Calls retain admitted providers and durable intent.
//! Artifact bytes come from the existing synthetic admission fixture, not inference.
use super::*;
use deadpan_core::{
    FrameRange, InstancePath, IterationOrder, PauseLength, PitchPolicy, ProjectFrame, RegisterName,
    RepeatInstance, ResolvedStep, RetimePurpose, SemanticAllocation, SemanticAllocationRequest,
    SemanticContext, SemanticInstruction, SemanticProgram, SemanticRegisterBank,
    SemanticSharedHoldDurationContext, plan_semantic,
};
use deadpan_jobs::GenerationOptions;
use std::num::NonZeroU32;
use std::sync::Arc;

fn initial() -> Result<ProjectDocument> {
    let base = document()?;
    let mut wire = serde_json::to_value(&base)?;
    let quiet = |frames| -> Result<BeatNode> {
        Ok(BeatNode::hold(
            "Background",
            HoldRecipe {
                duration: FrameDuration::new(frames)?,
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
                picture_context: None,
            },
        ))
    };
    wire["nodes"]["suffix"] = serde_json::to_value(quiet(24)?)?;
    wire["nodes"]["operand"] = serde_json::to_value(BeatNode::sequence(
        "Fixed input",
        vec![NodeId::new("hold")?, NodeId::new("suffix")?],
    ))?;
    let mut stage = BeatNode::sequence("Fixed stage", Vec::new());
    stage.kind = NodeKind::Retime {
        child: NodeId::new("operand")?,
        duration: FrameDuration::new(12)?,
        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(24))?,
        pitch: PitchPolicy::FollowSpeed,
        purpose: RetimePurpose::Edit,
    };
    wire["nodes"]["stage"] = serde_json::to_value(stage)?;
    wire["nodes"]["owned"] = serde_json::to_value(quiet(12)?)?;
    let order = IterationOrder::new(RevisionId::new("fixed-outer-plays")?, 3)?;
    let mut outer = BeatNode::sequence("Outer", Vec::new());
    outer.kind = NodeKind::Repeat {
        child: NodeId::new("stage")?,
        iterations: order.clone(),
        gap: None,
        escalation: None,
    };
    wire["nodes"]["outer"] = serde_json::to_value(outer)?;
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["outer"]);
    wire["overrides"]["outer"] =
        serde_json::to_value(deadpan_core::PlayOverrides::try_from(vec![
            deadpan_core::PlayOverride {
                iteration: order.at(1).unwrap(),
                root: NodeId::new("owned")?,
            },
        ])?)?;
    Ok(ProjectDocument::from_json(&wire.to_string())?)
}

fn shared_target() -> Result<ScopedNodeTarget> {
    Ok(ScopedNodeTarget {
        node: NodeId::new("hold")?,
        repeats: vec![RepeatEditStep {
            repeat: NodeId::new("outer")?,
            branch: RepeatEditBranch::Default,
        }],
    })
}

fn content_eq(document: &ProjectDocument, original: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(original)?;
    expected["revision_id"] = serde_json::json!(document.revision_id());
    assert_eq!(serde_json::to_value(document)?, expected);
    Ok(())
}

#[test]
fn shared_fixed_retime_duration_calls_retain_accepted_shortening_and_exact_extension_birth_through_history()
-> Result {
    for final_frames in [8, 18] {
        let scratch = tempfile::tempdir()?;
        let package = scratch
            .path()
            .join(format!("shared-fixed-{final_frames}.deadpan"));
        let mut store = ProjectStore::create(&package, &initial()?)?;
        store.set_generation_context_resolver(Arc::new(Relevant));
        let target = shared_target()?;
        accepted_for_duration(
            &mut store,
            &target,
            FrameDuration::new(12)?,
            "fixed-accepted",
            42,
        )?;
        let source_request = store
            .generation_request(&RequestId::new("fixed-accepted-request")?)?
            .unwrap();
        let before = store.snapshot()?;
        let NodeKind::Hold {
            recipe: accepted_recipe,
        } = &before.nodes()[&target.node].kind
        else {
            panic!("accepted Hold")
        };
        let HoldVideo::Generated { accepted } = &accepted_recipe.video else {
            panic!("admitted provider")
        };
        let outer = NodeId::new("outer")?;
        let NodeKind::Repeat { iterations, .. } = &before.nodes()[&outer].kind else {
            panic!("outer")
        };
        let authority = SemanticSharedHoldDurationContext {
            root: outer.clone(),
            target: target.clone(),
            presentation: Some(InstancePath {
                node: target.node.clone(),
                repeats: vec![RepeatInstance {
                    node: outer.clone(),
                    iteration: iterations.at(2).unwrap(),
                }],
            }),
        };
        let visible = before
            .shared_hold_duration_visibility(before.root(), &authority)?
            .unwrap();
        assert_eq!(
            visible.frames,
            FrameRange::new(ProjectFrame(24), ProjectFrame(30))?
        );
        let a = RegisterName::new('a')?;
        let b = RegisterName::new('b')?;
        let setter = |frames| SemanticInstruction::SetHoldDuration {
            length: PauseLength::Frames {
                frames: NonZeroU32::new(frames).unwrap(),
            },
        };
        let bank = store.save_macro(
            before.project_id(),
            before.revision_id(),
            store.register_version()?,
            b,
            Arc::new(SemanticProgram::new(vec![
                setter(if final_frames == 8 { 10 } else { 14 }),
                setter(final_frames),
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
            mixed_hold_duration: None,
            selected_hold_duration: None,
            shared_hold_duration: Some(authority),
            parent: before.root().clone(),
            cursor: visible.frames.start(),
            selected_child: Some(outer),
            visual_selection: None,
        };
        let mut allocations = 0;
        let plan = plan_semantic(
            &before,
            &context,
            &SemanticProgram::new(vec![SemanticInstruction::Call {
                register: a,
                count: NonZeroU32::new(2).unwrap(),
            }])?,
            SemanticRegisterBank {
                entries: &bank.entries,
                version: bank.version,
            },
            RevisionId::new("shared-fixed-final")?,
            |request| {
                let SemanticAllocationRequest::ParameterEdit { step_index } = request else {
                    panic!("no isolation")
                };
                allocations += 1;
                Ok(SemanticAllocation::ParameterEdit {
                    new_revision: RevisionId::new(format!("fixed-leaf-{step_index}")).unwrap(),
                })
            },
            |_, _| panic!("no Original resolver"),
        )?;
        assert_eq!(allocations, 4);
        assert_eq!(
            plan.context.shared_hold_duration,
            context.shared_hold_duration
        );
        assert_eq!(plan.context.cursor, ProjectFrame(24));
        let request = plan.request.as_ref().unwrap();
        let Command::Compound { transaction } = &request.command else {
            panic!("one Compound")
        };
        assert_eq!(transaction.steps().len(), 4);
        for step in transaction.steps() {
            let ResolvedStep::Edit { edit } = step else {
                panic!("parameter edit")
            };
            assert!(
                matches!(edit.command.as_command(), Command::SetHoldDuration { node, .. } if *node == target.node)
            );
        }
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
        assert_eq!(after.duration()?.frames(), 36);
        assert_eq!(
            after.nodes().keys().collect::<Vec<_>>(),
            before.nodes().keys().collect::<Vec<_>>()
        );
        assert_eq!(after.overrides(), before.overrides());
        assert_eq!(after.gap_overrides(), before.gap_overrides());
        for (node, original) in before.nodes() {
            if *node != target.node {
                assert_eq!(&after.nodes()[node], original);
            }
        }
        let NodeKind::Hold { recipe } = &after.nodes()[&target.node].kind else {
            panic!("resized Hold")
        };
        assert_eq!(recipe.duration.frames(), i64::from(final_frames));
        let current = store
            .generation_request(&source_request.request_id)?
            .unwrap();
        assert_eq!(current.target, target);
        assert_eq!(current.origin_target, source_request.origin_target);
        assert_eq!(current.binding, source_request.binding);
        assert_eq!(current.provider, source_request.provider);
        assert_eq!(current.constraints, source_request.constraints);
        assert_eq!(current.relevance, deadpan_jobs::Relevance::Stale);
        let preparation = if final_frames == 8 {
            assert_eq!(recipe.video, accepted_recipe.video);
            assert!(committed.generation_preparations.is_empty());
            None
        } else {
            assert_eq!(recipe.video, HoldVideo::Background);
            assert_eq!(committed.generation_preparations.len(), 1);
            let p = store
                .generation_preparation(&committed.generation_preparations[0])?
                .unwrap();
            assert_eq!(p.target, target);
            assert_eq!(p.origin_target, target);
            assert_eq!(p.duration.frames(), 18);
            assert_eq!(p.state, PreparationState::Queued);
            assert_eq!(p.origin_revision, request.new_revision);
            assert_eq!(
                p.origin,
                PreparationOrigin::AcceptedExtension {
                    accepted: Box::new(accepted.artifact.clone()),
                    controls: PreparationControls::Request {
                        request_id: source_request.request_id.clone(),
                        options: GenerationOptions::from_constraints(&source_request.constraints),
                    },
                }
            );
            Some(p)
        };
        store.validate_full()?;
        drop(store);
        let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
        store.set_generation_context_resolver(Arc::new(Relevant));
        assert_eq!(store.snapshot()?, after);
        assert_eq!(store.registers()?, bank);
        if let Some(p) = &preparation {
            assert_eq!(store.generation_preparation(&p.id)?, Some(p.clone()));
        }
        store.undo(&store.head_revision()?, RevisionId::new("fixed-undo")?)?;
        content_eq(&store.snapshot()?, &before)?;
        assert_eq!(store.registers()?, bank);
        if let Some(p) = &preparation {
            assert_eq!(
                store.generation_preparation(&p.id)?.unwrap().state,
                PreparationState::Cancelled
            );
        }
        let redo = store.redo(&store.head_revision()?, RevisionId::new("fixed-redo")?)?;
        content_eq(&store.snapshot()?, &after)?;
        assert_eq!(store.registers()?, bank);
        if let Some(original) = preparation {
            assert_eq!(redo.generation_preparations.len(), 1);
            assert_ne!(redo.generation_preparations[0], original.id);
            let replayed = store
                .generation_preparation(&redo.generation_preparations[0])?
                .unwrap();
            assert_eq!(replayed.target, original.target);
            assert_eq!(replayed.origin_target, original.origin_target);
            assert_eq!(replayed.origin, original.origin);
            assert_eq!(replayed.duration, original.duration);
            assert_eq!(replayed.state, PreparationState::Queued);
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
    Ok(())
}
