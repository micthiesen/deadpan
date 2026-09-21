#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use deadpan_core::{AssetId, ExactRatio, NodeId, NodeKind, SourceVideo};
use deadpan_store::{AccessMode, ProjectStore};
use rusqlite::Connection;
use serde_json::{Value, json};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn cli(arguments: &[&str]) -> Result<Output> {
    Ok(Command::new(env!("CARGO_BIN_EXE_deadpan-cli"))
        .args(arguments)
        .output()?)
}

fn success(arguments: &[&str]) -> Result<Value> {
    let output = cli(arguments)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn failure(arguments: &[&str]) -> Result<Value> {
    let output = cli(arguments)?;
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    Ok(serde_json::from_slice(&output.stderr)?)
}

fn create(package: &Path, fps: &str) -> Result<Value> {
    success(&[
        "project",
        "create",
        package.to_str().unwrap(),
        "--fps",
        fps,
        "--size",
        "320x180",
    ])
}

fn retain(package: &Path, source: &Path, bytes: &[u8], linked: bool) -> Result<Value> {
    fs::write(source, bytes)?;
    let mut arguments = vec![
        "project",
        "retain-original",
        package.to_str().unwrap(),
        source.to_str().unwrap(),
    ];
    if linked {
        arguments.push("--linked");
    }
    let value = success(&arguments)?;
    Ok(value["retained_original"]["record"]["object"]["content"].clone())
}

fn request(package: &Path, original: Value, streams: Value, next: &str) -> Result<Value> {
    let snapshot = ProjectStore::open(package, AccessMode::ReadOnly)?.snapshot()?;
    Ok(json!({
        "protocol": 1,
        "registration": {
            "expected_revision": snapshot.revision_id(), "new_revision": next,
            "original": original, "new_asset_id": "imported-asset", "label": "Qualified source",
            "insertion": {"parent": snapshot.root(), "index": 0, "node": "imported-source", "label": "Imported source"}
        },
        "streams": streams
    }))
}

fn save(directory: &Path, request: &Value) -> Result<PathBuf> {
    let path = directory.join("request.json");
    fs::write(&path, serde_json::to_vec(request)?)?;
    Ok(path)
}

fn registration(package: &Path, request: &Path, dry_run: bool, succeeds: bool) -> Result<Value> {
    let mut arguments = vec![
        "project",
        "register-source",
        package.to_str().unwrap(),
        "--request-json",
        request.to_str().unwrap(),
    ];
    if dry_run {
        arguments.push("--dry-run");
    }
    if succeeds {
        success(&arguments)
    } else {
        failure(&arguments)
    }
}

fn counts(package: &Path) -> Result<(u32, u32, u32)> {
    let connection = Connection::open(package.join("project.sqlite"))?;
    Ok(connection.query_row(
        "SELECT (SELECT count(*) FROM revisions), (SELECT count(*) FROM history), (SELECT count(*) FROM source_qualifications)",
        [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?)
}

#[test]
fn cfr_registration_previews_beside_writer_commits_once_and_preserves_undo_receipt() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("cfr.deadpan");
    create(&package, "30000/1001")?;
    let original = retain(
        &package,
        &scratch.path().join("source.mp4"),
        include_bytes!("../../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4"),
        false,
    )?;
    let input = request(
        &package,
        original,
        json!({"type":"video_and_audio","audio_stream":1}),
        "registered",
    )?;
    let path = save(scratch.path(), &input)?;
    let writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let before = writer.snapshot()?;
    let preview = registration(&package, &path, true, true)?;
    assert_eq!(preview["protocol"], 1);
    assert_eq!(preview["committed"], false);
    assert_eq!(preview["preview"]["asset_id"], "imported-asset");
    assert!(preview["preview"]["edit"].is_object());
    assert!(preview.to_string().len() < 8192);
    assert_eq!(writer.snapshot()?, before);
    assert_eq!(counts(&package)?, (1, 0, 0));
    assert_eq!(
        registration(&package, &path, false, false)?["error"]["code"],
        "ProjectAlreadyOpen"
    );
    drop(writer);
    fs::remove_file(scratch.path().join("source.mp4"))?;
    let outcome = registration(&package, &path, false, true)?;
    assert_eq!(outcome["committed"], true);
    assert_eq!(
        outcome["outcome"]["qualification"],
        preview["preview"]["qualification"]
    );
    assert_eq!(counts(&package)?, (2, 1, 1));
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let imported = store.snapshot()?;
    let asset = &imported.assets()[&AssetId::new("imported-asset")?];
    assert!(asset.video.is_some());
    assert!(asset.audio.is_some());
    let receipt = store.source_qualification(asset.source_qualification.as_ref().unwrap())?;
    assert_eq!(
        receipt
            .snapshot()
            .video()
            .unwrap()
            .index()
            .index()
            .frames()
            .len(),
        120
    );
    assert_eq!(receipt.snapshot().audio().unwrap().stream().stream_index, 1);
    assert_eq!(imported.presentation_basis(), before.presentation_basis());
    assert_eq!(imported.duration()?.frames(), 120);
    drop(store);

    let mut duplicate = input.clone();
    duplicate["registration"]["expected_revision"] = json!(imported.revision_id());
    duplicate["registration"]["new_revision"] = "unused-deduplicated-revision".into();
    duplicate["registration"]["new_asset_id"] = "unused-new-alias".into();
    duplicate["registration"]["insertion"] = Value::Null;
    let path = save(scratch.path(), &duplicate)?;
    let duplicate = registration(&package, &path, false, true)?;
    assert_eq!(duplicate["committed"], false);
    assert_eq!(duplicate["outcome"]["asset_id"], "imported-asset");
    assert!(duplicate["outcome"]["commit"].is_null());
    assert_eq!(counts(&package)?, (2, 1, 1));
    success(&[
        "project",
        "undo",
        package.to_str().unwrap(),
        "--expected",
        "registered",
    ])?;
    let undone = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_eq!(undone.assets(), before.assets());
    assert_eq!(undone.nodes(), before.nodes());
    assert_eq!(counts(&package)?, (3, 1, 1));
    success(&[
        "project",
        "redo",
        package.to_str().unwrap(),
        "--expected",
        undone.revision_id().as_str(),
    ])?;
    let reopened = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    reopened.validate()?;
    assert_eq!(reopened.snapshot()?.assets(), imported.assets());
    assert_eq!(reopened.snapshot()?.nodes(), imported.nodes());
    assert_eq!(counts(&package)?, (4, 1, 1));
    Ok(())
}

#[test]
fn offset_av_and_audio_only_register_exact_measured_placements() -> Result {
    for (name, bytes, streams, fps, expected_duration) in [
        (
            "offset",
            include_bytes!("../../../native/deadpan-source/tests/fixtures/offset-bframes.mp4")
                .as_slice(),
            json!({"type":"video_and_audio","audio_stream":1}),
            "30000/1001",
            121,
        ),
        (
            "audio",
            include_bytes!(
                "../../../native/deadpan-source/tests/audio-fixtures/pcm-mono-44100.wav"
            )
            .as_slice(),
            json!({"type":"audio_only","stream":0}),
            "30/1",
            31,
        ),
    ] {
        let scratch = tempfile::tempdir()?;
        let package = scratch.path().join(format!("{name}.deadpan"));
        create(&package, fps)?;
        let original = retain(&package, &scratch.path().join("source.media"), bytes, false)?;
        let path = save(
            scratch.path(),
            &request(&package, original, streams, "registered")?,
        )?;
        registration(&package, &path, false, true)?;
        let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
        let document = store.snapshot()?;
        assert_eq!(document.duration()?.frames(), expected_duration);
        let NodeKind::Source { source } = &document.nodes()[&NodeId::new("imported-source")?].kind
        else {
            panic!()
        };
        let asset = &document.assets()[&AssetId::new("imported-asset")?];
        assert_eq!(source.audio_offset.0, 0);
        assert_eq!(source.audio_mapping.start_frames(), ExactRatio::ZERO);
        assert_eq!(source.audio.as_ref().unwrap().span, asset.audio.unwrap());
        if name == "offset" {
            assert_eq!(
                source.video_mapping.start_frames(),
                ExactRatio::new(640, 1001)?
            );
            assert_eq!(
                source.audio_mapping.duration_frames(source.duration)?,
                ExactRatio::new(120760, 1001)?
            );
            assert_eq!(asset.audio.unwrap().start().ticks, 95072);
            assert_eq!(asset.audio.unwrap().end().ticks, 288288);
            assert_eq!(asset.video.unwrap().start().ticks, 60060);
            assert_eq!(asset.video.unwrap().end().ticks, 180180);
        } else {
            assert!(matches!(source.video, SourceVideo::Blank));
            assert!(asset.video.is_none());
            assert_eq!(asset.audio.unwrap().end().ticks, 44117);
            assert_eq!(
                source.audio_mapping.duration_frames(source.duration)?,
                ExactRatio::new(44117, 1470)?
            );
            let receipt =
                store.source_qualification(asset.source_qualification.as_ref().unwrap())?;
            assert!(receipt.snapshot().video().is_none());
            assert_eq!(
                receipt.snapshot().audio().unwrap().stream().sample_rate,
                44100
            );
        }
        store.validate()?;
    }
    Ok(())
}

#[test]
fn protocol_is_required_and_rejected_before_opening_the_project() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("protocol.deadpan");
    create(&package, "30000/1001")?;
    // Holding the writer proves malformed protocols never open a writable
    // store; the absent original also proves dry runs never reach decoding.
    let writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let valid = request(
        &package,
        json!({"algorithm":"blake3","digest":"a".repeat(64)}),
        json!({"type":"video_only"}),
        "never-committed",
    )?;
    for (protocol, expected) in [
        (Some(json!(999)), "ProtocolUnsupported"),
        (Some(json!(0)), "ProtocolUnsupported"),
        (Some(Value::Null), "InvalidInput"),
        (Some(json!("1")), "InvalidInput"),
        (None, "InvalidInput"),
    ] {
        let mut input = valid.clone();
        match protocol {
            Some(protocol) => input["protocol"] = protocol,
            None => {
                input.as_object_mut().unwrap().remove("protocol");
            }
        }
        let path = save(scratch.path(), &input)?;
        for dry_run in [false, true] {
            assert_eq!(
                registration(&package, &path, dry_run, false)?["error"]["code"],
                expected
            );
            assert_eq!(counts(&package)?, (1, 0, 0));
        }
    }
    assert!(writer.snapshot()?.assets().is_empty());
    Ok(())
}

#[test]
fn stream_selection_is_required_and_failed_audio_never_becomes_video_only() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("streams.deadpan");
    create(&package, "30000/1001")?;
    let original = retain(
        &package,
        &scratch.path().join("source.mp4"),
        include_bytes!("../../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4"),
        false,
    )?;
    let mut input = request(
        &package,
        original,
        json!({"type":"video_and_audio","audio_stream":0}),
        "registered",
    )?;
    let path = save(scratch.path(), &input)?;
    assert_eq!(
        registration(&package, &path, false, false)?["error"]["code"],
        "SourceAudioDecodeFailed"
    );
    assert_eq!(counts(&package)?, (1, 0, 0));
    for invalid in [
        Value::Null,
        json!({"type":"video_and_audio"}),
        json!({"type":"video_only","audio_stream":1}),
    ] {
        input["streams"] = invalid;
        let path = save(scratch.path(), &input)?;
        assert_eq!(
            registration(&package, &path, false, false)?["error"]["code"],
            "InvalidInput"
        );
        assert_eq!(counts(&package)?, (1, 0, 0));
    }
    input.as_object_mut().unwrap().remove("streams");
    let path = save(scratch.path(), &input)?;
    assert_eq!(
        registration(&package, &path, false, false)?["error"]["code"],
        "InvalidInput"
    );
    input["streams"] = json!({"type":"video_only"});
    let path = save(scratch.path(), &input)?;
    registration(&package, &path, false, true)?;
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let document = store.snapshot()?;
    let asset = &document.assets()[&AssetId::new("imported-asset")?];
    assert!(asset.video.is_some());
    assert!(asset.audio.is_none());
    let receipt = store.source_qualification(asset.source_qualification.as_ref().unwrap())?;
    assert_eq!(
        receipt
            .snapshot()
            .video()
            .unwrap()
            .interpretation()
            .audio_streams
            .len(),
        1
    );
    assert!(receipt.snapshot().audio().is_none());
    Ok(())
}

#[test]
fn stale_revision_is_checked_before_unavailable_original_bytes() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("stale.deadpan");
    create(&package, "30000/1001")?;
    let source = scratch.path().join("source.mp4");
    let original = retain(
        &package,
        &source,
        include_bytes!("../../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4"),
        true,
    )?;
    let mut input = request(
        &package,
        original,
        json!({"type":"video_and_audio","audio_stream":1}),
        "registered",
    )?;
    let current = input["registration"]["expected_revision"].clone();
    input["registration"]["expected_revision"] = "stale-revision".into();
    fs::remove_file(source)?;
    let path = save(scratch.path(), &input)?;
    for dry_run in [true, false] {
        let error = registration(&package, &path, dry_run, false)?;
        assert_eq!(error["error"]["code"], "RevisionConflict");
        assert_eq!(error["error"]["current_revision"], current);
    }
    input["registration"]["expected_revision"] = current;
    let path = save(scratch.path(), &input)?;
    assert_eq!(
        registration(&package, &path, true, false)?["error"]["code"],
        "OriginalOffline"
    );
    assert_eq!(counts(&package)?, (1, 0, 0));
    Ok(())
}

#[test]
fn pending_generation_allows_preview_but_requires_real_relevance_on_commit() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("generation.deadpan");
    fs::create_dir(&package)?;
    fs::create_dir(package.join("Snapshots"))?;
    fs::create_dir_all(package.join("Media/Originals"))?;
    fs::create_dir_all(package.join("Media/Generated"))?;
    let database = Connection::open(package.join("project.sqlite"))?;
    database.pragma_update(None, "foreign_keys", false)?;
    database.execute_batch(include_str!(
        "../../deadpan-store/tests/fixtures/v13-history.sql"
    ))?;
    drop(database);
    ProjectStore::migrate(&package)?;
    let original = retain(
        &package,
        &scratch.path().join("source.mp4"),
        include_bytes!("../../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4"),
        false,
    )?;
    let input = request(
        &package,
        original,
        json!({"type":"video_and_audio","audio_stream":1}),
        "registered",
    )?;
    let path = save(scratch.path(), &input)?;
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let before_counts = counts(&package)?;
    assert!(registration(&package, &path, true, true)?["preview"]["edit"].is_object());
    assert_eq!(
        registration(&package, &path, false, false)?["error"]["code"],
        "GenerationRelevanceRequired"
    );
    assert_eq!(counts(&package)?, before_counts);
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?,
        before
    );
    Ok(())
}
