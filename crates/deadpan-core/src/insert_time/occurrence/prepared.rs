//! Exact continuation of the production pause command, never a cursor sample.

use super::*;
use crate::{
    Anchor, AnchorIndex, AnchorTarget, BoundaryAnchor, CommandRequest, EditTransaction,
    InsertionBias, RepeatEditBranch, RepeatEditStep, RepeatInstance, ResolvedBoundary,
    ScopedIsolationRecord, ScopedNodeTarget,
};

#[derive(Debug)]
pub struct PreparedOccurrenceTime {
    pub transaction: EditTransaction,
    pub document: ProjectDocument,
    pub target: ScopedNodeTarget,
    pub presentation: InstancePath,
    pub boundary: ResolvedBoundary,
    isolation: ScopedIsolationRecord,
}

impl PreparedOccurrenceTime {
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

/// Execute one actual pause reducer and retain its inserted Hold's exact
/// authoring address, concrete presentation and final zero-local boundary.
pub fn prepare_occurrence_time(
    document: &ProjectDocument,
    request: &CommandRequest,
) -> Result<PreparedOccurrenceTime, EditError> {
    crate::command::check_revision(
        document,
        &request.project_id,
        &request.expected_revision,
        &request.new_revision,
    )?;
    let Command::InsertOccurrenceTime {
        at,
        within,
        target: authority,
        all_plays,
        id,
        ..
    } = &request.command
    else {
        return Err(invalid(
            "occurrence preparation requires an InsertOccurrenceTime command",
        ));
    };
    let site = target::for_authority(
        document,
        *at,
        within.as_ref(),
        authority.as_ref(),
        *all_plays,
    )?;
    let mut scope = site.scope();
    let mut presentation = site_instance(&site.site).clone();
    // An unmaterialized default gap has no physical occurrence yet. The
    // insertion materializes that exact gap under the named Repeat play.
    if let Site::Gap { repeat, after, .. } = &site.site {
        scope.repeats.push(RepeatEditStep {
            repeat: repeat.node.clone(),
            branch: RepeatEditBranch::Play {
                iteration: after.clone(),
            },
        });
        presentation.repeats.push(RepeatInstance {
            node: repeat.node.clone(),
            iteration: after.clone(),
        });
    }
    let (transaction, after, steps) = crate::command::apply_with_isolation(document, request)?;
    let isolation = ScopedIsolationRecord::from_execution(document, request, &after, steps)?;
    // The physical leaf can be replaced by a split. Map only the retained
    // ancestry, then install the new Hold identity before validating ownership.
    scope = isolation
        .bind_prepared(document, &after)?
        .map_retained_forward(&scope);
    presentation = map_instance(&presentation, isolation.steps());
    scope.node = id.clone();
    presentation.node = id.clone();
    if !scope.matches_instance(&after, &presentation)? {
        return Err(invalid(
            "inserted pause lost its concrete authoring address",
        ));
    }
    let boundary = AnchorIndex::new(&after)
        .map_err(|error| invalid(&error.to_string()))?
        .resolve_target(&AnchorTarget {
            boundary: BoundaryAnchor {
                coordinate: Anchor::Occurrence {
                    instance: presentation.clone(),
                    position: ExactRatio::ZERO,
                },
                bias: InsertionBias::Right,
            },
            occurrence: None,
        })
        .map_err(|error| invalid(&error.to_string()))?;
    Ok(PreparedOccurrenceTime {
        transaction,
        document: after,
        target: scope,
        presentation,
        boundary,
        isolation,
    })
}

fn map_instance(instance: &InstancePath, steps: &[crate::ScopedIsolationStep]) -> InstancePath {
    let mut mapped = instance.clone();
    for step in steps {
        if crate::scoped_edit::matches_prefix(&mapped, step.before_prefix()) {
            crate::occurrence_edit::remap_instance(&mut mapped, step.nodes());
        }
    }
    mapped
}
