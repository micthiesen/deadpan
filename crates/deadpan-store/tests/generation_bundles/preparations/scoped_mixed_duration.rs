//! Mixed Default/Play duration isolation retains exact accepted-generation scope.

use super::*;
#[path = "scoped_mixed_duration/nested_fixed_retime.rs"]
mod nested_fixed_retime;
#[path = "scoped_mixed_duration/owned_gap.rs"]
mod owned_gap;
#[path = "scoped_mixed_duration/shared_fixed.rs"]
mod shared_fixed;
#[path = "scoped_mixed_duration/this_play_fixed_retime.rs"]
mod this_play_fixed_retime;
use deadpan_core::{OccurrenceIdentities, RepeatEditBranch, RepeatEditStep, ScopedNodeTarget};

fn accepted_for_duration(
    store: &mut ProjectStore,
    target: &ScopedNodeTarget,
    duration: FrameDuration,
    key: &str,
    seed: u64,
) -> Result {
    let video = VideoSpec::new(duration, rate(), 512, 320)?;
    let constraints = HoldConstraints {
        video: video.clone(),
        conditioning: ConditioningMode::Bridge,
        motion: MotionAmount::Still,
        instructions: None,
        region_target: None,
    };
    let plan = BridgeGenerationPlan::new(
        duration,
        rate(),
        &BridgeCapability::new(
            true,
            FrameRate::new(24, 1)?,
            FrameCountFormula::new(1, 0, 2, 97)?,
            DimensionLimits::new(AxisLimits::new(512, 512, 1)?, AxisLimits::new(320, 320, 1)?),
        ),
        NativeDimensions::new(512, 320)?,
    )?;
    let input = GenerationRequestInput {
        request_id: RequestId::new(format!("{key}-request"))?,
        expected_revision: store.head_revision()?,
        hold_id: target.node.clone(),
        context_sha256: sha('b'),
        constraints,
        provider: provider(seed),
    };
    let request = if target.repeats.is_empty() {
        store.record_bridge_generation_request(input, plan.clone())?
    } else {
        store.record_scoped_bridge_generation_request(input, target.clone(), plan.clone())?
    };
    publish_bundle(store)?;
    publish_inputs(store)?;
    let identity = begin(store, &request, &format!("{key}-attempt"))?;
    let candidate = native_candidate_for(&request, 1);
    complete_bridge(store, &identity, &candidate)?;
    let receipt = BundleValidationReceipt::new(
        &candidate,
        object(NATIVE_BYTES),
        object(SAMPLED_BYTES),
        object(PROVENANCE_BYTES),
        video,
        plan,
        ValidatorIdentity::new("deadpan-media", "bridge-1")?,
    )?
    .with_admission(admission(request.binding.context_sha256.clone()))?;
    store.record_generation_bundle_ready(&identity, &candidate, receipt.clone(), media_limits())?;
    let acceptance = GenerationAcceptance {
        expected_revision: store.head_revision()?,
        new_revision: RevisionId::new(format!("{key}-accepted"))?,
        identity,
        expected_receipt: receipt,
        native_asset: AssetId::new(format!("{key}-native"))?,
        sampled_asset: AssetId::new(format!("{key}-sampled"))?,
    };
    store.accept_generation_bundle(
        &acceptance,
        &unchanged_relevance(store, &acceptance.new_revision)?,
        media_limits(),
    )?;
    Ok(())
}

fn scoped_duration_leaf(
    document: &ProjectDocument,
    target: ScopedNodeTarget,
    duration: FrameDuration,
    prefix: &str,
) -> Result<Command> {
    let required = document.scoped_hold_duration_requirements(&target, duration)?;
    Ok(Command::SetScopedHoldDuration {
        target,
        duration,
        identities: OccurrenceIdentities {
            nodes: (0..required.nodes)
                .map(|index| NodeId::new(format!("{prefix}-node-{index}")))
                .collect::<std::result::Result<_, _>>()?,
            marks: (0..required.marks)
                .map(|index| deadpan_core::MarkId::new(format!("{prefix}-mark-{index}")))
                .collect::<std::result::Result<_, _>>()?,
        },
    })
}

fn outer_play_inner_default(
    document: &ProjectDocument,
    outer_play: u32,
) -> Result<ScopedNodeTarget> {
    let outer = NodeId::new("outer")?;
    let inner = NodeId::new("inner")?;
    let NodeKind::Repeat {
        iterations: outer_order,
        ..
    } = &document.nodes()[&outer].kind
    else {
        return Err("expected outer Repeat".into());
    };
    if outer_play >= outer_order.len() {
        return Err("outer Repeat play is absent".into());
    }
    Ok(ScopedNodeTarget {
        node: NodeId::new("hold")?,
        repeats: vec![
            RepeatEditStep {
                repeat: outer,
                branch: RepeatEditBranch::Play {
                    iteration: outer_order.at(outer_play).unwrap(),
                },
            },
            RepeatEditStep {
                repeat: inner,
                branch: RepeatEditBranch::Default,
            },
        ],
    })
}

fn target(document: &ProjectDocument, outer_play: bool) -> Result<ScopedNodeTarget> {
    let branch = |node: &str, play: bool| -> Result<RepeatEditStep> {
        let repeat = NodeId::new(node)?;
        let NodeKind::Repeat { iterations, .. } = &document.nodes()[&repeat].kind else {
            return Err("expected Repeat".into());
        };
        Ok(RepeatEditStep {
            repeat,
            branch: if play {
                RepeatEditBranch::Play {
                    iteration: iterations.at(1).unwrap(),
                }
            } else {
                RepeatEditBranch::Default
            },
        })
    };
    Ok(ScopedNodeTarget {
        node: NodeId::new("hold")?,
        repeats: vec![branch("outer", outer_play)?, branch("inner", !outer_play)?],
    })
}

#[test]
fn mixed_batch_zero_net_maps_one_extension_and_one_owned_provider_through_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("mixed-batch-zero-net.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    store.commit(&command(
        &store,
        "short-base",
        Command::SetHoldDuration {
            node: NodeId::new("hold")?,
            duration: FrameDuration::new(2)?,
        },
    )?)?;
    accepted_for_duration(
        &mut store,
        &ScopedNodeTarget {
            node: NodeId::new("hold")?,
            repeats: Vec::new(),
        },
        FrameDuration::new(2)?,
        "base-two",
        2,
    )?;
    store.set_generation_context_resolver(std::sync::Arc::new(Relevant));

    for (child, owner, plays) in [("hold", "inner", 2), ("inner", "outer", 2)] {
        store.commit(&command(
            &store,
            &format!("wrap-{owner}"),
            Command::WrapRepeat {
                node: NodeId::new(child)?,
                id: NodeId::new(owner)?,
                plays,
                gap: None,
                anchor_policy: Default::default(),
            },
        )?)?;
    }

    // Give outer Play 0 its own accepted four-frame Hold; Play 1 still points
    // at the shared two-frame accepted Hold. IterationOrder.at preserves the
    // actual inserted order instead of assuming identifier sort order.
    let before_ownership = store.snapshot()?;
    let play_zero = outer_play_inner_default(&before_ownership, 0)?;
    let setup = command(
        &store,
        "own-four-frame-family",
        scoped_duration_leaf(
            &before_ownership,
            play_zero.clone(),
            FrameDuration::new(4)?,
            "own-four",
        )?,
    )?;
    let setup_after = store
        .preview(&setup)?
        .forward
        .apply_stored(&before_ownership)?;
    let setup_proof =
        deadpan_core::derive_scoped_isolation(&before_ownership, &setup, &setup_after)?;
    let owned_target = setup_proof.map_retained_forward(&play_zero);
    store.commit(&setup)?;
    accepted_for_duration(
        &mut store,
        &owned_target,
        FrameDuration::new(4)?,
        "owned-four",
        4,
    )?;

    let before = store.snapshot()?;
    let target_four = owned_target.clone();
    let target_two = outer_play_inner_default(&before, 1)?;
    assert_ne!(target_four.node, target_two.node);
    let NodeKind::Hold { recipe: four } = &before.nodes()[&target_four.node].kind else {
        panic!("owned target is a Hold")
    };
    let NodeKind::Hold { recipe: two } = &before.nodes()[&target_two.node].kind else {
        panic!("shared target is a Hold")
    };
    assert_eq!(four.duration, FrameDuration::new(4)?);
    assert_eq!(two.duration, FrameDuration::new(2)?);
    let HoldVideo::Generated {
        accepted: four_provider,
    } = &four.video
    else {
        panic!("four-frame family retains accepted media")
    };
    let HoldVideo::Generated {
        accepted: two_provider,
    } = &two.video
    else {
        panic!("two-frame family retains accepted media")
    };
    assert_eq!(
        four_provider
            .artifact
            .sampling
            .output_frame_count()
            .frames(),
        4
    );
    assert_eq!(
        two_provider.artifact.sampling.output_frame_count().frames(),
        2
    );
    assert_ne!(four_provider.artifact, two_provider.artifact);

    let first_leaf = scoped_duration_leaf(
        &before,
        target_four.clone(),
        FrameDuration::new(3)?,
        "batch-four-to-three",
    )?;
    let staged_request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: RevisionId::new("batch-stage-first")?,
        command: first_leaf.clone(),
    };
    let staged_after = deadpan_core::apply(&before, &staged_request)?
        .forward
        .apply_stored(&before)?;
    let staged_proof =
        deadpan_core::derive_scoped_isolation(&before, &staged_request, &staged_after)?;
    let mapped_two = staged_proof.map_retained_forward(&target_two);
    let second_leaf = scoped_duration_leaf(
        &staged_after,
        mapped_two.clone(),
        FrameDuration::new(3)?,
        "batch-two-to-three",
    )?;
    let request = compound(&store, vec![first_leaf, second_leaf])?;
    let expected = store.preview(&request)?.forward.apply_stored(&before)?;
    let proof = deadpan_core::derive_scoped_isolation(&before, &request, &expected)?;
    let mapped_four = proof.map_retained_forward(&target_four);
    let mapped_two = proof.map_retained_forward(&target_two);
    let outcome = store.commit(&request)?;
    let after = store.snapshot()?;
    assert_eq!(outcome.revision_id, request.new_revision);
    assert_eq!(
        after.duration()?,
        before.duration()?,
        "+2 and -2 frames cancel"
    );
    let mut before_content = serde_json::to_value(&before)?;
    let mut after_content = serde_json::to_value(&after)?;
    before_content["revision_id"] = serde_json::json!("same-revision");
    after_content["revision_id"] = serde_json::json!("same-revision");
    assert_ne!(
        after_content, before_content,
        "zero net duration still changes both Hold recipes"
    );
    assert_eq!(outcome.generation_preparations.len(), 1);

    let NodeKind::Hold { recipe: retained } = &after.nodes()[&mapped_four.node].kind else {
        panic!("mapped four-frame target is a Hold")
    };
    assert_eq!(retained.duration, FrameDuration::new(3)?);
    let HoldVideo::Generated {
        accepted: retained_provider,
    } = &retained.video
    else {
        panic!("shortening within support keeps accepted media")
    };
    assert_eq!(retained_provider, four_provider);

    let NodeKind::Hold { recipe: extended } = &after.nodes()[&mapped_two.node].kind else {
        panic!("mapped two-frame target is a Hold")
    };
    assert_eq!(extended.duration, FrameDuration::new(3)?);
    assert_eq!(extended.video, HoldVideo::Background);
    let preparation_id = &outcome.generation_preparations[0];
    let preparation = store.generation_preparation(preparation_id)?.unwrap();
    assert_eq!(preparation.target, mapped_two);
    assert_eq!(preparation.origin_target, mapped_two);
    assert_eq!(preparation.duration, FrameDuration::new(3)?);
    assert_eq!(preparation.state, PreparationState::Queued);
    let PreparationOrigin::AcceptedExtension {
        accepted,
        controls: PreparationControls::AcceptedArtifact,
    } = &preparation.origin
    else {
        panic!("extension retains the exact accepted artifact controls")
    };
    assert_eq!(accepted.as_ref(), &two_provider.artifact);
    assert_eq!(preparation.origin_revision, request.new_revision);
    assert_eq!(
        after.nodes()[&NodeId::new("hold")?],
        before.nodes()[&NodeId::new("hold")?],
        "the unselected shared Default remains the two-frame accepted Hold"
    );
    store.validate_full()?;
    drop(store);

    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    store.set_generation_context_resolver(std::sync::Arc::new(Relevant));
    assert_eq!(store.snapshot()?, after);
    assert_eq!(
        store.generation_preparation(preparation_id)?,
        Some(preparation.clone())
    );
    store.undo(&store.head_revision()?, RevisionId::new("batch-undo")?)?;
    let undone = store.snapshot()?;
    let mut expected_undo = serde_json::to_value(&before)?;
    expected_undo["revision_id"] = serde_json::json!(undone.revision_id());
    assert_eq!(serde_json::to_value(&undone)?, expected_undo);
    assert_eq!(
        store.generation_preparation(preparation_id)?.unwrap().state,
        PreparationState::Cancelled
    );
    let redo = store.redo(&store.head_revision()?, RevisionId::new("batch-redo")?)?;
    let redone = store.snapshot()?;
    let mut expected_redo = serde_json::to_value(&after)?;
    expected_redo["revision_id"] = serde_json::json!(redone.revision_id());
    assert_eq!(serde_json::to_value(&redone)?, expected_redo);
    assert_eq!(redo.generation_preparations.len(), 1);
    assert_ne!(redo.generation_preparations[0], *preparation_id);
    let replayed = store
        .generation_preparation(&redo.generation_preparations[0])?
        .unwrap();
    assert_eq!(replayed.target, mapped_two);
    assert_eq!(replayed.origin_target, mapped_two);
    assert_eq!(replayed.duration, FrameDuration::new(3)?);
    assert_eq!(replayed.origin, preparation.origin);
    assert_eq!(replayed.state, PreparationState::Queued);
    store.validate_full()?;
    Ok(())
}

#[test]
fn mixed_default_play_generated_duration_maps_birth_and_exact_history() -> Result {
    for outer_play in [true, false] {
        for compound_leaf in [false, true] {
            for frames in [8, 16] {
                let suffix = if outer_play {
                    "outer-play"
                } else {
                    "inner-play"
                };
                let command_kind = if compound_leaf { "compound" } else { "raw" };
                let scratch = tempfile::tempdir()?;
                let package = scratch
                    .path()
                    .join(format!("mixed-{suffix}-{command_kind}-{frames}.deadpan"));
                let mut store = accepted(&package)?;
                let accepted_recipe = recipe(&store)?;
                let HoldVideo::Generated { accepted: original } = &accepted_recipe.video else {
                    panic!("admitted provider")
                };
                for (child, owner, plays) in [("hold", "inner", 2), ("inner", "outer", 3)] {
                    store.commit(&command(
                        &store,
                        &format!("wrap-{owner}"),
                        Command::WrapRepeat {
                            node: NodeId::new(child)?,
                            id: NodeId::new(owner)?,
                            plays,
                            gap: None,
                            anchor_policy: Default::default(),
                        },
                    )?)?;
                }
                let before = store.snapshot()?;
                let scoped = target(&before, outer_play)?;
                let duration = FrameDuration::new(frames)?;
                let required = before.scoped_hold_duration_requirements(&scoped, duration)?;
                let leaf = Command::SetScopedHoldDuration {
                    target: scoped.clone(),
                    duration,
                    identities: OccurrenceIdentities {
                        nodes: (0..required.nodes)
                            .map(|index| NodeId::new(format!("mixed-{suffix}-{frames}-{index}")))
                            .collect::<std::result::Result<_, _>>()?,
                        marks: (0..required.marks)
                            .map(|index| {
                                deadpan_core::MarkId::new(format!(
                                    "mixed-{suffix}-{frames}-mark-{index}"
                                ))
                            })
                            .collect::<std::result::Result<_, _>>()?,
                    },
                };
                let request = if compound_leaf {
                    compound(&store, vec![leaf])?
                } else {
                    command(&store, &format!("mixed-duration-{suffix}-{frames}"), leaf)?
                };
                if compound_leaf {
                    let Command::Compound { transaction } = &request.command else {
                        panic!("resolved compound request")
                    };
                    let leaf_request = transaction.steps()[0].edit().unwrap().request(&before);
                    assert_ne!(leaf_request.new_revision, request.new_revision);
                    assert!(matches!(
                        leaf_request.command,
                        Command::SetScopedHoldDuration { .. }
                    ));
                }
                let expected = store.preview(&request)?.forward.apply_stored(&before)?;
                let proof = deadpan_core::derive_scoped_isolation(&before, &request, &expected)?;
                let mapped_target = proof.map_retained_forward(&scoped);
                let outcome = store.commit(&request)?;
                let after = store.snapshot()?;
                assert_eq!(
                    after.duration()?.frames(),
                    before.duration()?.frames() + (frames - 12) * if outer_play { 2 } else { 3 }
                );
                assert_eq!(
                    after.nodes()[&NodeId::new("hold")?],
                    before.nodes()[&NodeId::new("hold")?]
                );
                let NodeKind::Hold { recipe: resized } = &after.nodes()[&mapped_target.node].kind
                else {
                    panic!("mapped mixed Hold")
                };
                assert_eq!(resized.duration, duration);
                assert_eq!(resized.audio, accepted_recipe.audio);
                assert_eq!(resized.picture_context, accepted_recipe.picture_context);
                if frames <= original.artifact.sampling.output_frame_count().frames() {
                    assert!(outcome.generation_preparations.is_empty());
                    assert_eq!(resized.video, accepted_recipe.video);
                } else {
                    assert_eq!(outcome.generation_preparations.len(), 1);
                    assert_eq!(resized.video, HoldVideo::Background);
                    let pending = store
                        .generation_preparation(&outcome.generation_preparations[0])?
                        .unwrap();
                    assert_eq!(pending.target, mapped_target);
                    assert_eq!(pending.origin_target, mapped_target);
                    assert_eq!(pending.duration, duration);
                    assert_eq!(pending.state, PreparationState::Queued);
                    let PreparationOrigin::AcceptedExtension {
                        accepted,
                        controls: PreparationControls::AcceptedArtifact,
                    } = &pending.origin
                    else {
                        panic!("preparation retains admitted controls")
                    };
                    assert_eq!(accepted.as_ref(), &original.artifact);
                }
                store.validate_full()?;
                drop(store);

                let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
                store.set_generation_context_resolver(std::sync::Arc::new(Relevant));
                assert_eq!(store.snapshot()?, after);
                store.undo(&store.head_revision()?, RevisionId::new("mixed-undo")?)?;
                let undone = store.snapshot()?;
                let mut expected_undo = serde_json::to_value(&before)?;
                expected_undo["revision_id"] = serde_json::json!(undone.revision_id());
                assert_eq!(serde_json::to_value(&undone)?, expected_undo);
                if frames > original.artifact.sampling.output_frame_count().frames() {
                    assert_eq!(
                        store
                            .generation_preparation(&outcome.generation_preparations[0])?
                            .unwrap()
                            .state,
                        PreparationState::Cancelled
                    );
                }
                let redo = store.redo(&store.head_revision()?, RevisionId::new("mixed-redo")?)?;
                let redone = store.snapshot()?;
                let mut expected_redo = serde_json::to_value(&after)?;
                expected_redo["revision_id"] = serde_json::json!(redone.revision_id());
                assert_eq!(serde_json::to_value(&redone)?, expected_redo);
                if frames > original.artifact.sampling.output_frame_count().frames() {
                    assert_eq!(redo.generation_preparations.len(), 1);
                    let replayed = store
                        .generation_preparation(&redo.generation_preparations[0])?
                        .unwrap();
                    assert_eq!(replayed.target, mapped_target);
                    assert_eq!(replayed.origin_target, mapped_target);
                    assert_eq!(replayed.duration, duration);
                    assert_eq!(replayed.state, PreparationState::Queued);
                } else {
                    assert!(redo.generation_preparations.is_empty());
                }
                store.validate_full()?;
            }
        }
    }
    Ok(())
}
