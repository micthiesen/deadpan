use super::*;

use deadpan_core::{
    ExactFrameRange, LinkRelation, SourceAudioMapping, SourcePoint, SourceVideo, TimeError,
};
use deadpan_media::source_import_timing::{SourceMomentTiming, derive_source_moment};

fn moment(
    video: &SourceIndexSnapshot,
    audio: Option<&AudioIndexSnapshot>,
    start: u64,
    end: u64,
) -> SourceMomentTiming {
    derive_source_moment(video, audio, start..end, FrameRate::new(30, 1).unwrap()).unwrap()
}

fn selected_original_ticks(moment: &SourceMomentTiming, local: ExactRatio) -> ExactRatio {
    let audio = moment.audio.unwrap().placement;
    ExactRatio::integer(audio.span.start().ticks)
        .checked_add(
            local
                .checked_sub(audio.start_frames)
                .unwrap()
                .checked_div(audio.duration_frames)
                .unwrap()
                .checked_mul(ExactRatio::integer(
                    audio.span.end().ticks - audio.span.start().ticks,
                ))
                .unwrap(),
        )
        .unwrap()
}

#[test]
fn fractional_audio_selection_keeps_full_samples_phase_and_exact_cut_before_picture_slack() {
    let video = indexed_video(0, &[1, 1, 1], SourceTimeBase::new(1, 1000).unwrap());
    let audio = indexed_audio(0, 44_100, &[441]);
    let selected = moment(&video, Some(&audio), 1, 2);
    assert_eq!(selected.ordinals, 1..2);
    assert_eq!(selected.origin_seconds, ratio(1, 1000));
    assert_eq!(selected.duration.frames(), 1);
    assert_eq!(selected.video.duration_frames, ratio(3, 100));
    let placement = selected.audio.unwrap();
    assert_eq!(placement.placement.span.start().ticks, 0);
    assert_eq!(placement.placement.span.end().ticks, 441);
    assert_eq!(
        placement.placement.span.start().time_base,
        SourceTimeBase::new(1, 44_100).unwrap()
    );
    assert_eq!(placement.placement.start_frames, ratio(-3, 100));
    assert_eq!(placement.placement.duration_frames, ratio(3, 10));
    assert_eq!(
        placement.selection,
        ExactFrameRange::new(ExactRatio::ZERO, ratio(3, 100)).unwrap()
    );
    assert_eq!(
        selected_original_ticks(&selected, ExactRatio::ZERO),
        ratio(441, 10)
    );
    assert_eq!(
        selected_original_ticks(&selected, placement.selection.end),
        ratio(441, 5)
    );
    // Discrete filter support is 45..89, but no boundary is snapped to either
    // sample. The exact 1ms audible extent owns only the first 48 mix samples.
    assert_eq!(
        selected_original_ticks(&selected, ExactRatio::ZERO)
            .ceil()
            .unwrap(),
        45
    );
    assert_eq!(
        selected_original_ticks(&selected, placement.selection.end)
            .ceil()
            .unwrap(),
        89
    );
    assert_eq!(
        placement
            .selection
            .end
            .checked_mul(ExactRatio::integer(1600))
            .unwrap(),
        ExactRatio::integer(48)
    );
    let node = selected.source_node(AssetId::new("source").unwrap());
    assert_eq!(node.audio_offset, AudioSample(0));
    assert_eq!(node.link, LinkRelation::Linked);
    assert_eq!(node.video_mapping.endpoints(), EndpointPolicy::HoldAdjacent);
    assert_eq!(
        node.video_mapping.duration_frames(node.duration).unwrap(),
        ratio(9, 100)
    );
    assert_eq!(node.video_mapping.start_frames(), ratio(-3, 100));
    assert_eq!(
        node.video_mapping.selection_frames(node.duration).unwrap(),
        ExactFrameRange::new(ExactRatio::ZERO, ratio(3, 100)).unwrap()
    );
    assert_eq!(
        node.audio_mapping.duration_frames(node.duration).unwrap(),
        ratio(3, 10)
    );
    assert_eq!(
        node.audio_mapping.selection_frames(node.duration).unwrap(),
        placement.selection
    );
    assert!(matches!(
        node.audio_mapping,
        SourceAudioMapping::SelectedPlacement { .. }
    ));
    // Deriving a moment never changes the whole-source import union or mapping.
    let full = derive_import_timing(Some(&video), Some(&audio), selected.project_rate).unwrap();
    assert_eq!(full.audio.unwrap().span, placement.placement.span);
    assert!(matches!(
        full.source_node(AssetId::new("source").unwrap())
            .audio_mapping,
        SourceAudioMapping::Placement { .. }
    ));
}

#[test]
fn vfr_selection_uses_presentation_intervals_and_the_measured_final_boundary() {
    let video = indexed_video(-5, &[1, 3, 2], SourceTimeBase::new(1, 1000).unwrap());
    for (ordinals, start, end, frames) in [
        (0..1, -5, -4, ratio(3, 100)),
        (1..2, -4, -1, ratio(9, 100)),
        (1..3, -4, 1, ratio(3, 20)),
        (2..3, -1, 1, ratio(3, 50)),
    ] {
        let selected = moment(&video, None, ordinals.start, ordinals.end);
        assert_eq!(selected.video.span.start().ticks, start);
        assert_eq!(selected.video.span.end().ticks, end);
        assert_eq!(selected.video.duration_frames, frames);
        assert_eq!(selected.duration.frames(), 1);
        let held = video
            .index()
            .select_in_span(
                SourcePoint {
                    ticks: ExactRatio::integer(end),
                    time_base: video.index().time_base(),
                },
                selected.video.span,
                EndpointPolicy::HoldAdjacent,
            )
            .unwrap();
        assert_eq!(held.identity, SourceFrameId(ordinals.end - 1));
    }
    // Selection rounds its own elapsed duration once, not two independently
    // rounded global coordinates or an ordinal count divided by nominal fps.
    let video = indexed_video(1, &[11, 13, 17], SourceTimeBase::new(1, 1000).unwrap());
    let selected = derive_source_moment(&video, None, 1..3, rate()).unwrap();
    assert_eq!(selected.video.duration_frames, ratio(900, 1001));
    assert_eq!(selected.duration.frames(), 1);
}

#[test]
fn moving_the_shared_signed_origin_preserves_relative_audio_selection() {
    let mut previous = None;
    for shift in [-11, 0, 29] {
        let video = indexed_video(
            shift * 1000 + 1,
            &[1, 1],
            SourceTimeBase::new(1, 1000).unwrap(),
        );
        let audio = indexed_audio(shift * 44_100, 44_100, &[441]);
        let selected = moment(&video, Some(&audio), 0, 1);
        assert_eq!(
            selected.origin_seconds,
            ExactRatio::integer(shift)
                .checked_add(ratio(1, 1000))
                .unwrap()
        );
        let audio = selected.audio.unwrap();
        let relative = (
            audio.placement.start_frames,
            audio.placement.duration_frames,
            audio.selection,
            selected.video.duration_frames,
            selected.duration,
        );
        if let Some(previous) = previous {
            assert_eq!(relative, previous);
        }
        previous = Some(relative);
        assert_eq!(
            selected_original_ticks(&selected, ExactRatio::ZERO),
            ExactRatio::integer(shift * 44_100)
                .checked_add(ratio(441, 10))
                .unwrap()
        );
    }
}

#[test]
fn delayed_early_ending_and_nonoverlapping_audio_keep_measured_av_alignment() {
    let video = indexed_video(0, &[10, 10], SourceTimeBase::new(1, 1000).unwrap());
    for (start, count, expected_start, expected_end) in [
        (240, 480, ratio(3, 20), ratio(3, 10)),
        (-240, 480, ExactRatio::ZERO, ratio(3, 20)),
    ] {
        let audio = indexed_audio(start, 48_000, &[count]);
        let selected = moment(&video, Some(&audio), 0, 1);
        let audio = selected.audio.unwrap();
        assert_eq!(
            audio.selection,
            ExactFrameRange::new(expected_start, expected_end).unwrap()
        );
        assert_eq!(audio.placement.span.start().ticks, start);
        assert_eq!(audio.placement.span.end().ticks, start + i64::from(count));
        assert_eq!(selected.duration.frames(), 1);
    }
    for (start, count, point) in [
        (-480, 480, ratio(0, 1)),
        (480, 480, ratio(3, 10)),
        (960, 480, ratio(3, 5)),
    ] {
        let audio = indexed_audio(start, 48_000, &[count]);
        let selected = moment(&video, Some(&audio), 0, 1);
        let dormant = selected.audio.unwrap();
        assert_eq!(
            dormant.selection,
            ExactFrameRange {
                start: point,
                end: point
            }
        );
        assert_eq!(dormant.placement.span.start().ticks, start);
        assert_eq!(dormant.placement.span.end().ticks, start + i64::from(count));
        let node = selected.source_node(AssetId::new("source").unwrap());
        assert_eq!(node.audio.as_ref().unwrap().span, dormant.placement.span);
        assert_eq!(
            node.audio_mapping.selection_frames(node.duration).unwrap(),
            dormant.selection
        );
        assert_eq!(node.link, LinkRelation::Linked);
        assert_eq!(node.audio_offset, AudioSample(0));
        assert_eq!(
            derive_import_timing(Some(&video), Some(&audio), selected.project_rate)
                .unwrap()
                .audio
                .unwrap()
                .span,
            dormant.placement.span
        );
    }
    let absent = moment(&video, None, 0, 1);
    assert!(absent.audio.is_none());
    let node = absent.source_node(AssetId::new("source").unwrap());
    assert!(node.audio.is_none());
    assert_eq!(node.audio_mapping, SourceAudioMapping::FitBeat);
    assert_eq!(node.link, LinkRelation::Independent);
}

#[test]
fn a_positive_window_with_no_discrete_audio_sample_is_retained_without_snapping() {
    let video = indexed_video(1, &[1], SourceTimeBase::new(1, 1_000_000).unwrap());
    let audio = indexed_audio(0, 44_100, &[4]);
    let selected = moment(&video, Some(&audio), 0, 1);
    let selection = selected.audio.unwrap().selection;
    assert_eq!(selection.end, ratio(3, 100_000));
    let first = selected_original_ticks(&selected, selection.start);
    let last = selected_original_ticks(&selected, selection.end);
    assert_eq!(first, ratio(441, 10_000));
    assert_eq!(last, ratio(441, 5_000));
    assert_eq!(first.ceil().unwrap(), last.ceil().unwrap());
    assert_eq!(selected.duration.frames(), 1);
}

#[test]
fn moment_rejects_invalid_ordinals_and_reuses_full_stream_evidence_checks() {
    let video = indexed_video(0, &[1, 2, 3], SourceTimeBase::new(1, 1000).unwrap());
    for (start, end) in [(0, 0), (2, 1), (3, 3), (0, 4), (u64::MAX, u64::MAX)] {
        let ordinals = start..end;
        assert!(matches!(
            derive_source_moment(&video, None, ordinals, rate()),
            Err(ImportTimingError::InvalidMomentRange)
        ));
    }
    let audio = indexed_audio(0, 48_000, &[4, 4, 4]);
    for same_stream in [false, true] {
        let mut stream = audio.stream().clone();
        stream.stream_index = if same_stream { 0 } else { 1 };
        let content = if same_stream {
            identity()
        } else {
            SourceContentIdentity::new([88; 32], 100).unwrap()
        };
        let mismatched =
            AudioIndexSnapshot::new(content, stream, audio.observations().to_vec()).unwrap();
        assert!(matches!(
            derive_source_moment(&video, Some(&mismatched), 0..1, rate()),
            Err(ImportTimingError::StreamMismatch)
        ));
    }
    let mut observations = audio.observations().to_vec();
    observations[1].discard = true;
    let unavailable =
        AudioIndexSnapshot::new(identity(), audio.stream().clone(), observations).unwrap();
    assert!(matches!(
        derive_source_moment(&video, Some(&unavailable), 0..1, rate()),
        Err(ImportTimingError::UnavailableAudio)
    ));
    let index = video.index();
    for wrong_duration in [false, true] {
        let mut frames = index.frames().to_vec();
        let provenance = if wrong_duration {
            frames.last_mut().unwrap().reported_duration = Some(99);
            TerminalProvenance::DecodedFrameDuration
        } else {
            TerminalProvenance::Explicit
        };
        let unmeasured = SourceIndexSnapshot::new(
            identity(),
            0,
            SourceFrameIndex::new(
                index.asset().clone(),
                index.time_base(),
                frames,
                index.terminal_end(),
                provenance,
            )
            .unwrap(),
        )
        .unwrap();
        // A prefix is still evidence from the same completely measured Original.
        assert!(matches!(
            derive_source_moment(&unmeasured, None, 0..1, rate()),
            Err(ImportTimingError::UnmeasuredVideoEnd)
        ));
    }
}

#[test]
fn moment_duration_and_mapping_arithmetic_are_checked() {
    let video = indexed_video(
        i64::MAX - 1,
        &[1],
        SourceTimeBase::new(u32::MAX, 1).unwrap(),
    );
    assert!(matches!(
        derive_source_moment(&video, None, 0..1, FrameRate::new(u32::MAX, 1).unwrap()),
        Err(ImportTimingError::Time(TimeError::Overflow))
    ));
    let video = indexed_video(0, &[i64::MAX], SourceTimeBase::new(1, 1).unwrap());
    assert!(matches!(
        derive_source_moment(&video, None, 0..1, FrameRate::new(2, 1).unwrap()),
        Err(ImportTimingError::Time(TimeError::Overflow))
    ));
}

#[test]
fn decoded_cfr_offset_and_vfr_moments_retain_real_pts_and_endpoint_picture_identity() {
    for name in ["cfr-bframes.mp4", "offset-bframes.mp4", "vfr.mp4"] {
        let input = fixture_input("deadpan-source/tests/fixtures", name);
        let mut video = video_session(input.clone());
        let audio = audio_session(input, 1);
        let count = u64::try_from(video.index().index().frames().len()).unwrap();
        for ordinals in [7..15, count - 1..count] {
            let selected =
                derive_source_moment(video.index(), Some(audio.index()), ordinals.clone(), rate())
                    .unwrap();
            for ordinal in [ordinals.start, ordinals.end - 1] {
                let frame = video
                    .frame(
                        SourceFrameId(ordinal),
                        Duration::from_secs(2),
                        &AtomicBool::new(false),
                    )
                    .unwrap();
                assert_eq!(
                    frame.metadata.pts,
                    video
                        .index()
                        .index()
                        .interval(SourceFrameId(ordinal))
                        .unwrap()
                        .0,
                    "{name}"
                );
                if ordinal == ordinals.start {
                    assert_eq!(
                        selected.video.span.start().ticks,
                        frame.metadata.pts,
                        "{name}"
                    );
                }
            }
            let span = selected.video.span;
            let node = selected.source_node(AssetId::new("source").unwrap());
            let SourceVideo::Stream { span: context, .. } = node.video else {
                panic!("selected video lost its source context");
            };
            let exact = node
                .video_mapping
                .selection_in_source(context, node.duration)
                .unwrap();
            assert_eq!(
                exact.start().ticks,
                ExactRatio::integer(span.start().ticks),
                "{name}"
            );
            assert_eq!(
                exact.end().ticks,
                ExactRatio::integer(span.end().ticks),
                "{name}"
            );
            assert_eq!(
                context.start().ticks,
                video.index().index().frames()[0].pts,
                "{name}"
            );
            assert_eq!(
                context.end().ticks,
                video.index().index().terminal_end(),
                "{name}"
            );
            let endpoint = video
                .index()
                .index()
                .select_in_exact_span(
                    SourcePoint {
                        ticks: ExactRatio::integer(span.end().ticks),
                        time_base: span.end().time_base,
                    },
                    exact,
                    EndpointPolicy::HoldAdjacent,
                )
                .unwrap();
            assert_eq!(endpoint.identity, SourceFrameId(ordinals.end - 1), "{name}");
            let whole =
                derive_import_timing(Some(video.index()), Some(audio.index()), rate()).unwrap();
            assert_eq!(
                selected.audio.unwrap().placement.span,
                whole.audio.unwrap().span,
                "{name}"
            );
        }
    }
}

#[test]
fn dormant_and_audible_moments_share_the_original_fractional_sample_clock() {
    let video = indexed_video(0, &[1, 1, 1], SourceTimeBase::new(1, 1000).unwrap());
    let audio = indexed_audio(60, 44_100, &[200]);
    let dormant = moment(&video, Some(&audio), 0, 1);
    let audible = moment(&video, Some(&audio), 1, 2);
    let retained = dormant.audio.unwrap();
    assert_eq!(retained.selection.start, retained.selection.end);
    assert_eq!(retained.selection.start, ratio(2, 49));
    assert_eq!(
        retained.placement.span,
        audible.audio.unwrap().placement.span
    );
    assert_eq!(
        retained.placement.duration_frames,
        audible.audio.unwrap().placement.duration_frames
    );
    assert_eq!(
        selected_original_ticks(&dormant, ratio(3, 100)),
        selected_original_ticks(&audible, ExactRatio::ZERO)
    );
    assert_eq!(
        selected_original_ticks(&audible, ExactRatio::ZERO),
        ratio(441, 10)
    );
    // The source clock may lie before measured audio; only the selected support
    // controls reads. Derivation does not round the affine clock to sample 60.
    assert_eq!(audible.audio.unwrap().selection.start, ratio(53, 4900));
}
