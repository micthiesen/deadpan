use super::*;
use crate::{ActivityAudio, AnalysedAudio, VAD_HOP};
use proptest::prelude::*;

const ORIGIN: i64 = 2_048;
const RATE: u32 = 48_000;

fn audio(duration_cs: u32) -> AnalysedAudio {
    AnalysedAudio {
        origin: ORIGIN,
        sample_rate: RATE,
        duration_cs,
    }
}

fn word(text: &str, start_cs: u32, end_cs: u32, segment: u32) -> Word {
    Word {
        text: text.into(),
        start_cs,
        end_cs,
        probability: 0.9,
        segment,
    }
}

fn proposal() -> Transcript {
    Transcript::new(
        audio(1_000),
        vec![
            word("We", 10, 30, 0),
            word("gonna", 35, 80, 0),
            word("to", 90, 110, 1),
            word("day", 112, 150, 1),
            word("now.", 160, 200, 1),
        ],
    )
    .unwrap()
}

fn texts(transcript: &Transcript) -> Vec<(String, u32, u32)> {
    transcript
        .words()
        .iter()
        .map(|word| (word.text.clone(), word.start_cs, word.end_cs))
        .collect()
}

fn empty() -> Corrections {
    Corrections::empty(Corrections::clock_of(&proposal()))
}

#[test]
fn no_corrections_leave_the_recognized_words() {
    let corrected = empty().apply_to_transcript(&proposal()).unwrap();
    assert_eq!(corrected.transcript, proposal());
    assert_eq!(corrected.replaced, 0);
    assert!(!corrected.corrected(0));
}

#[test]
fn editing_text_splits_merges_and_deletes_words() {
    let recognized = proposal();
    let corrections = empty();
    let current = corrections.apply_to_transcript(&recognized).unwrap();

    // Two words share the time in proportion to their letters.
    let split = corrections
        .edit_word_text(&current, 1, "going to", &[])
        .unwrap();
    let applied = split.apply_to_transcript(&recognized).unwrap();
    assert_eq!(
        texts(&applied.transcript)[1..3],
        [("going".into(), 35, 67), ("to".into(), 67, 80)]
    );
    assert_eq!(applied.replaced, 1);
    assert!(applied.corrected(1) && applied.corrected(2) && !applied.corrected(3));
    // A measured edge within 80 ms of the proportional split wins.
    let snapped = corrections
        .edit_word_text(&current, 1, "going to", &[60])
        .unwrap()
        .apply_to_transcript(&recognized)
        .unwrap();
    assert_eq!(texts(&snapped.transcript)[1].2, 60);

    // Join "to" and "day" without a space.
    let merged = split
        .merge_words(&applied, 3)
        .unwrap()
        .apply_to_transcript(&recognized)
        .unwrap();
    assert_eq!(texts(&merged.transcript)[3], ("today".into(), 90, 150));
    assert_eq!(merged.transcript.search("today").len(), 1);

    // Empty text removes the word.
    let deleted = corrections
        .edit_word_text(&current, 4, "", &[])
        .unwrap()
        .apply_to_transcript(&recognized)
        .unwrap();
    assert_eq!(deleted.transcript.words().len(), 4);
    assert!(
        corrections.edit_word_text(&current, 0, "We", &[]).is_err(),
        "unchanged text is not a correction"
    );
}

#[test]
fn moving_an_edge_shortens_the_neighbour_instead_of_overlapping_it() {
    let recognized = proposal();
    let corrections = empty();
    let current = corrections.apply_to_transcript(&recognized).unwrap();
    let (moved, start, end) = corrections.set_word_bounds(&current, 3, 100, 155).unwrap();
    assert_eq!((start, end), (100, 155));
    let applied = moved.apply_to_transcript(&recognized).unwrap();
    assert_eq!(
        texts(&applied.transcript)[2..5],
        [
            ("to".into(), 90, 100),
            ("day".into(), 100, 155),
            ("now.".into(), 160, 200)
        ]
    );
    // An edge cannot pass its neighbour's far edge.
    let (_, start, _) = corrections.set_word_bounds(&current, 3, 0, 150).unwrap();
    assert_eq!(start, 90);
}

#[test]
fn corrections_survive_a_new_transcription_and_win_over_straddling_words() {
    let recognized = proposal();
    let corrections = empty();
    let current = corrections.apply_to_transcript(&recognized).unwrap();
    let corrected = corrections
        .edit_word_text(&current, 1, "going to", &[])
        .unwrap();
    // A new transcription hears different words and times.
    let again = Transcript::new(
        audio(1_000),
        vec![
            word("We're", 8, 33, 0),
            word("gon", 34, 60, 0),
            word("na", 60, 85, 0),
            word("today", 88, 150, 1),
            word("now", 160, 205, 1),
        ],
    )
    .unwrap();
    let applied = corrected.apply_to_transcript(&again).unwrap();
    // "na" straddles the corrected region's end (35–80) and is replaced.
    assert_eq!(
        texts(&applied.transcript),
        [
            ("We're".into(), 8, 33),
            ("going".into(), 35, 67),
            ("to".into(), 67, 80),
            ("today".into(), 88, 150),
            ("now".into(), 160, 205)
        ]
    );
    assert_eq!(applied.replaced, 2);
    // Corrected words take the sentence of the first word they replaced.
    assert_eq!(applied.transcript.words()[1].segment, 0);
    assert_eq!(applied.transcript.words()[1].probability, 1.0);
}

#[test]
fn corrections_from_another_clock_or_beyond_the_audio_do_not_apply() {
    let recognized = proposal();
    let corrections = empty();
    let current = corrections.apply_to_transcript(&recognized).unwrap();
    let corrected = corrections
        .edit_word_text(&current, 4, "then.", &[])
        .unwrap();
    let moved = Transcript::new(
        AnalysedAudio {
            origin: ORIGIN + 1,
            ..audio(1_000)
        },
        recognized.words().to_vec(),
    )
    .unwrap();
    let applied = corrected.apply_to_transcript(&moved).unwrap();
    assert_eq!(applied.transcript, moved);
    assert_eq!(applied.skipped, 1);
    let short = Transcript::new(audio(150), recognized.words()[..4].to_vec()).unwrap();
    assert_eq!(corrected.apply_to_transcript(&short).unwrap().skipped, 1);
    assert!(
        corrected
            .replace_words(&moved_current(&moved), 0..1, vec![])
            .is_err()
    );
}

#[test]
fn a_zero_length_word_at_the_end_of_the_audio_is_refused_not_dropped() {
    let recognized = proposal();
    let corrections = empty();
    let current = corrections.apply_to_transcript(&recognized).unwrap();
    let result = corrections.replace_words(
        &current,
        5..5,
        vec![CorrectedWord {
            text: "end".into(),
            start_cs: 1_000,
            end_cs: 1_000,
        }],
    );
    assert!(
        matches!(result, Err(CorrectionError::Refused(_))),
        "{result:?}"
    );
    // Inside the audio a zero-length word is kept.
    let kept = corrections
        .replace_words(
            &current,
            5..5,
            vec![CorrectedWord {
                text: "end".into(),
                start_cs: 900,
                end_cs: 900,
            }],
        )
        .unwrap()
        .apply_to_transcript(&recognized)
        .unwrap();
    assert_eq!(texts(&kept.transcript)[5], ("end".into(), 900, 900));
}

fn moved_current(transcript: &Transcript) -> CorrectedTranscript {
    CorrectedTranscript::recognized(transcript.clone())
}

#[test]
fn stored_corrections_round_trip_and_reject_invalid_regions() {
    let recognized = proposal();
    let corrections = empty();
    let current = corrections.apply_to_transcript(&recognized).unwrap();
    let corrected = corrections.merge_words(&current, 2).unwrap();
    let json = serde_json::to_string(&corrected).unwrap();
    assert!(json.contains(CORRECTION_RULE));
    assert_eq!(
        serde_json::from_str::<Corrections>(&json).unwrap(),
        corrected
    );
    let clock = corrected.clock();
    let overlapping = vec![
        WordCorrection {
            start_cs: 0,
            end_cs: 20,
            words: vec![],
        },
        WordCorrection {
            start_cs: 10,
            end_cs: 30,
            words: vec![],
        },
    ];
    assert!(Corrections::new(clock, overlapping, vec![]).is_err());
    let spaced = vec![WordCorrection {
        start_cs: 0,
        end_cs: 20,
        words: vec![CorrectedWord {
            text: "two words".into(),
            start_cs: 0,
            end_cs: 20,
        }],
    }];
    assert!(Corrections::new(clock, spaced, vec![]).is_err());
    let outside = vec![PauseCorrection {
        start: 100,
        end: 200,
        pauses: vec![Pause {
            start: 150,
            end: 250,
        }],
    }];
    assert!(Corrections::new(clock, vec![], outside).is_err());
    assert!(serde_json::from_str::<Corrections>(&json.replace(CORRECTION_RULE, "other")).is_err());
}

/// Speech everywhere except the quiet hops, at a flat energy, so detected
/// pauses are the quiet runs widened by the rule's 60 ms advance.
fn activity(samples: u64, quiet: &[(u64, u64)]) -> SpeechActivity {
    let hops = samples.div_ceil(VAD_HOP);
    let speech = (0..hops)
        .map(|hop| {
            if quiet
                .iter()
                .any(|(start, end)| (*start..*end).contains(&hop))
            {
                0
            } else {
                255
            }
        })
        .collect();
    let energy = vec![60; samples.div_ceil(ENERGY_HOP) as usize];
    SpeechActivity::new(
        ActivityAudio {
            origin: ORIGIN,
            sample_rate: RATE,
            samples,
        },
        speech,
        energy,
    )
    .unwrap()
}

#[test]
fn pauses_can_be_removed_added_and_resized() {
    let detected = activity(160_000, &[(20, 30), (100, 110)]);
    assert_eq!(detected.pauses().len(), 2);
    let corrections = Corrections::empty(Corrections::clock_of_activity(&detected));
    let current = corrections.apply_to_pauses(&detected);
    assert_eq!(current.pauses, detected.pauses());

    let removed = corrections
        .replace_pauses(&detected, &current, 0..1, vec![])
        .unwrap();
    let applied = removed.apply_to_pauses(&detected);
    assert_eq!(applied.pauses, detected.pauses()[1..]);

    let added = removed
        .add_pause(
            &detected,
            &applied,
            Pause {
                start: 80_000,
                end: 84_000,
            },
        )
        .unwrap();
    let with_new = added.apply_to_pauses(&detected);
    let kept = detected.pauses()[1];
    let new = Pause {
        start: 80_000,
        end: 84_000,
    };
    assert_eq!(with_new.pauses, [kept, new]);
    assert_eq!(with_new.corrected, [false, true]);

    let (resized, start, end) = added
        .set_pause_bounds(&detected, &with_new, 1, 79_000, 85_000)
        .unwrap();
    assert_eq!((start, end), (79_000, 85_000));
    assert_eq!(
        resized.apply_to_pauses(&detected).pauses[1],
        Pause {
            start: 79_000,
            end: 85_000
        }
    );
    // A pause cannot grow over its neighbour; touching joins it.
    let (joined, start, _) = added
        .set_pause_bounds(&detected, &with_new, 1, 0, 84_000)
        .unwrap();
    assert_eq!(start, kept.end);
    let joined = joined.apply_to_pauses(&detected);
    assert_eq!(
        joined.pauses,
        [Pause {
            start: kept.start,
            end: 84_000
        }]
    );
    assert_eq!(joined.corrected, [true]);
}

#[test]
fn detection_again_keeps_corrected_pauses_and_drops_short_remainders() {
    let detected = activity(160_000, &[(20, 30)]);
    let corrections = Corrections::empty(Corrections::clock_of_activity(&detected));
    let current = corrections.apply_to_pauses(&detected);
    let removed = corrections
        .replace_pauses(&detected, &current, 0..1, vec![])
        .unwrap();
    // A new detection finds a slightly wider pause: its remainders outside
    // the corrected region are too short to be pauses.
    let again = activity(160_000, &[(19, 31)]);
    assert!(again.pauses()[0].start < detected.pauses()[0].start);
    assert!(removed.apply_to_pauses(&again).pauses.is_empty());
    // A much longer pause keeps its part outside the region.
    let longer = activity(160_000, &[(20, 60)]);
    let kept = removed.apply_to_pauses(&longer).pauses;
    assert_eq!(
        kept,
        [Pause {
            start: detected.pauses()[0].end,
            end: longer.pauses()[0].end
        }]
    );
}

#[test]
fn edges_are_large_energy_steps_and_pause_bounds() {
    let mut energy = vec![60_u8; 100];
    energy[40..60].fill(160);
    let speech = vec![255; 16_000_u64.div_ceil(VAD_HOP) as usize];
    let measured = SpeechActivity::new(
        ActivityAudio {
            origin: ORIGIN,
            sample_rate: RATE,
            samples: 16_000,
        },
        speech,
        energy,
    )
    .unwrap();
    let pause = Pause {
        start: 1_000,
        end: 4_000,
    };
    let edges = measured_edges(&measured, &[pause]);
    assert_eq!(edges, [1_000, 4_000, 40 * 160, 60 * 160]);
    assert_eq!(next_edge(&edges, 4_000, true), Some(6_400));
    assert_eq!(next_edge(&edges, 4_000, false), Some(1_000));
    assert_eq!(next_edge(&edges, 9_600, true), None);
    let transcript = Transcript::new(audio(100), vec![]).unwrap();
    assert_eq!(
        edges_in_centiseconds(&edges, &transcript, &measured),
        [6, 25, 40, 60]
    );
}

#[derive(Debug, Clone)]
enum Operation {
    Text(usize, String),
    Merge(usize),
    Bounds(usize, u32, u32),
}

fn operation() -> impl Strategy<Value = Operation> {
    prop_oneof![
        (0..8_usize, "[a-z]{1,5}( [a-z]{1,5}){0,2}|")
            .prop_map(|(at, text)| Operation::Text(at, text)),
        (0..8_usize).prop_map(Operation::Merge),
        (0..8_usize, 0..300_u32, 0..300_u32).prop_map(|(at, a, b)| Operation::Bounds(at, a, b)),
    ]
}

proptest! {
    /// Whatever sequence of corrections a person makes, reapplying the stored
    /// corrections to the recognized words shows exactly what they saw.
    #[test]
    fn reapplied_corrections_reproduce_the_edited_words(
        operations in proptest::collection::vec(operation(), 1..12)
    ) {
        let recognized = proposal();
        let mut corrections = empty();
        let mut current = corrections.apply_to_transcript(&recognized).unwrap();
        for operation in operations {
            let count = current.transcript.words().len();
            if count == 0 {
                break;
            }
            let (next, expected) = match operation {
                Operation::Text(at, text) => {
                    let at = at % count;
                    match corrections.edit_word_text(&current, at, &text, &[]) {
                        Ok(next) => {
                            let mut words = texts(&current.transcript);
                            let parts: Vec<_> = text.split_whitespace().collect();
                            let replaced: Vec<_> = words.splice(at..at + 1, Vec::new()).collect();
                            let (start, end) = (replaced[0].1, replaced[0].2);
                            let added: Vec<_> = parts.iter().map(|part| (part.to_string(), start, end)).collect();
                            words.splice(at..at, added);
                            (next, Some((words, at, parts.len())))
                        }
                        Err(_) => continue,
                    }
                }
                Operation::Merge(at) => match corrections.merge_words(&current, at % count) {
                    Ok(next) => (next, None),
                    Err(_) => continue,
                },
                Operation::Bounds(at, a, b) => {
                    match corrections.set_word_bounds(&current, at % count, a.min(b), a.max(b)) {
                        Ok((next, _, _)) => (next, None),
                        Err(_) => continue,
                    }
                }
            };
            let applied = next.apply_to_transcript(&recognized).unwrap();
            if let Some((words, at, parts)) = expected {
                // Text and the outer bounds of a split word are exact.
                let seen = texts(&applied.transcript);
                prop_assert_eq!(seen.len(), words.len());
                for (index, (seen, wanted)) in seen.iter().zip(&words).enumerate() {
                    prop_assert_eq!(&seen.0, &wanted.0);
                    if index < at || index >= at + parts {
                        prop_assert_eq!(seen, wanted);
                    }
                }
            }
            // Reapplying is stable: the result of applying equals itself again.
            let again = next.apply_to_transcript(&recognized).unwrap();
            prop_assert_eq!(&again.transcript, &applied.transcript);
            corrections = next;
            current = applied;
        }
    }
}
