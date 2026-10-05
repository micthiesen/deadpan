use std::io::{self, Cursor};

use deadpan_core::{
    AudioSample, ColorPolicy, ExactRatio, FrameRange, FrameRate, ProjectFrame, ProjectId,
    RevisionId,
};
use deadpan_encode::{
    AUDIO_FRAME_SAMPLES, AUDIO_SAMPLE_RATE, BFramePolicy, EncodeReport, EncoderInfo, EncoderMode,
};
use deadpan_jobs::{
    AttemptId, MAX_FRAME_BYTES, RequestId, Sha256, WorkspaceArtifact, WorkspaceRef,
};
use serde_json::{Value, json};

use crate::{
    encoded_render::protocol::{EncodedRenderContract, EncoderChoice, MOVIE_REF},
    render_worker::protocol::{RenderContract, RenderTimeBase},
};

use super::super::VerificationStage;
use super::*;

fn identity() -> RenderIdentity {
    RenderIdentity {
        request_id: RequestId::new("verify-1").unwrap(),
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

fn manifest() -> EncodedManifest {
    let contract = contract();
    let native = contract.native_contract().unwrap();
    let policy = native.policy();
    let report = EncodeReport {
        info: EncoderInfo {
            abi_version: 1,
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
            video_max_b_frames: 0,
            video_gop_size: i32::try_from(policy.gop_frames).unwrap(),
            audio_profile: 1,
            audio_initial_padding: 1_024,
            audio_trailing_padding: 0,
            requested_mode: native.mode(),
            video_bitrate: policy.video_bitrate,
            audio_bitrate: policy.audio_bitrate,
            maximum_moov_bytes: 1_048_576 + 256 * 128,
        },
        video_frames: native.video_frames(),
        audio_samples: native.audio_samples(),
        video_packets: native.video_frames(),
        audio_packets: native.audio_samples().div_ceil(1_024) + 1,
        output_bytes: 4_096,
        packet_bytes: 2_048,
        video_duration_from_contract_packets: native.video_frames(),
        faststart_read_opens: 1,
        faststart_read_closes: 1,
        video_eof: true,
        audio_eof: true,
    };
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

fn report() -> VerificationReport {
    let manifest = manifest();
    let native = manifest.contract.native_contract().unwrap();
    VerificationReport {
        policy_version: 1,
        contract: manifest.contract.clone(),
        document_sha256: manifest.document_sha256.clone(),
        movie_sha256: manifest.movie.sha256().clone(),
        movie_bytes: manifest.movie.byte_length(),
        video_frames: native.video_frames(),
        audio_samples: native.audio_samples(),
        video_packets: manifest.report.video_packets,
        audio_packets: manifest.report.audio_packets,
        gops: 1,
        fresh_gop_frames: native.video_frames(),
        maximum_b_run: 0,
        runtime_versions: [4_066_151, 4_064_103, 3_934_311],
        movie_timescale: native.policy().movie_timescale,
        video_edit_media_time: 0,
        audio_edit_media_time: 1_024,
        manual_first_sample: -1_024,
        manual_physical_samples: 3_072,
        ordinary_first_sample: 0,
        ordinary_physical_samples: 2_048,
        content_light: None,
    }
}

fn request() -> HostMessage {
    HostMessage::Inspect {
        protocol: VERSION,
        identity: identity(),
        cancellation_token: CancellationToken::new("cancel-exact-attempt").unwrap(),
        manifest: Box::new(manifest()),
        limits: VerificationLimits {
            maximum_bytes: 4_096,
            maximum_packets: 4,
        },
        timeout_millis: 30_000,
    }
}

fn completed(report: VerificationReport) -> WorkerMessage {
    WorkerMessage::Completed {
        protocol: VERSION,
        identity: identity(),
        report: Box::new(report),
    }
}

fn wire(value: &Value) -> Vec<u8> {
    let mut bytes = Vec::new();
    write_frame(&mut bytes, value).unwrap();
    bytes
}

#[test]
fn fragmented_wire_round_trip_preserves_the_exact_cancellation_target() {
    struct Fragmented(Cursor<Vec<u8>>);
    impl Read for Fragmented {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            let count = bytes.len().min(1);
            self.0.read(&mut bytes[..count])
        }
    }
    let request = request();
    let protocol = VerificationProtocol::from_request(&request).unwrap();
    let mut bytes = Vec::new();
    VerificationProtocol::write_request(&mut bytes, &request).unwrap();
    let mut reader = Fragmented(Cursor::new(bytes));
    assert_eq!(read_host(&mut reader).unwrap(), Some(request.clone()));
    assert_eq!(read_host(&mut reader).unwrap(), None);

    let HostMessage::Inspect {
        identity,
        cancellation_token,
        ..
    } = request
    else {
        unreachable!()
    };
    let cancellation = protocol.cancellation();
    assert_eq!(
        cancellation,
        HostMessage::Cancel {
            protocol: VERSION,
            identity,
            cancellation_token
        }
    );
    assert!(VerificationProtocol::from_request(&cancellation).is_err());
    let mut bytes = Vec::new();
    VerificationProtocol::write_request(&mut bytes, &cancellation).unwrap();
    assert_eq!(
        read_host(&mut Fragmented(Cursor::new(bytes))).unwrap(),
        Some(cancellation.clone())
    );
    let encoded_cancel = serde_json::to_value(cancellation).unwrap();
    for (path, value) in [
        ("/protocol", json!(2)),
        ("/cancellation_token", json!("")),
        ("/identity/attempt_id", json!("../stale")),
    ] {
        let mut invalid = encoded_cancel.clone();
        *invalid.pointer_mut(path).unwrap() = value;
        assert!(
            read_host(&mut Cursor::new(wire(&invalid))).is_err(),
            "accepted {path}"
        );
    }

    let reply = completed(report());
    let mut bytes = Vec::new();
    write_worker(&mut bytes, &reply).unwrap();
    let mut reader = Fragmented(Cursor::new(bytes));
    assert_eq!(
        VerificationProtocol::read_response(&mut reader).unwrap(),
        Some(reply)
    );
    assert_eq!(
        VerificationProtocol::read_response(&mut reader).unwrap(),
        None
    );
}

#[test]
fn unknown_fields_and_unrecognized_stages_fail_closed() {
    let request = serde_json::to_value(request()).unwrap();
    for path in [
        "",
        "/identity",
        "/manifest",
        "/manifest/contract",
        "/manifest/contract/picture",
        "/manifest/contract/choice",
        "/manifest/movie",
        "/manifest/report",
        "/manifest/report/info",
        "/limits",
    ] {
        let mut invalid = request.clone();
        invalid
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!(true));
        assert!(
            read_host(&mut Cursor::new(wire(&invalid))).is_err(),
            "accepted {path}"
        );
    }
    let reply = serde_json::to_value(completed(report())).unwrap();
    for path in [
        "",
        "/identity",
        "/report",
        "/report/contract",
        "/report/contract/picture",
        "/report/contract/choice",
    ] {
        let mut invalid = reply.clone();
        invalid
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!(true));
        assert!(
            VerificationProtocol::read_response(&mut Cursor::new(wire(&invalid))).is_err(),
            "accepted {path}"
        );
    }
    let invalid = json!({"event":"progress", "protocol":VERSION, "identity":identity(),
        "progress":{"stage":"published", "completed":1, "total":1}});
    assert!(VerificationProtocol::read_response(&mut Cursor::new(wire(&invalid))).is_err());
}

#[test]
fn request_rejects_invalid_scalars_and_insufficient_host_budgets() {
    let original = serde_json::to_value(request()).unwrap();
    for (path, value) in [
        ("/protocol", json!(0)),
        ("/protocol", json!(2)),
        ("/identity/request_id", json!("")),
        ("/identity/attempt_id", json!("x".repeat(129))),
        ("/cancellation_token", json!("../cancel")),
        ("/manifest/document_sha256", json!("a".repeat(63))),
        ("/timeout_millis", json!(0)),
        ("/timeout_millis", json!(86_400_001)),
        ("/limits/maximum_bytes", json!(0)),
        ("/limits/maximum_bytes", json!(4_095)),
        (
            "/limits/maximum_bytes",
            json!(64_u64 * 1024 * 1024 * 1024 + 1),
        ),
        ("/limits/maximum_packets", json!(0)),
        ("/limits/maximum_packets", json!(3)),
        ("/limits/maximum_packets", json!(1_000_001)),
    ] {
        let mut invalid = original.clone();
        *invalid.pointer_mut(path).unwrap() = value;
        assert!(
            read_host(&mut Cursor::new(wire(&invalid))).is_err(),
            "accepted {path}"
        );
    }
    for field in [
        "protocol",
        "identity",
        "manifest",
        "limits",
        "timeout_millis",
        "cancellation_token",
    ] {
        let mut invalid = original.clone();
        invalid.as_object_mut().unwrap().remove(field);
        assert!(
            read_host(&mut Cursor::new(wire(&invalid))).is_err(),
            "accepted missing {field}"
        );
    }
    let mut maximal = original;
    maximal["limits"] = serde_json::to_value(VerificationLimits::default()).unwrap();
    maximal["timeout_millis"] = json!(86_400_000);
    read_host(&mut Cursor::new(wire(&maximal))).unwrap();
}

#[test]
fn malformed_or_oversized_frames_and_diagnostics_are_rejected() {
    for value in [
        serde_json::to_value(request()).unwrap(),
        serde_json::to_value(completed(report())).unwrap(),
    ] {
        let bytes = wire(&value);
        for count in [1, 2, 3, 4, bytes.len() - 1] {
            assert!(read_host(&mut Cursor::new(&bytes[..count])).is_err());
            assert!(
                VerificationProtocol::read_response(&mut Cursor::new(&bytes[..count])).is_err()
            );
        }
    }
    let oversize = u32::try_from(MAX_FRAME_BYTES + 1).unwrap().to_be_bytes();
    assert!(read_host(&mut Cursor::new(oversize)).is_err());
    assert!(VerificationProtocol::read_response(&mut Cursor::new(oversize)).is_err());
    assert!(read_host(&mut Cursor::new([0_u8; 4])).is_err());
    assert!(VerificationProtocol::read_response(&mut Cursor::new([0_u8; 4])).is_err());
    let invalid = json!({"event":"failed", "protocol":VERSION, "identity":identity(),
        "diagnostic":"x".repeat(deadpan_jobs::MAX_DIAGNOSTIC_BYTES + 1)});
    assert!(VerificationProtocol::read_response(&mut Cursor::new(wire(&invalid))).is_err());
}

#[test]
fn every_response_binds_the_complete_attempt_identity() {
    let protocol = VerificationProtocol::from_request(&request()).unwrap();
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
            WorkerMessage::Progress {
                protocol: VERSION,
                identity: altered.clone(),
                progress: VerificationProgress {
                    stage: VerificationStage::Pictures,
                    completed: 0,
                    total: 1,
                },
            },
            WorkerMessage::Completed {
                protocol: VERSION,
                identity: altered.clone(),
                report: Box::new(report()),
            },
            WorkerMessage::Failed {
                protocol: VERSION,
                identity: altered.clone(),
                diagnostic: Diagnostic::new("failed").unwrap(),
            },
            WorkerMessage::Cancelled {
                protocol: VERSION,
                identity: altered,
            },
        ] {
            assert!(protocol.classify(&response).is_err());
        }
    }
    assert_eq!(
        protocol.classify(&completed(report())).unwrap(),
        ResponseKind::Completed
    );
    assert_eq!(
        protocol
            .classify(&WorkerMessage::Cancelled {
                protocol: VERSION,
                identity: identity()
            })
            .unwrap(),
        ResponseKind::Terminal
    );
    assert!(
        protocol
            .classify(&WorkerMessage::Cancelled {
                protocol: VERSION + 1,
                identity: identity()
            })
            .is_err()
    );
}

#[test]
fn progress_totals_are_bound_to_each_captured_stage() {
    let protocol = VerificationProtocol::from_request(&request()).unwrap();
    for (stage, expected) in [
        (VerificationStage::Packets, 4),
        (VerificationStage::Pictures, 1),
        (VerificationStage::ManualAudio, 1_601),
        (VerificationStage::OrdinaryAudio, 1_601),
    ] {
        for (completed, total, valid) in [
            (0, expected, true),
            (expected, expected, true),
            (expected + 1, expected, false),
            (0, expected + 1, false),
            (0, 0, false),
        ] {
            let response = WorkerMessage::Progress {
                protocol: VERSION,
                identity: identity(),
                progress: VerificationProgress {
                    stage,
                    completed,
                    total,
                },
            };
            assert_eq!(
                protocol.classify(&response).is_ok(),
                valid,
                "accepted {response:?}"
            );
        }
    }
}

#[test]
fn completion_cannot_change_the_committed_contract_or_private_bytes() {
    let protocol = VerificationProtocol::from_request(&request()).unwrap();
    let changes: [fn(&mut VerificationReport); 6] = [
        |value| value.contract.picture.project_id = ProjectId::new("other-project").unwrap(),
        |value| value.contract.picture.revision_id = RevisionId::new("later").unwrap(),
        |value| value.contract.choice.mode = EncoderMode::Software,
        |value| value.document_sha256 = hash('c'),
        |value| value.movie_sha256 = hash('c'),
        |value| value.movie_bytes -= 1,
    ];
    for change in changes {
        let mut changed = report();
        change(&mut changed);
        changed.validate(VerificationLimits::default()).unwrap();
        assert!(protocol.classify(&completed(changed)).is_err());
    }
    let value = report();
    assert_eq!(
        value.audio_samples, 1_601,
        "retain absolute project sample phase"
    );
    assert_eq!(value.manual_physical_samples, 3_072);
    assert_eq!(value.ordinary_physical_samples, 2_048);
    assert!(
        value
            .validate(VerificationLimits {
                maximum_bytes: 4_095,
                maximum_packets: 4
            })
            .is_err()
    );
    assert!(
        value
            .validate(VerificationLimits {
                maximum_bytes: 4_096,
                maximum_packets: 3
            })
            .is_err()
    );
}

#[test]
fn report_rejects_inconsistent_clocks_runtime_and_physical_sample_claims() {
    let original = serde_json::to_value(completed(report())).unwrap();
    for (path, value) in [
        ("/policy_version", json!(0)),
        ("/movie_bytes", json!(0)),
        ("/video_frames", json!(2)),
        ("/audio_samples", json!(1_602)),
        ("/video_packets", json!(0)),
        ("/audio_packets", json!(2)),
        ("/audio_packets", json!(u64::MAX)),
        ("/gops", json!(0)),
        ("/gops", json!(2)),
        ("/fresh_gop_frames", json!(0)),
        ("/maximum_b_run", json!(1)),
        ("/movie_timescale", json!(30_000)),
        ("/video_edit_media_time", json!(-1)),
        ("/video_edit_media_time", json!(1)),
        ("/video_edit_media_time", json!(i64::MAX)),
        ("/audio_edit_media_time", json!(0)),
        ("/audio_edit_media_time", json!(1_025)),
        ("/manual_first_sample", json!(0)),
        ("/ordinary_first_sample", json!(1)),
        ("/manual_physical_samples", json!(2_048)),
        ("/ordinary_physical_samples", json!(1_600)),
        ("/ordinary_physical_samples", json!(1_601)),
        ("/ordinary_physical_samples", json!(2_047)),
        ("/ordinary_physical_samples", json!(2_049)),
        ("/runtime_versions", json!([0, 4_064_103, 3_934_311])),
        (
            "/runtime_versions",
            json!([4_066_152, 4_064_103, 3_934_311]),
        ),
        (
            "/runtime_versions",
            json!([4_066_151, 4_064_104, 3_934_311]),
        ),
        (
            "/runtime_versions",
            json!([4_066_151, 4_064_103, 3_934_312]),
        ),
    ] {
        let mut invalid = original.clone();
        *invalid["report"].pointer_mut(path).unwrap() = value;
        assert!(
            VerificationProtocol::read_response(&mut Cursor::new(wire(&invalid))).is_err(),
            "accepted {path}: {invalid}"
        );
    }
}

#[test]
fn reordering_observations_must_use_whole_frames_within_the_captured_policy() {
    let mut value = report();
    value.contract.choice.mode = EncoderMode::Software;
    value.contract.choice.b_frames = BFramePolicy::TargetTwo;
    value.video_edit_media_time = 1_001;
    value.validate(VerificationLimits::default()).unwrap();
    for ticks in [1, 1_002, 3_003, i64::MAX] {
        let mut invalid = value.clone();
        invalid.video_edit_media_time = ticks;
        assert!(
            invalid.validate(VerificationLimits::default()).is_err(),
            "accepted {ticks}"
        );
    }
    value.maximum_b_run = 3;
    assert!(value.validate(VerificationLimits::default()).is_err());
}

#[test]
fn long_exports_require_enough_gops_and_actual_requested_reordering() {
    let mut value = report();
    let frames = 64;
    value.contract.picture.range = FrameRange::new(ProjectFrame(1), ProjectFrame(65)).unwrap();
    value.contract.picture.frame_count = frames;
    value.contract.picture.terminal_pts = 64 * 1_001;
    value.contract.picture.project_audio_end = value
        .contract
        .picture
        .frame_rate
        .audio_boundary(ProjectFrame(65))
        .unwrap();
    value.contract.choice.mode = EncoderMode::Software;
    value.contract.choice.b_frames = BFramePolicy::TargetTwo;
    let native = value.contract.native_contract().unwrap();
    value.video_frames = frames;
    value.video_packets = frames;
    value.fresh_gop_frames = frames;
    value.audio_samples = native.audio_samples();
    value.audio_packets = native.audio_samples().div_ceil(1_024) + 1;
    value.manual_physical_samples = value.audio_packets * 1_024;
    value.ordinary_physical_samples = native.audio_samples().div_ceil(1_024) * 1_024;
    value.maximum_b_run = 1;
    value.video_edit_media_time = 1_001;
    value.gops = frames.div_ceil(u64::from(native.policy().gop_frames) + 1);
    assert!(value.gops > 1);
    value.validate(VerificationLimits::default()).unwrap();

    let mut insufficient = value.clone();
    insufficient.gops -= 1;
    assert!(
        insufficient
            .validate(VerificationLimits::default())
            .is_err()
    );
    let mut no_observed_reordering = value.clone();
    no_observed_reordering.maximum_b_run = 0;
    assert!(
        no_observed_reordering
            .validate(VerificationLimits::default())
            .is_err()
    );
    value.contract.choice.b_frames = BFramePolicy::None;
    value.maximum_b_run = 0;
    value.video_edit_media_time = 0;
    value.validate(VerificationLimits::default()).unwrap();
}
