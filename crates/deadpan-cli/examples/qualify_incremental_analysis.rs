//! Real local-worker qualification of range order, cancellation and cache reuse.
//!
//! Arguments: SOURCE_VIDEO WHISPER_MODEL VAD_MODEL_OR_none WORKER SCRATCH_DIRECTORY
//! Source must contain more than 48 seconds of admitted audio. No downloads,
//! inference stand-ins or authored project edits are used.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use deadpan_cli::transcription::incremental::{detect_original_ranges, transcribe_original_ranges};
use deadpan_cli::transcription::{TranscriptionError, TranscriptionRuntime};
use deadpan_jobs::Sha256;
use deadpan_jobs::transcription::{Language, ModelInput};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::json;
use sha2::Digest;

fn hash(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut digest = sha2::Sha256::new();
    let mut buffer = [0_u8; 65_536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn model(path: &Path) -> Result<ModelInput, Box<dyn std::error::Error>> {
    let path = path.canonicalize()?;
    Ok(ModelInput {
        byte_length: std::fs::metadata(&path)?.len(),
        sha256: Sha256::new(hash(&path)?)?,
        path,
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [source, whisper, vad_path, worker, scratch] = arguments.as_slice() else {
        return Err(
            "usage: SOURCE_VIDEO WHISPER_MODEL VAD_MODEL_OR_none WORKER SCRATCH_DIRECTORY".into(),
        );
    };
    std::fs::create_dir_all(scratch)?;
    let scratch = scratch.canonicalize()?;
    let package = scratch.join("witness.deadpan");
    if package.exists() {
        return Err("witness package already exists; choose a fresh scratch directory".into());
    }
    let cancelled = AtomicBool::new(false);
    deadpan_cli::single_original::create(
        &package,
        &source.canonicalize()?,
        "Incremental analysis witness",
        &cancelled,
        |_, _| Ok(()),
    )?;
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let revision = store.head_revision()?;
    let model = model(whisper)?;
    let vad = if vad_path == Path::new("none") {
        None
    } else {
        Some(self::model(vad_path)?)
    };
    let runtime = TranscriptionRuntime {
        executable: worker.canonicalize()?,
        environment: Default::default(),
    };
    let cache = scratch.join("ranges");
    let cursor = AtomicU64::new(u64::MAX);
    let deadline = Instant::now() + Duration::from_secs(1_800);
    let mut cancelled_coverage = Vec::new();
    let first_started = Instant::now();
    let first = transcribe_original_ranges(
        &store,
        &runtime,
        &model,
        vad.as_ref(),
        Language::Code("en".into()),
        Some(&cache),
        &cursor,
        &cancelled,
        deadline,
        || Ok(()),
        |progress| {
            assert!(
                progress.total_samples > 2 * deadpan_analysis::incremental::AUDIO_RANGE_SAMPLES,
                "witness needs more than 48 seconds of source audio"
            );
            cancelled_coverage = progress.coverage;
            cancelled.store(true, Ordering::Release);
            Ok(())
        },
    );
    match first {
        Err(TranscriptionError::Cancelled) => {}
        Err(error) => {
            return Err(
                format!("first range did not reach the cancellation witness: {error}").into(),
            );
        }
        Ok(_) => return Err("first range unexpectedly completed after cancellation".into()),
    }
    assert_eq!(cancelled_coverage.len(), 1);
    assert!(cancelled_coverage[0].start >= 2 * deadpan_analysis::incremental::AUDIO_RANGE_SAMPLES);
    let cancellation_ms = first_started.elapsed().as_millis();
    cancelled.store(false, Ordering::Release);
    let mut last_coverage: Vec<std::ops::Range<u64>> = Vec::new();
    let mut completion_order = Vec::new();
    let mut worker_admissions = 0;
    let started = Instant::now();
    let completed = transcribe_original_ranges(
        &store,
        &runtime,
        &model,
        vad.as_ref(),
        Language::Code("en".into()),
        Some(&cache),
        &cursor,
        &cancelled,
        deadline,
        || {
            worker_admissions += 1;
            Ok(())
        },
        |progress| {
            let new: Vec<_> = progress
                .coverage
                .iter()
                .filter(|range| !last_coverage.contains(*range))
                .cloned()
                .collect();
            assert_eq!(new.len(), 1);
            completion_order.push(new[0].clone());
            last_coverage = progress.coverage;
            cursor.store(0, Ordering::Release);
            Ok(())
        },
    )?;
    assert_eq!(completion_order[0], cancelled_coverage[0]);
    assert_eq!(completion_order[1].start, 0);
    assert_eq!(completed.cached_ranges, 1);
    assert_eq!(worker_admissions, completion_order.len() - 1);
    let resumed_ms = started.elapsed().as_millis();
    let mut cached_admissions = 0;
    let started = Instant::now();
    let cached = transcribe_original_ranges(
        &store,
        &runtime,
        &model,
        vad.as_ref(),
        Language::Code("en".into()),
        Some(&cache),
        &cursor,
        &cancelled,
        deadline,
        || {
            cached_admissions += 1;
            Ok(())
        },
        |_| Ok(()),
    )?;
    assert_eq!(cached_admissions, 0);
    assert_eq!(cached.cached_ranges, completion_order.len());
    assert_eq!(cached.transcript, completed.transcript);
    assert_eq!(
        cached.activity.as_ref().map(|(_, activity)| activity),
        completed.activity.as_ref().map(|(_, activity)| activity)
    );
    let cached_ms = started.elapsed().as_millis();
    let standalone = if let Some(vad) = &vad {
        let separate = scratch.join("vad-ranges");
        let detection = detect_original_ranges(
            &store,
            &runtime,
            vad,
            Some(&separate),
            &cursor,
            &cancelled,
            deadline,
            |_, _| Ok(()),
        )?;
        let restored = detect_original_ranges(
            &store,
            &runtime,
            vad,
            Some(&separate),
            &cursor,
            &cancelled,
            deadline,
            |_, _| Ok(()),
        )?;
        assert_eq!(detection.1, restored.1);
        assert_eq!(
            Some(&detection.1),
            completed.activity.as_ref().map(|(_, activity)| activity)
        );
        json!({"checked": true, "samples": detection.1.audio().samples, "pauses": detection.1.pauses()})
    } else {
        json!({"checked": false, "reason": "no retained VAD model supplied"})
    };
    assert_eq!(store.head_revision()?, revision);
    let report = json!({
        "implementation": deadpan_cli::transcription::incremental::RANGE_IMPLEMENTATION,
        "source_sha256": hash(source)?, "worker_sha256": hash(worker)?,
        "executed_example_sha256": hash(&std::env::current_exe()?)?,
        "model_sha256": model.sha256.as_str(), "vad_sha256": vad.as_ref().map(|model| model.sha256.as_str()),
        "cancelled_after": cancelled_coverage, "completion_order": completion_order,
        "cancellation_elapsed_ms": cancellation_ms, "resumed_elapsed_ms": resumed_ms, "cached_elapsed_ms": cached_ms,
        "resumed_cached_ranges": completed.cached_ranges, "all_cached_ranges": cached.cached_ranges,
        "cache_failures": completed.cache_failures + cached.cache_failures,
        "worker_admissions": worker_admissions, "cached_worker_admissions": cached_admissions,
        "transcript": completed.transcript, "standalone_vad": standalone,
        "authored_revision_unchanged": revision,
        "initial_decode": "retained AudioSession opening still decodes the full source audio"
    });
    std::fs::write(
        scratch.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
