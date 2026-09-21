//! Explicit authored admission of a selected, retained generated bundle.
//!
//! Only identities and optimistic expectations come from the caller. The exact
//! command, asset records and sampling map come from persisted host evidence.
//! This is a bounded I/O API for a host service, never an audio/UI callback.

use std::collections::BTreeMap;

use deadpan_core::{
    AssetId, AssetRecord, Command, CommandRequest, EditTransaction, GeneratedArtifact,
    GeneratedObjectRef, NodeId, NodeKind, ProjectDocument, RevisionId, SourceSpan,
};
use deadpan_jobs::{JobState, MessageIdentity, Relevance, VideoSpec};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

use crate::generated_media::GeneratedMediaLimits;
use crate::generation::{ContextObservation, RelevancePlan};
use crate::generation_attempts::{
    BundleValidationReceipt, CandidateAvailability, read_attempt, read_request,
    validate_bundle_receipt,
};
use crate::{
    CommandPlan, CommitOutcome, ProjectStore, StoreError, prepare_admitted_command, read_snapshot,
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
        Ok(prepare_acceptance(&transaction, input)?.edit)
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
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let plan = prepare_acceptance(&transaction, input)?;
        if !relevance.observations.iter().any(|observation| {
            observation.request_id == input.identity.request_id
                && observation.after_context
                    == ContextObservation::Resolved(observation.binding.context_sha256.clone())
        }) {
            return Err(invalid(
                "acceptance must preserve the selected request's resolved context",
            ));
        }
        let outcome = write_command_plan(&transaction, plan, Some(relevance))?;
        transaction.commit()?;
        Ok(outcome)
    }
}

fn prepare_acceptance(
    connection: &Connection,
    input: &GenerationAcceptance,
) -> Result<CommandPlan, StoreError> {
    let current = read_snapshot(connection)?;
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
    validate_bundle_receipt(&request, attempt.declared_candidate.as_ref(), receipt)?;
    let evidence = receipt
        .admission()
        .ok_or_else(|| invalid("legacy bundle lacks measured spans and retained input evidence"))?;
    let plan = request
        .bridge_plan
        .as_ref()
        .ok_or_else(|| invalid("legacy request has no bridge plan"))?;
    let Some(NodeKind::Hold { recipe }) = current
        .nodes()
        .get(&request.binding.hold_id)
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
    require_single_generation_occurrence(&current, &request.binding.hold_id)?;
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
        sampling: plan
            .sampling_map()
            .map_err(|_| invalid("invalid retained sampling map"))?,
    };
    let command = CommandRequest {
        project_id: current.project_id().clone(),
        expected_revision: input.expected_revision.clone(),
        new_revision: input.new_revision.clone(),
        command: Command::AcceptGeneratedHold {
            node: request.binding.hold_id,
            artifact: artifact.clone(),
            assets: BTreeMap::from([
                (input.native_asset.clone(), native),
                (input.sampled_asset.clone(), sampled),
            ]),
        },
    };
    prepare_admitted_command(connection, &command, Some(&artifact))
}

fn asset_record(object: &GeneratedObjectRef, video: &VideoSpec, span: SourceSpan) -> AssetRecord {
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
