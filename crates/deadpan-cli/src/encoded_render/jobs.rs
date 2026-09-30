//! Worker stages for durable engineering renders. The caller owns short store
//! transitions between stages; no stage holds the writable SQLite connection.
//! Reopening a checkpoint always hashes private bytes and runs the independent
//! verifier again. Persisted reports never reconstruct a live media capability.

use std::{
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Instant,
};

use deadpan_core::{FrameRange, RevisionId};
use deadpan_encode::{BFramePolicy, EncoderMode};
use deadpan_jobs::{
    AttemptId, RequestId,
    render::{
        RenderAttemptState, RenderBFrames, RenderEncoder, RenderEngineeringPolicy, RenderIntent,
        RenderVerificationObservation,
    },
};
use deadpan_store::{
    render_jobs::{StoredRenderAttempt, StoredRenderCheckpoint},
    render_media::{
        MAX_RENDER_MANIFEST_BYTES, PreparedRenderRetention, RenderMediaLimits, RenderReadHandle,
        RenderWriteHandle,
    },
};
use serde::{Deserialize, Serialize};

use super::{
    EncodedCandidate, EncodedProgress, EncodedRenderError, EncodedWorkerLimits, check_control,
    host::encode_guarded,
    protocol::{EncodedManifest, EncodedRenderContract, EncoderChoice},
    verification::{
        self, VerificationLimits, VerificationProgress, VerificationRequest, VerifiedCandidate,
    },
};
use crate::{
    export_picture::ExportPictureContract,
    picture::ProjectPictureSession,
    render_worker::{
        RenderPictureRequest, RenderWorkerRuntime, document_hash, protocol::RenderIdentity,
    },
};

#[derive(Debug, Clone)]
pub struct CaptureRenderIntent {
    pub package: PathBuf,
    pub revision: RevisionId,
    pub range: Option<FrameRange>,
    pub job_id: RequestId,
    pub policy: RenderEngineeringPolicy,
}

/// Capture geometry, clocks and the exact document before allocating a job.
/// Call on a preparation worker, then pass the result to create_render_job.
pub fn capture_intent(
    request: CaptureRenderIntent,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<RenderIntent, EncodedRenderError> {
    check_control(cancelled, deadline)?;
    let pictures = ProjectPictureSession::open_revision(
        &request.package,
        &request.revision,
        request.range,
        cancelled,
    )?;
    let contract = ExportPictureContract::capture(&pictures)?;
    EncodedRenderContract::from_contract(&contract, encoder_choice(&request.policy))
        .validate()
        .map_err(EncodedRenderError::Protocol)?;
    let intent = RenderIntent {
        schema_version: 1,
        job_id: request.job_id,
        project_id: contract.project_id().clone(),
        revision_id: contract.revision_id().clone(),
        document_sha256: document_hash(pictures.document(), cancelled, deadline)?,
        range: contract.range(),
        policy: request.policy,
    };
    intent
        .validate()
        .map_err(|error| EncodedRenderError::Protocol(error.to_string()))?;
    check_control(cancelled, deadline)?;
    Ok(intent)
}

#[derive(Debug, Clone)]
pub struct RenderStageRequest {
    pub package: PathBuf,
    pub intent: RenderIntent,
    /// The exact committed phase and sequence returned by the store.
    pub attempt: StoredRenderAttempt,
}

/// Encode one immutable intent and retain both complete objects. The caller
/// first commits Encoding and then checkpoints the returned opaque token.
/// Failure may leave owned orphan objects; it never deletes retained work.
pub fn encode_and_retain(
    runtime: &RenderWorkerRuntime,
    request: &RenderStageRequest,
    writer: &RenderWriteHandle,
    limits: (EncodedWorkerLimits, RenderMediaLimits),
    cancelled: &AtomicBool,
    deadline: Instant,
    progress: impl FnMut(EncodedProgress),
) -> Result<PreparedRenderRetention, EncodedRenderError> {
    validate_attempt(request, RenderAttemptState::Encoding)?;
    if request.attempt.checkpoint_attempt_id.is_some() {
        return Err(EncodedRenderError::Configuration(
            "encoding cannot replace an existing checkpoint",
        ));
    }
    writer.check_live(cancelled)?;
    let expected = reconstruct_contract(&request.package, &request.intent, cancelled, deadline)?;
    let mut candidate = encode_guarded(
        runtime,
        RenderPictureRequest {
            package: request.package.clone(),
            revision: request.intent.revision_id.clone(),
            range: Some(request.intent.range),
            identity: render_identity(&request.attempt),
            cancellation_token: request.attempt.cancellation_token.clone(),
        },
        encoder_choice(&request.intent.policy),
        limits.0,
        cancelled,
        deadline,
        progress,
        || {
            writer
                .check_live(cancelled)
                .map_err(EncodedRenderError::from)
        },
    )?;
    writer.check_live(cancelled)?;
    let manifest = RetainedRenderManifest {
        schema_version: 1,
        intent: request.intent.clone(),
        encoding_attempt_id: request.attempt.attempt_id.clone(),
        encoded: candidate.manifest().clone(),
    };
    manifest.validate(&request.intent, &request.attempt.attempt_id, &expected)?;
    let bytes = manifest_bytes(&manifest)?;
    let expected_bytes = candidate.byte_length();
    let expected_sha256 = candidate.manifest().movie.sha256().clone();
    let mut reader = CandidateReader {
        candidate: &mut candidate,
        offset: 0,
        cancelled,
        deadline,
    };
    let retained = writer.prepare_retention(
        &request.attempt.identity(),
        &mut reader,
        expected_bytes,
        &expected_sha256,
        &bytes,
        limits.1,
        cancelled,
        deadline,
    );
    // Preserve cancellation/deadline categories when a controlled read failed.
    check_control(cancelled, deadline)?;
    Ok(retained?)
}

/// Rehash retained bytes, bind their strict manifest to the historical
/// document and run a fresh isolated verifier. The caller first commits
/// Verifying. This function never accepts a persisted verification report.
pub fn verify_checkpoint(
    runtime: &RenderWorkerRuntime,
    request: &RenderStageRequest,
    checkpoint: (&StoredRenderCheckpoint, &RenderReadHandle),
    limits: (VerificationLimits, RenderMediaLimits),
    cancelled: &AtomicBool,
    deadline: Instant,
    progress: impl FnMut(VerificationProgress),
) -> Result<VerifiedCandidate, EncodedRenderError> {
    validate_attempt(request, RenderAttemptState::Verifying)?;
    let (checkpoint, reader) = checkpoint;
    if checkpoint.job_id != request.intent.job_id
        || request.attempt.checkpoint_attempt_id.as_ref() != Some(&checkpoint.encoding_attempt_id)
    {
        return Err(EncodedRenderError::Configuration(
            "verification checkpoint belongs to another attempt or job",
        ));
    }
    check_control(cancelled, deadline)?;
    let snapshot = reader.snapshot(&checkpoint.media, limits.1, cancelled, deadline)?;
    let manifest: RetainedRenderManifest = serde_json::from_slice(snapshot.manifest_bytes())?;
    let contract = reconstruct_contract(&request.package, &request.intent, cancelled, deadline)?;
    manifest.validate(&request.intent, &checkpoint.encoding_attempt_id, &contract)?;
    let candidate = EncodedCandidate::from_retained(
        contract,
        request.intent.document_sha256.clone(),
        manifest.encoded,
        snapshot,
        cancelled,
        deadline,
    )?;
    verification::verify(
        runtime,
        candidate,
        VerificationRequest {
            identity: render_identity(&request.attempt),
            cancellation_token: request.attempt.cancellation_token.clone(),
            limits: limits.0,
        },
        cancelled,
        deadline,
        progress,
    )
    .map_err(|failure| failure.error)
}

/// Historical evidence for record_render_verification. Creating this envelope
/// grants no publication authority and cannot reconstruct its live input.
pub fn verification_observation(
    candidate: &VerifiedCandidate,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<RenderVerificationObservation, EncodedRenderError> {
    candidate.candidate().check_live(cancelled, deadline)?;
    let report = candidate.report();
    let observation = RenderVerificationObservation {
        schema_version: 1,
        validator_id: "deadpan-sdr-finished-file".into(),
        validator_version: format!(
            "{}:policy-{}",
            env!("CARGO_PKG_VERSION"),
            report.policy_version
        ),
        movie_sha256: report.movie_sha256.clone(),
        movie_byte_length: report.movie_bytes,
        report: serde_json::to_value(report)?,
    };
    observation
        .validate()
        .map_err(|error| EncodedRenderError::Protocol(error.to_string()))?;
    candidate.candidate().check_live(cancelled, deadline)?;
    Ok(observation)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedRenderManifest {
    schema_version: u32,
    intent: RenderIntent,
    encoding_attempt_id: AttemptId,
    encoded: EncodedManifest,
}

impl RetainedRenderManifest {
    fn validate(
        &self,
        intent: &RenderIntent,
        encoding_attempt: &AttemptId,
        contract: &ExportPictureContract,
    ) -> Result<(), EncodedRenderError> {
        self.encoded
            .validate()
            .map_err(EncodedRenderError::Protocol)?;
        if self.schema_version != 1
            || &self.intent != intent
            || &self.encoding_attempt_id != encoding_attempt
            || self.encoded.document_sha256 != intent.document_sha256
            || self.encoded.contract
                != EncodedRenderContract::from_contract(contract, encoder_choice(&intent.policy))
        {
            return Err(EncodedRenderError::Protocol("retained manifest differs from the captured intent, encoding attempt or historical contract".into()));
        }
        Ok(())
    }
}

fn reconstruct_contract(
    package: &Path,
    intent: &RenderIntent,
    cancelled: &AtomicBool,
    deadline: Instant,
) -> Result<ExportPictureContract, EncodedRenderError> {
    check_control(cancelled, deadline)?;
    intent
        .validate()
        .map_err(|error| EncodedRenderError::Protocol(error.to_string()))?;
    let pictures = ProjectPictureSession::open_revision(
        package,
        &intent.revision_id,
        Some(intent.range),
        cancelled,
    )?;
    let contract = ExportPictureContract::capture(&pictures)?;
    if contract.project_id() != &intent.project_id
        || document_hash(pictures.document(), cancelled, deadline)? != intent.document_sha256
    {
        return Err(EncodedRenderError::Protocol(
            "render job document identity changed".into(),
        ));
    }
    EncodedRenderContract::from_contract(&contract, encoder_choice(&intent.policy))
        .validate()
        .map_err(EncodedRenderError::Protocol)?;
    check_control(cancelled, deadline)?;
    Ok(contract)
}

fn validate_attempt(
    request: &RenderStageRequest,
    expected: RenderAttemptState,
) -> Result<(), EncodedRenderError> {
    request
        .intent
        .validate()
        .map_err(|error| EncodedRenderError::Protocol(error.to_string()))?;
    if request.attempt.job_id != request.intent.job_id
        || request.attempt.state != expected
        || request.attempt.cancellation_requested
        || !(1..=deadpan_jobs::render::MAX_RENDER_COUNTER)
            .contains(&request.attempt.transition_sequence)
    {
        return Err(EncodedRenderError::Configuration(
            "render stage requires its exact active attempt phase",
        ));
    }
    Ok(())
}

fn render_identity(attempt: &StoredRenderAttempt) -> RenderIdentity {
    RenderIdentity {
        request_id: attempt.job_id.clone(),
        attempt_id: attempt.attempt_id.clone(),
    }
}

fn encoder_choice(policy: &RenderEngineeringPolicy) -> EncoderChoice {
    EncoderChoice {
        mode: match policy.encoder {
            RenderEncoder::Hardware => EncoderMode::Hardware,
            RenderEncoder::Software => EncoderMode::Software,
        },
        b_frames: match policy.b_frames {
            RenderBFrames::None => BFramePolicy::None,
            RenderBFrames::TargetTwo => BFramePolicy::TargetTwo,
        },
    }
}

fn manifest_bytes(manifest: &RetainedRenderManifest) -> Result<Vec<u8>, EncodedRenderError> {
    struct Bounded(Vec<u8>);
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len()
                > usize::try_from(MAX_RENDER_MANIFEST_BYTES)
                    .expect("manifest bound")
                    .saturating_sub(self.0.len())
            {
                return Err(io::Error::new(
                    io::ErrorKind::FileTooLarge,
                    "render manifest byte bound",
                ));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Bounded(Vec::new());
    serde_json::to_writer(&mut writer, manifest)?;
    Ok(writer.0)
}

struct CandidateReader<'a> {
    candidate: &'a mut EncodedCandidate,
    offset: u64,
    cancelled: &'a AtomicBool,
    deadline: Instant,
}

impl Read for CandidateReader<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let length = output.len().min(64 * 1024);
        let read = self
            .candidate
            .read_at(
                self.offset,
                &mut output[..length],
                self.cancelled,
                self.deadline,
            )
            .map_err(io::Error::other)?;
        self.offset = self
            .offset
            .checked_add(u64::try_from(read).expect("bounded read"))
            .ok_or_else(|| io::Error::other("render candidate offset overflow"))?;
        Ok(read)
    }
}
