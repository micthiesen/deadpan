//! Automatic replacements go through ordinary duration edits and the native
//! generation worker. Synthetic media replaces inference, not admission,
//! qualification, durable claims, acceptance, or history.
use super::*;
use deadpan_store::generation_preparations::PreparationState;

fn accepted(scripts: impl IntoIterator<Item = Script>) -> Fixture {
    let mut fixture = project_with_pause(Backend::Scripted(Arc::new(ScriptQueue::new(
        std::iter::once(ready_script()).chain(scripts),
    ))));
    let mut operation = start(&fixture, 1);
    if let GenerationOperation::Start { options, .. } = &mut operation {
        *options = Some(deadpan_jobs::GenerationOptions {
            motion: deadpan_jobs::MotionAmount::Still,
            instructions: Some(
                deadpan_jobs::HoldInstructions::new("Keep the subject still.").unwrap(),
            ),
            region_target: deadpan_jobs::GenerationTarget::None,
        });
    }
    generation(&fixture.service, operation);
    let ready = job_until(&fixture.service, |job| !job.running());
    assert!(
        matches!(outcome(&ready), Some(Outcome::Ready(_))),
        "{:?}",
        outcome(&ready)
    );
    let candidate = ready.generation.unwrap().candidates[&ordinary(&fixture.hold)].clone();
    let update = generation(
        &fixture.service,
        GenerationOperation::Accept {
            session: fixture.workspace.session,
            revision: fixture.workspace.document.revision_id().clone(),
            request: candidate.request,
            attempt: candidate.selected,
            hold: fixture.hold.clone(),
            cursor: ProjectFrame(10),
            scope: SequenceScope::default(),
            scoped: None,
        },
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    fixture.workspace = update.workspace.unwrap();
    fixture
}

fn extend(fixture: &Fixture) -> ProjectUpdate {
    let update = command(
        &fixture.service,
        edit_request(
            &fixture.workspace,
            ProjectEdit::HoldDuration {
                node: fixture.hold.clone(),
                duration: FrameDuration::new(18).unwrap(),
            },
        ),
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    let workspace = update.workspace.as_ref().unwrap();
    let NodeKind::Hold { recipe } = &workspace.document.nodes()[&fixture.hold].kind else {
        panic!("Hold remains");
    };
    assert_eq!(recipe.duration.frames(), 18);
    assert!(
        matches!(recipe.video, HoldVideo::Freeze { .. }),
        "extension commits deterministic fallback immediately"
    );
    update
}

fn unavailable(
    fixture: &Fixture,
    initial: &ProjectUpdate,
) -> crate::project::generation::Preparation {
    if let Some(item) = initial.generation.as_ref().and_then(|state| {
        state
            .preparations
            .iter()
            .find(|item| item.state == PreparationState::Unavailable)
    }) {
        return item.clone();
    }
    let update = wait(&fixture.service, |update| {
        update.generation.as_ref().is_some_and(|state| {
            state
                .preparations
                .iter()
                .any(|item| item.state == PreparationState::Unavailable)
        })
    });
    update
        .generation
        .unwrap()
        .preparations
        .iter()
        .find(|item| item.state == PreparationState::Unavailable)
        .unwrap()
        .clone()
}

#[test]
fn extending_an_accepted_pause_automatically_generates_full_replacement_without_accepting() {
    if !synthetic_ready_available() {
        eprintln!("skipped: synthetic Ready tools unavailable");
        return;
    }
    let fixture = accepted([ready_script()]);
    let extended_update = extend(&fixture);
    let extended = extended_update.workspace.as_ref().unwrap().clone();
    let finished = job_until(&fixture.service, |job| {
        job.revision == *extended.document.revision_id() && !job.running()
    });
    let state = finished.generation.unwrap();
    let job = state.job.unwrap();
    assert!(
        matches!(job.outcome, Some(Outcome::Ready(_))),
        "{:?}",
        job.outcome
    );
    assert!(!job.controls_pending);
    assert_eq!(job.options.motion, deadpan_jobs::MotionAmount::Still);
    assert_eq!(
        job.options.instructions.as_ref().unwrap().as_str(),
        "Keep the subject still."
    );
    assert_eq!(
        job.options.region_target,
        deadpan_jobs::GenerationTarget::None
    );
    let candidate = &state.candidates[&ordinary(&fixture.hold)];
    assert_eq!(candidate.variants.len(), 1);
    assert_eq!(candidate.variants[0].sampled_frames, 18);
    assert!(
        state.preparations.is_empty(),
        "fulfilled preparations leave recovery rows"
    );
    assert_eq!(
        finished.workspace.unwrap().document,
        extended.document,
        "background Ready never accepts or writes timing"
    );
    let store = reader(&extended);
    let request = store
        .generation_request(&candidate.request)
        .unwrap()
        .unwrap();
    assert_eq!(request.origin_revision, *extended.document.revision_id());
    assert_eq!(request.origin_target, ordinary(&fixture.hold));
    assert_eq!(request.constraints.video.frames().frames(), 18);
}

#[test]
fn missing_runtime_is_durable_retryable_and_discardable_without_changing_the_edit() {
    if !synthetic_ready_available() {
        eprintln!("skipped: synthetic Ready tools unavailable");
        return;
    }
    let missing = Script {
        unavailable: Some("Install the local model pack before retrying.".into()),
        ..waiting(0)
    };
    let fixture = accepted([missing.clone(), missing]);
    let extended_update = extend(&fixture);
    let extended = extended_update.workspace.as_ref().unwrap().clone();
    let first = unavailable(&fixture, &extended_update);
    assert_eq!(first.target, ordinary(&fixture.hold));
    assert_eq!(first.frames, 18);
    assert_eq!(first.revision, *extended.document.revision_id());
    assert!(first.retryable());
    let stored = reader(&extended)
        .generation_preparation(&first.id)
        .unwrap()
        .unwrap();
    assert!(stored.request_id.is_none());
    let stale = generation(
        &fixture.service,
        GenerationOperation::RetryPreparation {
            ticket: 10,
            session: extended.session,
            revision: fixture.workspace.document.revision_id().clone(),
            id: first.id.clone(),
        },
    );
    assert_eq!(
        refusal(&stale).as_deref(),
        Some("Project changed before the request")
    );
    let retried = generation(
        &fixture.service,
        GenerationOperation::RetryPreparation {
            ticket: 11,
            session: extended.session,
            revision: first.revision.clone(),
            id: first.id.clone(),
        },
    );
    assert!(refusal(&retried).is_none());
    let second = unavailable(&fixture, &retried);
    assert!(second.sequence > first.sequence);
    let stale_discard = generation(
        &fixture.service,
        GenerationOperation::DiscardPreparation {
            ticket: 12,
            session: extended.session,
            id: first.id.clone(),
            sequence: first.sequence,
        },
    );
    assert!(
        refusal(&stale_discard).is_some(),
        "a stale row cannot discard a newer retry"
    );
    let discarded = generation(
        &fixture.service,
        GenerationOperation::DiscardPreparation {
            ticket: 13,
            session: extended.session,
            id: second.id.clone(),
            sequence: second.sequence,
        },
    );
    assert!(refusal(&discarded).is_none());
    assert_eq!(discarded.workspace.unwrap().document, extended.document);
    assert!(discarded.generation.unwrap().preparations.is_empty());
    assert_eq!(
        reader(&extended)
            .generation_preparation(&second.id)
            .unwrap()
            .unwrap()
            .state,
        PreparationState::Cancelled
    );
}

#[test]
fn undo_cancels_a_blocked_replacement_and_retry_cannot_resurrect_it() {
    if !synthetic_ready_available() {
        eprintln!("skipped: synthetic Ready tools unavailable");
        return;
    }
    let fixture = accepted([Script {
        unavailable: Some("Missing model".into()),
        ..waiting(0)
    }]);
    let extended_update = extend(&fixture);
    let extended = extended_update.workspace.as_ref().unwrap().clone();
    let preparation = unavailable(&fixture, &extended_update);
    let undone = command(
        &fixture.service,
        ProjectRequest::Undo {
            expected_revision: extended.document.revision_id().clone(),
        },
    );
    assert!(undone.error.is_none(), "{:?}", undone.error);
    let workspace = undone.workspace.unwrap();
    let mut restored = serde_json::to_value(&workspace.document).unwrap();
    let expected = serde_json::to_value(&fixture.workspace.document).unwrap();
    assert_ne!(
        restored["revision_id"], expected["revision_id"],
        "Undo records a new revision"
    );
    restored["revision_id"] = expected["revision_id"].clone();
    assert_eq!(restored, expected, "Undo restores every authored field");
    assert!(undone.generation.unwrap().preparations.is_empty());
    let retry = generation(
        &fixture.service,
        GenerationOperation::RetryPreparation {
            ticket: 12,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            id: preparation.id.clone(),
        },
    );
    assert!(refusal(&retry).is_some());
    assert_eq!(
        reader(&workspace)
            .generation_preparation(&preparation.id)
            .unwrap()
            .unwrap()
            .state,
        PreparationState::Cancelled
    );
}

#[test]
fn a_replacement_uses_the_existing_cancellable_model_slot() {
    if !synthetic_ready_available() {
        eprintln!("skipped: synthetic Ready tools unavailable");
        return;
    }
    let fixture = accepted([waiting(2)]);
    let extended_update = extend(&fixture);
    let extended = extended_update.workspace.as_ref().unwrap().clone();
    let active = job_until(&fixture.service, |job| {
        job.revision == *extended.document.revision_id() && job.phase.steps() == Some((2, 2))
    });
    let job = active.generation.unwrap().job.unwrap();
    let rows = fixture.service.jobs().snapshot();
    assert_eq!(
        rows.iter()
            .filter(|row| row.kind == crate::jobs::JobKind::AiPause)
            .count(),
        1
    );
    let cancel = generation(
        &fixture.service,
        GenerationOperation::Cancel {
            ticket: 99,
            session: extended.session,
            job: job.ticket,
        },
    );
    assert!(refusal(&cancel).is_none());
    let ended = job_until(&fixture.service, |current| {
        current.ticket == job.ticket && !current.running()
    });
    assert_eq!(outcome(&ended), Some(Outcome::Cancelled));
    assert_eq!(ended.workspace.unwrap().document, extended.document);
    assert_eq!(
        attempt_states(&extended, job.request.as_ref().unwrap()),
        [JobState::Cancelled]
    );
}
