//! Accepted artifacts use the existing admitted synthetic bundle fixture.
//! This verifies durable scope/lifecycle, not model generation or media decoding.
use super::*;
use deadpan_jobs::GenerationOptions;

fn gap_target(document: &ProjectDocument, gap: u32) -> Result<ScopedNodeTarget> {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&NodeId::new("inner")?].kind else {
        return Err("inner Repeat required".into());
    };
    Ok(ScopedNodeTarget {
        node: NodeId::new(format!("gap-{gap}"))?,
        repeats: vec![
            RepeatEditStep {
                repeat: NodeId::new("outer")?,
                branch: RepeatEditBranch::Default,
            },
            RepeatEditStep {
                repeat: NodeId::new("inner")?,
                branch: RepeatEditBranch::Play {
                    iteration: iterations.at(gap).ok_or("gap play absent")?,
                },
            },
        ],
    })
}

fn setup(store: &mut ProjectStore) -> Result {
    store.commit(&command(
        store,
        "body-two",
        Command::SetHoldDuration {
            node: NodeId::new("hold")?,
            duration: FrameDuration::new(2)?,
        },
    )?)?;
    let gap = HoldRecipe {
        duration: FrameDuration::new(2)?,
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
        picture_context: None,
    };
    for (child, owner, plays, recipe) in
        [("hold", "inner", 3, Some(gap)), ("inner", "outer", 2, None)]
    {
        store.commit(&command(
            store,
            &format!("wrap-{owner}"),
            Command::WrapRepeat {
                node: NodeId::new(child)?,
                id: NodeId::new(owner)?,
                plays,
                gap: recipe,
                anchor_policy: Default::default(),
            },
        )?)?;
    }
    for (index, frames) in [2, 4, 2].into_iter().enumerate() {
        let before = store.snapshot()?;
        let NodeKind::Repeat { iterations, .. } = &before.nodes()[&NodeId::new("inner")?].kind
        else {
            return Err("inner Repeat required".into());
        };
        let key = format!("gap-{index}");
        store.commit(&command(
            store,
            &format!("install-{key}"),
            Command::SetGapOverride {
                node: NodeId::new("inner")?,
                iteration: iterations.at(u32::try_from(index)?).unwrap(),
                subtree: Subtree {
                    root: NodeId::new(&key)?,
                    nodes: BTreeMap::from([(
                        NodeId::new(&key)?,
                        BeatNode::hold(
                            "Owned gap",
                            HoldRecipe {
                                duration: FrameDuration::new(frames)?,
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                                picture_context: None,
                            },
                        ),
                    )]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        )?)?;
        let target = gap_target(&store.snapshot()?, u32::try_from(index)?)?;
        accepted_for_duration(
            store,
            &target,
            FrameDuration::new(frames)?,
            &format!("accepted-{key}"),
            u64::try_from(index)? + 20,
        )?;
    }
    Ok(())
}

fn assert_content(document: &ProjectDocument, expected: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(expected)?;
    expected["revision_id"] = serde_json::json!(document.revision_id());
    assert_eq!(serde_json::to_value(document)?, expected);
    Ok(())
}

#[test]
fn owned_gap_compound_keeps_shortened_provider_and_maps_live_and_dormant_extensions_through_history()
-> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("owned-gap-duration.deadpan");
    let mut store = ProjectStore::create(&package, &document()?)?;
    store.set_generation_context_resolver(std::sync::Arc::new(Relevant));
    setup(&mut store)?;
    let before = store.snapshot()?;
    assert_eq!(before.duration()?.frames(), 24);
    let targets = [
        gap_target(&before, 2)?,
        gap_target(&before, 1)?,
        gap_target(&before, 0)?,
    ];
    let requests = [2, 1, 0].map(|index| {
        store
            .generation_request(&RequestId::new(format!("accepted-gap-{index}-request")).unwrap())
            .unwrap()
            .unwrap()
    });
    let accepted = targets
        .iter()
        .map(|target| {
            let NodeKind::Hold { recipe } = &before.nodes()[&target.node].kind else {
                panic!("gap Hold")
            };
            let HoldVideo::Generated { accepted } = &recipe.video else {
                panic!("admitted gap provider")
            };
            accepted.clone()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        accepted[0].artifact.sampling.output_frame_count().frames(),
        2
    );
    assert_eq!(
        accepted[1].artifact.sampling.output_frame_count().frames(),
        4
    );
    assert_eq!(
        accepted[2].artifact.sampling.output_frame_count().frames(),
        2
    );
    let mut staged = before.clone();
    let mut commands = Vec::new();
    for (index, target) in targets.iter().enumerate() {
        let leaf = scoped_duration_leaf(
            &staged,
            target.clone(),
            FrameDuration::new(3)?,
            &format!("gap-resize-{index}"),
        )?;
        let request = CommandRequest {
            project_id: staged.project_id().clone(),
            expected_revision: staged.revision_id().clone(),
            new_revision: RevisionId::new(format!("gap-stage-{index}"))?,
            command: leaf.clone(),
        };
        staged = deadpan_core::apply(&staged, &request)?
            .forward
            .apply_stored(&staged)?;
        if index == 0 {
            assert_eq!(
                staged.duration()?,
                before.duration()?,
                "final gap has no output"
            );
            assert_eq!(
                staged.audio_bindings(),
                before.audio_bindings(),
                "dormant resize retains clocks"
            );
        }
        commands.push(leaf);
    }
    let request = compound(&store, commands)?;
    let expected = store.preview(&request)?.forward.apply_stored(&before)?;
    let proof = deadpan_core::derive_scoped_isolation(&before, &request, &expected)?;
    let mapped = targets
        .iter()
        .map(|target| proof.map_forward(target))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let outcome = store.commit(&request)?;
    let after = store.snapshot()?;
    assert_eq!(after, expected);
    assert_eq!(
        after.duration()?,
        before.duration()?,
        "live +1/-1 cancel in both outer plays; final gap remains absent"
    );
    let NodeKind::Hold { recipe } = &after.nodes()[&mapped[1].node].kind else {
        panic!("shortened gap")
    };
    assert_eq!(recipe.duration, FrameDuration::new(3)?);
    assert_eq!(
        recipe.video,
        HoldVideo::Generated {
            accepted: accepted[1].clone()
        }
    );
    for (index, original) in requests.iter().enumerate() {
        let current = store.generation_request(&original.request_id)?.unwrap();
        assert_eq!(current.origin_target, original.origin_target);
        assert_eq!(current.binding, original.binding);
        assert_eq!(current.provider, original.provider);
        assert_eq!(current.constraints, original.constraints);
        assert_eq!(current.target, mapped[index]);
        assert_eq!(current.relevance, deadpan_jobs::Relevance::Stale);
    }
    assert_eq!(outcome.generation_preparations.len(), 2);
    let preparations = outcome
        .generation_preparations
        .iter()
        .map(|id| store.generation_preparation(id).map(|p| p.unwrap()))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for index in [0, 2] {
        let target = &mapped[index];
        let NodeKind::Hold { recipe } = &after.nodes()[&target.node].kind else {
            panic!("extended gap")
        };
        assert_eq!(recipe.duration, FrameDuration::new(3)?);
        assert_eq!(recipe.video, HoldVideo::Background);
        let preparation = preparations.iter().find(|p| &p.target == target).unwrap();
        assert_eq!(&preparation.origin_target, target);
        assert_eq!(preparation.duration, FrameDuration::new(3)?);
        assert_eq!(preparation.state, PreparationState::Queued);
        assert_eq!(preparation.origin_revision, request.new_revision);
        // These requests were captured at the final scoped gap addresses.
        // Their explicit controls are still available at extension birth.
        assert_eq!(
            preparation.origin,
            PreparationOrigin::AcceptedExtension {
                accepted: Box::new(accepted[index].artifact.clone()),
                controls: PreparationControls::Request {
                    request_id: requests[index].request_id.clone(),
                    options: GenerationOptions::from_constraints(&requests[index].constraints),
                },
            }
        );
    }
    // The configured default remains independent from all three owned recipes.
    let NodeKind::Repeat {
        gap: Some(default), ..
    } = &after.nodes()[&NodeId::new("inner")?].kind
    else {
        panic!("default gap")
    };
    assert_eq!(default.duration, FrameDuration::new(2)?);
    assert_eq!(default.video, HoldVideo::Background);
    store.validate_full()?;
    drop(store);
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    store.set_generation_context_resolver(std::sync::Arc::new(Relevant));
    assert_eq!(store.snapshot()?, after);
    for p in &preparations {
        assert_eq!(store.generation_preparation(&p.id)?, Some(p.clone()));
    }
    store.undo(&store.head_revision()?, RevisionId::new("owned-gap-undo")?)?;
    assert_content(&store.snapshot()?, &before)?;
    for p in &preparations {
        assert_eq!(
            store.generation_preparation(&p.id)?.unwrap().state,
            PreparationState::Cancelled
        );
    }
    let redo = store.redo(&store.head_revision()?, RevisionId::new("owned-gap-redo")?)?;
    assert_content(&store.snapshot()?, &after)?;
    assert_eq!(redo.generation_preparations.len(), 2);
    for id in &redo.generation_preparations {
        assert!(!outcome.generation_preparations.contains(id));
        let p = store.generation_preparation(id)?.unwrap();
        let old = preparations
            .iter()
            .find(|old| old.target == p.target)
            .unwrap();
        assert_eq!(p.origin_target, old.origin_target);
        assert_eq!(p.origin, old.origin);
        assert_eq!(p.duration, old.duration);
        assert_eq!(p.state, PreparationState::Queued);
    }
    store.validate_full()?;
    drop(store);
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_content(&store.snapshot()?, &after)?;
    store.validate_full()?;
    Ok(())
}
