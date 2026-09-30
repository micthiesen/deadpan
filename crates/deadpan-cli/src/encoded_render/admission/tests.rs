use super::*;
use deadpan_encode::{BFramePolicy, EncoderMode};
use deadpan_jobs::{AttemptId, CancellationToken, RequestId, process::WorkerProtocol};
use protocol::{HostMessage, PROTOCOL_VERSION, ProbeProtocol};

fn spec() -> ProbeSpec {
    ProbeSpec {
        raster: [320, 180],
        frame_rate: [30_000, 1001],
        choice: EncoderChoice {
            mode: EncoderMode::Hardware,
            b_frames: BFramePolicy::TargetTwo,
        },
    }
}
fn request() -> HostMessage {
    HostMessage::Probe {
        protocol: PROTOCOL_VERSION,
        identity: RenderIdentity {
            request_id: RequestId::new("job").unwrap(),
            attempt_id: AttemptId::new("probe").unwrap(),
        },
        cancellation_token: CancellationToken::new("cancel").unwrap(),
        spec: spec(),
        limits: AdmissionLimits::default().encode,
        timeout_millis: 120_000,
    }
}

#[test]
fn probe_contract_has_its_own_exact_recipe_clock_and_identity() {
    let spec = spec();
    let contract = spec.contract().unwrap();
    let native = contract.native_contract().unwrap();
    assert_eq!(native.video_frames(), 46);
    assert_eq!(native.audio_samples(), 73_674);
    assert_eq!(contract.picture.range.start(), ProjectFrame(0));
    assert_eq!(contract.picture.project_audio_end, AudioSample(73_674));
    assert_eq!(contract.picture.terminal_pts, 46_046);
    assert_ne!(
        spec.document_sha256().unwrap(),
        ProbeSpec {
            frame_rate: [60, 1],
            ..spec.clone()
        }
        .document_sha256()
        .unwrap()
    );
    assert_eq!(
        spec.document_sha256().unwrap(),
        ProbeSpec {
            choice: EncoderChoice {
                mode: EncoderMode::Software,
                b_frames: BFramePolicy::None
            },
            ..spec
        }
        .document_sha256()
        .unwrap()
    );
}

#[test]
fn probe_rejects_ambiguous_picture_geometry_before_starting_a_worker() {
    for raster in [[2, 2], [12, 48], [14, 14], [15, 48], [0, 180]] {
        assert!(
            ProbeSpec { raster, ..spec() }.contract().is_err(),
            "{raster:?}"
        );
    }
    assert!(
        ProbeSpec {
            raster: [14, 16],
            ..spec()
        }
        .contract()
        .is_ok()
    );
    assert!(
        ProbeSpec {
            frame_rate: [1, 10],
            ..spec()
        }
        .contract()
        .is_err()
    );
}

#[test]
fn wire_admission_rejects_loose_fields_versions_and_work_before_execution() {
    use serde_json::json;
    let original = serde_json::to_value(request()).unwrap();
    for (path, value) in [
        ("/protocol", json!(0)),
        ("/protocol", json!(2)),
        ("/timeout_millis", json!(0)),
        ("/timeout_millis", json!(120_001)),
        ("/spec/frame_rate", json!([120, 1])),
        ("/spec/raster", json!([2, 2])),
        ("/limits/maximum_output_bytes", json!(MAX_PROBE_BYTES + 1)),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(path).unwrap() = value;
        let parsed = serde_json::from_value::<HostMessage>(changed);
        assert!(
            parsed.is_err() || parsed.unwrap().validate().is_err(),
            "{path}"
        );
    }
    for path in ["", "/spec", "/spec/choice", "/limits"] {
        let mut changed = original.clone();
        changed
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("trust_me".into(), json!(true));
        assert!(
            serde_json::from_value::<HostMessage>(changed).is_err(),
            "{path}"
        );
    }
    let protocol = ProbeProtocol::from_request(&request()).unwrap();
    let HostMessage::Probe { identity, .. } = request() else {
        unreachable!()
    };
    let valid = protocol::WorkerMessage::Progress {
        protocol: PROTOCOL_VERSION,
        identity: identity.clone(),
        completed_frames: 0,
        total_frames: 46,
    };
    assert!(protocol.classify(&valid).is_ok());
    for changed in [
        protocol::WorkerMessage::Progress {
            protocol: PROTOCOL_VERSION,
            identity: identity.clone(),
            completed_frames: 47,
            total_frames: 46,
        },
        protocol::WorkerMessage::Progress {
            protocol: PROTOCOL_VERSION,
            identity: RenderIdentity {
                attempt_id: AttemptId::new("stale").unwrap(),
                ..identity
            },
            completed_frames: 0,
            total_frames: 46,
        },
    ] {
        assert!(protocol.classify(&changed).is_err());
    }
}
