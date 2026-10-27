use super::*;

#[test]
fn post_treatment_coincidence_keeps_displaced_round_even_labels_and_hold_mask() {
    // A 1.5-sample frame moves the authored label2 to label4, while the
    // coincident current Hold end owns label3 on the new physical grid.
    let rate = FrameRate::new(32_000, 1).unwrap();
    let plan = RenderPlan::compile(&document(
        &[(1, false), (1, true), (2, false)],
        rate,
        3,
        Some(vec![insert(0, 1, rate)]),
        Some(ExactRatio::integer(1)..ExactRatio::integer(3)),
    ))
    .unwrap();
    let sound = plan.root_sound(&id()).unwrap();
    let post = sound
        .post_treatment_gates(AudioSample(4)..AudioSample(5), Default::default())
        .unwrap();
    assert_eq!(post.spans.len(), 1);
    assert_eq!(post.spans[0].samples, AudioSample(4)..AudioSample(5));
    assert_eq!(post.spans[0].length, 3);
    assert_eq!(post.spans[0].progress_at_start, 1);
    assert_eq!(post.spans[0].start_edge, AudioEdgePolicy::Hard);
    assert_eq!(post.spans[0].end_edge, AudioEdgePolicy::Hard);
    let masked = sound
        .post_treatment_gates(AudioSample(2)..AudioSample(4), Default::default())
        .unwrap();
    assert!(masked.spans.iter().all(|span| span.length == 0));
}

#[test]
fn routed_post_treatment_gates_remove_input_edges_but_keep_edit_and_hold_cuts() {
    for rate in [
        FrameRate::new(48_000, 1).unwrap(),
        FrameRate::new(32_000, 1).unwrap(),
    ] {
        for remove in [false, true] {
            for held in [false, true] {
                let output = if remove { 899 } else { 1101 };
                let edit = if remove {
                    delete(401, 502, rate)
                } else {
                    insert(401, 101, rate)
                };
                let parts = if held {
                    vec![(300, false), (100, true), (output - 400, false)]
                } else {
                    vec![(output, false)]
                };
                let plan =
                    RenderPlan::compile(&document(&parts, rate, 1000, Some(vec![edit]), None))
                        .unwrap();
                let sound = plan.root_sound(&id()).unwrap();
                let samples = AudioSample(0)..plan.audio_duration().unwrap();
                let authored = sound
                    .gate_fades(samples.clone(), Default::default())
                    .unwrap();
                let post = sound
                    .post_treatment_gates(samples.clone(), Default::default())
                    .unwrap();
                let input = authored
                    .spans
                    .iter()
                    .filter(|span| span.length > 0)
                    .collect::<Vec<_>>();
                let output = post
                    .spans
                    .iter()
                    .filter(|span| span.length > 0)
                    .collect::<Vec<_>>();
                assert!(input.len() >= 2);
                assert_eq!(input.len(), output.len());
                assert_eq!(authored.spans.len(), post.spans.len());
                assert_eq!(input[0].start_edge, AudioEdgePolicy::Automatic);
                assert_eq!(input.last().unwrap().end_edge, AudioEdgePolicy::Automatic);
                assert_eq!(output[0].start_edge, AudioEdgePolicy::Hard);
                assert_eq!(output.last().unwrap().end_edge, AudioEdgePolicy::Hard);
                for pair in output.windows(2) {
                    assert_eq!(pair[0].end_edge, AudioEdgePolicy::Automatic);
                    assert_eq!(pair[1].start_edge, AudioEdgePolicy::Automatic);
                }
                // Edge policy changes cannot move support, retained labels or
                // envelope progress, including odd half-sample translations.
                for (a, b) in authored.spans.iter().zip(&post.spans) {
                    assert_eq!(a.samples, b.samples);
                    assert_eq!(a.length, b.length);
                    assert_eq!(a.progress_at_start, b.progress_at_start);
                }
                assert!(
                    sound
                        .post_treatment_gates(
                            samples.clone(),
                            AudioQueryLimits {
                                maximum_work: post.work,
                                ..Default::default()
                            },
                        )
                        .is_ok()
                );
                assert!(
                    sound
                        .post_treatment_gates(
                            samples,
                            AudioQueryLimits {
                                maximum_work: post.work - 1,
                                ..Default::default()
                            },
                        )
                        .is_err()
                );
            }
        }
    }
}

#[test]
fn post_treatment_coincidence_deduplicates_input_edges_and_keeps_hard_choices() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    for routed in [false, true] {
        for authored in [AudioEdgePolicy::Automatic, AudioEdgePolicy::Hard] {
            for current in [AudioEdgePolicy::Automatic, AudioEdgePolicy::Hard] {
                let lead = if routed { 150 } else { 100 };
                let edits = routed.then(|| {
                    let mut edit = insert(100, 50, rate);
                    edit.cuts = RootSoundCutEdges {
                        before: current,
                        after: current,
                    };
                    vec![edit]
                });
                let doc = document(
                    &[(lead, true), (900, false)],
                    rate,
                    1000,
                    edits,
                    Some(ExactRatio::integer(100)..ExactRatio::integer(900)),
                );
                let mut wire = serde_json::to_value(doc).unwrap();
                wire["sounds"]["sound"]["start_edge"] = json!(authored);
                wire["nodes"]["part0"]["audio_edges"] = json!(AudioEdgePolicies {
                    node_end: current,
                    ..Default::default()
                });
                let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
                let plan = RenderPlan::compile(&doc).unwrap();
                let sound = plan.root_sound(&id()).unwrap();
                let post = sound
                    .post_treatment_gates(
                        AudioSample(0)..plan.audio_duration().unwrap(),
                        Default::default(),
                    )
                    .unwrap();
                let first = post.spans.iter().find(|span| span.length > 0).unwrap();
                assert_eq!(first.samples.start, AudioSample(lead));
                let expected =
                    if authored == AudioEdgePolicy::Hard || current == AudioEdgePolicy::Hard {
                        AudioEdgePolicy::Hard
                    } else {
                        AudioEdgePolicy::Automatic
                    };
                let complete = sound
                    .gate_fades(
                        AudioSample(0)..plan.audio_duration().unwrap(),
                        Default::default(),
                    )
                    .unwrap();
                let first_complete = complete.spans.iter().find(|span| span.length > 0).unwrap();
                assert_eq!(first_complete.start_edge, expected);
                // The moved authored onset, route cut and Hold end coincide.
                // The input edge already fed the event processor. A second
                // post fade at that endpoint would attenuate it twice.
                assert_eq!(first.start_edge, AudioEdgePolicy::Hard);
            }
        }
    }
}
