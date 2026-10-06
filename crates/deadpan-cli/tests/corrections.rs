#![cfg(any(target_os = "macos", target_os = "linux"))]
//! Headless transcript corrections: the `:correct` sheet's store path with
//! version checks, target checks, dry runs and correction Undo.

use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use deadpan_analysis::{AnalysedAudio, Transcript, Word};
use deadpan_store::{AccessMode, ProjectStore, TranscriptKey};
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
    assert!(
        !output.status.success(),
        "unexpected success: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    Ok(serde_json::from_slice(&output.stderr)?)
}

fn registered(scratch: &Path) -> Result<PathBuf> {
    let package = scratch.join("speech.deadpan");
    success(&[
        "project",
        "create",
        package.to_str().unwrap(),
        "--fps",
        "30000/1001",
        "--size",
        "320x180",
    ])?;
    let source = scratch.join("source.mp4");
    fs::write(
        &source,
        include_bytes!("../../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4"),
    )?;
    let retained = success(&[
        "project",
        "retain-original",
        package.to_str().unwrap(),
        source.to_str().unwrap(),
    ])?;
    let original = retained["retained_original"]["record"]["object"]["content"].clone();
    let snapshot = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let request = json!({
        "protocol": 1,
        "registration": {
            "expected_revision": snapshot.revision_id(), "new_revision": "registered",
            "original": original, "new_asset_id": "speech", "label": "Speech",
            "insertion": {"parent": snapshot.root(), "index": 0, "node": "speech-source", "label": "Speech"}
        },
        "streams": {"type":"video_and_audio","audio_stream":1}
    });
    let path = scratch.join("request.json");
    fs::write(&path, serde_json::to_vec(&request)?)?;
    success(&[
        "project",
        "register-source",
        package.to_str().unwrap(),
        "--request-json",
        path.to_str().unwrap(),
    ])?;
    Ok(package)
}

/// Every corrections row, compared to prove dry runs and refusals write nothing.
fn rows(package: &Path) -> Result<Vec<String>> {
    let database = rusqlite::Connection::open(package.join("project.sqlite"))?;
    let mut rows = Vec::new();
    for query in [
        "SELECT json_array(content,audio_stream,version,value) FROM analysis_corrections",
        "SELECT json_array(content,audio_stream,stack,label,value) FROM analysis_correction_steps ORDER BY rowid",
    ] {
        rows.extend(
            database
                .prepare(query)?
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?,
        );
    }
    Ok(rows)
}

fn change(scratch: &Path, package: &Path, request: &Value, dry_run: bool) -> Result<Output> {
    let path = scratch.join("change.json");
    fs::write(&path, request.to_string())?;
    let mut arguments = vec![
        "corrections",
        package.to_str().unwrap(),
        "--json",
        path.to_str().unwrap(),
    ];
    if dry_run {
        arguments.push("--dry-run");
    }
    cli(&arguments)
}

fn json_of(output: &Output) -> Result<Value> {
    Ok(serde_json::from_slice(if output.status.success() {
        &output.stdout
    } else {
        &output.stderr
    })?)
}

#[test]
fn corrections_preview_apply_refuse_stale_requests_and_undo() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = registered(scratch.path())?;
    let path = package.to_str().unwrap();

    let empty = success(&["corrections", path, "--asset", "speech"])?;
    assert_eq!(empty["version"], 0);
    assert!(empty["words"].is_null());
    let content = empty["key"]["content"].as_str().unwrap().to_owned();
    let audio_stream = u32::try_from(empty["key"]["audio_stream"].as_u64().unwrap())?;
    // Nothing to correct yet: refused with a structured reason.
    let nothing = change(
        scratch.path(),
        &package,
        &json!({"protocol":1,"expected_version":0,"asset":"speech",
            "change":{"type":"edit_word","word":0,"expected_text":"helo","text":"hello"}}),
        false,
    )?;
    assert_eq!(json_of(&nothing)?["error"]["code"], "AnalysisUnavailable");

    let words = [("helo", 10, 40), ("world", 50, 90)];
    let transcript = Transcript::new(
        AnalysedAudio {
            origin: 0,
            sample_rate: 48_000,
            duration_cs: 200,
        },
        words
            .iter()
            .map(|(text, start, end)| Word {
                text: (*text).into(),
                start_cs: *start,
                end_cs: *end,
                probability: 0.9,
                segment: 0,
            })
            .collect(),
    )?;
    ProjectStore::open(&package, AccessMode::ReadWrite)?.save_transcript(
        &TranscriptKey {
            content: content.clone(),
            audio_stream,
            model_sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002".into(),
            language: "en".into(),
            engine: "whisper.cpp 1.8.3".into(),
        },
        &transcript,
    )?;
    let head = ProjectStore::open(&package, AccessMode::ReadOnly)?.head_revision()?;

    let seen = success(&["corrections", path, "--asset", "speech"])?;
    assert_eq!(seen["words"][0]["text"], "helo");
    assert_eq!(seen["words"][0]["corrected"], false);
    let edit = json!({"protocol":1,"expected_version":0,"asset":"speech",
        "change":{"type":"edit_word","word":0,"expected_text":"helo","text":"hello"}});

    let before = rows(&package)?;
    let preview = json_of(&change(scratch.path(), &package, &edit, true)?)?;
    assert_eq!(preview["committed"], false);
    assert_eq!(preview["label"], "“helo” → “hello”");
    assert!(preview["proposed"]["words"].is_array());
    assert_eq!(rows(&package)?, before);

    // The caller saw different text: refused before any write.
    let mut moved = edit.clone();
    moved["change"]["expected_text"] = json!("hullo");
    let refused = json_of(&change(scratch.path(), &package, &moved, false)?)?;
    assert_eq!(refused["error"]["code"], "CorrectionTargetChanged");
    assert_eq!(rows(&package)?, before);

    let applied = json_of(&change(scratch.path(), &package, &edit, false)?)?;
    assert_eq!(applied["committed"], true, "{applied}");
    assert_eq!(applied["version"], 1);
    assert_eq!(applied["undo"], "“helo” → “hello”");
    let corrected = success(&["corrections", path, "--asset", "speech"])?;
    assert_eq!(corrected["words"][0]["text"], "hello");
    assert_eq!(corrected["words"][0]["corrected"], true);
    // Corrections never create document revisions.
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.head_revision()?,
        head
    );

    // Repeating the request at its old version is a conflict, both as a dry
    // run and as a commit, and writes nothing.
    let after = rows(&package)?;
    for dry_run in [true, false] {
        let stale = json_of(&change(scratch.path(), &package, &edit, dry_run)?)?;
        assert_eq!(stale["error"]["code"], "AnalysisCorrectionsConflict");
    }
    assert_eq!(rows(&package)?, after);

    let undo = json!({"protocol":1,"expected_version":1,"asset":"speech","change":{"type":"undo"}});
    let preview = json_of(&change(scratch.path(), &package, &undo, true)?)?;
    assert_eq!(preview["label"], "undid “helo” → “hello”");
    assert_eq!(rows(&package)?, after);
    let undone = json_of(&change(scratch.path(), &package, &undo, false)?)?;
    assert_eq!(undone["version"], 2);
    assert_eq!(undone["redo"], "“helo” → “hello”");
    let restored = success(&["corrections", path, "--asset", "speech"])?;
    assert_eq!(restored["words"][0]["text"], "helo");

    let join = json!({"protocol":1,"expected_version":2,"asset":"speech",
        "change":{"type":"join_words","word":0,"expected_text":"helo"}});
    let joined = json_of(&change(scratch.path(), &package, &join, false)?)?;
    assert_eq!(joined["label"], "join “helo” and “world”");
    let merged = success(&["corrections", path, "--asset", "speech"])?;
    assert_eq!(merged["words"].as_array().unwrap().len(), 1);
    // A new change clears Redo, as in the sheet.
    assert!(merged["redo"].is_null());

    for arguments in [
        vec!["corrections"],
        vec!["corrections", path, "--json"],
        vec!["corrections", path, "--asset"],
    ] {
        let error = failure(&arguments)?;
        assert!(error["error"]["code"].is_string(), "{arguments:?}");
    }
    Ok(())
}

#[test]
fn pause_corrections_check_the_seen_bounds_and_dry_runs_write_nothing() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = registered(scratch.path())?;
    let path = package.to_str().unwrap();
    let empty = success(&["corrections", path, "--asset", "speech"])?;
    // One second of analysis audio: speech, then a quiet half second.
    let activity = deadpan_analysis::SpeechActivity::new(
        deadpan_analysis::ActivityAudio {
            origin: 0,
            sample_rate: 48_000,
            samples: 16_000,
        },
        (0..32).map(|hop| if hop < 16 { 230 } else { 5 }).collect(),
        (0..100)
            .map(|frame| if frame < 50 { 180 } else { 20 })
            .collect(),
    )?;
    ProjectStore::open(&package, AccessMode::ReadWrite)?.save_speech_activity(
        &deadpan_store::SpeechActivityKey {
            content: empty["key"]["content"].as_str().unwrap().to_owned(),
            audio_stream: u32::try_from(empty["key"]["audio_stream"].as_u64().unwrap())?,
            model_sha256: "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987".into(),
            engine: "whisper.cpp 1.8.3".into(),
        },
        &activity,
    )?;
    let seen = success(&["corrections", path, "--asset", "speech"])?;
    let pause = seen["pauses"][0].clone();
    assert!(pause["start"].is_u64(), "{seen}");
    let remove = json!({"protocol":1,"expected_version":0,"asset":"speech",
        "change":{"type":"remove_pause","pause":0,
            "expected_start":pause["start"],"expected_end":pause["end"]}});

    let before = rows(&package)?;
    let preview = json_of(&change(scratch.path(), &package, &remove, true)?)?;
    assert_eq!(preview["committed"], false, "{preview}");
    assert_eq!(rows(&package)?, before);
    let mut moved = remove.clone();
    moved["change"]["expected_end"] = json!(pause["end"].as_u64().unwrap() + 1);
    let refused = json_of(&change(scratch.path(), &package, &moved, false)?)?;
    assert_eq!(refused["error"]["code"], "CorrectionTargetChanged");
    assert_eq!(rows(&package)?, before);

    let removed = json_of(&change(scratch.path(), &package, &remove, false)?)?;
    assert_eq!(removed["version"], 1, "{removed}");
    let after = success(&["corrections", path, "--asset", "speech"])?;
    assert_eq!(
        after["pauses"].as_array().unwrap().len() + 1,
        seen["pauses"].as_array().unwrap().len()
    );
    Ok(())
}
