use super::*;

fn trim(
    start: i64,
    end: i64,
    i: i64,
    o: i64,
    rate: FrameRate,
    cuts: RootSoundCutEdges,
) -> RootSoundEdit {
    RootSoundEdit {
        grid: RootSoundGrid::root(rate),
        operation: RootSoundOperation::Trim {
            range: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            in_frames: i,
            out_frames: o,
        },
        cuts,
    }
}

fn labels(plan: &RenderPlan, samples: Range<i64>) -> Vec<Option<ExactRatio>> {
    let sound = plan.root_sound(&id()).unwrap();
    let route = sound.routed_input().unwrap().route();
    let step = route.recipe_grid().frames_per_sample();
    route
        .query(
            AudioSample(samples.start)..AudioSample(samples.end),
            AudioQueryLimits::default(),
        )
        .unwrap()
        .spans
        .into_iter()
        .flat_map(|span| {
            (span.samples.start.0..span.samples.end.0).map(move |n| {
                span.sampling.map(|mapping| {
                    mapping
                        .local_at(AudioSample(n))
                        .unwrap()
                        .checked_div(step)
                        .unwrap()
                })
            })
        })
        .collect()
}

#[test]
fn in_only_merges_translated_body_and_suffix_without_new_phase_or_fade() {
    let rate = FrameRate::new(30000, 1001).unwrap();
    for (before, after) in [
        (AudioEdgePolicy::Automatic, AudioEdgePolicy::Hard),
        (AudioEdgePolicy::Hard, AudioEdgePolicy::Automatic),
    ] {
        let cuts = RootSoundCutEdges { before, after };
        let combined = RenderPlan::compile(&document(
            &[(5, false)],
            rate,
            6,
            Some(vec![trim(1, 3, 1, 0, rate, cuts)]),
            None,
        ))
        .unwrap();
        let mut scalar = delete(1, 2, rate);
        scalar.cuts = cuts;
        let scalar =
            RenderPlan::compile(&document(&[(5, false)], rate, 6, Some(vec![scalar]), None))
                .unwrap();
        assert_eq!(labels(&combined, 0..8008), labels(&scalar, 0..8008));
        assert_eq!(
            factors(&gates(&combined, 0..8008)),
            factors(&gates(&scalar, 0..8008))
        );
        assert_eq!(
            labels(&combined, 3203..3204),
            vec![Some(ExactRatio::integer(4804))]
        );
        assert_ne!(
            labels(&combined, 3203..3204),
            vec![Some(ExactRatio::integer(4805))]
        );
        assert_eq!(
            factors(&gates(&combined, 3202..3205)),
            vec![ExactRatio::ONE; 3]
        );
    }
}

#[test]
fn normalized_keep_retains_independent_offset_on_both_source_sample_rates() {
    let rate = FrameRate::new(30000, 1001).unwrap();
    for (source_rate, expected) in [
        (48000, ExactRatio::integer(4797)),
        (44100, ExactRatio::new(705159, 160).unwrap()),
    ] {
        let mut wire = json!(document(
            &[(5, false)],
            rate,
            6,
            Some(vec![trim(1, 3, 1, 0, rate, Default::default())]),
            None
        ));
        let time_base = SourceTimeBase::new(1, source_rate).unwrap();
        let span = SourceSpan::new(
            SourceTimestamp {
                ticks: 0,
                time_base,
            },
            SourceTimestamp {
                ticks: 1000000,
                time_base,
            },
        )
        .unwrap();
        wire["assets"]["sound"]["audio"] = json!(span);
        wire["sounds"]["sound"]["source"]["span"] = json!(span);
        wire["sounds"]["sound"]["offset"] = json!(7);
        wire["sounds"]["sound"]["mapping"]["frames"] = json!(
            SourceAudioMapping::natural_rate(span, rate)
                .unwrap()
                .duration_frames(FrameDuration::ZERO)
                .unwrap()
        );
        wire["sounds"]["sound"]["mapping"]["selection"]["end"] = json!(
            ExactRatio::integer(6)
                .checked_sub(ExactRatio::new(35, 8008).unwrap())
                .unwrap()
        );
        let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let plan = RenderPlan::compile(&document).unwrap();
        assert_eq!(
            labels(&plan, 3203..3204),
            vec![Some(ExactRatio::integer(4804))]
        );
        let sound = plan.root_sound(&id()).unwrap();
        let AudioRoutedRootInput::Source(raw) = sound.routed_input().unwrap().input() else {
            panic!()
        };
        assert_eq!(
            raw.query(
                AudioSample(4804)..AudioSample(4805),
                AudioQueryLimits::default()
            )
            .unwrap()
            .spans[0]
                .source_point(AudioSample(4804))
                .unwrap()
                .ticks,
            expected
        );
        assert_eq!(factors(&gates(&plan, 3203..3204)), vec![ExactRatio::ONE]);
    }
}

#[test]
fn direct_equal_in_out_retains_final_sample_that_sequential_edits_drop() {
    let rate = FrameRate::new(30000, 1001).unwrap();
    let direct = RenderPlan::compile(&document(
        &[(6, false)],
        rate,
        6,
        Some(vec![trim(1, 3, 1, 1, rate, Default::default())]),
        None,
    ))
    .unwrap();
    assert_eq!(
        labels(&direct, 9609..9610),
        vec![Some(ExactRatio::integer(9609))]
    );
    assert!(
        direct
            .root_sound(&id())
            .unwrap()
            .selects_sample(AudioSample(9609))
            .unwrap()
    );
    assert!(
        factors(&gates(&direct, 9609..9610))[0]
            .compare_integer(0)
            .is_gt()
    );
    assert!(labels(&direct, 3203..4805).iter().all(Option::is_none));
    let sequential = RenderPlan::compile(&document(
        &[(6, false)],
        rate,
        6,
        Some(vec![delete(1, 2, rate), insert(2, 1, rate)]),
        None,
    ))
    .unwrap();
    // Delete loses old9609 at the temporary endpoint. Insert then translates
    // surviving old9608 into new9609; that is a wrong sample, not silence.
    assert_eq!(
        labels(&sequential, 9609..9610),
        vec![Some(ExactRatio::integer(9608))]
    );
}

#[test]
fn previous_route_then_one_trim_retains_literal_labels_and_shuffled_chunks() {
    let rate = FrameRate::new(30000, 1001).unwrap();
    let plan = RenderPlan::compile(&document(
        &[(7, false)],
        rate,
        6,
        Some(vec![
            insert(1, 1, rate),
            trim(2, 4, 1, 1, rate, Default::default()),
        ]),
        None,
    ))
    .unwrap();
    for (sample, old) in [
        (0, Some(0)),
        (1601, Some(1601)),
        (1602, None),
        (3203, Some(3204)),
        (4803, Some(4804)),
        (4804, None),
        (4805, None),
        (6406, Some(4805)),
        (11210, Some(9609)),
    ] {
        assert_eq!(
            labels(&plan, sample..sample + 1),
            vec![old.map(ExactRatio::integer)],
            "sample {sample}"
        );
    }
    let all = labels(&plan, 0..11211);
    let all_fades = factors(&gates(&plan, 0..11211));
    for interval in [6406..11211, 1602..4805, 0..1602, 4805..6406, 3203..6407] {
        let start = usize::try_from(interval.start).unwrap();
        let end = usize::try_from(interval.end).unwrap();
        assert_eq!(labels(&plan, interval.clone()), all[start..end]);
        assert_eq!(factors(&gates(&plan, interval)), all_fades[start..end]);
    }
}

#[test]
fn mixed_extension_keeps_both_cut_policies_without_marking_query_boundaries() {
    let rate = FrameRate::new(30000, 1001).unwrap();
    for hard_before in [false, true] {
        let cuts = RootSoundCutEdges {
            before: if hard_before {
                AudioEdgePolicy::Hard
            } else {
                AudioEdgePolicy::Automatic
            },
            after: if hard_before {
                AudioEdgePolicy::Automatic
            } else {
                AudioEdgePolicy::Hard
            },
        };
        let plan = RenderPlan::compile(&document(
            &[(8, false)],
            rate,
            6,
            Some(vec![trim(1, 3, -1, 1, rate, cuts)]),
            None,
        ))
        .unwrap();
        // First gap[1,2), second gap[4,5); mapped starts/ends get their own side.
        let all = factors(&gates(&plan, 0..12813));
        assert_eq!(all[1601] == ExactRatio::ONE, hard_before);
        assert_eq!(all[3203] == ExactRatio::ONE, !hard_before);
        assert_eq!(all[6405] == ExactRatio::ONE, hard_before);
        assert_eq!(all[8008] == ExactRatio::ONE, !hard_before);
        assert!(all[1602..3203].iter().all(|v| *v == ExactRatio::ZERO));
        assert!(all[6406..8008].iter().all(|v| *v == ExactRatio::ZERO));
        assert_eq!(
            factors(&gates(&plan, 3500..4000)),
            vec![ExactRatio::ONE; 500]
        );
    }
}

#[test]
fn all_scalar_trim_edges_preserve_existing_insert_delete_cut_policy_queries() {
    let rate = FrameRate::new(30000, 1001).unwrap();
    for (i, o, scalar) in [
        (1, 0, delete(1, 2, rate)),
        (-1, 0, insert(1, 1, rate)),
        (0, 1, insert(4, 1, rate)),
        (0, -1, delete(3, 4, rate)),
    ] {
        for cuts in [
            RootSoundCutEdges {
                before: AudioEdgePolicy::Hard,
                after: AudioEdgePolicy::Automatic,
            },
            RootSoundCutEdges {
                before: AudioEdgePolicy::Automatic,
                after: AudioEdgePolicy::Hard,
            },
        ] {
            let extent = 6 + o - i;
            let combined = RenderPlan::compile(&document(
                &[(extent, false)],
                rate,
                6,
                Some(vec![trim(1, 4, i, o, rate, cuts)]),
                None,
            ))
            .unwrap();
            let scalar = RootSoundEdit { cuts, ..scalar };
            let reference = RenderPlan::compile(&document(
                &[(extent, false)],
                rate,
                6,
                Some(vec![scalar]),
                None,
            ))
            .unwrap();
            let end = rate.audio_boundary(ProjectFrame(extent)).unwrap().0;
            assert_eq!(labels(&combined, 0..end), labels(&reference, 0..end));
            assert_eq!(
                factors(&gates(&combined, 0..end)),
                factors(&gates(&reference, 0..end))
            );
        }
    }
}
