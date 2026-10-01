//! Pure complete-intent Source geometry. This grants neither media authority
//! nor admission for bindings, root sounds, marks or an overwrite overlay.

use serde::{Deserialize, Serialize};

use crate::source_edit::edge::{SourceEdgeCandidate, window_candidate};
use crate::source_edit::{Admission, admit, invalid};
use crate::{
    AssetId, DocumentError, EditError, EditErrorCode, ExactRatio, FrameDuration, FrameRange,
    MAX_DOCUMENT_NODES, NodeId, NodeKind, ProjectDocument, ProjectFrame, SourceEditWindow,
    SourceNode, SourceQualificationId, TimeError,
};

mod constraints;
use constraints::{Constraints, context_constraints};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SourceTrimPolicy {
    #[default]
    Ripple,
    Overwrite,
}

/// Bounded accepted values relative to one entry snapshot. Active inspection
/// mode is intentionally separate; changing tabs cannot change this value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct SourceTrimIntent {
    pub in_frames: i64,
    pub out_frames: i64,
    pub slip_frames: i64,
    pub roll_frames: i64,
    pub policy: SourceTrimPolicy,
}

impl SourceTrimIntent {
    pub const fn is_zero(self) -> bool {
        self.in_frames == 0
            && self.out_frames == 0
            && self.slip_frames == 0
            && self.roll_frames == 0
    }

    pub const fn value(self, control: SourceTrimControl) -> i64 {
        match control {
            SourceTrimControl::In => self.in_frames,
            SourceTrimControl::Out => self.out_frames,
            SourceTrimControl::Slip => self.slip_frames,
            SourceTrimControl::Roll => self.roll_frames,
        }
    }

    fn with_value(mut self, control: SourceTrimControl, value: i64) -> Self {
        match control {
            SourceTrimControl::In => self.in_frames = value,
            SourceTrimControl::Out => self.out_frames = value,
            SourceTrimControl::Slip => self.slip_frames = value,
            SourceTrimControl::Roll => self.roll_frames = value,
        }
        self
    }

    fn values(self) -> [i64; 4] {
        [
            self.in_frames,
            self.out_frames,
            self.slip_frames,
            self.roll_frames,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceTrimControl {
    In,
    Out,
    Slip,
    Roll,
}

impl SourceTrimControl {
    fn index(self) -> usize {
        match self {
            Self::In => 0,
            Self::Out => 1,
            Self::Slip => 2,
            Self::Roll => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceTrimGeometryOwner {
    Target,
    Right,
    Scope,
    Project,
    Intent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceTrimGeometryConstraint {
    PictureStart,
    PictureEnd,
    MinimumSelectedDuration,
    MinimumOutputDuration,
    PhysicalDuration,
    ScopeStart,
    ScopeEnd,
    ProjectDuration,
    IntegerValue,
}

/// A bound on the active accepted scalar, not on a hypothetical scalar edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SourceTrimGeometryLimit {
    pub value: ExactRatio,
    pub inclusive: bool,
    pub owner: SourceTrimGeometryOwner,
    pub constraint: SourceTrimGeometryConstraint,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SourceTrimRollAvailability {
    Available {
        right: NodeId,
    },
    Unavailable {
        captured_right: Option<NodeId>,
        error: EditError,
    },
}

/// Old physical-local allocation geometry for the later phase resolver. This
/// does not resolve a binding or assert that the endpoint lifecycle is authored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceTrimPhaseAnchor {
    Retained { allocation: FrameRange },
    SourceStart,
    SourceEnd,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceTrimOwnerGeometry {
    pub target: NodeId,
    pub physical_source: NodeId,
    pub asset: AssetId,
    pub qualification: SourceQualificationId,
    pub slot: usize,
    pub allocation_before: FrameRange,
    pub allocation_after: FrameRange,
    pub output_before: FrameRange,
    /// In overwrite, the right owner's Roll placement precedes structural
    /// overlay/cropping. The report does not admit that overlay.
    pub output_after: FrameRange,
    pub window_before: SourceEditWindow,
    pub window_after: SourceEditWindow,
    pub effective_before: SourceEditWindow,
    pub effective_after: SourceEditWindow,
    pub before: SourceNode,
    /// Media/window/duration only. Owner effects and audio clocks are untouched.
    pub after: SourceNode,
    pub physical_prefix: FrameDuration,
    pub needs_wrapper: bool,
    pub phase_anchor: SourceTrimPhaseAnchor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceTrimGeometry {
    pub parent: NodeId,
    pub intent: SourceTrimIntent,
    pub scope_before: FrameRange,
    pub scope_after: FrameRange,
    pub project_duration_before: FrameDuration,
    pub project_duration_after: FrameDuration,
    pub duration_delta_frames: i64,
    pub target: SourceTrimOwnerGeometry,
    /// Included when the captured literal neighbor is eligible, even at R=0.
    /// Absence/ineligibility never blocks an otherwise valid non-Roll intent.
    pub right: Option<SourceTrimOwnerGeometry>,
    pub roll_availability: SourceTrimRollAvailability,
    /// Minimum own-Source crop identities only; overwrite refinement is separate.
    pub required_source_wrappers: usize,
    pub requires_overwrite_overlay: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceTrimAdjustment {
    pub control: SourceTrimControl,
    pub previous_value: i64,
    pub requested_step: i64,
    pub requested_value: i64,
    pub applied_value: i64,
    pub minimum: SourceTrimGeometryLimit,
    pub maximum: SourceTrimGeometryLimit,
    pub minimum_value: i64,
    pub maximum_value: i64,
    pub clamp: Option<SourceTrimGeometryLimit>,
    pub geometry: SourceTrimGeometry,
}

impl ProjectDocument {
    /// Validate a complete accepted tuple exactly, without clamping or authoring.
    /// A policy toggle preserves all four values by calling this with its new
    /// policy; failure leaves the caller's accepted tuple unchanged.
    pub fn source_trim_geometry(
        &self,
        parent: &NodeId,
        node: &NodeId,
        captured_right: Option<&NodeId>,
        intent: SourceTrimIntent,
    ) -> Result<SourceTrimGeometry, EditError> {
        let context = Context::new(self, parent, node, captured_right)?;
        context.resolve(self, intent)
    }

    /// Clamp only this control's requested accepted value against the complete
    /// entry-bound proposal. No rejected overshoot survives in the returned intent.
    pub fn adjust_source_trim_geometry(
        &self,
        parent: &NodeId,
        node: &NodeId,
        captured_right: Option<&NodeId>,
        accepted: SourceTrimIntent,
        control: SourceTrimControl,
        step: i64,
    ) -> Result<SourceTrimAdjustment, EditError> {
        let context = Context::new(self, parent, node, captured_right)?;
        context.resolve(self, accepted)?;
        if control == SourceTrimControl::Roll {
            context.require_right()?;
        }
        let previous_value = accepted.value(control);
        let requested_value = previous_value
            .checked_add(step)
            .ok_or_else(|| EditError::from(DocumentError::from(TimeError::Overflow)))?;
        let constraints = context.constraints(accepted)?;
        let (minimum, maximum, minimum_value, maximum_value) = constraints
            .limits(accepted, control)
            .map_err(DocumentError::from)?;
        let applied_value = requested_value.clamp(minimum_value, maximum_value);
        let geometry = context.resolve(self, accepted.with_value(control, applied_value))?;
        Ok(SourceTrimAdjustment {
            control,
            previous_value,
            requested_step: step,
            requested_value,
            applied_value,
            minimum,
            maximum,
            minimum_value,
            maximum_value,
            clamp: if requested_value < minimum_value {
                Some(minimum)
            } else if requested_value > maximum_value {
                Some(maximum)
            } else {
                None
            },
            geometry,
        })
    }
}

struct Context<'a> {
    target: Admission<'a>,
    right: Option<Admission<'a>>,
    availability: SourceTrimRollAvailability,
    scope: FrameRange,
    total: FrameDuration,
}

impl<'a> Context<'a> {
    fn new(
        document: &'a ProjectDocument,
        parent: &'a NodeId,
        node: &'a NodeId,
        captured_right: Option<&'a NodeId>,
    ) -> Result<Self, EditError> {
        let target = admit(document, parent, node, "combined Source trim geometry")?;
        let neighbor = captured_right
            .ok_or_else(|| {
                EditError::new(
                    EditErrorCode::SelectionUnavailable,
                    "Roll had no captured right neighbor",
                )
            })
            .and_then(|right| admit(document, parent, right, "combined Source trim Roll"))
            .and_then(|right| {
                if target.slot.checked_add(1) != Some(right.slot)
                    || target.output.end() != right.output.start()
                {
                    Err(EditError::new(
                        EditErrorCode::SelectionUnavailable,
                        "Roll requires the captured literally adjacent right child",
                    ))
                } else {
                    Ok(right)
                }
            });
        let (right, availability) = match neighbor {
            Ok(right) => {
                let availability = SourceTrimRollAvailability::Available {
                    right: right.target.clone(),
                };
                (Some(right), availability)
            }
            Err(error) => (
                None,
                SourceTrimRollAvailability::Unavailable {
                    captured_right: captured_right.cloned(),
                    error,
                },
            ),
        };
        let NodeKind::Sequence { children } = &document.nodes()[parent].kind else {
            unreachable!()
        };
        let scope = FrameRange::new(
            document.source_splice_boundary(parent, 0)?,
            document.source_splice_boundary(parent, children.len())?,
        )
        .map_err(DocumentError::from)?;
        Ok(Self {
            target,
            right,
            availability,
            scope,
            total: document.duration()?,
        })
    }

    fn require_right(&self) -> Result<(), EditError> {
        match &self.availability {
            SourceTrimRollAvailability::Available { .. } => Ok(()),
            SourceTrimRollAvailability::Unavailable { error, .. } => Err(error.clone()),
        }
    }

    fn constraints(&self, intent: SourceTrimIntent) -> Result<Constraints, EditError> {
        if intent.roll_frames != 0 {
            self.require_right()?;
        }
        context_constraints(self, intent.policy).map_err(Into::into)
    }

    fn resolve(
        &self,
        document: &ProjectDocument,
        intent: SourceTrimIntent,
    ) -> Result<SourceTrimGeometry, EditError> {
        let constraints = self.constraints(intent)?;
        constraints.validate(intent)?;
        self.candidate(document, intent).map_err(Into::into)
    }

    fn candidate(
        &self,
        document: &ProjectDocument,
        intent: SourceTrimIntent,
    ) -> Result<SourceTrimGeometry, DocumentError> {
        let i = ExactRatio::integer(intent.in_frames);
        let o = ExactRatio::integer(intent.out_frames);
        let r = ExactRatio::integer(intent.roll_frames);
        let s = ExactRatio::integer(intent.slip_frames);
        let out = o.checked_add(r)?;
        let delta = if intent.policy == SourceTrimPolicy::Ripple {
            frame(o.checked_sub(i)?)?.0
        } else {
            0
        };
        let candidate = window_candidate(document, &self.target, i, out, s)?;
        let target_start = ExactRatio::integer(self.target.output.start().0);
        let target_end = ExactRatio::integer(self.target.output.end().0).checked_add(out)?;
        let target_output = match intent.policy {
            SourceTrimPolicy::Ripple => {
                FrameRange::new(frame(target_start)?, frame(target_end.checked_sub(i)?)?)?
            }
            SourceTrimPolicy::Overwrite => {
                FrameRange::new(frame(target_start.checked_add(i)?)?, frame(target_end)?)?
            }
        };
        let target = owner_geometry(&self.target, candidate, target_output)?;
        let right = self
            .right
            .as_ref()
            .map(|right| {
                let candidate =
                    window_candidate(document, right, r, ExactRatio::ZERO, ExactRatio::ZERO)?;
                let output = match intent.policy {
                    SourceTrimPolicy::Ripple => FrameRange::new(
                        target_output.end(),
                        frame(
                            ExactRatio::integer(right.output.end().0)
                                .checked_add(ExactRatio::integer(delta))?,
                        )?,
                    )?,
                    SourceTrimPolicy::Overwrite => FrameRange::new(
                        frame(ExactRatio::integer(right.output.start().0).checked_add(r)?)?,
                        right.output.end(),
                    )?,
                };
                owner_geometry(right, candidate, output)
            })
            .transpose()?;
        let required_source_wrappers = usize::from(target.needs_wrapper)
            + usize::from(right.as_ref().is_some_and(|right| right.needs_wrapper));
        check_wrapper_budget(document.nodes().len(), required_source_wrappers)?;
        Ok(SourceTrimGeometry {
            parent: self.target.parent.clone(),
            intent,
            scope_before: self.scope,
            scope_after: FrameRange::new(
                self.scope.start(),
                frame(
                    ExactRatio::integer(self.scope.end().0)
                        .checked_add(ExactRatio::integer(delta))?,
                )?,
            )?,
            project_duration_before: self.total,
            project_duration_after: FrameDuration::new(
                self.total
                    .frames()
                    .checked_add(delta)
                    .ok_or(TimeError::Overflow)?,
            )?,
            duration_delta_frames: delta,
            target,
            right,
            roll_availability: self.availability.clone(),
            required_source_wrappers,
            requires_overwrite_overlay: intent.policy == SourceTrimPolicy::Overwrite
                && (intent.in_frames != 0 || intent.out_frames != 0),
        })
    }
}

fn owner_geometry(
    admission: &Admission<'_>,
    candidate: SourceEdgeCandidate,
    output: FrameRange,
) -> Result<SourceTrimOwnerGeometry, DocumentError> {
    let p = candidate.prefix.frames();
    let unprefixed = FrameRange::new(
        ProjectFrame(
            candidate
                .allocation
                .start()
                .0
                .checked_sub(p)
                .ok_or(TimeError::Overflow)?,
        ),
        ProjectFrame(
            candidate
                .allocation
                .end()
                .0
                .checked_sub(p)
                .ok_or(TimeError::Overflow)?,
        ),
    )?;
    let phase_anchor = if unprefixed.start() >= admission.allocation.end() {
        SourceTrimPhaseAnchor::SourceEnd
    } else if unprefixed.end() <= admission.allocation.start() {
        SourceTrimPhaseAnchor::SourceStart
    } else {
        SourceTrimPhaseAnchor::Retained {
            allocation: FrameRange::new(
                unprefixed.start().max(admission.allocation.start()),
                unprefixed.end().min(admission.allocation.end()),
            )?,
        }
    };
    Ok(SourceTrimOwnerGeometry {
        target: admission.target.clone(),
        physical_source: admission.physical_source.clone(),
        asset: admission.asset.clone(),
        qualification: admission.qualification.clone(),
        slot: admission.slot,
        allocation_before: admission.allocation,
        allocation_after: candidate.allocation,
        output_before: admission.output,
        output_after: output,
        window_before: admission.window,
        window_after: candidate.window,
        effective_before: admission.effective,
        effective_after: candidate.effective,
        before: admission.source.clone(),
        after: candidate.source,
        physical_prefix: candidate.prefix,
        needs_wrapper: candidate.needs_wrapper,
        phase_anchor,
    })
}

fn frame(value: ExactRatio) -> Result<ProjectFrame, TimeError> {
    let integer = value.floor();
    if ExactRatio::new(integer, 1)? != value {
        return Err(TimeError::InvalidRatio);
    }
    Ok(ProjectFrame(
        i64::try_from(integer).map_err(|_| TimeError::Overflow)?,
    ))
}

fn check_wrapper_budget(existing: usize, required: usize) -> Result<(), DocumentError> {
    if existing
        .checked_add(required)
        .is_none_or(|nodes| nodes > MAX_DOCUMENT_NODES)
    {
        return Err(DocumentError::new(
            crate::DocumentErrorCode::LimitExceeded,
            "combined Source trim geometry exceeds the Source wrapper node budget",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapper_budget_admits_exact_capacity_and_rejects_allocation_overflow() {
        assert!(check_wrapper_budget(MAX_DOCUMENT_NODES, 0).is_ok());
        assert!(check_wrapper_budget(MAX_DOCUMENT_NODES - 2, 2).is_ok());
        for (existing, required) in [(MAX_DOCUMENT_NODES - 1, 2), (usize::MAX, 1)] {
            assert_eq!(
                check_wrapper_budget(existing, required).unwrap_err().code,
                crate::DocumentErrorCode::LimitExceeded
            );
        }
    }
}
