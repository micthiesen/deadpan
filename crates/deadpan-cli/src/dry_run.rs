//! Spec §6.4 dry-run observations beyond the transaction itself: warnings a
//! caller should see before committing and the background jobs a commit
//! would need. Both are derived from the previewed transaction and the
//! store's current operational state; neither writes anything.

use std::collections::BTreeSet;

use deadpan_core::{Command, CommandRequest, EditTransaction, MarkState, NodeId};
use deadpan_store::ProjectStore;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Warning {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RequiredJob {
    pub job: &'static str,
    pub target: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Observations {
    pub warnings: Vec<Warning>,
    pub required_jobs: Vec<RequiredJob>,
}

/// Observe a previewed edit. Store read failures are reported as a warning
/// rather than failing the dry run, which already validated the edit.
pub fn observe(
    store: &ProjectStore,
    request: &CommandRequest,
    edit: &EditTransaction,
) -> Observations {
    let mut observations = Observations::default();
    let forward = &edit.forward;
    let mut removed = Vec::new();
    let mut unresolved = Vec::new();
    for (id, change) in &forward.marks {
        match (&change.before, &change.after) {
            (Some(_), None) => removed.push(id.to_string()),
            (Some(before), Some(after))
                if matches!(before.state, MarkState::Bound)
                    && matches!(after.state, MarkState::Unresolved { .. }) =>
            {
                unresolved.push(id.to_string())
            }
            _ => {}
        }
    }
    if !removed.is_empty() {
        observations.warnings.push(Warning {
            code: "marks_removed",
            message: "this edit removes marks whose content it deletes".into(),
            ids: removed,
        });
    }
    if !unresolved.is_empty() {
        observations.warnings.push(Warning {
            code: "marks_unresolved",
            message: "this edit leaves marks unresolved; they will not reattach on their own"
                .into(),
            ids: unresolved,
        });
    }
    let sounds: Vec<String> = forward
        .sounds
        .iter()
        .filter(|(_, change)| change.before.is_some() && change.after.is_none())
        .map(|(id, _)| id.to_string())
        .collect();
    if !sounds.is_empty() {
        observations.warnings.push(Warning {
            code: "sounds_removed",
            message: "this edit removes placed sounds".into(),
            ids: sounds,
        });
    }
    let clocked: Vec<String> = forward
        .derived_clocks
        .iter()
        .filter(|(_, change)| change.before.is_none() && change.after.is_some())
        .map(|(id, _)| id.to_string())
        .collect();
    if !clocked.is_empty() {
        observations.warnings.push(Warning {
            code: "exact_speed_change_clocks",
            message: "this edit gives speed changes exact pause clocks; later edits inside them are limited to pause durations".into(),
            ids: clocked,
        });
    }
    let changed: BTreeSet<&NodeId> = edit.changed_ids.iter().collect();
    match store.current_generation_requests() {
        Ok(requests) => {
            let touched: Vec<String> = requests
                .iter()
                .filter(|request| changed.contains(&request.target.node))
                .map(|request| request.request_id.as_str().to_owned())
                .collect();
            if !touched.is_empty() {
                observations.warnings.push(Warning {
                    code: "ai_requests_may_go_stale",
                    message: "this edit changes a pause with pending or ready AI candidates; they become stale if their inputs change".into(),
                    ids: touched,
                });
            }
        }
        Err(error) => observations.warnings.push(Warning {
            code: "ai_requests_unknown",
            message: format!("AI request relevance could not be read: {error}"),
            ids: Vec::new(),
        }),
    }
    collect_jobs(&request.command, &mut observations.required_jobs);
    observations
}

fn collect_jobs(command: &Command, jobs: &mut Vec<RequiredJob>) {
    match command {
        Command::InsertAiTime { id, .. } => jobs.push(RequiredJob {
            job: "ai_generation",
            target: id.to_string(),
        }),
        Command::InsertOccurrenceTime { id, ai: true, .. } => jobs.push(RequiredJob {
            job: "ai_generation",
            target: id.to_string(),
        }),
        Command::Compound { transaction } => {
            for step in transaction.steps() {
                if let deadpan_core::ResolvedStep::Edit { edit } = step {
                    collect_jobs(edit.command.as_command(), jobs);
                }
            }
        }
        _ => {}
    }
}
