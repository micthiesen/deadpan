//! Durable operation choice with real authored-black input evidence. These
//! tests never fabricate a measured source, candidate bundle, or Ready movie.

use super::*;
use crate::generation::GenerationRequestInput;
use crate::generation_attempts::BeginGenerationAttempt;
use crate::generation_inputs::{ExtensionCapturePolicy, GenerationCaptureSpec, GenerationInputs};
use crate::generation_pictures::GenerationPictureIdentity;
use crate::generation_preparations::{
    PreparationClaim, PreparationState, StoredGenerationPreparation,
};
use deadpan_core::{
    AudioTimingId, BeatNode, ColorPolicy, Command, CommandRequest, ExtensionDirection, FrameRate,
    HoldAudio, HoldRecipe, NodeId, PresentationBasis, ProjectFrame, SplitIdentities, Subtree,
};
use deadpan_jobs::{
    AttemptId, CancellationToken, GenerationModePreference, GenerationPlan, HoldConstraints,
    JobState, MessageIdentity, MotionAmount, ProtocolVersion, Relevance, Sha256 as ContentHash,
    VideoSpec,
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn revision(name: &str) -> RevisionId {
    RevisionId::new(name).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn rate() -> FrameRate {
    FrameRate::new(24, 1).unwrap()
}

pub(super) fn document(length: i64) -> Result<ProjectDocument> {
    let empty = ProjectDocument::new(
        ProjectId::new("extension-intent-fixture")?,
        revision("empty"),
        PresentationBasis {
            width: 512,
            height: 320,
            frame_rate: rate(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )?;
    let command = CommandRequest {
        project_id: empty.project_id().clone(),
        expected_revision: empty.revision_id().clone(),
        new_revision: revision("initial"),
        command: Command::Insert {
            parent: id("root"),
            index: 0,
            subtree: Subtree {
                root: id("black"),
                nodes: BTreeMap::from([(
                    id("black"),
                    BeatNode::hold(
                        "Authored black",
                        HoldRecipe {
                            duration: frames(length),
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
    Ok(deadpan_core::apply(&empty, &command)?
        .forward
        .apply(&empty)?)
}

pub(super) fn insert(store: &mut ProjectStore, at: i64) -> Result<StoredGenerationPreparation> {
    let before = store.snapshot()?;
    let split = before.insert_time_target(ProjectFrame(at))?.split;
    let request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision("insert-ai"),
        command: Command::InsertAiTime {
            at: ProjectFrame(at),
            id: id("ai"),
            hold: HoldRecipe {
                duration: frames(12),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
                picture_context: None,
            },
            identities: SplitIdentities {
                nodes: (0..split.map_or(0, |split| split.required_ids))
                    .map(|index| id(&format!("split-{index}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision("insert-ai"),
                ordinal: 0,
            },
        },
    };
    let outcome = store.commit(&request)?;
    assert_eq!(outcome.generation_preparations.len(), 1);
    assert_eq!(
        store.snapshot()?.duration()?.frames(),
        before.duration()?.frames() + 12
    );
    Ok(store
        .generation_preparation(&outcome.generation_preparations[0])?
        .unwrap())
}

fn spec(direction: ExtensionDirection) -> GenerationCaptureSpec {
    GenerationCaptureSpec::Extension {
        direction,
        native_rate: rate(),
        context_frames: 9,
        policy: ExtensionCapturePolicy::TemporalContextV1,
    }
}

fn plan(direction: ExtensionDirection) -> Result<GenerationPlan> {
    // Reuse the generic V3 wire contract, not a claim about an installed model.
    let mut wire: serde_json::Value = serde_json::from_str(include_str!(
        "../../../deadpan-jobs/tests/fixtures/generate_extension_v3.json"
    ))?;
    wire["plan"]["sampling"]["direction"] = serde_json::to_value(direction)?;
    Ok(GenerationPlan::Extension(serde_json::from_value(
        wire["plan"].clone(),
    )?))
}

fn input(
    store: &ProjectStore,
    preparation: &StoredGenerationPreparation,
    name: &str,
    plan: &GenerationPlan,
) -> Result<GenerationRequestInput> {
    let wire: serde_json::Value = serde_json::from_str(include_str!(
        "../../../deadpan-jobs/tests/fixtures/generate_extension_v3.json"
    ))?;
    let dimensions = plan.native_dimensions();
    Ok(GenerationRequestInput {
        request_id: RequestId::new(name)?,
        expected_revision: store.head_revision()?,
        hold_id: preparation.target.node.clone(),
        context_sha256: ContentHash::new("a".repeat(64))?,
        constraints: HoldConstraints {
            video: VideoSpec::new(
                plan.project_frames(),
                plan.project_frame_rate(),
                dimensions.width(),
                dimensions.height(),
            )?,
            conditioning: plan.conditioning(),
            motion: MotionAmount::Still,
            instructions: None,
            region_target: None,
        },
        provider: serde_json::from_value(wire["provider"].clone())?,
    })
}

fn attempt(request: &RequestId) -> Result<BeginGenerationAttempt> {
    Ok(BeginGenerationAttempt {
        identity: MessageIdentity::new(request.clone(), AttemptId::new("attempt")?),
        cancellation_token: CancellationToken::new("cancel")?,
    })
}

fn assert_refused_atomically(
    store: &mut ProjectStore,
    claim: &PreparationClaim,
    input: GenerationRequestInput,
    plan: GenerationPlan,
) -> Result {
    let before = store.snapshot()?;
    let preparation = store.generation_preparation(&claim.preparation.id)?;
    let heads = store.generation_intents(None, 16)?;
    let scopes: i64 =
        store
            .connection
            .query_row("SELECT COUNT(*) FROM generation_scopes", [], |row| {
                row.get(0)
            })?;
    let attempt = attempt(&input.request_id)?;
    let identity = attempt.identity.clone();
    assert!(
        store
            .fulfil_generation_preparation(claim, input, plan, attempt)
            .is_err()
    );
    assert_eq!(store.snapshot()?, before);
    assert_eq!(
        store.generation_preparation(&claim.preparation.id)?,
        preparation
    );
    assert_eq!(store.generation_intents(None, 16)?, heads);
    assert!(store.current_generation_requests()?.is_empty());
    assert!(store.generation_request(&identity.request_id)?.is_none());
    let attempts: i64 = store.connection.query_row(
        "SELECT COUNT(*) FROM generation_attempts WHERE request_id=?1",
        [identity.request_id.as_str()],
        |row| row.get(0),
    )?;
    assert_eq!(attempts, 0);
    assert_eq!(
        store
            .connection
            .query_row("SELECT COUNT(*) FROM generation_scopes", [], |row| row
                .get::<_, i64>(0))?,
        scopes
    );
    assert!(store.generation_preparation_claim_is_current(claim)?);
    Ok(())
}

#[test]
fn inserted_intents_resolve_edges_before_temporal_availability_and_keep_interior_bridge() -> Result
{
    for (length, at, direction) in [
        (2, 0, Some(ExtensionDirection::FromRight)),
        (2, 2, Some(ExtensionDirection::FromLeft)),
        (24, 0, Some(ExtensionDirection::FromRight)),
        (24, 24, Some(ExtensionDirection::FromLeft)),
        (24, 12, None),
    ] {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join("capture.deadpan");
        let mut store = ProjectStore::create(&path, &document(length)?)?;
        let preparation = insert(&mut store, at)?;
        let expected = direction.map_or(GenerationCaptureSpec::Bridge, spec);
        assert_eq!(preparation.intent.capture, Some(expected));
        assert_eq!(
            preparation.origin.options().unwrap().mode,
            GenerationModePreference::Automatic
        );
        match &preparation.intent.input_binding {
            IntentInputBinding::Unavailable { cause, .. } => {
                assert_eq!(length, 2);
                assert_eq!(*cause, InputUnavailableCause::MissingContext);
                let claim =
                    store.claim_generation_preparation(&preparation.id, &store.head_revision()?)?;
                let plan = plan(direction.unwrap())?;
                let input = input(&store, &preparation, "short-context", &plan)?;
                assert_refused_atomically(&mut store, &claim, input, plan)?;
            }
            IntentInputBinding::Measured { binding } => {
                assert_eq!(length, 24);
                assert_eq!(binding.capture_spec(), expected);
                match &binding.inputs {
                    GenerationInputs::Bridge { left, right } => {
                        assert!(direction.is_none());
                        assert_eq!(left, &Some(GenerationPictureIdentity::AuthoredBlack));
                        assert_eq!(right, left);
                    }
                    GenerationInputs::Extension {
                        samples,
                        opposite,
                        support,
                        ..
                    } => {
                        assert!(direction.is_some());
                        assert_eq!(samples.len(), 9);
                        assert!(samples.iter().all(
                            |sample| sample.picture == GenerationPictureIdentity::AuthoredBlack
                        ));
                        assert!(
                            opposite.is_none(),
                            "the recipe's black fallback is not an opposite endpoint"
                        );
                        assert!(!support.is_empty());
                    }
                }
            }
        }
        let retained = store.generation_preparation(&preparation.id)?.unwrap();
        assert_eq!(
            store.generation_intents(None, 1)?[0].birth,
            preparation.intent_birth()
        );
        store.validate_full()?;
        drop(store);
        let store = ProjectStore::open(&path, crate::AccessMode::ReadOnly)?;
        assert_eq!(
            store.generation_preparation(&preparation.id)?,
            Some(retained)
        );
        assert_eq!(
            store.generation_intents(None, 1)?[0].birth.receipt.capture,
            Some(expected)
        );
        store.validate_full()?;
    }
    Ok(())
}

#[test]
fn measured_extension_fulfilment_requires_captured_direction_context_and_controls() -> Result {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let scratch = tempfile::tempdir()?;
        let mut store =
            ProjectStore::create(&scratch.path().join("fulfil.deadpan"), &document(24)?)?;
        let preparation = insert(
            &mut store,
            if direction == ExtensionDirection::FromLeft {
                24
            } else {
                0
            },
        )?;
        let claim = store.claim_generation_preparation(&preparation.id, &store.head_revision()?)?;
        let captured = plan(direction)?;
        let opposite = plan(match direction {
            ExtensionDirection::FromLeft => ExtensionDirection::FromRight,
            ExtensionDirection::FromRight => ExtensionDirection::FromLeft,
        })?;
        let input_opposite = input(&store, &preparation, "candidate", &opposite)?;
        assert_refused_atomically(&mut store, &claim, input_opposite, opposite)?;

        let mut changed_context = serde_json::to_value(&captured)?;
        changed_context["plan"]["sampling"]["context_frame_count"] = 17.into();
        let changed_context: GenerationPlan = serde_json::from_value(changed_context)?;
        let context_input = input(&store, &preparation, "candidate", &changed_context)?;
        assert_refused_atomically(&mut store, &claim, context_input, changed_context)?;

        let mut changed_controls = input(&store, &preparation, "candidate", &captured)?;
        changed_controls.constraints.motion = MotionAmount::Subtle;
        assert_refused_atomically(&mut store, &claim, changed_controls, captured.clone())?;
        let mut changed_duration = serde_json::to_value(&captured)?;
        changed_duration["plan"]["sampling"]["output_frame_count"] = 13.into();
        let changed_duration: GenerationPlan = serde_json::from_value(changed_duration)?;
        let duration_input = input(&store, &preparation, "candidate", &changed_duration)?;
        assert_refused_atomically(&mut store, &claim, duration_input, changed_duration)?;

        let request_input = input(&store, &preparation, "candidate", &captured)?;
        let begun = attempt(&request_input.request_id)?;
        let before = store.snapshot()?;
        let (request, attempt) =
            store.fulfil_generation_preparation(&claim, request_input, captured.clone(), begun)?;
        assert_eq!(request.plan, Some(captured));
        assert_eq!(request.binding.request_version.get(), 1);
        assert_eq!(attempt.ordinal, 1);
        assert_eq!(attempt.checkpoint.protocol, ProtocolVersion::V3);
        assert_eq!(attempt.checkpoint.state, JobState::Queued);
        let IntentInputBinding::Measured { binding } = &preparation.intent.input_binding else {
            panic!("measured black context")
        };
        assert_eq!(request.input_binding.as_ref(), Some(binding.as_ref()));
        assert_eq!(
            store.snapshot()?,
            before,
            "fulfilment records metadata, not authored pictures"
        );
        assert_eq!(
            store
                .generation_preparation(&preparation.id)?
                .unwrap()
                .state,
            PreparationState::Fulfilled
        );
        assert!(
            store
                .selected_generation_bundle(&request.request_id)?
                .is_none()
        );
        store.validate_full()?;
    }
    Ok(())
}

#[test]
fn extension_birth_survives_retirement_reopen_and_fresh_redo_without_resurrection() -> Result {
    for direction in [ExtensionDirection::FromLeft, ExtensionDirection::FromRight] {
        let scratch = tempfile::tempdir()?;
        let path = scratch.path().join("history.deadpan");
        let initial = document(24)?;
        let mut store = ProjectStore::create(&path, &initial)?;
        let first = insert(
            &mut store,
            if direction == ExtensionDirection::FromLeft {
                24
            } else {
                0
            },
        )?;
        let claim = store.claim_generation_preparation(&first.id, &store.head_revision()?)?;
        let captured = plan(direction)?;
        let request_input = input(&store, &first, "candidate", &captured)?;
        let begun = attempt(&request_input.request_id)?;
        let identity = begun.identity.clone();
        let cancellation = begun.cancellation_token.clone();
        let (request, _) =
            store.fulfil_generation_preparation(&claim, request_input, captured.clone(), begun)?;
        store.request_generation_attempt_cancel(&identity, &cancellation)?;
        store.finish_generation_attempt_cancelled(&identity, &cancellation)?;
        crate::generation_preparations::compact_completed_for_test(&store.connection)?;
        crate::audit::refresh_generation_scopes(&store.connection)?;
        assert!(store.generation_preparation(&first.id)?.is_none());
        let current = store.generation_intents(None, 1)?.remove(0);
        assert_eq!(current.birth, first.intent_birth());
        assert_eq!(current.head.request_id, Some(request.request_id.clone()));
        store.validate_full()?;
        drop(store);
        let mut store = ProjectStore::open(&path, crate::AccessMode::ReadWrite)?;
        assert_eq!(
            store.generation_intents(None, 1)?[0].birth,
            first.intent_birth()
        );
        assert_eq!(
            store.generation_request(&request.request_id)?.unwrap().plan,
            Some(captured)
        );
        assert_eq!(
            store
                .generation_attempt(&identity)?
                .unwrap()
                .checkpoint
                .protocol,
            ProtocolVersion::V3
        );
        assert!(store.cancel_generation_intent(&first.id, &store.head_revision()?)?);
        assert_eq!(
            store
                .generation_request(&request.request_id)?
                .unwrap()
                .relevance,
            Relevance::Stale
        );
        store.undo(&store.head_revision()?, revision("undo-ai"))?;
        assert_eq!(store.snapshot()?.nodes(), initial.nodes());
        assert!(store.generation_intents(None, 1)?.is_empty());
        let redo = store.redo(&store.head_revision()?, revision("redo-ai"))?;
        assert_eq!(redo.generation_preparations.len(), 1);
        let next = store
            .generation_preparation(&redo.generation_preparations[0])?
            .unwrap();
        assert_ne!(next.id, first.id);
        assert_eq!(next.intent.capture, first.intent.capture);
        assert_eq!(next.intent.input_binding, first.intent.input_binding);
        assert_eq!(
            next.intent.authorization,
            IntentAuthorization::Redo {
                original_activation: first.id.clone()
            }
        );
        assert_eq!(next.state, PreparationState::Queued);
        assert!(next.request_id.is_none());
        assert_eq!(
            read_birth(&store.connection, &first.id)?,
            first.intent_birth()
        );
        assert_eq!(
            read_terminal(&store.connection, &first.id)?.unwrap().reason,
            IntentTerminalReason::UserCancelled
        );
        store.validate_full()?;
        drop(store);
        let store = ProjectStore::open(&path, crate::AccessMode::ReadOnly)?;
        assert_eq!(
            store.generation_intents(None, 1)?[0].birth.receipt.capture,
            Some(spec(direction))
        );
        assert_eq!(
            store.generation_intents(None, 1)?[0].head.activation_id,
            next.id
        );
        store.validate_full()?;
    }
    Ok(())
}
