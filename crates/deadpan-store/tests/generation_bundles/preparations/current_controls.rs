//! Recorded request controls must survive the pure historical birth replay.
use super::*;
use deadpan_core::{ExtensionDirection, ScopedNodeTarget};
use deadpan_jobs::{
    ExtensionCapability, ExtensionGenerationPlan, GenerationOptions, GenerationPlan,
    HoldInstructions,
};
use deadpan_store::generation_inputs::{
    ExtensionCapturePolicy, GenerationCaptureSpec, GenerationInputs,
};
use deadpan_store::generation_intents::IntentInputBinding;
use deadpan_store::generation_pictures::GenerationPictureIdentity;

fn accepted_with_context(package: &Path) -> Result<ProjectStore> {
    // Black is an authored picture, so these contexts use the production
    // metadata provider without inventing an Original qualification or index.
    let mut initial = document()?;
    for (name, index) in [("left-context", 0), ("right-context", 2)] {
        let node = NodeId::new(name)?;
        let request = CommandRequest {
            project_id: initial.project_id().clone(),
            expected_revision: initial.revision_id().clone(),
            new_revision: RevisionId::new(format!("setup-{name}"))?,
            command: Command::Insert {
                parent: initial.root().clone(),
                index,
                subtree: Subtree {
                    root: node.clone(),
                    nodes: BTreeMap::from([(
                        node,
                        BeatNode::hold(
                            name,
                            HoldRecipe {
                                duration: FrameDuration::new(24)?,
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
        };
        initial = deadpan_core::apply(&initial, &request)?
            .forward
            .apply(&initial)?;
    }
    let mut store = ProjectStore::create(package, &initial)?;
    let acceptance = ready_for_acceptance(&mut store)?;
    store.accept_generation_bundle(
        &acceptance,
        &unchanged_relevance(&store, &acceptance.new_revision)?,
        media_limits(),
    )?;
    store.set_generation_context_resolver(std::sync::Arc::new(Relevant));
    Ok(store)
}

fn request_extension(
    store: &mut ProjectStore,
    direction: ExtensionDirection,
) -> Result<StoredGenerationRequest> {
    let mut current = constraints();
    current.conditioning = match direction {
        ExtensionDirection::FromLeft => ConditioningMode::ExtendFromLeft,
        ExtensionDirection::FromRight => ConditioningMode::ExtendFromRight,
    };
    current.motion = MotionAmount::Subtle;
    current.instructions = Some(HoldInstructions::new("Keep the empty scene still.")?);
    // These exact settings differ from Automatic's 24 Hz / 9 picture defaults,
    // so reconstructing an operation from options alone cannot pass this test.
    let capability = ExtensionCapability::new(
        rate(),
        17,
        FrameCountFormula::new(8, 0, 8, 32)?,
        DimensionLimits::new(AxisLimits::new(512, 512, 1)?, AxisLimits::new(320, 320, 1)?),
        FrameDuration::new(100)?,
    )?;
    let plan = GenerationPlan::Extension(ExtensionGenerationPlan::new(
        direction,
        FrameDuration::new(12)?,
        rate(),
        &capability,
        NativeDimensions::new(512, 320)?,
    )?);
    let target = ScopedNodeTarget {
        node: NodeId::new("hold")?,
        repeats: Vec::new(),
    };
    Ok(store.record_scoped_generation_request(
        GenerationRequestInput {
            request_id: RequestId::new("new-current-extension")?,
            expected_revision: store.head_revision()?,
            hold_id: target.node.clone(),
            context_sha256: sha('e'),
            constraints: current,
            provider: provider(19),
        },
        target,
        plan,
    )?)
}

#[test]
fn accepted_duration_birth_replays_the_new_current_extension_controls() -> Result {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let scratch = tempfile::tempdir()?;
        let package = scratch.path().join("current-controls.deadpan");
        let mut store = accepted_with_context(&package)?;
        let accepted = recipe(&store)?;
        let HoldVideo::Generated {
            accepted: generated,
        } = &accepted.video
        else {
            panic!("accepted fixture")
        };
        let current = request_extension(&mut store, direction)?;
        assert_eq!(current.relevance, deadpan_jobs::Relevance::Current);
        assert_eq!(store.current_generation_requests()?, vec![current.clone()]);
        let options = GenerationOptions::from_constraints(&current.constraints);
        let capture = GenerationCaptureSpec::Extension {
            direction,
            native_rate: rate(),
            context_frames: 17,
            policy: ExtensionCapturePolicy::TemporalContextV1,
        };
        assert_eq!(
            GenerationCaptureSpec::from_plan(current.plan.as_ref().unwrap()),
            capture
        );

        let outcome = resize(&mut store, "lengthen-current-operation", 18)?;
        assert_eq!(outcome.generation_preparations.len(), 1);
        let prepared = store
            .generation_preparation(&outcome.generation_preparations[0])?
            .unwrap();
        assert_eq!(prepared.state, PreparationState::Queued);
        assert_eq!(prepared.target, current.target);
        assert_eq!(
            prepared.origin,
            PreparationOrigin::AcceptedExtension {
                accepted: Box::new(generated.artifact.clone()),
                controls: PreparationControls::Request {
                    request_id: current.request_id.clone(),
                    options
                },
            }
        );
        assert_eq!(prepared.intent.capture, Some(capture));
        let IntentInputBinding::Measured { binding } = &prepared.intent.input_binding else {
            panic!("authored black context must remain available")
        };
        assert_eq!(binding.duration, FrameDuration::new(18)?);
        assert_eq!(binding.frame_rate, rate());
        assert_eq!(binding.region, None);
        let GenerationInputs::Extension {
            capture: bound_capture,
            samples,
            opposite,
            support,
            terminal,
        } = &binding.inputs
        else {
            panic!("the new request's extension operation must survive replay")
        };
        assert_eq!(*bound_capture, capture);
        assert_eq!(samples.len(), 17);
        assert!(
            samples
                .iter()
                .all(|sample| sample.picture == GenerationPictureIdentity::AuthoredBlack)
        );
        assert_eq!(
            opposite.as_ref().unwrap().picture,
            GenerationPictureIdentity::AuthoredBlack
        );
        assert!(!support.is_empty());
        assert!(support.iter().all(
            |span| span.first == GenerationPictureIdentity::AuthoredBlack
                && span.last == GenerationPictureIdentity::AuthoredBlack
        ));
        assert_eq!(terminal.picture, GenerationPictureIdentity::AuthoredBlack);
        let fallback = recipe(&store)?;
        assert_eq!(fallback.duration, FrameDuration::new(18)?);
        assert_eq!(fallback.video, HoldVideo::Background);
        assert_eq!(fallback.audio, accepted.audio);
        store.validate_full()?;
        drop(store);

        let reader = ProjectStore::open(&package, AccessMode::ReadOnly)?;
        reader.validate_full()?;
        assert_eq!(
            reader.generation_preparation(&prepared.id)?,
            Some(prepared.clone())
        );
        drop(reader);
        let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
        store.set_generation_context_resolver(std::sync::Arc::new(Relevant));
        store.undo(&store.head_revision()?, RevisionId::new("undo-lengthen")?)?;
        assert_eq!(recipe(&store)?, accepted);
        let redo = store.redo(&store.head_revision()?, RevisionId::new("redo-lengthen")?)?;
        let fresh = store
            .generation_preparation(&redo.generation_preparations[0])?
            .unwrap();
        assert_ne!(fresh.id, prepared.id);
        assert_eq!(fresh.origin, prepared.origin);
        assert_eq!(fresh.intent.capture, prepared.intent.capture);
        assert_eq!(fresh.intent.input_binding, prepared.intent.input_binding);
        store.validate_full()?;
    }
    Ok(())
}
