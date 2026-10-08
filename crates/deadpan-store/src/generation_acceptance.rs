//! Explicit authored admission of a selected, retained generated bundle.
//!
//! Only identities and optimistic expectations come from the caller. The exact
//! command, asset records and sampling map come from persisted host evidence.
//! This is a bounded I/O API for a host service, never an audio/UI callback.

use std::collections::BTreeMap;

use deadpan_core::{
    AssetId, AssetRecord, Command, CommandRequest, EditTransaction, GeneratedArtifact,
    GeneratedObjectRef, MarkId, NodeId, NodeKind, OccurrenceIdentities, ProjectDocument,
    RevisionId, ScopedNodeEdit, SourceSpan,
};
use deadpan_jobs::{JobState, MessageIdentity, Relevance, VideoSpec};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};

use crate::generated_media::GeneratedMediaLimits;
use crate::generation::{ContextObservation, RelevancePlan};
use crate::generation_attempts::{
    BundleValidationReceipt, CandidateAvailability, read_attempt, read_request,
    validate_bundle_receipt,
};
use crate::{
    CommandPlan, CommitOutcome, ProjectStore, StoreError, prepare_admitted_command,
    write_command_plan,
};

/// An explicit user acceptance of the exact bundle observed by the host.
/// Asset identities must be fresh; metadata cannot be supplied by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationAcceptance {
    pub expected_revision: RevisionId,
    pub new_revision: RevisionId,
    pub identity: MessageIdentity,
    pub expected_receipt: BundleValidationReceipt,
    pub sampled_asset: AssetId,
    pub native_asset: AssetId,
}

impl ProjectStore {
    /// Produces the exact atomic edit used to resolve complete before/after
    /// request relevance. This performs no writes and is available read-only.
    pub fn preview_generation_acceptance(
        &self,
        input: &GenerationAcceptance,
        limits: GeneratedMediaLimits,
    ) -> Result<EditTransaction, StoreError> {
        self.verify_bundle_objects(&input.expected_receipt, limits)?;
        let transaction = self.connection.unchecked_transaction()?;
        Ok(prepare_acceptance(&transaction, &self.documents, input)?.edit)
    }

    /// The exact accepted edit and every current request at its proven after
    /// address, for hosts preparing a complete explicit relevance plan.
    pub fn preview_generation_acceptance_contexts(
        &self,
        input: &GenerationAcceptance,
        limits: GeneratedMediaLimits,
    ) -> Result<
        (
            EditTransaction,
            Vec<crate::generation::StoredGenerationRequest>,
        ),
        StoreError,
    > {
        self.verify_bundle_objects(&input.expected_receipt, limits)?;
        let transaction = self.connection.unchecked_transaction()?;
        let plan = prepare_acceptance(&transaction, &self.documents, input)?;
        let request = serde_json::from_str(&plan.request_json)?;
        let contexts = crate::generation_scope::preview_command(
            &transaction,
            &plan.current,
            &request,
            &plan.next,
        )?;
        Ok((plan.edit, contexts))
    }

    /// Revalidates all retained dependencies before taking the SQLite write
    /// lock, then atomically admits precisely the still-selected Ready bundle.
    pub fn accept_generation_bundle(
        &mut self,
        input: &GenerationAcceptance,
        relevance: &RelevancePlan,
        limits: GeneratedMediaLimits,
    ) -> Result<CommitOutcome, StoreError> {
        self.require_writer()?;
        self.verify_bundle_objects(&input.expected_receipt, limits)?;
        let resolver = self.context_resolver.clone();
        let documents = &self.documents;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let plan = prepare_acceptance(&transaction, documents, input)?;
        let admitted_request =
            crate::generation::read_stored_request(&transaction, &input.identity.request_id)?
                .ok_or_else(|| invalid("qualified acceptance has no retained request"))?;
        let authored: CommandRequest = serde_json::from_str(&plan.request_json)?;
        let artifact = match authored.command.base_command() {
            Command::AcceptGeneratedHold { artifact, .. }
            | Command::EditScoped {
                edit: ScopedNodeEdit::AcceptGeneratedHold { artifact, .. },
                ..
            } => artifact.clone(),
            _ => {
                return Err(invalid(
                    "qualified acceptance has no exact generated artifact",
                ));
            }
        };
        let automatically_replaced = plan.boundary_births.iter().any(|birth| {
            matches!(&birth.origin,
                crate::generation_preparations::PreparationOrigin::AcceptedBoundary { accepted, .. }
                if **accepted == artifact)
        });
        if !automatically_replaced
            && !relevance.observations.iter().any(|observation| {
                observation.request_id == input.identity.request_id
                    && observation.after_context
                        == ContextObservation::Resolved(observation.binding.context_sha256.clone())
            })
        {
            return Err(invalid(
                "acceptance must preserve the selected request's resolved context",
            ));
        }
        let (outcome, next) = write_command_plan(
            &transaction,
            documents,
            plan,
            Some(relevance),
            resolver.as_deref(),
        )?;
        // Operational: an accepted variant never expires, even after Undo,
        // because history keeps naming it.
        crate::generation_retention::record_accepted(&transaction, &input.identity)?;
        let origin = crate::generation_origins::capture(
            &transaction,
            &admitted_request,
            &input.identity,
            &artifact,
            &input.new_revision,
        )?;
        crate::generation_origins::insert(&transaction, &origin)?;
        crate::audit::refresh_generation_scopes(&transaction)?;
        transaction.commit()?;
        documents.insert(next);
        Ok(outcome)
    }
}

fn prepare_acceptance(
    connection: &Connection,
    documents: &crate::document_cache::DocumentCache,
    input: &GenerationAcceptance,
) -> Result<CommandPlan, StoreError> {
    let current = documents.head(connection)?;
    if current.revision_id() != &input.expected_revision {
        return Err(StoreError::RevisionConflict {
            expected: input.expected_revision.as_str().into(),
            current: current.revision_id().as_str().into(),
        });
    }
    let request = read_request(connection, &input.identity.request_id)?;
    if request.relevance != Relevance::Current {
        return Err(invalid("the request is no longer current"));
    }
    let selected: Option<String> = connection
        .query_row(
            "SELECT selected_ready_attempt_id FROM generation_attempt_heads WHERE request_id=?1",
            [input.identity.request_id.as_str()],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    if selected.as_deref() != Some(input.identity.attempt_id.as_str()) {
        return Err(invalid("the selected bundle changed"));
    }
    let attempt = read_attempt(connection, &input.identity)?;
    let receipt = attempt
        .bundle_receipt
        .as_ref()
        .ok_or_else(|| invalid("the attempt has no bundle receipt"))?;
    if attempt.checkpoint.state != JobState::Ready
        || receipt.availability() != CandidateAvailability::Present
        || receipt != &input.expected_receipt
    {
        return Err(invalid(
            "the selected Ready receipt changed or is unavailable",
        ));
    }
    validate_bundle_receipt(
        &request,
        attempt.ordinal,
        attempt.declared_candidate.as_ref(),
        receipt,
    )?;
    let evidence = receipt
        .admission()
        .ok_or_else(|| invalid("legacy bundle lacks measured spans and retained input evidence"))?;
    let plan = request
        .plan
        .as_ref()
        .ok_or_else(|| invalid("acceptance requires a retained native plan"))?;
    let Some(NodeKind::Hold { recipe }) = current
        .nodes()
        .get(&request.target.node)
        .map(|node| &node.kind)
    else {
        return Err(invalid("the target Hold no longer exists"));
    };
    if current.project_id() != &request.binding.project_id
        || recipe.duration != plan.project_frames()
        || current.presentation_basis().frame_rate != plan.project_frame_rate()
        || evidence.inputs().context_sha256() != &request.binding.context_sha256
    {
        return Err(invalid(
            "the Hold, rate or context no longer matches the request",
        ));
    }
    request.target.validate(&current)?;
    for id in [&input.native_asset, &input.sampled_asset] {
        if current.assets().contains_key(id) {
            return Err(invalid("acceptance requires fresh asset identities"));
        }
    }
    let native = asset_record(
        receipt.native_object(),
        receipt.native_video(),
        evidence.native_span(),
    );
    let sampled = asset_record(
        receipt.sampled_object(),
        receipt.sampled_video(),
        evidence.sampled_span(),
    );
    if input.native_asset == input.sampled_asset && native != sampled {
        return Err(invalid(
            "one asset identity cannot describe distinct masters",
        ));
    }
    let artifact = GeneratedArtifact {
        sampled_asset: input.sampled_asset.clone(),
        sampled_object: receipt.sampled_object().clone(),
        native_asset: input.native_asset.clone(),
        native_object: receipt.native_object().clone(),
        provenance: receipt.provenance_object().clone(),
        sampling: receipt
            .sampling_map()
            .map_err(|_| invalid("invalid retained sampling map"))?,
        // Relevance binds the canvas: an accepted request's context, which
        // includes the canvas, is unchanged since it was conditioned.
        content_aspect: Some([
            current.presentation_basis().width,
            current.presentation_basis().height,
        ]),
    };
    let assets = BTreeMap::from([
        (input.native_asset.clone(), native),
        (input.sampled_asset.clone(), sampled),
    ]);
    let operation = if request.target.repeats.is_empty() {
        Command::AcceptGeneratedHold {
            node: request.target.node.clone(),
            artifact: artifact.clone(),
            assets,
        }
    } else {
        let edit = ScopedNodeEdit::AcceptGeneratedHold {
            artifact: artifact.clone(),
            assets,
        };
        let requirements = current.scoped_edit_requirements(&request.target, &edit)?;
        // Fresh identities are deterministic for this captured commit so the
        // read-only preview and atomic write derive precisely the same clone.
        let namespace: String = Sha256::digest(input.new_revision.as_str().as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let identities = OccurrenceIdentities {
            nodes: (0..requirements.nodes)
                .map(|index| NodeId::new(format!("ai-{namespace}-node-{index}")))
                .collect::<Result<_, _>>()?,
            marks: (0..requirements.marks)
                .map(|index| MarkId::new(format!("ai-{namespace}-mark-{index}")))
                .collect::<Result<_, _>>()?,
        };
        Command::EditScoped {
            target: request.target.clone(),
            edit,
            identities,
        }
    };
    let command = CommandRequest {
        project_id: current.project_id().clone(),
        expected_revision: input.expected_revision.clone(),
        new_revision: input.new_revision.clone(),
        command: operation,
    };
    let stored = crate::generation::read_stored_request(connection, &input.identity.request_id)?
        .ok_or_else(|| invalid("qualified acceptance has no retained request"))?;
    let origin = crate::generation_origins::prepare_acceptance_origin(
        connection,
        &stored,
        &input.identity,
        &artifact,
        &input.new_revision,
    )?;
    let mut plan =
        prepare_admitted_command(connection, documents, &command, Some((&artifact, &origin)))?;
    plan.acceptance = Some(input.identity.clone());
    Ok(plan)
}

pub(crate) fn asset_record(
    object: &GeneratedObjectRef,
    video: &VideoSpec,
    span: SourceSpan,
) -> AssetRecord {
    AssetRecord {
        source_qualification: None,
        label: format!("Generated {}", object.content().digest()),
        content_hash: object.content().to_string(),
        video: Some(span),
        audio: None,
        still_image: false,
        frame_count: Some(video.frames()),
    }
}

/// Requests currently bind a structural Hold ID, without an occurrence path.
/// Require exactly one effective occurrence. A default subtree completely hidden
/// by overrides is unreachable; sparse override subtrees each count once per
/// ancestor occurrence. No Repeat is expanded. Retime ancestry is conservatively
/// rejected because its crop can omit or partially expose the bound Hold.
pub(crate) fn require_single_generation_occurrence(
    document: &ProjectDocument,
    hold: &NodeId,
) -> Result<(), StoreError> {
    let parents: BTreeMap<_, _> = document
        .nodes()
        .keys()
        .flat_map(|id| document.children(id).map(move |child| (child, id)))
        .collect();
    let mut child = hold;
    let mut occurrences = 1_u32;
    while child != document.root() {
        let parent = parents
            .get(child)
            .ok_or_else(|| invalid("generation Hold is unreachable"))?;
        match &document.nodes()[*parent].kind {
            NodeKind::Repeat {
                child: default,
                iterations,
                ..
            } if child == default => {
                let overrides = document
                    .overrides()
                    .get(*parent)
                    .map_or(0, |entries| entries.len());
                let defaults = iterations
                    .len()
                    .checked_sub(
                        u32::try_from(overrides).map_err(|_| invalid("invalid override count"))?,
                    )
                    .ok_or_else(|| invalid("invalid override count"))?;
                occurrences = occurrences.saturating_mul(defaults).min(2);
            }
            NodeKind::Repeat { iterations, .. } => {
                if let Some((iteration, _)) = document
                    .gap_overrides()
                    .get(*parent)
                    .and_then(|entries| entries.iter().find(|(_, root)| *root == child))
                    && iterations
                        .position(iteration)
                        .is_none_or(|position| position + 1 == iterations.len())
                {
                    occurrences = 0;
                }
            }
            NodeKind::Retime { .. } => {
                return Err(invalid(
                    "generation requires a concrete Hold outside a Retime crop",
                ));
            }
            _ => {}
        }
        child = parent;
    }
    if occurrences != 1 {
        return Err(invalid(
            "generation requires an isolated single concrete Hold occurrence",
        ));
    }
    Ok(())
}

fn invalid(message: &str) -> StoreError {
    StoreError::GenerationAcceptance(message.into())
}

#[cfg(test)]
mod gap_occurrence_tests {
    use super::*;
    use deadpan_core::{ColorPolicy, FrameRate, PresentationBasis, ProjectId};
    use serde_json::json;

    #[test]
    fn dormant_gap_is_not_an_effective_generation_target() {
        let blank = ProjectDocument::new(
            ProjectId::new("gap-generation").unwrap(),
            RevisionId::new("initial").unwrap(),
            PresentationBasis {
                width: 16,
                height: 16,
                frame_rate: FrameRate::new(30, 1).unwrap(),
                color_policy: ColorPolicy::SdrRec709,
            },
            NodeId::new("root").unwrap(),
        )
        .unwrap();
        let mut wire = serde_json::to_value(blank).unwrap();
        let hold = json!({"label":"Hold","kind":{"type":"hold","recipe":{
            "duration":2,"video":{"type":"background"},"audio":{"type":"silence"}
        }}});
        wire["nodes"] = json!({
            "root":{"label":"Root","kind":{"type":"sequence","children":["repeat"]}},
            "repeat":{"label":"Repeat","kind":{"type":"repeat","child":"child",
                "iterations":{"runs":[{"allocation":"plays","first":0,"count":2}]},"gap":null}},
            "child":hold,"gap":hold,
        });
        wire["gap_overrides"] =
            json!({"repeat":[{"iteration":{"allocation":"plays","ordinal":1},"root":"gap"}]});
        let dormant = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let gap = NodeId::new("gap").unwrap();
        assert!(require_single_generation_occurrence(&dormant, &gap).is_err());
        wire["gap_overrides"]["repeat"][0]["iteration"]["ordinal"] = json!(0);
        let active = ProjectDocument::from_json(&wire.to_string()).unwrap();
        assert!(require_single_generation_occurrence(&active, &gap).is_ok());
    }
}
