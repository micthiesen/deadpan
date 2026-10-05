use std::collections::BTreeMap;
use std::io::{self, Cursor};
use std::sync::atomic::AtomicBool;

use deadpan_core::{
    BeatNode, Command, CommandRequest, FrameDuration, HoldAudio, HoldRecipe, HoldVideo, NodeId,
    PresentationBasis, ProjectDocument, ProjectFrame, Subtree,
};
use deadpan_jobs::MAX_FRAME_BYTES;
use deadpan_store::ProjectStore;
use serde_json::{Value, json};

use crate::picture::ProjectPictureSession;

use super::*;

fn identity() -> RenderIdentity {
    RenderIdentity {
        request_id: RequestId::new("pictures-1").unwrap(),
        attempt_id: AttemptId::new("attempt-1").unwrap(),
    }
}

fn sha256(digit: char) -> Sha256 {
    Sha256::new(digit.to_string().repeat(64)).unwrap()
}

fn contract() -> RenderContract {
    RenderContract {
        project_id: ProjectId::new("project").unwrap(),
        revision_id: RevisionId::new("committed").unwrap(),
        range: FrameRange::new(ProjectFrame(7), ProjectFrame(10)).unwrap(),
        canvas: [319, 181],
        raster: [318, 180],
        frame_rate: FrameRate::new(30_000, 1_001).unwrap(),
        color_policy: ColorPolicy::SdrRec709,
        time_base: RenderTimeBase {
            numerator: 1,
            denominator: 30_000,
        },
        frame_count: 3,
        terminal_pts: 3_003,
        project_audio_start: AudioSample(11_211),
        project_audio_end: AudioSample(16_016),
        relative_aspect_error: ExactRatio::new(23, 9_570).unwrap(),
        mastering_display: None,
    }
}

fn request() -> RenderHostMessage {
    RenderHostMessage::Prepare {
        protocol: PROTOCOL_VERSION,
        identity: identity(),
        cancellation_token: CancellationToken::new("cancel-exact-attempt-1").unwrap(),
        contract: Box::new(contract()),
        document_sha256: sha256('a'),
        output_scope: WorkspaceRef::new(OUTPUT_SCOPE).unwrap(),
        maximum_output_bytes: MAX_PICTURE_BYTES,
        timeout_millis: 30_000,
    }
}

fn manifest() -> RenderManifest {
    let contract = contract();
    let byte_length = contract.total_bytes().unwrap();
    RenderManifest {
        contract,
        document_sha256: sha256('a'),
        planes: WorkspaceArtifact::new(
            WorkspaceRef::new(PICTURE_REF).unwrap(),
            sha256('b'),
            byte_length,
        )
        .unwrap(),
        pixel_policy: RenderPixelPolicy::I420Rec709LimitedLeft,
    }
}

fn completed(manifest: RenderManifest) -> RenderWorkerMessage {
    RenderWorkerMessage::Completed {
        protocol: PROTOCOL_VERSION,
        identity: identity(),
        manifest: Box::new(manifest),
    }
}

fn wire(value: &Value) -> Vec<u8> {
    let mut bytes = Vec::new();
    write_frame(&mut bytes, value).unwrap();
    bytes
}

#[test]
fn contract_evidence_matches_every_field_of_a_real_captured_revision() {
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("captured.deadpan");
    let initial = ProjectDocument::new(
        ProjectId::new("project").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 319,
            height: 181,
            frame_rate: FrameRate::new(30_000, 1_001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root").unwrap(),
    )
    .unwrap();
    let mut store = ProjectStore::create(&path, &initial).unwrap();
    let beat = NodeId::new("black").unwrap();
    store
        .commit(&CommandRequest {
            project_id: initial.project_id().clone(),
            expected_revision: initial.revision_id().clone(),
            new_revision: RevisionId::new("committed").unwrap(),
            command: Command::Insert {
                parent: initial.root().clone(),
                index: 0,
                subtree: Subtree {
                    root: beat.clone(),
                    nodes: BTreeMap::from([(
                        beat,
                        BeatNode::hold(
                            "Black",
                            HoldRecipe {
                                duration: FrameDuration::new(30).unwrap(),
                                video: HoldVideo::Background,
                                audio: HoldAudio::Silence,
                                picture_context: None,
                            },
                        ),
                    )]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        })
        .unwrap();
    let session = ProjectPictureSession::open_revision(
        &path,
        &RevisionId::new("committed").unwrap(),
        Some(contract().range),
        &AtomicBool::new(false),
    )
    .unwrap();
    let captured = ExportPictureContract::capture(&session).unwrap();
    let evidence = RenderContract::from_contract(&captured);
    assert_eq!(evidence, contract());
    assert!(evidence.matches(&captured));
    evidence.validate().unwrap();
    evidence.validate_for_encoding().unwrap();
    let encoded = serde_json::to_value(&evidence).unwrap();
    assert_eq!(encoded, serde_json::to_value(&captured).unwrap());
    assert_eq!(
        serde_json::from_value::<RenderContract>(encoded).unwrap(),
        evidence
    );
    let mut unrelated = evidence.clone();
    unrelated.revision_id = RevisionId::new("later").unwrap();
    assert!(!unrelated.matches(&captured));
    unrelated = evidence;
    unrelated.project_id = ProjectId::new("other-project").unwrap();
    assert!(!unrelated.matches(&captured));
}

#[test]
fn wire_round_trip_handles_fragmented_reads_and_preserves_cancellation() {
    struct Fragmented(Cursor<Vec<u8>>);
    impl Read for Fragmented {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            let length = bytes.len().min(1);
            self.0.read(&mut bytes[..length])
        }
    }
    let request = request();
    let protocol = RenderProtocol::from_request(&request).unwrap();
    let mut encoded = Vec::new();
    RenderProtocol::write_request(&mut encoded, &request).unwrap();
    let mut reader = Fragmented(Cursor::new(encoded));
    assert_eq!(
        read_host_message(&mut reader).unwrap(),
        Some(request.clone())
    );
    assert_eq!(read_host_message(&mut reader).unwrap(), None);
    let cancel = protocol.cancellation();
    let RenderHostMessage::Prepare {
        identity,
        cancellation_token,
        ..
    } = request
    else {
        unreachable!()
    };
    assert_eq!(
        cancel,
        RenderHostMessage::Cancel {
            protocol: PROTOCOL_VERSION,
            identity,
            cancellation_token
        }
    );
    assert!(RenderProtocol::from_request(&cancel).is_err());
    let mut encoded = Vec::new();
    RenderProtocol::write_request(&mut encoded, &cancel).unwrap();
    assert_eq!(
        read_host_message(&mut Cursor::new(encoded)).unwrap(),
        Some(cancel)
    );
    let reply = completed(manifest());
    let mut encoded = Vec::new();
    write_worker_message(&mut encoded, &reply).unwrap();
    let mut reader = Fragmented(Cursor::new(encoded));
    assert_eq!(
        RenderProtocol::read_response(&mut reader).unwrap(),
        Some(reply)
    );
    assert_eq!(RenderProtocol::read_response(&mut reader).unwrap(), None);
}

#[test]
fn wire_rejects_unknown_fields_at_every_structural_boundary() {
    let request = serde_json::to_value(request()).unwrap();
    for path in [
        "",
        "/identity",
        "/contract",
        "/contract/range",
        "/contract/frame_rate",
        "/contract/time_base",
        "/contract/relative_aspect_error",
    ] {
        let mut forged = request.clone();
        forged
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), json!(1));
        assert!(
            read_host_message(&mut Cursor::new(wire(&forged))).is_err(),
            "accepted {path}"
        );
    }
    let reply = serde_json::to_value(completed(manifest())).unwrap();
    for path in [
        "",
        "/identity",
        "/manifest",
        "/manifest/contract",
        "/manifest/planes",
    ] {
        let mut forged = reply.clone();
        forged
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unexpected".into(), json!(1));
        assert!(
            RenderProtocol::read_response(&mut Cursor::new(wire(&forged))).is_err(),
            "accepted {path}"
        );
    }
}

#[test]
fn wire_rejects_versions_missing_fields_and_invalid_bounded_values() {
    let request = serde_json::to_value(request()).unwrap();
    for (path, value) in [
        ("/protocol", json!(0)),
        ("/protocol", json!(2)),
        ("/identity/request_id", json!("")),
        ("/identity/attempt_id", json!("a".repeat(129))),
        ("/cancellation_token", json!("../cancel")),
        ("/document_sha256", json!("a".repeat(63))),
        ("/output_scope", json!("../output")),
        ("/contract/frame_rate/numerator", json!(0)),
    ] {
        let mut forged = request.clone();
        *forged.pointer_mut(path).unwrap() = value;
        assert!(
            read_host_message(&mut Cursor::new(wire(&forged))).is_err(),
            "accepted {path}"
        );
    }
    for field in [
        "identity",
        "protocol",
        "contract",
        "document_sha256",
        "timeout_millis",
    ] {
        let mut forged = request.clone();
        forged.as_object_mut().unwrap().remove(field);
        assert!(
            read_host_message(&mut Cursor::new(wire(&forged))).is_err(),
            "accepted missing {field}"
        );
    }
    let mut reply = serde_json::to_value(completed(manifest())).unwrap();
    reply["manifest"]["pixel_policy"] = json!("i420_rec709_limited_center");
    assert!(RenderProtocol::read_response(&mut Cursor::new(wire(&reply))).is_err());
    reply = serde_json::to_value(completed(manifest())).unwrap();
    reply["manifest"]["planes"]["sha256"] = json!("B".repeat(64));
    assert!(RenderProtocol::read_response(&mut Cursor::new(wire(&reply))).is_err());
    let mut failed = serde_json::to_value(RenderWorkerMessage::Failed {
        protocol: PROTOCOL_VERSION,
        identity: identity(),
        diagnostic: Diagnostic::new("decode failed").unwrap(),
    })
    .unwrap();
    failed["diagnostic"] = json!("x".repeat(deadpan_jobs::MAX_DIAGNOSTIC_BYTES + 1));
    assert!(RenderProtocol::read_response(&mut Cursor::new(wire(&failed))).is_err());
}

#[test]
fn wire_rejects_truncated_or_oversized_frames_before_deserialization() {
    let mut encoded = Vec::new();
    RenderProtocol::write_request(&mut encoded, &request()).unwrap();
    for length in [1, 2, 3, 4, encoded.len() - 1] {
        assert!(read_host_message(&mut Cursor::new(&encoded[..length])).is_err());
        assert!(RenderProtocol::read_response(&mut Cursor::new(&encoded[..length])).is_err());
    }
    let oversized = u32::try_from(MAX_FRAME_BYTES + 1).unwrap().to_be_bytes();
    assert!(
        read_host_message(&mut Cursor::new(oversized))
            .unwrap_err()
            .contains("maximum")
    );
    assert!(RenderProtocol::read_response(&mut Cursor::new(oversized)).is_err());
    assert!(read_host_message(&mut Cursor::new([0_u8; 4])).is_err());
    assert!(write_frame(&mut Vec::new(), &"a".repeat(MAX_FRAME_BYTES + 1)).is_err());
}

#[test]
fn contract_rejects_forged_clock_geometry_and_unsupported_color() {
    let changes: [fn(&mut RenderContract); 14] = [
        |value| value.frame_count += 1,
        |value| value.terminal_pts += 1,
        |value| value.project_audio_start.0 += 1,
        |value| value.project_audio_end.0 += 1,
        |value| value.time_base.numerator = 2,
        |value| value.time_base.denominator = 30,
        |value| value.frame_rate = FrameRate::new(30, 1).unwrap(),
        |value| value.frame_rate = FrameRate::new(u32::MAX, 1).unwrap(),
        |value| value.raster[0] = 319,
        |value| value.raster[0] = 320,
        |value| value.raster[1] = 0,
        |value| value.canvas[0] = 0,
        |value| value.relative_aspect_error = ExactRatio::ZERO,
        |value| {
            value.color_policy = ColorPolicy::SdrRec709;
            value.mastering_display = Some(deadpan_core::MasteringDisplay {
                primaries: [[35_400, 14_600], [8_500, 39_850], [6_550, 2_300]],
                white_point: [15_635, 16_450],
                max_luminance: 10_000_000,
                min_luminance: 50,
            });
        },
    ];
    contract().validate().unwrap();
    // HDR pictures are encodable, but the raw diagnostic sink stays SDR-only.
    for policy in [ColorPolicy::HdrRec2020Pq, ColorPolicy::HdrRec2020Hlg] {
        let mut hdr = contract();
        hdr.color_policy = policy;
        hdr.validate_for_encoding().unwrap();
        assert!(hdr.validate().is_err());
        assert_eq!(
            hdr.frame_bytes().unwrap(),
            2 * contract().frame_bytes().unwrap()
        );
    }
    for change in changes {
        let mut invalid = contract();
        change(&mut invalid);
        assert!(invalid.validate().is_err(), "accepted {invalid:?}");
        assert!(
            invalid.validate_for_encoding().is_err(),
            "accepted encoded {invalid:?}"
        );
    }
    for canvas in [
        [8_193, 2],
        [4_097, 4_097],
        [8_190, 2_049],
        [u32::MAX, u32::MAX],
    ] {
        let mut invalid = contract();
        invalid.canvas = canvas;
        assert!(invalid.validate().is_err());
    }
}

#[test]
fn range_and_audio_checks_use_absolute_boundaries_without_duration_rounding() {
    let mut value = contract();
    value.range = FrameRange::new(ProjectFrame(1), ProjectFrame(2)).unwrap();
    value.frame_count = 1;
    value.terminal_pts = 1_001;
    value.project_audio_start = AudioSample(1_602);
    value.project_audio_end = AudioSample(3_203);
    value.validate().unwrap();
    assert_eq!(
        value.project_audio_end.0 - value.project_audio_start.0,
        1_601
    );
    value.project_audio_end = AudioSample(3_204);
    assert!(value.validate().is_err());
    for range in [
        FrameRange::new(ProjectFrame(-1), ProjectFrame(2)).unwrap(),
        FrameRange::new(ProjectFrame(7), ProjectFrame(7)).unwrap(),
        FrameRange::new(ProjectFrame(i64::MAX - 3), ProjectFrame(i64::MAX)).unwrap(),
    ] {
        let mut invalid = contract();
        invalid.range = range;
        assert!(invalid.validate().is_err());
    }
}

#[test]
fn checked_picture_lengths_and_request_budgets_bound_the_raw_sink() {
    let value = contract();
    assert_eq!(value.frame_bytes().unwrap(), 85_860);
    assert_eq!(value.total_bytes().unwrap(), 257_580);
    let mut overflowing = value.clone();
    overflowing.frame_count = u64::MAX;
    assert!(overflowing.total_bytes().is_err());
    let request = serde_json::to_value(request()).unwrap();
    for (field, invalid) in [
        ("maximum_output_bytes", json!(0)),
        (
            "maximum_output_bytes",
            json!(value.total_bytes().unwrap() - 1),
        ),
        ("maximum_output_bytes", json!(MAX_PICTURE_BYTES + 1)),
        ("timeout_millis", json!(0)),
        ("timeout_millis", json!(MAX_TIMEOUT_MILLIS + 1)),
        ("output_scope", json!("output/subdir")),
    ] {
        let mut forged = request.clone();
        forged[field] = invalid;
        assert!(
            read_host_message(&mut Cursor::new(wire(&forged))).is_err(),
            "accepted {field}"
        );
    }
    for budget in [value.total_bytes().unwrap(), MAX_PICTURE_BYTES] {
        let mut admitted = request.clone();
        admitted["maximum_output_bytes"] = json!(budget);
        admitted["timeout_millis"] = json!(MAX_TIMEOUT_MILLIS);
        read_host_message(&mut Cursor::new(wire(&admitted))).unwrap();
    }
    for frames in [100_000, 100_001] {
        let mut large = value.clone();
        large.range = FrameRange::new(ProjectFrame(0), ProjectFrame(frames)).unwrap();
        large.frame_count = u64::try_from(frames).unwrap();
        large.terminal_pts = frames * 1_001;
        large.project_audio_start = AudioSample(0);
        large.project_audio_end = large.frame_rate.audio_boundary(large.range.end()).unwrap();
        assert!(large.validate().is_err());
        large.validate_for_encoding().unwrap();
        large.canvas = [2, 2];
        large.raster = [2, 2];
        large.relative_aspect_error = ExactRatio::ZERO;
        assert_eq!(large.validate().is_ok(), frames == 100_000);
        large.validate_for_encoding().unwrap();
    }
}

#[test]
fn classifier_rejects_other_requests_stale_attempts_and_changed_versions() {
    let protocol = RenderProtocol::from_request(&request()).unwrap();
    for altered_identity in [
        RenderIdentity {
            request_id: RequestId::new("other-request").unwrap(),
            ..identity()
        },
        RenderIdentity {
            attempt_id: AttemptId::new("previous-attempt").unwrap(),
            ..identity()
        },
    ] {
        for response in [
            RenderWorkerMessage::Progress {
                protocol: PROTOCOL_VERSION,
                identity: altered_identity.clone(),
                completed_frames: 0,
                total_frames: 3,
            },
            RenderWorkerMessage::Completed {
                protocol: PROTOCOL_VERSION,
                identity: altered_identity.clone(),
                manifest: Box::new(manifest()),
            },
            RenderWorkerMessage::Failed {
                protocol: PROTOCOL_VERSION,
                identity: altered_identity.clone(),
                diagnostic: Diagnostic::new("failed").unwrap(),
            },
            RenderWorkerMessage::Cancelled {
                protocol: PROTOCOL_VERSION,
                identity: altered_identity.clone(),
            },
        ] {
            assert!(protocol.classify(&response).is_err());
        }
    }
    assert!(
        protocol
            .classify(&RenderWorkerMessage::Cancelled {
                protocol: 2,
                identity: identity()
            })
            .is_err()
    );
}

#[test]
fn classifier_binds_progress_and_completed_evidence_to_the_request() {
    let protocol = RenderProtocol::from_request(&request()).unwrap();
    for (completed_frames, total_frames, expected) in [
        (0, 3, true),
        (3, 3, true),
        (4, 3, false),
        (0, 0, false),
        (1, 4, false),
        (0, 100_001, false),
    ] {
        let response = RenderWorkerMessage::Progress {
            protocol: PROTOCOL_VERSION,
            identity: identity(),
            completed_frames,
            total_frames,
        };
        assert_eq!(protocol.classify(&response).is_ok(), expected);
    }
    assert_eq!(
        protocol.classify(&completed(manifest())).unwrap(),
        ResponseKind::Completed
    );
    let mut wrong_document = manifest();
    wrong_document.document_sha256 = sha256('c');
    assert!(protocol.classify(&completed(wrong_document)).is_err());
    let mut wrong_revision = manifest();
    wrong_revision.contract.revision_id = RevisionId::new("new-revision").unwrap();
    assert!(protocol.classify(&completed(wrong_revision)).is_err());
    for (reference, length) in [
        ("output/other.i420", 257_580),
        (PICTURE_REF, 257_579),
        (PICTURE_REF, 257_581),
    ] {
        let mut wrong_planes = manifest();
        wrong_planes.planes =
            WorkspaceArtifact::new(WorkspaceRef::new(reference).unwrap(), sha256('b'), length)
                .unwrap();
        assert!(protocol.classify(&completed(wrong_planes)).is_err());
    }
    for response in [
        RenderWorkerMessage::Failed {
            protocol: PROTOCOL_VERSION,
            identity: identity(),
            diagnostic: Diagnostic::new("source unavailable").unwrap(),
        },
        RenderWorkerMessage::Cancelled {
            protocol: PROTOCOL_VERSION,
            identity: identity(),
        },
    ] {
        assert_eq!(
            protocol.classify(&response).unwrap(),
            ResponseKind::Terminal
        );
    }
}

#[test]
fn adversarial_render_worker_frames() {
    let cancel = RenderProtocol::from_request(&request())
        .unwrap()
        .cancellation();
    crate::adversarial::protocol::<RenderProtocol>(
        "cli-render-protocol",
        vec![request(), cancel],
        vec![completed(manifest())],
        |reader| read_host_message(reader),
    );
}
