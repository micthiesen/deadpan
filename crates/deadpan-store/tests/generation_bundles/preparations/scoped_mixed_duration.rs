//! Mixed Default/Play duration isolation retains exact accepted-generation scope.

use super::*;
use deadpan_core::{OccurrenceIdentities, RepeatEditBranch, RepeatEditStep, ScopedNodeTarget};

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
