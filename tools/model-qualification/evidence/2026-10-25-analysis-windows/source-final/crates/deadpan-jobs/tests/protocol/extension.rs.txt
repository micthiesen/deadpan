use super::*;
use deadpan_jobs::{EXTENSION_PROTOCOL_VERSION, ValueError};
use serde_json::{Value, json};

fn extension_request() -> HostMessage {
    serde_json::from_str(include_str!("../fixtures/generate_extension_v3.json")).unwrap()
}

fn extension_completed() -> WorkerMessage {
    serde_json::from_str(include_str!("../fixtures/completed_extension_v3.json")).unwrap()
}

fn read_host(value: &Value) -> Result<Option<HostMessage>, CodecError> {
    let mut wire = Vec::new();
    deadpan_jobs::write_frame(&mut wire, value).unwrap();
    deadpan_jobs::read_host_message(&mut Cursor::new(wire))
}

fn read_worker(value: &Value) -> Result<Option<WorkerMessage>, CodecError> {
    let mut wire = Vec::new();
    deadpan_jobs::write_frame(&mut wire, value).unwrap();
    deadpan_jobs::read_worker_message(&mut Cursor::new(wire))
}

#[test]
fn extension_v3_shared_fixtures_round_trip_fragmented_wire_and_both_directions() {
    for (direction, conditioning) in [
        ("from_left", "extend_from_left"),
        ("from_right", "extend_from_right"),
    ] {
        let mut value = serde_json::to_value(extension_request()).unwrap();
        value["plan"]["sampling"]["direction"] = json!(direction);
        value["constraints"]["conditioning"] = json!(conditioning);
        let request = read_host(&value).unwrap().unwrap();
        assert_eq!(request.protocol(), ProtocolVersion::V3);
        assert_eq!(request.identity(), &identity());
        assert_eq!(serde_json::to_value(&request).unwrap(), value);
        let mut encoded = Vec::new();
        deadpan_jobs::write_host_message(&mut encoded, &request).unwrap();
        let mut reader = Fragmented {
            inner: Cursor::new(encoded),
            maximum: 1,
        };
        assert_eq!(
            deadpan_jobs::read_host_message(&mut reader).unwrap(),
            Some(request)
        );
        assert_eq!(deadpan_jobs::read_host_message(&mut reader).unwrap(), None);
    }
    let completed = extension_completed();
    let mut encoded = Vec::new();
    deadpan_jobs::write_worker_message(&mut encoded, &completed).unwrap();
    let mut reader = Fragmented {
        inner: Cursor::new(encoded),
        maximum: 1,
    };
    assert_eq!(
        deadpan_jobs::read_worker_message(&mut reader).unwrap(),
        Some(completed)
    );
    assert_eq!(EXTENSION_PROTOCOL_VERSION, 3);
    assert_eq!(serde_json::to_value(ProtocolVersion::V3).unwrap(), 3);
    assert!(serde_json::from_str::<ProtocolVersion>("4").is_err());
}

#[test]
fn extension_v3_rejects_crossed_versions_operations_and_constraints() {
    let request = serde_json::to_value(extension_request()).unwrap();
    for protocol in [1, 2, 4] {
        let mut changed = request.clone();
        changed["protocol"] = json!(protocol);
        assert!(read_host(&changed).is_err());
    }
    for conditioning in ["bridge", "extend_from_right"] {
        let mut changed = request.clone();
        changed["constraints"]["conditioning"] = json!(conditioning);
        assert!(matches!(
            read_host(&changed),
            Err(CodecError::InvalidMessage(
                ValueError::ExtensionPlanMismatch
            ))
        ));
    }
    for (field, value) in [
        ("frames", json!(11)),
        ("width", json!(768)),
        ("height", json!(512)),
        ("frame_rate", json!({"numerator":30,"denominator":1})),
    ] {
        let mut changed = request.clone();
        changed["constraints"]["video"][field] = value;
        assert!(
            matches!(
                read_host(&changed),
                Err(CodecError::InvalidMessage(
                    ValueError::ExtensionPlanMismatch
                ))
            ),
            "{field}"
        );
    }
    let completed = serde_json::to_value(extension_completed()).unwrap();
    for protocol in [1, 2, 4] {
        let mut changed = completed.clone();
        changed["protocol"] = json!(protocol);
        assert!(read_worker(&changed).is_err());
    }
    let mut bridge = serde_json::to_value(bridge_request()).unwrap();
    bridge["protocol"] = json!(3);
    assert!(read_host(&bridge).is_err());
    let mut legacy = serde_json::to_value(super::request()).unwrap();
    legacy["protocol"] = json!(3);
    assert!(read_host(&legacy).is_err());
    for event in ["completed", "completed_bridge"] {
        let mut changed = completed.clone();
        changed["event"] = json!(event);
        assert!(read_worker(&changed).is_err());
    }
}

#[test]
fn extension_v3_wire_revalidates_plan_unknown_fields_and_native_references() {
    let request = serde_json::to_value(extension_request()).unwrap();
    for (field, value) in [
        ("context_frame_count", json!(2)),
        ("generated_frame_count", json!(7)),
        ("output_frame_count", json!(0)),
        ("direction", json!("automatic")),
        ("policy", json!("interior_only")),
        ("schema_version", json!(2)),
        ("generated_start", json!(9)),
    ] {
        let mut changed = request.clone();
        changed["plan"]["sampling"][field] = value;
        assert!(read_host(&changed).is_err(), "{field}");
    }
    for pointer in [
        "",
        "/plan",
        "/plan/native_dimensions",
        "/constraints",
        "/provider",
    ] {
        let mut changed = request.clone();
        changed.pointer_mut(pointer).unwrap()["unknown"] = json!(true);
        assert!(read_host(&changed).is_err(), "{pointer}");
    }
    let mut changed = serde_json::to_value(extension_completed()).unwrap();
    changed["candidate"]["provenance"]["reference"] =
        changed["candidate"]["native"]["reference"].clone();
    assert!(read_worker(&changed).is_err());
    let wire = serde_json::to_string(&extension_request()).unwrap();
    let duplicate = wire.replacen('{', "{\"protocol\":3,", 1);
    assert!(serde_json::from_str::<HostMessage>(&duplicate).is_err());
}

#[test]
fn extension_v3_stage_progress_failure_and_cancel_keep_version_and_identity() {
    let messages = [
        WorkerMessage::Stage {
            protocol: ProtocolVersion::V3,
            identity: identity(),
            stage: WorkerStage::Inference,
        },
        WorkerMessage::Progress {
            protocol: ProtocolVersion::V3,
            identity: identity(),
            stage: WorkerStage::Inference,
            progress: StageProgress::new(1, 4).unwrap(),
        },
        WorkerMessage::Failed {
            protocol: ProtocolVersion::V3,
            identity: identity(),
            failure: deadpan_jobs::WorkerFailure {
                code: deadpan_jobs::FailureCode::UnsupportedRequest,
                detail: deadpan_jobs::Diagnostic::new("context unavailable").unwrap(),
            },
        },
        WorkerMessage::Cancelled {
            protocol: ProtocolVersion::V3,
            identity: identity(),
        },
    ];
    for message in messages {
        assert_eq!(
            read_worker(&serde_json::to_value(&message).unwrap()).unwrap(),
            Some(message)
        );
    }
    let cancel = HostMessage::Cancel {
        protocol: ProtocolVersion::V3,
        identity: identity(),
        cancellation_token: CancellationToken::new("cancel-17-attempt-2").unwrap(),
    };
    assert_eq!(
        read_host(&serde_json::to_value(&cancel).unwrap()).unwrap(),
        Some(cancel)
    );
}
