//! Accepted media remains qualified through actual nested duration isolation.

use super::*;
use deadpan_core::{InstancePath, OccurrenceEdit, OccurrenceIdentities, RepeatInstance};

#[test]
fn nested_occurrence_duration_maps_admitted_generated_hold_shortening_and_extension_preparation_through_compound_history()
-> Result {
    for frames in [8, 18] {
        let scratch = tempfile::tempdir()?;
        let package = scratch.path().join("nested-duration.deadpan");
        // Reuse production bundle admission and acceptance, rather than
        // constructing a GeneratedArtifact or acceptance receipt here.
        let mut store = accepted(&package)?;
        let accepted_recipe = recipe(&store)?;
        let HoldVideo::Generated {
            accepted: original_provider,
        } = &accepted_recipe.video
        else {
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
        let instance = InstancePath {
            node: NodeId::new("hold")?,
            repeats: ["outer", "inner"]
                .into_iter()
                .map(|owner| {
                    let NodeKind::Repeat { iterations, .. } =
                        &before.nodes()[&NodeId::new(owner).unwrap()].kind
                    else {
                        panic!()
                    };
                    RepeatInstance {
                        node: NodeId::new(owner).unwrap(),
                        iteration: iterations.at(1).unwrap(),
                    }
                })
                .collect(),
        };
        let duration = FrameDuration::new(frames)?;
        let needs = before.occurrence_hold_duration_requirements(&instance, duration)?;
        let leaf = Command::EditOccurrence {
            instance: instance.clone(),
            edit: OccurrenceEdit::SetHoldDuration { duration },
            identities: OccurrenceIdentities {
                nodes: (0..needs.nodes)
                    .map(|index| NodeId::new(format!("resized-{index}")).unwrap())
                    .collect(),
                marks: (0..needs.marks)
                    .map(|index| deadpan_core::MarkId::new(format!("resized-{index}")).unwrap())
                    .collect(),
            },
        };
        let request = compound(&store, vec![leaf])?;
        let Command::Compound { transaction } = &request.command else {
            panic!()
        };
        let prepared = deadpan_core::prepare_occurrence_hold_duration(
            &before,
            &transaction.steps()[0].edit().unwrap().request(&before),
        )?;
        assert_ne!(prepared.target.node, instance.node);
        assert_ne!(prepared.target.repeats[1].repeat, NodeId::new("inner")?);
        assert_eq!(prepared.boundary.frame.0, 36);
        let outcome = store.commit(&request)?;
        let after = store.snapshot()?;
        assert_eq!(
            after.nodes()[&NodeId::new("hold")?],
            before.nodes()[&NodeId::new("hold")?]
        );
        assert_eq!(
            after.duration()?.frames(),
            before.duration()?.frames() + frames - 12
        );
        let NodeKind::Hold { recipe: resized } = &after.nodes()[&prepared.target.node].kind else {
            panic!()
        };
        assert_eq!(resized.duration, duration);
        assert_eq!(resized.audio, accepted_recipe.audio);
        assert_eq!(resized.picture_context, accepted_recipe.picture_context);
        if frames == 8 {
            assert!(outcome.generation_preparations.is_empty());
            let HoldVideo::Generated { accepted: retained } = &resized.video else {
                panic!("qualified accepted shortening")
            };
            assert_eq!(retained, original_provider);
        } else {
            assert_eq!(outcome.generation_preparations.len(), 1);
            assert_eq!(resized.video, HoldVideo::Background);
            let pending = store
                .generation_preparation(&outcome.generation_preparations[0])?
                .unwrap();
            assert_eq!(pending.target, prepared.target);
            assert_eq!(pending.origin_target, prepared.target);
            assert_eq!(pending.duration, duration);
            assert_eq!(pending.state, PreparationState::Queued);
            let PreparationOrigin::AcceptedExtension {
                accepted,
                controls: PreparationControls::AcceptedArtifact,
            } = &pending.origin
            else {
                panic!("immutable admitted controls")
            };
            assert_eq!(accepted.as_ref(), &original_provider.artifact);
        }
        store.validate_full()?;
        drop(store);
        let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
        store.set_generation_context_resolver(std::sync::Arc::new(Relevant));
        assert_eq!(store.snapshot()?, after);
        if frames == 18 {
            assert_eq!(
                store
                    .generation_preparation(&outcome.generation_preparations[0])?
                    .unwrap()
                    .target,
                prepared.target
            );
        }
        store.undo(&store.head_revision()?, RevisionId::new("undo-duration")?)?;
        let undone = store.snapshot()?;
        let mut expected = serde_json::to_value(&before)?;
        expected["revision_id"] = serde_json::json!(undone.revision_id());
        assert_eq!(serde_json::to_value(&undone)?, expected);
        if frames == 18 {
            assert_eq!(
                store
                    .generation_preparation(&outcome.generation_preparations[0])?
                    .unwrap()
                    .state,
                PreparationState::Cancelled
            );
        }
        let redo = store.redo(&store.head_revision()?, RevisionId::new("redo-duration")?)?;
        let redone = store.snapshot()?;
        let mut expected = serde_json::to_value(&after)?;
        expected["revision_id"] = serde_json::json!(redone.revision_id());
        assert_eq!(serde_json::to_value(&redone)?, expected);
        if frames == 18 {
            assert_eq!(redo.generation_preparations.len(), 1);
            assert_ne!(
                redo.generation_preparations[0],
                outcome.generation_preparations[0]
            );
            let pending = store
                .generation_preparation(&redo.generation_preparations[0])?
                .unwrap();
            assert_eq!(pending.target, prepared.target);
            assert_eq!(pending.duration, duration);
            assert_eq!(pending.state, PreparationState::Queued);
        } else {
            assert!(redo.generation_preparations.is_empty());
        }
        store.validate_full()?;
    }
    Ok(())
}
