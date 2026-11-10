//! Structural edits of one visible Repeat occurrence (`EditOccurrence` Split,
//! DeleteRipple, PasteSlice, PasteSource, RepeatChild, SetRepeatPlays,
//! SetRepeatGaps and Ungroup): exact isolation needs before
//! allocation and the selection that follows the committed result.

use super::*;
use crate::{OccurrenceEdit, RepeatInstance};

impl ProjectDocument {
    /// The exact whole-frame project interval of one visible occurrence, shared
    /// by native and semantic structural edits. A beat cropped or retimed in
    /// that play has no whole-frame interval and refuses.
    pub fn occurrence_structure_interval(
        &self,
        instance: &InstancePath,
    ) -> Result<(crate::ProjectFrame, crate::ProjectFrame), EditError> {
        use crate::{Anchor, AnchorIndex, AnchorTarget, BoundaryAnchor, ExactRatio, InsertionBias};
        let length = self
            .durations_shared()?
            .get(&instance.node)
            .ok_or_else(|| invalid("the selected beat no longer exists"))?
            .frames();
        let index = AnchorIndex::new(self).map_err(|error| invalid(&error.to_string()))?;
        let boundary = |position: i64, bias| {
            index
                .resolve_target(&AnchorTarget {
                    boundary: BoundaryAnchor {
                        coordinate: Anchor::Occurrence {
                            instance: instance.clone(),
                            position: ExactRatio::integer(position),
                        },
                        bias,
                    },
                    occurrence: None,
                })
                .map_err(|error| invalid(&error.to_string()))
        };
        let start = boundary(0, InsertionBias::Right)?;
        let end = boundary(length, InsertionBias::Left)?;
        if start.exact_frame.denominator() != 1
            || end.exact_frame.denominator() != 1
            || end.frame.0 - start.frame.0 != length
        {
            return Err(EditError::new(
                EditErrorCode::SelectionUnavailable,
                "this beat plays inside a speed change or crop in this play; splitting, deleting or repeating it there is not supported yet",
            ));
        }
        Ok((start.frame, end.frame))
    }

    /// Isolation needs for one Split, ripple deletion, Repeat wrap or Repeat
    /// count or gap change of a beat in explicit plays at every enclosing Repeat.
    /// Unsupported scopes, including sound-bearing documents, refuse before
    /// names are allocated.
    pub fn occurrence_structure_requirements(
        &self,
        instance: &InstancePath,
        edit: &OccurrenceEdit,
    ) -> Result<ScopedEditRequirements, EditError> {
        if instance.repeats.is_empty() {
            return Err(invalid(
                "a one-play edit needs This play chosen at every enclosing Repeat",
            ));
        }
        instance.validate(self)?;
        match edit {
            OccurrenceEdit::Split { at, .. } => {
                let duration = self.durations_shared()?[&instance.node];
                if at.frames() <= 0 || *at >= duration {
                    return Err(EditError::new(
                        EditErrorCode::SelectionUnavailable,
                        "move inside the selected beat to split it; existing boundaries need no split",
                    ));
                }
            }
            OccurrenceEdit::PasteSlice {
                at: Some(at),
                slice,
                ..
            } => {
                self.occurrence_placement(instance, *at, Some(slice))?;
            }
            OccurrenceEdit::PasteSource { at: Some(at), .. } => {
                self.occurrence_placement(instance, *at, None)?;
            }
            OccurrenceEdit::DeleteRipple { .. }
            | OccurrenceEdit::RepeatChild { .. }
            | OccurrenceEdit::PasteSlice { .. }
            | OccurrenceEdit::PasteSource { .. } => {
                crate::occurrence_edit::ripple::site(self, instance)?;
            }
            OccurrenceEdit::SetRepeatPlays { plays, .. } => {
                use crate::occurrence_edit::ripple;
                if ripple::repeat_plays(self, &instance.node, *plays)? == *plays {
                    return Err(ripple::unchanged(*plays));
                }
                ripple::site(self, instance)?;
            }
            OccurrenceEdit::SetRepeatGaps {
                plays,
                gap,
                branches,
                ..
            } => {
                use crate::occurrence_edit::ripple;
                ripple::repeat_plays(self, &instance.node, *plays)?;
                if ripple::gaps_unchanged(self, &instance.node, *plays, gap.as_ref(), branches) {
                    return Err(ripple::unchanged_gaps());
                }
                ripple::site(self, instance)?;
            }
            OccurrenceEdit::Ungroup { wrappers } => {
                // The isolated copy plans exactly as its definition does.
                let required = self.ungroup_requirements(&instance.node)?;
                if wrappers.len() != required {
                    return Err(EditError::new(
                        EditErrorCode::IdentityConflict,
                        format!("this ungroup requires exactly {required} wrapper identities"),
                    ));
                }
            }
            _ => {
                return Err(invalid(
                    "a one-play structural edit splits, deletes, pastes beside, repeats or ungroups a beat, or sets a Repeat's plays or gaps",
                ));
            }
        }
        crate::sound_events::validate_command(
            self,
            &Command::EditOccurrence {
                instance: instance.clone(),
                edit: edit.clone(),
                identities: OccurrenceIdentities::default(),
            },
        )?;
        temporal_requirements(self, &play_target(instance))
    }

    /// The scoped selection after `edit` applies with exactly the isolation
    /// `identities` sized above: Split's right fragment, the new or recounted
    /// Repeat, or a deleted beat's following sibling, else its preceding one,
    /// else its now empty group. Returns the authoring address and its
    /// presentation.
    pub fn occurrence_structure_selection(
        &self,
        instance: &InstancePath,
        edit: &OccurrenceEdit,
        identities: &OccurrenceIdentities,
    ) -> Result<(ScopedNodeTarget, InstancePath), EditError> {
        let (mapped, maps) = isolation::resolve_temporal(self, &play_target(instance), identities)?;
        let map = |node: &NodeId| {
            maps.iter().fold(node.clone(), |node, copies| {
                copies.get(&node).cloned().unwrap_or(node)
            })
        };
        let node = match edit {
            OccurrenceEdit::Split { identities, .. } => {
                let index = usize::from(!crate::split::refines(self, &instance.node));
                identities
                    .nodes
                    .get(index)
                    .cloned()
                    .ok_or_else(|| invalid("split needs more node identities"))?
            }
            OccurrenceEdit::RepeatChild { id, .. } | OccurrenceEdit::PasteSource { id, .. } => {
                id.clone()
            }
            OccurrenceEdit::PasteSlice { identities, .. } => identities
                .authored
                .nodes
                .first()
                .cloned()
                .ok_or_else(|| invalid("a pasted copy has no imported root"))?,
            OccurrenceEdit::SetRepeatPlays { .. } | OccurrenceEdit::SetRepeatGaps { .. } => {
                map(&instance.node)
            }
            OccurrenceEdit::DeleteRipple { .. } => map(&self.neighbor(&instance.node)?),
            // The first promoted child, or its wrapper; an empty group's
            // neighbor, as a deletion selects.
            OccurrenceEdit::Ungroup { wrappers } => match self.children(&instance.node).next() {
                Some(first) => {
                    let plan = crate::ungroup::plan(self, &instance.node)?;
                    match plan.wrapped().position(|child| child == first) {
                        Some(index) => wrappers
                            .get(index)
                            .cloned()
                            .ok_or_else(|| invalid("ungroup needs more wrapper identities"))?,
                        None => map(first),
                    }
                }
                None => map(&self.neighbor(&instance.node)?),
            },
            _ => {
                return Err(invalid(
                    "a one-play structural edit splits, deletes, pastes beside, repeats or ungroups a beat, or sets a Repeat's plays or gaps",
                ));
            }
        };
        let repeats = mapped
            .repeats
            .iter()
            .map(|step| match &step.branch {
                RepeatEditBranch::Play { iteration } => Ok(RepeatInstance {
                    node: step.repeat.clone(),
                    iteration: iteration.clone(),
                }),
                RepeatEditBranch::Default => {
                    Err(invalid("an isolated occurrence keeps explicit plays"))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok((
            ScopedNodeTarget {
                node: node.clone(),
                repeats: mapped.repeats,
            },
            InstancePath { node, repeats },
        ))
    }
}

impl ProjectDocument {
    /// The beat a removed `node` leaves selected: its following sibling, else
    /// its preceding one, else its now empty group.
    pub(crate) fn neighbor(&self, node: &NodeId) -> Result<NodeId, EditError> {
        let parent = self
            .parent_of(node)
            .ok_or_else(|| invalid("the removed beat has no group"))?;
        let NodeKind::Sequence { children } = &self.nodes()[&parent].kind else {
            return Err(invalid("the removed beat's parent is not a group"));
        };
        let slot = children
            .iter()
            .position(|child| child == node)
            .ok_or_else(|| invalid("the removed beat left its group"))?;
        Ok(children
            .get(slot + 1)
            .or_else(|| slot.checked_sub(1).and_then(|index| children.get(index)))
            .unwrap_or(&parent)
            .clone())
    }
}

fn play_target(instance: &InstancePath) -> ScopedNodeTarget {
    ScopedNodeTarget {
        node: instance.node.clone(),
        repeats: instance
            .repeats
            .iter()
            .map(|step| RepeatEditStep {
                repeat: step.node.clone(),
                branch: RepeatEditBranch::Play {
                    iteration: step.iteration.clone(),
                },
            })
            .collect(),
    }
}
