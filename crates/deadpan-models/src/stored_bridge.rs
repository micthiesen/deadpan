//! Revalidate retained bridge evidence independently of a worker or model pack.

use deadpan_core::{
    GeneratedArtifact, GeneratedObjectRef, GeneratedSamplingMap, ProjectId, SourceSpan,
};
use deadpan_jobs::{ConditioningMode, NativeCandidateManifest, WorkspaceArtifact};
use deadpan_media::protocol::{ConversionReport, VideoContract};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    BridgeContext, BridgeEndpointReport, BridgeGeometryReport, BridgeQualityReport,
    BridgeRegionReport, ConditioningReceipt, GenerationBinding, QualificationError,
    SelectedBridgeProvider,
};

const MAXIMUM_HOST_PROVENANCE_BYTES: usize = 32 * 1024 * 1024;
const MAXIMUM_WORKER_PROVENANCE_BYTES: usize = 4 * 1024 * 1024;
const MAXIMUM_CONTEXT_BYTES: usize = 1024 * 1024;
const MAXIMUM_FRAME_BYTES: u64 = 64 * 1024 * 1024;

/// An object-verified stored envelope. This alone does not admit media: use
/// `validate_for` against the captured authored artifact and retained context.
/// No stored workspace paths are opened, and no installed provider is consulted.
pub struct StoredBridgeProvenance {
    object: GeneratedObjectRef,
    envelope: StoredEnvelope,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredEnvelope {
    schema_version: u32,
    validation_profile: String,
    binding: GenerationBinding,
    selected_provider: SelectedBridgeProvider,
    declaration: NativeCandidateManifest,
    native: GeneratedObjectRef,
    sampled: GeneratedObjectRef,
    native_validation: ConversionReport,
    sampled_validation: ConversionReport,
    conditioning: ConditioningReceipt,
    native_span: SourceSpan,
    sampled_span: SourceSpan,
    worker_provenance_utf8: String,
    #[serde(default, deserialize_with = "deserialize_quality")]
    quality: Option<BridgeQualityReport>,
    #[serde(default, deserialize_with = "deserialize_endpoints")]
    endpoints: Option<BridgeEndpointReport>,
    #[serde(default, deserialize_with = "deserialize_geometry")]
    geometry: Option<BridgeGeometryReport>,
    #[serde(default, deserialize_with = "deserialize_region")]
    region: Option<BridgeRegionReport>,
}

// `Option<T>` normally treats a present JSON `null` like an absent field.
// Keep those states distinct so schema 3 really has no quality field and
// schema 4 must carry an actual report.
fn deserialize_quality<'de, D>(deserializer: D) -> Result<Option<BridgeQualityReport>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    BridgeQualityReport::deserialize(deserializer).map(Some)
}

fn deserialize_endpoints<'de, D>(deserializer: D) -> Result<Option<BridgeEndpointReport>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    BridgeEndpointReport::deserialize(deserializer).map(Some)
}

fn deserialize_geometry<'de, D>(deserializer: D) -> Result<Option<BridgeGeometryReport>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    BridgeGeometryReport::deserialize(deserializer).map(Some)
}

fn deserialize_region<'de, D>(deserializer: D) -> Result<Option<BridgeRegionReport>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    BridgeRegionReport::deserialize(deserializer).map(Some)
}

impl StoredBridgeProvenance {
    /// Decode bounded strict JSON and verify the exact retained object identity.
    /// Older profiles lack admission evidence and deliberately fail here.
    pub fn from_bytes(
        bytes: &[u8],
        expected: &GeneratedObjectRef,
    ) -> Result<Self, QualificationError> {
        verify_object(bytes, expected, MAXIMUM_HOST_PROVENANCE_BYTES)?;
        let envelope: StoredEnvelope = serde_json::from_value(crate::strict_json::parse(bytes)?)?;
        envelope.validate()?;
        Ok(Self {
            object: expected.clone(),
            envelope,
        })
    }

    pub fn context_object(&self) -> &GeneratedObjectRef {
        self.envelope.conditioning.manifest().object()
    }

    /// Host-recorded quality-screen metadata, when present. This reports the
    /// bounded motion/lighting checks; it does not admit or validate media bytes.
    pub fn quality(&self) -> Option<&BridgeQualityReport> {
        self.envelope.quality.as_ref()
    }

    pub fn endpoints(&self) -> Option<&BridgeEndpointReport> {
        self.envelope.endpoints.as_ref()
    }

    pub fn geometry(&self) -> Option<&BridgeGeometryReport> {
        self.envelope.geometry.as_ref()
    }

    pub fn region(&self) -> Option<&BridgeRegionReport> {
        self.envelope.region.as_ref()
    }

    /// Bind durable evidence to an immutable authored artifact and its project.
    /// Revision, Hold identity and current duration intentionally do not gate
    /// admission: accepted footage survives undo, copy and supported resizing.
    /// The caller must independently snapshot both media and retained input
    /// objects by the returned identities before using their bytes.
    pub fn validate_for(
        self,
        artifact: &GeneratedArtifact,
        project: &ProjectId,
        context_bytes: &[u8],
    ) -> Result<AcceptedBridgeEvidence, QualificationError> {
        let envelope = &self.envelope;
        let sampling = envelope.binding.plan.sampling_map().map_err(invalid)?;
        let GeneratedSamplingMap::Bridge(artifact_sampling) = &artifact.sampling else {
            return Err(invalid(
                "retained bridge cannot admit an extension artifact",
            ));
        };
        if &envelope.binding.project_id != project
            || self.object != artifact.provenance
            || envelope.native != artifact.native_object
            || envelope.sampled != artifact.sampled_object
            || &sampling != artifact_sampling
        {
            return Err(invalid(
                "retained bridge differs from the authored artifact/project",
            ));
        }
        verify_object(context_bytes, self.context_object(), MAXIMUM_CONTEXT_BYTES)?;
        verify_declaration(
            context_bytes,
            envelope.conditioning.manifest().declaration(),
        )?;
        let context: BridgeContext =
            serde_json::from_value(crate::strict_json::parse(context_bytes)?)?;
        if context.schema_version() == 5 && envelope.schema_version < 8 {
            return Err(invalid("definition clocks require stored bridge profile 8"));
        }
        // The immutable generation origin is authoritative even after an
        // accepted artifact is copied, its provider is undone, or its current
        // authored Hold changes. Project-clock contexts retain their grammar.
        context.validate_definition_binding(
            &envelope.binding.project_id,
            &envelope.binding.revision_id,
        )?;
        if context.plan() != &envelope.binding.plan
            || context.left() != envelope.conditioning.left().declaration()
            || context.right() != envelope.conditioning.right().declaration()
        {
            return Err(invalid(
                "retained context differs from the plan or prepared inputs",
            ));
        }
        context.check_model_output()?;
        if let Some(endpoints) = &envelope.endpoints {
            endpoints.validate_context(&context)?;
        }
        if let Some(geometry) = &envelope.geometry {
            geometry.validate_context(&context)?;
        }
        if let Some(region) = &envelope.region {
            region.validate_context(&context)?;
        }
        let worker = envelope.worker_provenance_utf8.as_bytes();
        verify_declaration(worker, &envelope.declaration.provenance)?;
        crate::provenance::validate(
            crate::strict_json::parse(worker)?,
            &envelope.binding,
            &envelope.declaration,
            &context,
        )?;
        Ok(AcceptedBridgeEvidence { provenance: self })
    }
}

impl StoredEnvelope {
    fn validate(&self) -> Result<(), QualificationError> {
        let legacy = self.schema_version == 3
            && self.validation_profile == "deadpan-ffv1-bridge-3"
            && self.quality.is_none()
            && self.endpoints.is_none()
            && self.geometry.is_none()
            && self.region.is_none();
        let measured = self.schema_version == 4
            && self.validation_profile == "deadpan-ffv1-bridge-4"
            && self.quality.is_some()
            && self.endpoints.is_none()
            && self.geometry.is_none()
            && self.region.is_none();
        let endpoints = self.schema_version == 5
            && self.validation_profile == "deadpan-ffv1-bridge-5"
            && self.quality.is_some()
            && self.endpoints.is_some()
            && self.geometry.is_none()
            && self.region.is_none();
        let geometry = self.schema_version == 6
            && self.validation_profile == "deadpan-ffv1-bridge-6"
            && self.quality.is_some()
            && self.endpoints.is_some()
            && self.geometry.is_some()
            && self.region.is_none();
        let region = matches!(
            (self.schema_version, self.validation_profile.as_str()),
            (7, "deadpan-ffv1-bridge-7") | (8, "deadpan-ffv1-bridge-8")
        ) && self.quality.is_some()
            && self.endpoints.is_some()
            && self.geometry.is_some()
            && self.region.is_some();
        if !legacy && !measured && !endpoints && !geometry && !region {
            return Err(invalid(
                "unsupported stored bridge provenance profile/schema",
            ));
        }
        let binding = &self.binding;
        if self.schema_version < 7 && binding.constraints.region_target.is_some() {
            return Err(invalid(
                "selected region targets require stored region evidence",
            ));
        }
        let plan = &binding.plan;
        plan.validate_for(self.selected_provider.capability())
            .map_err(invalid)?;
        let dimensions = plan.native_dimensions();
        if binding.constraints.conditioning != ConditioningMode::Bridge
            || binding.constraints.video.frames() != plan.project_frames()
            || binding.constraints.video.frame_rate() != plan.project_frame_rate()
            || binding.constraints.video.width() != dimensions.width()
            || binding.constraints.video.height() != dimensions.height()
            || self.selected_provider.selection() != &binding.provider
            || self.declaration.provider != binding.provider
            || self.declaration.video.frames().frames() != i64::from(plan.native_frame_count())
            || self.declaration.video.frame_rate() != plan.native_frame_rate()
            || self.declaration.video.width() != dimensions.width()
            || self.declaration.video.height() != dimensions.height()
            || self.declaration.native.reference() == self.declaration.provenance.reference()
        {
            return Err(invalid(
                "stored request/provider/declaration differs from the bridge plan",
            ));
        }
        let native = VideoContract {
            width: dimensions.width(),
            height: dimensions.height(),
            frames: plan.native_frame_count(),
            rate_num: plan.native_frame_rate().numerator(),
            rate_den: plan.native_frame_rate().denominator(),
        };
        let sampled = VideoContract {
            frames: u32::try_from(plan.project_frames().frames()).map_err(invalid)?,
            rate_num: plan.project_frame_rate().numerator(),
            rate_den: plan.project_frame_rate().denominator(),
            ..native
        };
        self.native_validation
            .validate_canonical(&native)
            .map_err(invalid)?;
        self.sampled_validation
            .validate_canonical(&sampled)
            .map_err(invalid)?;
        if let Some(quality) = &self.quality {
            quality.validate(plan, binding.constraints.motion)?;
        }
        if let Some(endpoints) = &self.endpoints {
            endpoints.validate(plan, &self.sampled, &self.conditioning)?;
        }
        if let Some(geometry) = &self.geometry {
            geometry.validate(plan, &self.native, &self.conditioning)?;
            if self
                .endpoints
                .as_ref()
                .is_none_or(|endpoints| endpoints.geometry() != geometry.geometry())
            {
                return Err(invalid(
                    "landmark and endpoint reports disagree on captured geometry",
                ));
            }
        }
        if let Some(region) = &self.region {
            region.validate(plan, &self.native, &self.conditioning)?;
            if region.capture().target_id() != binding.constraints.region_target.as_ref()
                || self.geometry.as_ref().is_none_or(|geometry| {
                    geometry.geometry() != region.geometry()
                        || region
                            .inspection_timings()
                            .is_some_and(|timings| timings != geometry.inspection_timings())
                })
            {
                return Err(invalid(
                    "region report differs from the requested target or shared inspection",
                ));
            }
        }
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
                "stored media objects, reports or measured spans disagree",
            ));
        }
        let manifest = self.conditioning.manifest();
        if manifest.declaration().reference() != &binding.input.manifest
            || manifest.declaration().sha256() != &binding.input.sha256
            || manifest.object().byte_length() > MAXIMUM_CONTEXT_BYTES as u64
            || self.conditioning.left().object().byte_length() > MAXIMUM_FRAME_BYTES
            || self.conditioning.right().object().byte_length() > MAXIMUM_FRAME_BYTES
            || self.worker_provenance_utf8.len() > MAXIMUM_WORKER_PROVENANCE_BYTES
            || (self.conditioning.left().object() == self.conditioning.right().object()
                && self.conditioning.left().declaration().sha256()
                    != self.conditioning.right().declaration().sha256())
        {
            return Err(invalid(
                "stored conditioning identity or provenance length is invalid",
            ));
        }
        Ok(())
    }
}

/// Opaque evidence for the accepted artifact, derived entirely from retained
/// objects. Its contracts still require fresh decoder admission of media bytes.
pub struct AcceptedBridgeEvidence {
    provenance: StoredBridgeProvenance,
}

impl AcceptedBridgeEvidence {
    pub(crate) fn conditioning(&self) -> &ConditioningReceipt {
        &self.provenance.envelope.conditioning
    }

    /// The controls that produced this accepted artifact, recovered from its
    /// verified immutable binding. They remain available after copying the
    /// Hold or retiring its operational request and need no installed model.
    pub fn generation_options(&self) -> deadpan_jobs::GenerationOptions {
        deadpan_jobs::GenerationOptions::from_constraints(
            &self.provenance.envelope.binding.constraints,
        )
    }

    pub fn region(&self) -> Option<&BridgeRegionReport> {
        self.provenance.envelope.region.as_ref()
    }
    pub fn geometry(&self) -> Option<&BridgeGeometryReport> {
        self.provenance.envelope.geometry.as_ref()
    }
    pub fn endpoints(&self) -> Option<&BridgeEndpointReport> {
        self.provenance.envelope.endpoints.as_ref()
    }
    pub fn quality(&self) -> Option<&BridgeQualityReport> {
        self.provenance.envelope.quality.as_ref()
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
    pub fn context_object(&self) -> &GeneratedObjectRef {
        self.provenance.context_object()
    }
    pub fn left_object(&self) -> &GeneratedObjectRef {
        self.provenance.envelope.conditioning.left().object()
    }
    pub fn right_object(&self) -> &GeneratedObjectRef {
        self.provenance.envelope.conditioning.right().object()
    }
    pub fn left_sha256(&self) -> &str {
        self.provenance
            .envelope
            .conditioning
            .left()
            .declaration()
            .sha256()
            .as_str()
    }
    pub fn right_sha256(&self) -> &str {
        self.provenance
            .envelope
            .conditioning
            .right()
            .declaration()
            .sha256()
            .as_str()
    }
}

pub(crate) fn verify_object(
    bytes: &[u8],
    expected: &GeneratedObjectRef,
    maximum: usize,
) -> Result<(), QualificationError> {
    let length = u64::try_from(bytes.len()).map_err(invalid)?;
    if bytes.len() > maximum
        || length != expected.byte_length()
        || blake3::hash(bytes).to_hex().as_str() != expected.content().digest()
    {
        return Err(invalid(
            "stored object exceeds its bound or differs from its identity",
        ));
    }
    Ok(())
}

pub(crate) fn verify_declaration(
    bytes: &[u8],
    declaration: &WorkspaceArtifact,
) -> Result<(), QualificationError> {
    let length = u64::try_from(bytes.len()).map_err(invalid)?;
    let sha256: String = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if length != declaration.byte_length() || sha256 != declaration.sha256().as_str() {
        return Err(invalid(
            "retained bytes differ from the declared SHA-256 or length",
        ));
    }
    Ok(())
}

fn invalid(reason: impl std::fmt::Display) -> QualificationError {
    QualificationError::Provenance(reason.to_string())
}
