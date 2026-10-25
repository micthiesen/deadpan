use super::*;
use crate::Word;

const RATE: u32 = 48_000;

fn word(text: &str, start_cs: u32, end_cs: u32) -> Word {
    Word {
        text: text.into(),
        start_cs,
        end_cs,
        probability: 0.95,
        segment: 0,
    }
}

fn transcript(duration_cs: u32, words: Vec<Word>) -> Transcript {
    Transcript::new(
        AnalysedAudio {
            origin: 1_024,
            sample_rate: RATE,
            duration_cs,
        },
        words,
    )
    .unwrap()
}

/// Words spoken one per 40 cs from `from_cs`.
fn spoken(text: &str, from_cs: u32) -> Vec<Word> {
    text.split_whitespace()
        .enumerate()
        .map(|(index, text)| {
            let start = from_cs + 40 * index as u32;
            word(text, start, start + 30)
        })
        .collect()
}

fn activity(duration_cs: u32) -> ActivityAudio {
    ActivityAudio {
        origin: 1_024,
        sample_rate: RATE,
        samples: u64::from(duration_cs) * ENERGY_HOP,
    }
}

const SECOND: u64 = ANALYSIS_SAMPLE_RATE as u64;

#[test]
fn a_confident_loop_is_flagged_at_its_shortest_period() {
    let mut text = String::from("but why didn't you tell me when you were here in the summer ");
    for _ in 0..8 {
        text.push_str("I didn't know when I was here in the morning. ");
    }
    text.push_str("and then it ended");
    let words = spoken(&text, 0);
    let count = words.len();
    let check = check_recognition(&transcript(40 * count as u32 + 100, words), None);
    assert_eq!(check.loops.len(), 1);
    let found = &check.loops[0];
    assert_eq!(found.period, 10);
    assert_eq!(found.copies, 8);
    assert_eq!(found.first_word, 13);
    assert_eq!(found.end_word, 93);
    assert_eq!(
        found.phrase,
        "I didn't know when I was here in the morning."
    );
    assert_eq!(found.start_cs, 13 * 40);
    assert_eq!(found.end_cs, 92 * 40 + 30);
    assert_eq!(check.longest_repetition.as_ref(), Some(found));
    assert!(check.flagged());
    assert!(matches!(check.coverage, Coverage::Unchecked { .. }));
}

#[test]
fn ordinary_repetition_is_not_a_loop() {
    // Emphatic repeats and a short refrain stay below both thresholds.
    let text = "no no no no no no I said it again and again and again \
                thank you thank you thank you so much for having me";
    let words = spoken(text, 0);
    let count = words.len();
    let check = check_recognition(&transcript(40 * count as u32 + 100, words), None);
    assert!(check.loops.is_empty(), "{:?}", check.loops);
    let longest = check.longest_repetition.unwrap();
    assert_eq!((longest.period, longest.copies), (1, 6), "{longest:?}");
}

#[test]
fn a_loop_needs_both_copies_and_words() {
    // 12 copies of a 1-word phrase: copies enough, too few words.
    let words = spoken(&"ha ".repeat(12), 0);
    assert!(
        check_recognition(&transcript(1_000, words), None)
            .loops
            .is_empty()
    );
    // 3 copies of a 10-word phrase: words enough, too few copies.
    let words = spoken(
        &"one two three four five six seven eight nine ten ".repeat(3),
        0,
    );
    assert!(
        check_recognition(&transcript(2_000, words), None)
            .loops
            .is_empty()
    );
    // 4 copies of a 6-word phrase spans exactly 24 words.
    let words = spoken(&"one two three four five six ".repeat(4), 0);
    let check = check_recognition(&transcript(2_000, words), None);
    assert_eq!(check.loops.len(), 1);
    assert_eq!((check.loops[0].period, check.loops[0].copies), (6, 4));
}

#[test]
fn punctuation_and_case_do_not_hide_a_loop() {
    let mut words = Vec::new();
    for copy in 0..6 {
        let base = copy * 200;
        words.push(word("Thank", base, base + 30));
        words.push(word("you,", base + 40, base + 70));
        words.push(word("—", base + 80, base + 80));
        words.push(word("really.", base + 90, base + 120));
    }
    let check = check_recognition(&transcript(1_300, words), None);
    // Three text words per copy, 18 words: below the word threshold.
    assert!(check.loops.is_empty());
    assert_eq!(check.longest_repetition.unwrap().copies, 6);
}

#[test]
fn speech_after_the_last_word_is_flagged() {
    // 23 s of audio: one heading, then 20 s of speech with no words.
    let words = spoken("chapter seven on the races of man", 0);
    let duration = 2_300;
    let pauses = [Pause {
        start: 3 * SECOND,
        end: 3 * SECOND + 8_000,
    }];
    let check = check_recognition(
        &transcript(duration, words),
        Some((activity(duration), &pauses)),
    );
    let Coverage::Checked {
        gaps,
        longest,
        speech_samples,
    } = &check.coverage
    else {
        panic!("coverage was not checked");
    };
    assert_eq!(*speech_samples, 23 * SECOND - 8_000);
    assert_eq!(gaps.len(), 1);
    let gap = &gaps[0];
    assert_eq!(gap.start, 270 * ENERGY_HOP, "the last word ends at 2.7 s");
    assert_eq!(gap.end, 23 * SECOND);
    assert_eq!(gap.speech_samples, 23 * SECOND - 270 * ENERGY_HOP - 8_000);
    assert_eq!(gap.previous_word, Some(6));
    assert_eq!(gap.next_word, None);
    assert_eq!(longest.as_ref(), Some(gap));
    assert!(check.flagged());
}

#[test]
fn pauses_between_words_are_not_unrecognized_speech() {
    // A long silence between two sentences is not speech.
    let mut words = spoken("first sentence here", 0);
    words.extend(spoken("second sentence here", 2_000));
    // The last word ends at 21.1 s; 50 ms of speech follow it.
    let duration = 2_115;
    let pauses = [Pause {
        start: 120 * ENERGY_HOP,
        end: 2_000 * ENERGY_HOP,
    }];
    let check = check_recognition(
        &transcript(duration, words),
        Some((activity(duration), &pauses)),
    );
    assert!(!check.flagged());
    let Coverage::Checked { longest, .. } = check.coverage else {
        panic!("coverage was not checked");
    };
    let longest = longest.unwrap();
    assert_eq!(longest.speech_samples, 10 * ENERGY_HOP, "110..120 cs");
}

#[test]
fn speech_with_no_words_at_all_is_one_stretch() {
    let duration = 1_000;
    let check = check_recognition(
        &transcript(duration, Vec::new()),
        Some((activity(duration), &[])),
    );
    assert_eq!(
        check.gaps(),
        &[UnrecognizedSpeech {
            start: 0,
            end: 10 * SECOND,
            speech_samples: 10 * SECOND,
            previous_word: None,
            next_word: None,
        }]
    );
}

#[test]
fn zero_length_words_divide_stretches() {
    // Words every 4 s, each of zero length: no stretch reaches 5 s.
    let words = (0..5)
        .map(|index| word("so", index * 400, index * 400))
        .collect();
    let duration = 2_000;
    let check = check_recognition(
        &transcript(duration, words),
        Some((activity(duration), &[])),
    );
    assert!(check.gaps().is_empty());
    let Coverage::Checked { longest, .. } = check.coverage else {
        panic!("coverage was not checked");
    };
    assert_eq!(longest.unwrap().speech_samples, 4 * SECOND);
}

#[test]
fn mismatched_activity_is_reported_not_guessed() {
    let words = spoken("hello there", 0);
    let mut other = activity(1_000);
    other.origin += 1;
    let check = check_recognition(&transcript(1_000, words.clone()), Some((other, &[])));
    assert!(matches!(check.coverage, Coverage::Unchecked { .. }));
    let disordered = [
        Pause {
            start: 200,
            end: 400,
        },
        Pause {
            start: 300,
            end: 500,
        },
    ];
    let check = check_recognition(
        &transcript(1_000, words),
        Some((activity(1_000), &disordered)),
    );
    assert!(matches!(check.coverage, Coverage::Unchecked { .. }));
}

#[test]
fn the_check_never_changes_the_transcript() {
    let mut text = String::new();
    for _ in 0..10 {
        text.push_str("over and over and over again ");
    }
    let words = spoken(&text, 0);
    let original = transcript(3_000, words);
    let copy = original.clone();
    let check = check_recognition(&original, Some((activity(3_000), &[])));
    assert!(!check.loops.is_empty());
    assert_eq!(original, copy);
}

#[test]
fn a_loop_passage_runs_between_its_unflagged_neighbors() {
    let mut text = String::from("we met in the summer ");
    for _ in 0..5 {
        text.push_str("I didn't know when I was here ");
    }
    text.push_str("goodbye");
    let words = spoken(&text, 100);
    let count = words.len();
    let duration = 100 + 40 * count as u32 + 100;
    let check = check_recognition(&transcript(duration, words.clone()), None);
    assert_eq!(check.passages.len(), 1);
    let passage = &check.passages[0];
    assert_eq!(passage.start_cs, words[4].end_cs, "after 'summer'");
    assert_eq!(
        passage.end_cs,
        words[count - 1].start_cs,
        "before 'goodbye'"
    );
    assert_eq!(passage.word, Some(5));
    assert_eq!((passage.loops, passage.unrecognized), (1, 0));
    assert!(passage.can_recognize_again());
}

#[test]
fn overlapping_flags_merge_into_one_passage() {
    // A loop of zero-length words at the end, then 10 s of speech.
    let mut words = spoken("it began", 0);
    for copy in 0..30 {
        words.push(word("again", 100 + copy, 100 + copy));
    }
    let duration = 1_200;
    let check = check_recognition(
        &transcript(duration, words),
        Some((activity(duration), &[])),
    );
    assert_eq!(check.loops.len(), 1);
    assert_eq!(check.gaps().len(), 1);
    assert_eq!(check.passages.len(), 1, "{:?}", check.passages);
    let passage = &check.passages[0];
    assert_eq!((passage.start_cs, passage.end_cs), (70, 1_200));
    assert_eq!((passage.loops, passage.unrecognized), (1, 1));
}

#[test]
fn recognizing_a_passage_again_replaces_only_its_words_once() {
    let words = spoken("chapter seven on the races of man", 0);
    let duration = 2_300;
    let first = transcript(duration, words);
    let check = check_recognition(&first, Some((activity(duration), &[])));
    let passage = check.passages[0].region();
    assert_eq!((passage.start_cs, passage.end_cs), (270, 2_300));
    // The second pass heard the passage on its own.
    let pass = Transcript::new(
        AnalysedAudio {
            origin: 0,
            sample_rate: RATE,
            duration_cs: passage.end_cs - passage.start_cs,
        },
        spoken("in determining whether two forms are species", 10),
    )
    .unwrap();
    let again = first.recognize_again(passage, &pass).unwrap();
    assert_eq!(again.words()[..7], first.words()[..7]);
    assert_eq!(again.words().len(), 14);
    assert_eq!(again.words()[7].text, "in");
    assert_eq!(again.words()[7].start_cs, 280);
    assert_eq!(
        again.words()[7].segment,
        1,
        "a new sentence after the heading"
    );
    assert_eq!(again.recognized_again(), &[passage]);
    // The result still reports what it checks, and offers no second attempt.
    let check = check_recognition(&again, Some((activity(duration), &[])));
    assert!(
        check
            .passages
            .iter()
            .all(|passage| !passage.can_recognize_again())
    );
    assert!(matches!(
        again.recognize_again(passage, &pass),
        Err(crate::TranscriptError::Timing(_))
    ));
    // Provenance survives storage.
    let stored = serde_json::to_string(&again).unwrap();
    assert!(stored.contains("recognized_again"));
    assert_eq!(serde_json::from_str::<Transcript>(&stored).unwrap(), again);
    let plain = serde_json::to_string(&first).unwrap();
    assert!(
        !plain.contains("recognized_again"),
        "unchanged form without it"
    );
}

#[test]
fn a_second_pass_must_cover_exactly_its_passage() {
    let first = transcript(1_000, spoken("hello there", 0));
    let passage = RecognizedAgain {
        start_cs: 100,
        end_cs: 600,
    };
    let wrong = transcript(400, Vec::new());
    assert!(first.recognize_again(passage, &wrong).is_err());
    let too_long = RecognizedAgain {
        start_cs: 0,
        end_cs: 1_001,
    };
    assert!(first.recognize_again(too_long, &wrong).is_err());
}
