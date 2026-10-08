//! Deterministic model inputs from immutable document and measured metadata.
//!
//! Capture never decodes media. Structural support records what must remain
//! unchanged for a previous worker-side shot qualification to remain relevant;
//! it is not a visual qualification or permission to accept a movie.

use std::io;

use deadpan_core::{
    BoundaryQueryLimits, ExtensionDirection, FrameDuration, FrameRate, NodeKind, ProjectDocument,
    ScopedNodeTarget, TargetId,
};
use deadpan_plan::{RenderPlan, ScopedHoldBoundaries, ScopedHoldContext, ScopedHoldContextRequest};

use crate::StoreError;
use crate::generation_pictures::GenerationPictures;

use deadpan_jobs::generation_inputs::GenerationInputBinding as InputBinding;
pub use deadpan_jobs::generation_inputs::{
    ExtensionCapturePolicy, GenerationCaptureSpec, GenerationInputBinding, GenerationInputSettings,
    GenerationInputSupport, GenerationInputs, GenerationPictureIdentity, GenerationRegionIdentity,
    MAX_INPUT_BINDING_BYTES, RelativeGenerationPicture,
};

pub const MAX_INPUT_CAPTURE_BYTES: usize = deadpan_core::MAX_DOCUMENT_JSON_BYTES;
const MAX_CAPTURE_WORK: usize = 64 * deadpan_core::MAX_DOCUMENT_NODES;
const MAX_CAPTURE_SPANS: usize = 8 * deadpan_core::MAX_DOCUMENT_NODES;

/// Stateless store-side capture engine. The returned binding is a shared wire
/// type, so model providers can consume it without depending on the store.
pub struct GenerationInputCapture;

impl GenerationInputCapture {
    /// The implemented temporal capture contract, independent of installation
    /// or provider admission. Selecting it does not promise runnable inference.
    pub fn for_preference(
        preference: deadpan_jobs::GenerationModePreference,
        left_present: bool,
        right_present: bool,
    ) -> Result<GenerationCaptureSpec, StoreError> {
        use deadpan_jobs::{ConditioningMode, ConditioningSupport, GenerationModePreference};
        // An explicit operation stays captured even while its anchor is absent.
        // Provider admission separately checks whether it can run now.
        let mode = match preference {
            GenerationModePreference::Bridge => ConditioningMode::Bridge,
            GenerationModePreference::ExtendFromLeft => ConditioningMode::ExtendFromLeft,
            GenerationModePreference::ExtendFromRight => ConditioningMode::ExtendFromRight,
            GenerationModePreference::Automatic => preference
                .resolve(
                    left_present,
                    right_present,
                    ConditioningSupport {
                        bridge: true,
                        extend_from_left: true,
                        extend_from_right: true,
                    },
                )
                .map_err(StoreError::GenerationInputMode)?,
        };
        Ok(match mode {
            ConditioningMode::Bridge => GenerationCaptureSpec::Bridge,
            ConditioningMode::ExtendFromLeft | ConditioningMode::ExtendFromRight => {
                GenerationCaptureSpec::Extension {
                    direction: if mode == ConditioningMode::ExtendFromLeft {
                        ExtensionDirection::FromLeft
                    } else {
                        ExtensionDirection::FromRight
                    },
                    native_rate: FrameRate::new(24, 1).map_err(time_error)?,
                    context_frames: 9,
                    policy: ExtensionCapturePolicy::TemporalContextV1,
                }
            }
        })
    }

    fn validate(capture: GenerationCaptureSpec) -> Result<(), StoreError> {
        if let GenerationCaptureSpec::Extension { context_frames, .. } = capture
            && !(1..=deadpan_plan::MAX_HOLD_CONTEXT_FRAMES).contains(&context_frames)
        {
            return Err(invalid("extension context count exceeds the capture bound"));
        }
        Ok(())
    }
}

/// One ledger for a transition, including failed queries. Bounds do not reset
/// when callers capture another target or switch bridge/extension operations.
pub struct InputCaptureBudget {
    queries: BoundaryQueryLimits,
    bytes: usize,
    spans: usize,
}

impl Default for InputCaptureBudget {
    fn default() -> Self {
        Self {
            queries: BoundaryQueryLimits {
                max_scopes: MAX_CAPTURE_WORK,
                max_comparisons: MAX_CAPTURE_WORK,
            },
            bytes: MAX_INPUT_CAPTURE_BYTES,
            spans: MAX_CAPTURE_SPANS,
        }
    }
}

impl InputCaptureBudget {
    #[cfg(test)]
    pub(crate) fn with_test_byte_limit(bytes: usize) -> Self {
        Self {
            bytes,
            ..Self::default()
        }
    }

    pub(crate) fn resolve_capture(
        &mut self,
        plan: &RenderPlan,
        target: &ScopedNodeTarget,
        preference: deadpan_jobs::GenerationModePreference,
    ) -> Result<GenerationCaptureSpec, StoreError> {
        let result = plan.scoped_hold_boundaries(target, self.queries);
        let boundaries = match result {
            Ok(value) => value,
            Err(error) => {
                self.queries = BoundaryQueryLimits {
                    max_scopes: 0,
                    max_comparisons: 0,
                };
                self.bytes = 0;
                self.spans = 0;
                return Err(plan_error(error));
            }
        };
        self.query(boundaries.lookup)?;
        GenerationInputCapture::for_preference(
            preference,
            boundaries.left.is_some(),
            boundaries.right.is_some(),
        )
    }

    fn query(&mut self, stats: deadpan_plan::LookupStats) -> Result<(), StoreError> {
        self.queries.max_scopes = self
            .queries
            .max_scopes
            .checked_sub(stats.visited_nodes)
            .ok_or(StoreError::GenerationInputLimit("node work"))?;
        self.queries.max_comparisons = self
            .queries
            .max_comparisons
            .checked_sub(stats.sequence_comparisons)
            .and_then(|left| left.checked_sub(stats.iteration_run_comparisons))
            .ok_or(StoreError::GenerationInputLimit("comparison work"))?;
        Ok(())
    }

    fn charge(&mut self, binding: &GenerationInputBinding) -> Result<(), StoreError> {
        let mut count = ByteCount {
            remaining: self.bytes.min(MAX_INPUT_BINDING_BYTES),
            written: 0,
        };
        serde_json::to_writer(&mut count, binding)
            .map_err(|_| StoreError::GenerationInputLimit("metadata bytes"))?;
        self.bytes -= count.written;
        Ok(())
    }
}

impl GenerationInputCapture {
    pub fn capture(
        document: &ProjectDocument,
        target: &ScopedNodeTarget,
        pictures: &dyn GenerationPictures,
    ) -> Result<InputBinding, StoreError> {
        let plan = RenderPlan::compile(document).map_err(plan_error)?;
        Self::capture_with_plan(
            document,
            &plan,
            target,
            GenerationCaptureSpec::Bridge,
            None,
            pictures,
            &mut InputCaptureBudget::default(),
        )
    }

    pub fn capture_with_plan(
        document: &ProjectDocument,
        plan: &RenderPlan,
        target: &ScopedNodeTarget,
        capture: GenerationCaptureSpec,
        region: Option<&TargetId>,
        pictures: &dyn GenerationPictures,
        budget: &mut InputCaptureBudget,
    ) -> Result<InputBinding, StoreError> {
        Self::validate(capture)?;
        let result = (|| match capture {
            GenerationCaptureSpec::Bridge => {
                let boundary = plan
                    .scoped_hold_boundaries(target, budget.queries)
                    .map_err(plan_error)?;
                budget.query(boundary.lookup)?;
                Self::from_boundaries(document, &boundary, pictures)
            }
            GenerationCaptureSpec::Extension {
                direction,
                native_rate,
                context_frames,
                ..
            } => {
                let observation = plan
                    .scoped_hold_context_observation(
                        &ScopedHoldContextRequest {
                            target: target.clone(),
                            direction,
                            native_rate,
                            frame_count: context_frames,
                        },
                        budget.queries,
                    )
                    .map_err(plan_error)?;
                budget.query(observation.lookup())?;
                let context = match observation {
                    deadpan_plan::ScopedHoldContextObservation::Available(context) => context,
                    deadpan_plan::ScopedHoldContextObservation::Unavailable { reason, .. } => {
                        return Err(plan_error(deadpan_plan::PlanError::HoldContextUnavailable(
                            reason,
                        )));
                    }
                };
                budget.spans = budget
                    .spans
                    .checked_sub(context.coverage.spans.len() + 1)
                    .ok_or(StoreError::GenerationInputLimit("structural spans"))?;
                Self::from_context(document, &context, capture, pictures)
            }
        })();
        let result = match result.and_then(|binding| Self::with_region(binding, document, region)) {
            Ok(result) => result,
            Err(error) => {
                // A failed canonical query has no partial work receipt. The
                // provider runs only after a successful query was charged, so
                // an unsupported picture need not poison independent captures.
                if matches!(
                    error,
                    StoreError::GenerationInputQuery(_) | StoreError::GenerationInputLimit(_)
                ) && !matches!(
                    error,
                    StoreError::GenerationInputQuery(
                        deadpan_plan::PlanError::HoldContextUnavailable(_)
                    )
                ) {
                    budget.queries = BoundaryQueryLimits {
                        max_scopes: 0,
                        max_comparisons: 0,
                    };
                    budget.bytes = 0;
                    budget.spans = 0;
                }
                return Err(error);
            }
        };
        if let Err(error) = budget.charge(&result) {
            budget.queries = BoundaryQueryLimits {
                max_scopes: 0,
                max_comparisons: 0,
            };
            budget.bytes = 0;
            budget.spans = 0;
            return Err(error);
        }
        Ok(result)
    }

    pub fn with_region(
        mut binding: InputBinding,
        document: &ProjectDocument,
        region: Option<&TargetId>,
    ) -> Result<InputBinding, StoreError> {
        binding.region = region
            .map(|id| {
                let record = document.targets().get(id);
                let mut count = ByteCount {
                    remaining: MAX_INPUT_BINDING_BYTES,
                    written: 0,
                };
                serde_json::to_writer(&mut count, &record)
                    .map_err(|_| StoreError::GenerationInputLimit("region bytes"))?;
                Ok::<_, StoreError>(GenerationRegionIdentity {
                    id: id.clone(),
                    record: record.cloned(),
                })
            })
            .transpose()?;
        let mut budget = InputCaptureBudget::default();
        budget.charge(&binding)?;
        Ok(binding)
    }

    /// Samples are observations only. Durable admission independently captures
    /// them again from the authoritative document and its measured receipts.
    pub fn from_boundaries(
        document: &ProjectDocument,
        boundaries: &ScopedHoldBoundaries,
        pictures: &dyn GenerationPictures,
    ) -> Result<InputBinding, StoreError> {
        validate_boundary(document, boundaries)?;
        Ok(Self::base(
            document,
            boundaries.duration,
            GenerationInputs::Bridge {
                left: boundaries
                    .left
                    .as_ref()
                    .map(|sample| pictures.identity(document, &sample.picture))
                    .transpose()?,
                right: boundaries
                    .right
                    .as_ref()
                    .map(|sample| pictures.identity(document, &sample.picture))
                    .transpose()?,
            },
        ))
    }

    pub fn from_context(
        document: &ProjectDocument,
        context: &ScopedHoldContext,
        capture: GenerationCaptureSpec,
        pictures: &dyn GenerationPictures,
    ) -> Result<InputBinding, StoreError> {
        Self::validate(capture)?;
        validate_boundary(document, &context.boundaries)?;
        let GenerationCaptureSpec::Extension {
            direction,
            native_rate,
            context_frames,
            ..
        } = capture
        else {
            return Err(invalid("temporal inputs require an extension capture"));
        };
        if direction != context.direction
            || native_rate != context.native_rate
            || context.pictures.len() != context_frames as usize
            || context.coverage.spans.len() > deadpan_plan::MAX_DEFINITION_PICTURE_SPANS
        {
            return Err(invalid(
                "extension context differs from its captured operation",
            ));
        }
        let anchor = match direction {
            ExtensionDirection::FromLeft => context.pictures.last(),
            ExtensionDirection::FromRight => context.pictures.first(),
        }
        .ok_or_else(|| invalid("extension context has no anchor"))?
        .position;
        let observe = |sample: &deadpan_plan::DefinitionPictureSample| -> Result<RelativeGenerationPicture, StoreError> {
            if sample.project_id != context.boundaries.project_id
                || sample.revision_id != context.boundaries.revision_id
                || sample.definition != context.boundaries.definition {
                return Err(invalid("extension sample belongs to another definition"));
            }
            Ok(RelativeGenerationPicture {
                position: sample.position.checked_sub(anchor).map_err(time_error)?,
                picture: pictures.identity(document, &sample.picture)?,
            })
        };
        let mut support = Vec::with_capacity(context.coverage.spans.len());
        for span in &context.coverage.spans {
            observe(&span.start)?;
            if !span.start.position.compare(span.end_exclusive).is_lt() {
                return Err(invalid("extension support is not a positive interval"));
            }
            let identities = pictures.support(document, span)?;
            support.push(GenerationInputSupport {
                start: span
                    .start
                    .position
                    .checked_sub(anchor)
                    .map_err(time_error)?,
                end_exclusive: span.end_exclusive.checked_sub(anchor).map_err(time_error)?,
                first: identities.first,
                last: identities.last,
            });
        }
        let terminal = observe(&context.coverage.terminal)?;
        let samples = context
            .pictures
            .iter()
            .map(observe)
            .collect::<Result<Vec<_>, _>>()?;
        let opposite = match direction {
            ExtensionDirection::FromLeft => &context.boundaries.right,
            ExtensionDirection::FromRight => &context.boundaries.left,
        }
        .as_ref()
        .map(observe)
        .transpose()?;
        Ok(Self::base(
            document,
            context.boundaries.duration,
            GenerationInputs::Extension {
                capture,
                samples,
                opposite,
                support,
                terminal,
            },
        ))
    }

    pub(crate) fn base(
        document: &ProjectDocument,
        duration: FrameDuration,
        inputs: GenerationInputs,
    ) -> InputBinding {
        InputBinding {
            duration,
            frame_rate: document.presentation_basis().frame_rate,
            canvas: [
                document.presentation_basis().width,
                document.presentation_basis().height,
            ],
            inputs,
            region: None,
        }
    }
}

fn validate_boundary(
    document: &ProjectDocument,
    boundaries: &ScopedHoldBoundaries,
) -> Result<(), StoreError> {
    let Some(NodeKind::Hold { recipe }) = document
        .nodes()
        .get(&boundaries.target.node)
        .map(|node| &node.kind)
    else {
        return Err(invalid("input binding target is not an authored Hold"));
    };
    if &boundaries.project_id != document.project_id()
        || &boundaries.revision_id != document.revision_id()
        || boundaries.duration != recipe.duration
        || boundaries.duration == FrameDuration::ZERO
        || boundaries
            .left
            .iter()
            .chain(&boundaries.right)
            .any(|sample| {
                sample.project_id != boundaries.project_id
                    || sample.revision_id != boundaries.revision_id
                    || sample.definition != boundaries.definition
            })
    {
        return Err(invalid(
            "input binding boundaries belong to another Hold revision",
        ));
    }
    Ok(())
}

struct ByteCount {
    remaining: usize,
    written: usize,
}
impl io::Write for ByteCount {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.remaining = self
            .remaining
            .checked_sub(bytes.len())
            .ok_or_else(|| io::Error::other("capture metadata limit"))?;
        self.written += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn plan_error(error: deadpan_plan::PlanError) -> StoreError {
    StoreError::GenerationInputQuery(error)
}
fn time_error(error: deadpan_core::TimeError) -> StoreError {
    invalid(&error.to_string())
}
fn invalid(reason: &str) -> StoreError {
    StoreError::GenerationPlan(reason.into())
}

#[cfg(test)]
mod tests;
