use std::os::fd::OwnedFd;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::net::UnixStream;
use std::sync::atomic::Ordering;

use deadpan_jobs::{AttemptId, RequestId, write_frame};

use super::*;

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(2)
}

fn identity() -> RenderIdentity {
    RenderIdentity {
        request_id: RequestId::new("encoded-test").unwrap(),
        attempt_id: AttemptId::new("encoded-attempt").unwrap(),
    }
}

fn control_pair() -> (UnixStream, ControlReader) {
    let (host, child) = UnixStream::pair().unwrap();
    let reader = ControlReader::new(
        File::from(OwnedFd::from(child)),
        Arc::new(AtomicBool::new(false)),
        deadline(),
    )
    .unwrap();
    (host, reader)
}

fn start_pump(reader: ControlReader) -> ControlPump {
    ControlPump::start_with(reader, deadline(), |reader| {
        receive_control(
            reader,
            &identity(),
            &CancellationToken::new("expected-token").unwrap(),
        )
    })
    .unwrap()
}

#[test]
fn queued_encoded_cancellation_and_invalid_controls_are_drained_before_terminal() {
    for (version, token, valid) in [
        (PROTOCOL_VERSION, "expected-token", true),
        (PROTOCOL_VERSION, "wrong-token", false),
        (99, "expected-token", false),
    ] {
        let (mut host, reader) = control_pair();
        write_frame(
            &mut host,
            &EncodedHostMessage::Cancel {
                protocol: version,
                identity: identity(),
                cancellation_token: CancellationToken::new(token).unwrap(),
            },
        )
        .unwrap();
        let mut pump = start_pump(reader);
        if valid {
            assert_eq!(pump.finish(), Ok(ControlEnd::Cancelled));
        } else {
            assert!(pump.finish().is_err());
        }
        assert!(pump.cancelled().load(Ordering::Acquire));
    }
    let (mut host, reader) = control_pair();
    host.write_all(&[0, 0]).unwrap();
    let mut pump = start_pump(reader);
    assert!(pump.finish().unwrap_err().contains("incomplete"));
    let (host, reader) = control_pair();
    drop(host);
    assert!(start_pump(reader).finish().unwrap_err().contains("closed"));
}

#[test]
fn encoded_output_is_private_exclusive_contained_and_hashed_through_its_descriptor() {
    let workspace = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    symlink(outside.path(), workspace.path().join("output")).unwrap();
    assert!(open_output(workspace.path(), WorkerOutput::Movie).is_err());
    assert!(!outside.path().join("movie.mp4").exists());
    std::fs::remove_file(workspace.path().join("output")).unwrap();
    std::fs::create_dir(workspace.path().join("output")).unwrap();
    let original = outside.path().join("original");
    std::fs::write(&original, b"preserve").unwrap();
    let path = workspace.path().join("output/movie.mp4");
    symlink(&original, &path).unwrap();
    assert!(open_output(workspace.path(), WorkerOutput::Movie).is_err());
    assert_eq!(std::fs::read(&original).unwrap(), b"preserve");
    std::fs::remove_file(&path).unwrap();
    let mut file = open_output(workspace.path(), WorkerOutput::Movie).unwrap();
    assert_eq!(file.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    file.write_all(b"abc").unwrap();
    assert!(open_output(workspace.path(), WorkerOutput::Movie).is_err());
    // A same-size replacement cannot change which descriptor the worker hashes.
    std::fs::rename(&path, workspace.path().join("output/detached")).unwrap();
    std::fs::write(&path, b"xyz").unwrap();
    let hash = hash_movie(&mut file, 3, 3, &AtomicBool::new(false), deadline()).unwrap();
    assert_eq!(
        hash,
        Sha256::new("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_owned())
            .unwrap()
    );
    assert!(hash_movie(&mut file, 3, 2, &AtomicBool::new(false), deadline()).is_err());
    assert!(hash_movie(&mut file, 4, 4, &AtomicBool::new(false), deadline()).is_err());
}

#[test]
fn hashing_checks_exact_extent_and_interrupts_between_bounded_reads() {
    let cancelled = AtomicBool::new(false);
    for (bytes, expected) in [(b"abc".as_slice(), 2), (b"ab".as_slice(), 3)] {
        assert!(
            hash_exact(
                &mut io::Cursor::new(bytes),
                expected,
                &cancelled,
                deadline()
            )
            .is_err()
        );
    }
    struct CancellingReader<'a> {
        cancelled: &'a AtomicBool,
        calls: usize,
    }
    impl Read for CancellingReader<'_> {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            assert!(bytes.len() <= HASH_CHUNK_BYTES);
            self.calls += 1;
            bytes.fill(7);
            self.cancelled.store(true, Ordering::Release);
            Ok(bytes.len())
        }
    }
    let mut reader = CancellingReader {
        cancelled: &cancelled,
        calls: 0,
    };
    let expected = u64::try_from(HASH_CHUNK_BYTES + 1).unwrap();
    assert_eq!(
        hash_exact(&mut reader, expected, &cancelled, deadline())
            .unwrap_err()
            .kind,
        EncodedFailureKind::Control
    );
    assert_eq!(reader.calls, 1);
    cancelled.store(false, Ordering::Release);
    assert_eq!(
        hash_exact(&mut reader, expected, &cancelled, Instant::now())
            .unwrap_err()
            .kind,
        EncodedFailureKind::Control
    );
    assert_eq!(reader.calls, 1);
}

#[test]
fn fractional_range_rebases_absolute_audio_without_rounding_or_padding() {
    // At 30000/1001, [1,2) contains 1601 samples, beginning at B(1)=1602.
    assert_eq!(
        audio_start(AudioSample(1602), AudioSample(3203), 0, 1024).unwrap(),
        AudioSample(1602)
    );
    assert_eq!(
        audio_start(AudioSample(1602), AudioSample(3203), 1024, 577).unwrap(),
        AudioSample(2626)
    );
    assert!(audio_start(AudioSample(1602), AudioSample(3203), 1024, 578).is_err());
    assert!(audio_start(AudioSample(1602), AudioSample(3203), u64::MAX, 1).is_err());
    assert!(audio_start(AudioSample(i64::MAX), AudioSample(i64::MAX), 1, 1).is_err());
    assert!(audio_start(AudioSample(0), AudioSample(8000), 0, 1025).is_err());
    let samples = vec![[0.25, -0.75]; 577];
    let planar = PlanarInput::new(&samples, 577).unwrap();
    assert_eq!(planar.count, 577);
    assert!(
        planar.left[..planar.count]
            .iter()
            .all(|sample| *sample == 0.25)
    );
    assert!(
        planar.right[..planar.count]
            .iter()
            .all(|sample| *sample == -0.75)
    );
    assert!(PlanarInput::new(&samples, 576).is_err());
    assert!(PlanarInput::new(&[], 0).is_err());
    for sample in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(PlanarInput::new(&[[sample, 0.]], 1).is_err());
        assert!(PlanarInput::new(&[[0., sample]], 1).is_err());
    }
}

#[test]
fn native_failure_kinds_survive_worker_mapping_without_reading_prose() {
    use deadpan_encode::EncodeFailureKind;
    for (error, expected) in [
        (
            EncodeError::Native {
                code: "video_encoder_unavailable".into(),
                message: "opaque".into(),
            },
            EncodeFailureKind::EncoderUnavailable,
        ),
        (
            EncodeError::Native {
                code: "video_timestamp_order".into(),
                message: "opaque".into(),
            },
            EncodeFailureKind::VideoTimestampOrder,
        ),
        (
            EncodeError::Native {
                code: "codec_open".into(),
                message: "video_encoder_unavailable video_timestamp_order".into(),
            },
            EncodeFailureKind::Native,
        ),
        (
            EncodeError::Io(io::Error::other("video_encoder_unavailable")),
            EncodeFailureKind::Io,
        ),
        (
            EncodeError::Input("video_timestamp_order"),
            EncodeFailureKind::Input,
        ),
        (
            EncodeError::Configuration("video_encoder_unavailable"),
            EncodeFailureKind::Configuration,
        ),
        (
            EncodeError::Evidence("video_encoder_unavailable"),
            EncodeFailureKind::Evidence,
        ),
    ] {
        let text = error.to_string();
        let failure = encoder_failure(error);
        assert_eq!(failure.kind, EncodedFailureKind::Encoder(expected));
        assert_eq!(failure.diagnostic.as_str(), text);
    }
}

#[test]
fn misleading_output_io_diagnostics_cannot_claim_encoder_capability() {
    struct FailingReader;
    impl Read for FailingReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other(
                "native encoder video_encoder_unavailable: video_timestamp_order",
            ))
        }
    }
    let error = hash_exact(&mut FailingReader, 1, &AtomicBool::new(false), deadline()).unwrap_err();
    assert_eq!(error.kind, EncodedFailureKind::Output);
    assert!(
        error
            .diagnostic
            .as_str()
            .contains("video_encoder_unavailable")
    );
    for kind in [
        EncodedFailureKind::Control,
        EncodedFailureKind::Contract,
        EncodedFailureKind::Source,
        EncodedFailureKind::Picture,
        EncodedFailureKind::Audio,
    ] {
        assert_eq!(
            failure(kind, "video_encoder_unavailable video_timestamp_order").kind,
            kind
        );
    }
    assert_eq!(
        document_failure(crate::render_worker::RenderWorkerError::Worker(
            "video_encoder_unavailable".into()
        ))
        .kind,
        EncodedFailureKind::Contract
    );
    assert_eq!(
        document_failure(crate::render_worker::RenderWorkerError::Deadline).kind,
        EncodedFailureKind::Control
    );
}

#[test]
fn contract_and_source_admission_failures_are_typed_before_output_allocation() {
    use super::super::protocol::EncoderChoice;
    use crate::render_worker::protocol::{RenderContract, RenderTimeBase};
    use deadpan_core::{
        ColorPolicy, ExactRatio, FrameRange, FrameRate, ProjectFrame, ProjectId, RevisionId,
    };
    use deadpan_encode::{BFramePolicy, EncoderMode};

    let scratch = tempfile::tempdir().unwrap();
    let missing = scratch.path().join("video_encoder_unavailable.deadpan");
    let mut request = Request {
        identity: identity(),
        contract: EncodedRenderContract {
            picture: RenderContract {
                project_id: ProjectId::new("project").unwrap(),
                revision_id: RevisionId::new("committed").unwrap(),
                range: FrameRange::new(ProjectFrame(0), ProjectFrame(1)).unwrap(),
                canvas: [64, 48],
                raster: [64, 48],
                frame_rate: FrameRate::new(30, 1).unwrap(),
                color_policy: ColorPolicy::SdrRec709,
                time_base: RenderTimeBase {
                    numerator: 1,
                    denominator: 30,
                },
                frame_count: 1,
                terminal_pts: 1,
                project_audio_start: AudioSample(0),
                project_audio_end: AudioSample(1600),
                relative_aspect_error: ExactRatio::new(0, 1).unwrap(),
            },
            choice: EncoderChoice {
                mode: EncoderMode::Hardware,
                b_frames: BFramePolicy::None,
            },
        },
        document_sha256: Sha256::new("a".repeat(64)).unwrap(),
        limits: EncodeLimits::default(),
    };
    let mut output = Vec::new();
    let error = prepare(
        &missing,
        &request,
        &AtomicBool::new(false),
        deadline(),
        &mut output,
    )
    .unwrap_err();
    assert_eq!(error.kind, EncodedFailureKind::Source);
    request.contract.picture.frame_count = 0;
    let error = prepare(
        &missing,
        &request,
        &AtomicBool::new(false),
        deadline(),
        &mut output,
    )
    .unwrap_err();
    assert_eq!(error.kind, EncodedFailureKind::Contract);
    assert!(output.is_empty());
    assert!(!missing.exists());
}
