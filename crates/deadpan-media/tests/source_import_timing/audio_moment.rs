use super::*;
use deadpan_media::source_import_timing::{derive_source_audio_moment, derive_source_moment};

#[test]
fn inward_audio_ranges_use_measured_pts_and_native_sample_clock() {
    let video = indexed_video(1, &[1, 1, 3], SourceTimeBase::new(1, 1000).unwrap());
    let audio = indexed_audio(0, 44_100, &[441]);
    for (ordinals, start, end) in [(0..1, 45, 88), (1..3, 89, 264), (0..3, 45, 264)] {
        let span = derive_source_audio_moment(&video, &audio, ordinals).unwrap();
        assert_eq!((span.start().ticks, span.end().ticks), (start, end));
        assert_eq!(
            span.start().time_base,
            SourceTimeBase::new(1, 44_100).unwrap()
        );
        assert_eq!(span.end().time_base, span.start().time_base);
    }
    let video = indexed_video(-2, &[1, 3], SourceTimeBase::new(1, 1000).unwrap());
    let audio = indexed_audio(-100, 44_100, &[200]);
    let first = derive_source_audio_moment(&video, &audio, 0..1).unwrap();
    assert_eq!((first.start().ticks, first.end().ticks), (-88, -45));
    let audio = indexed_audio(-60, 44_100, &[120]);
    let whole = derive_source_audio_moment(&video, &audio, 0..2).unwrap();
    assert_eq!((whole.start().ticks, whole.end().ticks), (-60, 60));
}

/// Room tone authored from a copied moment's Source (`SetRoomTone`, Nothing
/// Happens) must hear exactly the inward range the native room-tone sheet
/// derives from measured picture PTS, at any project rate.
#[test]
fn copied_moment_sources_yield_the_same_inward_audio_range() {
    let asset = deadpan_core::AssetId::new("original").unwrap();
    for (video, audio) in [
        (
            indexed_video(1, &[1, 1, 3], SourceTimeBase::new(1, 1000).unwrap()),
            indexed_audio(0, 44_100, &[441]),
        ),
        (
            indexed_video(-2, &[1, 3], SourceTimeBase::new(1, 1000).unwrap()),
            indexed_audio(-60, 44_100, &[120]),
        ),
    ] {
        let frames = u64::try_from(video.index().frames().len()).unwrap();
        for rate in [
            deadpan_core::FrameRate::new(30_000, 1001).unwrap(),
            deadpan_core::FrameRate::new(24, 1).unwrap(),
        ] {
            for start in 0..frames {
                for end in start + 1..=frames {
                    let expected = derive_source_audio_moment(&video, &audio, start..end);
                    let source = derive_source_moment(&video, Some(&audio), start..end, rate)
                        .unwrap()
                        .source_node(asset.clone());
                    let actual = deadpan_core::copied_moment_audio(&source).map(|audio| audio.span);
                    match expected {
                        Ok(span) => assert_eq!(actual.unwrap(), span, "{start}..{end} at {rate:?}"),
                        Err(_) => assert!(actual.is_err(), "{start}..{end} at {rate:?}"),
                    }
                }
            }
        }
    }
}

#[test]
fn inward_audio_ranges_reject_empty_overlap_gaps_wrong_identity_and_overflow() {
    let video = indexed_video(1, &[1], SourceTimeBase::new(1, 1_000_000).unwrap());
    let audio = indexed_audio(0, 44_100, &[441]);
    assert!(matches!(
        derive_source_audio_moment(&video, &audio, 0..1),
        Err(ImportTimingError::UnavailableAudio)
    ));
    let video = indexed_video(0, &[1, 2, 3], SourceTimeBase::new(1, 1000).unwrap());
    let audio = indexed_audio(10_000, 44_100, &[441]);
    assert!(matches!(
        derive_source_audio_moment(&video, &audio, 0..3),
        Err(ImportTimingError::UnavailableAudio)
    ));
    let audio = indexed_audio(0, 48_000, &[96, 96, 96]);
    for (start, end) in [(0, 0), (2, 1), (0, 4), (u64::MAX, u64::MAX)] {
        assert!(matches!(
            derive_source_audio_moment(&video, &audio, start..end),
            Err(ImportTimingError::InvalidMomentRange)
        ));
    }
    let mut observations = audio.observations().to_vec();
    observations[1].discard = true;
    let gap = AudioIndexSnapshot::new(identity(), audio.stream().clone(), observations).unwrap();
    assert!(matches!(
        derive_source_audio_moment(&video, &gap, 0..1),
        Err(ImportTimingError::UnavailableAudio)
    ));
    let wrong = AudioIndexSnapshot::new(
        SourceContentIdentity::new([42; 32], 100).unwrap(),
        audio.stream().clone(),
        audio.observations().to_vec(),
    )
    .unwrap();
    assert!(matches!(
        derive_source_audio_moment(&video, &wrong, 0..1),
        Err(ImportTimingError::StreamMismatch)
    ));
    let video = indexed_video(
        i64::MAX - 1,
        &[1],
        SourceTimeBase::new(u32::MAX, 1).unwrap(),
    );
    assert!(matches!(
        derive_source_audio_moment(&video, &audio, 0..1),
        Err(ImportTimingError::Time(deadpan_core::TimeError::Overflow))
    ));
}

#[test]
fn inward_audio_ranges_require_a_measured_terminal_even_for_a_prefix() {
    let video = indexed_video(0, &[1, 2], SourceTimeBase::new(1, 1000).unwrap());
    let index = video.index();
    let unmeasured = SourceIndexSnapshot::new(
        identity(),
        0,
        SourceFrameIndex::new(
            index.asset().clone(),
            index.time_base(),
            index.frames().to_vec(),
            index.terminal_end(),
            TerminalProvenance::Explicit,
        )
        .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        derive_source_audio_moment(&unmeasured, &indexed_audio(0, 48_000, &[480]), 0..1),
        Err(ImportTimingError::UnmeasuredVideoEnd)
    ));
}
