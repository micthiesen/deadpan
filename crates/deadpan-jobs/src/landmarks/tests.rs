use super::*;
use crate::protocol::Sha256;

fn artifact(path: &str, bytes: u64) -> WorkspaceArtifact {
    WorkspaceArtifact::new(
        WorkspaceRef::new(path).unwrap(),
        Sha256::new("1".repeat(64)).unwrap(),
        bytes,
    )
    .unwrap()
}

fn request() -> HostMessage {
    HostMessage::InspectLandmarks {
        protocol: VERSION,
        request: RequestId::new("landmarks-1").unwrap(),
        attempt: AttemptId::new("attempt-1").unwrap(),
        cancellation_token: CancellationToken::new("cancel-1").unwrap(),
        source: artifact("input/source.mkv", 16_000),
        stream: ExpectedStream {
            stream_index: 0,
            width: 768,
            height: 320,
            time_base_num: 1,
            time_base_den: 1000,
            rotation_quarter_turns: 0,
        },
        picture_pts: vec![0, 42, 83, 125],
        boundaries: Some(Box::new(BoundaryInputs {
            left: artifact("input/left.png", 800),
            right: artifact("input/right.png", 900),
        })),
        output_scope: WorkspaceRef::new("output").unwrap(),
        maximum_output_bytes: 16_384,
        timeout_millis: 60_000,
    }
}

fn completed() -> WorkerMessage {
    WorkerMessage::Completed {
        protocol: VERSION,
        request: RequestId::new("landmarks-1").unwrap(),
        attempt: AttemptId::new("attempt-1").unwrap(),
        observations: artifact("output/landmarks.json", 8_192),
        runtime: RuntimeReport {
            engine: ENGINE.into(),
            request_revision: REQUEST_REVISION,
            constellation: CONSTELLATION,
        },
        decoded: 4,
        analysed: 6,
        decode_millis: 2,
        vision_millis: 8,
        elapsed_millis: 10,
    }
}

#[test]
fn request_requires_a_complete_bounded_ordered_sequence() {
    assert_eq!(
        MAX_FRAMES,
        deadpan_analysis::generated_geometry::MAX_NATIVE_FRAMES
    );
    request().validate().unwrap();
    for points in [
        vec![],
        vec![0],
        vec![1, 1],
        vec![2, 1],
        vec![0; MAX_FRAMES + 1],
    ] {
        let mut value = request();
        if let HostMessage::InspectLandmarks { picture_pts, .. } = &mut value {
            *picture_pts = points;
        }
        assert!(value.validate().is_err());
    }
    let mut maximum = request();
    if let HostMessage::InspectLandmarks { picture_pts, .. } = &mut maximum {
        *picture_pts = (0..MAX_FRAMES as i64).collect();
    }
    maximum.validate().unwrap();
    let mut framed = Vec::new();
    LandmarkProtocol::write_request(&mut framed, &maximum).unwrap();
    assert_eq!(read_host(&mut framed.as_slice()).unwrap(), Some(maximum));
}

#[test]
fn inputs_outputs_and_deadlines_cannot_escape_their_bounds() {
    for (field, value) in [
        (
            "source",
            serde_json::to_value(artifact("input/nested/source", 1)).unwrap(),
        ),
        (
            "source",
            serde_json::to_value(artifact("input/source", MAX_SOURCE_BYTES + 1)).unwrap(),
        ),
        ("output_scope", serde_json::json!("input")),
        ("output_scope", serde_json::json!("output/nested")),
        (
            "maximum_output_bytes",
            serde_json::json!(MAX_OBSERVATION_BYTES + 1),
        ),
        ("timeout_millis", serde_json::json!(0)),
        ("timeout_millis", serde_json::json!(MAX_TIMEOUT_MILLIS + 1)),
        ("protocol", serde_json::json!(2)),
    ] {
        let mut wire = serde_json::to_value(request()).unwrap();
        wire[field] = value;
        assert!(
            serde_json::from_value::<HostMessage>(wire)
                .unwrap()
                .validate()
                .is_err(),
            "{field}"
        );
    }
    let mut value = request();
    if let HostMessage::InspectLandmarks {
        stream, boundaries, ..
    } = &mut value
    {
        stream.width = MAX_DIMENSION + 1;
        boundaries.as_mut().unwrap().left = artifact("input/left.png", MAX_PNG_BYTES + 1);
    }
    assert!(value.validate().is_err());
    let mut alias = request();
    if let HostMessage::InspectLandmarks {
        source, boundaries, ..
    } = &mut alias
    {
        boundaries.as_mut().unwrap().left = source.clone();
    }
    assert!(alias.validate().is_err());
}

#[test]
fn strict_wire_rejects_unknown_fields_and_other_worker_modes() {
    for (field, value) in [
        ("extra", serde_json::json!(1)),
        ("op", serde_json::json!("detect_faces")),
    ] {
        let mut wire = serde_json::to_value(request()).unwrap();
        wire[field] = value;
        assert!(serde_json::from_value::<HostMessage>(wire).is_err());
    }
    let mut nested = serde_json::to_value(request()).unwrap();
    nested["boundaries"]["extra"] = true.into();
    assert!(serde_json::from_value::<HostMessage>(nested).is_err());
}

#[test]
fn completion_is_bound_to_attempt_exact_artifact_counts_and_pinned_runtime() {
    let protocol = LandmarkProtocol::from_request(&request()).unwrap();
    assert_eq!(
        protocol.classify(&completed()).unwrap(),
        ResponseKind::Completed
    );
    for (field, value) in [
        ("request", serde_json::json!("other")),
        ("attempt", serde_json::json!("other")),
        ("decoded", serde_json::json!(3)),
        ("analysed", serde_json::json!(4)),
        (
            "observations",
            serde_json::to_value(artifact("output/nested/landmarks.json", 10)).unwrap(),
        ),
        (
            "observations",
            serde_json::to_value(artifact("output/landmarks.json", 16_385)).unwrap(),
        ),
        ("vision_millis", serde_json::json!(11)),
    ] {
        let mut wire = serde_json::to_value(completed()).unwrap();
        wire[field] = value;
        assert!(
            protocol
                .classify(&serde_json::from_value::<WorkerMessage>(wire).unwrap())
                .is_err(),
            "{field}"
        );
    }
    for (field, value) in [
        ("request_revision", serde_json::json!(2)),
        ("constellation", serde_json::json!(65)),
        ("engine", serde_json::json!("different")),
    ] {
        let mut wire = serde_json::to_value(completed()).unwrap();
        wire["runtime"][field] = value;
        assert!(
            protocol
                .classify(&serde_json::from_value::<WorkerMessage>(wire).unwrap())
                .is_err()
        );
    }
    let cancel = protocol.cancellation();
    assert!(LandmarkProtocol::from_request(&cancel).is_err());
    let HostMessage::Cancel {
        protocol: version,
        request,
        attempt,
        ..
    } = cancel
    else {
        panic!()
    };
    assert_eq!(
        protocol
            .classify(&WorkerMessage::Cancelled {
                protocol: version,
                request,
                attempt
            })
            .unwrap(),
        ResponseKind::Terminal
    );
}

#[test]
fn optional_boundaries_change_the_exact_analysed_count() {
    let mut value = request();
    if let HostMessage::InspectLandmarks { boundaries, .. } = &mut value {
        *boundaries = None;
    }
    let protocol = LandmarkProtocol::from_request(&value).unwrap();
    assert!(protocol.classify(&completed()).is_err());
    let mut completion = completed();
    if let WorkerMessage::Completed { analysed, .. } = &mut completion {
        *analysed = 4;
    }
    assert_eq!(
        protocol.classify(&completion).unwrap(),
        ResponseKind::Completed
    );
}
