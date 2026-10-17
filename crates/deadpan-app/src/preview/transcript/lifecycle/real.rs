//! Opt-in qualification of the lifecycle watcher against the installed native
//! Whisper worker. This deliberately uses no UI or physical sleep/wake.

use super::*;
use deadpan_cli::transcription::{
    AnalysisInput, TranscriptionError, TranscriptionRuntime, transcribe,
};
use deadpan_jobs::Sha256;
use deadpan_jobs::transcription::{Language, ModelInput};
use sha2::Digest;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::time::Instant;

const MODEL_BYTES: u64 = 147_964_211;
const MODEL_SHA256: &str = "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002";

fn required_path(name: &str) -> PathBuf {
    let value = std::env::var_os(name)
        .unwrap_or_else(|| panic!("{name} must name the installed qualification input"));
    std::fs::canonicalize(value).unwrap_or_else(|error| panic!("{name}: {error}"))
}

fn file_sha256(path: &std::path::Path) -> String {
    let mut file =
        File::open(path).unwrap_or_else(|error| panic!("open {}: {error}", path.display()));
    let mut digest = sha2::Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn sine_input(seconds: usize) -> AnalysisInput {
    AnalysisInput {
        samples: (0..16_000 * seconds)
            .map(|index| (index as f32 * 0.05).sin() * 0.1)
            .collect(),
        origin: 0,
        source_rate: 48_000,
    }
}

#[test]
#[ignore = "requires explicit installed Whisper model and deadpan-transcribe helper paths"]
fn real_worker_is_interrupted_by_sleep_then_succeeds_on_explicit_retry() {
    let helper = required_path("DEADPAN_TRANSCRIBE_WORKER");
    let model_path = required_path("DEADPAN_WHISPER_MODEL");
    assert!(
        helper.is_file(),
        "worker helper is not a file: {}",
        helper.display()
    );
    assert!(
        model_path.is_file(),
        "model is not a file: {}",
        model_path.display()
    );
    let metadata = std::fs::metadata(&model_path).expect("read model metadata");
    assert_eq!(metadata.len(), MODEL_BYTES, "unexpected model size");
    assert_eq!(
        file_sha256(&model_path),
        MODEL_SHA256,
        "unexpected model digest"
    );
    let runtime = TranscriptionRuntime {
        executable: helper.clone(),
        environment: Default::default(),
    };
    let model = ModelInput {
        path: model_path,
        sha256: Sha256::new(MODEL_SHA256).unwrap(),
        byte_length: MODEL_BYTES,
    };

    // Four Whisper windows make an interior worker progress report likely,
    // while keeping the generated input small and independent of media files.
    let lifecycle = crate::jobs::Lifecycle::default();
    let cancel = Arc::new(AtomicBool::new(false));
    let run = RunCapture::new(lifecycle.clone(), cancel.clone());
    let injected = Arc::new(AtomicBool::new(false));
    let injected_at = Arc::new(Mutex::new(None));
    let progress_at_injection = Arc::new(Mutex::new(None));
    let attempt_started = Instant::now();
    let interrupted = run.run(|| {
        transcribe(
            &runtime,
            &model,
            &sine_input(120),
            Language::Code("en".into()),
            "lifecycle-sleep-real",
            &cancel,
            Instant::now() + Duration::from_secs(600),
            |percent| {
                if percent > 0 && percent < 100 && !injected.swap(true, Ordering::AcqRel) {
                    *injected_at
                        .lock()
                        .unwrap_or_else(|error| error.into_inner()) = Some(Instant::now());
                    *progress_at_injection
                        .lock()
                        .unwrap_or_else(|error| error.into_inner()) = Some(percent);
                    lifecycle.will_sleep();
                    lifecycle.did_wake();
                }
            },
        )
    });
    let injection_time = injected_at
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .expect("worker never reported interior progress; lifecycle injection was not exercised");
    assert_eq!(run.stop(), Some(Stop::Interrupted));
    assert!(
        matches!(&interrupted, Err(TranscriptionError::Cancelled)),
        "sleep-interrupted worker returned {interrupted:?}"
    );
    let cancellation_drain = injection_time.elapsed();
    assert!(
        cancellation_drain < Duration::from_secs(30),
        "native worker did not drain promptly after sleep: {cancellation_drain:?}"
    );

    // Wake does not restart the old attempt. A fresh capture and cancellation
    // flag represent the user's explicit retry and must complete normally.
    let retry_cancel = Arc::new(AtomicBool::new(false));
    let retry = RunCapture::new(lifecycle, retry_cancel.clone());
    assert_eq!(
        retry.stop(),
        None,
        "wake must not interrupt the fresh retry"
    );
    let retry_started = Instant::now();
    let result = retry.run(|| {
        transcribe(
            &runtime,
            &model,
            &sine_input(1),
            Language::Code("en".into()),
            "lifecycle-awake-retry",
            &retry_cancel,
            Instant::now() + Duration::from_secs(120),
            |_| {},
        )
    });
    let result = result.expect("fresh retry should complete after wake");
    assert_eq!(retry.stop(), None);
    eprintln!(
        "real transcription lifecycle: helper={} model_sha256={} model_bytes={} injected_progress={} attempt_before_injection={:?} cancel_drain={:?} retry={:?} engine={} backend={:?} words={}",
        helper.display(),
        MODEL_SHA256,
        MODEL_BYTES,
        progress_at_injection
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .expect("injection progress recorded"),
        injection_time.duration_since(attempt_started),
        cancellation_drain,
        retry_started.elapsed(),
        result.runtime.engine,
        result.runtime.backend,
        result.transcript.words().len(),
    );
}
