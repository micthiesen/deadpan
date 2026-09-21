use deadpan_core::{CommandRequest, RevisionId};
use deadpan_store::generation::{ContextObservation, RelevanceObservation, RelevancePlan};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::{Value, json};
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn relevance(store: &ProjectStore, next: &RevisionId) -> Result<RelevancePlan> {
    Ok(RelevancePlan {
        from_revision: store.snapshot()?.revision_id().clone(),
        to_revision: next.clone(),
        observations: store
            .current_generation_requests()?
            .into_iter()
            .map(|r| RelevanceObservation {
                request_id: r.request_id,
                after_context: ContextObservation::Resolved(r.binding.context_sha256.clone()),
                binding: r.binding,
            })
            .collect(),
    })
}
fn navigate(store: &mut ProjectStore, action: &str, next: &str) -> Result<()> {
    let next = RevisionId::new(next)?;
    let plan = relevance(store, &next)?;
    let head = store.snapshot()?.revision_id().clone();
    if action == "undo" {
        store.undo_reconciled(&head, next, &plan)?;
    } else {
        store.redo_reconciled(&head, next, &plan)?;
    }
    Ok(())
}
fn commit(store: &mut ProjectStore, next: &str, command: Value) -> Result<()> {
    let snapshot = store.snapshot()?;
    let request: CommandRequest = serde_json::from_value(
        json!({"project_id":snapshot.project_id(), "expected_revision":snapshot.revision_id(), "new_revision":next, "command":command}),
    )?;
    store.commit_reconciled(&request, &relevance(store, &request.new_revision)?)?;
    Ok(())
}
fn main() -> Result<()> {
    let mut store = ProjectStore::open(
        Path::new(&std::env::args().nth(1).expect("project path")),
        AccessMode::ReadWrite,
    )?;
    navigate(&mut store, "redo", "schema12-redo-inherited")?;
    for (next, command) in [
        (
            "schema12-video-negative",
            json!({"command":"set_source_video_mapping", "node":"source-negative", "mapping":{"type":"duration", "frames":{"numerator":"28750", "denominator":"1001"}, "endpoints":"hold_adjacent"}}),
        ),
        (
            "schema12-video-positive",
            json!({"command":"edit_occurrence", "instance":{"node":"source-positive","repeats":[]}, "edit":{"type":"set_source_video_mapping", "mapping":{"type":"duration", "frames":{"numerator":"120000", "denominator":"1001"}, "endpoints":"reject"}}, "identities":{"nodes":[],"marks":[]}}),
        ),
        (
            "schema12-local-mark",
            json!({"command":"set_mark", "id":"picture-mapping-local-mark", "owner":"source-negative", "label":"Exact picture mapping boundary", "boundary":{"coordinate":{"space":"local", "node":"source-negative", "position":{"numerator":"13", "denominator":"7"}}, "bias":"right"}, "loss_policy":"keep_unresolved"}),
        ),
        (
            "schema12-abandoned-rename",
            json!({"command":"rename", "node":"source-negative", "label":"Abandoned schema 12 branch"}),
        ),
    ] {
        commit(&mut store, next, command)?;
    }
    navigate(&mut store, "undo", "schema12-undo-abandoned")?;
    commit(
        &mut store,
        "schema12-rename-branch",
        json!({"command":"rename", "node":"source-negative", "label":"Schema 12 independent streams"}),
    )?;
    for (action, next) in [
        ("undo", "schema12-undo-branch"),
        ("redo", "schema12-redo-branch"),
        ("undo", "schema12-pending-redo"),
    ] {
        navigate(&mut store, action, next)?;
    }
    store.validate()?;
    println!(
        "schema12 exact picture and audio mappings, abandoned branch, and pending redo validated at {}",
        store.snapshot()?.revision_id()
    );
    Ok(())
}
