use super::*;
use crate::Word;

#[test]
fn latest_cursor_replaces_priority_without_abandoning_running_work() {
    let mut schedule = RangeSchedule::new(101, 20, 3).unwrap();
    let first = schedule.next(75).unwrap().unwrap();
    assert_eq!(first.owned, 60..80);
    assert_eq!(first.context, 57..83);
    assert!(schedule.next(0).is_err());
    let mut wrong = first.clone();
    wrong.owned = 40..60;
    assert!(schedule.finish(&wrong).is_err());
    schedule.finish(&first).unwrap();
    assert_eq!(schedule.completed_samples(), 20);
    let at_end = schedule.next(u64::MAX).unwrap().unwrap();
    assert_eq!(at_end.owned, 100..101);
    schedule.retry_running();
    assert_eq!(schedule.next(101).unwrap().unwrap(), at_end);
    schedule.finish(&at_end).unwrap();
    let at_start = schedule.next(0).unwrap().unwrap();
    assert_eq!(at_start.owned, 0..20);
    schedule.finish(&at_start).unwrap();
    let mut coverage = vec![first.owned, at_end.owned, at_start.owned];
    while let Some(range) = schedule.next(50).unwrap() {
        coverage.push(range.owned.clone());
        schedule.finish(&range).unwrap();
    }
    coverage.sort_by_key(|range| range.start);
    assert_eq!(
        coverage,
        vec![0..20, 20..40, 40..60, 60..80, 80..100, 100..101]
    );
    assert!(schedule.is_complete());
    assert_eq!(schedule.completed_samples(), 101);
}

#[test]
fn ranges_are_bounded_and_audio_context_joins_all_measurement_grids() {
    assert!(RangeSchedule::new(1, 0, 0).is_err());
    assert!(RangeSchedule::new(0, 1, 0).is_err());
    assert!(RangeSchedule::new(MAX_ANALYSIS_RANGES + 1, 1, 0).is_err());
    let mut schedule = RangeSchedule::audio(3 * 60 * 60 * 16_000 + 1).unwrap();
    while let Some(range) = schedule.next(14_000_000).unwrap() {
        assert!(range.context.end - range.context.start <= 30 * 16_000);
        for hop in [160, 512] {
            assert_eq!(range.context.start % hop, 0);
            assert_eq!(range.owned.start % hop, 0);
        }
        schedule.finish(&range).unwrap();
    }
}

fn word(text: &str, start_cs: u32, end_cs: u32, segment: u32) -> Word {
    Word {
        text: text.into(),
        start_cs,
        end_cs,
        probability: 0.8,
        segment,
    }
}

#[test]
fn out_of_order_transcript_ranges_keep_global_clock_and_seam_word_once() {
    let audio = AnalysedAudio {
        origin: -4_410,
        sample_rate: 44_100,
        duration_cs: 4_800,
    };
    let extent = 48 * 16_000;
    let before = TranscriptRange {
        range: AnalysisRange {
            owned: 0..384_000,
            context: 0..422_400,
        },
        transcript: Transcript::new(
            AnalysedAudio {
                duration_cs: 2_640,
                ..audio
            },
            vec![word("first", 100, 110, 2), word("seam", 2_390, 2_410, 7)],
        )
        .unwrap(),
    };
    let after = TranscriptRange {
        range: AnalysisRange {
            owned: 384_000..extent,
            context: 345_600..extent,
        },
        transcript: Transcript::new(
            AnalysedAudio {
                duration_cs: 2_640,
                ..audio
            },
            vec![word("seam", 230, 250, 0), word("last", 2_500, 2_520, 3)],
        )
        .unwrap(),
    };
    let partial = assemble_transcript(audio, extent, std::slice::from_ref(&after)).unwrap();
    assert_eq!(partial.words()[0].text, "seam");
    assert_eq!(partial.words()[0].start_cs, 2_390);
    let result = assemble_transcript(audio, extent, &[after.clone(), before.clone()]).unwrap();
    assert_eq!(result.audio(), audio);
    assert_eq!(
        result
            .words()
            .iter()
            .map(|word| word.text.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "seam", "last"]
    );
    assert_eq!(result.words()[2].end_cs, 4_680);
    assert!(
        result
            .words()
            .windows(2)
            .all(|pair| pair[0].segment < pair[1].segment)
    );
    assert!(assemble_transcript(audio, extent, &[before.clone(), before]).is_err());
}

#[test]
fn seam_jitter_neither_duplicates_nor_loses_a_word_and_keeps_real_repetitions() {
    let audio = AnalysedAudio {
        origin: -441,
        sample_rate: 44_100,
        duration_cs: 4_800,
    };
    for (left, right) in [((2_389, 2_409), (231, 251)), ((2_391, 2_411), (229, 249))] {
        let before = TranscriptRange {
            range: AnalysisRange {
                owned: 0..384_000,
                context: 0..422_400,
            },
            transcript: Transcript::new(
                AnalysedAudio {
                    duration_cs: 2_640,
                    ..audio
                },
                vec![
                    word("the", 2_370, 2_380, 0),
                    word("seam", left.0, left.1, 0),
                    word("the", 2_420, 2_430, 0),
                ],
            )
            .unwrap(),
        };
        let after = TranscriptRange {
            range: AnalysisRange {
                owned: 384_000..768_000,
                context: 345_600..768_000,
            },
            transcript: Transcript::new(
                AnalysedAudio {
                    duration_cs: 2_640,
                    ..audio
                },
                vec![
                    word("the", 210, 220, 0),
                    word("seam", right.0, right.1, 0),
                    word("the", 260, 270, 0),
                ],
            )
            .unwrap(),
        };
        let result = assemble_transcript(audio, 768_000, &[after, before]).unwrap();
        assert_eq!(
            result
                .words()
                .iter()
                .map(|word| word.text.as_str())
                .collect::<Vec<_>>(),
            vec!["the", "seam", "the"]
        );
    }
    assert!(!same_timed_word(
        &word("no", 2_390, 2_400, 0),
        &word("no", 2_400, 2_410, 0)
    ));
    assert!(!same_timed_word(
        &word("no", 2_390, 2_400, 0),
        &word("no", 2_410, 2_420, 0)
    ));
}

#[test]
fn final_partial_centisecond_keeps_measured_word_and_excludes_unmeasured_start() {
    let audio = AnalysedAudio {
        origin: -441,
        sample_rate: 44_100,
        duration_cs: 7,
    };
    let range = TranscriptRange {
        range: AnalysisRange {
            owned: 0..1_001,
            context: 0..1_001,
        },
        transcript: Transcript::new(
            audio,
            vec![word("ending", 6, 7, 0), word("outside", 7, 7, 0)],
        )
        .unwrap(),
    };
    let result = assemble_transcript(audio, 1_001, &[range]).unwrap();
    assert_eq!(result.words(), &[word("ending", 6, 7, 1)]);
    let partial_audio = AnalysedAudio {
        duration_cs: 160,
        ..audio
    };
    let partial = TranscriptRange {
        range: AnalysisRange {
            owned: 0..12_800,
            context: 0..25_600,
        },
        transcript: Transcript::new(
            partial_audio,
            vec![word("covered", 79, 80, 0), word("uncovered", 100, 110, 0)],
        )
        .unwrap(),
    };
    let result = assemble_transcript(partial_audio, 25_600, &[partial]).unwrap();
    assert_eq!(result.words().len(), 1);
    assert_eq!(result.words()[0].text, "covered");
}

#[test]
fn global_vad_and_energy_grids_stitch_exactly_and_reject_missing_ranges() {
    let audio = ActivityAudio {
        origin: -2_205,
        sample_rate: 22_050,
        samples: 48 * 16_000 + 137,
    };
    let speech: Vec<_> = (0..audio.samples.div_ceil(VAD_HOP))
        .map(|index| (index % 255) as u8)
        .collect();
    let energy: Vec<_> = (0..audio.samples.div_ceil(ENERGY_HOP))
        .map(|index| (index % 241) as u8)
        .collect();
    let expected = SpeechActivity::new(audio, speech.clone(), energy.clone()).unwrap();
    let mut schedule = RangeSchedule::audio(audio.samples).unwrap();
    let mut parts = Vec::new();
    while let Some(range) = schedule.next(audio.samples).unwrap() {
        let local = ActivityAudio {
            samples: range.context.end - range.context.start,
            ..audio
        };
        let crop = |data: &[u8], hop: u64| {
            data[(range.context.start / hop) as usize..range.context.end.div_ceil(hop) as usize]
                .to_vec()
        };
        let activity =
            SpeechActivity::new(local, crop(&speech, VAD_HOP), crop(&energy, ENERGY_HOP)).unwrap();
        parts.push(ActivityRange {
            range: range.clone(),
            activity,
        });
        schedule.finish(&range).unwrap();
    }
    assert_eq!(assemble_activity(audio, &parts).unwrap(), expected);
    assert!(assemble_activity(audio, &parts[1..]).is_err());
    let mut wrong = parts.clone();
    wrong[0].range.context.start += 1;
    assert!(assemble_activity(audio, &wrong).is_err());
}
