//! Isolate an occurrence before its Split captures sound transport. The actual
//! cut then uses the ordinary retained-context reducer and one net transaction.

use std::collections::BTreeSet;

use crate::{
    Command, CommandRequest, EditError, EditErrorCode, MAX_DOCUMENT_MARKS, MAX_DOCUMENT_NODES,
    OccurrenceEdit, OccurrenceIdentities, ProjectDocument, RepeatEditBranch, RepeatEditStep,
    ScopedIsolationStep, ScopedNodeTarget,
};

pub(crate) struct Prepared {
    pub document: ProjectDocument,
    pub command: Command,
    pub isolation: Vec<ScopedIsolationStep>,
}

pub(crate) fn prepare(
    document: &ProjectDocument,
    request: &CommandRequest,
) -> Result<Option<Prepared>, EditError> {
    let Command::EditOccurrence {
        instance,
        edit: OccurrenceEdit::Split { at, identities },
        identities: supplied,
    } = &request.command
    else {
        return Ok(None);
    };
    instance.validate(document)?;
    let duration = document.durations_shared()?[&instance.node];
    if at.frames() <= 0 || *at >= duration {
        return Err(invalid("split must be strictly inside the selected beat"));
    }
    let target = ScopedNodeTarget {
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
    };
    let required = crate::scoped_edit::temporal_requirements(document, &target)?;
    if supplied.nodes.len() < required.nodes || supplied.marks.len() < required.marks {
        return Err(invalid(
            "occurrence Split needs its complete isolation identity pools",
        ));
    }
    if supplied.nodes.len() > MAX_DOCUMENT_NODES
        || supplied.marks.len() > MAX_DOCUMENT_MARKS
        || identities.nodes.len() > MAX_DOCUMENT_NODES
    {
        return Err(invalid(
            "occurrence Split identity pools exceed document limits",
        ));
    }
    // Historical audio names are occupied too. The cut's identity pool cannot
    // alias the isolation pool, even though both phases share one allocation.
    let mut occupied = crate::scoped_edit::occupied_node_ids(document);
    if supplied
        .nodes
        .iter()
        .chain(&identities.nodes)
        .any(|node| !occupied.insert(node))
    {
        return Err(invalid(
            "occurrence Split identities must be fresh and distinct, including retained audio names",
        ));
    }
    let mut marks = BTreeSet::new();
    if supplied
        .marks
        .iter()
        .any(|mark| document.marks().contains_key(mark) || !marks.insert(mark))
    {
        return Err(invalid(
            "occurrence Split mark identities must be fresh and distinct",
        ));
    }
    // EditOccurrence historically permits unused identities. Keep that wire
    // behavior while passing only the exact consumed prefix to scoped isolation.
    let isolation = OccurrenceIdentities {
        nodes: supplied.nodes[..required.nodes].to_vec(),
        marks: supplied.marks[..required.marks].to_vec(),
    };
    let mut allowances =
        crate::sound_allowance::SoundAllowanceEdit::capture(document, &request.command);
    let mut structural = document.clone();
    structural.sound_allowances.clear();
    let (mut result, mapped, maps) = crate::scoped_edit::isolate_temporal(
        &structural,
        &target,
        &isolation,
        crate::command::EditContext {
            allocation: &request.new_revision,
            allowances: allowances.as_mut(),
            sound_clocks: None,
        },
    )?;
    if let Some(allowances) = allowances {
        allowances.restore(&mut result)?;
    }
    result.validate()?;
    Ok(Some(Prepared {
        document: result,
        command: Command::Split {
            node: mapped.node,
            at: *at,
            identities: identities.clone(),
        },
        isolation: maps.into_iter().map(|mapping| mapping.proof()).collect(),
    }))
}

fn invalid(message: &str) -> EditError {
    EditError::new(EditErrorCode::InvalidCommand, message)
}

#[cfg(test)]
mod tests;
