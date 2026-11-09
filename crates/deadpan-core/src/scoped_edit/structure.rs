//! Structural edits of one visible Repeat occurrence (`EditOccurrence` Split,
//! DeleteRipple, RepeatChild and SetRepeatPlays): exact isolation needs before
//! allocation and the selection that follows the committed result.

use super::*;
use crate::{OccurrenceEdit, RepeatInstance};

impl ProjectDocument {
    /// Isolation needs for one Split, ripple deletion, Repeat wrap or Repeat
    /// count change of a beat in explicit plays at every enclosing Repeat.
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
            OccurrenceEdit::DeleteRipple { .. } | OccurrenceEdit::RepeatChild { .. } => {
                crate::occurrence_edit::ripple::site(self, instance)?;
            }
            OccurrenceEdit::SetRepeatPlays { plays, .. } => {
                use crate::occurrence_edit::ripple;
                if ripple::repeat_plays(self, &instance.node, *plays)? == *plays {
                    return Err(ripple::unchanged(*plays));
                }
                ripple::site(self, instance)?;
            }
            _ => {
                return Err(invalid(
                    "a one-play structural edit splits, deletes or repeats a beat, or sets a Repeat's plays",
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
                let index = usize::from(!crate::split::refines(&self.nodes()[&instance.node]));
                identities
                    .nodes
                    .get(index)
                    .cloned()
                    .ok_or_else(|| invalid("split needs more node identities"))?
            }
            OccurrenceEdit::RepeatChild { id, .. } => id.clone(),
            OccurrenceEdit::SetRepeatPlays { .. } => map(&instance.node),
            OccurrenceEdit::DeleteRipple { .. } => {
                let parent = self
                    .parent_of(&instance.node)
                    .ok_or_else(|| invalid("the deleted beat has no group"))?;
                let NodeKind::Sequence { children } = &self.nodes()[&parent].kind else {
                    return Err(invalid("the deleted beat's parent is not a group"));
                };
                let slot = children
                    .iter()
                    .position(|child| child == &instance.node)
                    .ok_or_else(|| invalid("the deleted beat left its group"))?;
                map(children
                    .get(slot + 1)
                    .or_else(|| slot.checked_sub(1).and_then(|index| children.get(index)))
                    .unwrap_or(&parent))
            }
            _ => {
                return Err(invalid(
                    "a one-play structural edit splits, deletes or repeats a beat, or sets a Repeat's plays",
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
