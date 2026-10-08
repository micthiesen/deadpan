//! Bounded temporal conditioning queries in the authored definition clock.

use deadpan_core::{
    BoundaryQueryLimits, ExactRatio, ExtensionDirection, FrameRate, InstancePath,
    MAX_DOCUMENT_DEPTH, ScopedNodeTarget,
};
use serde::{Deserialize, Serialize};

use super::{
    DefinitionPictureCoverage, DefinitionPictureSample, PictureBudget, ScopedHoldBoundaries,
};
use crate::{LookupStats, PlanError, RenderPlan};

/// Bounds for retained query results, independent of traversal work limits.
/// Provider capabilities may impose a smaller, measured context length.
pub const MAX_HOLD_CONTEXT_FRAMES: u32 = 64;
pub const MAX_HOLD_CONTEXT_BATCH: usize = 64;
const MAX_CONTEXT_METADATA_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScopedHoldContextRequest {
    pub target: ScopedNodeTarget,
    pub direction: ExtensionDirection,
    pub native_rate: FrameRate,
    pub frame_count: u32,
}

/// Chronological pictures at exact native-time spacing, ending at L or
/// beginning at R. These are structural queries, not decoded or same-shot
/// qualifications. Hosts must measure source ordinals and reject discontinuous
/// or unsupported context before supplying it to a model.
/// Relevance binds the measured raw picture identities and relative capture
/// times. Hashing this complete inspection value would incorrectly include
/// revision IDs, editorial framing and captions as model inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScopedHoldContext {
    pub boundaries: ScopedHoldBoundaries,
    pub direction: ExtensionDirection,
    pub native_rate: FrameRate,
    pub pictures: Vec<DefinitionPictureSample>,
    /// Every provider-contiguous affine interval between the first and last
    /// context coordinates, including transitions that fall between samples.
    pub coverage: DefinitionPictureCoverage,
    /// Total ancestry, boundary and context work for this request. Summing a
    /// batch's results gives its work against the single aggregate budget.
    pub lookup: LookupStats,
}

/// A valid authored target can lose its temporal neighbor through an ordinary
/// edit. These geometric absences differ from malformed input and query failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum HoldContextUnavailable {
    #[error("the requested extension anchor is absent")]
    MissingAnchor,
    #[error("the definition has insufficient temporal context for this extension")]
    InsufficientContext,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "availability", rename_all = "snake_case")]
pub enum ScopedHoldContextObservation {
    Available(Box<ScopedHoldContext>),
    Unavailable {
        boundaries: Box<ScopedHoldBoundaries>,
        reason: HoldContextUnavailable,
        lookup: LookupStats,
    },
}

impl ScopedHoldContextObservation {
    pub fn lookup(&self) -> LookupStats {
        match self {
            Self::Available(context) => context.lookup,
            Self::Unavailable { lookup, .. } => *lookup,
        }
    }

    fn into_available(self) -> Result<ScopedHoldContext, PlanError> {
        match self {
            Self::Available(context) => Ok(*context),
            Self::Unavailable { reason, .. } => Err(PlanError::HoldContextUnavailable(reason)),
        }
    }
}

impl RenderPlan {
    /// Query an effective scoped Hold, including a shared Play or dormant
    /// Default, without choosing a representative rendered occurrence. Context
    /// cannot cross the definition's edge or borrow an outer Repeat neighbor.
    pub fn scoped_hold_context(
        &self,
        request: &ScopedHoldContextRequest,
        limits: BoundaryQueryLimits,
    ) -> Result<ScopedHoldContext, PlanError> {
        self.scoped_hold_context_observation(request, limits)?
            .into_available()
    }

    /// Observe a valid effective target, retaining work accounting when an edit
    /// removes its anchor or leaves too little context within its definition.
    /// Invalid ownership, malformed evidence and exhausted budgets remain Err.
    pub fn scoped_hold_context_observation(
        &self,
        request: &ScopedHoldContextRequest,
        limits: BoundaryQueryLimits,
    ) -> Result<ScopedHoldContextObservation, PlanError> {
        validate_request(request)?;
        let mut budget = PictureBudget::for_context(limits);
        budget.reserve_metadata(std::mem::size_of::<ScopedHoldContext>())?;
        let before = budget.lookup;
        let boundaries = self.scoped_hold_boundaries_with_budget(&request.target, &mut budget)?;
        self.context_in(request, boundaries, before, &mut budget)
    }

    /// Query canonical, already-owned authored targets under one work budget.
    /// Order and duplicates are retained. Every frame and both seam queries
    /// consume the shared budget; no partial result escapes on an invalid
    /// address, unavailable context or exhausted budget. Use the single query
    /// for effective shared Play addresses that require isolation before edits.
    pub fn scoped_hold_context_batch(
        &self,
        requests: &[ScopedHoldContextRequest],
        limits: BoundaryQueryLimits,
    ) -> Result<Vec<ScopedHoldContext>, PlanError> {
        self.scoped_hold_context_observations_batch(requests, limits)?
            .into_iter()
            .map(ScopedHoldContextObservation::into_available)
            .collect()
    }

    /// Mixed available/unavailable observations under one shared ledger. A
    /// geometric absence consumes its actual query work and never poisons other
    /// independent targets. Every requested target must already be owned.
    pub fn scoped_hold_context_observations_batch(
        &self,
        requests: &[ScopedHoldContextRequest],
        limits: BoundaryQueryLimits,
    ) -> Result<Vec<ScopedHoldContextObservation>, PlanError> {
        let index = self
            .definition_index
            .as_ref()
            .ok_or(PlanError::AudioOnlyContext)?;
        if requests.len() > MAX_HOLD_CONTEXT_BATCH || requests.len() > limits.max_scopes {
            return Err(PlanError::PictureQueryLimit("context requests"));
        }
        for request in requests {
            validate_request(request)?;
        }
        let mut budget = PictureBudget::for_context(limits);
        budget.reserve_metadata(
            requests.len()
                * (std::mem::size_of::<ScopedHoldContext>()
                    + std::mem::size_of::<ScopedHoldContextObservation>()),
        )?;
        let mut results = Vec::with_capacity(requests.len());
        for request in requests {
            let before = budget.lookup;
            let hold =
                *self
                    .by_id
                    .get(&request.target.node)
                    .ok_or(PlanError::InvalidScopedHold(
                        "the Hold is absent from this revision",
                    ))?;
            if !matches!(
                &self.nodes[hold].kind,
                super::super::CompiledKind::Hold { .. }
            ) {
                return Err(PlanError::InvalidScopedHold(
                    "the target is not an authored Hold",
                ));
            }
            let address = index.validate(hold, &request.target, &mut budget)?;
            let boundaries = self.hold_boundaries_in(
                &request.target,
                hold,
                address.definition,
                address.start,
                before,
                &mut budget,
            )?;
            results.push(self.context_in(request, boundaries, before, &mut budget)?);
        }
        Ok(results)
    }

    fn context_in(
        &self,
        request: &ScopedHoldContextRequest,
        boundaries: ScopedHoldBoundaries,
        before: LookupStats,
        budget: &mut PictureBudget,
    ) -> Result<ScopedHoldContextObservation, PlanError> {
        let anchor = match request.direction {
            ExtensionDirection::FromLeft => boundaries.left.as_ref(),
            ExtensionDirection::FromRight => boundaries.right.as_ref(),
        };
        let Some(anchor) = anchor else {
            return Ok(ScopedHoldContextObservation::Unavailable {
                boundaries: Box::new(boundaries),
                reason: HoldContextUnavailable::MissingAnchor,
                lookup: budget.since(before),
            });
        };
        let project_rate = self.metadata.presentation_basis.frame_rate;
        // One native-frame duration expressed in project-frame coordinates.
        let step = ExactRatio::new(
            i128::from(project_rate.numerator()) * i128::from(request.native_rate.denominator()),
            i128::from(project_rate.denominator()) * i128::from(request.native_rate.numerator()),
        )?;
        let definition = *self
            .by_id
            .get(&boundaries.definition)
            .ok_or(PlanError::InvalidPlan(
                "conditioning definition disappeared from its immutable plan",
            ))?;
        let count = usize::try_from(request.frame_count)
            .map_err(|_| PlanError::PictureQueryLimit("context frames"))?;
        let extent = step.checked_mul(ExactRatio::integer(i64::from(request.frame_count - 1)))?;
        let far = match request.direction {
            ExtensionDirection::FromLeft => anchor.position.checked_sub(extent)?,
            ExtensionDirection::FromRight => anchor.position.checked_add(extent)?,
        };
        if far.compare_integer(0).is_lt()
            || !far
                .compare_integer(self.nodes[definition].inspection.duration.frames())
                .is_lt()
        {
            return Ok(ScopedHoldContextObservation::Unavailable {
                boundaries: Box::new(boundaries),
                reason: HoldContextUnavailable::InsufficientContext,
                lookup: budget.since(before),
            });
        }
        budget.reserve_metadata(count * std::mem::size_of::<DefinitionPictureSample>())?;
        let mut pictures = Vec::with_capacity(count);
        for frame in 0..request.frame_count {
            let distance = match request.direction {
                ExtensionDirection::FromLeft => request.frame_count - 1 - frame,
                ExtensionDirection::FromRight => frame,
            };
            if distance == 0 {
                // Already visited under this budget while resolving the seam.
                // The context retains a distinct owned copy of its metadata.
                budget.retain_sample(anchor)?;
                pictures.push(anchor.clone());
                continue;
            }
            let offset = step.checked_mul(ExactRatio::integer(i64::from(distance)))?;
            let position = match request.direction {
                ExtensionDirection::FromLeft => anchor.position.checked_sub(offset)?,
                ExtensionDirection::FromRight => anchor.position.checked_add(offset)?,
            };
            pictures.push(self.sample_definition_picture(definition, position, true, budget)?);
        }
        let coverage = self.definition_picture_coverage_with_budget(
            definition,
            pictures[0].position,
            pictures[count - 1].position,
            budget,
        )?;
        Ok(ScopedHoldContextObservation::Available(Box::new(
            ScopedHoldContext {
                boundaries,
                direction: request.direction,
                native_rate: request.native_rate,
                pictures,
                coverage,
                lookup: budget.since(before),
            },
        )))
    }
}

impl PictureBudget {
    pub(super) fn for_context(limits: BoundaryQueryLimits) -> Self {
        let mut budget = Self::new(limits);
        budget.retained_metadata_left = Some(MAX_CONTEXT_METADATA_BYTES);
        budget.retained_spans_left = Some(super::MAX_DEFINITION_PICTURE_SPANS);
        budget
    }

    pub(super) fn reserve_metadata(&mut self, bytes: usize) -> Result<(), PlanError> {
        if let Some(left) = &mut self.retained_metadata_left {
            *left = left
                .checked_sub(bytes)
                .ok_or(PlanError::PictureQueryLimit("context metadata"))?;
        }
        Ok(())
    }

    pub(in crate::plan) fn retain_sample(
        &mut self,
        sample: &DefinitionPictureSample,
    ) -> Result<(), PlanError> {
        if self.retained_metadata_left.is_none() {
            return Ok(());
        }
        // Vec capacities cover spare slots as well as used entries. The strings
        // here were cloned from validated identifiers, so their allocation is
        // exactly their length. Geometry, accepted artifacts and plan identity
        // tokens remain shared Arcs; cloning them allocates no object or strings.
        // Admission happens after one bounded walker result and before retaining
        // it. Peak query storage is this cumulative bound plus one transient
        // sample, itself bounded by the document's depth and per-node limits.
        let mut bytes = std::mem::size_of_val(sample);
        for length in [
            sample.project_id.as_str().len(),
            sample.revision_id.as_str().len(),
            sample.definition.as_str().len(),
            instance_bytes(&sample.instance),
        ] {
            bytes = bytes.saturating_add(length);
        }
        if let Some(gap) = &sample.gap_after {
            bytes = bytes.saturating_add(gap.allocation.as_str().len());
        }
        let asset = match &sample.picture {
            crate::Picture::Source { asset, .. }
            | crate::Picture::Still { asset }
            | crate::Picture::Freeze { asset, .. }
            | crate::Picture::Accepted { asset, .. } => Some(asset),
            crate::Picture::Background | crate::Picture::Blank => None,
        };
        if let Some(asset) = asset {
            bytes = bytes.saturating_add(asset.as_str().len());
        }
        bytes = bytes.saturating_add(
            sample
                .framing
                .capacity()
                .saturating_mul(std::mem::size_of::<crate::PictureFraming>()),
        );
        for framing in &sample.framing {
            bytes = bytes.saturating_add(instance_bytes(&framing.instance));
        }
        bytes = bytes.saturating_add(
            sample
                .captions
                .capacity()
                .saturating_mul(std::mem::size_of::<crate::PictureCaption>()),
        );
        for caption in &sample.captions {
            bytes = bytes.saturating_add(caption.text.capacity());
        }
        if let Some(witness) = &sample.hold_provider {
            bytes = bytes.saturating_add(instance_bytes(&witness.instance));
        }
        self.reserve_metadata(bytes)
    }

    pub(super) fn retain_boundaries(
        &mut self,
        boundaries: &ScopedHoldBoundaries,
    ) -> Result<(), PlanError> {
        if self.retained_metadata_left.is_none() {
            return Ok(());
        }
        // Inline samples are conservatively charged again; their dynamic data
        // was already reserved during the canonical walk.
        let mut bytes = std::mem::size_of_val(boundaries)
            .saturating_add(boundaries.project_id.as_str().len())
            .saturating_add(boundaries.revision_id.as_str().len())
            .saturating_add(boundaries.definition.as_str().len())
            .saturating_add(boundaries.target.node.as_str().len())
            .saturating_add(
                boundaries
                    .target
                    .repeats
                    .capacity()
                    .saturating_mul(std::mem::size_of::<deadpan_core::RepeatEditStep>()),
            );
        for step in &boundaries.target.repeats {
            bytes = bytes.saturating_add(step.repeat.as_str().len());
            if let deadpan_core::RepeatEditBranch::Play { iteration } = &step.branch {
                bytes = bytes.saturating_add(iteration.allocation.as_str().len());
            }
        }
        self.reserve_metadata(bytes)
    }
}

fn instance_bytes(instance: &InstancePath) -> usize {
    let mut bytes = instance.node.as_str().len().saturating_add(
        instance
            .repeats
            .capacity()
            .saturating_mul(std::mem::size_of::<deadpan_core::RepeatInstance>()),
    );
    for step in &instance.repeats {
        bytes = bytes
            .saturating_add(step.node.as_str().len())
            .saturating_add(step.iteration.allocation.as_str().len());
    }
    bytes
}

fn validate_request(request: &ScopedHoldContextRequest) -> Result<(), PlanError> {
    if request.frame_count == 0 || request.frame_count > MAX_HOLD_CONTEXT_FRAMES {
        return Err(PlanError::PictureQueryLimit("context frames"));
    }
    if request.target.repeats.len() > MAX_DOCUMENT_DEPTH {
        return Err(PlanError::InvalidScopedHold(
            "Repeat ancestry exceeds its bound",
        ));
    }
    Ok(())
}
