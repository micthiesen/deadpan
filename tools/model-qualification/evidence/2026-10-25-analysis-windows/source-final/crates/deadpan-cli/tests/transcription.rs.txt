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

use deadpan_cli::transcription::{
    MAX_ANALYSIS_WINDOW_FRAMES, TranscriptionError, open_original_analysis, prepare_original_audio,
    prepare_original_audio_window,
};
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
    registered_media(
        scratch,
        include_bytes!("../../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4"),
        json!({"type":"video_and_audio","audio_stream":1}),
    )
}

fn registered_media(scratch: &Path, bytes: &[u8], streams: Value) -> Result<std::path::PathBuf> {
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
    fs::write(&source, bytes)?;
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
        "streams": streams
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

// Add signed composition offsets to the admitted AAC fixture, preserving its
// encoded packets and duration tables. Adjust its edit's media_time so FFmpeg
// still discards the same 1024 priming samples: 543 + 481 = 1024. Actual source
// registration measures the resulting -481 origin on the 48000 Hz clock.
fn negative_aac_container() -> Vec<u8> {
    fn number(bytes: &[u8]) -> u32 {
        u32::from_be_bytes(bytes[..4].try_into().unwrap())
    }
    fn boxes(mut bytes: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
        std::iter::from_fn(move || {
            if bytes.is_empty() {
                return None;
            }
            let size = usize::try_from(number(bytes)).unwrap();
            assert!((8..=bytes.len()).contains(&size));
            let (atom, rest) = bytes.split_at(size);
            bytes = rest;
            Some((&atom[4..8], &atom[8..]))
        })
    }
    fn child<'a>(bytes: &'a [u8], kind: &[u8]) -> &'a [u8] {
        boxes(bytes).find(|(tag, _)| *tag == kind).unwrap().1
    }
    fn rewrite(bytes: &[u8], audio: bool) -> Vec<u8> {
        let mut result = Vec::new();
        for (kind, payload) in boxes(bytes) {
            let audio = if kind == b"trak" {
                &child(child(payload, b"mdia"), b"hdlr")[8..12] == b"soun"
            } else {
                audio
            };
            let mut body = if [b"moov", b"trak", b"mdia", b"minf", b"stbl", b"edts"]
                .iter()
                .any(|tag| kind == *tag)
            {
                rewrite(payload, audio)
            } else {
                payload.to_vec()
            };
            if kind == b"elst" && audio {
                assert_eq!(number(&body), 0); // Version 0.
                assert_eq!(number(&body[4..]), 1); // One edit.
                assert_eq!(number(&body[12..]), 1024);
                body[12..16].copy_from_slice(&543_u32.to_be_bytes());
            }
            if kind == b"stco" {
                let count = usize::try_from(number(&body[4..])).unwrap();
                assert_eq!(body.len(), 8 + 4 * count);
                for offset in body[8..].chunks_exact_mut(4) {
                    offset.copy_from_slice(&number(offset).checked_add(24).unwrap().to_be_bytes());
                }
            }
            if kind == b"stbl" && audio {
                assert!(!boxes(payload).any(|(tag, _)| tag == b"ctts"));
                assert_eq!(number(&child(payload, b"stsz")[8..]), 189);
                body.extend_from_slice(&24_u32.to_be_bytes());
                body.extend_from_slice(b"ctts");
                for value in [
                    0x0100_0000,
                    1,
                    189,
                    u32::from_be_bytes((-481_i32).to_be_bytes()),
                ] {
                    body.extend_from_slice(&value.to_be_bytes());
                }
            }
            result.extend_from_slice(&u32::try_from(body.len() + 8).unwrap().to_be_bytes());
            result.extend_from_slice(kind);
            result.extend_from_slice(&body);
        }
        result
    }
    let original = include_bytes!("../../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4");
    // The growing moov precedes every media chunk, so both tracks' stco entries
    // move by the added 24-byte ctts box. Nothing inside mdat changes.
    let kinds: Vec<_> = boxes(original).map(|(kind, _)| kind).collect();
    assert_eq!(kinds, [b"ftyp", b"moov", b"free", b"mdat"]);
    rewrite(original, false)
}

#[test]
fn selected_analysis_windows_match_full_pcm_bits_on_the_global_phase_grid() -> Result {
    let negative = negative_aac_container();
    for (bytes, streams, rate, origin) in [
        (
            include_bytes!(
                "../../../native/deadpan-source/tests/audio-fixtures/pcm-mono-44100.wav"
            )
            .as_slice(),
            json!({"type":"audio_only","stream":0,"interpretation":"mono"}),
            44100,
            0,
        ),
        (
            include_bytes!(
                "../../../native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav"
            )
            .as_slice(),
            json!({"type":"audio_only","stream":0,"interpretation":"stereo_left_right"}),
            48000,
            0,
        ),
        (
            include_bytes!("../../../native/deadpan-source/tests/fixtures/offset-bframes.mp4")
                .as_slice(),
            json!({"type":"video_and_audio","audio_stream":1}),
            48000,
            95072,
        ),
        (
            negative.as_slice(),
            json!({"type":"video_and_audio","audio_stream":1}),
            48000,
            -481,
        ),
    ] {
        let scratch = tempfile::tempdir()?;
        let package = registered_media(scratch.path(), bytes, streams)?;
        let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
        let revision = store.head_revision()?;
        let before = serde_json::to_vec(&store.snapshot()?)?;
        let asset = AssetId::new("speech")?;
        let receipt = store.registered_source(&revision, &asset)?;
        let index = receipt.snapshot().audio().unwrap();
        assert_eq!(index.stream().sample_rate, rate);
        assert_eq!(index.frames().first().unwrap().valid_start, origin);
        let cancel = AtomicBool::new(false);
        let deadline = Instant::now() + Duration::from_secs(120);
        let full = prepare_original_audio(&store, Some(&asset), &cancel, deadline)?;
        assert_eq!(full.input.source_rate, rate);
        assert_eq!(full.input.origin, origin);
        let total = u64::try_from(full.input.samples.len())?;
        let session = open_original_analysis(&store, Some(&asset), &cancel, deadline)?;
        assert_eq!(session.complete_analysis_frames(), total);
        // Non-integral 44.1k source phases, a resampler block seam, interior
        // context and the terminal halo must all equal a complete-analysis crop.
        for range in [0..17, 1..257, 157..total - 1, total - 73..total] {
            let window = session.prepare_window(range.clone(), &cancel, deadline)?;
            assert_eq!(window.analysis_origin, origin);
            assert_eq!(window.analysis_offset, range.start);
            assert_eq!(window.complete_analysis_frames, total);
            assert_eq!(window.source_rate, rate);
            assert_eq!(window.content, full.content);
            assert_eq!(window.audio_stream, full.audio_stream);
            assert_eq!(window.source_range.start, origin);
            assert_eq!(window.samples.len() as u64, range.end - range.start);
            assert_eq!(
                window
                    .samples
                    .iter()
                    .map(|s| s.to_bits())
                    .collect::<Vec<_>>(),
                full.input.samples[range.start as usize..range.end as usize]
                    .iter()
                    .map(|s| s.to_bits())
                    .collect::<Vec<_>>(),
                "rate {rate}, origin {origin}, range {range:?}",
            );
        }
        let once = prepare_original_audio_window(&store, Some(&asset), 3..71, &cancel, deadline)?;
        assert_eq!(once.analysis_offset, 3);
        assert_eq!(once.samples, full.input.samples[3..71]);
        assert_eq!(store.head_revision()?, revision);
        assert_eq!(serde_json::to_vec(&store.snapshot()?)?, before);
        store.validate()?;
    }
    Ok(())
}

#[test]
fn local_window_preflight_refuses_before_source_reads_and_retained_windows_reuse_the_cache()
-> Result {
    let scratch = tempfile::tempdir()?;
    let package = registered(scratch.path())?;
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let asset = AssetId::new("speech")?;
    let cancel = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(120);
    let revision = store.head_revision()?;
    let session = open_original_analysis(&store, Some(&asset), &cancel, deadline)?;
    let before = session.prepare_window(1..73, &cancel, deadline)?;
    let originals = package.join("Media/Originals");
    let unavailable = scratch.path().join("originals-unavailable");
    fs::rename(&originals, &unavailable)?;
    for range in [
        0..0,
        std::ops::Range { start: 9, end: 8 },
        0..MAX_ANALYSIS_WINDOW_FRAMES + 1,
        session.complete_analysis_frames()..session.complete_analysis_frames() + 1,
    ] {
        let error = prepare_original_audio_window(&store, Some(&asset), range, &cancel, deadline)
            .unwrap_err();
        assert!(
            matches!(error, TranscriptionError::Configuration(_)),
            "{error}"
        );
    }
    // Static bad requests must not depend on a source/Original selection at all.
    assert!(matches!(
        prepare_original_audio_window(&store, None, 0..0, &cancel, deadline),
        Err(TranscriptionError::Configuration(
            "analysis window is empty or reversed"
        ))
    ));
    assert!(prepare_original_audio_window(&store, Some(&asset), 1..73, &cancel, deadline).is_err());
    let after = session.prepare_window(1..73, &cancel, deadline)?;
    assert_eq!(before.samples, after.samples);
    assert!(matches!(
        session.prepare_window(1..73, &AtomicBool::new(true), deadline),
        Err(TranscriptionError::Cancelled)
    ));
    assert!(matches!(
        prepare_original_audio_window(
            &store,
            Some(&asset),
            1..73,
            &AtomicBool::new(true),
            deadline
        ),
        Err(TranscriptionError::Cancelled)
    ));
    assert!(matches!(
        session.prepare_window(1..73, &cancel, Instant::now()),
        Err(TranscriptionError::Deadline)
    ));
    assert!(matches!(
        prepare_original_audio_window(&store, Some(&asset), 1..73, &cancel, Instant::now()),
        Err(TranscriptionError::Deadline)
    ));
    fs::rename(&unavailable, &originals)?;
    assert_eq!(store.head_revision()?, revision);
    store.validate()?;
    Ok(())
}
