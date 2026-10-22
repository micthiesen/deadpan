//! Duration-only authority for an inspected Hold inside fixed Edit Retimes.

use super::*;
use crate::{ExactFrameRange, ExactRatio, InstancePath, RetimePurpose};

/// A captured ordinary Hold address, independent of ordinary group navigation.
/// This runtime authority never belongs to a saved semantic program.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticHoldDurationContext {
    /// The inspected fixed Edit Retime, a direct child of the ordinary parent.
    pub root: NodeId,
    pub target: NodeId,
    /// A concrete presentation with no Repeat ancestors.
    pub presentation: InstancePath,
}

/// Current structural picture visibility, after every enclosing Retime crop.
/// This proves timeline ownership, not the Hold's evaluated provider or cutaway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HoldDurationVisibility {
    pub exact: ExactFrameRange,
    /// Project pictures whose centers belong to the half-open exact interval.
    pub frames: FrameRange,
}

fn unavailable(message: &str) -> EditError {
    EditError::new(EditErrorCode::SelectionUnavailable, message)
}

impl ProjectDocument {
    /// Resolve one ordinary inspected Hold without sampling a cursor or changing
    /// authority. Only Sequence and fixed, unclocked Edit Retime ancestors are
    /// admitted. Work is bounded by the compact graph, never play count.
    ///
    /// Picture center f + 1/2 belongs to [In, Out): a tie at In is included
    /// and a tie at Out is excluded. Hidden or unsampled Holds have no safe
    /// continuation and are refused. Hosts still validate actual presentation.
    pub fn ordinary_hold_duration_visibility(
        &self,
        parent: &NodeId,
        authority: &SemanticHoldDurationContext,
    ) -> Result<HoldDurationVisibility, EditError> {
        self.validate()?;
        scope_bounds(self, parent)?;
        let Some(NodeKind::Sequence { children }) = self.nodes().get(parent).map(|node| &node.kind)
        else {
            return Err(unavailable(
                "Hold duration requires an ordinary Sequence parent",
            ));
        };
        if !children.contains(&authority.root)
            || !matches!(
                self.nodes().get(&authority.root).map(|node| &node.kind),
                Some(NodeKind::Retime {
                    purpose: RetimePurpose::Edit,
                    ..
                })
            )
        {
            return Err(unavailable(
                "Hold duration inspector root must be a direct fixed Edit Retime child",
            ));
        }
        if authority.target != authority.presentation.node
            || !authority.presentation.repeats.is_empty()
        {
            return Err(unavailable(
                "ordinary Hold duration requires its captured target with no Repeat ancestry",
            ));
        }
        let Some(NodeKind::Hold { recipe }) =
            self.nodes().get(&authority.target).map(|node| &node.kind)
        else {
            return Err(unavailable(
                "ordinary Hold duration requires a captured Hold",
            ));
        };
        // A descendant clock also seals its enclosing stage. Retained physical
        // audio bindings alone do not, so a second duration edit stays eligible.
        let clocked = self.retimes_with_derived_clocks();
        if clocked.contains(&authority.root) {
            return Err(unavailable(
                "ordinary Hold duration requires fixed Retimes without exact pause clocks",
            ));
        }
        let extents = self.exact_durations()?;
        let mut parents = BTreeMap::new();
        for id in self.nodes().keys() {
            for child in self.children(id) {
                parents.insert(child.clone(), id.clone());
            }
        }
        let mut current = authority.target.clone();
        let mut saw_root = false;
        let mut visible = Some((
            ExactRatio::ZERO,
            ExactRatio::integer(recipe.duration.frames()),
        ));
        for _ in 0..=crate::MAX_DOCUMENT_DEPTH {
            saw_root |= current == authority.root;
            if current == *self.root() {
                if !saw_root {
                    return Err(unavailable(
                        "Hold duration target lies outside its captured inspector root",
                    ));
                }
                let (start, end) = visible
                    .ok_or_else(|| unavailable("the captured Hold is hidden by its Retime crop"))?;
                let half = ExactRatio::new(1, 2).map_err(crate::DocumentError::from)?;
                let total = extents[self.root()]
                    .ceil()
                    .map_err(crate::DocumentError::from)?;
                let sampled = |value: ExactRatio| -> Result<ProjectFrame, EditError> {
                    let frame = value
                        .checked_sub(half)
                        .and_then(ExactRatio::ceil)
                        .map_err(crate::DocumentError::from)?;
                    Ok(ProjectFrame(i64::try_from(frame.clamp(0, total)).map_err(
                        |_| {
                            EditError::new(
                                EditErrorCode::TimingOverflow,
                                "visible Hold picture boundary overflows",
                            )
                        },
                    )?))
                };
                let frames = FrameRange::new(sampled(start)?, sampled(end)?)
                    .map_err(crate::DocumentError::from)?;
                if frames.start() == frames.end() {
                    return Err(unavailable(
                        "the captured Hold has no visible sampled project picture",
                    ));
                }
                return Ok(HoldDurationVisibility {
                    exact: ExactFrameRange::new(start, end)?,
                    frames,
                });
            }
            let ancestor = parents.get(&current).ok_or_else(|| {
                unavailable("Hold duration target has no captured inspector ancestry")
            })?;
            match &self.nodes()[ancestor].kind {
                NodeKind::Sequence { children } => {
                    let mut offset = ExactRatio::ZERO;
                    for child in children {
                        if *child == current {
                            break;
                        }
                        offset = offset
                            .checked_add(extents[child])
                            .map_err(crate::DocumentError::from)?;
                    }
                    visible = visible
                        .map(|(start, end)| {
                            Ok::<_, EditError>((
                                start
                                    .checked_add(offset)
                                    .map_err(crate::DocumentError::from)?,
                                end.checked_add(offset)
                                    .map_err(crate::DocumentError::from)?,
                            ))
                        })
                        .transpose()?;
                }
                NodeKind::Retime {
                    duration,
                    mapping,
                    purpose: RetimePurpose::Edit,
                    ..
                } if !saw_root && !clocked.contains(ancestor) => {
                    let crop_start = ExactRatio::integer(mapping.start().0);
                    let crop_end = ExactRatio::integer(mapping.end().0);
                    let scale = ExactRatio::integer(duration.frames())
                        .checked_div(ExactRatio::integer(mapping.duration().frames()))
                        .map_err(crate::DocumentError::from)?;
                    visible = visible
                        .map(|(start, end)| {
                            let start = if start.compare(crop_start).is_lt() {
                                crop_start
                            } else {
                                start
                            };
                            let end = if end.compare(crop_end).is_gt() {
                                crop_end
                            } else {
                                end
                            };
                            if !start.compare(end).is_lt() {
                                return Ok(None);
                            }
                            let mapped = |value: ExactRatio| {
                                value.checked_sub(crop_start)?.checked_mul(scale)
                            };
                            Ok::<_, EditError>(Some((
                                mapped(start).map_err(crate::DocumentError::from)?,
                                mapped(end).map_err(crate::DocumentError::from)?,
                            )))
                        })
                        .transpose()?
                        .flatten();
                }
                _ => {
                    return Err(unavailable(
                        "ordinary Hold duration ancestry must contain only Sequences and fixed unclocked Edit Retimes, without Repeats",
                    ));
                }
            }
            current = ancestor.clone();
        }
        Err(unavailable(
            "Hold duration ancestry exceeds the document depth limit",
        ))
    }
}

pub(super) fn validate_context(
    document: &ProjectDocument,
    context: &SemanticContext,
) -> Result<(), EditError> {
    let Some(authority) = &context.hold_duration else {
        return Ok(());
    };
    if context.scoped.is_some()
        || context.shared_hold_duration.is_some()
        || context.original.is_some()
        || context.visual_selection.is_some()
        || context.selected_child.as_ref() != Some(&authority.root)
    {
        return Err(unavailable(
            "ordinary inspected Hold duration requires Your edit, its selected inspector root, no Visual selection and no single-play authority",
        ));
    }
    document.ordinary_hold_duration_visibility(&context.parent, authority)?;
    Ok(())
}

impl<F, R, S, P> Planner<'_, F, R, S, P>
where
    F: FnMut(SemanticAllocationRequest) -> Result<SemanticAllocation, EditError>,
    R: FnMut(&ProjectDocument, &RegisterValue) -> Result<SourceNode, EditError>,
    S: FnMut(&ProjectDocument) -> Result<Arc<SpeechTimeline>, EditError>,
    P: FnMut(&ProjectDocument, PauseSite) -> Result<PauseProvider, EditError>,
{
    pub(super) fn set_inspected_hold_duration(
        &mut self,
        trace_index: usize,
        length: crate::PauseLength,
        authority: SemanticHoldDurationContext,
    ) -> Result<(), EditError> {
        validate_context(&self.current, &self.context)?;
        let NodeKind::Hold { recipe } = &self.current.nodes()[&authority.target].kind else {
            unreachable!("the duration authority validated a Hold")
        };
        let duration = length.resolve(self.current.presentation_basis().frame_rate)?;
        if recipe.duration == duration {
            return Err(invalid(
                "the pause already has that duration; no edit was made",
            ));
        }
        self.charge_step(false)?;
        let SemanticAllocation::ParameterEdit { new_revision } =
            (self.allocate)(SemanticAllocationRequest::ParameterEdit {
                step_index: self.steps.len(),
            })?
        else {
            return Err(invalid("a pause duration requires a parameter allocation"));
        };
        self.reserve_revision(&new_revision)?;
        self.commit_leaf(LeafEdit::new(
            new_revision,
            Command::SetHoldDuration {
                node: authority.target.clone(),
                duration,
            },
        )?)?;
        let visible = self
            .current
            .ordinary_hold_duration_visibility(&self.context.parent, &authority)?;
        self.context.cursor = visible.frames.start();
        super::validate_context(&self.current, &self.context)?;
        self.trace[trace_index].resolved_parent = self.current.parent_of(&authority.target);
        self.trace[trace_index].resolved_selection = Some(SliceCaptureSelection::Child {
            node: authority.target,
        });
        self.trace[trace_index].resolved_range = Some(visible.frames);
        Ok(())
    }
}
