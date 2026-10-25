use super::alignment::{Graph, align};
use super::*;
use serde_json::Value;

fn control() -> impl FnMut() -> bool {
    || false
}

fn pronunciation(labels: Vec<u8>, index: usize) -> Pronunciation {
    Pronunciation {
        labels,
        dictionary_line: index as u32 + 1,
        dictionary_spelling: format!("test({index})"),
    }
}

fn enumerate(choices: &[Vec<Pronunciation>]) -> Vec<Vec<usize>> {
    let mut paths = vec![Vec::new()];
    for word in choices {
        paths = paths
            .into_iter()
            .flat_map(|prefix| {
                (0..word.len()).map(move |i| {
                    let mut result = prefix.clone();
                    result.push(i);
                    result
                })
            })
            .collect();
    }
    paths
}

/// Deliberately independent straight-chain reference recurrence.
fn brute(logp: &[f64], frames: usize, phones: &[u8]) -> Option<(f64, Vec<u8>)> {
    let states = phones.len() * 2 + 1;
    let labels: Vec<_> = (0..states)
        .map(|s| if s % 2 == 0 { 0 } else { phones[s / 2] })
        .collect();
    let mut previous = vec![f64::NEG_INFINITY; states];
    previous[0] = logp[0];
    previous[1] = logp[usize::from(phones[0])];
    let mut trace = vec![0usize; frames * states];
    for t in 1..frames {
        let mut current = vec![f64::NEG_INFINITY; states];
        for s in 0..states {
            let mut from = s;
            if s >= 1 && previous[s - 1] > previous[from] {
                from = s - 1;
            }
            if s >= 3
                && s % 2 == 1
                && labels[s] != labels[s - 2]
                && previous[s - 2] > previous[from]
            {
                from = s - 2;
            }
            current[s] = previous[from] + logp[t * LABELS.len() + usize::from(labels[s])];
            trace[t * states + s] = from;
        }
        previous = current;
    }
    let last = if previous[states - 1] >= previous[states - 2] {
        states - 1
    } else {
        states - 2
    };
    if !previous[last].is_finite() {
        return None;
    }
    let mut path = vec![last; frames];
    for t in (1..frames).rev() {
        path[t - 1] = trace[t * states + path[t]];
    }
    Some((previous[last], path.iter().map(|s| labels[*s]).collect()))
}

#[test]
fn dag_matches_cartesian_reference_including_repeats_and_exact_ties() {
    let mut random = 0x53ab12u64;
    let mut next = || {
        random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
        random >> 32
    };
    for case in 0..120 {
        let words = 1 + next() as usize % 3;
        let choices: Vec<Vec<Pronunciation>> = (0..words)
            .map(|_| {
                (0..(1 + next() as usize % 3))
                    .map(|i| {
                        let labels = (0..(1 + next() as usize % 3))
                            .map(|_| (1 + next() % 3) as u8)
                            .collect();
                        pronunciation(labels, i)
                    })
                    .collect()
            })
            .collect();
        let frames = 3 + next() as usize % 14;
        let logp: Vec<_> = (0..frames * LABELS.len())
            .map(|_| {
                if case % 4 == 0 {
                    -(41f64).ln()
                } else {
                    -((1 + next() % 30) as f64) / 5.0
                }
            })
            .collect();
        let mut expected: Option<(f64, Vec<u8>, Vec<usize>)> = None;
        for path in enumerate(&choices) {
            let phones: Vec<_> = path
                .iter()
                .enumerate()
                .flat_map(|(w, a)| choices[w][*a].labels.iter().copied())
                .collect();
            if let Some((score, labels)) = brute(&logp, frames, &phones)
                && expected.as_ref().is_none_or(|p| score > p.0)
            {
                expected = Some((score, labels, path));
            }
        }
        let borrowed: Vec<_> = choices.iter().map(Vec::as_slice).collect();
        let graph = Graph::new(&borrowed).unwrap();
        let actual = align(
            &logp,
            frames,
            &graph,
            &mut RefinementControl::new(&mut control()),
        );
        if let Some((score, labels, path)) = expected {
            let actual = actual.unwrap();
            assert_eq!(actual.score, score, "case {case}");
            assert_eq!(actual.labels, labels, "case {case}");
            assert_eq!(actual.choices, path, "case {case}");
        } else {
            assert!(matches!(actual, Err(RefinementError::Unalignable)));
        }
        let inspection = super::guard::inspect(
            &logp,
            frames,
            &graph,
            &borrowed,
            &mut RefinementControl::new(&mut control()),
        )
        .unwrap();
        let mut minimum = u32::MAX;
        let mut best = Vec::new();
        let mut best_steps = Vec::new();
        for path in enumerate(&choices) {
            let phones: Vec<_> = path
                .iter()
                .enumerate()
                .flat_map(|(w, a)| choices[w][*a].labels.iter().copied())
                .collect();
            let (distance, steps) = super::guard::edit(
                &phones,
                &inspection.greedy_phones,
                &mut RefinementControl::new(&mut control()),
            )
            .unwrap();
            if distance < minimum {
                minimum = distance;
                best = path;
                best_steps = steps;
            }
        }
        assert_eq!(inspection.minimum_edit_distance, minimum);
        assert_eq!(inspection.alignment_steps, best_steps);
        assert_eq!(
            inspection.selected_pronunciations,
            best.iter()
                .enumerate()
                .map(|(w, a)| choices[w][*a].clone())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn sentence_with_millions_of_pronunciation_tuples_is_bounded_polynomial_work() {
    let choices = vec![vec![pronunciation(vec![1], 0), pronunciation(vec![2], 1)]; 30];
    let borrowed: Vec<_> = choices.iter().map(Vec::as_slice).collect();
    let graph = Graph::new(&borrowed).unwrap();
    let logp = vec![-1.0; 100 * LABELS.len()];
    let result = align(
        &logp,
        100,
        &graph,
        &mut RefinementControl::new(&mut control()),
    )
    .unwrap();
    assert_eq!(result.choices, vec![0; 30]);
    assert_eq!(result.phones.len(), 30);
    super::guard::inspect(
        &logp,
        100,
        &graph,
        &borrowed,
        &mut RefinementControl::new(&mut control()),
    )
    .unwrap();
}

#[test]
fn normalization_and_dictionary_order_are_explicit() {
    assert_eq!(
        normalize_words("I’m co-op Straße İ Σ 𐐀").unwrap(),
        ["i'm", "coop", "strasse", "i", "σ", "𐐨"]
    );
    assert!(normalize_words("123").is_err());
    assert!(normalize_words("").is_err());
    let phones = LABELS[1..40].join("\n");
    let lexicon = PronunciationLexicon::parse(
        ";;; comment\nTEST AA1\nTEST(2) AA0\nTEST(3) AE2\nbad ZZ\ninline AA #comment text\n",
        &phones,
    )
    .unwrap();
    let words = normalize_words("test").unwrap();
    let lookup = lexicon.lookup(&words).unwrap();
    assert_eq!(lookup[0].len(), 2);
    assert_eq!(lookup[0][0].dictionary_line, 2);
    assert_eq!(lookup[0][1].dictionary_spelling, "TEST(3)");
    assert!(lexicon.lookup(&["inline".to_owned()]).is_err());
    assert!(PronunciationLexicon::parse("test AA", "AA").is_err());
}

#[test]
fn input_rejection_and_cooperative_cancellation_precede_offers() {
    let phones = LABELS[1..40].join("\n");
    let lexicon = PronunciationLexicon::parse("test AA", &phones).unwrap();
    let samples = 1600;
    let logp = vec![-1.0; 6 * LABELS.len()];
    assert!(matches!(
        propose(
            &logp,
            samples,
            "test",
            &lexicon,
            &mut RefinementControl::new(&mut || true)
        ),
        Err(RefinementError::Cancelled)
    ));
    assert!(matches!(
        propose(
            &logp,
            samples,
            "test",
            &lexicon,
            &mut RefinementControl::with_budget(&mut control(), 1)
        ),
        Err(RefinementError::Limit("work visits"))
    ));
    assert!(
        propose(
            &logp[..10],
            samples,
            "test",
            &lexicon,
            &mut RefinementControl::new(&mut control())
        )
        .is_err()
    );
    assert!(
        propose(
            &logp,
            400,
            "test",
            &lexicon,
            &mut RefinementControl::new(&mut control())
        )
        .is_err()
    );
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.1] {
        let mut invalid = logp.clone();
        invalid[0] = bad;
        assert!(
            propose(
                &invalid,
                samples,
                "test",
                &lexicon,
                &mut RefinementControl::new(&mut control())
            )
            .is_err()
        );
    }
}

fn close(actual: f64, expected: &Value) {
    assert!(
        (actual - expected.as_f64().unwrap()).abs() < 1e-11,
        "{actual} != {expected}"
    );
}

fn labels(pronunciations: &[Pronunciation]) -> Vec<Vec<&'static str>> {
    pronunciations
        .iter()
        .map(|p| p.labels.iter().map(|id| LABELS[usize::from(*id)]).collect())
        .collect()
}

#[test]
fn all_twenty_four_observed_backends_retain_paths_intervals_support_and_guard() {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/refinement/fixtures");
    let records: Value = serde_json::from_str(include_str!("fixtures/goldens.json")).unwrap();
    let lexicon = PronunciationLexicon::parse(
        include_str!("fixtures/cmudict-subset.dict"),
        include_str!("fixtures/cmudict.phones"),
    )
    .unwrap();
    assert_eq!(records.as_array().unwrap().len(), 24);
    for record in records.as_array().unwrap() {
        let name = record["case"].as_str().unwrap();
        let bytes = std::fs::read(fixture.join(format!("{name}.f64le"))).unwrap();
        let logp: Vec<_> = bytes
            .chunks_exact(8)
            .map(|b| f64::from_le_bytes(b.try_into().unwrap()))
            .collect();
        let proposal = propose(
            &logp,
            record["input_samples"].as_u64().unwrap() as u32,
            record["transcript"].as_str().unwrap(),
            &lexicon,
            &mut RefinementControl::new(&mut control()),
        )
        .unwrap();
        let raw = &record["raw"];
        let guard = &record["guard"];
        assert_eq!(
            serde_json::to_value(&proposal.frame_label_path).unwrap(),
            raw["frame_label_path"],
            "{name}"
        );
        close(proposal.path_score, &raw["path_score"]);
        close(proposal.path_score_per_frame, &raw["path_score_per_frame"]);
        close(proposal.frame_blank_fraction, &raw["frame_blank_fraction"]);
        assert_eq!(proposal.words.len(), raw["words"].as_array().unwrap().len());
        assert_eq!(
            serde_json::to_value(labels(
                &proposal
                    .words
                    .iter()
                    .map(|w| w.pronunciation.clone())
                    .collect::<Vec<_>>()
            ))
            .unwrap(),
            raw["selected_pronunciations"]
        );
        for (i, word) in proposal.words.iter().enumerate() {
            let expected = &raw["words"][i];
            assert_eq!(word.word, expected["word"].as_str().unwrap());
            assert_eq!(
                word.phones.len(),
                expected["phones"].as_array().unwrap().len()
            );
            assert_eq!(
                word.interval.start.0,
                expected["interval_samples"][0].as_u64().unwrap() as u32,
                "{name}"
            );
            assert_eq!(
                word.interval.end.0,
                expected["interval_samples"][1].as_u64().unwrap() as u32,
                "{name}"
            );
            close(
                word.mean_phone_frame_posterior,
                &expected["mean_phone_frame_posterior"],
            );
            close(
                word.minimum_phone_mean_posterior,
                &expected["minimum_phone_mean_posterior"],
            );
            assert_eq!(
                matches!(word.availability, ProposalAvailability::Approximate),
                record["filtered"][i]["status"] == "proposed",
                "{name}"
            );
            for (p, expected) in word
                .phones
                .iter()
                .zip(expected["phones"].as_array().unwrap())
            {
                assert_eq!(
                    p.start_frame,
                    expected["start_frame"].as_u64().unwrap() as u32
                );
                assert_eq!(p.end_frame, expected["end_frame"].as_u64().unwrap() as u32);
                close(p.mean_posterior, &expected["mean_posterior"]);
                close(p.minimum_posterior, &expected["minimum_posterior"]);
            }
        }
        let g = &proposal.context_guard;
        assert_eq!(
            g.greedy_phones.len(),
            guard["greedy_phones"].as_array().unwrap().len()
        );
        for (actual, expected) in g
            .greedy_phones
            .iter()
            .zip(guard["greedy_phones"].as_array().unwrap())
        {
            assert_eq!(actual.label, expected["label"].as_u64().unwrap() as u8);
            assert_eq!(
                actual.start_frame,
                expected["start_frame"].as_u64().unwrap() as u32
            );
            assert_eq!(
                actual.end_frame,
                expected["end_frame"].as_u64().unwrap() as u32
            );
            close(actual.mean_posterior, &expected["mean_posterior"]);
        }
        assert_eq!(g.abstain, guard["abstain"].as_bool().unwrap(), "{name}");
        assert_eq!(
            g.minimum_edit_distance,
            guard["minimum_edit_distance"].as_u64().unwrap() as u32,
            "{name}"
        );
        assert_eq!(
            serde_json::to_value(labels(&g.selected_pronunciations)).unwrap(),
            guard["selected_pronunciations"],
            "{name}"
        );
        assert_eq!(
            g.residual_runs.len(),
            guard["residual_runs"].as_array().unwrap().len(),
            "{name}"
        );
        for (actual, expected) in g
            .residual_runs
            .iter()
            .zip(guard["residual_runs"].as_array().unwrap())
        {
            assert_eq!(
                serde_json::to_value(&actual.observed_indices).unwrap(),
                expected["observed_indices"],
                "{name}"
            );
            assert_eq!(
                actual.supported_frames,
                expected["supported_frames"].as_u64().unwrap() as u32
            );
            assert_eq!(actual.qualifies, expected["qualifies"].as_bool().unwrap());
            close(
                actual.minimum_mean_posterior,
                &expected["minimum_mean_posterior"],
            );
        }
        assert_eq!(
            g.alignment_steps.len(),
            guard["alignment_steps"].as_array().unwrap().len()
        );
        for (actual, expected) in g
            .alignment_steps
            .iter()
            .zip(guard["alignment_steps"].as_array().unwrap())
        {
            assert_eq!(
                actual.expected_index,
                expected["expected_index"].as_u64().map(|i| i as usize),
                "{name}"
            );
            assert_eq!(
                actual.observed_index,
                expected["observed_index"].as_u64().map(|i| i as usize),
                "{name}"
            );
            let operation = match actual.operation {
                EditOperation::Match => "match",
                EditOperation::Substitute => "substitute",
                EditOperation::DeleteExpected => "delete_expected",
                EditOperation::InsertObserved => "insert_observed",
            };
            assert_eq!(operation, expected["op"].as_str().unwrap(), "{name}");
        }
    }
}

/// Compare serialized structural fields exactly while accounting for inference
/// arithmetic separately. Float support is diagnostic, never accuracy evidence.
fn compare_native_result(
    actual: &Value,
    frozen: &Value,
    path: &str,
    frames: usize,
    emission_error: f64,
    maximum_support_error: &mut f64,
) {
    match (actual, frozen) {
        (Value::Number(a), Value::Number(b)) if a.is_f64() && b.is_f64() => {
            let error = (a.as_f64().unwrap() - b.as_f64().unwrap()).abs();
            if path.ends_with("/path_score") {
                assert!(
                    error <= emission_error * frames as f64 + 1e-9,
                    "{path}: aggregate score error {error}"
                );
            } else if path.ends_with("/path_score_per_frame") {
                assert!(
                    error <= emission_error + 1e-11,
                    "{path}: per-frame score error {error}"
                );
            } else {
                *maximum_support_error = maximum_support_error.max(error);
                assert!(
                    error <= emission_error + 1e-11,
                    "{path}: support error {error} exceeds log-probability difference"
                );
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (index, (a, b)) in a.iter().zip(b).enumerate() {
                compare_native_result(
                    a,
                    b,
                    &format!("{path}/{index}"),
                    frames,
                    emission_error,
                    maximum_support_error,
                );
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (key, a) in a {
                let b = b
                    .get(key)
                    .unwrap_or_else(|| panic!("{path}: missing {key}"));
                compare_native_result(
                    a,
                    b,
                    &format!("{path}/{key}"),
                    frames,
                    emission_error,
                    maximum_support_error,
                );
            }
        }
        _ => assert_eq!(actual, frozen, "{path}"),
    }
}

#[test]
fn native_emissions_preserve_all_observed_proposal_and_guard_structure() {
    use sha2::{Digest, Sha256};

    // This is a completed observation input. The native crate's separately
    // named export directory may still be written by another nextest process.
    let Some(directory) = std::env::var_os("DEADPAN_ALIGN_OBSERVED_OUTPUT") else {
        eprintln!(
            "UNEXERCISED native emission downstream parity: set DEADPAN_ALIGN_OBSERVED_OUTPUT to the completed native 24-case output directory"
        );
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/refinement/fixtures");
    let records: Value = serde_json::from_str(include_str!("fixtures/goldens.json")).unwrap();
    let lexicon = PronunciationLexicon::parse(
        include_str!("fixtures/cmudict-subset.dict"),
        include_str!("fixtures/cmudict.phones"),
    )
    .unwrap();
    assert_eq!(records.as_array().unwrap().len(), 24);
    let mut observations = Vec::new();
    for record in records.as_array().unwrap() {
        let name = record["case"].as_str().unwrap();
        let path = directory.join(format!("{name}.emissions.f64le"));
        let bytes = std::fs::read(&path).unwrap();
        let frozen_bytes = std::fs::read(fixture.join(format!("{name}.f64le"))).unwrap();
        assert_eq!(
            bytes.len(),
            frozen_bytes.len(),
            "{name} emission byte extent"
        );
        assert!(bytes.len().is_multiple_of(8));
        let decode = |bytes: &[u8]| -> Vec<f64> {
            bytes
                .chunks_exact(8)
                .map(|b| f64::from_le_bytes(b.try_into().unwrap()))
                .collect()
        };
        let native = decode(&bytes);
        let frozen = decode(&frozen_bytes);
        assert!(native.iter().all(|x| x.is_finite()), "{name}");
        let maximum_emission_error = native
            .iter()
            .zip(&frozen)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f64, f64::max);
        // Qualification's implementation comparison bound, not a timing bound.
        assert!(
            maximum_emission_error < 0.001,
            "{name}: {maximum_emission_error}"
        );
        let run = |logp: &[f64]| {
            propose(
                logp,
                record["input_samples"].as_u64().unwrap() as u32,
                record["transcript"].as_str().unwrap(),
                &lexicon,
                &mut RefinementControl::new(&mut control()),
            )
            .unwrap()
        };
        let actual = run(&native);
        let baseline = run(&frozen);
        assert_eq!(
            serde_json::to_value(&baseline.frame_label_path).unwrap(),
            record["raw"]["frame_label_path"],
            "{name}: baseline must still match frozen structural expectation"
        );
        let mut maximum_support_error = 0.0;
        compare_native_result(
            &serde_json::to_value(&actual).unwrap(),
            &serde_json::to_value(&baseline).unwrap(),
            name,
            actual.frame_label_path.len(),
            maximum_emission_error,
            &mut maximum_support_error,
        );
        observations.push(serde_json::json!({
            "case": name,
            "consumed_native_emissions": path,
            "bytes": bytes.len(),
            "sha256": Sha256::digest(&bytes).iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
            "maximum_log_probability_absolute_error": maximum_emission_error,
            "maximum_support_absolute_error": maximum_support_error,
            "path_score_per_frame_absolute_error":
                (actual.path_score_per_frame - baseline.path_score_per_frame).abs(),
            "label_pronunciation_span_availability_and_guard_structure_equal": true,
        }));
    }
    eprintln!(
        "NATIVE_DOWNSTREAM_PARITY {}",
        serde_json::to_string(&serde_json::json!({
            "cases": observations,
            "comparison_only_not_timing_or_correctness_evidence": true,
            "log_probability_absolute_error_limit_exclusive": 0.001,
            "support_error_bound": "consumed maximum log-probability absolute error + 1e-11",
        }))
        .unwrap()
    );
}
