use std::io::{self, Cursor, Read};

use deadpan_core::{FrameDuration, FrameRate, NodeId, ProjectId, RevisionId};
use deadpan_jobs::{
    AttemptId, AxisLimits, BRIDGE_PROTOCOL_VERSION, BridgeCapability, BridgeGenerationPlan,
    CancellationToken, CandidateManifest, CodecError, ConditioningMode, ContextArtifact,
    DimensionLimits, FrameCountFormula, HoldConstraints, HoldTarget, HostMessage, MAX_FRAME_BYTES,
    MessageIdentity, MotionAmount, NativeCandidateManifest, NativeDimensions, PROTOCOL_VERSION,
    ProtocolVersion, ProviderPackId, ProviderPackVersion, ProviderSelection, RequestId,
    RequestVersion, RuntimeId, RuntimeVersion, Sha256, StageProgress, VideoSpec, WorkerMessage,
    WorkerStage, WorkspaceArtifact, WorkspaceRef,
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
            instructions: None,
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

fn bridge_plan() -> BridgeGenerationPlan {
    BridgeGenerationPlan::new(
        FrameDuration::new(3).unwrap(),
        FrameRate::new(30, 1).unwrap(),
        &BridgeCapability::new(
            true,
            FrameRate::new(24, 1).unwrap(),
            FrameCountFormula::new(1, 0, 2, 97).unwrap(),
            DimensionLimits::new(
                AxisLimits::new(512, 512, 1).unwrap(),
                AxisLimits::new(320, 320, 1).unwrap(),
            ),
        ),
        NativeDimensions::new(512, 320).unwrap(),
    )
    .unwrap()
}

fn bridge_request() -> HostMessage {
    HostMessage::GenerateBridge {
        protocol: ProtocolVersion::V2,
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
            video: VideoSpec::new(
                FrameDuration::new(3).unwrap(),
                FrameRate::new(30, 1).unwrap(),
                512,
                320,
            )
            .unwrap(),
            conditioning: ConditioningMode::Bridge,
            motion: MotionAmount::Subtle,
            instructions: None,
        },
        provider: Box::new(provider()),
        plan: Box::new(bridge_plan()),
    }
}

fn native_candidate() -> NativeCandidateManifest {
    NativeCandidateManifest {
        native: WorkspaceArtifact::new(
            WorkspaceRef::new("outputs/native.mp4").unwrap(),
            sha('c'),
            101,
        )
        .unwrap(),
        provenance: WorkspaceArtifact::new(
            WorkspaceRef::new("outputs/provenance.json").unwrap(),
            sha('d'),
            202,
        )
        .unwrap(),
        video: VideoSpec::new(
            FrameDuration::new(4).unwrap(),
            FrameRate::new(24, 1).unwrap(),
            512,
            320,
        )
        .unwrap(),
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
        r#"{"operation":"cancel","protocol":3,"identity":{"request_id":"job","attempt_id":"attempt"},"cancellation_token":"token"}"#,
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

#[test]
fn bridge_v2_round_trips_strict_plan_and_native_declaration() {
    let request = bridge_request();
    request.validate().unwrap();
    assert_eq!(request.protocol(), ProtocolVersion::V2);
    let mut encoded = Vec::new();
    deadpan_jobs::write_host_message(&mut encoded, &request).unwrap();
    assert_eq!(
        deadpan_jobs::read_host_message(&mut Cursor::new(encoded)).unwrap(),
        Some(request)
    );

    let completed = WorkerMessage::CompletedBridge {
        protocol: ProtocolVersion::V2,
        identity: identity(),
        candidate: native_candidate(),
    };
    completed.validate().unwrap();
    let mut encoded = Vec::new();
    deadpan_jobs::write_worker_message(&mut encoded, &completed).unwrap();
    assert_eq!(
        deadpan_jobs::read_worker_message(&mut Cursor::new(encoded)).unwrap(),
        Some(completed)
    );
    assert_eq!(BRIDGE_PROTOCOL_VERSION, 2);
    assert!(serde_json::from_str::<ProtocolVersion>("3").is_err());
}

#[test]
fn rejects_protocol_operation_crossovers_and_plan_mismatch() {
    let mut legacy = request();
    if let HostMessage::GenerateHold { protocol, .. } = &mut legacy {
        *protocol = ProtocolVersion::V2;
    }
    assert!(legacy.validate().is_err());
    assert!(deadpan_jobs::write_host_message(&mut Vec::new(), &legacy).is_err());

    let mut bridge = bridge_request();
    if let HostMessage::GenerateBridge { protocol, .. } = &mut bridge {
        *protocol = ProtocolVersion::V1;
    }
    assert!(bridge.validate().is_err());

    let mut mismatch = bridge_request();
    if let HostMessage::GenerateBridge { constraints, .. } = &mut mismatch {
        constraints.video = VideoSpec::new(
            FrameDuration::new(4).unwrap(),
            FrameRate::new(30, 1).unwrap(),
            512,
            320,
        )
        .unwrap();
    }
    assert!(mismatch.validate().is_err());

    let crossed_legacy = WorkerMessage::Completed {
        protocol: ProtocolVersion::V2,
        identity: identity(),
        candidate: candidate(),
    };
    let crossed_bridge = WorkerMessage::CompletedBridge {
        protocol: ProtocolVersion::V1,
        identity: identity(),
        candidate: native_candidate(),
    };
    assert!(crossed_legacy.validate().is_err());
    assert!(crossed_bridge.validate().is_err());
}

#[test]
fn native_manifest_rejects_duplicate_references_and_unknown_fields() {
    let candidate = native_candidate();
    let mut duplicate = serde_json::to_value(&candidate).unwrap();
    duplicate["provenance"]["reference"] = duplicate["native"]["reference"].clone();
    assert!(serde_json::from_value::<NativeCandidateManifest>(duplicate).is_err());

    let mut unknown = serde_json::to_value(candidate).unwrap();
    unknown["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<NativeCandidateManifest>(unknown).is_err());
}

#[test]
fn legacy_v1_wire_shape_is_unchanged() {
    let wire = serde_json::to_string(&request()).unwrap();
    let expected = [
        r#"{"operation":"generate_hold","protocol":1,"identity":{"request_id":"job-17","attempt_id":"attempt-2"},"cancellation_token":"cancel-17-attempt-2","project_id":"project-1","revision_id":"revision-9","target":{"hold_id":"hold-4","request_version":3},"input":{"manifest":"inputs/context.json","sha256":""#,
        &"a".repeat(64),
        r#""},"output_workspace":"work/job-17/attempt-2","constraints":{"video":{"frames":45,"frame_rate":{"numerator":30000,"denominator":1001},"width":512,"height":320},"conditioning":"bridge","motion":"subtle"},"provider":{"pack_id":"qualified-local-pack","pack_version":"2026.09","runtime_id":"mlx","runtime_version":"1.2.3+deadpan","seed":38117}}"#,
    ]
    .concat();
    assert_eq!(wire, expected);

    let completed = WorkerMessage::Completed {
        protocol: ProtocolVersion::V1,
        identity: identity(),
        candidate: candidate(),
    };
    let wire = serde_json::to_string(&completed).unwrap();
    let expected = [
        r#"{"event":"completed","protocol":1,"identity":{"request_id":"job-17","attempt_id":"attempt-2"},"candidate":{"media":{"reference":"outputs/candidate.mov","sha256":""#,
        &"b".repeat(64),
        r#"","byte_length":91337},"video":{"frames":45,"frame_rate":{"numerator":30000,"denominator":1001},"width":512,"height":320},"provider":{"pack_id":"qualified-local-pack","pack_version":"2026.09","runtime_id":"mlx","runtime_version":"1.2.3+deadpan","seed":38117}}}"#,
    ]
    .concat();
    assert_eq!(wire, expected);
}

/// Gate G: mutated generation frame streams read by both sides end in a
/// typed `CodecError` or validated messages, never a panic. Selector 0 reads
/// host messages, 1 reads worker messages, 2 classifies responses against the
/// preceding request with the shared stable/nightly exercise.
#[test]
fn adversarial_generation_frames() {
    use deadpan_chaos::{Target, fuzz, protocol_stream};
    use deadpan_jobs::process::WorkerProtocol;
    use deadpan_jobs::supervisor::GenerationProtocol;
    fn frames(selector: u8, messages: &[serde_json::Value]) -> Vec<u8> {
        let mut bytes = vec![selector];
        for message in messages {
            bytes.extend(deadpan_chaos::frame(&serde_json::to_vec(message).unwrap()));
        }
        bytes
    }
    let hosts = [
        serde_json::to_value(request()).unwrap(),
        serde_json::to_value(bridge_request()).unwrap(),
    ];
    let workers = [
        serde_json::to_value(WorkerMessage::Completed {
            protocol: ProtocolVersion::V1,
            identity: identity(),
            candidate: candidate(),
        })
        .unwrap(),
        serde_json::to_value(WorkerMessage::CompletedBridge {
            protocol: ProtocolVersion::V2,
            identity: identity(),
            candidate: native_candidate(),
        })
        .unwrap(),
        serde_json::to_value(WorkerMessage::Progress {
            protocol: ProtocolVersion::V1,
            identity: identity(),
            stage: WorkerStage::Inference,
            progress: StageProgress::new(7, 23).unwrap(),
        })
        .unwrap(),
    ];
    let mut seeds = vec![frames(0, &hosts), frames(1, &workers)];
    seeds.extend(
        hosts
            .iter()
            .map(|host| frames(0, std::slice::from_ref(host))),
    );
    seeds.extend(
        workers
            .iter()
            .map(|worker| frames(1, std::slice::from_ref(worker))),
    );
    for host in &hosts {
        for worker in &workers {
            seeds.push(frames(2, &[host.clone(), worker.clone()]));
        }
    }
    let exercise = |input: &[u8], calls: &mut usize| {
        protocol_stream(
            input,
            |reader| deadpan_jobs::read_host_message(reader).map_err(|error| error.to_string()),
            GenerationProtocol::write_request,
            |reader| GenerationProtocol::read_response(reader),
            |request| GenerationProtocol::from_request(request).map_err(|error| error.to_string()),
            |protocol, response| {
                *calls += 1;
                protocol.classify(response).map(|_| ())
            },
        )
    };
    let mut calls = 0;
    exercise(
        &frames(2, &[hosts[0].clone(), workers[0].clone()]),
        &mut calls,
    )
    .expect("valid generation request/response must reach classification");
    assert_eq!(calls, 1);
    let report = fuzz(Target::frames("jobs-generation-protocol"), seeds, |input| {
        exercise(input, &mut 0)
    });
    report.assert_clean();
}

#[test]
fn attempt_providers_are_seeded_variants_below_the_worker_seed_limit() {
    let base = provider();
    assert_eq!(base.for_attempt(1), base);
    assert_eq!(base.for_attempt(0), base);
    let second = base.for_attempt(2);
    assert_eq!(second.seed, base.seed + 1);
    assert_eq!(
        ProviderSelection {
            seed: base.seed,
            ..second.clone()
        },
        base,
        "only the seed varies"
    );
    let high = ProviderSelection {
        seed: (1 << 32) - 1,
        ..base.clone()
    };
    assert_eq!(high.for_attempt(1).seed, (1 << 32) - 1);
    assert_eq!(high.for_attempt(2).seed, 0);
    assert_eq!(
        high.for_attempt(u64::MAX).seed,
        ((1_u64 << 32) - 1 + (u64::MAX - 1) % (1 << 32)) % (1 << 32)
    );
    for ordinal in [2, 3, 1_000, u64::MAX] {
        assert!(base.for_attempt(ordinal).seed < 1 << 32);
    }
}
