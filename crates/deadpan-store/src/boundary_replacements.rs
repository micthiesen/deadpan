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
    AcceptedGeneration, BoundaryQueryLimits, BoundaryReplacement, FrameDuration, GeneratedArtifact,
    HoldVideo, NodeId, NodeKind, ProjectDocument, ScopedNodeTarget, ValidatedDocument,
};
use deadpan_plan::{DefinitionPictureSample, RenderPlan, ScopedHoldContextRequest};
use rusqlite::Connection;
use serde::Serialize;

use crate::StoreError;
use crate::generation_inputs::{
    GenerationCaptureSpec, GenerationInputBinding, GenerationInputSettings, MAX_INPUT_BINDING_BYTES,
};
use crate::generation_intents::IntentCause;
use crate::generation_origins::AcceptedOriginReceipt;
use crate::generation_pictures::{
    GenerationPictureIdentity, GenerationPictures, QualifiedGenerationPictures,
};

mod decisions;
mod observation;

// Independent of rendered duration and Repeat play count. Canonical address
// enumeration permits 400,000 Repeat steps; retained addresses, endpoint facts,
// and result metadata share one 64 MiB admission budget with origin receipts.
// Reserve the origin reader's full row
// limit before each new read, then charge its actual canonical size. Thus even
// a rejected last receipt cannot allocate beyond that cache's remaining budget.
const MAX_METADATA_BYTES: usize = deadpan_core::MAX_DOCUMENT_JSON_BYTES;
const MAX_ORIGIN_ROW_BYTES: usize = crate::generation_origins::MAX_ROW_BYTES;
const MAX_ARTIFACT_BYTES: usize = 16 * 1024;
const MAX_ADDRESS_STEPS: usize = 4 * deadpan_core::MAX_DOCUMENT_NODES;
const MAX_QUERY_WORK: usize = 64 * deadpan_core::MAX_DOCUMENT_NODES;
const QUERY_BATCH: usize = 64;
const MAX_OBSERVATION_ATOMS: usize = 8 * deadpan_core::MAX_DOCUMENT_NODES;
const MAX_OBSERVATION_SPANS: usize = 8 * deadpan_core::MAX_DOCUMENT_NODES;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntentCaptureSettings {
    Resolved(GenerationInputSettings),
    /// Only Automatic may wait for its first endpoint-based resolution. A
    /// resolved operation never changes when later endpoints appear/disappear.
    Unresolved {
        preference: deadpan_jobs::GenerationModePreference,
        region: Option<deadpan_core::TargetId>,
    },
}

impl From<GenerationInputSettings> for IntentCaptureSettings {
    fn from(settings: GenerationInputSettings) -> Self {
        Self::Resolved(settings)
    }
}

#[derive(Debug, Default)]
pub(crate) struct DerivedReplacements {
    pub(crate) entries: Vec<BoundaryReplacement>,
    pub(crate) births: Vec<BoundaryReplacementBirth>,
    /// Final inputs for candidates and exact retained live-intent nodes only.
    /// Canonical addresses may change when the base edit moves a whole branch.
    pub(crate) bindings: BTreeMap<ScopedNodeTarget, GenerationInputBinding>,
    /// Only typed geometric context absence enters this map. Corrupt evidence,
    /// ownership failures and resource exhaustion still reject the transition.
    pub(crate) unavailable: BTreeMap<ScopedNodeTarget, GenerationInputSettings>,
}

#[derive(Debug)]
pub(crate) struct BoundaryReplacementBirth {
    pub(crate) target: ScopedNodeTarget,
    pub(crate) origin: Arc<AcceptedOriginReceipt>,
    pub(crate) cause: IntentCause,
    pub(crate) duration: FrameDuration,
    pub(crate) settings: GenerationInputSettings,
    pub(crate) input_binding: Option<GenerationInputBinding>,
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
    intent_nodes: &BTreeMap<NodeId, IntentCaptureSettings>,
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

fn derive_with_pictures(
    connection: &Connection,
    document: &ProjectDocument,
    excluded_targets: &BTreeSet<ScopedNodeTarget>,
    intent_nodes: &BTreeMap<NodeId, IntentCaptureSettings>,
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
    let remaining_work = MAX_QUERY_WORK
        - all_targets.len()
        - all_targets
            .iter()
            .map(|target| target.repeats.len())
            .sum::<usize>();
    let mut queries_left = BoundaryQueryLimits {
        max_scopes: remaining_work,
        max_comparisons: remaining_work,
    };
    let mut origins = OriginCache::new(MAX_METADATA_BYTES);
    if let Some(receipt) = ephemeral_origin {
        metadata.charge(receipt)?;
        metadata.charge(receipt.artifact())?;
        origins.seed(receipt)?;
    }
    let mut candidates = Vec::new();
    let mut by_node = BTreeMap::new();
    let mut targets = BTreeMap::new();
    let mut unresolved = BTreeMap::new();
    for target in all_targets {
        let NodeKind::Hold { recipe } = &document.nodes()[&target.node].kind else {
            return Err(invalid(
                "canonical authored Hold enumeration returned another node kind",
            ));
        };
        let settings = if let HoldVideo::Generated { accepted } = &recipe.video {
            if !excluded_targets.contains(&target) {
                let origin =
                    origins.read_shared(connection, &accepted.artifact, Some(&mut metadata))?;
                metadata.charge(&target)?;
                by_node.insert(target.node.clone(), candidates.len());
                let settings = origin.input_binding().settings();
                candidates.push(Candidate {
                    target: target.clone(),
                    accepted,
                    origin,
                });
                Some(settings)
            } else {
                None
            }
        } else {
            None
        };
        match settings
            .map(IntentCaptureSettings::Resolved)
            .or_else(|| intent_nodes.get(&target.node).cloned())
        {
            Some(IntentCaptureSettings::Resolved(settings)) => {
                metadata.charge(&(&settings.capture, &settings.region))?;
                targets.insert(target, settings);
            }
            Some(IntentCaptureSettings::Unresolved { preference, region }) => {
                if preference != deadpan_jobs::GenerationModePreference::Automatic {
                    return Err(invalid(
                        "only Automatic generation intent may have an unresolved operation",
                    ));
                }
                metadata.charge(&(&preference, &region))?;
                unresolved.insert(target, (preference, region));
            }
            None => {}
        }
    }
    // Endpoint presence is structural and independent of the final provider
    // assignment. Resolve it once on this same plan, under the same work ledger.
    let unresolved_targets: Vec<_> = unresolved.keys().cloned().collect();
    metadata.charge(&unresolved_targets)?;
    for batch in unresolved_targets.chunks(QUERY_BATCH) {
        for boundary in plan
            .scoped_hold_boundaries_batch(batch, query_limits(queries_left))
            .map_err(invalid)?
        {
            charge_query(&mut queries_left, boundary.lookup)?;
            charge_work(&mut queries_left, 1)?;
            let (preference, region) = unresolved
                .remove(&boundary.target)
                .ok_or_else(|| invalid("unresolved intent has no canonical settings"))?;
            if boundary.left.is_none() && boundary.right.is_none() {
                continue;
            }
            let capture = GenerationCaptureSpec::for_preference(
                preference,
                boundary.left.is_some(),
                boundary.right.is_some(),
            )?;
            let settings = GenerationInputSettings { capture, region };
            metadata.charge(&(&settings.capture, &settings.region))?;
            targets.insert(boundary.target, settings);
        }
    }
    // The full address vector above is now dropped. Batches also discard the
    // plan's framing/caption/witness payload after extracting tiny raw facts.
    let mut observations = BTreeMap::new();
    let mut unavailable = BTreeSet::new();
    let mut atoms_left = MAX_OBSERVATION_ATOMS;
    let mut spans_left = MAX_OBSERVATION_SPANS;
    let ordered: Vec<_> = targets.iter().collect();
    for batch in ordered.chunks(QUERY_BATCH) {
        let bridges: Vec<_> = batch
            .iter()
            .filter(|(_, settings)| settings.capture == GenerationCaptureSpec::Bridge)
            .map(|(target, _)| (*target).clone())
            .collect();
        for boundary in plan
            .scoped_hold_boundaries_batch(&bridges, query_limits(queries_left))
            .map_err(invalid)?
        {
            charge_query(&mut queries_left, boundary.lookup)?;
            reserve_observation(
                &metadata,
                &mut queries_left,
                &mut atoms_left,
                &mut spans_left,
                boundary.left.iter().chain(&boundary.right).count(),
                0,
            )?;
            let observation = observation::Observation::boundaries(
                &plan,
                document,
                &boundary,
                &targets[&boundary.target],
                &by_node,
                pictures,
            )?;
            metadata.charge(&observation)?;
            observations.insert(boundary.target.node, observation);
        }
        let extensions: Vec<_> = batch
            .iter()
            .filter_map(|(target, settings)| match settings.capture {
                GenerationCaptureSpec::Bridge => None,
                GenerationCaptureSpec::Extension {
                    direction,
                    native_rate,
                    context_frames,
                    ..
                } => Some(ScopedHoldContextRequest {
                    target: (*target).clone(),
                    direction,
                    native_rate,
                    frame_count: context_frames,
                }),
            })
            .collect();
        for observed in plan
            .scoped_hold_context_observations_batch(&extensions, query_limits(queries_left))
            .map_err(invalid)?
        {
            let context = match observed {
                deadpan_plan::ScopedHoldContextObservation::Available(context) => context,
                deadpan_plan::ScopedHoldContextObservation::Unavailable {
                    boundaries,
                    lookup,
                    ..
                } => {
                    charge_query(&mut queries_left, lookup)?;
                    reserve_observation(
                        &metadata,
                        &mut queries_left,
                        &mut atoms_left,
                        &mut spans_left,
                        0,
                        0,
                    )?;
                    unavailable.insert(boundaries.target.node);
                    continue;
                }
            };
            charge_query(&mut queries_left, context.lookup)?;
            let opposite = match context.direction {
                deadpan_core::ExtensionDirection::FromLeft => context.boundaries.right.is_some(),
                deadpan_core::ExtensionDirection::FromRight => context.boundaries.left.is_some(),
            };
            reserve_observation(
                &metadata,
                &mut queries_left,
                &mut atoms_left,
                &mut spans_left,
                context.pictures.len()
                    + usize::from(opposite)
                    + 2 * context.coverage.spans.len()
                    + 1,
                context.coverage.spans.len() + 1,
            )?;
            let observation = observation::Observation::context(
                &plan,
                document,
                &context,
                &targets[&context.boundaries.target],
                &by_node,
                pictures,
            )?;
            metadata.charge(&observation)?;
            observations.insert(context.boundaries.target.node, observation);
        }
    }
    let equations: Vec<_> = candidates
        .iter()
        .map(|candidate| match observations.get(&candidate.target.node) {
            Some(observed) => observed.terms(candidate.origin.input_binding()),
            None => vec![decisions::MismatchTerm::Different],
        })
        .collect();
    let decisions = decisions::decide(
        &equations,
        decisions::DecisionLimits {
            max_work: queries_left.max_scopes,
            ..decisions::DecisionLimits::default()
        },
    )
    .map_err(invalid)?;
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
    for (target, settings) in targets {
        let Some(observed) = observations.remove(&target.node) else {
            if !unavailable.remove(&target.node)
                || by_node
                    .get(&target.node)
                    .is_some_and(|&index| !decisions.nodes[index].replace)
            {
                return Err(invalid(
                    "input observation is absent without replacement authority",
                ));
            }
            result.unavailable.insert(target, settings);
            continue;
        };
        let binding = observed.resolve(&decisions);
        MetadataBudget::new(MAX_INPUT_BINDING_BYTES).charge(&binding)?;
        if let Some(&index) = by_node.get(&target.node)
            && !decisions.nodes[index].replace
            && !observation::matches_retained(&binding, candidates[index].origin.input_binding())
        {
            return Err(invalid(
                "retained accepted provider differs from its complete final inputs",
            ));
        }
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
        let input_binding = result.bindings.get(&candidate.target);
        metadata.charge(&input_binding)?;
        metadata.charge(&cause)?;
        result.entries.push(BoundaryReplacement {
            target: candidate.target.clone(),
            accepted: Box::new(candidate.accepted.clone()),
        });
        result.births.push(BoundaryReplacementBirth {
            duration: match &document.nodes()[&candidate.target.node].kind {
                NodeKind::Hold { recipe } => recipe.duration,
                _ => return Err(invalid("replacement target is no longer a Hold")),
            },
            settings: candidate.origin.input_binding().settings(),
            target: candidate.target,
            origin: candidate.origin,
            cause,
            input_binding: input_binding.cloned(),
        });
    }
    Ok(result)
}

fn charge_query(
    left: &mut BoundaryQueryLimits,
    lookup: deadpan_plan::LookupStats,
) -> Result<(), StoreError> {
    charge_work(
        left,
        lookup.visited_nodes + lookup.sequence_comparisons + lookup.iteration_run_comparisons,
    )
}

fn query_limits(left: BoundaryQueryLimits) -> BoundaryQueryLimits {
    // The planner has separate node and comparison counters. Capping each at
    // half the shared remainder also bounds their sum before any work starts.
    BoundaryQueryLimits {
        max_scopes: left.max_scopes / 2,
        max_comparisons: left.max_scopes / 2,
    }
}

fn charge_work(left: &mut BoundaryQueryLimits, work: usize) -> Result<(), StoreError> {
    let remaining = left
        .max_scopes
        .checked_sub(work)
        .ok_or_else(|| invalid("boundary input work exceeded its aggregate budget"))?;
    *left = BoundaryQueryLimits {
        max_scopes: remaining,
        max_comparisons: remaining,
    };
    Ok(())
}

fn reserve_observation(
    metadata: &MetadataBudget,
    work: &mut BoundaryQueryLimits,
    atoms: &mut usize,
    spans: &mut usize,
    count: usize,
    span_count: usize,
) -> Result<(), StoreError> {
    // Reserve before measured identities or their fallback copies allocate.
    // The binding and alternatives each fit the binding's individual bound.
    if metadata.remaining < 2 * MAX_INPUT_BINDING_BYTES {
        return Err(invalid(
            "boundary input metadata cannot admit another bounded observation",
        ));
    }
    *atoms = atoms
        .checked_sub(count + 1)
        .ok_or_else(|| invalid("boundary input atom budget exhausted"))?;
    *spans = spans
        .checked_sub(span_count)
        .ok_or_else(|| invalid("boundary support span budget exhausted"))?;
    // Capture, fallback witnesses, flattening, final substitution and the
    // independent retained comparison each visit a bounded number of atoms.
    // The solver consumes the same remaining ledger separately below.
    charge_work(work, 8 * (count + 1) + span_count)?;
    Ok(())
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

    #[cfg(test)]
    fn read(
        &mut self,
        connection: &Connection,
        artifact: &GeneratedArtifact,
    ) -> Result<Arc<AcceptedOriginReceipt>, StoreError> {
        self.read_shared(connection, artifact, None)
    }

    fn read_shared(
        &mut self,
        connection: &Connection,
        artifact: &GeneratedArtifact,
        mut shared: Option<&mut MetadataBudget>,
    ) -> Result<Arc<AcceptedOriginReceipt>, StoreError> {
        MetadataBudget::new(MAX_ARTIFACT_BYTES).charge(artifact)?;
        let key = serde_json::to_vec(artifact)?;
        if let Some(receipt) = self.values.get(&key) {
            return Ok(Arc::clone(receipt));
        }
        if self.budget.remaining < MAX_ORIGIN_ROW_BYTES + key.len()
            || shared
                .as_ref()
                .is_some_and(|budget| budget.remaining < MAX_ORIGIN_ROW_BYTES + key.len())
        {
            return Err(invalid(
                "accepted origin cache cannot admit another bounded receipt",
            ));
        }
        let receipt = crate::generation_origins::read(connection, artifact)?.ok_or_else(|| {
            invalid("accepted generated Hold has no immutable accepted-origin receipt")
        })?;
        self.budget.remaining -= key.len();
        self.budget.charge(&receipt)?;
        if let Some(budget) = shared.as_mut() {
            budget.remaining -= key.len();
            budget.charge(&receipt)?;
        }
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
