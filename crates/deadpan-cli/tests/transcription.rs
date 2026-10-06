#![cfg(any(target_os = "macos", target_os = "linux"))]
//! Analysis PCM from a registered source and the transcript commands. The
//! recognizer itself is exercised by the worker's tests and qualification.

use std::{
    error::Error,
    fs,
    path::Path,
    process::{Command, Output},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use deadpan_cli::transcription::prepare_original_audio;
use deadpan_core::AssetId;
use deadpan_store::{AccessMode, ProjectStore};
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

fn registered(scratch: &Path) -> Result<std::path::PathBuf> {
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

#[test]
fn registered_audio_becomes_exact_length_mono_analysis_pcm() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = registered(scratch.path())?;
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let analysis = prepare_original_audio(
        &store,
        Some(&AssetId::new("speech")?),
        &AtomicBool::new(false),
        Instant::now() + Duration::from_secs(120),
    )?;
    let receipt = store.registered_source(&store.head_revision()?, &AssetId::new("speech")?)?;
    let index = receipt.snapshot().audio().unwrap();
    let rate = index.stream().sample_rate;
    let first = index.frames().first().unwrap().valid_start;
    let last = index.frames().last().unwrap().valid_end;
    // Analysis sample k is source sample origin + k * rate / 16000; the last
    // analysis sample stays inside the measured source coverage.
    assert_eq!(analysis.input.origin, first);
    assert_eq!(analysis.input.source_rate, rate);
    assert_eq!(
        analysis.input.samples.len() as i64,
        (last - first) * 16_000 / i64::from(rate)
    );
    assert!(
        analysis
            .input
            .samples
            .iter()
            .all(|sample| sample.is_finite())
    );
    let energy: f64 = analysis
        .input
        .samples
        .iter()
        .map(|sample| f64::from(*sample).powi(2))
        .sum::<f64>()
        / analysis.input.samples.len() as f64;
    // The fixture's channels are nearly opposite in polarity, so their mono
    // average is very quiet. FFmpeg's independent `pan=mono|c0=0.5*c0+0.5*c1,
    // aresample=16000,volumedetect` reports -79.2 dB (-55.7/-56.9 dB per
    // channel); the canonical preparation must agree.
    let decibels = 10.0 * energy.log10();
    assert!(
        (-80.2..=-78.2).contains(&decibels),
        "mono analysis level {decibels} dB"
    );
    assert_eq!(analysis.content, receipt.original().content().to_string());
    assert_eq!(analysis.audio_stream, index.stream().stream_index);

    // A generic project has no Original to default to.
    let error = prepare_original_audio(
        &store,
        None,
        &AtomicBool::new(false),
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap_err();
    assert!(error.to_string().contains("no ready Original"), "{error}");
    let cancelled = AtomicBool::new(true);
    assert!(
        prepare_original_audio(
            &store,
            Some(&AssetId::new("speech")?),
            &cancelled,
            Instant::now() + Duration::from_secs(5),
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn transcript_commands_report_absence_usage_and_unavailable_sources() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = registered(scratch.path())?;
    let path = package.to_str().unwrap();
    let empty = success(&["transcript", path, "--asset", "speech"])?;
    assert_eq!(
        empty,
        json!({
            "protocol": 1,
            "transcripts": [],
            "correction_rule": "deadpan-corrections-1",
            "corrections_version": null,
            "corrections_error": null,
        })
    );
    let searched = success(&["transcript", path, "--asset", "speech", "--search", "hello"])?;
    assert_eq!(searched["transcripts"], json!([]));
    for arguments in [
        vec!["transcript"],
        vec!["transcript", path, "--search"],
        vec!["transcribe", path],
        vec!["transcribe", path, "--model", "/nonexistent.bin"],
        vec!["transcribe", path, "--sha256", "abc"],
    ] {
        let output = cli(&arguments)?;
        assert!(!output.status.success(), "{arguments:?}");
        let error: Value = serde_json::from_slice(&output.stderr)?;
        assert!(error["error"]["code"].is_string(), "{error}");
    }
    let missing = cli(&["transcript", path])?;
    let error: Value = serde_json::from_slice(&missing.stderr)?;
    assert_eq!(error["error"]["code"], "TranscriptionUnavailable");
    Ok(())
}
