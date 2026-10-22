//! One concrete Hold resize and its exact revision-bound continuation.

use super::*;
use crate::{
    Anchor, AnchorIndex, AnchorTarget, BoundaryAnchor, FrameDuration, InsertionBias,
    OccurrenceEdit, ResolvedBoundary,
};

impl ProjectDocument {
    /// Exact outside-in isolation needs for one visible Repeat occurrence's
    /// Hold resize. Unsupported temporal clocks fail before allocating names.
    pub fn occurrence_hold_duration_requirements(
        &self,
        instance: &InstancePath,
        duration: FrameDuration,
    ) -> Result<ScopedEditRequirements, EditError> {
        let target = target(instance);
        if target.repeats.is_empty() {
            return Err(invalid(
                "a one-play Hold duration needs explicit Repeat ancestry",
            ));
        }
        if !target.matches_instance(self, instance)? {
            return Err(invalid(
                "Hold duration presentation differs from its target",
            ));
        }
        let NodeKind::Hold { recipe } = &self.nodes()[&instance.node].kind else {
            return Err(EditError::new(
                EditErrorCode::WrongNodeKind,
                "a duration change applies to a pause (Hold)",
            ));
        };
        if duration == FrameDuration::ZERO {
            return Err(invalid("a pause needs positive duration"));
        }
        if recipe.duration == duration {
            return Err(invalid(
                "the pause already has that duration; no edit was made",
            ));
        }
        let mut node = instance.node.clone();
        for _ in 0..=MAX_DOCUMENT_DEPTH {
            let Some(parent) = self.parent_of(&node) else {
                break;
            };
            if matches!(self.nodes()[&parent].kind, NodeKind::Retime { .. }) {
                return Err(invalid(
                    "changing one play's Hold duration beneath Retime is not available yet",
                ));
            }
            node = parent;
        }
        // An occurrence wrapper does not yet transport duration-specific root
        // or attached sound intervals. Keep that existing refusal explicit.
        crate::sound_events::validate_command(
            self,
            &Command::EditOccurrence {
                instance: instance.clone(),
                edit: OccurrenceEdit::SetHoldDuration { duration },
                identities: OccurrenceIdentities::default(),
            },
        )?;
        isolation::temporal_requirements(self, &target)
    }
}

fn target(instance: &InstancePath) -> ScopedNodeTarget {
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

/// Validate an ordinary duration path against one shared parent index. Owned
/// gaps follow their preceding stable Play, including a dormant final branch.
/// A valid address alone does not prove that it has a project occurrence.
pub(crate) fn duration_target_dormant(
    document: &ProjectDocument,
    target: &ScopedNodeTarget,
    parents: &BTreeMap<NodeId, NodeId>,
) -> Result<bool, EditError> {
    if target.repeats.is_empty() || target.repeats.len() > MAX_DOCUMENT_DEPTH {
        return Err(invalid(
            "Hold duration requires bounded explicit Repeat ancestry",
        ));
    }
    if !document.nodes().contains_key(&target.node) {
        return Err(invalid("Hold duration target is absent"));
    }
    let mut current = &target.node;
    let mut remaining = target.repeats.len();
    let mut dormant = false;
    for _ in 0..=MAX_DOCUMENT_DEPTH {
        if current == document.root() {
            return if remaining == 0 {
                Ok(dormant)
            } else {
                Err(invalid(
                    "Hold duration target contains extra Repeat ancestry",
                ))
            };
        }
        let parent = parents
            .get(current)
            .ok_or_else(|| invalid("Hold duration target has no project ancestry"))?;
        match &document.nodes()[parent].kind {
            NodeKind::Sequence { .. } => {}
            NodeKind::Repeat {
                child, iterations, ..
            } => {
                remaining = remaining
                    .checked_sub(1)
                    .ok_or_else(|| invalid("Hold duration target omits a Repeat ancestor"))?;
                let step = &target.repeats[remaining];
                if &step.repeat != parent {
                    return Err(invalid(
                        "Hold duration target names the wrong Repeat ancestor",
                    ));
                }
                match &step.branch {
                    RepeatEditBranch::Default => {
                        if current != child {
                            return Err(invalid("Default must follow the Repeat's literal body"));
                        }
                        let owned = document
                            .overrides()
                            .get(parent)
                            .map_or(0, |entries| entries.len());
                        dormant |= u64::try_from(owned)
                            .is_ok_and(|count| count == u64::from(iterations.len()));
                    }
                    RepeatEditBranch::Play { iteration } => {
                        let position = iterations
                            .position(iteration)
                            .ok_or_else(|| invalid("Hold duration target names a retired Play"))?;
                        let body = document
                            .overrides()
                            .get(parent)
                            .and_then(|entries| entries.get(iteration))
                            .unwrap_or(child);
                        let gap = document
                            .gap_overrides()
                            .get(parent)
                            .and_then(|entries| entries.get(iteration));
                        if gap == Some(current) {
                            dormant |= position + 1 == iterations.len();
                        } else if current != body {
                            return Err(invalid(
                                "Hold duration target is outside its selected Play's owned branches",
                            ));
                        }
                    }
                }
            }
            _ => {
                return Err(invalid(
                    "Hold duration requires ordinary Sequence/Repeat ancestry without Retime or Partition",
                ));
            }
        }
        current = parent;
    }
    Err(limit("Hold duration ancestry exceeds the depth bound"))
}

#[derive(Debug)]
pub struct PreparedOccurrenceHoldDuration {
    pub transaction: EditTransaction,
    pub document: ProjectDocument,
    pub target: ScopedNodeTarget,
    pub presentation: InstancePath,
    pub boundary: ResolvedBoundary,
    isolation: ScopedIsolationRecord,
}

impl PreparedOccurrenceHoldDuration {
    pub fn isolation(&self) -> &ScopedIsolationRecord {
        &self.isolation
    }

    pub fn map_target(
        &self,
        before: &ProjectDocument,
        target: &ScopedNodeTarget,
    ) -> Result<ScopedNodeTarget, EditError> {
        self.isolation
            .bind_prepared(before, &self.document)?
            .map_forward(target)
    }

    pub fn map_instance(
        &self,
        before: &ProjectDocument,
        instance: &InstancePath,
    ) -> Result<InstancePath, EditError> {
        self.isolation.bind_prepared(before, &self.document)?;
        instance.validate(before)?;
        let result = map_instance(instance, self.isolation.steps());
        result.validate(&self.document)?;
        Ok(result)
    }
}

/// Execute precisely one EditOccurrence::SetHoldDuration and retain its actual
/// isolation plus the resized Hold's exact final start, without sampling time.
pub fn prepare_occurrence_hold_duration(
    document: &ProjectDocument,
    request: &CommandRequest,
) -> Result<PreparedOccurrenceHoldDuration, EditError> {
    crate::command::check_revision(
        document,
        &request.project_id,
        &request.expected_revision,
        &request.new_revision,
    )?;
    let Command::EditOccurrence {
        instance,
        edit: OccurrenceEdit::SetHoldDuration { duration },
        identities,
    } = &request.command
    else {
        return Err(invalid(
            "Hold duration preparation requires an EditOccurrence duration command",
        ));
    };
    let requirements = document.occurrence_hold_duration_requirements(instance, *duration)?;
    if identities.nodes.len() != requirements.nodes || identities.marks.len() != requirements.marks
    {
        return Err(invalid(
            "one-play Hold duration requires its exact node and mark identity pools",
        ));
    }
    let (transaction, after, steps) = crate::command::apply_with_isolation(document, request)?;
    let isolation = ScopedIsolationRecord::from_execution(document, request, &after, steps)?;
    let mapped = isolation
        .bind_prepared(document, &after)?
        .map_forward(&target(instance))?;
    let presentation = map_instance(instance, isolation.steps());
    if !mapped.matches_instance(&after, &presentation)? {
        return Err(invalid("resized Hold lost its concrete presentation"));
    }
    let boundary = AnchorIndex::new(&after)
        .map_err(|error| invalid(&error.to_string()))?
        .resolve_target(&AnchorTarget {
            boundary: BoundaryAnchor {
                coordinate: Anchor::Occurrence {
                    instance: presentation.clone(),
                    position: crate::ExactRatio::ZERO,
                },
                bias: InsertionBias::Right,
            },
            occurrence: None,
        })
        .map_err(|error| invalid(&error.to_string()))?;
    Ok(PreparedOccurrenceHoldDuration {
        transaction,
        document: after,
        target: mapped,
        presentation,
        boundary,
        isolation,
    })
}

fn map_instance(instance: &InstancePath, steps: &[ScopedIsolationStep]) -> InstancePath {
    let mut mapped = instance.clone();
    for step in steps {
        if isolation::matches_prefix(&mapped, step.before_prefix()) {
            crate::occurrence_edit::remap_instance(&mut mapped, step.nodes());
        }
    }
    mapped
}
