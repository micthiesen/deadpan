//! Worker stages for durable engineering and automatic renders. The caller owns short store
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
        RenderAttemptIdentity, RenderAttemptState, RenderAutomaticAlgorithm, RenderBFrames,
        RenderEncoder, RenderEngineeringPolicy, RenderIntent, RenderPolicy,
        RenderVerificationObservation, admission::RenderEncodingDecision,
        deserialize_render_intent_v1,
    },
};
use deadpan_store::{
    AccessMode, ProjectStore,
    render_jobs::{StoredRenderAttempt, StoredRenderCheckpoint},
    render_media::{
        MAX_RENDER_MANIFEST_BYTES, PreparedRenderRetention, RenderMediaLimits, RenderReadHandle,
        RenderWriteHandle,
    },
};
use serde::{Deserialize, Serialize};

use super::{
    EncodedCandidate, EncodedProgress, EncodedRenderError, EncodedWorkerLimits,
    admission::{
        self, AdmissionFailure, AdmissionLimits, AdmissionRequest, QualifiedEncoder, durable,
    },
    check_control,
    host::encode_guarded,
    protocol::{EncodedManifest, EncodedRenderContract, EncoderChoice},
    runtime::EncodingBinding,
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

#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub struct CaptureRenderIntent {
    pub package: PathBuf,
    pub revision: RevisionId,
    pub range: Option<FrameRange>,
    pub job_id: RequestId,
    pub policy: RenderPolicy,
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
    validate_output(&contract, &request.policy)?;
    let intent = RenderIntent {
        schema_version: if request.policy.is_automatic() { 2 } else { 1 },
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

/// The stage worker retains this live capability until the owner has committed
/// its exact decision and the next transition. It cannot be serialized/restored.
pub(super) struct PreparedEncoding {
    qualified: QualifiedEncoder,
    decision: RenderEncodingDecision,
    queued: RenderAttemptIdentity,
}

impl PreparedEncoding {
    pub(super) fn decision(&self) -> &RenderEncodingDecision {
        &self.decision
    }
}

pub(super) struct QualificationFailure {
    pub error: EncodedRenderError,
    pub decision: Option<Box<RenderEncodingDecision>>,
}

impl From<EncodedRenderError> for QualificationFailure {
    fn from(error: EncodedRenderError) -> Self {
        Self {
            error,
            decision: None,
        }
    }
}

pub(super) fn qualify_for_attempt(
    runtime: &RenderWorkerRuntime,
    request: &RenderStageRequest,
    writer: &RenderWriteHandle,
    cancelled: &AtomicBool,
    deadline: Instant,
    progress: impl FnMut(EncoderChoice, u64, u64),
) -> Result<PreparedEncoding, QualificationFailure> {
    validate_attempt(request, RenderAttemptState::Queued)?;
    if !request.intent.policy.is_automatic() || request.attempt.checkpoint_attempt_id.is_some() {
        return Err(EncodedRenderError::Configuration(
            "qualification requires a fresh automatic attempt without a checkpoint",
        )
        .into());
    }
    writer
        .check_live(cancelled)
        .map_err(EncodedRenderError::from)?;
    let contract = reconstruct_contract(&request.package, &request.intent, cancelled, deadline)?;
    let rate = contract.frame_rate();
    let qualified = admission::qualify_guarded(
        runtime,
        AdmissionRequest {
            identity: render_identity(&request.attempt),
            cancellation_token: request.attempt.cancellation_token.clone(),
            raster: contract.raster(),
            frame_rate: [rate.numerator(), rate.denominator()],
            color_policy: contract.color_policy(),
        },
        AdmissionLimits::default(),
        cancelled,
        deadline,
        progress,
        || writer.check_live(cancelled).map_err(EncodedRenderError::from),
    ).map_err(|failure: AdmissionFailure| {
        let decision = if failure.error.cleanup_confirmed() {
            match durable::from_failure(&request.intent, &request.attempt.attempt_id, &contract, &failure) {
                Ok(decision) => Some(Box::new(decision)),
                Err(error) => return QualificationFailure {
                    error: EncodedRenderError::Protocol(format!(
                        "qualification failed: {}; its durable observation was invalid: {error}",
                        failure.error,
                    )),
                    decision: None,
                },
            }
        } else {
            None
        };
        QualificationFailure { error: failure.error, decision }
    })?;
    writer
        .check_live(cancelled)
        .map_err(EncodedRenderError::from)?;
    let decision = durable::from_qualified(
        &request.intent,
        &request.attempt.attempt_id,
        &contract,
        &qualified,
    )?;
    Ok(PreparedEncoding {
        qualified,
        decision,
        queued: request.attempt.identity(),
    })
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
    encode_with_admission_and_retain(
        runtime, request, writer, None, limits, cancelled, deadline, progress,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn encode_with_admission_and_retain(
    runtime: &RenderWorkerRuntime,
    request: &RenderStageRequest,
    writer: &RenderWriteHandle,
    prepared: Option<PreparedEncoding>,
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
    let picture_request = RenderPictureRequest {
        package: request.package.clone(),
        revision: request.intent.revision_id.clone(),
        range: Some(request.intent.range),
        identity: render_identity(&request.attempt),
        cancellation_token: request.attempt.cancellation_token.clone(),
    };
    let (mut candidate, decision) = match (&request.intent.policy, prepared) {
        (RenderPolicy::Engineering(policy), None) => (
            encode_guarded(
                runtime,
                picture_request,
                encoder_choice(policy),
                limits.0,
                cancelled,
                deadline,
                progress,
                || {
                    writer
                        .check_live(cancelled)
                        .map_err(EncodedRenderError::from)
                },
            )?,
            None,
        ),
        (RenderPolicy::Automatic(_), Some(prepared)) => {
            let current = request.attempt.identity();
            if prepared.queued.job_id != current.job_id
                || prepared.queued.attempt_id != current.attempt_id
                || prepared.queued.cancellation_token != current.cancellation_token
                || prepared.queued.expected_sequence.checked_add(1)
                    != Some(current.expected_sequence)
            {
                return Err(EncodedRenderError::Configuration(
                    "fresh qualification belongs to another committed encoding transition",
                ));
            }
            let binding = durable::binding_for_decision(
                &request.intent,
                &current.attempt_id,
                &expected,
                &prepared.decision,
            )?;
            let automatic = prepared.qualified.encode_guarded(
                runtime,
                picture_request,
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
            let (candidate, _) = automatic.into_parts();
            if candidate.encoding_binding() != Some(&binding) {
                return Err(EncodedRenderError::Protocol(
                    "automatic encode changed its committed runtime decision".into(),
                ));
            }
            (
                candidate.with_encoding_provenance(binding, prepared.decision.clone()),
                Some(prepared.decision),
            )
        }
        _ => {
            return Err(EncodedRenderError::Configuration(
                "automatic encoding requires its exact live queued qualification",
            ));
        }
    };
    writer.check_live(cancelled)?;
    let manifest = RetainedRenderManifest::capture(request, &candidate, decision.clone())?;
    manifest.validate(
        &request.intent,
        &request.attempt.attempt_id,
        &expected,
        decision.as_ref(),
    )?;
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
    let store = ProjectStore::open(&request.package, AccessMode::ReadOnly)
        .map_err(|error| EncodedRenderError::Protocol(error.to_string()))?;
    let decision = store
        .render_encoding_decision(&request.intent.job_id, &checkpoint.encoding_attempt_id)
        .map_err(|error| EncodedRenderError::Protocol(error.to_string()))?;
    drop(store);
    check_control(cancelled, deadline)?;
    let snapshot = reader.snapshot(&checkpoint.media, limits.1, cancelled, deadline)?;
    let manifest: RetainedRenderManifest = serde_json::from_slice(snapshot.manifest_bytes())?;
    let contract = reconstruct_contract(&request.package, &request.intent, cancelled, deadline)?;
    let binding = manifest.validate(
        &request.intent,
        &checkpoint.encoding_attempt_id,
        &contract,
        decision.as_ref(),
    )?;
    let mut candidate = EncodedCandidate::from_retained(
        contract,
        request.intent.document_sha256.clone(),
        manifest.into_encoded(),
        snapshot,
        cancelled,
        deadline,
    )?;
    if let (Some(binding), Some(decision)) = (binding, decision) {
        candidate = candidate.with_encoding_provenance(binding, decision);
    }
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
#[serde(untagged)]
enum RetainedRenderManifest {
    V1(Box<RetainedRenderManifestV1>),
    V2(Box<RetainedRenderManifestV2>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedRenderManifestV1 {
    schema_version: u32,
    #[serde(deserialize_with = "deserialize_render_intent_v1")]
    intent: RenderIntent,
    encoding_attempt_id: AttemptId,
    encoded: EncodedManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedRenderManifestV2 {
    schema_version: u32,
    intent: RenderIntent,
    encoding_attempt_id: AttemptId,
    encoded: EncodedManifest,
    encoding_decision: RenderEncodingDecision,
    encoding_binding: EncodingBinding,
}

impl RetainedRenderManifest {
    fn capture(
        request: &RenderStageRequest,
        candidate: &EncodedCandidate,
        decision: Option<RenderEncodingDecision>,
    ) -> Result<Self, EncodedRenderError> {
        match (
            &request.intent.policy,
            decision,
            candidate.encoding_binding(),
        ) {
            (RenderPolicy::Engineering(_), None, None) => {
                Ok(Self::V1(Box::new(RetainedRenderManifestV1 {
                    schema_version: 1,
                    intent: request.intent.clone(),
                    encoding_attempt_id: request.attempt.attempt_id.clone(),
                    encoded: candidate.manifest().clone(),
                })))
            }
            (RenderPolicy::Automatic(_), Some(decision), Some(binding)) => {
                Ok(Self::V2(Box::new(RetainedRenderManifestV2 {
                    schema_version: 2,
                    intent: request.intent.clone(),
                    encoding_attempt_id: request.attempt.attempt_id.clone(),
                    encoded: candidate.manifest().clone(),
                    encoding_decision: decision,
                    encoding_binding: binding.clone(),
                })))
            }
            _ => Err(EncodedRenderError::Protocol(
                "retained manifest policy and live encoding provenance differ".into(),
            )),
        }
    }

    fn into_encoded(self) -> EncodedManifest {
        match self {
            Self::V1(value) => value.encoded,
            Self::V2(value) => value.encoded,
        }
    }

    fn validate(
        &self,
        intent: &RenderIntent,
        encoding_attempt: &AttemptId,
        contract: &ExportPictureContract,
        decision: Option<&RenderEncodingDecision>,
    ) -> Result<Option<EncodingBinding>, EncodedRenderError> {
        let (recorded_intent, recorded_attempt, encoded, choice, binding) = match self {
            Self::V1(value) => {
                let policy = intent.policy.engineering().ok_or_else(|| {
                    EncodedRenderError::Protocol(
                        "legacy manifest cannot contain automatic intent".into(),
                    )
                })?;
                if value.schema_version != 1 || decision.is_some() {
                    return Err(EncodedRenderError::Protocol(
                        "legacy manifest cannot contain an automatic decision".into(),
                    ));
                }
                (
                    &value.intent,
                    &value.encoding_attempt_id,
                    &value.encoded,
                    encoder_choice(policy),
                    None,
                )
            }
            Self::V2(value) => {
                if value.schema_version != 2
                    || !intent.policy.is_automatic()
                    || decision != Some(&value.encoding_decision)
                {
                    return Err(EncodedRenderError::Protocol(
                        "automatic manifest differs from its exact stored encoding decision".into(),
                    ));
                }
                let binding = durable::binding_for_decision(
                    intent,
                    encoding_attempt,
                    contract,
                    &value.encoding_decision,
                )?;
                if value.encoding_binding != binding {
                    return Err(EncodedRenderError::Protocol(
                        "automatic manifest completion runtime differs from its decision".into(),
                    ));
                }
                (
                    &value.intent,
                    &value.encoding_attempt_id,
                    &value.encoded,
                    binding.choice,
                    Some(binding),
                )
            }
        };
        encoded
            .validate_retained()
            .map_err(EncodedRenderError::Protocol)?;
        if recorded_intent != intent
            || recorded_attempt != encoding_attempt
            || encoded.document_sha256 != intent.document_sha256
            || encoded.contract != EncodedRenderContract::from_contract(contract, choice)
        {
            return Err(EncodedRenderError::Protocol("retained manifest differs from the captured intent, encoding attempt or historical contract".into()));
        }
        Ok(binding)
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
    validate_output(&contract, &intent.policy)?;
    check_control(cancelled, deadline)?;
    Ok(contract)
}

fn validate_output(
    contract: &ExportPictureContract,
    policy: &RenderPolicy,
) -> Result<(), EncodedRenderError> {
    // An automatic intent pins the algorithm of the committed color branch:
    // AutomaticSdrV1 for SDR (including a tone-mapped HDR-basis fallback),
    // AutomaticHdrV1 for PQ/HLG output.
    if let Some(automatic) = policy.automatic()
        && !automatic.algorithm.admits_output(contract.color_policy())
    {
        return Err(EncodedRenderError::Protocol(format!(
            "automatic algorithm {} does not match the committed {:?} output branch (expected {})",
            automatic.algorithm.as_str(),
            contract.color_policy(),
            RenderAutomaticAlgorithm::for_output(contract.color_policy()).as_str(),
        )));
    }
    // This checks geometry/clocks only. Automatic selection still requires a
    // fresh probe; this temporary choice is never persisted as a decision.
    let choice = policy
        .engineering()
        .map(encoder_choice)
        .unwrap_or(EncoderChoice {
            mode: EncoderMode::Hardware,
            b_frames: BFramePolicy::None,
        });
    EncodedRenderContract::from_contract(contract, choice)
        .validate()
        .map_err(EncodedRenderError::Protocol)
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

pub(super) fn encoder_choice(policy: &RenderEngineeringPolicy) -> EncoderChoice {
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
