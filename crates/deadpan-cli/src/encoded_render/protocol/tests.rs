use std::io::{self, Cursor};

use deadpan_core::{
    AudioSample, ColorPolicy, ExactRatio, FrameRange, FrameRate, ProjectFrame, ProjectId,
    RevisionId,
};
use deadpan_encode::EncoderInfo;
use deadpan_jobs::{AttemptId, MAX_FRAME_BYTES, RequestId};
use serde_json::{Value, json};

use crate::render_worker::protocol::{MAX_PICTURE_BYTES, MAX_PICTURE_FRAMES, RenderTimeBase};

use super::*;

fn identity() -> RenderIdentity {
    RenderIdentity {
        request_id: RequestId::new("encode-1").unwrap(),
        attempt_id: AttemptId::new("attempt-1").unwrap(),
    }
}

fn hash(digit: char) -> Sha256 {
    Sha256::new(digit.to_string().repeat(64)).unwrap()
}

fn contract() -> EncodedRenderContract {
    EncodedRenderContract {
        picture: RenderContract {
            project_id: ProjectId::new("project").unwrap(),
            revision_id: RevisionId::new("committed").unwrap(),
            range: FrameRange::new(ProjectFrame(1), ProjectFrame(2)).unwrap(),
            canvas: [319, 181],
            raster: [318, 180],
            frame_rate: FrameRate::new(30_000, 1_001).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
            time_base: RenderTimeBase {
                numerator: 1,
                denominator: 30_000,
            },
            frame_count: 1,
            terminal_pts: 1_001,
            project_audio_start: AudioSample(1_602),
            project_audio_end: AudioSample(3_203),
            relative_aspect_error: ExactRatio::new(23, 9_570).unwrap(),
            mastering_display: None,
        },
        choice: EncoderChoice {
            mode: EncoderMode::Hardware,
            b_frames: BFramePolicy::None,
        },
    }
}

fn limits() -> EncodeLimits {
    EncodeLimits {
        maximum_output_bytes: 1_048_576,
        maximum_packets: 256,
        maximum_packet_bytes: 262_144,
    }
}

fn request() -> EncodedHostMessage {
    EncodedHostMessage::Prepare {
        protocol: PROTOCOL_VERSION,
        identity: identity(),
        cancellation_token: CancellationToken::new("cancel-this-attempt").unwrap(),
        contract: Box::new(contract()),
        binding: None,
        document_sha256: hash('a'),
        output_scope: WorkspaceRef::new(OUTPUT_SCOPE).unwrap(),
        limits: limits(),
        timeout_millis: 30_000,
    }
}

fn report(contract: &EncodedRenderContract, limits: EncodeLimits) -> EncodeReport {
    let native = contract.native_contract().unwrap();
    let policy = native.policy();
    EncodeReport {
        info: EncoderInfo {
            abi_version: 2,
            avcodec_version: 4_066_151,
            avformat_version: 4_064_103,
            avutil_version: 3_934_311,
            movie_timescale: policy.movie_timescale,
            video_time_base_num: 1,
            video_time_base_den: native.frame_rate()[0],
            audio_time_base_num: 1,
            audio_time_base_den: AUDIO_SAMPLE_RATE,
            audio_frame_size: AUDIO_FRAME_SAMPLES,
            video_profile: 100,
            video_has_b_frames: 0,
            video_max_b_frames: i32::try_from(policy.b_frames).unwrap(),
            video_gop_size: i32::try_from(policy.gop_frames).unwrap(),
            audio_profile: 1,
            audio_initial_padding: 1_024,
            audio_trailing_padding: 0,
            requested_mode: native.mode(),
            video_bitrate: policy.video_bitrate,
            audio_bitrate: policy.audio_bitrate,
            maximum_moov_bytes: moov_bound(limits.maximum_packets).unwrap(),
        },
        video_frames: native.video_frames(),
        audio_samples: native.audio_samples(),
        video_packets: native.video_frames(),
        audio_packets: native
            .audio_samples()
            .div_ceil(u64::from(AUDIO_FRAME_SAMPLES))
            + u64::from(AUDIO_PRIMING_SAMPLES / AUDIO_FRAME_SAMPLES),
        output_bytes: 4_096,
        packet_bytes: 2_048,
        video_duration_from_contract_packets: native.video_frames(),
        video_media_duration_correction: None,
        faststart_read_opens: 1,
        faststart_read_closes: 1,
        video_eof: true,
        audio_eof: true,
    }
}

fn manifest() -> EncodedManifest {
    let contract = contract();
    let report = report(&contract, limits());
    EncodedManifest {
        contract,
        document_sha256: hash('a'),
        movie: WorkspaceArtifact::new(
            WorkspaceRef::new(MOVIE_REF).unwrap(),
            hash('b'),
            report.output_bytes,
        )
        .unwrap(),
        report,
    }
}

fn completed(manifest: EncodedManifest) -> EncodedWorkerMessage {
    EncodedWorkerMessage::Completed {
        protocol: PROTOCOL_VERSION,
        identity: identity(),
        manifest: Box::new(manifest),
        binding: None,
    }
}

fn wire(value: &Value) -> Vec<u8> {
    let mut bytes = Vec::new();
    write_frame(&mut bytes, value).unwrap();
    bytes
}

#[test]
fn encoded_contract_keeps_absolute_audio_phase_and_exceeds_only_raw_sink_limits() {
    let evidence = contract();
    evidence.validate().unwrap();
    let native = evidence.native_contract().unwrap();
    assert_eq!(native.audio_samples(), 1_601);
    assert_eq!(native.picture_timing(0).unwrap(), (0, 1_001));
    assert_eq!(native.raster(), [318, 180]);
    let mut large = evidence;
    let frames = i64::try_from(MAX_PICTURE_FRAMES + 1).unwrap();
    large.picture.range = FrameRange::new(ProjectFrame(0), ProjectFrame(frames)).unwrap();
    large.picture.frame_count = u64::try_from(frames).unwrap();
    large.picture.terminal_pts = frames * 1_001;
    large.picture.project_audio_start = AudioSample(0);
    large.picture.project_audio_end = large
        .picture
        .frame_rate
        .audio_boundary(ProjectFrame(frames))
        .unwrap();
    assert!(large.picture.total_bytes().unwrap() > MAX_PICTURE_BYTES);
    assert!(large.picture.validate().is_err());
    large.validate().unwrap();
    let mut request = request();
    if let EncodedHostMessage::Prepare {
        contract, limits, ..
    } = &mut request
    {
        **contract = large;
        *limits = EncodeLimits::default();
    }
    EncodedProtocol::from_request(&request).unwrap();
    assert!(request.validate().is_ok());
}

#[test]
fn native_reconstruction_rejects_clock_geometry_and_encoding_bounds() {
    let changes: [fn(&mut EncodedRenderContract); 10] = [
        |value| value.picture.project_audio_end.0 += 1,
        |value| value.picture.project_audio_start.0 -= 1,
        |value| value.picture.frame_count += 1,
        |value| value.picture.terminal_pts += 1,
        |value| value.picture.time_base.numerator = 2,
        |value| value.picture.raster[0] += 1,
        |value| value.picture.relative_aspect_error = ExactRatio::ZERO,
        // Mastering metadata belongs only to PQ output.
        |value| {
            value.picture.color_policy = ColorPolicy::HdrRec2020Hlg;
            value.picture.mastering_display = Some(deadpan_core::MasteringDisplay {
                primaries: [[35_400, 14_600], [8_500, 39_850], [6_550, 2_300]],
                white_point: [15_635, 16_450],
                max_luminance: 10_000_000,
                min_luminance: 50,
            });
        },
        |value| value.picture.canvas = [8193, 2],
        |value| value.picture.range = FrameRange::new(ProjectFrame(-1), ProjectFrame(0)).unwrap(),
    ];
    for change in changes {
        let mut invalid = contract();
        change(&mut invalid);
        assert!(invalid.native_contract().is_err(), "accepted {invalid:?}");
    }
    for (policy, format) in [
        (
            ColorPolicy::HdrRec2020Pq,
            deadpan_encode::VideoFormat::HevcMain10Rec2100Pq,
        ),
        (
            ColorPolicy::HdrRec2020Hlg,
            deadpan_encode::VideoFormat::HevcMain10Rec2100Hlg,
        ),
    ] {
        let mut hdr = contract();
        hdr.picture.color_policy = policy;
        let native = hdr.native_contract().unwrap();
        assert_eq!(native.video_format(), format);
        assert_eq!(native.picture_bytes(), hdr.picture.frame_bytes().unwrap());
    }
    let mut too_fast = contract();
    too_fast.picture.frame_rate = FrameRate::new(120, 1).unwrap();
    too_fast.picture.time_base.denominator = 120;
    too_fast.picture.terminal_pts = 1;
    too_fast.picture.project_audio_start = AudioSample(400);
    too_fast.picture.project_audio_end = AudioSample(800);
    too_fast.picture.validate_for_encoding().unwrap();
    assert!(too_fast.native_contract().is_err());
    for frames in [MAX_VIDEO_FRAMES + 1, 24 * 60 * 60 * 30 + 1] {
        let mut invalid = contract();
        let end = i64::try_from(frames).unwrap();
        invalid.picture.range = FrameRange::new(ProjectFrame(0), ProjectFrame(end)).unwrap();
        invalid.picture.frame_count = frames;
        invalid.picture.terminal_pts = end * 1001;
        invalid.picture.project_audio_start = AudioSample(0);
        invalid.picture.project_audio_end = invalid
            .picture
            .frame_rate
            .audio_boundary(ProjectFrame(end))
            .unwrap();
        assert!(invalid.native_contract().is_err());
    }
}

#[test]
fn explicit_software_b_frame_attempt_keeps_requested_and_observed_counts_distinct() {
    let mut value = manifest();
    value.contract.choice = EncoderChoice {
        mode: EncoderMode::Software,
        b_frames: BFramePolicy::TargetTwo,
    };
    value.report = report(&value.contract, limits());
    value.report.info.video_has_b_frames = 1;
    value.validate_for(limits()).unwrap();
    let mut request = request();
    if let EncodedHostMessage::Prepare { contract, .. } = &mut request {
        **contract = value.contract.clone();
    }
    let protocol = EncodedProtocol::from_request(&request).unwrap();
    assert_eq!(
        protocol.classify(&completed(value.clone())).unwrap(),
        ResponseKind::Completed
    );
    value.report.info.video_has_b_frames = 3;
    assert!(protocol.classify(&completed(value)).is_err());
}

#[test]
fn round_trip_handles_fragmented_frames_and_preserves_exact_cancellation() {
    struct Fragmented(Cursor<Vec<u8>>);
    impl Read for Fragmented {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            let count = bytes.len().min(1);
            self.0.read(&mut bytes[..count])
        }
    }
    let request = request();
    let protocol = EncodedProtocol::from_request(&request).unwrap();
    let mut bytes = Vec::new();
    EncodedProtocol::write_request(&mut bytes, &request).unwrap();
    assert_eq!(
        read_host_message(&mut Fragmented(Cursor::new(bytes))).unwrap(),
        Some(request.clone())
    );
    let EncodedHostMessage::Prepare {
        identity,
        cancellation_token,
        ..
    } = request
    else {
        unreachable!()
    };
    let cancel = protocol.cancellation();
    assert_eq!(
        cancel,
        EncodedHostMessage::Cancel {
            protocol: PROTOCOL_VERSION,
            identity,
            cancellation_token
        }
    );
    assert!(EncodedProtocol::from_request(&cancel).is_err());
    let mut bytes = Vec::new();
    EncodedProtocol::write_request(&mut bytes, &cancel).unwrap();
    assert_eq!(
        read_host_message(&mut Cursor::new(bytes)).unwrap(),
        Some(cancel)
    );
    let response = completed(manifest());
    let mut bytes = Vec::new();
    write_worker_message(&mut bytes, &response).unwrap();
    let mut reader = Fragmented(Cursor::new(bytes));
    assert_eq!(
        EncodedProtocol::read_response(&mut reader).unwrap(),
        Some(response)
    );
    assert_eq!(EncodedProtocol::read_response(&mut reader).unwrap(), None);
}

#[test]
fn version_three_requires_explicit_binding_without_changing_persisted_manifest() {
    assert_eq!(PROTOCOL_VERSION, 3);
    let prepare = serde_json::to_value(request()).unwrap();
    let complete = serde_json::to_value(completed(manifest())).unwrap();
    assert_eq!(prepare.get("binding"), Some(&Value::Null));
    assert_eq!(complete.get("binding"), Some(&Value::Null));
    for value in [&prepare, &complete] {
        for replacement in [None, Some(json!({})), Some(json!(false))] {
            let mut changed = value.clone();
            if let Some(replacement) = replacement {
                changed["binding"] = replacement;
            } else {
                changed.as_object_mut().unwrap().remove("binding");
            }
            let bytes = wire(&changed);
            if value.get("op").is_some() {
                assert!(read_host_message(&mut Cursor::new(bytes)).is_err());
            } else {
                assert!(EncodedProtocol::read_response(&mut Cursor::new(bytes)).is_err());
            }
        }
        let mut legacy = value.clone();
        legacy["protocol"] = json!(2);
        let bytes = wire(&legacy);
        if value.get("op").is_some() {
            assert!(read_host_message(&mut Cursor::new(bytes)).is_err());
        } else {
            assert!(EncodedProtocol::read_response(&mut Cursor::new(bytes)).is_err());
        }
    }
    let mut persisted = complete["manifest"].clone();
    let mut keys: Vec<_> = persisted
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["contract", "document_sha256", "movie", "report"]);
    serde_json::from_value::<EncodedManifest>(persisted.clone()).unwrap();
    persisted["binding"] = Value::Null;
    assert!(serde_json::from_value::<EncodedManifest>(persisted).is_err());
}

#[test]
fn runtime_bound_completion_requires_the_exact_requested_observation() {
    let binding = crate::encoded_render::runtime::test_binding(&contract());
    let engineering = EncodedProtocol::from_request(&request()).unwrap();
    let mut bound_request = request();
    if let EncodedHostMessage::Prepare { binding: slot, .. } = &mut bound_request {
        *slot = Some(Box::new(binding.clone()));
    }
    let bound = EncodedProtocol::from_request(&bound_request).unwrap();
    assert!(bound.classify(&completed(manifest())).is_err());
    let mut response = completed(manifest());
    if let EncodedWorkerMessage::Completed { binding: slot, .. } = &mut response {
        *slot = Some(Box::new(binding.clone()));
    }
    assert_eq!(bound.classify(&response).unwrap(), ResponseKind::Completed);
    assert!(engineering.classify(&response).is_err());
    let mut bytes = Vec::new();
    EncodedProtocol::write_request(&mut bytes, &bound_request).unwrap();
    assert_eq!(
        read_host_message(&mut Cursor::new(bytes)).unwrap(),
        Some(bound_request.clone())
    );
    let mut bytes = Vec::new();
    write_worker_message(&mut bytes, &response).unwrap();
    assert_eq!(
        EncodedProtocol::read_response(&mut Cursor::new(bytes)).unwrap(),
        Some(response.clone())
    );
    if let EncodedWorkerMessage::Completed {
        binding: Some(binding),
        ..
    } = &mut response
    {
        binding.runtime.images[0].sha256 = hash('c');
    }
    response.validate().unwrap();
    assert!(bound.classify(&response).is_err());
    if let EncodedHostMessage::Prepare {
        binding: Some(binding),
        ..
    } = &mut bound_request
    {
        binding.policy_version += 1;
    }
    assert!(EncodedProtocol::from_request(&bound_request).is_err());
}

#[test]
fn unknown_fields_and_unrecognized_encoder_choices_fail_closed() {
    let request = serde_json::to_value(request()).unwrap();
    for path in [
        "",
        "/identity",
        "/contract",
        "/contract/picture",
        "/contract/choice",
        "/limits",
    ] {
        let mut changed = request.clone();
        changed
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!(true));
        assert!(
            read_host_message(&mut Cursor::new(wire(&changed))).is_err(),
            "accepted {path}"
        );
    }
    for (path, value) in [
        ("/contract/choice/mode", json!("automatic")),
        ("/contract/choice/b_frames", json!("unbounded")),
    ] {
        let mut changed = request.clone();
        *changed.pointer_mut(path).unwrap() = value;
        assert!(read_host_message(&mut Cursor::new(wire(&changed))).is_err());
    }
    let response = serde_json::to_value(completed(manifest())).unwrap();
    for path in [
        "",
        "/manifest",
        "/manifest/contract",
        "/manifest/movie",
        "/manifest/report",
        "/manifest/report/info",
    ] {
        let mut changed = response.clone();
        changed
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!(true));
        assert!(
            EncodedProtocol::read_response(&mut Cursor::new(wire(&changed))).is_err(),
            "accepted {path}"
        );
    }
}

#[test]
fn request_limits_versions_and_bounded_scalars_are_admitted_before_allocation() {
    let request = serde_json::to_value(request()).unwrap();
    for (path, value) in [
        ("/protocol", json!(0)),
        ("/protocol", json!(1)),
        ("/protocol", json!(2)),
        ("/protocol", json!(PROTOCOL_VERSION + 1)),
        ("/identity/request_id", json!("")),
        ("/identity/attempt_id", json!("a".repeat(129))),
        ("/cancellation_token", json!("../cancel")),
        ("/document_sha256", json!("a".repeat(63))),
        ("/output_scope", json!("output/other")),
        ("/timeout_millis", json!(0)),
        ("/timeout_millis", json!(MAX_TIMEOUT_MILLIS + 1)),
        ("/limits/maximum_output_bytes", json!(0)),
        ("/limits/maximum_output_bytes", json!(MAX_OUTPUT_BYTES + 1)),
        ("/limits/maximum_packets", json!(4)),
        ("/limits/maximum_packets", json!(MAX_PACKETS + 1)),
        ("/limits/maximum_packet_bytes", json!(0)),
        ("/limits/maximum_packet_bytes", json!(MAX_PACKET_BYTES + 1)),
    ] {
        let mut changed = request.clone();
        *changed.pointer_mut(path).unwrap() = value;
        assert!(
            read_host_message(&mut Cursor::new(wire(&changed))).is_err(),
            "accepted {path}"
        );
    }
    for field in [
        "protocol",
        "contract",
        "document_sha256",
        "limits",
        "timeout_millis",
    ] {
        let mut changed = request.clone();
        changed.as_object_mut().unwrap().remove(field);
        assert!(read_host_message(&mut Cursor::new(wire(&changed))).is_err());
    }
    let mut maximal = request;
    maximal["limits"] = serde_json::to_value(EncodeLimits::default()).unwrap();
    maximal["timeout_millis"] = json!(MAX_TIMEOUT_MILLIS);
    read_host_message(&mut Cursor::new(wire(&maximal))).unwrap();
}

#[test]
fn wire_rejects_truncation_oversize_and_unbounded_diagnostics() {
    let bytes = wire(&serde_json::to_value(request()).unwrap());
    for length in [1, 2, 3, 4, bytes.len() - 1] {
        assert!(read_host_message(&mut Cursor::new(&bytes[..length])).is_err());
        assert!(EncodedProtocol::read_response(&mut Cursor::new(&bytes[..length])).is_err());
    }
    let oversize = u32::try_from(MAX_FRAME_BYTES + 1).unwrap().to_be_bytes();
    assert!(read_host_message(&mut Cursor::new(oversize)).is_err());
    assert!(EncodedProtocol::read_response(&mut Cursor::new(oversize)).is_err());
    assert!(read_host_message(&mut Cursor::new([0_u8; 4])).is_err());
    let failed = json!({"event":"failed", "protocol":PROTOCOL_VERSION, "identity":identity(),
        "failure": {"kind": {"stage": "output"},
        "diagnostic":"x".repeat(deadpan_jobs::MAX_DIAGNOSTIC_BYTES + 1)}});
    assert!(EncodedProtocol::read_response(&mut Cursor::new(wire(&failed))).is_err());
}

#[test]
fn progress_and_terminals_bind_both_counts_and_exact_attempt() {
    let protocol = EncodedProtocol::from_request(&request()).unwrap();
    for (frames, total_frames, samples, total_audio_samples, accepted) in [
        (0, 1, 0, 1601, true),
        (1, 1, 1601, 1601, true),
        (2, 1, 0, 1601, false),
        (0, 1, 1602, 1601, false),
        (0, 2, 0, 1601, false),
        (0, 1, 0, 1602, false),
        (0, 0, 0, 1601, false),
        (0, 1, 0, 0, false),
        (0, MAX_VIDEO_FRAMES + 1, 0, 1601, false),
        (0, 1, 0, MAX_AUDIO_SAMPLES + 1, false),
    ] {
        let progress = EncodedWorkerMessage::Progress {
            protocol: PROTOCOL_VERSION,
            identity: identity(),
            completed_frames: frames,
            total_frames,
            completed_audio_samples: samples,
            total_audio_samples,
        };
        assert_eq!(protocol.classify(&progress).is_ok(), accepted);
    }
    for altered in [
        RenderIdentity {
            request_id: RequestId::new("other").unwrap(),
            ..identity()
        },
        RenderIdentity {
            attempt_id: AttemptId::new("stale").unwrap(),
            ..identity()
        },
    ] {
        for response in [
            EncodedWorkerMessage::Progress {
                protocol: PROTOCOL_VERSION,
                identity: altered.clone(),
                completed_frames: 0,
                total_frames: 1,
                completed_audio_samples: 0,
                total_audio_samples: 1601,
            },
            EncodedWorkerMessage::Completed {
                protocol: PROTOCOL_VERSION,
                identity: altered.clone(),
                manifest: Box::new(manifest()),
                binding: None,
            },
            EncodedWorkerMessage::Failed {
                protocol: PROTOCOL_VERSION,
                identity: altered.clone(),
                failure: EncodedFailure {
                    kind: EncodedFailureKind::Contract,
                    diagnostic: Diagnostic::new("failed").unwrap(),
                },
            },
            EncodedWorkerMessage::Cancelled {
                protocol: PROTOCOL_VERSION,
                identity: altered.clone(),
            },
        ] {
            assert!(protocol.classify(&response).is_err());
        }
    }
    assert!(
        protocol
            .classify(&EncodedWorkerMessage::Cancelled {
                protocol: 1,
                identity: identity()
            })
            .is_err()
    );
    assert_eq!(
        protocol
            .classify(&EncodedWorkerMessage::Cancelled {
                protocol: PROTOCOL_VERSION,
                identity: identity()
            })
            .unwrap(),
        ResponseKind::Terminal
    );
    assert_eq!(
        protocol.classify(&completed(manifest())).unwrap(),
        ResponseKind::Completed
    );
}

#[test]
fn failures_require_a_strict_typed_boundary_independent_of_diagnostic_text() {
    let response = EncodedWorkerMessage::Failed {
        protocol: PROTOCOL_VERSION,
        identity: identity(),
        failure: EncodedFailure {
            kind: EncodedFailureKind::Encoder(
                deadpan_encode::EncodeFailureKind::VideoTimestampOrder,
            ),
            diagnostic: Diagnostic::new("actual native rejection").unwrap(),
        },
    };
    let protocol = EncodedProtocol::from_request(&request()).unwrap();
    assert_eq!(protocol.classify(&response).unwrap(), ResponseKind::Failed);
    let original = serde_json::to_value(&response).unwrap();
    for path in ["", "/failure", "/failure/kind"] {
        let mut changed = original.clone();
        changed
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("fallback".into(), json!(true));
        assert!(
            EncodedProtocol::read_response(&mut Cursor::new(wire(&changed))).is_err(),
            "{path}"
        );
    }
    for (path, value) in [
        ("/failure/kind/stage", json!("future_stage")),
        ("/failure/kind/kind", json!("future_encoder_kind")),
        (
            "/failure/kind",
            json!({"stage": "source", "kind": "encoder_unavailable"}),
        ),
        ("/failure/kind", json!({"stage": "encoder"})),
        ("/failure/diagnostic", json!("\0")),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(path).unwrap() = value;
        assert!(
            EncodedProtocol::read_response(&mut Cursor::new(wire(&changed))).is_err(),
            "{path}"
        );
    }
    for missing in ["kind", "diagnostic"] {
        let mut changed = original.clone();
        changed["failure"].as_object_mut().unwrap().remove(missing);
        assert!(EncodedProtocol::read_response(&mut Cursor::new(wire(&changed))).is_err());
    }
    let mut source = original;
    source["failure"]["kind"] = json!({"stage": "source"});
    source["failure"]["diagnostic"] = json!("video_encoder_unavailable: misleading source text");
    let Some(EncodedWorkerMessage::Failed { failure, .. }) =
        EncodedProtocol::read_response(&mut Cursor::new(wire(&source))).unwrap()
    else {
        panic!("failure response lost")
    };
    assert_eq!(failure.kind, EncodedFailureKind::Source);
    assert_eq!(failure.kind.code(), "render_source_failed");
}

#[test]
fn completion_rejects_forged_native_counts_clocks_policy_and_drain_claims() {
    let original = serde_json::to_value(completed(manifest())).unwrap();
    for (path, value) in [
        ("/video_frames", json!(2)),
        ("/audio_samples", json!(1602)),
        ("/video_packets", json!(0)),
        ("/audio_packets", json!(2)),
        ("/audio_packets", json!(3)),
        ("/output_bytes", json!(0)),
        ("/packet_bytes", json!(0)),
        ("/packet_bytes", json!(4097)),
        ("/video_duration_from_contract_packets", json!(2)),
        ("/faststart_read_opens", json!(0)),
        ("/faststart_read_closes", json!(2)),
        ("/video_eof", json!(false)),
        ("/audio_eof", json!(false)),
        ("/info/abi_version", json!(1)),
        ("/info/avcodec_version", json!(0)),
        ("/info/avformat_version", json!(0)),
        ("/info/avutil_version", json!(u32::MAX)),
        ("/info/movie_timescale", json!(30000)),
        ("/info/video_time_base_num", json!(2)),
        ("/info/video_time_base_den", json!(30)),
        ("/info/audio_time_base_num", json!(2)),
        ("/info/audio_time_base_den", json!(44100)),
        ("/info/audio_frame_size", json!(960)),
        ("/info/video_profile", json!(77)),
        ("/info/audio_profile", json!(2)),
        ("/info/video_has_b_frames", json!(-1)),
        ("/info/video_has_b_frames", json!(1)),
        ("/info/video_max_b_frames", json!(2)),
        ("/info/video_gop_size", json!(30)),
        ("/info/audio_initial_padding", json!(-1)),
        ("/info/audio_initial_padding", json!(0)),
        ("/info/audio_initial_padding", json!(2_048)),
        ("/info/audio_trailing_padding", json!(8193)),
        ("/info/requested_mode", json!("software")),
        ("/info/video_bitrate", json!(1)),
        ("/info/audio_bitrate", json!(128000)),
        ("/info/maximum_moov_bytes", json!(0)),
    ] {
        let mut changed = original.clone();
        *changed["manifest"]["report"].pointer_mut(path).unwrap() = value;
        assert!(
            EncodedProtocol::read_response(&mut Cursor::new(wire(&changed))).is_err(),
            "accepted {path}"
        );
    }
}

#[test]
fn historical_codec_only_priming_cannot_enter_current_worker_admission() {
    let mut old = manifest();
    old.report.info.abi_version = 1;
    old.report.audio_packets -= 1;
    old.validate_retained().unwrap();
    assert!(old.validate().is_err());
    assert!(old.validate_for(limits()).is_err());
    assert!(
        EncodedProtocol::from_request(&request())
            .unwrap()
            .classify(&completed(old.clone()))
            .is_err()
    );
    old.report.info.abi_version = 2;
    assert!(old.validate_retained().is_err());
    old.report.info.abi_version = 3;
    assert!(old.validate_retained().is_err());
}

#[test]
fn completion_rejects_media_duration_correction_for_an_incompatible_clock() {
    let mut manifest = manifest();
    manifest.report.video_media_duration_correction =
        Some(deadpan_encode::VideoMediaDurationCorrection {
            previous_ticks: 2_002,
            corrected_ticks: 1_001,
            first_cts: 2_002,
            minimum_cts: 1_001,
        });
    // Arithmetic agrees, but the captured encoder has no B frames.
    assert!(manifest.validate().is_err());
    let message = serde_json::to_value(completed(manifest)).unwrap();
    assert!(EncodedProtocol::read_response(&mut Cursor::new(wire(&message))).is_err());
}

#[test]
fn completed_manifest_binds_identity_contract_artifact_and_all_host_budgets() {
    let protocol = EncodedProtocol::from_request(&request()).unwrap();
    let mut changed = manifest();
    changed.document_sha256 = hash('c');
    assert!(protocol.classify(&completed(changed)).is_err());
    let mut changed = manifest();
    changed.contract.picture.revision_id = RevisionId::new("later").unwrap();
    assert!(protocol.classify(&completed(changed)).is_err());
    let mut changed = manifest();
    changed.contract.choice.mode = EncoderMode::Software;
    changed.report.info.requested_mode = EncoderMode::Software;
    changed.validate().unwrap();
    assert!(protocol.classify(&completed(changed)).is_err());
    for (path, bytes) in [("output/other.mp4", 4096), (MOVIE_REF, 4095)] {
        let mut changed = manifest();
        changed.movie =
            WorkspaceArtifact::new(WorkspaceRef::new(path).unwrap(), hash('b'), bytes).unwrap();
        assert!(protocol.classify(&completed(changed)).is_err());
    }
    let mut changed = manifest();
    changed.report.info.maximum_moov_bytes = moov_bound(limits().maximum_packets + 1).unwrap();
    changed.validate().unwrap();
    assert!(protocol.classify(&completed(changed)).is_err());
    let mut changed = manifest();
    changed.report.output_bytes = limits().maximum_output_bytes + 1;
    changed.movie = WorkspaceArtifact::new(
        WorkspaceRef::new(MOVIE_REF).unwrap(),
        hash('b'),
        changed.report.output_bytes,
    )
    .unwrap();
    changed.validate().unwrap();
    assert!(protocol.classify(&completed(changed)).is_err());
    let mut request = request();
    if let EncodedHostMessage::Prepare { limits, .. } = &mut request {
        limits.maximum_packet_bytes = 1;
    }
    let strict_packets = EncodedProtocol::from_request(&request).unwrap();
    assert!(strict_packets.classify(&completed(manifest())).is_err());
}

#[test]
fn adversarial_encoded_render_frames() {
    let cancel = EncodedProtocol::from_request(&request())
        .unwrap()
        .cancellation();
    crate::adversarial::protocol::<EncodedProtocol>(
        "cli-encoded-protocol",
        vec![request(), cancel],
        vec![completed(manifest())],
        |reader| read_host_message(reader),
    );
}
