#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deadpan_core::{
    AssetId, AudioSample, EndpointPolicy, ExactRatio, FrameRate, IndexedSourceFrame, SourceFrameId,
    SourceFrameIndex, SourceTimeBase, TerminalProvenance,
};
use deadpan_media::audio_index::{
    AudioChannelLayout, AudioFrameObservation, AudioIndexSnapshot, AudioSkipSamples,
    AudioStreamDescriptor,
};
use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
use deadpan_media::source_import_timing::{
    CadenceConfidence, ImportAudioPolicy, ImportTimingError, MAX_CADENCE_INTERVALS,
    audio_only_basis, derive_import_timing, derive_presentation_basis,
};
use deadpan_media::source_index::{SourceContentIdentity, SourceIndexSnapshot};
use deadpan_media::source_input::VerifiedSourceInput;
use deadpan_media::source_session::{SourceSession, SourceSessionLimits};
use deadpan_source::{
    ColorMatrix, ColorMetadata, ColorPrimaries, ColorRange, ColorTransfer, SourceStreamInfo,
};
use sha2::{Digest, Sha256};

fn ratio(numerator: i128, denominator: i128) -> ExactRatio {
    ExactRatio::new(numerator, denominator).unwrap()
}

fn rate() -> FrameRate {
    FrameRate::new(30000, 1001).unwrap()
}

fn fixture_input(folder: &str, name: &str) -> VerifiedSourceInput {
    let bytes = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../native")
            .join(folder)
            .join(name),
    )
    .unwrap();
    let identity =
        SourceContentIdentity::new(Sha256::digest(&bytes).into(), bytes.len() as u64).unwrap();
    VerifiedSourceInput::copy_verified(
        &mut Cursor::new(bytes),
        identity,
        identity.byte_length(),
        Duration::from_secs(10),
        &AtomicBool::new(false),
    )
    .unwrap()
}

fn video_session(input: VerifiedSourceInput) -> SourceSession {
    SourceSession::open_input(
        input,
        AssetId::new("source").unwrap(),
        SourceSessionLimits::default(),
        &AtomicBool::new(false),
    )
    .unwrap()
}

fn audio_session(input: VerifiedSourceInput, stream: u32) -> AudioSession {
    AudioSession::open_input(
        input,
        stream,
        AudioSessionLimits::default(),
        &AtomicBool::new(false),
    )
    .unwrap()
}

#[test]
fn decoded_cfr_offset_and_vfr_keep_measured_tails_and_original_av_alignment() {
    for (name, origin, picture_start, picture_frames, audio_start, audio_end, beat_frames) in [
        (
            "cfr-bframes.mp4",
            ExactRatio::ZERO,
            ExactRatio::ZERO,
            120,
            0,
            192192,
            120,
        ),
        (
            "offset-bframes.mp4",
            ratio(2971, 1500),
            ratio(640, 1001),
            120,
            95072,
            288288,
            121,
        ),
        (
            "vfr.mp4",
            ExactRatio::ZERO,
            ExactRatio::ZERO,
            238,
            0,
            384384,
            240,
        ),
    ] {
        let input = fixture_input("deadpan-source/tests/fixtures", name);
        let video = video_session(input.clone());
        let audio = audio_session(input, 1);
        let timing =
            derive_import_timing(Some(video.index()), Some(audio.index()), rate()).unwrap();
        assert_eq!(timing.origin_seconds, origin, "{name}");
        assert_eq!(timing.duration.frames(), beat_frames, "{name}");
        assert_eq!(timing.video.unwrap().start_frames, picture_start);
        assert_eq!(
            timing.video.unwrap().duration_frames,
            ExactRatio::integer(picture_frames)
        );
        assert_eq!(timing.audio.unwrap().start_frames, ExactRatio::ZERO);
        assert_eq!(timing.audio.unwrap().span.start().ticks, audio_start);
        assert_eq!(timing.audio.unwrap().span.end().ticks, audio_end);
        assert_eq!(
            timing.audio_policy,
            ImportAudioPolicy::MeasuredAvailableCoverage
        );
        assert_eq!(timing.video_endpoints, EndpointPolicy::HoldAdjacent);
        let node = timing.source_node(AssetId::new("source").unwrap());
        assert_eq!(node.audio_offset, AudioSample(0));
        assert_eq!(node.video_mapping.start_frames(), picture_start);
        assert_eq!(node.audio_mapping.start_frames(), ExactRatio::ZERO);
        assert_eq!(node.video_mapping.endpoints(), EndpointPolicy::HoldAdjacent);
        if name == "offset-bframes.mp4" {
            assert_eq!(timing.video.unwrap().span.start().ticks, 60060);
            assert_eq!(timing.video.unwrap().span.end().ticks, 180180);
            assert_eq!(timing.audio.unwrap().duration_frames, ratio(120760, 1001));
            assert!(audio.index().observations()[0].skip_samples.is_none());
        }
        let candidate = derive_presentation_basis(video.index(), video.info()).unwrap();
        assert_eq!(candidate.basis.frame_rate, rate());
        assert_eq!(candidate.basis.width, video.info().width);
        assert_eq!(candidate.basis.height, video.info().height);
        if name == "vfr.mp4" {
            assert_eq!(
                candidate.cadence.confidence,
                CadenceConfidence::RepeatedIntegralVfr
            );
            assert_eq!(candidate.cadence.tied_modes, 2);
            assert_eq!(candidate.cadence.selected_interval_ticks, 1001);
        } else {
            assert_eq!(candidate.cadence.confidence, CadenceConfidence::ExactCfr);
        }
    }
}

#[test]
fn measured_av_candidates_drive_picture_plans_through_leading_and_trailing_holds() {
    use std::collections::BTreeMap;

    use deadpan_core::{
        AssetRecord, BeatNode, Command, CommandRequest, FrameDuration, NodeId, NodeKind,
        ProjectDocument, ProjectFrame, ProjectId, RevisionId, Subtree, apply,
    };
    use deadpan_plan::RenderPlan;

    for (name, first_held, last_held) in [("offset-bframes.mp4", 0, 120), ("vfr.mp4", 0, 239)] {
        let input = fixture_input("deadpan-source/tests/fixtures", name);
        let mut video = video_session(input.clone());
        let audio = audio_session(input, 1);
        let timing =
            derive_import_timing(Some(video.index()), Some(audio.index()), rate()).unwrap();
        let asset = AssetId::new("source").unwrap();
        let root = NodeId::new("root").unwrap();
        let leaf = NodeId::new("clip").unwrap();
        let bytes = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../native/deadpan-source/tests/fixtures")
                .join(name),
        )
        .unwrap();
        let mut document = ProjectDocument::new(
            ProjectId::new("measured-placement").unwrap(),
            RevisionId::new("initial").unwrap(),
            derive_presentation_basis(video.index(), video.info())
                .unwrap()
                .basis,
            root.clone(),
        )
        .unwrap();
        for (revision, command) in [
            (
                "asset",
                Command::AddAsset {
                    id: asset.clone(),
                    asset: AssetRecord {
                        label: name.into(),
                        content_hash: blake3::hash(&bytes).to_hex().to_string(),
                        video: timing.video.map(|placement| placement.span),
                        audio: timing.audio.map(|placement| placement.span),
                        still_image: false,
                        frame_count: Some(
                            FrameDuration::new(
                                i64::try_from(video.index().index().frames().len()).unwrap(),
                            )
                            .unwrap(),
                        ),
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
                        nodes: BTreeMap::from([(
                            leaf,
                            BeatNode {
                                label: name.into(),
                                kind: NodeKind::Source {
                                    source: timing.source_node(asset),
                                },
                            },
                        )]),
                    },
                },
            ),
        ] {
            let edit = apply(
                &document,
                &CommandRequest {
                    project_id: document.project_id().clone(),
                    expected_revision: document.revision_id().clone(),
                    new_revision: RevisionId::new(revision).unwrap(),
                    command,
                },
            )
            .unwrap();
            let next = edit.forward.apply(&document).unwrap();
            assert_eq!(edit.inverse.apply(&next).unwrap(), document);
            document = next;
        }
        let document = ProjectDocument::from_json(&document.to_json().unwrap()).unwrap();
        let plan = RenderPlan::compile(&document).unwrap();
        for (project_frame, expected) in [(first_held, 0), (last_held, 119)] {
            let sample = plan.picture(ProjectFrame(project_frame)).unwrap();
            let selected = sample
                .picture
                .select_source_frame(video.index().index())
                .unwrap()
                .clone();
            assert_eq!(selected.identity, SourceFrameId(expected), "{name}");
            let frame = video
                .frame(
                    selected.identity,
                    Duration::from_secs(2),
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(frame.metadata.pts, selected.pts);
        }
    }
}

#[test]
fn decoded_rgb_rotation_anamorphic_and_single_frame_basis_use_measured_geometry_and_cadence() {
    for (folder, name, width, height, fps, count) in [
        (
            "deadpan-media-worker/tests/fixtures",
            "rgb25_24.mp4",
            4,
            2,
            FrameRate::new(24, 1).unwrap(),
            25,
        ),
        (
            "deadpan-media-worker/tests/fixtures",
            "rgb30_30000_1001.mp4",
            4,
            2,
            rate(),
            30,
        ),
        (
            "deadpan-source/tests/fixtures",
            "rotated90.mp4",
            2,
            4,
            FrameRate::new(24, 1).unwrap(),
            1,
        ),
        // Matroska decoded terminal duration is 41 milliseconds. Nominal 24
        // fps is not available measured evidence for this one-frame fixture.
        (
            "deadpan-source/tests/fixtures",
            "anamorphic.mkv",
            8,
            2,
            FrameRate::new(1000, 41).unwrap(),
            1,
        ),
    ] {
        let video = video_session(fixture_input(folder, name));
        let candidate = derive_presentation_basis(video.index(), video.info()).unwrap();
        assert_eq!(
            (candidate.basis.width, candidate.basis.height),
            (width, height),
            "{name}"
        );
        assert_eq!(candidate.basis.frame_rate, fps, "{name}");
        assert_eq!(candidate.geometry.relative_aspect_error, ExactRatio::ZERO);
        let timing = derive_import_timing(Some(video.index()), None, fps).unwrap();
        assert_eq!(timing.duration.frames(), count);
        assert_eq!(
            timing.video.unwrap().duration_frames,
            ExactRatio::integer(count)
        );
        assert!(timing.audio.is_none());
        if count == 1 {
            assert_eq!(
                candidate.cadence.confidence,
                CadenceConfidence::SingleDecodedFrame
            );
        }
    }
}

#[test]
fn decoded_44100_audio_keeps_native_samples_and_exact_extent_on_audio_only_canvas() {
    let session = audio_session(
        fixture_input("deadpan-source/tests/audio-fixtures", "pcm-mono-44100.wav"),
        0,
    );
    let basis = audio_only_basis();
    assert_eq!((basis.width, basis.height), (1920, 1080));
    assert_eq!(basis.frame_rate, FrameRate::new(30, 1).unwrap());
    let timing = derive_import_timing(None, Some(session.index()), basis.frame_rate).unwrap();
    let audio = timing.audio.unwrap();
    assert_eq!(audio.span.start().ticks, 0);
    assert_eq!(audio.span.end().ticks, 44117);
    assert_eq!(
        audio.span.start().time_base,
        SourceTimeBase::new(1, 44100).unwrap()
    );
    assert_eq!(audio.duration_frames, ratio(44117, 1470));
    assert_eq!(timing.duration.frames(), 31);
    assert!(timing.video.is_none());
}

#[test]
fn an_existing_project_rate_changes_sampling_occupancy_but_never_original_duration() {
    let video = video_session(fixture_input(
        "deadpan-media-worker/tests/fixtures",
        "rgb25_24.mp4",
    ));
    let timing = derive_import_timing(Some(video.index()), None, rate()).unwrap();
    assert_eq!(timing.project_rate, rate());
    assert_eq!(timing.video.unwrap().duration_frames, ratio(31250, 1001));
    assert_eq!(timing.duration.frames(), 32);
    let selected = timing.video.unwrap().span;
    assert_eq!(
        selected.start().ticks,
        video.index().index().frames()[0].pts
    );
    assert_eq!(selected.end().ticks, video.index().index().terminal_end());
    let node = timing.source_node(AssetId::new("source").unwrap());
    assert_eq!(
        node.video_mapping.duration_frames(node.duration).unwrap(),
        ratio(31250, 1001)
    );
    assert_eq!(
        derive_presentation_basis(video.index(), video.info())
            .unwrap()
            .basis
            .frame_rate,
        FrameRate::new(24, 1).unwrap()
    );
}

fn identity() -> SourceContentIdentity {
    SourceContentIdentity::new([31; 32], 100).unwrap()
}

fn indexed_video(start: i64, intervals: &[i64], time_base: SourceTimeBase) -> SourceIndexSnapshot {
    let mut pts = start;
    let frames = intervals
        .iter()
        .enumerate()
        .map(|(ordinal, interval)| {
            let frame = IndexedSourceFrame {
                identity: SourceFrameId(ordinal as u64),
                pts,
                reported_duration: Some(*interval),
                keyframe: ordinal == 0,
                seek_from: Some(SourceFrameId(0)),
                decode_timestamp: None,
            };
            pts += interval;
            frame
        })
        .collect();
    SourceIndexSnapshot::new(
        identity(),
        0,
        SourceFrameIndex::new(
            AssetId::new("source").unwrap(),
            time_base,
            frames,
            pts,
            TerminalProvenance::DecodedFrameDuration,
        )
        .unwrap(),
    )
    .unwrap()
}

fn indexed_audio(start: i64, sample_rate: u32, counts: &[u32]) -> AudioIndexSnapshot {
    let stream = AudioStreamDescriptor {
        stream_index: 1,
        codec: "pcm_s16le".into(),
        time_base: SourceTimeBase::new(1, sample_rate).unwrap(),
        sample_rate,
        channel_layout: AudioChannelLayout::Unspecified { channels: 1 },
        stream_start: Some(99999),
        stream_duration: Some(99999),
        initial_padding: 1024,
        trailing_padding: 1024,
        seek_preroll: 1024,
    };
    let mut pts = start;
    let observations = counts
        .iter()
        .map(|count| {
            let observation = AudioFrameObservation {
                pts,
                discard: false,
                decode_timestamp: None,
                reported_duration: Some(i64::from(*count)),
                sample_count: *count,
                sample_format: "s16".into(),
                skip_samples: None,
            };
            pts += i64::from(*count);
            observation
        })
        .collect();
    AudioIndexSnapshot::new(identity(), stream, observations).unwrap()
}

fn info(time_base: SourceTimeBase) -> SourceStreamInfo {
    SourceStreamInfo {
        width: 1920,
        height: 1080,
        stream_index: 0,
        time_base_num: time_base.numerator(),
        time_base_den: time_base.denominator(),
        sample_aspect_num: 1,
        sample_aspect_den: 1,
        rotation_quarter_turns: 0,
        color: ColorMetadata {
            range: ColorRange::Full,
            matrix: ColorMatrix::Rgb,
            transfer: ColorTransfer::Srgb,
            primaries: ColorPrimaries::Bt709,
        },
        codec: "h264".into(),
        pixel_format: "gbrp".into(),
        stream_start: Some(987),
        stream_duration: Some(654),
        container_start: Some(321),
        container_duration: Some(456),
        audio_streams: Vec::new(),
    }
}

#[test]
fn signed_global_origins_do_not_change_fractional_cross_clock_placement() {
    let clock = SourceTimeBase::new(1, 30000).unwrap();
    let mut original = None;
    for shift_seconds in [-10, 0, 17] {
        let video = indexed_video(1 + shift_seconds * 30000, &[1001; 30], clock);
        let audio = indexed_audio(1 + shift_seconds * 44100, 44100, &[44117]);
        let timing = derive_import_timing(Some(&video), Some(&audio), rate()).unwrap();
        assert_eq!(
            timing.origin_seconds,
            ratio(1, 44100)
                .checked_add(ExactRatio::integer(shift_seconds))
                .unwrap()
        );
        let picture = timing.video.unwrap();
        assert_eq!(picture.start_frames, ratio(47, 147147));
        assert_eq!(picture.duration_frames, ExactRatio::integer(30));
        assert_eq!(timing.duration.frames(), 31);
        assert_eq!(timing.audio.unwrap().start_frames, ExactRatio::ZERO);
        assert_eq!(
            timing
                .source_node(AssetId::new("source").unwrap())
                .audio_offset,
            AudioSample(0)
        );
        let placement = (
            picture.start_frames,
            picture.duration_frames,
            timing.audio.unwrap().duration_frames,
            timing.duration,
        );
        if let Some(expected) = original {
            assert_eq!(placement, expected);
        } else {
            original = Some(placement);
        }
    }
    // Now picture precedes audio. Its exact 44.1 kHz origin must not be
    // rounded into an integral number of 48 kHz mix samples.
    let video = indexed_video(0, &[1001; 30], clock);
    let audio = indexed_audio(1, 44100, &[44117]);
    let timing = derive_import_timing(Some(&video), Some(&audio), rate()).unwrap();
    assert_eq!(timing.audio.unwrap().start_frames, ratio(100, 147147));
    assert_eq!(
        timing
            .source_node(AssetId::new("source").unwrap())
            .audio_mapping
            .start_frames(),
        ratio(100, 147147)
    );
}

#[test]
fn audio_interior_exclusions_identity_mismatch_and_unmeasured_video_end_fail() {
    let audio = indexed_audio(-4, 48000, &[4, 4, 4]);
    let mut excluded = audio.observations().to_vec();
    for observation in &mut excluded {
        observation.discard = true;
    }
    // The measured-index constructor rejects empty available coverage before
    // it can become import timing input.
    assert!(AudioIndexSnapshot::new(identity(), audio.stream().clone(), excluded).is_err());
    for hole in [0, 1] {
        let mut observations = audio.observations().to_vec();
        if hole == 0 {
            observations[1].discard = true;
        } else {
            observations[1].skip_samples = Some(AudioSkipSamples {
                leading: 1,
                trailing: 1,
                leading_reason: 0,
                trailing_reason: 0,
            });
        }
        let index =
            AudioIndexSnapshot::new(identity(), audio.stream().clone(), observations).unwrap();
        assert!(matches!(
            derive_import_timing(None, Some(&index), rate()),
            Err(ImportTimingError::UnavailableAudio)
        ));
    }
    let mut observations = audio.observations().to_vec();
    observations[0].discard = true;
    observations[2].skip_samples = Some(AudioSkipSamples {
        leading: 0,
        trailing: 2,
        leading_reason: 0,
        trailing_reason: 0,
    });
    let clipped =
        AudioIndexSnapshot::new(identity(), audio.stream().clone(), observations).unwrap();
    let timing = derive_import_timing(None, Some(&clipped), rate()).unwrap();
    assert_eq!(
        (
            timing.audio.unwrap().span.start().ticks,
            timing.audio.unwrap().span.end().ticks
        ),
        (0, 6)
    );
    let video = indexed_video(0, &[1001; 3], SourceTimeBase::new(1, 30000).unwrap());
    for same_stream in [false, true] {
        let mut stream = audio.stream().clone();
        stream.stream_index = if same_stream { 0 } else { 1 };
        let content = if same_stream {
            identity()
        } else {
            SourceContentIdentity::new([32; 32], 100).unwrap()
        };
        let mismatched =
            AudioIndexSnapshot::new(content, stream, audio.observations().to_vec()).unwrap();
        assert!(matches!(
            derive_import_timing(Some(&video), Some(&mismatched), rate()),
            Err(ImportTimingError::StreamMismatch)
        ));
    }
    for provenance in [
        TerminalProvenance::ContainerEnd,
        TerminalProvenance::Explicit,
        TerminalProvenance::StreamEnd,
    ] {
        let index = video.index();
        let unmeasured = SourceIndexSnapshot::new(
            identity(),
            0,
            SourceFrameIndex::new(
                index.asset().clone(),
                index.time_base(),
                index.frames().to_vec(),
                index.terminal_end(),
                provenance,
            )
            .unwrap(),
        )
        .unwrap();
        assert!(matches!(
            derive_import_timing(Some(&unmeasured), None, rate()),
            Err(ImportTimingError::UnmeasuredVideoEnd)
        ));
    }
    assert!(matches!(
        derive_import_timing(None, None, rate()),
        Err(ImportTimingError::NoStreams)
    ));
}

#[test]
fn high_rates_use_highest_common_exact_divisor_then_exact_rational_fallback() {
    for (numerator, denominator, wanted_numerator, wanted_denominator, divisor) in [
        (120000, 1001, 60000, 1001, 2),
        (120, 1, 60, 1, 2),
        (100, 1, 50, 1, 2),
        (90, 1, 30, 1, 3),
        (72, 1, 24, 1, 3),
        (901, 10, 901, 20, 2),
        (24000, 1001, 24000, 1001, 1),
    ] {
        let clock = SourceTimeBase::new(1, numerator).unwrap();
        let video = indexed_video(0, &[i64::from(denominator); 5], clock);
        let candidate = derive_presentation_basis(&video, &info(clock)).unwrap();
        assert_eq!(
            candidate.basis.frame_rate,
            FrameRate::new(wanted_numerator, wanted_denominator).unwrap()
        );
        assert_eq!(
            candidate.cadence.observed_rate,
            FrameRate::new(numerator, denominator).unwrap()
        );
        assert_eq!(candidate.cadence.presentation_divisor, divisor);
    }
}

#[test]
fn cadence_requires_repeated_integral_evidence_and_bounds_histogram_growth() {
    let clock = SourceTimeBase::new(1, 30000).unwrap();
    for intervals in [
        vec![1001, 1501, 1001, 1501, 1001],
        vec![1001, 2002, 3003, 4004, 5005],
        vec![1001, 1001, 9009, 1001],
    ] {
        let video = indexed_video(0, &intervals, clock);
        assert!(matches!(
            derive_presentation_basis(&video, &info(clock)),
            Err(ImportTimingError::AmbiguousCadence)
        ));
    }
    let intervals = (1..=MAX_CADENCE_INTERVALS + 2)
        .map(|value| value as i64)
        .collect::<Vec<_>>();
    let video = indexed_video(0, &intervals, clock);
    assert!(matches!(
        derive_presentation_basis(&video, &info(clock)),
        Err(ImportTimingError::AmbiguousCadence)
    ));
    let single = indexed_video(-4004, &[1001], clock);
    let candidate = derive_presentation_basis(&single, &info(clock)).unwrap();
    assert_eq!(
        candidate.cadence.confidence,
        CadenceConfidence::SingleDecodedFrame
    );
    assert_eq!(candidate.basis.frame_rate, rate());
    let mut frames = single.index().frames().to_vec();
    frames[0].reported_duration = None;
    let absent = SourceIndexSnapshot::new(
        identity(),
        0,
        SourceFrameIndex::new(
            AssetId::new("source").unwrap(),
            clock,
            frames,
            -3003,
            TerminalProvenance::DecodedFrameDuration,
        )
        .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        derive_presentation_basis(&absent, &info(clock)),
        Err(ImportTimingError::UnmeasuredVideoEnd)
    ));
}

#[test]
fn geometry_applies_sar_before_rotation_with_even_rounding_and_no_target_upscale() {
    let clock = SourceTimeBase::new(1, 24).unwrap();
    let video = indexed_video(0, &[1; 3], clock);
    for (width, height, sar_num, sar_den, turns, expected_width, expected_height) in [
        (720, 576, 16, 15, 1, 576, 768),
        (720, 576, 16, 15, 3, 576, 768),
        (720, 576, 16, 15, 2, 768, 576),
        (1921, 1081, 1, 1, 0, 1920, 1080),
        (5, 3, 1, 1, 0, 4, 2),
        (1, 1, 1, 1, 0, 2, 2),
        (640, 480, 1, 2, 0, 320, 480),
        (4, 2, 2, 1, 0, 8, 2),
    ] {
        let mut metadata = info(clock);
        metadata.width = width;
        metadata.height = height;
        metadata.sample_aspect_num = sar_num;
        metadata.sample_aspect_den = sar_den;
        metadata.rotation_quarter_turns = turns;
        let candidate = derive_presentation_basis(&video, &metadata).unwrap();
        assert_eq!(
            (candidate.basis.width, candidate.basis.height),
            (expected_width, expected_height)
        );
        for (rounded, exact) in [
            (candidate.basis.width, candidate.geometry.display_width),
            (candidate.basis.height, candidate.geometry.display_height),
        ] {
            let error = ExactRatio::integer(i64::from(rounded))
                .checked_sub(exact)
                .unwrap();
            assert!(!error.compare_integer(-1).is_lt() && !error.compare_integer(1).is_gt());
        }
        if width == 5 {
            assert_eq!(candidate.geometry.relative_aspect_error, ratio(1, 5));
        }
    }
    for (numerator, denominator) in [(u32::MAX, 1), (1, u32::MAX), (0, 1), (1, 0)] {
        let mut metadata = info(clock);
        metadata.sample_aspect_num = numerator;
        metadata.sample_aspect_den = denominator;
        assert!(matches!(
            derive_presentation_basis(&video, &metadata),
            Err(ImportTimingError::UnsupportedGeometry)
        ));
    }
    let mut metadata = info(clock);
    metadata.time_base_den = 25;
    assert!(matches!(
        derive_presentation_basis(&video, &metadata),
        Err(ImportTimingError::VideoMetadataMismatch)
    ));
}
