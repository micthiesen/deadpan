#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_cli::refinement::{RefinementError, RefinementRuntime, infer};
use deadpan_jobs::Sha256;
use deadpan_jobs::refinement::{
    CHECKPOINT_BYTES, CHECKPOINT_SHA256, STATISTICS_BYTES, STATISTICS_SHA256,
};
use deadpan_jobs::transcription::ModelInput;

fn input(path: PathBuf, bytes: u64, hash: &str) -> ModelInput {
    ModelInput {
        path,
        byte_length: bytes,
        sha256: Sha256::new(hash).unwrap(),
    }
}

#[test]
fn invalid_cancelled_and_expired_contexts_never_launch_a_worker() {
    let runtime = RefinementRuntime {
        executable: PathBuf::from("/no-refinement-worker"),
        environment: BTreeMap::new(),
    };
    let checkpoint = input(
        PathBuf::from("/absent-checkpoint"),
        CHECKPOINT_BYTES,
        CHECKPOINT_SHA256,
    );
    let statistics = input(
        PathBuf::from("/absent-statistics"),
        STATISTICS_BYTES,
        STATISTICS_SHA256,
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    assert!(matches!(
        infer(
            &runtime,
            &checkpoint,
            &statistics,
            &[0.; 401],
            "too-long",
            &AtomicBool::new(false),
            Instant::now() + Duration::from_secs(601)
        ),
        Err(RefinementError::Configuration(_))
    ));
    for samples in [
        vec![],
        vec![0.; 400],
        vec![0.; 480_001],
        vec![f32::NAN; 401],
    ] {
        assert!(matches!(
            infer(
                &runtime,
                &checkpoint,
                &statistics,
                &samples,
                "invalid",
                &AtomicBool::new(false),
                deadline
            ),
            Err(RefinementError::Configuration(_))
        ));
    }
    assert!(matches!(
        infer(
            &runtime,
            &checkpoint,
            &statistics,
            &[0.; 401],
            "cancelled",
            &AtomicBool::new(true),
            deadline
        ),
        Err(RefinementError::Cancelled)
    ));
    assert!(matches!(
        infer(
            &runtime,
            &checkpoint,
            &statistics,
            &[0.; 401],
            "expired",
            &AtomicBool::new(false),
            Instant::now()
        ),
        Err(RefinementError::Deadline)
    ));
}

#[test]
fn non_english_cached_words_refuse_before_model_loading_or_annotation_changes()
-> Result<(), Box<dyn std::error::Error>> {
    use deadpan_analysis::{AnalysedAudio, Transcript, Word};
    use deadpan_store::{AccessMode, ProjectStore, TranscriptKey};

    let scratch = tempfile::tempdir()?;
    let source = scratch.path().join("original.mp4");
    std::fs::write(
        &source,
        include_bytes!("../../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4"),
    )?;
    let package = scratch.path().join("foreign.deadpan");
    let cancelled = AtomicBool::new(false);
    let created =
        deadpan_cli::single_original::create(&package, &source, "Original", &cancelled, |_, _| {
            Ok(())
        })?;
    let store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let deadline = Instant::now() + Duration::from_secs(60);
    let session = deadpan_cli::transcription::open_original_analysis(
        &store,
        Some(&created.asset_id),
        &cancelled,
        deadline,
    )?;
    let beginning = session.prepare_window(0..401, &cancelled, deadline)?;
    let transcript = Transcript::new(
        AnalysedAudio {
            origin: beginning.analysis_origin,
            sample_rate: beginning.source_rate,
            duration_cs: u32::try_from(session.complete_analysis_frames().div_ceil(160))?,
        },
        vec![Word {
            text: "la".into(),
            start_cs: 100,
            end_cs: 150,
            probability: 0.8,
            segment: 0,
        }],
    )?;
    let key = TranscriptKey {
        content: created.content,
        audio_stream: beginning.audio_stream,
        model_sha256: "b".repeat(64),
        language: "fr".into(),
        engine: "foreign-cached-transcript".into(),
    };
    store.save_transcript(&key, &transcript)?;
    let head = store.head_revision()?;
    drop(session);
    drop(store);
    let models = scratch.path().join("not-installed");
    let error = deadpan_cli::refinement::run(&[
        &package.to_string_lossy(),
        "--word",
        "0",
        "--models-root",
        &models.to_string_lossy(),
    ])
    .unwrap_err();
    assert!(
        error.to_string().contains("English transcripts (en) only"),
        "{error}"
    );
    assert!(!models.exists());
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    assert_eq!(store.head_revision()?, head);
    assert_eq!(store.transcript(&key)?, Some(transcript));
    assert!(
        store
            .analysis_corrections(&deadpan_store::CorrectionsKey {
                content: key.content,
                audio_stream: key.audio_stream,
            })?
            .is_none()
    );
    Ok(())
}

/// Explicit real-worker qualification, using only retained offline model data.
#[test]
fn real_isolated_worker_preserves_all_observed_native_emissions() {
    let Some(worker) = std::env::var_os("DEADPAN_ALIGN_TEST_WORKER") else {
        eprintln!(
            "UNEXERCISED: DEADPAN_ALIGN_TEST_WORKER and DEADPAN_ALIGN_REFERENCE_ROOT are required for actual worker inference"
        );
        return;
    };
    let root =
        PathBuf::from(std::env::var_os("DEADPAN_ALIGN_REFERENCE_ROOT").expect("reference root"));
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
    let checkpoint = input(
        PathBuf::from(std::env::var_os("DEADPAN_ALIGN_CHECKPOINT").expect("original checkpoint")),
        CHECKPOINT_BYTES,
        CHECKPOINT_SHA256,
    );
    let statistics = input(
        root.parent().unwrap().join("upstream/global_stats.json"),
        STATISTICS_BYTES,
        STATISTICS_SHA256,
    );
    let runtime = RefinementRuntime {
        executable: PathBuf::from(worker),
        environment: BTreeMap::new(),
    };
    let cases = manifest["records"].as_array().unwrap();
    assert_eq!(cases.len(), 24);
    let mut reports = Vec::new();
    for case in cases {
        let name = case["case"].as_str().unwrap();
        let pcm = std::fs::read(root.join(case["pcm"]["file"].as_str().unwrap())).unwrap();
        let samples: Vec<_> = pcm
            .chunks_exact(4)
            .map(|v| f32::from_le_bytes(v.try_into().unwrap()))
            .collect();
        let result = infer(
            &runtime,
            &checkpoint,
            &statistics,
            &samples,
            name,
            &AtomicBool::new(false),
            Instant::now() + Duration::from_secs(60),
        )
        .unwrap();
        let expected =
            std::fs::read(root.join(case["emissions"]["file"].as_str().unwrap())).unwrap();
        assert_eq!(result.log_probs.len() * 8, expected.len());
        let maximum = result
            .log_probs
            .iter()
            .zip(expected.chunks_exact(8))
            .map(|(actual, expected)| {
                (actual - f64::from_le_bytes(expected.try_into().unwrap())).abs()
            })
            .fold(0_f64, f64::max);
        assert!(maximum < 0.001, "{name}: {maximum}");
        reports.push(serde_json::json!({"case": name, "frames": result.frames, "input_samples":result.input_samples, "maximum_log_probability_absolute_error":maximum, "runtime":result.runtime, "worker_elapsed_millis":result.elapsed.as_millis()}));
    }
    eprintln!(
        "ACTUAL_REFINEMENT_WORKER {}",
        serde_json::to_string(&reports).unwrap()
    );
}

/// This tone fixture establishes clock/ownership wiring, not phonetic accuracy.
#[test]
fn qualified_original_preparation_is_read_only_and_manual_words_stay_pinned() {
    let Some(worker) = std::env::var_os("DEADPAN_ALIGN_TEST_WORKER") else {
        eprintln!(
            "UNEXERCISED: real Original refinement needs DEADPAN_ALIGN_TEST_WORKER and DEADPAN_ALIGN_MODEL_ROOT"
        );
        return;
    };
    let model_root = PathBuf::from(
        std::env::var_os("DEADPAN_ALIGN_MODEL_ROOT").expect("installed offline model root"),
    );
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(120);
    let models =
        deadpan_cli::models::installed_refinement_models(&model_root, &cancelled, deadline)
            .unwrap()
            .expect("installed model");
    let scratch = tempfile::tempdir().unwrap();
    let source = scratch.path().join("original.mp4");
    std::fs::write(
        &source,
        include_bytes!("../../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4"),
    )
    .unwrap();
    let package = scratch.path().join("refinement.deadpan");
    let created =
        deadpan_cli::single_original::create(&package, &source, "Original", &cancelled, |_, _| {
            Ok(())
        })
        .unwrap();
    let store =
        deadpan_store::ProjectStore::open(&package, deadpan_store::AccessMode::ReadWrite).unwrap();
    let session = deadpan_cli::transcription::open_original_analysis(
        &store,
        Some(&created.asset_id),
        &cancelled,
        deadline,
    )
    .unwrap();
    let beginning = session
        .prepare_window(0..401, &cancelled, deadline)
        .unwrap();
    let frames = session.complete_analysis_frames();
    let transcript = deadpan_analysis::Transcript::new(
        deadpan_analysis::AnalysedAudio {
            origin: beginning.analysis_origin,
            sample_rate: beginning.source_rate,
            duration_cs: u32::try_from(frames.div_ceil(160)).unwrap(),
        },
        vec![deadpan_analysis::Word {
            text: "hello".into(),
            start_cs: 100,
            end_cs: 200,
            probability: 0.8,
            segment: 0,
        }],
    )
    .unwrap();
    let key = deadpan_store::TranscriptKey {
        content: created.content,
        audio_stream: beginning.audio_stream,
        model_sha256: "a".repeat(64),
        language: "en".into(),
        engine: "refinement-clock-fixture".into(),
    };
    store.save_transcript(&key, &transcript).unwrap();
    let head = store.head_revision().unwrap();
    let runtime = RefinementRuntime {
        executable: PathBuf::from(worker),
        environment: BTreeMap::new(),
    };
    let reader =
        deadpan_store::ProjectStore::open(&package, deadpan_store::AccessMode::ReadOnly).unwrap();
    let prepared = deadpan_cli::refinement::prepare(
        &reader,
        &key,
        0..1,
        &runtime,
        &models,
        &cancelled,
        deadline,
    )
    .unwrap();
    assert_eq!(prepared.expected_revision, head);
    assert_eq!(prepared.expected_version, 0);
    assert_eq!(prepared.window.samples.len() as u64, frames);
    assert_eq!(prepared.window.analysis_offset, 0);
    assert_eq!(prepared.window.analysis_origin, transcript.audio().origin);
    assert_eq!(prepared.selection_guard.key, key);
    assert_eq!(prepared.review.offers().len(), 1);
    assert_eq!(store.head_revision().unwrap(), head);
    assert_eq!(store.transcript(&key).unwrap(), Some(transcript.clone()));
    let corrections_key = deadpan_store::CorrectionsKey {
        content: key.content.clone(),
        audio_stream: key.audio_stream,
    };
    assert!(
        store
            .analysis_corrections(&corrections_key)
            .unwrap()
            .is_none()
    );
    let empty =
        deadpan_analysis::Corrections::empty(deadpan_analysis::Corrections::clock_of(&transcript));
    let pinned = empty
        .set_word_bounds(
            &deadpan_analysis::CorrectedTranscript::recognized(transcript.clone()),
            0,
            110,
            190,
        )
        .unwrap()
        .0;
    let saved = store
        .change_analysis_corrections(
            &corrections_key,
            0,
            deadpan_store::CorrectionChange::Apply {
                corrections: pinned,
                label: "manual timing".into(),
            },
        )
        .unwrap();
    let absent_worker = RefinementRuntime {
        executable: PathBuf::from("/no-refinement-worker"),
        environment: BTreeMap::new(),
    };
    assert!(matches!(
        deadpan_cli::refinement::prepare(
            &reader,
            &key,
            0..1,
            &absent_worker,
            &models,
            &cancelled,
            deadline
        ),
        Err(deadpan_cli::CliError::Refinement(
            RefinementError::Configuration(_)
        ))
    ));
    assert_eq!(
        store.analysis_corrections(&corrections_key).unwrap(),
        Some(saved)
    );
    assert_eq!(store.head_revision().unwrap(), head);
}
