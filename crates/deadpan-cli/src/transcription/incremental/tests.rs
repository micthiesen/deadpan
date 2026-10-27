use super::*;
use sha2::Digest;
use std::time::Duration;

#[test]
fn coordinator_refusal_and_cancellation_precede_both_actual_worker_calls() {
    let runtime = TranscriptionRuntime {
        executable: "/no-such-incremental-worker".into(),
        environment: Default::default(),
    };
    let model = ModelInput {
        path: "/no-such-incremental-model".into(),
        sha256: deadpan_jobs::Sha256::new("a".repeat(64)).unwrap(),
        byte_length: 1,
    };
    let input = AnalysisInput {
        samples: vec![0.0; 512],
        origin: 0,
        source_rate: 44_100,
    };
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(5);
    for vad in [None, Some(&model)] {
        let result = infer_range(
            &runtime,
            &model,
            vad,
            &input,
            Language::Code("en".into()),
            &cancelled,
            deadline,
            || Err(TranscriptionError::Configuration("coordinator refused")),
        );
        assert!(matches!(
            result,
            Err(TranscriptionError::Configuration("coordinator refused"))
        ));
        let result = infer_range(
            &runtime,
            &model,
            vad,
            &input,
            Language::Code("en".into()),
            &cancelled,
            deadline,
            || {
                cancelled.store(true, Ordering::Release);
                Ok(())
            },
        );
        assert!(matches!(result, Err(TranscriptionError::Cancelled)));
        cancelled.store(false, Ordering::Release);
    }
}

#[test]
fn helper_hash_refuses_fifo_symlink_directory_and_excessive_file_before_reading()
-> Result<(), Box<dyn std::error::Error>> {
    let scratch = tempfile::tempdir()?;
    let fifo = scratch.path().join("worker.fifo");
    #[cfg(target_os = "linux")]
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )?;
    #[cfg(target_os = "macos")]
    {
        let mut command = std::process::Command::new("mkfifo");
        command.arg(&fifo);
        assert!(
            deadpan_native_process::spawn(&mut command)?
                .wait()?
                .success()
        );
    }
    let regular = scratch.path().join("regular");
    std::fs::write(&regular, b"qualified helper bytes")?;
    let symlink = scratch.path().join("symlink");
    std::os::unix::fs::symlink(&regular, &symlink)?;
    let oversized = scratch.path().join("large");
    std::fs::File::create(&oversized)?.set_len(256 * 1024 * 1024 + 1)?;
    let cancel = AtomicBool::new(false);
    for path in [&fifo, &symlink, &oversized, &scratch.path().to_owned()] {
        let began = Instant::now();
        let runtime = TranscriptionRuntime {
            executable: path.clone(),
            environment: Default::default(),
        };
        assert!(worker_hash(&runtime, &cancel, began + Duration::from_secs(1)).is_err());
        assert!(began.elapsed() < Duration::from_secs(1));
    }
    let runtime = TranscriptionRuntime {
        executable: regular,
        environment: Default::default(),
    };
    assert_eq!(
        worker_hash(&runtime, &cancel, Instant::now() + Duration::from_secs(1))?,
        crate::transcription::hex(&sha2::Sha256::digest(b"qualified helper bytes"))
    );
    assert!(matches!(
        worker_hash(
            &runtime,
            &AtomicBool::new(true),
            Instant::now() + Duration::from_secs(1)
        ),
        Err(TranscriptionError::Cancelled)
    ));
    Ok(())
}
