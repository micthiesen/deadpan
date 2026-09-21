use std::io::{self, Cursor, Read};

use deadpan_core::{FrameDuration, FrameRate, NodeId, ProjectId, RevisionId};
use deadpan_jobs::{
    AttemptId, CancellationToken, CandidateManifest, CodecError, ConditioningMode, ContextArtifact,
    HoldConstraints, HoldTarget, HostMessage, MAX_FRAME_BYTES, MessageIdentity, MotionAmount,
    PROTOCOL_VERSION, ProtocolVersion, ProviderPackId, ProviderPackVersion, ProviderSelection,
    RequestId, RequestVersion, RuntimeId, RuntimeVersion, Sha256, StageProgress, VideoSpec,
    WorkerMessage, WorkerStage, WorkspaceArtifact, WorkspaceRef,
};

fn sha(character: char) -> Sha256 {
    Sha256::new(character.to_string().repeat(64)).unwrap()
}

fn identity() -> MessageIdentity {
    MessageIdentity::new(
        RequestId::new("job-17").unwrap(),
        AttemptId::new("attempt-2").unwrap(),
    )
}

fn provider() -> ProviderSelection {
    ProviderSelection {
        pack_id: ProviderPackId::new("qualified-local-pack").unwrap(),
        pack_version: ProviderPackVersion::new("2026.09").unwrap(),
        runtime_id: RuntimeId::new("mlx").unwrap(),
        runtime_version: RuntimeVersion::new("1.2.3+deadpan").unwrap(),
        seed: 38_117,
    }
}

fn video() -> VideoSpec {
    VideoSpec::new(
        FrameDuration::new(45).unwrap(),
        FrameRate::new(30_000, 1_001).unwrap(),
        512,
        320,
    )
    .unwrap()
}

fn request() -> HostMessage {
    HostMessage::GenerateHold {
        protocol: ProtocolVersion::V1,
        identity: identity(),
        cancellation_token: CancellationToken::new("cancel-17-attempt-2").unwrap(),
        project_id: ProjectId::new("project-1").unwrap(),
        revision_id: RevisionId::new("revision-9").unwrap(),
        target: HoldTarget {
            hold_id: NodeId::new("hold-4").unwrap(),
            request_version: RequestVersion::new(3).unwrap(),
        },
        input: ContextArtifact {
            manifest: WorkspaceRef::new("inputs/context.json").unwrap(),
            sha256: sha('a'),
        },
        output_workspace: WorkspaceRef::new("work/job-17/attempt-2").unwrap(),
        constraints: HoldConstraints {
            video: video(),
            conditioning: ConditioningMode::Bridge,
            motion: MotionAmount::Subtle,
        },
        provider: Box::new(provider()),
    }
}

fn candidate() -> CandidateManifest {
    CandidateManifest {
        media: WorkspaceArtifact::new(
            WorkspaceRef::new("outputs/candidate.mov").unwrap(),
            sha('b'),
            91_337,
        )
        .unwrap(),
        video: video(),
        provider: provider(),
    }
}

struct Fragmented<R> {
    inner: R,
    maximum: usize,
}

impl<R: Read> Read for Fragmented<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let length = buffer.len().min(self.maximum);
        self.inner.read(&mut buffer[..length])
    }
}

#[test]
fn round_trips_fragmented_frames_and_clean_eof() {
    let message = WorkerMessage::Completed {
        protocol: ProtocolVersion::V1,
        identity: identity(),
        candidate: candidate(),
    };
    let mut encoded = Vec::new();
    deadpan_jobs::write_worker_message(&mut encoded, &message).unwrap();

    let mut reader = Fragmented {
        inner: Cursor::new(encoded),
        maximum: 1,
    };
    assert_eq!(
        deadpan_jobs::read_worker_message(&mut reader).unwrap(),
        Some(message)
    );
    assert_eq!(
        deadpan_jobs::read_worker_message(&mut reader).unwrap(),
        None
    );
}

#[test]
fn distinguishes_truncated_header_and_body() {
    let mut short_header = Cursor::new(vec![0, 0, 0]);
    assert!(matches!(
        deadpan_jobs::read_host_message(&mut short_header),
        Err(CodecError::TruncatedHeader { read: 3 })
    ));

    let mut short_body = Cursor::new([4_u32.to_be_bytes().as_slice(), b"{}"].concat());
    assert!(matches!(
        deadpan_jobs::read_host_message(&mut short_body),
        Err(CodecError::TruncatedBody {
            expected: 4,
            read: 2
        })
    ));
}

struct HeaderOnly {
    header: Option<[u8; 4]>,
}

impl Read for HeaderOnly {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let header = self
            .header
            .take()
            .expect("oversized frame must be rejected before reading a body");
        buffer.copy_from_slice(&header);
        Ok(header.len())
    }
}

#[test]
fn rejects_oversized_length_before_body_allocation_or_read() {
    let declared = u32::try_from(MAX_FRAME_BYTES + 1).unwrap();
    let mut reader = HeaderOnly {
        header: Some(declared.to_be_bytes()),
    };
    assert!(matches!(
        deadpan_jobs::read_worker_message(&mut reader),
        Err(CodecError::OversizedPayload { declared: value, max: MAX_FRAME_BYTES }) if value == declared
    ));
}

#[test]
fn rejects_malformed_unknown_and_wrong_protocol_payloads() {
    fn framed(json: &str) -> Vec<u8> {
        let mut bytes = u32::try_from(json.len()).unwrap().to_be_bytes().to_vec();
        bytes.extend_from_slice(json.as_bytes());
        bytes
    }

    for json in [
        "not json",
        r#"{"operation":"cancel","protocol":1,"identity":{"request_id":"job","attempt_id":"attempt"},"cancellation_token":"token","extra":true}"#,
        r#"{"operation":"cancel","protocol":2,"identity":{"request_id":"job","attempt_id":"attempt"},"cancellation_token":"token"}"#,
    ] {
        assert!(matches!(
            deadpan_jobs::read_host_message(&mut Cursor::new(framed(json))),
            Err(CodecError::MalformedPayload(_))
        ));
    }
}

#[test]
fn validates_paths_ids_hashes_dimensions_and_progress_during_deserialization() {
    for invalid in ["", "/absolute", "a//b", "a/./b", "a/../b", "a\\b"] {
        assert!(WorkspaceRef::new(invalid).is_err(), "accepted {invalid:?}");
    }
    assert!(RequestId::new("contains/slash").is_err());
    assert!(Sha256::new("A".repeat(64)).is_err());
    assert!(
        VideoSpec::new(
            FrameDuration::ZERO,
            FrameRate::new(30, 1).unwrap(),
            1920,
            1080
        )
        .is_err()
    );
    assert!(StageProgress::new(11, 10).is_err());
    assert!(
        WorkspaceArtifact::new(WorkspaceRef::new("outputs/empty.mov").unwrap(), sha('b'), 0)
            .is_err()
    );

    let json = serde_json::to_string(&request()).unwrap();
    let malicious = json.replace(
        r#""manifest":"inputs/context.json""#,
        r#""manifest":"inputs/../escape""#,
    );
    let mut bytes = u32::try_from(malicious.len())
        .unwrap()
        .to_be_bytes()
        .to_vec();
    bytes.extend_from_slice(malicious.as_bytes());
    assert!(matches!(
        deadpan_jobs::read_host_message(&mut Cursor::new(bytes)),
        Err(CodecError::MalformedPayload(_))
    ));

    let mut invalid_identity = serde_json::to_value(request()).unwrap();
    invalid_identity["identity"]["request_id"] = serde_json::json!("bad/request");
    assert!(serde_json::from_value::<HostMessage>(invalid_identity).is_err());

    let mut invalid_hash = serde_json::to_value(request()).unwrap();
    invalid_hash["input"]["sha256"] = serde_json::json!("A".repeat(64));
    assert!(serde_json::from_value::<HostMessage>(invalid_hash).is_err());

    let mut unknown_nested_field = serde_json::to_value(request()).unwrap();
    unknown_nested_field["constraints"]["video"]["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<HostMessage>(unknown_nested_field).is_err());
}

#[test]
fn request_round_trip_preserves_exact_typed_contract() {
    let request = request();
    let mut encoded = Vec::new();
    deadpan_jobs::write_host_message(&mut encoded, &request).unwrap();
    assert!(encoded.len() <= MAX_FRAME_BYTES + 4);
    assert_eq!(
        u32::from_be_bytes(encoded[..4].try_into().unwrap()) as usize,
        encoded.len() - 4
    );
    assert_eq!(
        deadpan_jobs::read_host_message(&mut Cursor::new(encoded)).unwrap(),
        Some(request)
    );
    assert_eq!(PROTOCOL_VERSION, 1);
}

#[test]
fn progress_is_stage_local_exact_units() {
    let progress = StageProgress::new(7, 23).unwrap();
    let message = WorkerMessage::Progress {
        protocol: ProtocolVersion::V1,
        identity: identity(),
        stage: WorkerStage::Inference,
        progress: progress.clone(),
    };
    let json = serde_json::to_value(message).unwrap();
    assert_eq!(json["progress"]["completed"], 7);
    assert_eq!(json["progress"]["total"], 23);
    assert!(json.get("percent").is_none());
}
