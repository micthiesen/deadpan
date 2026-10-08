//! Revalidate complete extension evidence without an installed model or worker.

use deadpan_core::{
    GeneratedArtifact, GeneratedObjectRef, GeneratedSamplingMap, ProjectId, SourceSpan,
};
use deadpan_jobs::NativeCandidateManifest;
use deadpan_media::protocol::{ConversionReport, VideoContract};
use serde::{Deserialize, Serialize};

use crate::stored_bridge::{verify_declaration, verify_object};
use crate::{
    ExtensionConditioningReceipt, ExtensionContext, ExtensionGenerationBinding,
    ExtensionGeometryChecks, ExtensionPixelReport, QualificationError, SelectedExtensionProvider,
};

pub(crate) const SCHEMA: u32 = 1;
pub(crate) const PROFILE: &str = "deadpan-ffv1-extension-1";
const MAXIMUM_HOST_BYTES: usize = 32 * 1024 * 1024;
const MAXIMUM_WORKER_BYTES: usize = 4 * 1024 * 1024;
const MAXIMUM_CONTEXT_BYTES: usize = 1024 * 1024;

/// Every report is required. Extension has no historical profile that can omit
/// rejection checks. Original worker diagnostics remain exact UTF-8 bytes.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExtensionEnvelope {
    pub(crate) schema_version: u32,
    pub(crate) validation_profile: String,
    pub(crate) binding: ExtensionGenerationBinding,
    pub(crate) selected_provider: SelectedExtensionProvider,
    pub(crate) declaration: NativeCandidateManifest,
    pub(crate) native: GeneratedObjectRef,
    pub(crate) sampled: GeneratedObjectRef,
    pub(crate) native_validation: ConversionReport,
    pub(crate) sampled_validation: ConversionReport,
    pub(crate) conditioning: ExtensionConditioningReceipt,
    pub(crate) native_span: SourceSpan,
    pub(crate) sampled_span: SourceSpan,
    pub(crate) worker_provenance_utf8: String,
    pub(crate) pixels: ExtensionPixelReport,
    pub(crate) geometry: ExtensionGeometryChecks,
}

/// An object-verified envelope. `validate_for` must bind it to the original
/// request and exact retained manifest before its observations are trusted.
pub struct StoredExtensionProvenance {
    object: GeneratedObjectRef,
    envelope: ExtensionEnvelope,
}

impl StoredExtensionProvenance {
    pub fn from_bytes(
        bytes: &[u8],
        expected: &GeneratedObjectRef,
    ) -> Result<Self, QualificationError> {
        verify_object(bytes, expected, MAXIMUM_HOST_BYTES)?;
        let envelope: ExtensionEnvelope =
            serde_json::from_value(crate::strict_json::parse(bytes)?)?;
        envelope.validate()?;
        Ok(Self {
            object: expected.clone(),
            envelope,
        })
    }

    pub fn context_object(&self) -> &GeneratedObjectRef {
        self.envelope.conditioning.manifest().object()
    }

    /// Captured request facts for comparing this envelope with an independent
    /// stored request before validating its retained context and reports.
    pub fn binding(&self) -> &ExtensionGenerationBinding {
        &self.envelope.binding
    }

    /// Admit the saved evidence for an accepted artifact in this project.
    /// The original revision, Hold and full sampling plan remain immutable
    /// provenance. The current Hold can be copied or shortened independently.
    /// Media/input bytes still need independent snapshot and decoder admission.
    pub fn validate_artifact(
        self,
        artifact: &GeneratedArtifact,
        project: &ProjectId,
        context_bytes: &[u8],
    ) -> Result<AcceptedExtensionEvidence, QualificationError> {
        let GeneratedSamplingMap::Extension(sampling) = &artifact.sampling else {
            return Err(invalid("retained extension cannot admit a bridge artifact"));
        };
        let envelope = &self.envelope;
        if &envelope.binding.project_id != project
            || self.object != artifact.provenance
            || envelope.native != artifact.native_object
            || envelope.sampled != artifact.sampled_object
            || envelope.binding.plan.sampling_map() != sampling
        {
            return Err(invalid(
                "retained extension differs from the authored artifact/project",
            ));
        }
        let binding = envelope.binding.clone();
        let evidence = self.validate_for(&binding, context_bytes)?;
        Ok(AcceptedExtensionEvidence { evidence })
    }

    /// Verify exact saved inputs and recompute rejection policy. This does not
    /// open stored paths, rerun inference/detection, or admit media bytes. A
    /// consumer must independently snapshot all returned media/input identities
    /// and decode the media against the returned contracts before using it.
    pub fn validate_for(
        self,
        binding: &ExtensionGenerationBinding,
        context_bytes: &[u8],
    ) -> Result<ValidatedExtensionEvidence, QualificationError> {
        let envelope = &self.envelope;
        if &envelope.binding != binding {
            return Err(invalid(
                "stored extension differs from the captured generation binding",
            ));
        }
        verify_object(context_bytes, self.context_object(), MAXIMUM_CONTEXT_BYTES)?;
        verify_declaration(
            context_bytes,
            envelope.conditioning.manifest().declaration(),
        )?;
        let context: ExtensionContext =
            serde_json::from_value(crate::strict_json::parse(context_bytes)?)?;
        envelope.conditioning.validate_binding(&context, binding)?;
        envelope.pixels.validate_bound(
            (&envelope.native_validation.video, &envelope.native),
            (&envelope.sampled_validation.video, &envelope.sampled),
            &context,
            &envelope.conditioning,
            binding,
        )?;
        envelope.geometry.validate_bound(
            &envelope.native_validation.video,
            &envelope.native,
            &context,
            &envelope.conditioning,
            binding,
        )?;
        crate::provenance::validate_extension(
            crate::strict_json::parse(envelope.worker_provenance_utf8.as_bytes())?,
            binding,
            &envelope.declaration,
            &context,
        )?;
        Ok(ValidatedExtensionEvidence {
            provenance: self,
            context,
        })
    }
}

impl ExtensionEnvelope {
    fn validate(&self) -> Result<(), QualificationError> {
        if self.schema_version != SCHEMA || self.validation_profile != PROFILE {
            return Err(invalid(
                "unsupported stored extension provenance profile/schema",
            ));
        }
        self.binding
            .validate_for(&self.selected_provider, &self.declaration)?;
        let native = crate::extension_motion::native_contract(&self.binding.plan);
        let sampled = crate::extension_endpoints::sampled_contract(&self.binding.plan)?;
        self.native_validation
            .validate_canonical(&native)
            .map_err(invalid)?;
        self.sampled_validation
            .validate_canonical(&sampled)
            .map_err(invalid)?;
        if self.native_validation.output_bytes != self.native.byte_length()
            || self.sampled_validation.output_bytes != self.sampled.byte_length()
            || self.native_validation.input_rgb_sha256 != self.native_validation.output_rgb_sha256
            || self.native_validation.output_rgb_sha256 != self.sampled_validation.input_rgb_sha256
            || self.native_validation.input_time_base_num
                != self.sampled_validation.input_time_base_num
            || self.native_validation.input_time_base_den
                != self.sampled_validation.input_time_base_den
            || self.native_validation.discarded_audio_streams
                != self.sampled_validation.discarded_audio_streams
            || self.native_validation.output_span().map_err(invalid)? != self.native_span
            || self.sampled_validation.output_span().map_err(invalid)? != self.sampled_span
            || (self.native == self.sampled && native != sampled)
        {
            return Err(invalid(
                "stored extension media, reports or measured spans disagree",
            ));
        }
        let manifest = self.conditioning.manifest();
        if manifest.declaration().reference() != &self.binding.input.manifest
            || manifest.declaration().sha256() != &self.binding.input.sha256
            || manifest.object().byte_length() > MAXIMUM_CONTEXT_BYTES as u64
            || self.worker_provenance_utf8.len() > MAXIMUM_WORKER_BYTES
        {
            return Err(invalid(
                "stored extension conditioning identity or provenance length is invalid",
            ));
        }
        verify_declaration(
            self.worker_provenance_utf8.as_bytes(),
            &self.declaration.provenance,
        )?;
        Ok(())
    }
}

/// Fully bound retained observations. This is candidate evidence, not authored
/// acceptance or proof that separately stored media/input bytes remain intact.
pub struct ValidatedExtensionEvidence {
    provenance: StoredExtensionProvenance,
    context: ExtensionContext,
}

impl ValidatedExtensionEvidence {
    pub fn binding(&self) -> &ExtensionGenerationBinding {
        &self.provenance.envelope.binding
    }
    pub fn context(&self) -> &ExtensionContext {
        &self.context
    }
    pub fn conditioning(&self) -> &ExtensionConditioningReceipt {
        &self.provenance.envelope.conditioning
    }
    pub fn pixels(&self) -> &ExtensionPixelReport {
        &self.provenance.envelope.pixels
    }
    pub fn geometry(&self) -> &ExtensionGeometryChecks {
        &self.provenance.envelope.geometry
    }
    pub fn native_contract(&self) -> VideoContract {
        self.provenance.envelope.native_validation.video
    }
    pub fn sampled_contract(&self) -> VideoContract {
        self.provenance.envelope.sampled_validation.video
    }
    pub fn native_span(&self) -> SourceSpan {
        self.provenance.envelope.native_span
    }
    pub fn sampled_span(&self) -> SourceSpan {
        self.provenance.envelope.sampled_span
    }
    pub fn native_object(&self) -> &GeneratedObjectRef {
        &self.provenance.envelope.native
    }
    pub fn sampled_object(&self) -> &GeneratedObjectRef {
        &self.provenance.envelope.sampled
    }
    pub fn provenance_object(&self) -> &GeneratedObjectRef {
        &self.provenance.object
    }
}

/// Opaque evidence bound to the accepted Extension artifact and project.
/// This does not prove that separately stored media/input bytes remain intact.
pub struct AcceptedExtensionEvidence {
    evidence: ValidatedExtensionEvidence,
}

impl AcceptedExtensionEvidence {
    pub fn generation_options(&self) -> deadpan_jobs::GenerationOptions {
        deadpan_jobs::GenerationOptions::from_constraints(&self.evidence.binding().constraints)
    }
    pub fn binding(&self) -> &ExtensionGenerationBinding {
        self.evidence.binding()
    }
    pub fn context(&self) -> &ExtensionContext {
        self.evidence.context()
    }
    pub fn conditioning(&self) -> &ExtensionConditioningReceipt {
        self.evidence.conditioning()
    }
    pub fn pixels(&self) -> &ExtensionPixelReport {
        self.evidence.pixels()
    }
    pub fn geometry(&self) -> &ExtensionGeometryChecks {
        self.evidence.geometry()
    }
    pub fn native_contract(&self) -> VideoContract {
        self.evidence.native_contract()
    }
    pub fn sampled_contract(&self) -> VideoContract {
        self.evidence.sampled_contract()
    }
    pub fn native_span(&self) -> SourceSpan {
        self.evidence.native_span()
    }
    pub fn sampled_span(&self) -> SourceSpan {
        self.evidence.sampled_span()
    }
    pub fn native_object(&self) -> &GeneratedObjectRef {
        self.evidence.native_object()
    }
    pub fn sampled_object(&self) -> &GeneratedObjectRef {
        self.evidence.sampled_object()
    }
    pub fn provenance_object(&self) -> &GeneratedObjectRef {
        self.evidence.provenance_object()
    }
    pub fn context_object(&self) -> &GeneratedObjectRef {
        self.evidence.conditioning().manifest().object()
    }
}

fn invalid(reason: impl std::fmt::Display) -> QualificationError {
    QualificationError::Provenance(format!("{PROFILE}: {reason}"))
}
