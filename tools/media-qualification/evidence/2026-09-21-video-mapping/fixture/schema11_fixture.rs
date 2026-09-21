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
    navigate(&mut store, "redo", "schema11-redo-inherited")?;
    for (next, command) in [
        (
            "schema11-audio-negative",
            json!({"command":"set_source_audio_mapping", "node":"source-negative", "mapping":{"type":"duration", "frames":{"numerator":"60000", "denominator":"1001"}}, "offset":-137}),
        ),
        (
            "schema11-audio-positive",
            json!({"command":"edit_occurrence", "instance":{"node":"source-positive","repeats":[]}, "edit":{"type":"set_source_audio_mapping", "mapping":{"type":"duration", "frames":{"numerator":"120000", "denominator":"1001"}}, "offset":2401}, "identities":{"nodes":[],"marks":[]}}),
        ),
        (
            "schema11-local-mark",
            json!({"command":"set_mark", "id":"mapping-local-mark", "owner":"source-negative", "label":"Exact mapping boundary", "boundary":{"coordinate":{"space":"local", "node":"source-negative", "position":{"numerator":"13", "denominator":"7"}}, "bias":"right"}, "loss_policy":"keep_unresolved"}),
        ),
        (
            "schema11-abandoned-rename",
            json!({"command":"rename", "node":"source-negative", "label":"Abandoned schema 11 branch"}),
        ),
    ] {
        commit(&mut store, next, command)?;
    }
    navigate(&mut store, "undo", "schema11-undo-abandoned")?;
    commit(
        &mut store,
        "schema11-rename-branch",
        json!({"command":"rename", "node":"source-negative", "label":"Schema 11 independent audio"}),
    )?;
    for (action, next) in [
        ("undo", "schema11-undo-branch"),
        ("redo", "schema11-redo-branch"),
        ("undo", "schema11-pending-redo"),
    ] {
        navigate(&mut store, action, next)?;
    }
    store.validate()?;
    println!(
        "schema11 exact audio mappings, abandoned branch, and pending redo validated at {}",
        store.snapshot()?.revision_id()
    );
    Ok(())
}
