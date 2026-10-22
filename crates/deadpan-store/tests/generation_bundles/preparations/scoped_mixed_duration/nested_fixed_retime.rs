//! Nested explicit Repeat choices keep admitted synthetic media, mapped
//! preparation intent and a saved nested macro durable in one Compound.
use super::*;
use deadpan_core::{
    FrameRange, InstancePath, IterationOrder, LeafEdit, OccurrenceEdit, PauseLength, PitchPolicy,
    PlayOverride, PlayOverrides, ProjectFrame, RegisterName, RepeatInstance, ResolvedStep,
    ResolvedTransaction, RetimePurpose, SemanticAllocation, SemanticAllocationRequest,
    SemanticContext, SemanticInstruction, SemanticProgram, SemanticRegisterBank,
    SemanticScopedContext, plan_semantic,
};
use std::num::NonZeroU32;
use std::sync::Arc;

pub(super) fn fixed_repeat_initial() -> Result<ProjectDocument> {
    let mut wire = serde_json::to_value(document()?)?;
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
    let order = IterationOrder::new(RevisionId::new("nested-fixed-repeat")?, 3)?;
    let mut outer = BeatNode::sequence("Three fixed stages", Vec::new());
    outer.kind = NodeKind::Repeat {
        child: NodeId::new("stage")?,
        iterations: order.clone(),
        gap: None,
        escalation: None,
    };
    wire["nodes"]["outer"] = serde_json::to_value(outer)?;
    let mut top = BeatNode::sequence("Three nested families", Vec::new());
    top.kind = NodeKind::Repeat {
        child: NodeId::new("outer")?,
        iterations: IterationOrder::new(RevisionId::new("nested-fixed-top")?, 3)?,
        gap: None,
        escalation: None,
    };
    wire["nodes"]["top"] = serde_json::to_value(top)?;
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["top"]);
    wire["overrides"]["outer"] =
        serde_json::to_value(PlayOverrides::try_from(vec![PlayOverride {
            iteration: order.at(0).unwrap(),
            root: NodeId::new("owned")?,
        }])?)?;
    let document = ProjectDocument::from_json(&wire.to_string())?;
    document.validate()?;
    Ok(document)
}

fn target(document: &ProjectDocument, play: u32) -> Result<ScopedNodeTarget> {
    let repeat = NodeId::new("outer")?;
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&repeat].kind else {
        return Err("outer Repeat required".into());
    };
    Ok(ScopedNodeTarget {
        node: NodeId::new("hold")?,
        repeats: vec![
            RepeatEditStep {
                repeat: NodeId::new("top")?,
                branch: RepeatEditBranch::Play {
                    iteration: match &document.nodes()[&NodeId::new("top")?].kind {
                        NodeKind::Repeat { iterations, .. } => iterations.at(1).unwrap(),
                        _ => return Err("top Repeat required".into()),
                    },
                },
            },
            RepeatEditStep {
                repeat,
                branch: RepeatEditBranch::Play {
                    iteration: iterations.at(play).ok_or("Repeat Play absent")?,
                },
            },
        ],
    })
}

fn shared_target() -> Result<ScopedNodeTarget> {
    Ok(ScopedNodeTarget {
        node: NodeId::new("hold")?,
        repeats: vec![
            RepeatEditStep {
                repeat: NodeId::new("top")?,
                branch: RepeatEditBranch::Default,
            },
            RepeatEditStep {
                repeat: NodeId::new("outer")?,
                branch: RepeatEditBranch::Default,
            },
        ],
    })
}

fn presentation(document: &ProjectDocument, target: &ScopedNodeTarget) -> Result<InstancePath> {
    target.validate(document)?;
    Ok(InstancePath {
        node: target.node.clone(),
        repeats: target
            .repeats
            .iter()
            .map(|step| {
                let RepeatEditBranch::Play { iteration } = &step.branch else {
                    return Err("explicit Play required".into());
                };
                Ok(RepeatInstance {
                    node: step.repeat.clone(),
                    iteration: iteration.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?,
    })
}

pub(super) fn set(frames: u32) -> SemanticInstruction {
    SemanticInstruction::SetHoldDuration {
        length: PauseLength::Frames {
            frames: NonZeroU32::new(frames).unwrap(),
        },
    }
}

pub(super) fn allocation(
    request: SemanticAllocationRequest,
) -> std::result::Result<SemanticAllocation, deadpan_core::EditError> {
    let SemanticAllocationRequest::Isolation {
        step_index,
        nodes,
        marks,
    } = request
    else {
        panic!("occurrence duration must request exact isolation pools only")
    };
    Ok(SemanticAllocation::Isolation {
        new_revision: RevisionId::new(format!("nested-fixed-leaf-{step_index}")).unwrap(),
        identities: OccurrenceIdentities {
            nodes: (0..nodes)
                .map(|index| {
                    NodeId::new(format!("nested-fixed-node-{step_index}-{index}")).unwrap()
                })
                .collect(),
            marks: (0..marks)
                .map(|index| {
                    deadpan_core::MarkId::new(format!("nested-fixed-mark-{step_index}-{index}"))
                        .unwrap()
                })
                .collect(),
        },
    })
}

pub(super) fn content_eq(document: &ProjectDocument, original: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(original)?;
    expected["revision_id"] = serde_json::json!(document.revision_id());
    assert_eq!(serde_json::to_value(document)?, expected);
    Ok(())
}

pub(super) fn db_counts(package: &std::path::Path) -> Result<[i64; 6]> {
    let db = rusqlite::Connection::open(package.join("project.sqlite"))?;
    Ok(db.query_row(
        "SELECT (SELECT count(*) FROM revisions), (SELECT count(*) FROM history), (SELECT count(*) FROM transaction_steps), (SELECT count(*) FROM generation_requests), (SELECT count(*) FROM generation_preparations), (SELECT count(*) FROM generation_bundle_receipts)",
        [],
        |row| Ok([row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?]),
    )?)
}

#[test]
fn nested_fixed_retime_macro_compound_keeps_admitted_media_scope_and_history_atomic() -> Result {
    for final_frames in [8, 18] {
        let scratch = tempfile::tempdir()?;
        let package = scratch
            .path()
            .join(format!("nested-fixed-{final_frames}.deadpan"));
        let mut store = ProjectStore::create(&package, &fixed_repeat_initial()?)?;
        store.set_generation_context_resolver(Arc::new(Relevant));
        let chosen = target(&store.snapshot()?, 1)?;
        let shared = shared_target()?;
        accepted_for_duration(
            &mut store,
            &shared,
            FrameDuration::new(12)?,
            "nested-fixed-accepted",
            73,
        )?;
        let before = store.snapshot()?;
        let source_request = store
            .generation_request(&RequestId::new("nested-fixed-accepted-request")?)?
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
        let instance = presentation(&before, &chosen)?;
        let visibility = before.fixed_occurrence_hold_duration_visibility(
            before.root(),
            &NodeId::new("top")?,
            &instance,
        )?;
        assert_eq!(
            visibility.frames,
            FrameRange::new(ProjectFrame(48), ProjectFrame(54))?
        );

        // A saved nested macro invokes the actual one-play semantic duration
        // path twice. The first leaf clones the exact default stage; the next
        // leaf reuses that owned occurrence with an empty identity pool.
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
            scoped: Some(SemanticScopedContext {
                root: NodeId::new("top")?,
                target: chosen.clone(),
                presentation: instance.clone(),
            }),
            hold_duration: None,
            shared_hold_duration: None,
            mixed_hold_duration: None,
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
            RevisionId::new("nested-fixed-final")?,
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
                    nodes: 10,
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
            plan.context.scoped.as_ref().unwrap().root,
            context.scoped.as_ref().unwrap().root
        );
        assert_eq!(plan.context.cursor, visibility.frames.start());
        let request = plan.request.as_ref().unwrap();
        let Command::Compound { transaction } = &request.command else {
            panic!("nested saved macro compiles to a Compound")
        };
        assert_eq!(transaction.steps().len(), 2);
        let first = transaction.steps()[0].edit().unwrap().request(&before);
        let prepared = deadpan_core::prepare_occurrence_hold_duration(&before, &first)?;
        assert_ne!(prepared.target.node, chosen.node);
        assert_eq!(
            prepared.presentation,
            plan.context.scoped.as_ref().unwrap().presentation
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
        let final_prepared =
            deadpan_core::prepare_occurrence_hold_duration(&prepared.document, &second)?;
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
            deadpan_core::replay_compound::<deadpan_core::EditError>(&before, request, |_| Ok(()))?;
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
        assert_eq!(after.overrides().len(), before.overrides().len() + 2);
        assert_eq!(after.gap_overrides(), before.gap_overrides());
        assert_eq!(after.duration()?.frames(), 108);
        assert_eq!(prepared.isolation().steps().len(), 2);
        assert_eq!(prepared.isolation().steps()[0].nodes().len(), 6);
        assert_eq!(prepared.isolation().steps()[1].nodes().len(), 4);
        let outer = NodeId::new("outer")?;
        let top = NodeId::new("top")?;
        let NodeKind::Repeat { iterations, .. } = &after.nodes()[&outer].kind else {
            panic!("inner Repeat retained")
        };
        assert_eq!(
            after.overrides()[&outer].get(&iterations.at(0).unwrap()),
            Some(&NodeId::new("owned")?)
        );
        let cloned_inner = &prepared.isolation().steps()[0].nodes()[&outer];
        let cloned_owned = &prepared.isolation().steps()[0].nodes()[&NodeId::new("owned")?];
        assert_eq!(
            after.nodes()[cloned_owned],
            before.nodes()[&NodeId::new("owned")?]
        );
        let NodeKind::Repeat {
            iterations: top_order,
            ..
        } = &after.nodes()[&top].kind
        else {
            panic!("top Repeat retained")
        };
        assert_eq!(
            after.overrides()[&top].get(&top_order.at(1).unwrap()),
            Some(cloned_inner)
        );
        let cloned_stage = &prepared.isolation().steps()[1].nodes()
            [&prepared.isolation().steps()[0].nodes()[&NodeId::new("stage")?]];
        assert_eq!(
            after.overrides()[cloned_inner].get(&iterations.at(1).unwrap()),
            Some(cloned_stage),
            "only the middle/middle Play receives the isolated Stage"
        );
        for (node, original) in before.nodes() {
            assert_eq!(&after.nodes()[node], original, "retained template {node}");
        }
        let before_plan = deadpan_plan::RenderPlan::compile(&before)?;
        let after_plan = deadpan_plan::RenderPlan::compile(&after)?;
        for frame in (0..48).chain(60..108) {
            assert_eq!(
                after_plan.picture(ProjectFrame(frame))?.picture,
                before_plan.picture(ProjectFrame(frame))?.picture,
                "every unselected provider output at root frame {frame}"
            );
        }
        assert_ne!(
            after_plan.picture(ProjectFrame(52))?.picture,
            before_plan.picture(ProjectFrame(52))?.picture,
            "the selected stage changes its actual provider output"
        );
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
        let stale_needs =
            before.occurrence_hold_duration_requirements(&instance, stale_duration)?;
        let stale = CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: RevisionId::new("nested-fixed-stale-request")?,
            command: Command::EditOccurrence {
                instance: instance.clone(),
                edit: OccurrenceEdit::SetHoldDuration {
                    duration: stale_duration,
                },
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
        assert!(store.commit(&stale).is_err());
        assert_eq!(store.snapshot()?, before_refusal);
        assert_eq!(db_counts(&package)?, row_counts);

        // A second Compound whose first valid duration leaf is followed by a
        // missing target is also rejected before publishing partial history.
        let mapped_instance = prepared.presentation.clone();
        let late_duration = FrameDuration::new(9)?;
        let late_needs =
            after.occurrence_hold_duration_requirements(&mapped_instance, late_duration)?;
        assert_eq!((late_needs.nodes, late_needs.marks), (0, 0));
        let late_first = LeafEdit::new(
            RevisionId::new("nested-fixed-late-valid")?,
            Command::EditOccurrence {
                instance: mapped_instance,
                edit: OccurrenceEdit::SetHoldDuration {
                    duration: late_duration,
                },
                identities: OccurrenceIdentities {
                    nodes: Vec::new(),
                    marks: Vec::new(),
                },
            },
        )?;
        let late_second = LeafEdit::new(
            RevisionId::new("nested-fixed-late-missing")?,
            Command::Rename {
                node: NodeId::new("missing-target")?,
                label: "Must not commit".into(),
            },
        )?;
        let late = command(
            &store,
            "nested-fixed-late-refusal",
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
        assert!(store.commit_compound(&late, None).is_err());
        assert_eq!(store.snapshot()?, before_refusal);
        assert_eq!(store.registers()?, bank);
        assert_eq!(db_counts(&package)?, row_counts);

        drop(store);
        let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
        store.set_generation_context_resolver(Arc::new(Relevant));
        assert_eq!(store.snapshot()?, after);
        assert_eq!(store.registers()?, bank);
        store.undo(
            &store.head_revision()?,
            RevisionId::new("nested-fixed-undo")?,
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
            RevisionId::new("nested-fixed-redo")?,
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
    Ok(())
}
