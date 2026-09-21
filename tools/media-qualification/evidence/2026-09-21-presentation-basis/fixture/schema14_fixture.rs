use deadpan_core::{AssetId, CommandRequest, NodeId, RevisionId};
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_index::SourceContentIdentity;
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_qualification::DecodedSourceQualification;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_store::generation::{ContextObservation, RelevanceObservation, RelevancePlan};
use deadpan_store::original_media::{OriginalMediaLimits, OriginalOwnership};
use deadpan_store::source_registration::{SourceInsertionRequest, SourceRegistration};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::json;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn relevance(store: &ProjectStore, next: &RevisionId) -> Result<RelevancePlan> {
    Ok(RelevancePlan {
        from_revision: store.snapshot()?.revision_id().clone(),
        to_revision: next.clone(),
        observations: store.current_generation_requests()?.into_iter().map(|request| {
            RelevanceObservation {
                request_id: request.request_id,
                after_context: ContextObservation::Resolved(request.binding.context_sha256.clone()),
                binding: request.binding,
            }
        }).collect(),
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

fn import(store: &mut ProjectStore, fixture: &str, next: &str, node: &str) -> Result<()> {
    let cancelled = AtomicBool::new(false);
    let limits = OriginalMediaLimits::new(2_000_000, Duration::from_secs(10))?;
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures")
        .join(fixture).canonicalize()?;
    let original = store.retain_original(&path, OriginalOwnership::Managed, limits, &cancelled)?.record;
    let mut snapshot = store.snapshot_original(original.object().content(), limits, &cancelled)?;
    let input = VerifiedSourceInput::copy_verified(
        &mut snapshot,
        SourceContentIdentity::new(original.sha256(), original.object().byte_length())?,
        2_000_000, Duration::from_secs(10), &cancelled,
    )?;
    let video = SourceSession::open_input(input.clone(), AssetId::new("decode-only")?, SourceSessionLimits::default(), &cancelled)?;
    let audio = AudioSession::open_input(input, 1, AudioSessionLimits::default(), &cancelled)?;
    let decoded = DecodedSourceQualification::from_sessions(Some(&video), Some(&audio))?;
    let revision = RevisionId::new(next)?;
    let request = SourceRegistration {
        expected_revision: store.snapshot()?.revision_id().clone(),
        new_revision: revision.clone(),
        original: original.object().content().clone(),
        new_asset_id: AssetId::new("qualified-camera")?,
        label: format!("Qualified {fixture}"),
        insertion: Some(SourceInsertionRequest {
            parent: store.snapshot()?.root().clone(),
            index: 0,
            node: NodeId::new(node)?,
            label: format!("Inserted {fixture}"),
        }),
    };
    let result = store.register_source(&request, &decoded, Some(&relevance(store, &revision)?), limits, &cancelled)?;
    assert!(result.commit.is_some());
    println!("{next}: {}", result.qualification);
    Ok(())
}

fn main() -> Result<()> {
    let path = std::env::args().nth(1).expect("project path");
    let mut store = ProjectStore::open(Path::new(&path), AccessMode::ReadWrite)?;
    navigate(&mut store, "redo", "schema14-redo-inherited")?;
    import(&mut store, "cfr-bframes.mp4", "schema14-first-import", "first-qualified-clip")?;
    let first = store.registered_source(&RevisionId::new("schema14-first-import")?, &AssetId::new("qualified-camera")?)?;
    navigate(&mut store, "undo", "schema14-undo-first-import")?;
    import(&mut store, "vfr.mp4", "schema14-second-import", "second-qualified-clip")?;
    let second = store.registered_source(&RevisionId::new("schema14-second-import")?, &AssetId::new("qualified-camera")?)?;
    assert_ne!(first.id(), second.id());
    assert_eq!(store.registered_source(&RevisionId::new("schema14-first-import")?, &AssetId::new("qualified-camera")?)?, first);
    let snapshot = store.snapshot()?;
    let request: CommandRequest = serde_json::from_value(json!({
        "project_id": snapshot.project_id(), "expected_revision": snapshot.revision_id(),
        "new_revision": "schema14-rename", "command": {
            "command": "rename", "node": "second-qualified-clip", "label": "Schema 14 qualified branch"
        }
    }))?;
    store.commit_reconciled(&request, &relevance(&store, &request.new_revision)?)?;
    navigate(&mut store, "undo", "schema14-undo-rename")?;
    navigate(&mut store, "redo", "schema14-redo-rename")?;
    navigate(&mut store, "undo", "schema14-pending-redo")?;
    store.validate()?;
    println!("schema14 qualified source branches and pending redo validated at {}", store.snapshot()?.revision_id());
    Ok(())
}
