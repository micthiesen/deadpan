use super::*;
use std::path::PathBuf;

const AUDIO: &str = "1111111111111111111111111111111111111111111111111111111111111111";
fn infer() -> HostMessage {
    HostMessage::Infer {
        protocol: VERSION,
        request: RequestId::new("request-1").unwrap(),
        attempt: AttemptId::new("attempt-1").unwrap(),
        cancellation_token: CancellationToken::new("cancel-1").unwrap(),
        checkpoint: Box::new(ModelInput {
            path: PathBuf::from("/models/model-0.6.ckpt"),
            sha256: Sha256::new(CHECKPOINT_SHA256).unwrap(),
            byte_length: CHECKPOINT_BYTES,
        }),
        statistics: Box::new(ModelInput {
            path: PathBuf::from("/models/global_stats.json"),
            sha256: Sha256::new(STATISTICS_SHA256).unwrap(),
            byte_length: STATISTICS_BYTES,
        }),
        audio: WorkspaceArtifact::new(
            WorkspaceRef::new("input/audio.f32le").unwrap(),
            Sha256::new(AUDIO).unwrap(),
            16_000 * 4,
        )
        .unwrap(),
        output_scope: WorkspaceRef::new("output").unwrap(),
        max_output_bytes: MAX_OUTPUT_BYTES,
        timeout_ms: 60_000,
    }
}
fn runtime() -> RuntimeReport {
    RuntimeReport {
        engine: ENGINE_ID.into(),
        ort_version: ORT_VERSION.into(),
        backend: Backend::Cpu,
        checkpoint_sha256: Sha256::new(CHECKPOINT_SHA256).unwrap(),
        statistics_sha256: Sha256::new(STATISTICS_SHA256).unwrap(),
        graph_sha256: Sha256::new(GRAPH_SHA256).unwrap(),
    }
}
fn completed() -> WorkerMessage {
    let frames = output_frames(16_000).unwrap();
    WorkerMessage::Completed {
        protocol: VERSION,
        request: RequestId::new("request-1").unwrap(),
        attempt: AttemptId::new("attempt-1").unwrap(),
        emissions: WorkspaceArtifact::new(
            WorkspaceRef::new("output/emissions.f32le").unwrap(),
            Sha256::new(AUDIO).unwrap(),
            u64::from(frames) * 41 * 4,
        )
        .unwrap(),
        input_samples: 16_000,
        frames,
        runtime: runtime(),
        elapsed_millis: 30,
    }
}

#[test]
fn bounds_exact_model_pins_pcm_scope_and_shape() {
    assert!(infer().validate().is_ok());
    assert_eq!(output_frames(401), Ok(2));
    assert_eq!(output_frames(480_000), Ok(1501));
    for bad in [0, 400, 480_001, u64::MAX] {
        assert!(output_frames(bad).is_err());
    }
    for mutation in 0..10 {
        let mut message = infer();
        if let HostMessage::Infer {
            protocol,
            checkpoint,
            statistics,
            audio,
            output_scope,
            max_output_bytes,
            timeout_ms,
            ..
        } = &mut message
        {
            match mutation {
                0 => *protocol += 1,
                1 => checkpoint.path = PathBuf::from("relative/model"),
                2 => checkpoint.sha256 = Sha256::new(AUDIO).unwrap(),
                3 => statistics.byte_length = 4097,
                4 => statistics.sha256 = Sha256::new(AUDIO).unwrap(),
                5 => {
                    *audio = WorkspaceArtifact::new(
                        WorkspaceRef::new("input/sub/audio").unwrap(),
                        Sha256::new(AUDIO).unwrap(),
                        64_000,
                    )
                    .unwrap()
                }
                6 => {
                    *audio = WorkspaceArtifact::new(
                        WorkspaceRef::new("input/audio").unwrap(),
                        Sha256::new(AUDIO).unwrap(),
                        1600,
                    )
                    .unwrap()
                }
                7 => *output_scope = WorkspaceRef::new("input/sub").unwrap(),
                8 => *max_output_bytes = 1,
                9 => *timeout_ms = MAX_TIMEOUT_MS + 1,
                _ => unreachable!(),
            }
        }
        assert!(message.validate().is_err(), "mutation {mutation}");
        assert!(RefinementProtocol::from_request(&message).is_err());
    }
}

#[test]
fn binds_completed_identity_input_scope_output_bytes_and_every_runtime_pin() {
    let protocol = RefinementProtocol::from_request(&infer()).unwrap();
    assert_eq!(protocol.classify(&completed()), Ok(ResponseKind::Completed));
    for mutation in 0..10 {
        let mut message = completed();
        if let WorkerMessage::Completed {
            request,
            attempt,
            emissions,
            input_samples,
            frames,
            runtime,
            ..
        } = &mut message
        {
            match mutation {
                0 => *request = RequestId::new("other").unwrap(),
                1 => *attempt = AttemptId::new("other").unwrap(),
                2 => *input_samples += 1,
                3 => *frames += 1,
                4 => {
                    *emissions = WorkspaceArtifact::new(
                        WorkspaceRef::new("output/lookalike.f32le").unwrap(),
                        Sha256::new(AUDIO).unwrap(),
                        emissions.byte_length(),
                    )
                    .unwrap()
                }
                5 => runtime.engine = "other".into(),
                6 => runtime.ort_version = "1.30.0".into(),
                7 => runtime.checkpoint_sha256 = Sha256::new(AUDIO).unwrap(),
                8 => runtime.statistics_sha256 = Sha256::new(AUDIO).unwrap(),
                9 => runtime.graph_sha256 = Sha256::new(AUDIO).unwrap(),
                _ => unreachable!(),
            }
        }
        assert!(protocol.classify(&message).is_err(), "mutation {mutation}");
    }
    let mut message = completed();
    if let WorkerMessage::Completed { emissions, .. } = &mut message {
        *emissions = WorkspaceArtifact::new(
            WorkspaceRef::new("output/emissions.f32le").unwrap(),
            Sha256::new(AUDIO).unwrap(),
            emissions.byte_length() + 4,
        )
        .unwrap();
    }
    assert!(protocol.classify(&message).is_err());
}

#[test]
fn cancellation_needs_all_three_identities() {
    let protocol = RefinementProtocol::from_request(&infer()).unwrap();
    let cancel = protocol.cancellation();
    assert!(cancel.cancels(&protocol.request, &protocol.attempt, &protocol.token));
    assert!(!cancel.cancels(
        &RequestId::new("other").unwrap(),
        &protocol.attempt,
        &protocol.token
    ));
    assert!(!cancel.cancels(
        &protocol.request,
        &AttemptId::new("other").unwrap(),
        &protocol.token
    ));
    assert!(!cancel.cancels(
        &protocol.request,
        &protocol.attempt,
        &CancellationToken::new("other").unwrap()
    ));
    assert!(RefinementProtocol::from_request(&cancel).is_err());
}

#[test]
fn framed_round_trips_reject_unknown_fields_and_wrong_protocol() {
    let mut wire = Vec::new();
    RefinementProtocol::write_request(&mut wire, &infer()).unwrap();
    assert_eq!(read_host(&mut wire.as_slice()).unwrap(), Some(infer()));
    wire.clear();
    write_worker(&mut wire, &completed()).unwrap();
    assert_eq!(
        RefinementProtocol::read_response(&mut wire.as_slice()).unwrap(),
        Some(completed())
    );
    let mut value = serde_json::to_value(infer()).unwrap();
    value["transcript"] = serde_json::json!("not accepted by inference");
    assert!(serde_json::from_value::<HostMessage>(value).is_err());
    let mut value = serde_json::to_value(runtime()).unwrap();
    value["backend"] = serde_json::json!("metal");
    assert!(serde_json::from_value::<RuntimeReport>(value).is_err());
}
