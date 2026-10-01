use super::*;
use std::collections::BTreeMap;

use deadpan_core::{BeatNode, CommandRequest, RevisionId, Subtree};
use deadpan_media::source_import_timing::derive_source_moment;

fn ready(directory: &Path) -> Result<PathBuf> {
    let package = directory.join("slip.deadpan");
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
    let timing = derive_source_moment(
        receipt.snapshot().video().unwrap().index(),
        receipt.snapshot().audio(),
        10..13,
        before.presentation_basis().frame_rate,
    )?;
    let node = NodeId::new("moment")?;
    store.commit(&CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: RevisionId::new("ready")?,
        command: deadpan_core::Command::Insert {
            parent: before.root().clone(),
            index: 0,
            subtree: Subtree {
                root: node.clone(),
                nodes: BTreeMap::from([(
                    node,
                    BeatNode {
                        label: "Selected moment".into(),
                        framing: None,
                        audio_treatments: Default::default(),
                        audio_editorial_edges: Default::default(),
                        audio_edges: Default::default(),
                        kind: NodeKind::Source {
                            source: timing.source_node(asset),
                        },
                    },
                )]),
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
    })?;
    Ok(package)
}

#[test]
fn headless_slip_reports_clamp_and_noop_without_losing_revision_guards() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = ready(scratch.path())?;
    let path = package.to_str().unwrap();
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let original_counts = counts(&package)?;
    let envelope = json!({
        "protocol":1, "project_id":before.project_id(),
        "expected_revision":before.revision_id(), "new_revision":"slipped",
        "command":{"command":"slip_source", "parent":before.root(),
                   "node":"moment", "delta_frames":-100}
    });
    let file = save(scratch.path(), &envelope)?;
    let file = file.to_str().unwrap();
    // The first ten source frames are the complete earlier picture handle.
    // A dry run does not reserve its future revision or need the writer lock.
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let preview = success(&["command", path, "--json", file, "--dry-run"])?;
    assert_eq!(preview["committed"], false);
    assert_eq!(preview["source_slip"]["requested_delta_frames"], -100);
    assert_eq!(preview["source_slip"]["applied_delta_frames"], -10);
    assert_eq!(preview["source_slip"]["minimum_delta_frames"], -10);
    assert_eq!(preview["source_slip"]["clamp"], "picture_start");
    assert_eq!(preview["edit"]["duration_delta"], 0);
    assert_eq!(counts(&package)?, original_counts);
    assert_eq!(writer.snapshot()?, before);
    // The live writer uses this same dispatch function, including its report.
    let request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: RevisionId::new("slipped")?,
        command: serde_json::from_value(envelope["command"].clone())?,
    };
    let (hosted, revision) = deadpan_cli::live_project::execute_short(
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
        (
            original_counts.0 + 1,
            original_counts.1 + 1,
            original_counts.2
        )
    );
    let after = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_eq!(after.duration()?, before.duration()?);
    assert_eq!(
        failure(&["command", path, "--json", file, "--dry-run"])?["error"]["code"],
        "RevisionConflict"
    );
    let mut noop = envelope;
    noop["expected_revision"] = json!(after.revision_id());
    noop["new_revision"] = "unused-noop".into();
    let noop_file = save(scratch.path(), &noop)?;
    let noop_file = noop_file.to_str().unwrap();
    let preview = success(&["command", path, "--json", noop_file, "--dry-run"])?;
    assert_eq!(preview["source_slip"]["applied_delta_frames"], 0);
    assert_eq!(preview["source_slip"]["clamp"], "picture_start");
    assert!(preview["edit"].is_null());
    assert_eq!(
        failure(&["command", path, "--json", noop_file])?["error"]["code"],
        "InvalidCommand"
    );
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
        after
    );
    success(&["project", "undo", path, "--expected", "slipped"])?;
    let undone = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_eq!(undone.nodes(), before.nodes());
    assert_ne!(undone.revision_id(), before.revision_id());
    Ok(())
}
