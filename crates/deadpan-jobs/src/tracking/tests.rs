use super::*;
use crate::protocol::Sha256;

const SOURCE: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const OUTPUT: &str = "2222222222222222222222222222222222222222222222222222222222222222";

fn track() -> HostMessage {
    HostMessage::Track {
        protocol: VERSION,
        request: RequestId::new("track-1").unwrap(),
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
        start_pts: 0,
        end_pts: 1_250,
        pictures: 30,
        stride: 2,
        region: NormalizedRect::new(0.1, 0.2, 0.1, 0.2).unwrap(),
        output_scope: WorkspaceRef::new("output").unwrap(),
        maximum_output_bytes: 1024 * 1024,
        timeout_millis: 60_000,
    }
}

fn with(change: impl FnOnce(&mut HostMessage)) -> HostMessage {
    let mut message = track();
    change(&mut message);
    message
}

fn completed(reference: &str, decoded: u32, analysed: u32) -> WorkerMessage {
    WorkerMessage::Completed {
        protocol: VERSION,
        request: RequestId::new("track-1").unwrap(),
        attempt: AttemptId::new("attempt-1").unwrap(),
        observations: WorkspaceArtifact::new(
            WorkspaceRef::new(reference).unwrap(),
            Sha256::new(OUTPUT).unwrap(),
            100,
        )
        .unwrap(),
        runtime: RuntimeReport {
            engine: "Apple Vision VNTrackObjectRequest".into(),
            request_revision: 2,
            tracking_level: "accurate".into(),
        },
        decoded,
        analysed,
        decode_millis: 1,
        vision_millis: 2,
        elapsed_millis: 3,
    }
}

#[test]
fn requests_bound_source_range_stride_scope_and_timeout() {
    assert!(track().validate().is_ok());
    let invalid = [
        with(|m| {
            if let HostMessage::Track { source, .. } = m {
                *source = WorkspaceArtifact::new(
                    WorkspaceRef::new("input/nested/source").unwrap(),
                    Sha256::new(SOURCE).unwrap(),
                    1,
                )
                .unwrap();
            }
        }),
        with(|m| {
            if let HostMessage::Track { end_pts, .. } = m {
                *end_pts = 0;
            }
        }),
        with(|m| {
            if let HostMessage::Track { stride, .. } = m {
                *stride = 0;
            }
        }),
        with(|m| {
            if let HostMessage::Track { pictures, .. } = m {
                *pictures = MAX_TRACK_PICTURES as u32 + 1;
            }
        }),
        with(|m| {
            if let HostMessage::Track { stream, .. } = m {
                stream.rotation_quarter_turns = 4;
            }
        }),
        with(|m| {
            if let HostMessage::Track { output_scope, .. } = m {
                *output_scope = WorkspaceRef::new("input/out").unwrap();
            }
        }),
        with(|m| {
            if let HostMessage::Track { timeout_millis, .. } = m {
                *timeout_millis = 0;
            }
        }),
        with(|m| {
            if let HostMessage::Track { protocol, .. } = m {
                *protocol = 2;
            }
        }),
    ];
    for message in invalid {
        assert!(message.validate().is_err(), "{message:?}");
    }
    let json = serde_json::to_value(track()).unwrap();
    let mut outside = json.clone();
    outside["region"]["x"] = 0.95.into();
    assert!(serde_json::from_value::<HostMessage>(outside).is_err());
    let mut unknown = json;
    unknown["extra"] = 1.into();
    assert!(serde_json::from_value::<HostMessage>(unknown).is_err());
}

#[test]
fn completions_must_match_the_attempt_scope_and_picture_counts() {
    let protocol = TrackingProtocol::from_request(&track()).unwrap();
    assert_eq!(
        protocol
            .classify(&completed("output/observations.json", 30, 15))
            .unwrap(),
        ResponseKind::Completed
    );
    assert!(
        protocol
            .classify(&completed("elsewhere/observations.json", 30, 15))
            .is_err()
    );
    assert!(
        protocol
            .classify(&completed("output/observations.json", 29, 15))
            .is_err()
    );
    assert!(
        protocol
            .classify(&completed("output/observations.json", 30, 31))
            .is_err()
    );
    let foreign = WorkerMessage::Cancelled {
        protocol: VERSION,
        request: RequestId::new("track-2").unwrap(),
        attempt: AttemptId::new("attempt-1").unwrap(),
    };
    assert!(protocol.classify(&foreign).is_err());
    assert!(matches!(
        protocol.cancellation(),
        HostMessage::Cancel { .. }
    ));
    assert!(TrackingProtocol::from_request(&protocol.cancellation()).is_err());
}

#[test]
fn adversarial_tracking_frames() {
    let cancel = TrackingProtocol::from_request(&track())
        .unwrap()
        .cancellation();
    crate::adversarial::protocol::<TrackingProtocol>(
        "jobs-tracking-protocol",
        vec![track(), cancel],
        vec![completed("output/observations.json", 30, 15)],
        |reader| read_host(reader),
    );
}
