//! Explicit operation dispatch for retained, accepted generation evidence.

use deadpan_core::{
    GeneratedArtifact, GeneratedObjectRef, GeneratedSamplingMap, ProjectId, SourceSpan,
};
use deadpan_media::protocol::VideoContract;

use crate::{
    AcceptedBridgeEvidence, AcceptedExtensionEvidence, ConditioningArtifactReceipt,
    QualificationError, StoredBridgeProvenance, StoredExtensionProvenance,
};

/// Object-verified metadata selected by the authored artifact's operation.
/// Parsing never retries another operation when an envelope is invalid.
pub enum StoredGeneratedProvenance {
    Bridge(Box<StoredBridgeProvenance>),
    Extension(Box<StoredExtensionProvenance>),
}

impl StoredGeneratedProvenance {
    pub fn from_bytes(
        bytes: &[u8],
        artifact: &GeneratedArtifact,
    ) -> Result<Self, QualificationError> {
        match &artifact.sampling {
            GeneratedSamplingMap::Bridge(_) => {
                StoredBridgeProvenance::from_bytes(bytes, &artifact.provenance)
                    .map(|value| Self::Bridge(Box::new(value)))
            }
            GeneratedSamplingMap::Extension(_) => {
                StoredExtensionProvenance::from_bytes(bytes, &artifact.provenance)
                    .map(|value| Self::Extension(Box::new(value)))
            }
        }
    }

    pub fn context_object(&self) -> &GeneratedObjectRef {
        match self {
            Self::Bridge(value) => value.context_object(),
            Self::Extension(value) => value.context_object(),
        }
    }

    pub fn validate_for(
        self,
        artifact: &GeneratedArtifact,
        project: &ProjectId,
        context_bytes: &[u8],
    ) -> Result<AcceptedGenerationEvidence, QualificationError> {
        match self {
            Self::Bridge(value) => value
                .validate_for(artifact, project, context_bytes)
                .map(|value| AcceptedGenerationEvidence::Bridge(Box::new(value))),
            Self::Extension(value) => value
                .validate_artifact(artifact, project, context_bytes)
                .map(|value| AcceptedGenerationEvidence::Extension(Box::new(value))),
        }
    }
}

/// Saved policy and input identities bound to an accepted artifact. Consumers
/// must still admit retained bytes before decoding or using them as new inputs.
pub enum AcceptedGenerationEvidence {
    Bridge(Box<AcceptedBridgeEvidence>),
    Extension(Box<AcceptedExtensionEvidence>),
}

impl AcceptedGenerationEvidence {
    pub fn generation_options(&self) -> deadpan_jobs::GenerationOptions {
        match self {
            Self::Bridge(value) => value.generation_options(),
            Self::Extension(value) => value.generation_options(),
        }
    }
    pub fn native_contract(&self) -> VideoContract {
        match self {
            Self::Bridge(value) => value.native_contract(),
            Self::Extension(value) => value.native_contract(),
        }
    }
    pub fn sampled_contract(&self) -> VideoContract {
        match self {
            Self::Bridge(value) => value.sampled_contract(),
            Self::Extension(value) => value.sampled_contract(),
        }
    }
    pub fn native_span(&self) -> SourceSpan {
        match self {
            Self::Bridge(value) => value.native_span(),
            Self::Extension(value) => value.native_span(),
        }
    }
    pub fn sampled_span(&self) -> SourceSpan {
        match self {
            Self::Bridge(value) => value.sampled_span(),
            Self::Extension(value) => value.sampled_span(),
        }
    }
    pub fn native_object(&self) -> &GeneratedObjectRef {
        match self {
            Self::Bridge(value) => value.native_object(),
            Self::Extension(value) => value.native_object(),
        }
    }
    pub fn sampled_object(&self) -> &GeneratedObjectRef {
        match self {
            Self::Bridge(value) => value.sampled_object(),
            Self::Extension(value) => value.sampled_object(),
        }
    }
    pub fn provenance_object(&self) -> &GeneratedObjectRef {
        match self {
            Self::Bridge(value) => value.provenance_object(),
            Self::Extension(value) => value.provenance_object(),
        }
    }
    pub fn context_object(&self) -> &GeneratedObjectRef {
        match self {
            Self::Bridge(value) => value.context_object(),
            Self::Extension(value) => value.context_object(),
        }
    }

    /// Every retained conditioning occurrence with its declared SHA-256. This
    /// includes the manifest, all PNGs, and Extension continuity signatures.
    /// Shared objects may occur more than once; callers can deduplicate snapshots
    /// by object identity while still checking each declaration.
    pub fn conditioning_inputs(&self) -> impl Iterator<Item = (&GeneratedObjectRef, &str)> {
        let (manifest, left, right, frames, opposite, signatures) = match self {
            Self::Bridge(value) => {
                let receipt = value.conditioning();
                (
                    receipt.manifest(),
                    Some(receipt.left()),
                    Some(receipt.right()),
                    &[] as &[ConditioningArtifactReceipt],
                    None,
                    None,
                )
            }
            Self::Extension(value) => {
                let receipt = value.conditioning();
                (
                    receipt.manifest(),
                    None,
                    None,
                    receipt.context(),
                    receipt.opposite(),
                    Some(receipt.signatures()),
                )
            }
        };
        std::iter::once(manifest)
            .chain(left)
            .chain(right)
            .chain(frames)
            .chain(opposite)
            .chain(signatures)
            .map(|receipt| (receipt.object(), receipt.declaration().sha256().as_str()))
    }
}
