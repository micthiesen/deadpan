use deadpan_core::{CommandRequest, RevisionId};
use deadpan_store::{AccessMode, ProjectStore};
use deadpan_store::generation::{ContextObservation, RelevanceObservation, RelevancePlan};
use serde_json::{Value, json};
use std::path::Path;

fn relevance(store: &ProjectStore, next: &RevisionId) -> Result<RelevancePlan, Box<dyn std::error::Error>> {
    Ok(RelevancePlan {
        from_revision: store.snapshot()?.revision_id().clone(),
        to_revision: next.clone(),
        observations: store.current_generation_requests()?.into_iter().map(|r| RelevanceObservation {
            request_id: r.request_id,
            after_context: ContextObservation::Resolved(r.binding.context_sha256.clone()),
            binding: r.binding,
        }).collect(),
    })
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = ProjectStore::open(Path::new(&std::env::args().nth(1).expect("project path")), AccessMode::ReadWrite)?;
    let span = json!({"start":{"ticks":-48000,"time_base":{"numerator":1,"denominator":48000}},"end":{"ticks":144000,"time_base":{"numerator":1,"denominator":48000}}});
    let video_span = json!({"start":{"ticks":0,"time_base":{"numerator":1,"denominator":1000}},"end":{"ticks":4000,"time_base":{"numerator":1,"denominator":1000}}});
    let mut operations: Vec<(&str,Value)> = vec![
        ("schema10-add-av-asset", json!({"command":"add_asset", "id":"original-av", "asset": {"label":"Original A/V with distinct origins", "content_hash":"7c28b611752f7d185482c9a832a951d5b1e7cfcd60bc1910d8d1281350ac33c4", "video":video_span, "audio":span, "still_image":false, "frame_count":120}})),
    ];
    for (id, offset) in [("source-negative", -137), ("source-positive", 2401)] {
        operations.push((if offset < 0 {"schema10-insert-negative"} else {"schema10-insert-positive"}, json!({"command":"insert", "parent":"acceptance-probe-root", "index": if offset < 0 {1} else {2}, "subtree": {"root":id, "nodes":{id:{"label":id, "kind":{"type":"source", "source":{"duration":60,"video":{"type":"stream","asset":"original-av","span":video_span},"audio":{"asset":"original-av","span":span},"link":"linked","audio_offset":offset}}}}, "overrides":{}}})));
    }
    operations.push(("schema10-source-mark", json!({"command":"set_mark", "id":"source-audio-mark", "owner":"source-negative", "label":"Original audio zero", "boundary":{"coordinate":{"space":"source", "asset":"original-av", "moment":{"type":"audio_sample","sample":0,"sample_rate":48000}},"bias":"right"},"loss_policy":"keep_unresolved"})));
    operations.push(("schema10-local-mark", json!({"command":"set_mark", "id":"local-mark", "owner":"source-positive", "label":"Exact local boundary", "boundary":{"coordinate":{"space":"local", "node":"source-positive", "position":{"numerator":"7","denominator":"3"}},"bias":"left"},"loss_policy":"keep_unresolved"})));
    operations.push(("schema10-rename-source", json!({"command":"rename","node":"source-negative","label":"Renamed source with negative offset"})));
    for (next,command) in operations {
        let snapshot = store.snapshot()?;
        let request: CommandRequest = serde_json::from_value(json!({"project_id":snapshot.project_id(), "expected_revision":snapshot.revision_id(), "new_revision":next, "command":command}))?;
        store.commit_reconciled(&request, &relevance(&store, &request.new_revision)?)?;
    }
    for (action, next) in [("undo", "schema10-undo-source"), ("redo", "schema10-redo-source"), ("undo", "schema10-pending-redo")] {
        let next = RevisionId::new(next)?;
        let plan = relevance(&store, &next)?;
        let head = store.snapshot()?.revision_id().clone();
        if action == "undo" {store.undo_reconciled(&head, next, &plan)?;} else {store.redo_reconciled(&head, next, &plan)?;}
    }
    store.validate()?;
    println!("schema10 source chronology validated at {}", store.snapshot()?.revision_id());
    Ok(())
}
