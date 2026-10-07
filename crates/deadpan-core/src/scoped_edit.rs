//! Explicit definition/occurrence authoring without inventing rendered plays.
//! Hosts allocate identities; browsing and allocation queries never isolate.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::{
    AudioBoundaryKind, AudioEdgePolicy, AudioTreatments, Command, CommandRequest, EditError,
    EditErrorCode, EditTransaction, Framing, HoldAudio, InstancePath, IterationId,
    MAX_DOCUMENT_DEPTH, NodeId, NodeKind, OccurrenceIdentities, ProjectDocument,
};

mod isolation;
mod proof;
pub use isolation::MAX_SCOPED_TARGETS;
pub(crate) use isolation::{apply_many_mapped, apply_mapped, matches_prefix};
pub use proof::{
    ScopedIsolationRecord, ScopedIsolationStep, ValidatedScopedIsolation, derive_scoped_isolation,
};

/// Default follows the authored template, even when no current play uses it.
/// Play follows that stable iteration's effective child or explicitly owned gap.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum RepeatEditBranch {
    #[serde(deserialize_with = "deserialize_empty")]
    Default,
    Play {
        iteration: IterationId,
    },
}

fn deserialize_empty<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<(), D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Empty {}
    Empty::deserialize(deserializer).map(|_| ())
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepeatEditStep {
    pub repeat: NodeId,
    pub branch: RepeatEditBranch,
}

/// Every Repeat ancestor appears exactly once, outermost first. Ordinary
/// Sequence/Retime owners remain in the tree but do not change this address.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedNodeTarget {
    pub node: NodeId,
    #[serde(deserialize_with = "bounded_steps")]
    pub repeats: Vec<RepeatEditStep>,
}

fn bounded_steps<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<RepeatEditStep>, D::Error> {
    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Vec<RepeatEditStep>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a bounded explicit Repeat ancestry")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            let mut steps = Vec::new();
            while let Some(step) = sequence.next_element()? {
                if steps.len() == MAX_DOCUMENT_DEPTH {
                    return Err(serde::de::Error::custom(
                        "scoped target exceeds depth limit",
                    ));
                }
                steps.push(step);
            }
            Ok(steps)
        }
    }
    deserializer.deserialize_seq(Visitor)
}

impl ScopedNodeTarget {
    /// Validate ownership only. This grants no concrete project position and
    /// does not require a Default branch to have any visible occurrence.
    pub fn validate(&self, document: &ProjectDocument) -> Result<(), EditError> {
        isolation::validate_target(document, self).map(|_| ())
    }

    /// Test an independently selected concrete presentation. Default steps
    /// accept only actual uses of this definition, never another override.
    pub fn matches_instance(
        &self,
        document: &ProjectDocument,
        instance: &InstancePath,
    ) -> Result<bool, EditError> {
        self.validate(document)?;
        instance.validate(document)?;
        Ok(instance.node == self.node
            && instance.repeats.len() == self.repeats.len()
            && matches_prefix(instance, &self.repeats))
    }
}

/// One target of a multi-target scoped edit with its own value, so a relative
/// change (such as +3 dB over a range) keeps each play's existing recipe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedTargetEdit {
    pub target: ScopedNodeTarget,
    pub edit: ScopedNodeEdit,
}

/// Value edits preserve every duration and source coordinate. Temporal edits
/// need a separate retained-clock and root-sound contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScopedNodeEdit {
    Rename {
        label: String,
    },
    SetFraming {
        framing: Option<Framing>,
    },
    SetAudioTreatments {
        treatments: AudioTreatments,
    },
    SetAudioEdge {
        edge: AudioBoundaryKind,
        policy: AudioEdgePolicy,
    },
    SetHoldAudio {
        audio: HoldAudio,
    },
    AcceptGeneratedHold {
        artifact: crate::GeneratedArtifact,
        #[serde(deserialize_with = "crate::document::unique_map")]
        assets: BTreeMap<crate::AssetId, crate::AssetRecord>,
    },
    RevertGeneratedHold,
}

impl ScopedNodeEdit {
    fn unchanged(&self, node: &crate::BeatNode) -> bool {
        match self {
            Self::Rename { label } => &node.label == label,
            Self::SetFraming { framing } => &node.framing == framing,
            Self::SetAudioTreatments { treatments } => &node.audio_treatments == treatments,
            Self::SetAudioEdge { edge, policy } => node.audio_edges.get(*edge) == *policy,
            Self::SetHoldAudio { audio } => matches!(&node.kind,
                NodeKind::Hold { recipe } if &recipe.audio == audio),
            Self::AcceptGeneratedHold { artifact, .. } => matches!(&node.kind,
                NodeKind::Hold { recipe } if matches!(&recipe.video,
                    crate::HoldVideo::Generated { accepted } if &accepted.artifact == artifact)),
            // The host must see the explicit provider choice even when the
            // fallback is already active, to supersede pending generation.
            Self::RevertGeneratedHold => false,
        }
    }

    pub(crate) fn command(&self, node: NodeId) -> Command {
        match self {
            Self::Rename { label } => Command::Rename {
                node,
                label: label.clone(),
            },
            Self::SetFraming { framing } => Command::SetFraming {
                node,
                framing: framing.clone(),
            },
            Self::SetAudioTreatments { treatments } => Command::SetAudioTreatments {
                node,
                treatments: treatments.clone(),
            },
            Self::SetAudioEdge { edge, policy } => Command::SetAudioEdge {
                node,
                edge: *edge,
                policy: *policy,
            },
            Self::SetHoldAudio { audio } => Command::SetHoldAudio {
                node,
                audio: audio.clone(),
            },
            Self::AcceptGeneratedHold { artifact, assets } => Command::AcceptGeneratedHold {
                node,
                artifact: artifact.clone(),
                assets: assets.clone(),
            },
            Self::RevertGeneratedHold => Command::RevertGeneratedHold { node },
        }
    }

    fn validate(&self, document: &ProjectDocument, node: &NodeId) -> Result<(), EditError> {
        match self {
            Self::Rename { label } => crate::document::validate_label(label)?,
            Self::SetFraming { framing } => {
                if let Some(framing) = framing {
                    framing
                        .validate()
                        .map_err(|error| invalid(&error.to_string()))?;
                }
            }
            Self::SetAudioTreatments { treatments } => {
                treatments.validate().map_err(crate::audio_gain::invalid)?;
            }
            Self::SetAudioEdge { edge, .. } => {
                if !edge.supports(&document.nodes()[node].kind) {
                    return Err(EditError::new(
                        EditErrorCode::WrongNodeKind,
                        "selected audio boundary does not belong to this node kind",
                    ));
                }
            }
            Self::SetHoldAudio { audio } => {
                let NodeKind::Hold { recipe } = &document.nodes()[node].kind else {
                    return Err(EditError::new(
                        EditErrorCode::WrongNodeKind,
                        "scoped Hold audio requires an authored Hold",
                    ));
                };
                let mut recipe = recipe.clone();
                recipe.audio = audio.clone();
                document.validate_hold(&recipe)?;
            }
            Self::AcceptGeneratedHold { artifact, assets } => {
                let mut staged = document.clone();
                crate::command::accept_generated_hold(&mut staged, node, artifact, assets)?;
                let NodeKind::Hold { recipe } = &staged.nodes()[node].kind else {
                    unreachable!("acceptance checks Hold kind")
                };
                staged.validate_hold(recipe)?;
            }
            Self::RevertGeneratedHold => {
                let NodeKind::Hold { recipe } = &document.nodes()[node].kind else {
                    return Err(EditError::new(
                        EditErrorCode::WrongNodeKind,
                        "scoped provider reversion requires an authored Hold",
                    ));
                };
                crate::command::reverted_hold_video(&recipe.video)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ScopedEditRequirements {
    pub nodes: usize,
    pub marks: usize,
    /// An exact no-op needs no isolation; the command refuses to author it.
    pub unchanged: bool,
}

impl ProjectDocument {
    /// Exact identity counts, including nested selective isolation. No identity
    /// or concrete iteration is generated, and no document is copied.
    pub fn scoped_edit_requirements(
        &self,
        target: &ScopedNodeTarget,
        edit: &ScopedNodeEdit,
    ) -> Result<ScopedEditRequirements, EditError> {
        isolation::preflight(self, target, edit).map(|plan| plan.requirements)
    }

    /// Exact per-target needs of [`Command::EditScopedMany`], staged in order
    /// so a later target sees the isolation an earlier one performed.
    pub fn scoped_many_requirements(
        &self,
        edits: &[ScopedTargetEdit],
    ) -> Result<Vec<ScopedEditRequirements>, EditError> {
        isolation::many_requirements(self, edits)
    }
}

#[derive(Debug)]
pub struct PreparedScopedEdit {
    pub transaction: EditTransaction,
    pub document: ProjectDocument,
    pub target: ScopedNodeTarget,
    isolation: ScopedIsolationRecord,
}

impl PreparedScopedEdit {
    pub fn isolation(&self) -> &ScopedIsolationRecord {
        &self.isolation
    }

    pub fn map_target(
        &self,
        before: &ProjectDocument,
        target: &ScopedNodeTarget,
    ) -> Result<ScopedNodeTarget, EditError> {
        self.isolation
            .bind_prepared(before, &self.document)?
            .map_forward(target)
    }
    /// Carry a separately captured concrete presentation through the same
    /// isolation. Default authoring alone never manufactures such a path.
    pub fn map_instance(
        &self,
        before: &ProjectDocument,
        instance: &InstancePath,
    ) -> Result<InstancePath, EditError> {
        if before.project_id() != &self.transaction.forward.project_id
            || before.revision_id() != &self.transaction.forward.from_revision
        {
            return Err(EditError::new(
                EditErrorCode::RevisionConflict,
                "presentation source differs from the prepared scoped edit",
            ));
        }
        instance.validate(before)?;
        let mut result = instance.clone();
        for mapping in self.isolation.steps() {
            if isolation::matches_prefix(&result, mapping.before_prefix()) {
                crate::occurrence_edit::remap_instance(&mut result, mapping.nodes());
            }
        }
        result.validate(&self.document)?;
        Ok(result)
    }
}

/// Preview precisely the same EditScoped command that the host will commit.
/// The mapped target is provisional until that command's receipt is observed.
pub fn prepare_scoped_edit(
    document: &ProjectDocument,
    request: &CommandRequest,
) -> Result<PreparedScopedEdit, EditError> {
    crate::command::check_revision(
        document,
        &request.project_id,
        &request.expected_revision,
        &request.new_revision,
    )?;
    let Command::EditScoped { target, .. } = &request.command else {
        return Err(invalid("scoped preparation requires an EditScoped command"));
    };
    let (transaction, after, steps) = crate::command::apply_with_isolation(document, request)?;
    let isolation = ScopedIsolationRecord::from_execution(document, request, &after, steps)?;
    let target = isolation
        .bind_prepared(document, &after)?
        .map_forward(target)?;
    Ok(PreparedScopedEdit {
        transaction,
        document: after,
        target,
        isolation,
    })
}

fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}
fn limit(message: &str) -> EditError {
    EditError::new(EditErrorCode::LimitExceeded, message)
}

#[cfg(test)]
mod tests;
