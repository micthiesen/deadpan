use super::*;
use proptest::prelude::*;

fn token(text: &str, t0: i64, t1: i64, p: f32) -> RawToken {
    RawToken {
        text: text.into(),
        t0,
        t1,
        p,
    }
}

/// Recorded whisper.cpp 1.8.3 base.en output for synthesized speech
/// ("The interview, made weird. Absolutely. I think the answer is absolutely
/// not. Let me say that again, absolutely.").
fn recorded() -> Vec<RawSegment> {
    vec![
        RawSegment {
            t0: 0,
            t1: 280,
            tokens: vec![
                token("[_BEG_]", 0, 0, 0.987),
                token(" The", 15, 22, 0.801),
                token(" interview", 22, 77, 0.987),
                token(",", 102, 105, 0.516),
                token(" made", 105, 135, 0.907),
                token(" weird", 135, 159, 0.999),
                token(",", 184, 184, 0.724),
                token(" absolutely", 190, 261, 0.900),
                token(".", 280, 280, 0.415),
                token("[_TT_140]", 280, 280, 0.086),
            ],
        },
        RawSegment {
            t0: 280,
            t1: 512,
            tokens: vec![
                token(" I", 287, 287, 0.979),
                token(" think", 295, 325, 0.999),
                token(" the", 325, 348, 0.996),
                token(" answer", 348, 394, 1.000),
                token(" is", 394, 409, 0.998),
                token(" absolutely", 409, 486, 0.876),
                token(" not", 497, 512, 0.995),
                token(".", 512, 512, 0.802),
            ],
        },
        RawSegment {
            t0: 512,
            t1: 736,
            tokens: vec![
                token(" Let", 527, 534, 0.986),
                token(" me", 534, 549, 0.999),
                token(" say", 549, 571, 0.999),
                token(" that", 571, 590, 0.999),
                token(" again", 614, 638, 0.998),
                token(",", 638, 655, 0.843),
                token(" absolutely", 655, 736, 0.975),
                token(".", 736, 736, 0.868),
            ],
        },
    ]
}

fn audio() -> AnalysedAudio {
    AnalysedAudio {
        origin: -1_024,
        sample_rate: 48_000,
        duration_cs: 740,
    }
}

#[test]
fn recorded_tokens_become_words_with_punctuation_segments_and_confidence() {
    let transcript = Transcript::from_segments(audio(), &recorded()).unwrap();
    let words: Vec<_> = transcript.words().iter().map(|w| w.text.as_str()).collect();
    assert_eq!(
        words,
        [
            "The",
            "interview,",
            "made",
            "weird,",
            "absolutely.",
            "I",
            "think",
            "the",
            "answer",
            "is",
            "absolutely",
            "not.",
            "Let",
            "me",
            "say",
            "that",
            "again,",
            "absolutely."
        ]
    );
    let interview = &transcript.words()[1];
    // Punctuation extends the text, never the heard interval or confidence.
    assert_eq!((interview.start_cs, interview.end_cs), (22, 77));
    assert_eq!(interview.probability, 0.987);
    assert!(!interview.approximate());
    assert!(transcript.words()[0].probability > APPROXIMATE_BELOW);
    assert_eq!(transcript.words()[5].segment, 1);
    assert_eq!(transcript.words()[17].segment, 2);
}

#[test]
fn analysis_centiseconds_map_exactly_to_original_audio_samples() {
    let transcript = Transcript::from_segments(audio(), &recorded()).unwrap();
    // 2.90 s at 48 kHz after an origin of -1024 samples.
    assert_eq!(
        transcript.original_sample(290).unwrap(),
        ExactRatio::new(290 * 480 - 1_024, 1).unwrap()
    );
    let odd = Transcript::new(
        AnalysedAudio {
            origin: 0,
            sample_rate: 44_100,
            duration_cs: 10,
        },
        vec![],
    )
    .unwrap();
    // 0.01 s at 44.1 kHz is exactly 441 samples; 0.03 s is 1323.
    assert_eq!(
        odd.original_sample(3).unwrap(),
        ExactRatio::new(1_323, 1).unwrap()
    );
}

#[test]
fn search_matches_phrases_in_order_and_prefixes_the_last_word() {
    let transcript = Transcript::from_segments(audio(), &recorded()).unwrap();
    assert_eq!(transcript.search("absolutely"), vec![4..5, 10..11, 17..18]);
    assert_eq!(transcript.search("ABSOL"), vec![4..5, 10..11, 17..18]);
    assert_eq!(transcript.search("absolutely no"), vec![10..12]);
    assert_eq!(transcript.search("the answer"), vec![7..9]);
    assert_eq!(transcript.search("weird, absolutely"), vec![3..5]);
    assert!(transcript.search("").is_empty());
    assert!(transcript.search("  ,  ").is_empty());
    assert!(transcript.search("absolutely yes").is_empty());
}

#[test]
fn word_at_uses_half_open_heard_intervals() {
    let transcript = Transcript::from_segments(audio(), &recorded()).unwrap();
    assert_eq!(transcript.word_at(22), Some(1));
    assert_eq!(transcript.word_at(76), Some(1));
    assert_eq!(transcript.word_at(77), None);
    assert_eq!(transcript.word_at(0), None);
    assert_eq!(transcript.word_at(735), Some(17));
}

#[test]
fn stored_transcripts_revalidate_and_reject_tampered_values() {
    let transcript = Transcript::from_segments(audio(), &recorded()).unwrap();
    let json = serde_json::to_value(&transcript).unwrap();
    let restored: Transcript = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(restored, transcript);
    for (path, value) in [
        ("/words/0/end_cs", serde_json::json!(9_999)),
        ("/words/1/start_cs", serde_json::json!(0)),
        ("/words/0/probability", serde_json::json!(1.5)),
        ("/words/0/text", serde_json::json!("  ")),
        ("/words/0/text", serde_json::json!("a\u{7}")),
        ("/audio/sample_rate", serde_json::json!(0)),
    ] {
        let mut tampered = json.clone();
        *tampered.pointer_mut(path).unwrap() = value;
        assert!(
            serde_json::from_value::<Transcript>(tampered).is_err(),
            "{path}"
        );
    }
    let mut unknown = json;
    unknown["extra"] = serde_json::json!(true);
    assert!(serde_json::from_value::<Transcript>(unknown).is_err());
}

#[test]
fn leading_punctuation_and_out_of_range_tokens_stay_valid_and_bounded() {
    let segments = vec![
        RawSegment {
            t0: 0,
            t1: 100,
            tokens: vec![token(" Yes", 10, 40, 0.9)],
        },
        RawSegment {
            // Starts before the previous word ended and runs past the audio.
            t0: 20,
            t1: 2_000,
            tokens: vec![token("...", 30, 30, 0.5), token(" maybe", 25, 5_000, 0.4)],
        },
    ];
    let transcript = Transcript::from_segments(audio(), &segments).unwrap();
    let words = transcript.words();
    assert_eq!(words[0].text, "Yes...");
    assert_eq!(words[1].text, "maybe");
    assert!(words[1].start_cs >= words[0].end_cs);
    assert_eq!(words[1].end_cs, 740);
    assert!(words[1].approximate());
    let invalid = vec![RawSegment {
        t0: 0,
        t1: 10,
        tokens: vec![token(" bad", 0, 5, f32::NAN)],
    }];
    assert_eq!(
        Transcript::from_segments(audio(), &invalid),
        Err(TranscriptError::Probability)
    );
}

proptest! {
    #[test]
    fn arbitrary_recognizer_output_validates_or_fails_without_panicking(
        segments in proptest::collection::vec(
            (
                -100_i64..2_000,
                -100_i64..2_000,
                proptest::collection::vec(
                    ("[ a-z,.!?']{0,6}", -100_i64..2_000, -100_i64..2_000, 0.0_f32..=1.0),
                    0..12,
                ),
            ),
            0..8,
        )
    ) {
        let segments: Vec<RawSegment> = segments
            .into_iter()
            .map(|(t0, t1, tokens)| RawSegment {
                t0,
                t1,
                tokens: tokens
                    .into_iter()
                    .map(|(text, t0, t1, p)| RawToken { text, t0, t1, p })
                    .collect(),
            })
            .collect();
        if let Ok(transcript) = Transcript::from_segments(audio(), &segments) {
            let words = transcript.words();
            for pair in words.windows(2) {
                prop_assert!(pair[0].start_cs <= pair[1].start_cs);
                prop_assert!(pair[0].end_cs <= pair[1].start_cs);
            }
            for word in words {
                prop_assert!(word.start_cs <= word.end_cs && word.end_cs <= 740);
            }
            let restored: Transcript =
                serde_json::from_value(serde_json::to_value(&transcript).unwrap()).unwrap();
            prop_assert_eq!(restored, transcript);
        }
    }
}
