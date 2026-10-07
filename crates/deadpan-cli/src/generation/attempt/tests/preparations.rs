use super::*;
use crate::generation::{acceptance, preparations};
use crate::generation_context::BoundaryContextResolver;
use crate::live_project::{self, ShortOperation};
use deadpan_store::generation_preparations::{
    PreparationControls, PreparationOrigin, PreparationState,
};

fn edit(store: &mut ProjectStore, revision: &str, command: Command) -> serde_json::Value {
    let document = store.snapshot().unwrap();
    live_project::execute_short(
        store,
        document.project_id(),
        &ShortOperation::Edit {
            request: Box::new(CommandRequest {
                project_id: document.project_id().clone(),
                expected_revision: document.revision_id().clone(),
                new_revision: RevisionId::new(revision).unwrap(),
                command,
            }),
            dry_run: false,
        },
    )
    .unwrap()
    .output
}

#[test]
fn replacement_keeps_controls_rejects_stale_claims_and_needs_explicit_acceptance() {
    let Some(worker) = synthetic_tools() else {
        eprintln!("skipped: needs ffmpeg and built media/tracking workers");
        return;
    };
    let directory = tempfile::tempdir().unwrap();
    let package = directory.path().join("test.deadpan");
    let mut store = project(directory.path());
    store.set_generation_context_resolver(std::sync::Arc::new(BoundaryContextResolver::default()));
    let options = deadpan_jobs::GenerationOptions {
        motion: MotionAmount::Subtle,
        instructions: Some(deadpan_jobs::HoldInstructions::new("Keep the hands still.").unwrap()),
        region_target: deadpan_jobs::GenerationTarget::None,
    };
    let mut inputs = picture_inputs();
    options.apply_to(&mut inputs.constraints);
    let revision = store.head_revision().unwrap();
    let first = allocate(
        &mut store,
        AllocateInput {
            hold: hold_id(),
            expected_revision: revision,
            seed: 7,
            inputs,
        },
    )
    .unwrap();
    let result = run_synthetic(&mut store, &first, &worker);
    assert_eq!(result.state, JobState::Ready, "{:?}", result.failure);
    acceptance::accept(
        &mut store,
        &first.request.request_id,
        RevisionId::new("accept-first").unwrap(),
    )
    .unwrap();
    let output = edit(
        &mut store,
        "extend",
        Command::SetHoldDuration {
            node: hold_id(),
            duration: FrameDuration::new(30).unwrap(),
        },
    );
    let queue = store.generation_preparations(None, 64).unwrap();
    assert_eq!(queue.len(), 1);
    let preparation = &queue[0];
    assert_eq!(
        output["outcome"]["generation_preparations"],
        serde_json::json!([preparation.id])
    );
    assert_eq!(preparation.state, PreparationState::Queued);
    assert_eq!(
        preparations::resolve_options(&package, preparation, &AtomicBool::new(false)).unwrap(),
        options
    );
    // The same accepted object supplies controls when copied to another Hold
    // that has no operational request. No media decoder or model is needed.
    let mut copied = preparation.clone();
    let PreparationOrigin::AcceptedExtension { controls, .. } = &mut copied.origin else {
        panic!("expected an accepted extension");
    };
    *controls = PreparationControls::AcceptedArtifact;
    assert_eq!(
        preparations::resolve_options(&package, &copied, &AtomicBool::new(false)).unwrap(),
        options
    );
    assert!(matches!(
        preparations::resolve_options(&package, &copied, &AtomicBool::new(true)),
        Err(GenerationError::Cancelled)
    ));

    let claim = store
        .claim_generation_preparation(&preparation.id, &store.head_revision().unwrap())
        .unwrap();
    edit(
        &mut store,
        "rename",
        Command::Rename {
            node: hold_id(),
            label: "Longer pause".into(),
        },
    );
    assert!(
        !store
            .generation_preparation_claim_is_current(&claim)
            .unwrap()
    );
    let replacement_inputs = || {
        let mut inputs = picture_inputs_for_frames([200, 40, 40], [40, 40, 200], 30);
        options.apply_to(&mut inputs.constraints);
        inputs
    };
    assert!(
        allocate_preparation_with_provider(
            &mut store,
            &claim,
            replacement_inputs(),
            crate::generation::development_provider(8)
        )
        .is_err()
    );
    assert!(
        store.current_generation_requests().unwrap().is_empty(),
        "stale allocation left no request"
    );
    let claim = store
        .claim_generation_preparation(&preparation.id, &store.head_revision().unwrap())
        .unwrap();
    let replacement = allocate_preparation_with_provider(
        &mut store,
        &claim,
        replacement_inputs(),
        crate::generation::development_provider(8),
    )
    .unwrap();
    let recorded = store
        .generation_preparation(&preparation.id)
        .unwrap()
        .unwrap();
    assert_eq!(recorded.state, PreparationState::Fulfilled);
    assert_eq!(
        recorded.request_id.as_ref(),
        Some(&replacement.request.request_id)
    );
    assert_eq!(replacement.ordinal(), 1);
    assert_eq!(
        deadpan_jobs::GenerationOptions::from_constraints(&replacement.request.constraints),
        options
    );
    let before_ready = store.snapshot().unwrap();
    let result = run_synthetic(&mut store, &replacement, &worker);
    assert_eq!(result.state, JobState::Ready, "{:?}", result.failure);
    assert_eq!(
        store.snapshot().unwrap(),
        before_ready,
        "Ready cannot accept itself"
    );
    acceptance::accept(
        &mut store,
        &replacement.request.request_id,
        RevisionId::new("accept-replacement").unwrap(),
    )
    .unwrap();
    let accepted = store.snapshot().unwrap();
    let deadpan_core::NodeKind::Hold { recipe } = &accepted.nodes()[&hold_id()].kind else {
        panic!("Hold");
    };
    assert_eq!(recipe.duration.frames(), 30);
    assert_eq!(recipe.audio, HoldAudio::Silence);
    assert!(matches!(recipe.video, HoldVideo::Generated { .. }));
    let undo = RevisionId::new("undo-replacement").unwrap();
    store.undo(accepted.revision_id(), undo).unwrap();
    let mut restored = serde_json::to_value(store.snapshot().unwrap()).unwrap();
    restored["revision_id"] = serde_json::json!(before_ready.revision_id());
    assert_eq!(restored, serde_json::to_value(before_ready).unwrap());
}
