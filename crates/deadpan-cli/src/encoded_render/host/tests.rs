use super::*;

#[test]
fn supervision_errors_and_control_expiry_invalidate_typed_reports() {
    for secondary in [
        EncodedRenderError::Deadline,
        EncodedRenderError::Cancelled,
        EncodedRenderError::Supervisor(deadpan_jobs::process::SupervisorError::Request(
            "poll teardown failed".into(),
        )),
    ] {
        let reported = protocol::EncodedFailure {
            kind: protocol::EncodedFailureKind::Encoder(
                deadpan_encode::EncodeFailureKind::EncoderUnavailable,
            ),
            diagnostic: deadpan_jobs::Diagnostic::new("first measured failure").unwrap(),
        };
        let expected = secondary.to_string();
        let error = invalidate_report(Some(reported.into()), secondary);
        let EncodedRenderError::WorkerFault { primary, fault } = error else {
            panic!("a later host failure must invalidate the report: {error:?}");
        };
        assert!(matches!(*primary, EncodedRenderError::WorkerFailure(_)));
        assert_eq!(fault, expected);
    }
    assert!(matches!(
        invalidate_report(None, EncodedRenderError::Deadline),
        EncodedRenderError::Deadline
    ));
}

#[test]
fn snapshot_interruption_retains_the_public_cancellation_and_deadline_outcomes() {
    assert!(matches!(
        artifact_failure(ArtifactError::Interrupted(SnapshotInterruption::Cancelled)),
        EncodedRenderError::Cancelled
    ));
    assert!(matches!(
        artifact_failure(ArtifactError::Interrupted(SnapshotInterruption::Deadline)),
        EncodedRenderError::Deadline
    ));
    assert!(matches!(
        artifact_failure(ArtifactError::SourceMutated),
        EncodedRenderError::Artifact(ArtifactError::SourceMutated)
    ));
}

#[test]
fn artifact_admission_can_retain_the_native_maximum_without_the_raw_picture_spool_limit() {
    let limits = EncodedWorkerLimits::default();
    assert!(limits.encode.maximum_output_bytes > crate::render_worker::protocol::MAX_PICTURE_BYTES);
    assert_eq!(
        ArtifactLimits::new(limits.encode.maximum_output_bytes)
            .unwrap()
            .maximum_bytes(),
        deadpan_encode::MAX_OUTPUT_BYTES
    );
}

#[cfg(target_os = "macos")]
mod finalization {
    use super::*;
    use deadpan_jobs::process::{ResponseKind, SupervisorError};

    struct PanicReader;

    impl WorkerProtocol for PanicReader {
        type Request = ();
        type Response = ();

        fn from_request(_: &()) -> Result<Self, SupervisorError> {
            Ok(Self)
        }
        fn cancellation(&self) {}
        fn write_request(_: &mut impl Write, _: &()) -> Result<(), String> {
            Ok(())
        }
        fn read_response(_: &mut impl Read) -> Result<Option<()>, String> {
            panic!("injected protocol reader panic")
        }
        fn classify(&self, _: &()) -> Result<ResponseKind, String> {
            Ok(ResponseKind::Progress)
        }
    }

    fn process(workspace: &std::path::Path) -> SupervisedProcess<PanicReader> {
        SupervisedProcess::spawn(
            ProcessSpec {
                executable: "/bin/sleep".into(),
                arguments: vec!["60".into()],
                environment: Default::default(),
                workspace: workspace.into(),
                limits: ProcessLimits {
                    maximum_duration: Duration::from_secs(60),
                    cancellation_grace: Duration::from_millis(50),
                    exit_grace: Duration::from_millis(50),
                },
            },
            (),
        )
        .unwrap()
    }

    #[test]
    fn joined_pump_panic_cannot_promote_a_successful_stage() {
        let workspace = tempfile::tempdir().unwrap();
        let mut child = process(workspace.path());
        let error = finish_owned_result(&mut child, Ok(())).unwrap_err();
        assert!(error.cleanup_confirmed());
        assert!(
            matches!(error, EncodedRenderError::Worker(message) if message.contains("pump panicked"))
        );
    }

    #[test]
    fn confirmed_cleanup_preserves_the_primary_error() {
        let workspace = tempfile::tempdir().unwrap();
        let mut child = process(workspace.path());
        let error = finish_owned_result::<(), _>(&mut child, Err(EncodedRenderError::Deadline))
            .unwrap_err();
        assert!(error.cleanup_confirmed());
        assert!(matches!(error, EncodedRenderError::Deadline));
    }

    #[test]
    fn joined_pump_panic_invalidates_a_typed_worker_report() {
        let workspace = tempfile::tempdir().unwrap();
        let mut child = process(workspace.path());
        let reported = crate::encoded_render::protocol::EncodedFailure {
            kind: crate::encoded_render::protocol::EncodedFailureKind::Encoder(
                deadpan_encode::EncodeFailureKind::EncoderUnavailable,
            ),
            diagnostic: deadpan_jobs::Diagnostic::new("named video codec missing").unwrap(),
        };
        let error = finish_owned_result::<(), _>(&mut child, Err(reported.into())).unwrap_err();
        assert!(error.cleanup_confirmed());
        let EncodedRenderError::WorkerFault { primary, fault } = error else {
            panic!("pump failure must invalidate the typed report: {error:?}");
        };
        assert!(matches!(*primary, EncodedRenderError::WorkerFailure(_)));
        assert!(fault.contains("pump panicked"));
    }
}
