use super::*;
use crate::refinement::{
    AnalysisSample, CONTEXT_GUARD_RULE, ContextGuard, PhoneSpan, Pronunciation, REFINEMENT_RULE,
    WordProposal,
};
use crate::{AnalysedAudio, Word, WordCorrection};

fn base(count: usize) -> Transcript {
    Transcript::new(
        AnalysedAudio {
            origin: -12_345,
            sample_rate: 44_100,
            duration_cs: count as u32 * 100 + 50,
        },
        (0..count)
            .map(|i| Word {
                text: format!("word{i},"),
                start_cs: i as u32 * 100 + 20,
                end_cs: i as u32 * 100 + 40,
                probability: 0.6,
                segment: i as u32,
            })
            .collect(),
    )
    .unwrap()
}

fn capture(
    base: &Transcript,
    corrections: &Corrections,
    selected: Range<usize>,
) -> RefinementCapture {
    let current = corrections.apply_to_transcript(base).unwrap();
    RefinementCapture::new(
        base,
        &current,
        corrections,
        selected,
        u64::from(base.audio().duration_cs) * SAMPLES_PER_CS,
    )
    .unwrap()
}

fn proposal(capture: &RefinementCapture, changes: &[(usize, u64, u64)]) -> RefinementProposal {
    let words = capture.current.transcript.words();
    let proposals = capture
        .context_words
        .clone()
        .enumerate()
        .map(|(index, global_index)| {
            let visible = &words[global_index];
            let (start, end) = changes.iter().find(|(i, _, _)| *i == global_index).map_or(
                (
                    u64::from(visible.start_cs) * 160,
                    u64::from(visible.end_cs) * 160,
                ),
                |(_, a, b)| (*a, *b),
            );
            let local_start = u32::try_from(start - capture.window.start.0).unwrap();
            let local_end = u32::try_from(end - capture.window.start.0).unwrap();
            WordProposal {
                index,
                word: normalize_words(&visible.text).unwrap().remove(0),
                pronunciation: Pronunciation {
                    labels: vec![1],
                    dictionary_line: 1,
                    dictionary_spelling: "word".to_owned(),
                },
                phones: vec![PhoneSpan {
                    label: 1,
                    start_frame: local_start / 320,
                    end_frame: local_end.div_ceil(320),
                    mean_posterior: 0.9,
                    minimum_posterior: 0.8,
                }],
                start_frame: local_start / 320,
                end_frame: local_end.div_ceil(320),
                interval: AnalysisInterval {
                    start: AnalysisSample(local_start),
                    end: AnalysisSample(local_end),
                },
                mean_phone_frame_posterior: 0.9,
                minimum_phone_mean_posterior: 0.9,
                availability: ProposalAvailability::Approximate,
            }
        })
        .collect();
    RefinementProposal {
        rule: REFINEMENT_RULE,
        guard_rule: CONTEXT_GUARD_RULE,
        words: proposals,
        path_score: -10.0,
        path_score_per_frame: -0.1,
        frame_label_path: vec![0; ((capture.input_samples() / 160 + 1).div_ceil(2)) as usize],
        frame_blank_fraction: 1.0,
        context_guard: ContextGuard {
            greedy_phones: Vec::new(),
            selected_pronunciations: Vec::new(),
            minimum_edit_distance: 0,
            alignment_steps: Vec::new(),
            residual_runs: Vec::new(),
            abstain: false,
        },
    }
}

#[test]
fn capture_keeps_complete_neighboring_segments_and_exact_global_offset() {
    let base = base(7);
    let corrections = Corrections::empty(Corrections::clock_of(&base));
    let capture = capture(&base, &corrections, 3..4);
    assert_eq!(capture.context_words(), 2..5);
    assert_eq!(capture.window().start.0, 180 * 160);
    assert_eq!(capture.window().end.0, 480 * 160);
    assert_eq!(capture.context_text(), "word2, word3, word4,");
    let raw = proposal(&capture, &[(3, 321 * 160 + 3, 339 * 160 + 5)]);
    let review = capture.review(raw).unwrap();
    assert_eq!(
        review.offers()[0].raw_global_interval.unwrap(),
        GlobalAnalysisInterval {
            start: GlobalAnalysisSample(321 * 160 + 3),
            end: GlobalAnalysisSample(339 * 160 + 5)
        }
    );
    assert_eq!(
        review.offers()[0].centisecond_bounds.unwrap(),
        CentisecondBounds {
            start_cs: 321,
            end_cs: 340
        }
    );
    let current = corrections.apply_to_transcript(&base).unwrap();
    let accepted = review.accept(&base, &current, &corrections, &[3]).unwrap();
    let after = accepted.apply_to_transcript(&base).unwrap();
    assert_eq!(after.transcript.words()[3].start_cs, 321);
    assert_eq!(after.transcript.words()[3].end_cs, 340);
    for i in [0, 1, 2, 4, 5, 6] {
        assert_eq!(after.transcript.words()[i], current.transcript.words()[i]);
        assert_eq!(after.corrected(i), current.corrected(i));
    }
    assert_eq!(after.transcript.audio().origin, -12_345);
    assert_eq!(after.transcript.audio().sample_rate, 44_100);
}

#[test]
fn stale_transcript_origin_corrections_origins_and_hidden_base_all_refuse() {
    let base = base(4);
    let corrections = Corrections::new(
        Corrections::clock_of(&base),
        vec![WordCorrection {
            start_cs: 220,
            end_cs: 240,
            words: vec![CorrectedWord {
                text: "pinned".to_owned(),
                start_cs: 220,
                end_cs: 240,
            }],
        }],
        Vec::new(),
    )
    .unwrap();
    let current = corrections.apply_to_transcript(&base).unwrap();
    let capture = capture(&base, &corrections, 1..2);
    let raw = proposal(&capture, &[(1, 122 * 160, 138 * 160)]);
    let review = capture.review(raw).unwrap();
    let mut origin_changed = current.clone();
    origin_changed.origins[1] = WordOrigin::Corrected(0);
    assert_eq!(
        review.accept(&base, &origin_changed, &corrections, &[1]),
        Err(RefinementSelectionError::Stale)
    );
    let mut words = base.words().to_vec();
    words[2].text = "different hidden recognition".to_owned();
    let hidden_changed = Transcript::new(base.audio(), words).unwrap();
    assert_eq!(
        corrections.apply_to_transcript(&hidden_changed).unwrap(),
        current
    );
    assert_eq!(
        review.accept(&hidden_changed, &current, &corrections, &[1]),
        Err(RefinementSelectionError::Stale)
    );
    let other = Corrections::empty(Corrections::clock_of(&base));
    assert_eq!(
        review.accept(&base, &current, &other, &[1]),
        Err(RefinementSelectionError::Stale)
    );
    let mut audio = base.audio();
    audio.origin += 1;
    let different_clock = Transcript::new(audio, base.words().to_vec()).unwrap();
    assert_eq!(
        review.accept(&different_clock, &current, &corrections, &[1]),
        Err(RefinementSelectionError::Stale)
    );
}

#[test]
fn pinned_and_unavailable_diagnostics_never_become_accepted() {
    let base = base(4);
    let corrections = Corrections::new(
        Corrections::clock_of(&base),
        vec![WordCorrection {
            start_cs: 120,
            end_cs: 140,
            words: vec![CorrectedWord {
                text: "manual".to_owned(),
                start_cs: 120,
                end_cs: 140,
            }],
        }],
        Vec::new(),
    )
    .unwrap();
    let current = corrections.apply_to_transcript(&base).unwrap();
    let capture = capture(&base, &corrections, 1..3);
    let mut raw = proposal(
        &capture,
        &[(1, 122 * 160, 138 * 160), (2, 222 * 160, 238 * 160)],
    );
    let local = 2 - capture.context_words.start;
    raw.words[local].availability = ProposalAvailability::Unavailable(vec![
        UnavailableReason::ContextIncompleteOrPlacementAmbiguous,
    ]);
    let raw_interval = raw.words[local].interval;
    let review = capture.review(raw).unwrap();
    assert!(matches!(&review.offers()[0].availability,
        RefinementOfferAvailability::Unavailable(r) if r.contains(&OfferUnavailableReason::ManuallyPinned)));
    assert_eq!(review.diagnostics().words[local].interval, raw_interval);
    assert_eq!(
        review.accept(&base, &current, &corrections, &[1]),
        Err(RefinementSelectionError::Unavailable(1))
    );
    assert_eq!(
        review.accept(&base, &current, &corrections, &[2]),
        Err(RefinementSelectionError::Unavailable(2))
    );
    assert_eq!(
        review.accept(&base, &current, &corrections, &[1, 2]),
        Err(RefinementSelectionError::Unavailable(1))
    );
    assert_eq!(corrections.apply_to_transcript(&base).unwrap(), current);
}

#[test]
fn offers_crossing_visible_or_pinned_neighbors_refuse_without_trimming() {
    let base = base(4);
    let corrections = Corrections::empty(Corrections::clock_of(&base));
    let current = corrections.apply_to_transcript(&base).unwrap();
    for (start, end, reason) in [
        (39, 138, OfferUnavailableReason::CrossesPreviousWord),
        (122, 221, OfferUnavailableReason::CrossesNextWord),
    ] {
        let capture = capture(&base, &corrections, 1..2);
        let raw = proposal(&capture, &[(1, start * 160, end * 160)]);
        let review = capture.review(raw).unwrap();
        assert!(matches!(&review.offers()[0].availability,
            RefinementOfferAvailability::Unavailable(r) if r.contains(&reason)));
        assert_eq!(
            review.accept(&base, &current, &corrections, &[1]),
            Err(RefinementSelectionError::Unavailable(1))
        );
    }
    assert_eq!(corrections.apply_to_transcript(&base).unwrap(), current);
}

#[test]
fn accepting_multiple_words_is_one_region_and_preserves_later_pin_owner() {
    let base = base(5);
    let corrections = Corrections::new(
        Corrections::clock_of(&base),
        vec![WordCorrection {
            start_cs: 420,
            end_cs: 440,
            words: vec![CorrectedWord {
                text: "manual".to_owned(),
                start_cs: 420,
                end_cs: 440,
            }],
        }],
        Vec::new(),
    )
    .unwrap();
    let current = corrections.apply_to_transcript(&base).unwrap();
    let capture = capture(&base, &corrections, 1..3);
    let raw = proposal(
        &capture,
        &[(1, 122 * 160, 138 * 160), (2, 222 * 160, 238 * 160)],
    );
    let review = capture.review(raw).unwrap();
    let accepted = review
        .accept(&base, &current, &corrections, &[2, 1])
        .unwrap();
    assert_eq!(accepted.word_regions().len(), 2);
    let after = accepted.apply_to_transcript(&base).unwrap();
    assert_eq!(after.transcript.words()[4], current.transcript.words()[4]);
    assert!(after.corrected(4));
    assert_eq!(accepted.word_regions()[1], corrections.word_regions()[0]);
    assert_eq!(after.transcript.words()[1].text, "word1,");
    assert_eq!(after.transcript.words()[2].text, "word2,");
    assert_eq!(after.transcript.words()[1].start_cs, 122);
    assert_eq!(after.transcript.words()[2].end_cs, 238);
    assert_eq!(after.transcript.words()[0], current.transcript.words()[0]);
    assert_eq!(after.transcript.words()[3], current.transcript.words()[3]);
}

#[test]
fn exact_fractional_centisecond_terminal_is_retained_and_ceil_is_explicit() {
    let mut audio = base(1).audio();
    audio.duration_cs = 101;
    let base = Transcript::new(
        audio,
        vec![Word {
            text: "word,".to_owned(),
            start_cs: 20,
            end_cs: 101,
            probability: 0.5,
            segment: 0,
        }],
    )
    .unwrap();
    let corrections = Corrections::empty(Corrections::clock_of(&base));
    let current = corrections.apply_to_transcript(&base).unwrap();
    let full_samples = 100 * 160 + 23;
    let capture =
        RefinementCapture::new(&base, &current, &corrections, 0..1, full_samples).unwrap();
    assert_eq!(capture.window().end.0, full_samples);
    let raw = proposal(&capture, &[(0, 22 * 160, full_samples)]);
    let review = capture.review(raw).unwrap();
    assert_eq!(review.offers()[0].centisecond_bounds.unwrap().end_cs, 101);
    let accepted = review.accept(&base, &current, &corrections, &[0]).unwrap();
    assert_eq!(
        accepted
            .apply_to_transcript(&base)
            .unwrap()
            .transcript
            .words()[0]
            .end_cs,
        101
    );
    assert!(RefinementCapture::new(&base, &current, &corrections, 0..1, 99 * 160).is_err());
}

#[test]
fn oversized_or_overlapping_context_seams_are_not_truncated() {
    let base = base(70);
    let corrections = Corrections::empty(Corrections::clock_of(&base));
    let mut words = base.words().to_vec();
    for word in &mut words {
        word.segment = 0;
    }
    let long_segment = Transcript::new(base.audio(), words).unwrap();
    let current = corrections.apply_to_transcript(&long_segment).unwrap();
    assert!(matches!(
        RefinementCapture::new(
            &long_segment,
            &current,
            &corrections,
            30..31,
            u64::from(base.audio().duration_cs) * 160
        ),
        Err(RefinementSelectionError::Context(_))
    ));
    let mut words = base.words().to_vec();
    words[0].end_cs = 121;
    let overlap = Transcript::new(base.audio(), words).unwrap();
    let current = corrections.apply_to_transcript(&overlap).unwrap();
    assert!(matches!(
        RefinementCapture::new(
            &overlap,
            &current,
            &corrections,
            2..3,
            u64::from(base.audio().duration_cs) * 160
        ),
        Err(RefinementSelectionError::Context(_))
    ));
}

#[test]
fn unselected_middle_word_cannot_be_incidentally_pinned_by_multiword_acceptance() {
    let base = base(5);
    let corrections = Corrections::empty(Corrections::clock_of(&base));
    let current = corrections.apply_to_transcript(&base).unwrap();
    let capture = capture(&base, &corrections, 1..4);
    let raw = proposal(
        &capture,
        &[(1, 122 * 160, 138 * 160), (3, 322 * 160, 338 * 160)],
    );
    let review = capture.review(raw).unwrap();
    assert_eq!(
        review.accept(&base, &current, &corrections, &[1, 3]),
        Err(RefinementSelectionError::WouldChangeOtherWords)
    );
    assert_eq!(corrections.apply_to_transcript(&base).unwrap(), current);
}
