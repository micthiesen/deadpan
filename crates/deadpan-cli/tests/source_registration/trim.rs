use super::*;

use std::collections::BTreeMap;

use deadpan_core::{BeatNode, CommandRequest, RevisionId, SourceTrimEdge, Subtree};

fn ready_trim(directory: &Path) -> Result<PathBuf> {
    let package = directory.join("trim.deadpan");
    create(&package, "30000/1001")?;
    let original = retain(
        &package,
        &directory.join("source.mp4"),
        include_bytes!("../../../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4"),
        false,
    )?;
    let mut input = request(
        &package,
        original,
        json!({"type":"video_and_audio","audio_stream":1}),
        "registered",
    )?;
    input["registration"]["insertion"] = Value::Null;
    registration(&package, &save(directory, &input)?, false, true)?;

    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let before = store.snapshot()?;
    let asset = AssetId::new("imported-asset")?;
    let receipt = store.registered_source(before.revision_id(), &asset)?;
    let timing = deadpan_media::source_import_timing::derive_source_moment(
        receipt.snapshot().video().unwrap().index(),
        receipt.snapshot().audio(),
        10..13,
        before.presentation_basis().frame_rate,
    )?;
    store.commit(&CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: RevisionId::new("ready")?,
        command: deadpan_core::Command::Insert {
            parent: before.root().clone(),
            index: 0,
            subtree: Subtree {
                root: NodeId::new("moment")?,
                nodes: BTreeMap::from([(
                    NodeId::new("moment")?,
                    BeatNode {
                        label: "Selected Original".into(),
                        framing: None,
                        audio_treatments: Default::default(),
                        audio_editorial_edges: Default::default(),
                        audio_edges: Default::default(),
                        kind: NodeKind::Source {
                            source: timing.source_node(asset),
                        },
                        cutaways: Vec::new(),
                    },
                )]),
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
    })?;
    drop(store);
    Ok(package)
}

fn trim_envelope(
    snapshot: &deadpan_core::ProjectDocument,
    new_revision: &str,
    edge: SourceTrimEdge,
    delta_frames: i64,
    wrapper: Option<&str>,
) -> Value {
    json!({
        "protocol": 1,
        "project_id": snapshot.project_id(),
        "expected_revision": snapshot.revision_id(),
        "new_revision": new_revision,
        "command": {
            "command": "trim_source",
            "parent": snapshot.root(),
            "node": "moment",
            "edge": edge,
            "delta_frames": delta_frames,
            "mode": "ripple",
            "wrapper": wrapper,
            "timing": {"allocation": new_revision, "ordinal": 0}
        }
    })
}

#[test]
fn headless_trim_reports_zero_and_one_commit_preview_consistently() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = ready_trim(scratch.path())?;
    let path = package.to_str().unwrap();
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let initial_counts = counts(&package)?;

    let noop = trim_envelope(&before, "noop-preview", SourceTrimEdge::In, 0, None);
    let noop_file = save(scratch.path(), &noop)?;
    let noop_file = noop_file.to_str().unwrap();
    let noop_preview = success(&["command", path, "--json", noop_file, "--dry-run"])?;
    assert_eq!(noop_preview["committed"], false);
    assert_eq!(noop_preview["source_trim"]["applied_delta_frames"], 0);
    assert!(noop_preview["edit"].is_null());
    assert_eq!(counts(&package)?, initial_counts);
    assert_eq!(
        failure(&["command", path, "--json", noop_file])?["error"]["code"],
        "InvalidCommand"
    );

    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let envelope = trim_envelope(
        &before,
        "trimmed",
        SourceTrimEdge::In,
        100,
        Some("trim-wrapper"),
    );
    let file = save(scratch.path(), &envelope)?;
    let file = file.to_str().unwrap();
    let preview = success(&["command", path, "--json", file, "--dry-run"])?;
    assert_eq!(preview["committed"], false);
    assert_eq!(preview["source_trim"]["requested_delta_frames"], 100);
    let applied = preview["source_trim"]["applied_delta_frames"]
        .as_i64()
        .unwrap();
    assert!(applied > 0 && applied < 100);
    assert_eq!(preview["source_trim"]["duration_delta_frames"], -applied);
    assert!(preview["source_trim"]["clamp"].is_string());
    assert_eq!(preview["source_trim"]["needs_wrapper"], true);
    assert!(preview["edit"].is_object());
    assert_eq!(counts(&package)?, initial_counts);
    assert_eq!(writer.snapshot()?, before);

    let request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: RevisionId::new("trimmed")?,
        command: serde_json::from_value(envelope["command"].clone())?,
    };
    let deadpan_cli::macros::Execution {
        output: hosted,
        committed_revision: revision,
        ..
    } = deadpan_cli::live_project::execute_short(
        &mut writer,
        before.project_id(),
        &deadpan_cli::live_project::ShortOperation::Edit {
            request: Box::new(request),
            dry_run: true,
        },
    )?;
    assert_eq!(hosted, preview);
    assert!(revision.is_none());
    drop(writer);

    let committed = success(&["command", path, "--json", file])?;
    assert_eq!(committed["committed"], true);
    assert_eq!(committed["outcome"]["edit"], preview["edit"]);
    assert_eq!(
        counts(&package)?,
        (initial_counts.0 + 1, initial_counts.1 + 1, initial_counts.2)
    );
    let after = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_eq!(
        after.nodes()[&NodeId::new("trim-wrapper")?].audio_editorial_edges,
        deadpan_core::AudioEditorialEdges {
            start: true,
            end: false
        }
    );
    assert!(
        after.nodes()[&NodeId::new("moment")?]
            .audio_editorial_edges
            .is_empty()
    );
    assert_eq!(
        after.duration()?.frames(),
        before.duration()?.frames() - applied
    );
    assert_eq!(
        failure(&["command", path, "--json", file, "--dry-run"])?["error"]["code"],
        "RevisionConflict"
    );
    success(&["project", "undo", path, "--expected", "trimmed"])?;
    let undone = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_eq!(undone.nodes(), before.nodes());
    assert_ne!(undone.revision_id(), before.revision_id());
    success(&[
        "project",
        "redo",
        path,
        "--expected",
        undone.revision_id().as_str(),
    ])?;
    let redone = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_eq!(redone.nodes(), after.nodes());
    assert_ne!(redone.revision_id(), after.revision_id());
    Ok(())
}
