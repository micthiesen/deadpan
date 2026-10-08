//! Read-only derivation of accepted providers invalidated by final raw inputs.
//!
//! One compiled plan supplies canonical authored addresses and exact endpoint
//! witnesses. Decisions use the final provider assignment, including the case
//! where reverting one Hold makes another accepted Hold valid again. No media,
//! sidecars, requests, history, or intent state are opened or changed here.

use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    sync::Arc,
};

use deadpan_core::{
    AcceptedGeneration, BoundaryQueryLimits, BoundaryReplacement, GeneratedArtifact, HoldVideo,
    NodeId, NodeKind, ProjectDocument, ScopedNodeTarget, ValidatedDocument,
};
use deadpan_plan::{DefinitionPictureSample, RenderPlan};
use rusqlite::Connection;
use serde::Serialize;

use crate::StoreError;
use crate::generation_intents::IntentCause;
use crate::generation_origins::{AcceptedOriginReceipt, GenerationInputBinding};
use crate::generation_pictures::{
    GenerationPictureIdentity, GenerationPictures, QualifiedGenerationPictures,
};

mod decisions;

// Independent of rendered duration and Repeat play count. Canonical address
// enumeration permits 400,000 Repeat steps; retained addresses, endpoint facts,
// and result metadata each share one 64 MiB admission budget. Origin receipts
// have a separate 64 MiB cache. Reserve the origin reader's full 256 KiB row
// limit before each new read, then charge its actual canonical size. Thus even
// a rejected last receipt cannot allocate beyond that cache's remaining budget.
const MAX_METADATA_BYTES: usize = deadpan_core::MAX_DOCUMENT_JSON_BYTES;
const MAX_ORIGIN_ROW_BYTES: usize = 256 * 1024;
const MAX_ARTIFACT_BYTES: usize = 16 * 1024;
const MAX_ADDRESS_STEPS: usize = 4 * deadpan_core::MAX_DOCUMENT_NODES;
const MAX_QUERY_WORK: usize = 64 * deadpan_core::MAX_DOCUMENT_NODES;
const QUERY_BATCH: usize = 64;

#[derive(Debug, Default)]
pub(crate) struct DerivedReplacements {
    pub(crate) entries: Vec<BoundaryReplacement>,
    pub(crate) births: Vec<BoundaryReplacementBirth>,
    /// Final inputs for candidates and exact retained live-intent nodes only.
    /// Canonical addresses may change when the base edit moves a whole branch.
    pub(crate) bindings: BTreeMap<ScopedNodeTarget, GenerationInputBinding>,
}

#[derive(Debug)]
pub(crate) struct BoundaryReplacementBirth {
    pub(crate) target: ScopedNodeTarget,
    pub(crate) origin: Arc<AcceptedOriginReceipt>,
    pub(crate) cause: IntentCause,
    pub(crate) input_binding: GenerationInputBinding,
}

/// `after_base` is the admitted base edit, before any derived provider writes.
/// Excluded targets remain constants. Fresh qualified acceptances participate
/// through their host-prepared ephemeral origin before it has been persisted.
/// Live intent is followed by exact authored NodeId, never copied into newly
/// cloned occurrence nodes.
/// Every missing required receipt, unsupported input, or exceeded bound fails
/// the whole derivation; no partial or guessed provider assignment escapes.
pub(crate) fn derive_with_bindings(
    connection: &Connection,
    after_base: &ValidatedDocument,
    excluded_targets: &BTreeSet<ScopedNodeTarget>,
    intent_nodes: &BTreeSet<NodeId>,
    ephemeral_origin: Option<&AcceptedOriginReceipt>,
) -> Result<DerivedReplacements, StoreError> {
    let pictures = QualifiedGenerationPictures::new(connection);
    after_base.scope(|| {
        derive_with_pictures(
            connection,
            after_base.document(),
            excluded_targets,
            intent_nodes,
            ephemeral_origin,
            &pictures,
        )
    })
}

struct Candidate<'a> {
    target: ScopedNodeTarget,
    accepted: &'a AcceptedGeneration,
    origin: Arc<AcceptedOriginReceipt>,
}

#[derive(Debug, Serialize)]
struct Endpoint {
    current: Option<GenerationPictureIdentity>,
    // Only a plan-branded Generated Hold witness can introduce a dependency.
    // Cutaways, implicit gaps, and explicitly accepted exclusions are constants.
    fallback: Option<(usize, GenerationPictureIdentity)>,
}

impl Endpoint {
    fn mismatch(&self, required: &Option<GenerationPictureIdentity>) -> decisions::MismatchTerm {
        if let Some((dependency, fallback)) = &self.fallback {
            decisions::MismatchTerm::from_matches(
                *dependency,
                &self.current == required,
                required.as_ref() == Some(fallback),
            )
        } else if &self.current == required {
            decisions::MismatchTerm::Same
        } else {
            decisions::MismatchTerm::Different
        }
    }

    fn final_identity(
        &self,
        decisions: &decisions::Decisions,
    ) -> Option<GenerationPictureIdentity> {
        if let Some((dependency, fallback)) = &self.fallback
            && decisions.nodes[*dependency].replace
        {
            return Some(fallback.clone());
        }
        self.current.clone()
    }
}

fn derive_with_pictures(
    connection: &Connection,
    document: &ProjectDocument,
    excluded_targets: &BTreeSet<ScopedNodeTarget>,
    intent_nodes: &BTreeSet<NodeId>,
    ephemeral_origin: Option<&AcceptedOriginReceipt>,
    pictures: &dyn GenerationPictures,
) -> Result<DerivedReplacements, StoreError> {
    if intent_nodes.len() > deadpan_core::MAX_DOCUMENT_NODES
        || excluded_targets.len() > deadpan_core::MAX_DOCUMENT_NODES
    {
        return Err(invalid(
            "boundary derivation target count exceeds its limit",
        ));
    }
    let plan = RenderPlan::compile(document).map_err(invalid)?;
    let all_targets = plan
        .authored_hold_targets(BoundaryQueryLimits {
            max_scopes: deadpan_core::MAX_DOCUMENT_NODES,
            max_comparisons: MAX_ADDRESS_STEPS,
        })
        .map_err(invalid)?;
    let mut metadata = MetadataBudget::new(MAX_METADATA_BYTES);
    metadata.charge(&all_targets)?;
    let mut queries_left = BoundaryQueryLimits {
        max_scopes: MAX_QUERY_WORK - all_targets.len(),
        max_comparisons: MAX_QUERY_WORK
            - all_targets
                .iter()
                .map(|target| target.repeats.len())
                .sum::<usize>(),
    };
    let mut origins = OriginCache::new(MAX_METADATA_BYTES);
    if let Some(receipt) = ephemeral_origin {
        origins.seed(receipt)?;
    }
    let mut candidates = Vec::new();
    let mut by_node = BTreeMap::new();
    let mut targets = Vec::new();
    for target in all_targets {
        let NodeKind::Hold { recipe } = &document.nodes()[&target.node].kind else {
            return Err(invalid(
                "canonical authored Hold enumeration returned another node kind",
            ));
        };
        let eligible = if let HoldVideo::Generated { accepted } = &recipe.video {
            if !excluded_targets.contains(&target) {
                let origin = origins.read(connection, &accepted.artifact)?;
                metadata.charge(&target)?;
                by_node.insert(target.node.clone(), candidates.len());
                candidates.push(Candidate {
                    target: target.clone(),
                    accepted,
                    origin,
                });
                true
            } else {
                false
            }
        } else {
            false
        };
        if eligible || intent_nodes.contains(&target.node) {
            targets.push(target);
        }
    }
    // The full address vector above is now dropped. Batches also discard the
    // plan's framing/caption/witness payload after extracting tiny raw facts.
    let mut endpoints = BTreeMap::new();
    for batch in targets.chunks(QUERY_BATCH) {
        let boundaries = plan
            .scoped_hold_boundaries_batch(batch, queries_left)
            .map_err(invalid)?;
        for boundary in boundaries {
            queries_left.max_scopes = queries_left
                .max_scopes
                .checked_sub(boundary.lookup.visited_nodes)
                .ok_or_else(|| invalid("boundary node work exceeded its aggregate budget"))?;
            queries_left.max_comparisons = queries_left
                .max_comparisons
                .checked_sub(boundary.lookup.sequence_comparisons)
                .and_then(|left| left.checked_sub(boundary.lookup.iteration_run_comparisons))
                .ok_or_else(|| invalid("boundary comparison work exceeded its aggregate budget"))?;
            let pair = [
                endpoint(&plan, document, boundary.left.as_ref(), &by_node, pictures)?,
                endpoint(&plan, document, boundary.right.as_ref(), &by_node, pictures)?,
            ];
            metadata.charge(&pair)?;
            endpoints.insert(boundary.target.node, pair);
        }
    }
    let equations: Vec<_> = candidates
        .iter()
        .map(|candidate| {
            let pair = &endpoints[&candidate.target.node];
            let required = candidate.origin.input_binding();
            [
                pair[0].mismatch(&required.left),
                pair[1].mismatch(&required.right),
            ]
        })
        .collect();
    let decisions =
        decisions::decide(&equations, decisions::DecisionLimits::default()).map_err(invalid)?;
    let cycle_causes: Vec<_> = decisions
        .cyclic_groups
        .iter()
        .map(|members| {
            for &member in members {
                metadata.charge(&candidates[member].target)?;
            }
            let targets: Vec<_> = members
                .iter()
                .map(|&member| candidates[member].target.clone())
                .collect();
            IntentCause::cyclic_group(&targets)
        })
        .collect::<Result<_, _>>()?;
    let mut result = DerivedReplacements::default();
    for target in targets {
        let NodeKind::Hold { recipe } = &document.nodes()[&target.node].kind else {
            return Err(invalid("input binding target is no longer a Hold"));
        };
        let pair = &endpoints[&target.node];
        let binding = GenerationInputBinding {
            duration: recipe.duration,
            frame_rate: document.presentation_basis().frame_rate,
            canvas: [
                document.presentation_basis().width,
                document.presentation_basis().height,
            ],
            left: pair[0].final_identity(&decisions),
            right: pair[1].final_identity(&decisions),
        };
        metadata.charge(&binding)?;
        result.bindings.insert(target, binding);
    }
    for (candidate, decision) in candidates.into_iter().zip(decisions.nodes) {
        if !decision.replace {
            continue;
        }
        let cause = match decision.reason {
            decisions::DecisionReason::BoundaryChanged => IntentCause::SourceBoundaryChanged,
            decisions::DecisionReason::CyclicBoundaryDependencies { group } => {
                cycle_causes[group].clone()
            }
            decisions::DecisionReason::InputsMatch => {
                return Err(invalid(
                    "matching accepted input was selected for replacement",
                ));
            }
        };
        // Account for the actual extra output copies before allocating them.
        metadata.charge(&candidate.target)?;
        metadata.charge(&candidate.target)?;
        metadata.charge(candidate.accepted)?;
        let input_binding = &result.bindings[&candidate.target];
        metadata.charge(input_binding)?;
        metadata.charge(&cause)?;
        result.entries.push(BoundaryReplacement {
            target: candidate.target.clone(),
            accepted: Box::new(candidate.accepted.clone()),
        });
        result.births.push(BoundaryReplacementBirth {
            target: candidate.target,
            origin: candidate.origin,
            cause,
            input_binding: input_binding.clone(),
        });
    }
    Ok(result)
}

fn endpoint(
    plan: &RenderPlan,
    document: &ProjectDocument,
    sample: Option<&DefinitionPictureSample>,
    by_node: &BTreeMap<NodeId, usize>,
    pictures: &dyn GenerationPictures,
) -> Result<Endpoint, StoreError> {
    let Some(sample) = sample else {
        return Ok(Endpoint {
            current: None,
            fallback: None,
        });
    };
    let current = Some(pictures.identity(document, &sample.picture)?);
    let fallback = if let Some(&dependency) = by_node.get(&sample.instance.node) {
        plan.definition_hold_fallback_picture(sample)
            .map_err(invalid)?
            .map(|picture| {
                pictures
                    .identity(document, &picture)
                    .map(|identity| (dependency, identity))
            })
            .transpose()?
    } else {
        None
    };
    Ok(Endpoint { current, fallback })
}

struct OriginCache {
    values: BTreeMap<Vec<u8>, Arc<AcceptedOriginReceipt>>,
    budget: MetadataBudget,
}

impl OriginCache {
    fn new(maximum: usize) -> Self {
        Self {
            values: BTreeMap::new(),
            budget: MetadataBudget::new(maximum),
        }
    }

    fn seed(&mut self, receipt: &AcceptedOriginReceipt) -> Result<(), StoreError> {
        MetadataBudget::new(MAX_ARTIFACT_BYTES).charge(receipt.artifact())?;
        MetadataBudget::new(MAX_ORIGIN_ROW_BYTES).charge(receipt)?;
        let key = serde_json::to_vec(receipt.artifact())?;
        self.budget.charge(&key)?;
        self.budget.charge(receipt)?;
        self.values.insert(key, Arc::new(receipt.clone()));
        Ok(())
    }

    fn read(
        &mut self,
        connection: &Connection,
        artifact: &GeneratedArtifact,
    ) -> Result<Arc<AcceptedOriginReceipt>, StoreError> {
        MetadataBudget::new(MAX_ARTIFACT_BYTES).charge(artifact)?;
        let key = serde_json::to_vec(artifact)?;
        if let Some(receipt) = self.values.get(&key) {
            return Ok(Arc::clone(receipt));
        }
        if self.budget.remaining < MAX_ORIGIN_ROW_BYTES + key.len() {
            return Err(invalid(
                "accepted origin cache cannot admit another bounded receipt",
            ));
        }
        let receipt = crate::generation_origins::read(connection, artifact)?.ok_or_else(|| {
            invalid("accepted generated Hold has no immutable accepted-origin receipt")
        })?;
        self.budget.remaining -= key.len();
        self.budget.charge(&receipt)?;
        let receipt = Arc::new(receipt);
        self.values.insert(key, Arc::clone(&receipt));
        Ok(receipt)
    }
}

struct MetadataBudget {
    remaining: usize,
}

impl MetadataBudget {
    fn new(maximum: usize) -> Self {
        Self { remaining: maximum }
    }

    fn charge(&mut self, value: &impl Serialize) -> Result<(), StoreError> {
        serde_json::to_writer(&mut *self, value).map_err(|error| {
            invalid(format!(
                "boundary metadata byte limit or encoding failure: {error}"
            ))
        })
    }
}

impl io::Write for MetadataBudget {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.remaining {
            return Err(io::Error::other("metadata byte budget exhausted"));
        }
        self.remaining -= bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn invalid(reason: impl std::fmt::Display) -> StoreError {
    StoreError::GenerationPlan(format!("accepted boundary derivation: {reason}"))
}

#[cfg(test)]
mod tests;
