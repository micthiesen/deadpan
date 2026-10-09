#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use deadpan_core::{AssetId, SourceFrameId, TerminalProvenance};
use deadpan_media::ConversionError;
use deadpan_media::source_index::{SourceContentIdentity, SourceIndexSnapshot};
use deadpan_media::source_input::{SourceInputError, VerifiedSourceInput};
use deadpan_media::source_session::{SourceSession, SourceSessionError, SourceSessionLimits};
use sha2::{Digest, Sha256};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-media-worker/tests/fixtures")
            .join(format!("{name}.mp4")),
    )
    .unwrap()
}

fn identity(bytes: &[u8]) -> SourceContentIdentity {
    SourceContentIdentity::new(Sha256::digest(bytes).into(), bytes.len() as u64).unwrap()
}

fn open(bytes: &[u8], limits: SourceSessionLimits) -> SourceSession {
    SourceSession::open_verified(
        &mut Cursor::new(bytes),
        identity(bytes),
        AssetId::new("source").unwrap(),
        limits,
        &AtomicBool::new(false),
    )
    .unwrap()
}

// Original authored fixture formula, independent of FFmpeg's output routines.
fn expected_rgba(frame: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    for y in 0..2 {
        for x in 0..4 {
            bytes.extend_from_slice(&[
                ((17 * frame + 31 * x + 7 * y + 3) % 256) as u8,
                ((29 * frame + 5 * x + 47 * y + 11) % 256) as u8,
                ((43 * frame + 13 * x + 19 * y + 23) % 256) as u8,
                255,
            ]);
        }
    }
    bytes
}

#[test]
fn indexed_original_frames_seek_exactly_and_own_pixels_after_source_and_decoder_loss() {
    for (name, count, rate_num, rate_den) in [
        ("rgb25_24", 25, 24, 1),
        ("rgb30_30000_1001", 30, 30_000, 1001),
    ] {
        let mut original = fixture(name);
        let mut session = open(&original, SourceSessionLimits::default());
        // Session input is a private checked copy, not a borrowed source buffer.
        original.fill(0);
        drop(original);
        let index = session.index();
        assert_eq!(index.index().frames().len(), count as usize);
        assert_eq!(
            index.index().terminal_provenance(),
            TerminalProvenance::DecodedFrameDuration
        );
        let saved = index.to_json().unwrap();
        assert_eq!(&SourceIndexSnapshot::from_json(&saved).unwrap(), index);
        let cancelled = AtomicBool::new(false);
        for id in (0..count).chain([count - 1, 0, 1, 17, 3, count - 1]) {
            let frame = session
                .frame(SourceFrameId(id), Duration::from_secs(2), &cancelled)
                .unwrap();
            assert_eq!(frame.rgba, expected_rgba(id), "{name} frame {id}");
            let clock = session.index().index().time_base();
            assert_eq!(
                i128::from(frame.metadata.pts) * i128::from(clock.numerator()) * rate_num,
                i128::from(id) * rate_den * i128::from(clock.denominator())
            );
        }
        let retained = session
            .frame(SourceFrameId(0), Duration::from_secs(2), &cancelled)
            .unwrap();
        session
            .frame(SourceFrameId(count - 1), Duration::from_secs(2), &cancelled)
            .unwrap();
        drop(session);
        assert_eq!(retained.rgba, expected_rgba(0));
    }
}

#[test]
fn natural_picture_plan_selects_and_decodes_original_pixels_after_trim_rounding() {
    use std::collections::BTreeMap;

    use deadpan_core::{
        AssetRecord, AudioSample, BeatNode, ColorPolicy, Command, CommandRequest, EndpointPolicy,
        ExactRatio, FrameDuration, FrameRate, LinkRelation, NodeId, NodeKind, PresentationBasis,
        ProjectDocument, ProjectFrame, ProjectId, RevisionId, SourceAudioMapping, SourceNode,
        SourceSpan, SourceTimestamp, SourceVideo, SourceVideoMapping, Subtree, apply,
    };
    use deadpan_plan::RenderPlan;

    let bytes = fixture("rgb25_24");
    let mut session = open(&bytes, SourceSessionLimits::default());
    let index = session.index().index();
    let clock = index.time_base();
    let full = SourceSpan::new(
        SourceTimestamp {
            ticks: index.frames()[0].pts,
            time_base: clock,
        },
        SourceTimestamp {
            ticks: index.terminal_end(),
            time_base: clock,
        },
    )
    .unwrap();
    // Trim one frame from each end. The remaining 23 source frames at 24 fps
    // occupy 28750/1001 project frames, rounded once to 29.
    let selected = SourceSpan::new(
        SourceTimestamp {
            ticks: index.frames()[1].pts,
            time_base: clock,
        },
        SourceTimestamp {
            ticks: index.frames()[24].pts,
            time_base: clock,
        },
    )
    .unwrap();
    let rate = FrameRate::new(30000, 1001).unwrap();
    let mapping = SourceVideoMapping::natural_rate(selected, rate, EndpointPolicy::Reject).unwrap();
    let duration = FrameDuration::new(29).unwrap();
    assert_eq!(
        mapping.duration_frames(duration).unwrap(),
        ExactRatio::new(28750, 1001).unwrap()
    );
    let asset = AssetId::new("source").unwrap();
    let root = NodeId::new("root").unwrap();
    let leaf = NodeId::new("clip").unwrap();
    let mut document = ProjectDocument::new(
        ProjectId::new("native-picture-mapping").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 4,
            height: 2,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        root.clone(),
    )
    .unwrap();
    for (revision, command) in [
        (
            "asset",
            Command::AddAsset {
                id: asset.clone(),
                asset: AssetRecord {
                    source_qualification: None,
                    label: "Measured RGB fixture".into(),
                    content_hash: blake3::hash(&bytes).to_hex().to_string(),
                    video: Some(full),
                    audio: None,
                    still_image: false,
                    frame_count: Some(FrameDuration::new(25).unwrap()),
                },
            },
        ),
        (
            "insert",
            Command::Insert {
                parent: root,
                index: 0,
                subtree: Subtree {
                    root: leaf.clone(),
                    overrides: Default::default(),
                    gap_overrides: Default::default(),
                    nodes: BTreeMap::from([(
                        leaf.clone(),
                        BeatNode {
                            audio_treatments: Default::default(),
                            framing: None,
                            audio_editorial_edges: Default::default(),
                            audio_edges: Default::default(),
                            label: "Natural trimmed picture".into(),
                            kind: NodeKind::Source {
                                source: SourceNode {
                                    duration,
                                    edit_window: None,
                                    video: SourceVideo::Stream {
                                        asset,
                                        span: selected,
                                    },
                                    video_mapping: mapping,
                                    audio: None,
                                    audio_mapping: SourceAudioMapping::FitBeat,
                                    audio_offset: AudioSample(0),
                                    link: LinkRelation::Independent,
                                },
                            },
                            cutaways: Vec::new(),
                            captions: Vec::new(),
                        },
                    )]),
                },
            },
        ),
    ] {
        let transaction = apply(
            &document,
            &CommandRequest {
                project_id: document.project_id().clone(),
                expected_revision: document.revision_id().clone(),
                new_revision: RevisionId::new(revision).unwrap(),
                command,
            },
        )
        .unwrap();
        document = transaction.forward.apply(&document).unwrap();
    }
    let natural = RenderPlan::compile(&document).unwrap();
    let transaction = apply(
        &document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new("fit").unwrap(),
            command: Command::SetSourceVideoMapping {
                node: leaf,
                mapping: SourceVideoMapping::FitBeat,
            },
        },
    )
    .unwrap();
    let fit = RenderPlan::compile(&transaction.forward.apply(&document).unwrap()).unwrap();
    let cancelled = AtomicBool::new(false);
    let mut differing_frames = Vec::new();
    for frame in 0..29 {
        let index = session.index().index();
        let selected = natural
            .picture(ProjectFrame(frame))
            .unwrap()
            .picture
            .select_source_frame(index)
            .unwrap()
            .identity;
        // Independent integer oracle: source frame coordinate = 1 +
        // project center * 24 * 1001/30000. Floor at the original PTS boundary.
        let expected = u64::try_from(1 + 1001 * (2 * frame + 1) / 2500).unwrap();
        assert_eq!(selected, SourceFrameId(expected));
        let old = fit
            .picture(ProjectFrame(frame))
            .unwrap()
            .picture
            .select_source_frame(index)
            .unwrap()
            .identity;
        if old != selected {
            differing_frames.push(frame);
        }
        let decoded = session
            .frame(selected, Duration::from_secs(2), &cancelled)
            .unwrap();
        assert_eq!(decoded.rgba, expected_rgba(expected));
    }
    assert_eq!(differing_frames, [2, 7, 12, 17, 22, 27]);
}

#[test]
fn invalid_native_limits_fail_before_any_snapshot_read() {
    struct NoRead;
    impl std::io::Read for NoRead {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            panic!("invalid native limits must not start snapshot I/O");
        }
    }
    let defaults = deadpan_source::DecodeLimits::default();
    for decode in [
        deadpan_source::DecodeLimits {
            max_input_bytes: 64 * 1024 * 1024 * 1024 + 1,
            ..defaults
        },
        deadpan_source::DecodeLimits {
            max_frames: 0,
            ..defaults
        },
        deadpan_source::DecodeLimits {
            max_packets: 40_000_001,
            ..defaults
        },
        deadpan_source::DecodeLimits {
            max_io_bytes_per_call: 1024 * 1024 * 1024 + 1,
            ..defaults
        },
        deadpan_source::DecodeLimits {
            max_packet_bytes: 16 * 1024 * 1024 + 1,
            ..defaults
        },
        deadpan_source::DecodeLimits {
            max_pixels: 8192 * 8192 + 1,
            ..defaults
        },
        deadpan_source::DecodeLimits {
            max_dimension: 8193,
            ..defaults
        },
        deadpan_source::DecodeLimits {
            max_packets_per_frame: 10_001,
            ..defaults
        },
    ] {
        assert!(matches!(
            SourceSession::open_verified(
                &mut NoRead,
                SourceContentIdentity::new([0; 32], 1).unwrap(),
                AssetId::new("source").unwrap(),
                SourceSessionLimits {
                    decode,
                    ..Default::default()
                },
                &AtomicBool::new(false),
            ),
            Err(SourceSessionError::Native(
                deadpan_source::SourceDecodeError::InvalidConfiguration(_)
            ))
        ));
    }
}

#[test]
fn shared_verified_bytes_survive_original_loss_and_independent_decoder_positions() {
    let mut bytes = fixture("rgb25_24");
    let cancelled = AtomicBool::new(false);
    let input = VerifiedSourceInput::copy_verified(
        &mut Cursor::new(&bytes),
        identity(&bytes),
        bytes.len() as u64,
        Duration::from_secs(2),
        &cancelled,
    )
    .unwrap();
    let content = input.identity();
    bytes.fill(0);
    let mut first = SourceSession::open_input(
        input.clone(),
        AssetId::new("first").unwrap(),
        SourceSessionLimits::default(),
        &cancelled,
    )
    .unwrap();
    let mut second = SourceSession::open_input(
        input,
        AssetId::new("second").unwrap(),
        SourceSessionLimits::default(),
        &cancelled,
    )
    .unwrap();
    assert_eq!(first.index().content(), content);
    assert_eq!(second.index().content(), content);
    for (left, right) in [(0, 24), (1, 0), (23, 12), (0, 13)] {
        assert_eq!(
            first
                .frame(SourceFrameId(left), Duration::from_secs(2), &cancelled)
                .unwrap()
                .rgba,
            expected_rgba(left)
        );
        assert_eq!(
            second
                .frame(SourceFrameId(right), Duration::from_secs(2), &cancelled)
                .unwrap()
                .rgba,
            expected_rgba(right)
        );
    }
    drop(first);
    assert_eq!(
        second
            .frame(SourceFrameId(14), Duration::from_secs(2), &cancelled)
            .unwrap()
            .rgba,
        expected_rgba(14)
    );
}

#[test]
fn shared_input_checks_limits_before_reading_and_identity_before_publication() {
    struct NoRead;
    impl std::io::Read for NoRead {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            panic!("preflight must reject without reading");
        }
    }
    let content = SourceContentIdentity::new([0; 32], 10).unwrap();
    for (maximum, timeout) in [
        (0, Duration::from_secs(1)),
        (9, Duration::from_secs(1)),
        (65 * 1024 * 1024 * 1024, Duration::from_secs(1)),
        (10, Duration::ZERO),
        (10, Duration::from_secs(86401)),
    ] {
        assert!(matches!(
            VerifiedSourceInput::copy_verified(
                &mut NoRead,
                content,
                maximum,
                timeout,
                &AtomicBool::new(false)
            ),
            Err(SourceInputError::Limits)
        ));
    }
    assert!(matches!(
        VerifiedSourceInput::copy_verified(
            &mut NoRead,
            content,
            10,
            Duration::from_secs(1),
            &AtomicBool::new(true)
        ),
        Err(SourceInputError::Snapshot(ConversionError::Cancelled))
    ));
    assert!(matches!(
        VerifiedSourceInput::copy_verified(
            &mut Cursor::new([0; 10]),
            content,
            10,
            Duration::from_secs(1),
            &AtomicBool::new(false)
        ),
        Err(SourceInputError::Snapshot(ConversionError::InputIdentity))
    ));
}

#[test]
fn hash_length_cancellation_and_index_budgets_fail_without_inventing_an_index() {
    let bytes = fixture("rgb25_24");
    let correct = identity(&bytes);
    for wrong in [
        SourceContentIdentity::new([0; 32], correct.byte_length()).unwrap(),
        SourceContentIdentity::new(correct.sha256(), correct.byte_length() - 1).unwrap(),
        SourceContentIdentity::new(correct.sha256(), correct.byte_length() + 1).unwrap(),
    ] {
        assert!(matches!(
            SourceSession::open_verified(
                &mut Cursor::new(&bytes),
                wrong,
                AssetId::new("source").unwrap(),
                SourceSessionLimits::default(),
                &AtomicBool::new(false)
            ),
            Err(SourceSessionError::Snapshot(ConversionError::InputIdentity))
        ));
    }
    let cancelled = AtomicBool::new(true);
    let mut reader = Cursor::new(&bytes);
    assert!(matches!(
        SourceSession::open_verified(
            &mut reader,
            correct,
            AssetId::new("source").unwrap(),
            SourceSessionLimits::default(),
            &cancelled
        ),
        Err(SourceSessionError::Snapshot(ConversionError::Cancelled))
    ));
    assert_eq!(reader.position(), 0);
    for limits in [
        SourceSessionLimits {
            maximum_index_frames: 1,
            ..Default::default()
        },
        SourceSessionLimits {
            maximum_index_bytes: 1,
            ..Default::default()
        },
    ] {
        assert!(matches!(
            SourceSession::open_verified(
                &mut Cursor::new(&bytes),
                correct,
                AssetId::new("source").unwrap(),
                limits,
                &AtomicBool::new(false)
            ),
            Err(SourceSessionError::Limits(_))
        ));
    }
}

#[test]
fn cancelled_or_budgeted_seek_reopens_only_the_decoder_and_preserves_the_index() {
    let bytes = fixture("rgb25_24");
    let mut session = open(
        &bytes,
        SourceSessionLimits {
            maximum_seek_frames: 1,
            ..Default::default()
        },
    );
    let before = session.index().to_json().unwrap();
    let cancelled = AtomicBool::new(true);
    assert!(
        session
            .frame(SourceFrameId(0), Duration::from_secs(2), &cancelled)
            .is_err()
    );
    cancelled.store(false, Ordering::Release);
    assert_eq!(
        session
            .frame(SourceFrameId(0), Duration::from_secs(2), &cancelled)
            .unwrap()
            .rgba,
        expected_rgba(0)
    );
    assert!(matches!(
        session.frame(SourceFrameId(24), Duration::from_secs(2), &cancelled),
        Err(SourceSessionError::SeekLimit)
    ));
    assert_eq!(
        session
            .frame(SourceFrameId(0), Duration::from_secs(2), &cancelled)
            .unwrap()
            .rgba,
        expected_rgba(0)
    );
    assert_eq!(session.index().to_json().unwrap(), before);
    assert!(matches!(
        session.frame(SourceFrameId(25), Duration::from_secs(2), &cancelled),
        Err(SourceSessionError::MissingFrame(_))
    ));
}

#[test]
fn unqualified_tags_and_corrupt_frames_cannot_create_a_source_session() {
    for name in ["rgb1_24_no_tags", "rgb1_24_corrupt"] {
        let bytes = fixture(name);
        assert!(
            SourceSession::open_verified(
                &mut Cursor::new(&bytes),
                identity(&bytes),
                AssetId::new("source").unwrap(),
                SourceSessionLimits::default(),
                &AtomicBool::new(false),
            )
            .is_err(),
            "{name}"
        );
    }
}

fn source_fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-source/tests/fixtures")
            .join(name),
    )
    .unwrap()
}

#[test]
fn vfr_final_interval_retains_encoded_duration_without_using_the_audio_endpoint() {
    use deadpan_core::{EndpointPolicy, ExactRatio, SourcePoint};

    let cancelled = AtomicBool::new(false);
    let mut retained_last_hash = None;
    for (name, duration, terminal) in [
        ("vfr.mp4", 1001, 238_238),
        ("vfr-long-terminal.mp4", 3003, 240_240),
    ] {
        let mut session = open(&source_fixture(name), SourceSessionLimits::default());
        let index = session.index().index();
        assert_eq!(index.frames().len(), 120, "{name}");
        assert_eq!(index.terminal_end(), terminal, "{name}");
        assert_eq!(
            index.terminal_provenance(),
            TerminalProvenance::DecodedFrameDuration
        );
        assert_eq!(index.frames()[119].reported_duration, Some(duration));
        assert_eq!(
            index.interval(SourceFrameId(119)).unwrap(),
            (237_237, terminal)
        );
        let point = |ticks| SourcePoint {
            ticks: ExactRatio::integer(ticks),
            time_base: index.time_base(),
        };
        assert_eq!(
            index
                .select(point(terminal - 1), EndpointPolicy::Reject)
                .unwrap()
                .identity,
            SourceFrameId(119)
        );
        assert!(
            index
                .select(point(terminal), EndpointPolicy::Reject)
                .is_err(),
            "{name}: half-open endpoint"
        );
        assert!(
            index
                .select(point(240_240), EndpointPolicy::Reject)
                .is_err(),
            "{name}: the audio endpoint is outside the half-open video span"
        );
        let saved = session.index().to_json().unwrap();
        assert_eq!(
            SourceIndexSnapshot::from_json(&saved).unwrap(),
            *session.index()
        );

        for frame in [119, 0, 119] {
            let picture = session
                .frame(SourceFrameId(frame), Duration::from_secs(10), &cancelled)
                .unwrap();
            if frame == 119 {
                assert_eq!(picture.metadata.pts, 237_237);
                assert_eq!(
                    picture.metadata.reported_duration,
                    Some(duration),
                    "{name} seek"
                );
                let hash = blake3::hash(&picture.rgba);
                if let Some(expected) = retained_last_hash {
                    assert_eq!(hash, expected, "timing patch must retain picture bytes");
                } else {
                    retained_last_hash = Some(hash);
                }
            }
        }
    }
}

/// Forward continuation past skipped pictures, incremental repositioning with
/// yields, finishing a reposition through a later request and seeking before
/// an unfinished target all return exactly the pictures of a sequential
/// single-threaded decode.
#[test]
fn forward_continuation_and_repositioning_return_sequential_pictures() {
    use deadpan_media::source_session::{DecodePlan, RepositionProgress};
    let cancelled = AtomicBool::new(false);
    let timeout = Duration::from_secs(20);
    for name in [
        "cfr-bframes.mp4",
        "pyramid-bframes.mp4",
        "offset-bframes.mp4",
        "vfr.mp4",
    ] {
        let bytes = source_fixture(name);
        let mut reference = open(&bytes, SourceSessionLimits::default());
        let count = reference.index().index().frames().len() as u64;
        assert!(count >= 12, "{name} has {count} pictures");
        let expected: Vec<_> = (0..count)
            .map(|frame| {
                blake3::hash(
                    &reference
                        .frame(SourceFrameId(frame), timeout, &cancelled)
                        .unwrap()
                        .rgba,
                )
            })
            .collect();
        for threads in [1, 8] {
            let mut limits = SourceSessionLimits::interactive();
            limits.decode.threads = threads;
            let mut session = open(&bytes, limits);
            let check = |session: &mut SourceSession, frame: u64| {
                let picture = session
                    .frame(SourceFrameId(frame), timeout, &cancelled)
                    .unwrap();
                assert_eq!(
                    blake3::hash(&picture.rgba),
                    expected[frame as usize],
                    "{name}, {threads} threads, picture {frame}"
                );
            };
            check(&mut session, 0);
            assert_eq!(session.decode_plan(SourceFrameId(0)), DecodePlan::Current);
            assert_eq!(
                session.decode_plan(SourceFrameId(1)),
                DecodePlan::Forward { pictures: 1 }
            );
            // Skipped pictures: whatever the plan, the picture is exact.
            for frame in [2, 4, 7] {
                check(&mut session, frame);
            }
            // Reposition toward a late target in single-picture steps.
            let target = count - 3;
            session
                .begin_reposition(SourceFrameId(target), timeout, &cancelled)
                .unwrap();
            assert_eq!(session.repositioning(), Some(SourceFrameId(target)));
            let mut steps = 0;
            loop {
                let mut budget = 1;
                let progress = session
                    .advance_reposition(timeout, &cancelled, &mut || {
                        if budget == 0 {
                            return true;
                        }
                        budget -= 1;
                        false
                    })
                    .unwrap();
                steps += 1;
                assert!(steps < 10_000);
                if matches!(progress, RepositionProgress::Reached { .. }) {
                    break;
                }
            }
            assert_eq!(session.repositioning(), None);
            assert_eq!(
                session.decode_plan(SourceFrameId(target)),
                DecodePlan::Current
            );
            check(&mut session, target);
            check(&mut session, target + 2);
            // An unfinished reposition finished by a later request.
            session
                .begin_reposition(SourceFrameId(5), timeout, &cancelled)
                .unwrap();
            let progress = session
                .advance_reposition(timeout, &cancelled, &mut || true)
                .unwrap();
            assert_eq!(progress, RepositionProgress::Pending { ordinals: 0 });
            assert!(matches!(
                session.decode_plan(SourceFrameId(6)),
                DecodePlan::Resume { after: 2, .. } | DecodePlan::Seek { .. }
            ));
            check(&mut session, 6);
            // A request before an unfinished target seeks afresh.
            session
                .begin_reposition(SourceFrameId(count - 1), timeout, &cancelled)
                .unwrap();
            assert!(matches!(
                session.decode_plan(SourceFrameId(3)),
                DecodePlan::Seek { .. }
            ));
            check(&mut session, 3);
            assert_eq!(session.repositioning(), None);
            // Advancing without a reposition is refused, not guessed.
            assert!(
                session
                    .advance_reposition(timeout, &cancelled, &mut || false)
                    .is_err()
            );
            check(&mut session, 4);
        }
    }
}
