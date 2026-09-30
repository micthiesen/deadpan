//! Current-format store fixtures, following tests/render_jobs.rs. Opaque movie
//! bytes and synthetic observations exercise persistence, not media validity.
use super::{
    BeginRenderAttempt, RenderAttemptTransition, StoredRenderAttempt, StoredRenderCheckpoint,
};
use crate::{ProjectStore, render_media::RenderMediaLimits};
use deadpan_core::{
    BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRange, FrameRate,
    HoldAudio, HoldRecipe, HoldVideo, NodeId, PresentationBasis, ProjectDocument, ProjectFrame,
    ProjectId, RevisionId, Subtree,
};
use deadpan_jobs::{
    AttemptId, CancellationToken, RequestId, Sha256,
    render::{
        RenderBFrames, RenderEncoder, RenderEngineeringPolicy, RenderIntent, RenderPolicy,
        RenderSelection, RenderVerificationObservation, document_sha256,
    },
};
use sha2::{Digest, Sha256 as DigestSha256};
use std::{
    collections::BTreeMap,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

pub(crate) fn store() -> Result<(tempfile::TempDir, ProjectStore)> {
    let document = ProjectDocument::new(
        ProjectId::new("synthetic-render-project")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 640,
            height: 360,
            frame_rate: FrameRate::new(30, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?;
    let edit = deadpan_core::apply(
        &document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new("baseline")?,
            command: Command::Insert {
                parent: document.root().clone(),
                index: 0,
                subtree: Subtree {
                    root: NodeId::new("hold")?,
                    nodes: BTreeMap::from([(
                        NodeId::new("hold")?,
                        BeatNode::hold(
                            "synthetic pause",
                            HoldRecipe {
                                picture_context: None,
                                duration: FrameDuration::new(12)?,
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                            },
                        ),
                    )]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        },
    )?;
    let root = tempfile::tempdir()?;
    let store = ProjectStore::create(
        &root.path().join("synthetic-render.deadpan"),
        &edit.forward.apply(&document)?,
    )?;
    Ok((root, store))
}

pub(crate) fn verified(store: &mut ProjectStore, job_name: &str) -> Result<StoredRenderAttempt> {
    let document = store.snapshot()?;
    let intent = RenderIntent {
        schema_version: 1,
        job_id: RequestId::new(job_name)?,
        project_id: document.project_id().clone(),
        revision_id: document.revision_id().clone(),
        document_sha256: document_sha256(&document, &AtomicBool::new(false), deadline())?,
        range: FrameRange::new(ProjectFrame(0), ProjectFrame(document.duration()?.frames()))?,
        policy: RenderPolicy::Engineering(RenderEngineeringPolicy {
            schema_version: 1,
            selection: RenderSelection::ExplicitEngineering,
            encoder: RenderEncoder::Software,
            b_frames: RenderBFrames::None,
        }),
    };
    let job = store.create_render_job(intent, &AtomicBool::new(false), deadline())?;
    let queued = store.begin_render_attempt(BeginRenderAttempt {
        job_id: job.job_id,
        attempt_id: AttemptId::new(format!("{job_name}-encode"))?,
        cancellation_token: CancellationToken::new(format!("{job_name}-encode-cancel"))?,
        checkpoint_attempt_id: None,
    })?;
    let encoding =
        store.transition_render_attempt(&queued.identity(), RenderAttemptTransition::Encoding)?;
    let movie = b"synthetic opaque movie byte fixture";
    let hash = Sha256::new(
        DigestSha256::digest(movie)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
    )?;
    let prepared = store.render_write_handle()?.prepare_retention(
        &encoding.identity(),
        &mut &movie[..],
        u64::try_from(movie.len())?,
        &hash,
        b"{\"synthetic_opaque_manifest\":true}",
        RenderMediaLimits::new(1024, 1024, 2048, 8192, 16)?,
        &AtomicBool::new(false),
        deadline(),
    )?;
    let retained = store.retain_render_checkpoint(
        &encoding.identity(),
        &prepared,
        &AtomicBool::new(false),
        deadline(),
    )?;
    finish_verification(store, &retained)
}

pub(crate) fn verify_again(
    store: &mut ProjectStore,
    previous: &StoredRenderAttempt,
    name: &str,
) -> Result<StoredRenderAttempt> {
    let queued = store.begin_render_attempt(BeginRenderAttempt {
        job_id: previous.job_id.clone(),
        attempt_id: AttemptId::new(name)?,
        cancellation_token: CancellationToken::new(format!("{name}-cancel"))?,
        checkpoint_attempt_id: previous.checkpoint_attempt_id.clone(),
    })?;
    finish_verification(store, &queued)
}

fn finish_verification(
    store: &mut ProjectStore,
    attempt: &StoredRenderAttempt,
) -> Result<StoredRenderAttempt> {
    let checkpoint = store.render_checkpoint(
        &attempt.job_id,
        attempt
            .checkpoint_attempt_id
            .as_ref()
            .ok_or("synthetic fixture has no checkpoint")?,
    )?;
    let verifying =
        store.transition_render_attempt(&attempt.identity(), RenderAttemptTransition::Verifying)?;
    Ok(store.record_render_verification(&verifying.identity(), observation(&checkpoint))?)
}

fn observation(checkpoint: &StoredRenderCheckpoint) -> RenderVerificationObservation {
    RenderVerificationObservation {
        schema_version: 1,
        validator_id: "synthetic-store-fixture".into(),
        validator_version: "v1".into(),
        movie_sha256: checkpoint.media.movie_sha256().clone(),
        movie_byte_length: checkpoint.media.movie().byte_length(),
        report: serde_json::json!({"synthetic_observation":true,"decoded_media":false}),
    }
}
