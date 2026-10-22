//! Temporal isolation preserving literal mixed Default/Play authority.

use super::*;
use crate::{
    Anchor, AnchorIndex, AnchorTarget, BoundaryAnchor, FrameDuration, InsertionBias,
    ResolvedBoundary,
};

impl ProjectDocument {
    /// Exact identity pools for a mixed-scope Hold resize. This does not isolate
    /// or invent a concrete presentation for an unused definition.
    pub fn scoped_hold_duration_requirements(
        &self,
        target: &ScopedNodeTarget,
        duration: FrameDuration,
    ) -> Result<ScopedEditRequirements, EditError> {
        mixed_body_dormant(self, target)?;
        let NodeKind::Hold { recipe } = &self.nodes()[&target.node].kind else {
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
        crate::sound_events::validate_command(
            self,
            &Command::SetScopedHoldDuration {
                target: target.clone(),
                duration,
                identities: OccurrenceIdentities::default(),
            },
        )?;
        temporal_requirements(self, target)
    }
}

/// Mixed body/gap liveness without inventing a concrete dormant occurrence.
pub(crate) fn mixed_body_dormant(
    document: &ProjectDocument,
    target: &ScopedNodeTarget,
) -> Result<bool, EditError> {
    if !target
        .repeats
        .iter()
        .any(|step| step.branch == RepeatEditBranch::Default)
        || !target
            .repeats
            .iter()
            .any(|step| matches!(step.branch, RepeatEditBranch::Play { .. }))
    {
        return Err(invalid(
            "mixed Hold duration requires both Default and explicit Play ancestry",
        ));
    }
    let mut parents = BTreeMap::new();
    for id in document.nodes().keys() {
        for child in document.children(id) {
            parents.insert(child.clone(), id.clone());
        }
    }
    super::duration_target_dormant(document, target, &parents)
}

pub(crate) struct ResolvedDuration {
    pub document: ProjectDocument,
    pub command: Command,
    pub isolation: Vec<ScopedIsolationStep>,
}

pub(crate) fn prepare_command(
    document: &ProjectDocument,
    request: &CommandRequest,
) -> Result<Option<ResolvedDuration>, EditError> {
    let Command::SetScopedHoldDuration {
        target,
        duration,
        identities,
    } = &request.command
    else {
        return Ok(None);
    };
    let requirements = document.scoped_hold_duration_requirements(target, *duration)?;
    if identities.nodes.len() != requirements.nodes || identities.marks.len() != requirements.marks
    {
        return Err(invalid(
            "mixed Hold duration requires its exact node and mark identity pools",
        ));
    }
    let (after, mapped, maps) = isolate_temporal(
        document,
        target,
        identities,
        crate::command::EditContext {
            allocation: &request.new_revision,
            allowances: None,
            sound_clocks: None,
        },
    )?;
    Ok(Some(ResolvedDuration {
        document: after,
        command: Command::SetHoldDuration {
            node: mapped.node,
            duration: *duration,
        },
        isolation: maps.into_iter().map(|mapping| mapping.proof()).collect(),
    }))
}

/// The actual isolated resize, with revision-bound authoring and presentation
/// mappings. A missing presentation is accepted only for a proven dormant body.
#[derive(Debug)]
pub struct PreparedScopedHoldDuration {
    pub transaction: EditTransaction,
    pub document: ProjectDocument,
    pub target: ScopedNodeTarget,
    pub presentation: Option<InstancePath>,
    pub boundary: Option<ResolvedBoundary>,
    isolation: ScopedIsolationRecord,
}

impl PreparedScopedHoldDuration {
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
        self.isolation
            .bind_prepared(before, &self.document)?
            .map_instance_forward(instance)
    }
}

pub fn prepare_scoped_hold_duration(
    document: &ProjectDocument,
    request: &CommandRequest,
    presentation: Option<&InstancePath>,
) -> Result<PreparedScopedHoldDuration, EditError> {
    crate::command::check_revision(
        document,
        &request.project_id,
        &request.expected_revision,
        &request.new_revision,
    )?;
    let Command::SetScopedHoldDuration {
        target, duration, ..
    } = &request.command
    else {
        return Err(invalid(
            "mixed Hold duration preparation requires SetScopedHoldDuration",
        ));
    };
    document.scoped_hold_duration_requirements(target, *duration)?;
    let dormant = mixed_body_dormant(document, target)?;
    match (presentation, dormant) {
        (None, true) => {}
        (Some(instance), false) if target.matches_instance(document, instance)? => {}
        _ => {
            return Err(invalid(
                "mixed Hold duration requires its live presentation or a proven dormant definition",
            ));
        }
    }
    let before_duration = document.structural_duration()?;
    let (transaction, after, steps) = crate::command::apply_with_isolation(document, request)?;
    let isolation = ScopedIsolationRecord::from_execution(document, request, &after, steps)?;
    let proof = isolation.bind_prepared(document, &after)?;
    let mapped = proof.map_forward(target)?;
    let presentation = presentation
        .map(|instance| proof.map_instance_forward(instance))
        .transpose()?;
    if mixed_body_dormant(&after, &mapped)? != dormant
        || (dormant && after.structural_duration()? != before_duration)
    {
        return Err(invalid(
            "mixed Hold duration changed dormant definition visibility",
        ));
    }
    let boundary = presentation
        .as_ref()
        .map(|instance| {
            if !mapped.matches_instance(&after, instance)? {
                return Err(invalid("resized Hold lost its concrete presentation"));
            }
            crate::derived_clock::with_sealed(|| {
                AnchorIndex::new(&after)
                    .and_then(|index| {
                        index.resolve_target(&AnchorTarget {
                            boundary: BoundaryAnchor {
                                coordinate: Anchor::Occurrence {
                                    instance: instance.clone(),
                                    position: crate::ExactRatio::ZERO,
                                },
                                bias: InsertionBias::Right,
                            },
                            occurrence: None,
                        })
                    })
                    .map_err(|error| invalid(&error.to_string()))
            })
        })
        .transpose()?;
    Ok(PreparedScopedHoldDuration {
        transaction,
        document: after,
        target: mapped,
        presentation,
        boundary,
        isolation,
    })
}
