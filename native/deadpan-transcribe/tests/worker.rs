//! The real worker executable under the shared supervisor, without a model:
//! verification and load failures must surface as typed worker failures.
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_cli::transcription::{
    AnalysisInput, TranscriptionError, TranscriptionRuntime, transcribe,
};
use deadpan_jobs::Sha256;
use deadpan_jobs::transcription::{Language, ModelInput};
use sha2::Digest;

fn runtime() -> TranscriptionRuntime {
    TranscriptionRuntime {
        executable: PathBuf::from(env!("CARGO_BIN_EXE_deadpan-transcribe")),
        environment: Default::default(),
    }
}

fn input() -> AnalysisInput {
    AnalysisInput {
        samples: (0..16_000).map(|i| (i as f32 * 0.05).sin() * 0.1).collect(),
        origin: 0,
        source_rate: 48_000,
    }
}

fn run(model: &ModelInput) -> Result<(), TranscriptionError> {
    transcribe(
        &runtime(),
        model,
        &input(),
        Language::Code("en".into()),
        "worker-test",
        &AtomicBool::new(false),
        Instant::now() + Duration::from_secs(60),
        |_| {},
    )
    .map(|_| ())
}

fn model_file(bytes: &[u8]) -> (tempfile::TempDir, PathBuf, String) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("model.bin");
    std::fs::write(&path, bytes).unwrap();
    let digest = sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    (directory, std::fs::canonicalize(path).unwrap(), digest)
}

#[test]
fn a_model_that_differs_from_its_manifest_is_refused_before_loading() {
    let (_directory, path, _) = model_file(b"not the verified model");
    let error = run(&ModelInput {
        path,
        sha256: Sha256::new("0".repeat(64)).unwrap(),
        byte_length: 22,
    })
    .unwrap_err();
    assert!(
        matches!(&error, TranscriptionError::Worker(message) if message.contains("model hash differs")),
        "{error}"
    );
}

#[test]
fn a_size_mismatch_and_an_unloadable_model_fail_without_a_transcript() {
    let (_directory, path, digest) = model_file(b"verified bytes that are not ggml");
    let size = run(&ModelInput {
        path: path.clone(),
        sha256: Sha256::new(digest.clone()).unwrap(),
        byte_length: 7,
    })
    .unwrap_err();
    assert!(
        matches!(&size, TranscriptionError::Worker(message) if message.contains("model size differs")),
        "{size}"
    );
    let load = run(&ModelInput {
        path,
        sha256: Sha256::new(digest).unwrap(),
        byte_length: 32,
    })
    .unwrap_err();
    assert!(
        matches!(&load, TranscriptionError::Worker(message) if message.contains("load model")),
        "{load}"
    );
}

#[test]
fn a_missing_model_or_expired_deadline_is_reported() {
    let missing = run(&ModelInput {
        path: PathBuf::from("/nonexistent/deadpan/model.bin"),
        sha256: Sha256::new("0".repeat(64)).unwrap(),
        byte_length: 1,
    })
    .unwrap_err();
    assert!(
        matches!(missing, TranscriptionError::Worker(_)),
        "{missing}"
    );
    let (_directory, path, digest) = model_file(b"x");
    let expired = transcribe(
        &runtime(),
        &ModelInput {
            path,
            sha256: Sha256::new(digest).unwrap(),
            byte_length: 1,
        },
        &input(),
        Language::Automatic,
        "worker-deadline",
        &AtomicBool::new(false),
        Instant::now(),
        |_| {},
    )
    .unwrap_err();
    assert!(matches!(expired, TranscriptionError::Deadline), "{expired}");
}
