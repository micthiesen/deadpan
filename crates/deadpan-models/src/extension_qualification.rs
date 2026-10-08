//! Host admission of a complete one-sided candidate, before store publication.

use std::io::{Read, Seek};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_core::SourceSpan;
use deadpan_jobs::artifact::{ArtifactLimits, ArtifactWorkspace, SnapshotInterruption};
use deadpan_jobs::{CandidateDeclaration, HostMessage, NativeCandidateManifest};
use deadpan_media::protocol::{
    ConversionLimits, EXTENSION_PROTOCOL_VERSION, ExtensionConversionRequest, ExtensionOperation,
};
use deadpan_media::{CanonicalMedia, ConversionError, canonicalize_extension};

use crate::quality_input::Control;
use crate::stored_extension::{ExtensionEnvelope, PROFILE, SCHEMA};
use crate::{
    ExtensionGenerationBinding, QualificationError, QualificationLimits, QualifiedProvenance,
    RetainedExtensionConditioning, SelectedExtensionProvider, StoredExtensionProvenance,
};

/// Independently selected inputs and the exact V3 terminal declaration. Capture
/// conditioning before launching the worker and retain it outside its workspace.
pub struct ExtensionQualification<'a> {
    pub request: &'a HostMessage,
    pub declaration: &'a CandidateDeclaration,
    pub selected_provider: &'a SelectedExtensionProvider,
    pub conditioning: RetainedExtensionConditioning,
}

/// Private immutable media, inputs and fully bound evidence. Publication, Ready
/// selection, audition and explicit authored acceptance are separate operations.
pub struct QualifiedExtensionBundle {
    binding: ExtensionGenerationBinding,
    declaration: NativeCandidateManifest,
    native: CanonicalMedia,
    sampled: CanonicalMedia,
    provenance: QualifiedProvenance,
    conditioning: RetainedExtensionConditioning,
    native_span: SourceSpan,
    sampled_span: SourceSpan,
}

impl QualifiedExtensionBundle {
    pub fn binding(&self) -> &ExtensionGenerationBinding {
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
    pub fn conditioning(&self) -> &RetainedExtensionConditioning {
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
        RetainedExtensionConditioning,
    ) {
        (
            self.native,
            self.sampled,
            self.provenance,
            self.conditioning,
        )
    }
}

/// Run after confirmed generation-worker group teardown. The caller owns that
/// process and must supply the original persisted request and host capability.
/// One deadline covers snapshots, conversion, pixel and Vision checks, and the
/// retained-envelope round trip. Run outside UI/audio/database transactions.
pub fn qualify_extension(
    executable: &Path,
    landmark_executable: &Path,
    workspace: &ArtifactWorkspace,
    inputs: ExtensionQualification<'_>,
    limits: QualificationLimits,
    cancelled: &AtomicBool,
) -> Result<QualifiedExtensionBundle, QualificationError> {
    limits.validate()?;
    let control = Control {
        deadline: Instant::now() + Duration::from_millis(limits.media.timeout_ms),
        cancelled,
    };
    control.remaining()?;
    let ExtensionQualification {
        request,
        declaration,
        selected_provider,
        mut conditioning,
    } = inputs;
    let CandidateDeclaration::NativeExtensionV3(declaration) = declaration else {
        return Err(QualificationError::Request(
            "extension qualification requires a NativeExtensionV3 declaration".into(),
        ));
    };
    let binding = ExtensionGenerationBinding::from_request(request)?;
    binding.validate_for(selected_provider, declaration)?;
    conditioning.validate_for(request)?;
    let HostMessage::GenerateExtension {
        output_workspace, ..
    } = request
    else {
        unreachable!()
    };
    let snapshot_control = || {
        if cancelled.load(Ordering::Acquire) {
            Err(SnapshotInterruption::Cancelled)
        } else if Instant::now() >= control.deadline {
            Err(SnapshotInterruption::Deadline)
        } else {
            Ok(())
        }
    };
    // Reject contradictory metadata before any expensive output decode.
    let mut snapshot = workspace
        .snapshot_with_control(
            output_workspace,
            &declaration.provenance,
            ArtifactLimits::new(limits.maximum_worker_provenance_bytes)?,
            snapshot_control,
        )
        .map_err(crate::qualification::snapshot_error)?;
    let mut worker_bytes = Vec::new();
    snapshot.read_to_end(&mut worker_bytes)?;
    control.remaining()?;
    crate::provenance::validate_extension(
        crate::strict_json::parse(&worker_bytes)?,
        &binding,
        declaration,
        conditioning.context(),
    )?;
    control.remaining()?;
    let mut native_input = workspace
        .snapshot_with_control(
            output_workspace,
            &declaration.native,
            ArtifactLimits::new(limits.media.max_input_bytes)?,
            snapshot_control,
        )
        .map_err(crate::qualification::snapshot_error)?;
    let remaining_ms = u64::try_from(control.remaining()?.as_millis())
        .map_err(|_| QualificationError::Deadline)?;
    if remaining_ms == 0 {
        return Err(QualificationError::Deadline);
    }
    let media = canonicalize_extension(
        executable,
        &mut native_input,
        crate::qualification::input_identity(declaration.native.sha256()),
        &ExtensionConversionRequest {
            protocol: EXTENSION_PROTOCOL_VERSION,
            operation: ExtensionOperation::SampleExtension,
            native: crate::extension_motion::native_contract(&binding.plan),
            sampling: binding.plan.sampling_map().clone(),
            input_byte_length: declaration.native.byte_length(),
            limits: ConversionLimits {
                timeout_ms: remaining_ms,
                ..limits.media
            },
        },
        cancelled,
    )?;
    control.remaining()?;
    let pixels = crate::inspect_extension_pixels(
        &media,
        &mut conditioning,
        request,
        control.deadline,
        cancelled,
    )?;
    let (mut native, mut sampled, _) = media.into_parts();
    let geometry = crate::inspect_extension_geometry(
        landmark_executable,
        &mut native,
        &mut conditioning,
        request,
        control.deadline,
        cancelled,
    )?;
    control.remaining()?;
    let native_span = native
        .report()
        .output_span()
        .map_err(ConversionError::from)?;
    let sampled_span = sampled
        .report()
        .output_span()
        .map_err(ConversionError::from)?;
    let envelope = ExtensionEnvelope {
        schema_version: SCHEMA,
        validation_profile: PROFILE.into(),
        binding: binding.clone(),
        selected_provider: selected_provider.clone(),
        declaration: declaration.clone(),
        native: native.object().clone(),
        sampled: sampled.object().clone(),
        native_validation: native.report().clone(),
        sampled_validation: sampled.report().clone(),
        conditioning: conditioning.receipt().clone(),
        native_span,
        sampled_span,
        worker_provenance_utf8: String::from_utf8(worker_bytes)
            .map_err(|error| QualificationError::Provenance(error.to_string()))?,
        pixels,
        geometry,
    };
    let bytes = crate::bounded_json::encode(&envelope, limits.maximum_host_provenance_bytes)
        .map_err(|error| QualificationError::Provenance(error.to_string()))?;
    let stored_object = QualifiedProvenance::from_bytes(bytes.clone())?;
    // Reopening exercises the same bounded reader as later persisted consumers.
    // Use exact retained manifest bytes, never a reserialized or worker copy.
    let manifest = conditioning.manifest_mut();
    manifest.rewind()?;
    let mut context_bytes = Vec::new();
    let read = manifest.take(1_048_577).read_to_end(&mut context_bytes);
    conditioning.manifest_mut().rewind()?;
    read?;
    control.remaining()?;
    StoredExtensionProvenance::from_bytes(&bytes, stored_object.object())?
        .validate_for(&binding, &context_bytes)?;
    control.remaining()?;
    native.rewind()?;
    sampled.rewind()?;
    Ok(QualifiedExtensionBundle {
        binding,
        declaration: declaration.clone(),
        native,
        sampled,
        provenance: stored_object,
        conditioning,
        native_span,
        sampled_span,
    })
}
