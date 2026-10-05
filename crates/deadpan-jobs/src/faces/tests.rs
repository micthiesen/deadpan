use super::*;
use crate::protocol::{Sha256, WorkspaceRef};

const SOURCE: &str = "1111111111111111111111111111111111111111111111111111111111111111";

fn detect() -> HostMessage {
    HostMessage::DetectFaces {
        protocol: VERSION,
        request: RequestId::new("faces-1").unwrap(),
        attempt: AttemptId::new("attempt-1").unwrap(),
        cancellation_token: CancellationToken::new("cancel-1").unwrap(),
        source: WorkspaceArtifact::new(
            WorkspaceRef::new("input/source").unwrap(),
            Sha256::new(SOURCE).unwrap(),
            49_000,
        )
        .unwrap(),
        stream: ExpectedStream {
            stream_index: 0,
            width: 320,
            height: 180,
            time_base_num: 1,
            time_base_den: 1_000,
            rotation_quarter_turns: 0,
        },
        pts: 1_000,
        timeout_millis: 60_000,
    }
}

fn face(x: f64, y: f64, confidence: f32) -> DetectedFace {
    DetectedFace {
        region: NormalizedRect::new(x, y, 0.1, 0.2).unwrap(),
        confidence,
    }
}

fn completed(pts: i64, faces: Vec<DetectedFace>) -> WorkerMessage {
    WorkerMessage::Completed {
        protocol: VERSION,
        request: RequestId::new("faces-1").unwrap(),
        attempt: AttemptId::new("attempt-1").unwrap(),
        pts,
        faces,
        runtime: RuntimeReport {
            engine: "Apple Vision VNDetectFaceRectanglesRequest".into(),
            request_revision: 3,
        },
        decode_millis: 1,
        vision_millis: 2,
        elapsed_millis: 3,
    }
}

#[test]
fn requests_bound_source_stream_and_timeout() {
    assert!(detect().validate().is_ok());
    let mut nested = detect();
    if let HostMessage::DetectFaces { source, .. } = &mut nested {
        *source = WorkspaceArtifact::new(
            WorkspaceRef::new("input/nested/source").unwrap(),
            Sha256::new(SOURCE).unwrap(),
            1,
        )
        .unwrap();
    }
    let mut rotated = detect();
    if let HostMessage::DetectFaces { stream, .. } = &mut rotated {
        stream.rotation_quarter_turns = 4;
    }
    let mut unbounded = detect();
    if let HostMessage::DetectFaces { timeout_millis, .. } = &mut unbounded {
        *timeout_millis = 0;
    }
    let mut future = detect();
    if let HostMessage::DetectFaces { protocol, .. } = &mut future {
        *protocol = 2;
    }
    for message in [nested, rotated, unbounded, future] {
        assert!(message.validate().is_err(), "{message:?}");
    }
    let mut unknown = serde_json::to_value(detect()).unwrap();
    unknown["extra"] = 1.into();
    assert!(serde_json::from_value::<HostMessage>(unknown).is_err());
    // The tracking protocol's message is not this protocol's.
    let mut other = serde_json::to_value(detect()).unwrap();
    other["op"] = "track".into();
    assert!(serde_json::from_value::<HostMessage>(other).is_err());
}

#[test]
fn completions_must_name_the_attempt_and_picture_with_ordered_bounded_faces() {
    let protocol = FaceProtocol::from_request(&detect()).unwrap();
    let ordered = vec![
        face(0.1, 0.5, 0.9),
        face(0.1, 0.6, 0.4),
        face(0.5, 0.1, 0.8),
    ];
    assert_eq!(
        protocol
            .classify(&completed(1_000, ordered.clone()))
            .unwrap(),
        ResponseKind::Completed
    );
    assert_eq!(
        protocol.classify(&completed(1_000, Vec::new())).unwrap(),
        ResponseKind::Completed
    );
    // Another picture, unordered, duplicate, out-of-range confidence, too many.
    assert!(protocol.classify(&completed(999, ordered.clone())).is_err());
    let mut reversed = ordered.clone();
    reversed.reverse();
    assert!(protocol.classify(&completed(1_000, reversed)).is_err());
    let duplicate = vec![face(0.1, 0.5, 0.9), face(0.1, 0.5, 0.9)];
    assert!(protocol.classify(&completed(1_000, duplicate)).is_err());
    for confidence in [f32::NAN, -0.1, 1.5] {
        assert!(
            protocol
                .classify(&completed(1_000, vec![face(0.1, 0.1, confidence)]))
                .is_err()
        );
    }
    let many = (0..=MAX_FACES)
        .map(|index| face(index as f64 * 0.001, 0.1, 0.5))
        .collect();
    assert!(protocol.classify(&completed(1_000, many)).is_err());
    let foreign = WorkerMessage::Cancelled {
        protocol: VERSION,
        request: RequestId::new("faces-2").unwrap(),
        attempt: AttemptId::new("attempt-1").unwrap(),
    };
    assert!(protocol.classify(&foreign).is_err());
    // Regions outside the picture never deserialize.
    let mut outside = serde_json::to_value(completed(1_000, ordered)).unwrap();
    outside["faces"][0]["region"]["x"] = 0.95.into();
    assert!(serde_json::from_value::<WorkerMessage>(outside).is_err());
}

#[test]
fn the_face_order_is_left_then_top_then_size() {
    let mut faces = vec![
        face(0.6, 0.1, 0.9),
        face(0.2, 0.7, 0.5),
        face(0.2, 0.3, 0.7),
    ];
    faces.sort_by(DetectedFace::order);
    assert!(validate_faces(&faces).is_ok());
    assert_eq!(
        faces
            .iter()
            .map(|face| (face.region.x(), face.region.y()))
            .collect::<Vec<_>>(),
        [(0.2, 0.3), (0.2, 0.7), (0.6, 0.1)]
    );
}
