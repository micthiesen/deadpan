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
    navigate(&mut store, "redo", "schema13-redo-inherited")?;
    for (next, command) in [
        (
            "schema13-place-video-negative",
            json!({"command":"set_source_video_mapping", "node":"source-negative", "mapping":{"type":"placement", "start":{"numerator":"2", "denominator":"3"}, "frames":{"numerator":"28750", "denominator":"1001"}, "endpoints":"hold_adjacent"}}),
        ),
        (
            "schema13-place-audio-negative",
            json!({"command":"edit_occurrence", "instance":{"node":"source-negative","repeats":[]}, "edit":{"type":"set_source_audio_mapping", "mapping":{"type":"placement", "start":{"numerator":"-1", "denominator":"147"}, "frames":{"numerator":"60000", "denominator":"1001"}}, "offset":-137}, "identities":{"nodes":[],"marks":[]}}),
        ),
        (
            "schema13-place-video-positive",
            json!({"command":"edit_occurrence", "instance":{"node":"source-positive","repeats":[]}, "edit":{"type":"set_source_video_mapping", "mapping":{"type":"placement", "start":{"numerator":"-3", "denominator":"7"}, "frames":{"numerator":"120000", "denominator":"1001"}, "endpoints":"reject"}}, "identities":{"nodes":[],"marks":[]}}),
        ),
        (
            "schema13-place-audio-positive",
            json!({"command":"set_source_audio_mapping", "node":"source-positive", "mapping":{"type":"placement", "start":{"numerator":"3", "denominator":"7"}, "frames":{"numerator":"120000", "denominator":"1001"}}, "offset":2401}),
        ),
        (
            "schema13-local-mark",
            json!({"command":"set_mark", "id":"placement-local-mark", "owner":"source-negative", "label":"Exact signed placement boundary", "boundary":{"coordinate":{"space":"local", "node":"source-negative", "position":{"numerator":"13", "denominator":"7"}}, "bias":"right"}, "loss_policy":"keep_unresolved"}),
        ),
        (
            "schema13-abandoned-rename",
            json!({"command":"rename", "node":"source-negative", "label":"Abandoned schema 13 branch"}),
        ),
    ] {
        commit(&mut store, next, command)?;
    }
    navigate(&mut store, "undo", "schema13-undo-abandoned")?;
    commit(
        &mut store,
        "schema13-rename-branch",
        json!({"command":"rename", "node":"source-negative", "label":"Schema 13 signed stream placements"}),
    )?;
    for (action, next) in [
        ("undo", "schema13-undo-branch"),
        ("redo", "schema13-redo-branch"),
        ("undo", "schema13-pending-redo"),
    ] {
        navigate(&mut store, action, next)?;
    }
    store.validate()?;
    println!(
        "schema13 signed A/V placements, unrelated operational rows, abandoned branch, and pending redo validated at {}",
        store.snapshot()?.revision_id()
    );
    Ok(())
}
