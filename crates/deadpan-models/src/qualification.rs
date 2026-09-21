use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_core::{GeneratedContentId, GeneratedObjectRef, ProjectId, RevisionId, SourceSpan};
use deadpan_jobs::artifact::{
    ArtifactError, ArtifactLimits, ArtifactWorkspace, SnapshotInterruption,
};
use deadpan_jobs::{
    BridgeGenerationPlan, ContextArtifact, HoldConstraints, HoldTarget, HostMessage,
    MessageIdentity, NativeCandidateManifest, ProviderSelection, Sha256,
};
use deadpan_media::protocol::{
    BRIDGE_PROTOCOL_VERSION, BridgeConversionRequest, BridgeOperation, ConversionLimits,
    ConversionReport, VideoContract,
};
use deadpan_media::{CanonicalMedia, ConversionError, InputIdentity, canonicalize_bridge};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Immutable generation dependencies, excluding process-control configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationBinding {
    pub identity: MessageIdentity,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub target: HoldTarget,
    pub input: ContextArtifact,
    pub constraints: HoldConstraints,
    pub provider: ProviderSelection,
    pub plan: BridgeGenerationPlan,
}

impl GenerationBinding {
    pub fn from_request(request: &HostMessage) -> Result<Self, QualificationError> {
        request
            .validate()
            .map_err(|error| QualificationError::Request(error.to_string()))?;
        let HostMessage::GenerateBridge {
            identity,
            project_id,
            revision_id,
            target,
            input,
            constraints,
            provider,
            plan,
            ..
        } = request
        else {
            return Err(QualificationError::Request(
                "a version-2 bridge request is required".into(),
            ));
        };
        Ok(Self {
            identity: identity.clone(),
            project_id: project_id.clone(),
            revision_id: revision_id.clone(),
            target: target.clone(),
            input: input.clone(),
            constraints: constraints.clone(),
            provider: provider.as_ref().clone(),
            plan: plan.as_ref().clone(),
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationLimits {
    pub media: ConversionLimits,
    pub maximum_worker_provenance_bytes: u64,
    pub maximum_host_provenance_bytes: u64,
}

impl QualificationLimits {
    fn validate(self) -> Result<(), QualificationError> {
        self.media.validate().map_err(ConversionError::from)?;
        if self.maximum_worker_provenance_bytes == 0
            || self.maximum_worker_provenance_bytes > 4 * 1024 * 1024
            || self.maximum_host_provenance_bytes == 0
            || self.maximum_host_provenance_bytes > 32 * 1024 * 1024
        {
            return Err(QualificationError::Provenance(
                "provenance limits are out of bounds".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum QualificationError {
    #[error("invalid bridge qualification request: {0}")]
    Request(String),
    #[error("invalid bridge provenance: {0}")]
    Provenance(String),
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
    #[error(transparent)]
    Media(#[from] ConversionError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("bridge qualification was cancelled")]
    Cancelled,
    #[error("bridge qualification exceeded its deadline")]
    Deadline,
}

/// Read-only bytes of the host's provenance envelope, including the exact
/// original worker report. No mutable backing buffer or file descriptor escapes.
pub struct QualifiedProvenance {
    bytes: Cursor<Vec<u8>>,
    object: GeneratedObjectRef,
}

impl QualifiedProvenance {
    pub fn object(&self) -> &GeneratedObjectRef {
        &self.object
    }
}

impl Read for QualifiedProvenance {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.bytes.read(bytes)
    }
}

impl Seek for QualifiedProvenance {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.bytes.seek(position)
    }
}

/// A fully materialized media/provenance bundle. This does not establish visual
/// continuity, selected-Ready persistence, audition, or authored acceptance.
pub struct QualifiedBridgeBundle {
    binding: GenerationBinding,
    declaration: NativeCandidateManifest,
    native: CanonicalMedia,
    sampled: CanonicalMedia,
    provenance: QualifiedProvenance,
    conditioning: crate::RetainedConditioning,
    native_span: SourceSpan,
    sampled_span: SourceSpan,
}

impl QualifiedBridgeBundle {
    pub fn binding(&self) -> &GenerationBinding {
        &self.binding
    }
    pub fn declaration(&self) -> &NativeCandidateManifest {
        &self.declaration
    }
    pub fn native(&self) -> &CanonicalMedia {
        &self.native
    }
    pub fn sampled(&self) -> &CanonicalMedia {
        &self.sampled
    }
    pub fn provenance(&self) -> &QualifiedProvenance {
        &self.provenance
    }
    pub fn conditioning(&self) -> &crate::RetainedConditioning {
        &self.conditioning
    }
    pub fn native_span(&self) -> SourceSpan {
        self.native_span
    }
    pub fn sampled_span(&self) -> SourceSpan {
        self.sampled_span
    }

    pub fn into_parts(
        self,
    ) -> (
        CanonicalMedia,
        CanonicalMedia,
        QualifiedProvenance,
        crate::RetainedConditioning,
    ) {
        (
            self.native,
            self.sampled,
            self.provenance,
            self.conditioning,
        )
    }
}

#[derive(Serialize)]
struct HostProvenance<'a> {
    schema_version: u32,
    validation_profile: &'static str,
    binding: &'a GenerationBinding,
    selected_provider: &'a crate::SelectedBridgeProvider,
    declaration: &'a NativeCandidateManifest,
    native: &'a GeneratedObjectRef,
    sampled: &'a GeneratedObjectRef,
    native_validation: &'a ConversionReport,
    sampled_validation: &'a ConversionReport,
    conditioning: &'a crate::ConditioningReceipt,
    native_span: SourceSpan,
    sampled_span: SourceSpan,
    // A string preserves original bytes exactly, including formatting. Backend
    // claims are retained as provenance, never used as media validation results.
    worker_provenance_utf8: &'a str,
}

/// Worker declarations paired with independently selected host inputs.
pub struct BridgeQualification<'a> {
    pub request: &'a HostMessage,
    pub declaration: &'a NativeCandidateManifest,
    pub selected_provider: &'a crate::SelectedBridgeProvider,
    pub conditioning: crate::RetainedConditioning,
}

/// Qualify a modern worker result after clean process-group teardown. Pin the
/// workspace before launching that worker. The caller supplies the immutable
/// request reconstructed from persisted intent, never a worker-chosen plan.
/// Supply the capability from the host's selected provider, never from the worker.
/// Capture conditioning before launching the worker and retain it separately from
/// its writable workspace. Qualification never rereads worker-controlled inputs.
/// Run on the background job service, outside UI/audio or database transactions.
pub fn qualify_bridge(
    executable: &Path,
    workspace: &ArtifactWorkspace,
    inputs: BridgeQualification<'_>,
    limits: QualificationLimits,
    cancelled: &AtomicBool,
) -> Result<QualifiedBridgeBundle, QualificationError> {
    let BridgeQualification {
        request,
        declaration,
        selected_provider,
        conditioning,
    } = inputs;
    limits.validate()?;
    let deadline = Instant::now() + Duration::from_millis(limits.media.timeout_ms);
    let check = || {
        if cancelled.load(Ordering::Acquire) {
            Err(QualificationError::Cancelled)
        } else if Instant::now() >= deadline {
            Err(QualificationError::Deadline)
        } else {
            Ok(())
        }
    };
    let snapshot_control = || {
        if cancelled.load(Ordering::Acquire) {
            Err(SnapshotInterruption::Cancelled)
        } else if Instant::now() >= deadline {
            Err(SnapshotInterruption::Deadline)
        } else {
            Ok(())
        }
    };
    check()?;
    let binding = GenerationBinding::from_request(request)?;
    if selected_provider.selection() != &binding.provider {
        return Err(QualificationError::Request(
            "host provider selection differs from request".into(),
        ));
    }
    binding
        .plan
        .validate_for(selected_provider.capability())
        .map_err(|error| QualificationError::Request(error.to_string()))?;
    conditioning.validate_for(request)?;
    let HostMessage::GenerateBridge {
        output_workspace, ..
    } = request
    else {
        unreachable!()
    };
    let dimensions = binding.plan.native_dimensions();
    if declaration.provider != binding.provider
        || declaration.video.frames().frames() != i64::from(binding.plan.native_frame_count())
        || declaration.video.frame_rate() != binding.plan.native_frame_rate()
        || declaration.video.width() != dimensions.width()
        || declaration.video.height() != dimensions.height()
        || declaration.native.reference() == declaration.provenance.reference()
    {
        return Err(QualificationError::Request(
            "native declaration differs from the original plan/provider".into(),
        ));
    }
    // Validate provenance before doing expensive decode work.
    let mut provenance_snapshot = workspace
        .snapshot_with_control(
            output_workspace,
            &declaration.provenance,
            ArtifactLimits::new(limits.maximum_worker_provenance_bytes)?,
            snapshot_control,
        )
        .map_err(snapshot_error)?;
    check()?;
    let mut provenance_bytes = Vec::new();
    provenance_snapshot.read_to_end(&mut provenance_bytes)?;
    let report = crate::strict_json::parse(&provenance_bytes)?;
    crate::provenance::validate(report, &binding, declaration, conditioning.context())?;
    check()?;
    let mut native_snapshot = workspace
        .snapshot_with_control(
            output_workspace,
            &declaration.native,
            ArtifactLimits::new(limits.media.max_input_bytes)?,
            snapshot_control,
        )
        .map_err(snapshot_error)?;
    check()?;
    let remaining_ms = u64::try_from(
        deadline
            .saturating_duration_since(Instant::now())
            .as_millis(),
    )
    .map_err(|_| QualificationError::Deadline)?;
    if remaining_ms == 0 {
        return Err(QualificationError::Deadline);
    }
    let conversion = BridgeConversionRequest {
        protocol: BRIDGE_PROTOCOL_VERSION,
        operation: BridgeOperation::SampleBridge,
        native: VideoContract {
            width: dimensions.width(),
            height: dimensions.height(),
            frames: binding.plan.native_frame_count(),
            rate_num: binding.plan.native_frame_rate().numerator(),
            rate_den: binding.plan.native_frame_rate().denominator(),
        },
        sampling: binding
            .plan
            .sampling_map()
            .map_err(|error| QualificationError::Request(error.to_string()))?,
        input_byte_length: declaration.native.byte_length(),
        limits: ConversionLimits {
            timeout_ms: remaining_ms,
            ..limits.media
        },
    };
    let masters = canonicalize_bridge(
        executable,
        &mut native_snapshot,
        input_identity(declaration.native.sha256()),
        &conversion,
        cancelled,
    )?;
    check()?;
    let worker_provenance_utf8 = std::str::from_utf8(&provenance_bytes)
        .map_err(|error| QualificationError::Provenance(error.to_string()))?;
    let native_span = masters
        .native()
        .report()
        .output_span()
        .map_err(ConversionError::from)?;
    let sampled_span = masters
        .sampled()
        .report()
        .output_span()
        .map_err(ConversionError::from)?;
    let bytes = crate::bounded_json::encode(
        &HostProvenance {
            schema_version: 3,
            validation_profile: "deadpan-ffv1-bridge-3",
            binding: &binding,
            selected_provider,
            declaration,
            native: masters.native().object(),
            sampled: masters.sampled().object(),
            native_validation: masters.native().report(),
            sampled_validation: masters.sampled().report(),
            conditioning: conditioning.receipt(),
            native_span,
            sampled_span,
            worker_provenance_utf8,
        },
        limits.maximum_host_provenance_bytes,
    )
    .map_err(|error| QualificationError::Provenance(error.to_string()))?;
    let object = GeneratedObjectRef::new(
        GeneratedContentId::new(blake3::hash(&bytes).to_hex().to_string())
            .map_err(|error| QualificationError::Provenance(error.to_string()))?,
        bytes.len() as u64,
    )
    .map_err(|error| QualificationError::Provenance(error.to_string()))?;
    check()?;
    let (native, sampled, _) = masters.into_parts();
    Ok(QualifiedBridgeBundle {
        binding,
        declaration: declaration.clone(),
        native,
        sampled,
        provenance: QualifiedProvenance {
            bytes: Cursor::new(bytes),
            object,
        },
        conditioning,
        native_span,
        sampled_span,
    })
}

fn input_identity(hash: &Sha256) -> InputIdentity {
    let mut sha256 = [0; 32];
    for (index, byte) in sha256.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hash.as_str()[index * 2..index * 2 + 2], 16)
            .expect("Sha256 guarantees 64 lowercase hexadecimal digits");
    }
    InputIdentity { sha256 }
}

fn snapshot_error(error: ArtifactError) -> QualificationError {
    match error {
        ArtifactError::Interrupted(SnapshotInterruption::Cancelled) => {
            QualificationError::Cancelled
        }
        ArtifactError::Interrupted(SnapshotInterruption::Deadline) => QualificationError::Deadline,
        other => QualificationError::Artifact(other),
    }
}
