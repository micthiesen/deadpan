use super::*;
use deadpan_media::source_import_timing::derive_source_audio_moment;

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
