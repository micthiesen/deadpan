//! Opt-in qualification of standalone Silero cancellation after detector load.

use super::*;
use deadpan_cli::transcription::{AnalysisInput, TranscriptionRuntime};
use deadpan_jobs::Sha256;
use deadpan_jobs::transcription::ModelInput;
use sha2::Digest;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::time::Instant;

const MODEL_BYTES: u64 = 885_098;
const MODEL_SHA256: &str = "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987";

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
#[ignore = "requires explicit installed Silero model and deadpan-transcribe helper paths"]
fn real_vad_worker_is_cancelled_after_model_load_and_succeeds_on_fresh_retry() {
    let helper = required_path("DEADPAN_TRANSCRIBE_WORKER");
    let model_path = required_path("DEADPAN_SILERO_MODEL");
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

    // The worker's zero progress event is emitted after WhisperVadContext has
    // loaded and immediately before its synchronous detect_speech call.
    // Trigger from that observed event, never from a guessed delay.
    let lifecycle = crate::jobs::Lifecycle::default();
    let cancel = Arc::new(AtomicBool::new(false));
    let run = RunCapture::new(lifecycle.clone(), cancel.clone());
    let detector_entered = Arc::new(AtomicBool::new(false));
    let injected_at = Arc::new(Mutex::new(None));
    let attempt_started = Instant::now();
    let interrupted = run.run(|| {
        deadpan_cli::activity::detect_speech_with_progress(
            &runtime,
            &model,
            &sine_input(120),
            "vad-lifecycle-sleep-real",
            &cancel,
            Instant::now() + Duration::from_secs(600),
            |percent| {
                assert_eq!(percent, 0, "unexpected VAD progress percentage");
                if !detector_entered.swap(true, Ordering::AcqRel) {
                    *injected_at
                        .lock()
                        .unwrap_or_else(|error| error.into_inner()) = Some(Instant::now());
                    lifecycle.will_sleep();
                    lifecycle.did_wake();
                }
            },
        )
    });
    let injection_time = injected_at
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .expect("worker never reported detector-loaded entry; injection was not exercised");
    assert_eq!(run.stop(), Some(Stop::Interrupted));
    assert!(
        matches!(
            &interrupted,
            Err(deadpan_cli::transcription::TranscriptionError::Cancelled)
        ),
        "sleep-interrupted VAD worker returned {interrupted:?}"
    );
    let cancellation_drain = injection_time.elapsed();
    assert!(
        cancellation_drain < Duration::from_secs(30),
        "VAD worker did not drain promptly after lifecycle injection: {cancellation_drain:?}"
    );

    let retry_cancel = Arc::new(AtomicBool::new(false));
    let retry = RunCapture::new(lifecycle, retry_cancel.clone());
    assert_eq!(
        retry.stop(),
        None,
        "wake must not interrupt the fresh retry"
    );
    let retry_started = Instant::now();
    let result = retry.run(|| {
        deadpan_cli::activity::detect_speech(
            &runtime,
            &model,
            &sine_input(1),
            "vad-lifecycle-awake-retry",
            &retry_cancel,
            Instant::now() + Duration::from_secs(120),
        )
    });
    let result = result.expect("fresh VAD retry should complete after wake");
    assert_eq!(retry.stop(), None);
    eprintln!(
        "real speech activity lifecycle: helper={} model_sha256={} model_bytes={} attempt_before_detector_entry={:?} cancel_drain={:?} retry={:?} engine={} backend={:?} retry_hops={}",
        helper.display(),
        MODEL_SHA256,
        MODEL_BYTES,
        injection_time.duration_since(attempt_started),
        cancellation_drain,
        retry_started.elapsed(),
        result.runtime.engine,
        result.runtime.backend,
        result.activity.speech().len(),
    );
}
