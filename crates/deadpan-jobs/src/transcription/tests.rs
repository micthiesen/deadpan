use super::*;

const MODEL: &str = "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002";
const AUDIO: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const OUTPUT: &str = "2222222222222222222222222222222222222222222222222222222222222222";

fn transcribe() -> HostMessage {
    HostMessage::Transcribe {
        protocol: VERSION,
        request: RequestId::new("transcript-1").unwrap(),
        attempt: AttemptId::new("attempt-1").unwrap(),
        cancellation_token: CancellationToken::new("cancel-1").unwrap(),
        model: ModelInput {
            path: PathBuf::from("/models/ggml-base.en.bin"),
            sha256: Sha256::new(MODEL).unwrap(),
            byte_length: 147_964_211,
        },
        audio: WorkspaceArtifact::new(
            WorkspaceRef::new("input/analysis.f32").unwrap(),
            Sha256::new(AUDIO).unwrap(),
            16_000 * 4,
        )
        .unwrap(),
        language: Language::Code("en".into()),
        output_scope: WorkspaceRef::new("output").unwrap(),
        maximum_output_bytes: 1024 * 1024,
        timeout_millis: 60_000,
    }
}

fn completed(reference: &str, bytes: u64, model: &str) -> WorkerMessage {
    WorkerMessage::Completed {
        protocol: VERSION,
        request: RequestId::new("transcript-1").unwrap(),
        attempt: AttemptId::new("attempt-1").unwrap(),
        transcript: WorkspaceArtifact::new(
            WorkspaceRef::new(reference).unwrap(),
            Sha256::new(OUTPUT).unwrap(),
            bytes,
        )
        .unwrap(),
        runtime: RuntimeReport {
            engine: "whisper.cpp 1.8.3".into(),
            backend: Backend::Metal,
            model_sha256: Sha256::new(model).unwrap(),
        },
        elapsed_millis: 1_200,
    }
}

fn with(change: impl FnOnce(&mut HostMessage)) -> HostMessage {
    let mut message = transcribe();
    change(&mut message);
    message
}

#[test]
fn requests_bound_paths_audio_scope_budget_and_timeout() {
    assert!(transcribe().validate().is_ok());
    let invalid = [
        with(|m| {
            if let HostMessage::Transcribe { model, .. } = m {
                model.path = PathBuf::from("relative/model.bin");
            }
        }),
        with(|m| {
            if let HostMessage::Transcribe { model, .. } = m {
                model.byte_length = 0;
            }
        }),
        with(|m| {
            if let HostMessage::Transcribe { audio, .. } = m {
                *audio = WorkspaceArtifact::new(
                    WorkspaceRef::new("input/analysis.f32").unwrap(),
                    Sha256::new(AUDIO).unwrap(),
                    6,
                )
                .unwrap();
            }
        }),
        with(|m| {
            if let HostMessage::Transcribe { audio, .. } = m {
                *audio = WorkspaceArtifact::new(
                    WorkspaceRef::new("output/analysis.f32").unwrap(),
                    Sha256::new(AUDIO).unwrap(),
                    8,
                )
                .unwrap();
            }
        }),
        with(|m| {
            if let HostMessage::Transcribe { output_scope, .. } = m {
                *output_scope = WorkspaceRef::new("input/out").unwrap();
            }
        }),
        with(|m| {
            if let HostMessage::Transcribe {
                maximum_output_bytes,
                ..
            } = m
            {
                *maximum_output_bytes = MAX_TRANSCRIPT_BYTES + 1;
            }
        }),
        with(|m| {
            if let HostMessage::Transcribe { timeout_millis, .. } = m {
                *timeout_millis = 0;
            }
        }),
        with(|m| {
            if let HostMessage::Transcribe { protocol, .. } = m {
                *protocol = VERSION + 1;
            }
        }),
    ];
    for message in invalid {
        assert!(message.validate().is_err(), "{message:?}");
        assert!(TranscriptionProtocol::from_request(&message).is_err());
    }
    let cancel = HostMessage::Cancel {
        protocol: VERSION,
        request: RequestId::new("transcript-1").unwrap(),
        attempt: AttemptId::new("attempt-1").unwrap(),
        cancellation_token: CancellationToken::new("cancel-1").unwrap(),
    };
    assert!(TranscriptionProtocol::from_request(&cancel).is_err());
}

#[test]
fn languages_are_auto_or_two_lowercase_letters() {
    for (text, ok) in [
        ("auto", true),
        ("en", true),
        ("de", true),
        ("EN", false),
        ("eng", false),
        ("", false),
    ] {
        assert_eq!(Language::try_from(text.to_owned()).is_ok(), ok, "{text}");
    }
    assert_eq!(Language::Automatic.code(), None);
    assert_eq!(Language::Code("ja".into()).code(), Some("ja"));
}

#[test]
fn responses_bind_identity_scope_budget_and_the_verified_model() {
    let protocol = TranscriptionProtocol::from_request(&transcribe()).unwrap();
    assert_eq!(
        protocol.classify(&completed("output/transcript.json", 900, MODEL)),
        Ok(ResponseKind::Completed)
    );
    assert!(
        protocol
            .classify(&completed("outputs/transcript.json", 900, MODEL))
            .is_err()
    );
    assert!(
        protocol
            .classify(&completed("output/transcript.json", 2 * 1024 * 1024, MODEL))
            .is_err()
    );
    assert!(
        protocol
            .classify(&completed("output/transcript.json", 900, AUDIO))
            .is_err()
    );
    let progress = WorkerMessage::Progress {
        protocol: VERSION,
        request: RequestId::new("transcript-1").unwrap(),
        attempt: AttemptId::new("attempt-2").unwrap(),
        percent: 10,
    };
    assert!(protocol.classify(&progress).is_err());
    let overrun = WorkerMessage::Progress {
        protocol: VERSION,
        request: RequestId::new("transcript-1").unwrap(),
        attempt: AttemptId::new("attempt-1").unwrap(),
        percent: 101,
    };
    assert!(protocol.classify(&overrun).is_err());
    let failed = WorkerMessage::Failed {
        protocol: VERSION,
        request: RequestId::new("transcript-1").unwrap(),
        attempt: AttemptId::new("attempt-1").unwrap(),
        diagnostic: Diagnostic::new("model hash differs").unwrap(),
    };
    assert_eq!(protocol.classify(&failed), Ok(ResponseKind::Failed));
    assert!(matches!(
        protocol.cancellation(),
        HostMessage::Cancel {
            protocol: VERSION,
            ..
        }
    ));
}

#[test]
fn framed_messages_round_trip_and_reject_unknown_fields() {
    let mut wire = Vec::new();
    TranscriptionProtocol::write_request(&mut wire, &transcribe()).unwrap();
    assert_eq!(read_host(&mut wire.as_slice()).unwrap(), Some(transcribe()));
    let mut wire = Vec::new();
    write_worker(&mut wire, &completed("output/transcript.json", 900, MODEL)).unwrap();
    assert_eq!(
        TranscriptionProtocol::read_response(&mut wire.as_slice()).unwrap(),
        Some(completed("output/transcript.json", 900, MODEL))
    );
    let mut value = serde_json::to_value(transcribe()).unwrap();
    value["unexpected"] = serde_json::json!(1);
    assert!(serde_json::from_value::<HostMessage>(value).is_err());
    let engine = WorkerMessage::Completed {
        protocol: VERSION,
        request: RequestId::new("transcript-1").unwrap(),
        attempt: AttemptId::new("attempt-1").unwrap(),
        transcript: WorkspaceArtifact::new(
            WorkspaceRef::new("output/transcript.json").unwrap(),
            Sha256::new(OUTPUT).unwrap(),
            1,
        )
        .unwrap(),
        runtime: RuntimeReport {
            engine: "x".repeat(65),
            backend: Backend::Cpu,
            model_sha256: Sha256::new(MODEL).unwrap(),
        },
        elapsed_millis: 1,
    };
    assert!(write_worker(&mut Vec::new(), &engine).is_err());
}

const VAD: &str = "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987";

fn detect() -> HostMessage {
    HostMessage::DetectSpeech {
        protocol: VERSION,
        request: RequestId::new("activity-1").unwrap(),
        attempt: AttemptId::new("attempt-1").unwrap(),
        cancellation_token: CancellationToken::new("cancel-1").unwrap(),
        model: ModelInput {
            path: PathBuf::from("/models/ggml-silero-v6.2.0.bin"),
            sha256: Sha256::new(VAD).unwrap(),
            byte_length: 885_098,
        },
        audio: WorkspaceArtifact::new(
            WorkspaceRef::new("input/analysis.f32").unwrap(),
            Sha256::new(AUDIO).unwrap(),
            16_000 * 4,
        )
        .unwrap(),
        output_scope: WorkspaceRef::new("output").unwrap(),
        maximum_output_bytes: 32 * 4,
        timeout_millis: 60_000,
    }
}

fn detected(reference: &str, bytes: u64, model: &str) -> WorkerMessage {
    WorkerMessage::SpeechDetected {
        protocol: VERSION,
        request: RequestId::new("activity-1").unwrap(),
        attempt: AttemptId::new("attempt-1").unwrap(),
        probabilities: WorkspaceArtifact::new(
            WorkspaceRef::new(reference).unwrap(),
            Sha256::new(OUTPUT).unwrap(),
            bytes,
        )
        .unwrap(),
        runtime: RuntimeReport {
            engine: "whisper.cpp 1.8.3".into(),
            backend: Backend::Cpu,
            model_sha256: Sha256::new(model).unwrap(),
        },
        elapsed_millis: 12,
    }
}

#[test]
fn speech_detection_requests_bound_their_probability_budget() {
    assert!(detect().validate().is_ok());
    let protocol = TranscriptionProtocol::from_request(&detect()).unwrap();
    assert_eq!(protocol.operation(), Operation::DetectSpeech);
    let budget = |bytes| {
        let mut message = detect();
        if let HostMessage::DetectSpeech {
            maximum_output_bytes,
            ..
        } = &mut message
        {
            *maximum_output_bytes = bytes;
        }
        message
    };
    assert!(budget(0).validate().is_err());
    assert!(budget(MAX_PROBABILITY_BYTES).validate().is_ok());
    assert!(budget(MAX_PROBABILITY_BYTES + 1).validate().is_err());
    let mut wrong = detect();
    if let HostMessage::DetectSpeech { output_scope, .. } = &mut wrong {
        *output_scope = WorkspaceRef::new("input").unwrap();
    }
    assert!(TranscriptionProtocol::from_request(&wrong).is_err());
    assert_eq!(MAX_PROBABILITY_BYTES, MAX_ANALYSIS_FRAMES.div_ceil(512) * 4);
}

#[test]
fn each_attempt_accepts_only_its_own_completion_kind() {
    let detecting = TranscriptionProtocol::from_request(&detect()).unwrap();
    assert_eq!(
        detecting.classify(&detected("output/activity.f32", 32 * 4, VAD)),
        Ok(ResponseKind::Completed)
    );
    for refused in [
        detected("output/activity.f32", 33 * 4, VAD),
        detected("output/activity.f32", 6, VAD),
        detected("elsewhere/activity.f32", 8, VAD),
        detected("output/activity.f32", 8, MODEL),
    ] {
        assert!(detecting.classify(&refused).is_err(), "{refused:?}");
    }
    let mut transcript = completed("output/transcript.json", 8, VAD);
    if let WorkerMessage::Completed {
        request, attempt, ..
    } = &mut transcript
    {
        *request = RequestId::new("activity-1").unwrap();
        *attempt = AttemptId::new("attempt-1").unwrap();
    }
    assert!(detecting.classify(&transcript).is_err());

    let transcribing = TranscriptionProtocol::from_request(&transcribe()).unwrap();
    assert_eq!(transcribing.operation(), Operation::Transcribe);
    let mut probabilities = detected("output/activity.f32", 8, MODEL);
    if let WorkerMessage::SpeechDetected {
        request, attempt, ..
    } = &mut probabilities
    {
        *request = RequestId::new("transcript-1").unwrap();
        *attempt = AttemptId::new("attempt-1").unwrap();
    }
    assert!(transcribing.classify(&probabilities).is_err());
}

#[test]
fn speech_detection_messages_round_trip() {
    let mut wire = Vec::new();
    TranscriptionProtocol::write_request(&mut wire, &detect()).unwrap();
    assert_eq!(read_host(&mut wire.as_slice()).unwrap(), Some(detect()));
    let mut wire = Vec::new();
    write_worker(&mut wire, &detected("output/activity.f32", 8, VAD)).unwrap();
    assert_eq!(
        TranscriptionProtocol::read_response(&mut wire.as_slice()).unwrap(),
        Some(detected("output/activity.f32", 8, VAD))
    );
    let mut value = serde_json::to_value(detect()).unwrap();
    value["language"] = serde_json::json!("en");
    assert!(serde_json::from_value::<HostMessage>(value).is_err());
}
