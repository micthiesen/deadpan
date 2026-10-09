//! Exact provider-clock neighborhoods from the canonical picture walker.

use deadpan_core::{BoundaryQueryLimits, Cutaway, CutawayFit, ExactRatio, FrameRate, NodeId};
use serde::Serialize;
use std::sync::Arc;

use super::{DefinitionPictureSample, PictureBudget};
use crate::{LookupStats, PlanError, RenderPlan};

#[path = "picture_coverage_selection.rs"]
mod selection;

/// Aggregate limit shared by every context in a batch, not a per-sample limit.
pub const MAX_DEFINITION_PICTURE_SPANS: usize = 512;

/// Raw provider-coordinate change per definition frame. Source endpoint policy
/// and selection still apply; a host must use the measured index to determine
/// which pictures the raw clock actually displays. Negative source slopes are
/// legal for Reverse Holds and bouncing cutaways. Editorial framing and captions
/// are intentionally not described by this clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", content = "rate", rename_all = "snake_case")]
pub enum PictureClockSlope {
    SourceTicks(ExactRatio),
    AcceptedFrames(ExactRatio),
    Constant,
}

/// A nonempty half-open definition interval. At position `p`, the raw provider
/// clock is the clock in `start.picture` plus `(p-start.position)*clock`.
/// No span crosses a structural provider/play/gap/cutaway or clock-slope seam,
/// even when the pictures on either side happen to be equal. The start sample's
/// framing/captions are facts at that one point, not constant span properties.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DefinitionPictureSpan {
    pub start: DefinitionPictureSample,
    pub end_exclusive: ExactRatio,
    pub clock: PictureClockSlope,
    #[serde(skip)]
    witness: Option<Arc<DefinitionSpanWitness>>,
}

/// The retained snapshot is charged under the same aggregate metadata ledger
/// as ordinary samples. It proves the whole original interval without another
/// canonical walk for each dependency in a replacement closure.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DefinitionSpanWitness {
    start: DefinitionPictureSample,
    end_exclusive: ExactRatio,
    clock: PictureClockSlope,
}

impl DefinitionPictureSpan {
    /// An inspected descriptor or zero-length terminal, without canonical span
    /// authority. Ordinal observation is available; provider substitution is not.
    pub fn observation(
        start: DefinitionPictureSample,
        end_exclusive: ExactRatio,
        clock: PictureClockSlope,
    ) -> Self {
        Self {
            start,
            end_exclusive,
            clock,
            witness: None,
        }
    }
}

/// Complete coverage of a closed definition interval: consecutive half-open
/// spans followed by the exact terminal sample. Equal endpoints yield no spans.
/// These are structural facts, not decoded pictures or same-shot qualification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DefinitionPictureCoverage {
    pub spans: Vec<DefinitionPictureSpan>,
    pub terminal: DefinitionPictureSample,
    pub lookup: LookupStats,
}

impl RenderPlan {
    /// Substitute the saved deterministic provider for one complete canonical
    /// Generated Hold interval. The exact start, endpoint and affine clock must
    /// still match this plan's issued coverage. Cutaways, implicit gaps and other
    /// providers return None. Public observations grant no substitution proof.
    pub fn definition_hold_fallback_span(
        &self,
        span: &DefinitionPictureSpan,
    ) -> Result<Option<DefinitionPictureSpan>, PlanError> {
        let witness = span.witness.as_ref().ok_or(PlanError::InvalidPlan(
            "picture span has no canonical interval evidence",
        ))?;
        if span.start != witness.start
            || span.end_exclusive != witness.end_exclusive
            || span.clock != witness.clock
        {
            return Err(PlanError::InvalidPlan(
                "picture span differs from its canonical interval evidence",
            ));
        }
        // Value equality does not grant an Arc identity. Validate the private
        // original too, so an equal sample from another compilation cannot
        // transplant its start onto this interval's retained evidence.
        self.definition_hold_fallback_picture(&witness.start)?;
        let Some(picture) = self.definition_hold_fallback_picture(&span.start)? else {
            return Ok(None);
        };
        let mut start = span.start.clone();
        start.picture = picture;
        start.hold_provider = None;
        Ok(Some(DefinitionPictureSpan::observation(
            start,
            span.end_exclusive,
            PictureClockSlope::Constant,
        )))
    }

    /// Enumerate only the requested definition interval, including dormant
    /// definitions, without expanding outer Repeat occurrences. Both endpoints
    /// must be valid picture coordinates, strictly before the definition end.
    /// Work, retained metadata (64 MiB), and span count (512) are all bounded.
    pub fn definition_picture_coverage(
        &self,
        definition: &NodeId,
        start: ExactRatio,
        end: ExactRatio,
        limits: BoundaryQueryLimits,
    ) -> Result<DefinitionPictureCoverage, PlanError> {
        let definition = *self
            .by_id
            .get(definition)
            .ok_or_else(|| PlanError::InvalidPictureDefinition(definition.clone()))?;
        self.definition_picture_coverage_with_budget(
            definition,
            start,
            end,
            &mut PictureBudget::for_context(limits),
        )
    }

    pub(in crate::plan) fn definition_picture_coverage_with_budget(
        &self,
        definition: usize,
        start: ExactRatio,
        end: ExactRatio,
        budget: &mut PictureBudget,
    ) -> Result<DefinitionPictureCoverage, PlanError> {
        if start.compare(end).is_gt() {
            return Err(PlanError::InvalidPlan(
                "picture coverage interval is reversed",
            ));
        }
        let before = budget.lookup;
        budget.reserve_metadata(std::mem::size_of::<DefinitionPictureCoverage>())?;
        // Validate the terminal before retaining any spans, including a bad
        // right endpoint which a half-open traversal would otherwise miss.
        let terminal = self.sample_definition_picture(definition, end, true, budget)?;
        let mut position = start;
        let mut spans = Vec::new();
        while position.compare(end).is_lt() {
            let left = budget
                .retained_spans_left
                .as_mut()
                .ok_or(PlanError::InvalidPlan(
                    "picture coverage requires a retained-result budget",
                ))?;
            *left = left
                .checked_sub(1)
                .ok_or(PlanError::PictureQueryLimit("picture spans"))?;
            budget.reserve_metadata(std::mem::size_of::<DefinitionPictureSpan>())?;
            let mut continuity = PictureContinuity::new(position, end);
            let sample = self.sample_definition_picture_with_continuity(
                definition,
                position,
                true,
                budget,
                Some(&mut continuity),
            )?;
            if !continuity.end.compare(position).is_gt() {
                return Err(PlanError::InvalidPlan("picture coverage did not advance"));
            }
            position = continuity.end;
            budget.reserve_metadata(
                std::mem::size_of::<DefinitionSpanWitness>() + 2 * std::mem::size_of::<usize>(),
            )?;
            budget.retain_sample(&sample)?;
            let witness = Arc::new(DefinitionSpanWitness {
                start: sample.clone(),
                end_exclusive: position,
                clock: continuity.clock,
            });
            // Exact reservation avoids uncharged geometric Vec spare capacity.
            spans.reserve_exact(1);
            spans.push(DefinitionPictureSpan {
                start: sample,
                end_exclusive: position,
                clock: continuity.clock,
                witness: Some(witness),
            });
        }
        Ok(DefinitionPictureCoverage {
            spans,
            terminal,
            lookup: budget.since(before),
        })
    }
}

/// Optional companion to the ordinary walker. All branch selection and provider
/// pictures still come from that walker. This only bounds the right neighborhood
/// of its chosen branch and carries its positive definition-to-local slope.
pub(in crate::plan) struct PictureContinuity {
    position: ExactRatio,
    pub(in crate::plan) end: ExactRatio,
    local_rate: ExactRatio,
    pub(in crate::plan) clock: PictureClockSlope,
}

impl PictureContinuity {
    fn new(position: ExactRatio, end: ExactRatio) -> Self {
        Self {
            position,
            end,
            local_rate: ExactRatio::integer(1),
            clock: PictureClockSlope::Constant,
        }
    }

    pub(in crate::plan) fn limit(
        &mut self,
        local: ExactRatio,
        end: ExactRatio,
    ) -> Result<(), PlanError> {
        if !end.compare(local).is_gt() {
            return Err(PlanError::InvalidPlan(
                "nonpositive picture validity neighborhood",
            ));
        }
        let end = self
            .position
            .checked_add(end.checked_sub(local)?.checked_div(self.local_rate)?)?;
        if end.compare(self.end).is_lt() {
            self.end = end;
        }
        Ok(())
    }

    pub(in crate::plan) fn retime(&mut self, scale: ExactRatio) -> Result<(), PlanError> {
        if !scale.compare(ExactRatio::ZERO).is_gt() {
            return Err(PlanError::InvalidPlan(
                "picture Retime scale must be positive",
            ));
        }
        self.local_rate = self.local_rate.checked_mul(scale)?;
        Ok(())
    }

    pub(in crate::plan) fn source(&mut self, rate: ExactRatio) -> Result<(), PlanError> {
        self.clock = PictureClockSlope::SourceTicks(rate.checked_mul(self.local_rate)?);
        Ok(())
    }

    pub(in crate::plan) fn accepted(&mut self) {
        self.clock = PictureClockSlope::AcceptedFrames(self.local_rate);
    }

    /// The caller has selected this cutaway using canonical range precedence.
    /// Returns whether it currently supplies a picture; Gap's exhausted tail
    /// must fall through to the host while still retaining the range-end seam.
    pub(in crate::plan) fn cutaway(
        &mut self,
        cutaway: &Cutaway,
        local: ExactRatio,
        rate: FrameRate,
    ) -> Result<bool, PlanError> {
        self.limit(local, ExactRatio::integer(cutaway.range.end().0))?;
        if cutaway.removed {
            return Ok(true);
        }
        let base = cutaway.selection.start().time_base;
        let ticks_per_frame = ExactRatio::new(
            i128::from(rate.denominator()) * i128::from(base.denominator()),
            i128::from(rate.numerator()) * i128::from(base.numerator()),
        )?;
        let length = cutaway
            .selection
            .end()
            .ticks
            .checked_sub(cutaway.selection.start().ticks)?;
        let duration = length.checked_div(ticks_per_frame)?;
        let start = ExactRatio::integer(cutaway.range.start().0);
        let offset = local.checked_sub(start)?;
        let pass = offset.checked_div(duration)?.floor();
        let exhausted = !offset.compare(duration).is_lt();
        match cutaway.fit {
            CutawayFit::Gap if exhausted => Ok(false),
            CutawayFit::Hold if exhausted => {
                self.source(ExactRatio::ZERO)?;
                Ok(true)
            }
            CutawayFit::Gap | CutawayFit::Hold => {
                self.limit(local, start.checked_add(duration)?)?;
                self.source(ticks_per_frame)?;
                Ok(true)
            }
            CutawayFit::Loop | CutawayFit::Bounce => {
                let next_pass = ExactRatio::new(
                    pass.checked_add(1)
                        .ok_or(deadpan_core::TimeError::Overflow)?,
                    1,
                )?;
                self.limit(local, start.checked_add(duration.checked_mul(next_pass)?)?)?;
                let reverse = cutaway.fit == CutawayFit::Bounce && pass % 2 != 0;
                self.source(if reverse {
                    ExactRatio::ZERO.checked_sub(ticks_per_frame)?
                } else {
                    ticks_per_frame
                })?;
                Ok(true)
            }
        }
    }

    pub(in crate::plan) fn original_hold(
        &mut self,
        local: ExactRatio,
        start: ExactRatio,
        end: ExactRatio,
        origin: ExactRatio,
        ticks_per_frame: ExactRatio,
        reverse: bool,
    ) -> Result<(), PlanError> {
        let (enter, leave) = if reverse {
            (origin.checked_sub(end)?, origin.checked_sub(start)?)
        } else {
            (start.checked_sub(origin)?, end.checked_sub(origin)?)
        };
        let enter = enter.checked_div(ticks_per_frame)?;
        let leave = leave.checked_div(ticks_per_frame)?;
        if local.compare(enter).is_lt() {
            self.limit(local, enter)?;
            self.source(ExactRatio::ZERO)
        } else if local.compare(leave).is_lt() {
            self.limit(local, leave)?;
            self.source(if reverse {
                ExactRatio::ZERO.checked_sub(ticks_per_frame)?
            } else {
                ticks_per_frame
            })
        } else {
            self.source(ExactRatio::ZERO)
        }
    }
}
