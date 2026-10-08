//! Immutable extension intent and its independently selected provider envelope.

use deadpan_core::{ExtensionDirection, FrameDuration, FrameRate, ProjectId, RevisionId};
use deadpan_jobs::{
    AxisLimits, ConditioningMode, ContextArtifact, DimensionLimits, ExtensionCapability,
    ExtensionGenerationPlan, FrameCountFormula, HoldConstraints, HoldTarget, HostMessage,
    MessageIdentity, NativeCandidateManifest, ProviderSelection,
};
use serde::{Deserialize, Serialize};

use crate::QualificationError;

/// The exact worker request binding, excluding process-control configuration.
/// The extension plan retains its own operation/version discriminator. Strong
/// identifier and request-version types validate those fields during decoding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "BindingWire")]
pub struct ExtensionGenerationBinding {
    pub identity: MessageIdentity,
    pub project_id: ProjectId,
    pub revision_id: RevisionId,
    pub target: HoldTarget,
    pub input: ContextArtifact,
    pub constraints: HoldConstraints,
    pub provider: ProviderSelection,
    pub plan: ExtensionGenerationPlan,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BindingWire {
    identity: MessageIdentity,
    project_id: ProjectId,
    revision_id: RevisionId,
    target: HoldTarget,
    input: ContextArtifact,
    constraints: HoldConstraints,
    provider: ProviderSelection,
    plan: ExtensionGenerationPlan,
}

impl ExtensionGenerationBinding {
    pub fn from_request(request: &HostMessage) -> Result<Self, QualificationError> {
        request.validate().map_err(invalid)?;
        let HostMessage::GenerateExtension {
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
            return Err(invalid("a version-3 extension request is required"));
        };
        let binding = Self {
            identity: identity.clone(),
            project_id: project_id.clone(),
            revision_id: revision_id.clone(),
            target: target.clone(),
            input: input.clone(),
            constraints: constraints.clone(),
            provider: provider.as_ref().clone(),
            plan: plan.as_ref().clone(),
        };
        binding.validate()?;
        Ok(binding)
    }

    /// Recheck stored intent without inventing a cancellation token, output
    /// workspace, or other process controls. Capability admission is separate.
    pub fn validate(&self) -> Result<(), QualificationError> {
        let conditioning = match self.plan.direction() {
            ExtensionDirection::FromLeft => ConditioningMode::ExtendFromLeft,
            ExtensionDirection::FromRight => ConditioningMode::ExtendFromRight,
        };
        let dimensions = self.plan.native_dimensions();
        if self.constraints.conditioning != conditioning
            || self.constraints.video.frames() != self.plan.project_frames()
            || self.constraints.video.frame_rate() != self.plan.project_frame_rate()
            || self.constraints.video.width() != dimensions.width()
            || self.constraints.video.height() != dimensions.height()
        {
            return Err(invalid(
                "constraints differ from the captured extension plan",
            ));
        }
        Ok(())
    }

    /// Bind untrusted native metadata to the host's independent provider choice.
    /// This admits neither media bytes nor worker provenance; their retained
    /// snapshots and output checks remain qualification prerequisites.
    pub fn validate_for(
        &self,
        selected: &SelectedExtensionProvider,
        declaration: &NativeCandidateManifest,
    ) -> Result<(), QualificationError> {
        self.validate()?;
        self.plan
            .validate_for(selected.capability())
            .map_err(invalid)?;
        declaration.validate().map_err(invalid)?;
        let dimensions = self.plan.native_dimensions();
        if selected.selection() != &self.provider
            || declaration.provider != self.provider
            || declaration.video.frames().frames() != i64::from(self.plan.native_frame_count())
            || declaration.video.frame_rate() != self.plan.native_frame_rate()
            || declaration.video.width() != dimensions.width()
            || declaration.video.height() != dimensions.height()
        {
            return Err(invalid(
                "provider or native declaration differs from the captured extension plan",
            ));
        }
        Ok(())
    }
}

impl TryFrom<BindingWire> for ExtensionGenerationBinding {
    type Error = QualificationError;

    fn try_from(wire: BindingWire) -> Result<Self, Self::Error> {
        let value = Self {
            identity: wire.identity,
            project_id: wire.project_id,
            revision_id: wire.revision_id,
            target: wire.target,
            input: wire.input,
            constraints: wire.constraints,
            provider: wire.provider,
            plan: wire.plan,
        };
        value.validate()?;
        Ok(value)
    }
}

/// Host-selected extension capability, independent of worker declarations.
/// Constructing this value does not attest installed model/runtime bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ProviderWire", into = "ProviderWire")]
pub struct SelectedExtensionProvider {
    selection: ProviderSelection,
    capability: ExtensionCapability,
}

impl SelectedExtensionProvider {
    pub const fn new(selection: ProviderSelection, capability: ExtensionCapability) -> Self {
        Self {
            selection,
            capability,
        }
    }

    pub fn selection(&self) -> &ProviderSelection {
        &self.selection
    }

    pub const fn capability(&self) -> &ExtensionCapability {
        &self.capability
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Operation {
    Extension,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderWire {
    schema_version: u32,
    operation: Operation,
    selection: ProviderSelection,
    native_frame_rate: FrameRate,
    context_frame_count: u32,
    generated_frame_counts: CountsWire,
    width: AxisWire,
    height: AxisWire,
    maximum_output_frames: FrameDuration,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CountsWire {
    step: u32,
    offset: u32,
    minimum: u32,
    maximum: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AxisWire {
    minimum: u32,
    maximum: u32,
    multiple: u32,
}

impl From<AxisLimits> for AxisWire {
    fn from(axis: AxisLimits) -> Self {
        Self {
            minimum: axis.minimum(),
            maximum: axis.maximum(),
            multiple: axis.multiple(),
        }
    }
}

impl TryFrom<ProviderWire> for SelectedExtensionProvider {
    type Error = QualificationError;

    fn try_from(wire: ProviderWire) -> Result<Self, Self::Error> {
        if wire.schema_version != 1 {
            return Err(invalid("unsupported selected extension provider schema"));
        }
        let Operation::Extension = wire.operation;
        let counts = FrameCountFormula::new(
            wire.generated_frame_counts.step,
            wire.generated_frame_counts.offset,
            wire.generated_frame_counts.minimum,
            wire.generated_frame_counts.maximum,
        )
        .map_err(invalid)?;
        let dimensions = DimensionLimits::new(
            AxisLimits::new(wire.width.minimum, wire.width.maximum, wire.width.multiple)
                .map_err(invalid)?,
            AxisLimits::new(
                wire.height.minimum,
                wire.height.maximum,
                wire.height.multiple,
            )
            .map_err(invalid)?,
        );
        let capability = ExtensionCapability::new(
            wire.native_frame_rate,
            wire.context_frame_count,
            counts,
            dimensions,
            wire.maximum_output_frames,
        )
        .map_err(invalid)?;
        Ok(Self::new(wire.selection, capability))
    }
}

impl From<SelectedExtensionProvider> for ProviderWire {
    fn from(value: SelectedExtensionProvider) -> Self {
        let capability = value.capability;
        let counts = capability.generated_frame_counts();
        Self {
            schema_version: 1,
            operation: Operation::Extension,
            selection: value.selection,
            native_frame_rate: capability.native_frame_rate(),
            context_frame_count: capability.context_frame_count(),
            generated_frame_counts: CountsWire {
                step: counts.step(),
                offset: counts.offset(),
                minimum: counts.minimum(),
                maximum: counts.maximum(),
            },
            width: capability.dimensions().width().into(),
            height: capability.dimensions().height().into(),
            maximum_output_frames: capability.maximum_output_frames(),
        }
    }
}

fn invalid(reason: impl std::fmt::Display) -> QualificationError {
    QualificationError::Request(format!("extension binding: {reason}"))
}

#[cfg(test)]
mod tests;
