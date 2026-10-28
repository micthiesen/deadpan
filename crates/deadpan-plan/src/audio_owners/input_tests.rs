use super::*;
use crate::AudioOwnerInputScope;

fn owner<'a>(plan: &'a RenderPlan, name: &str, at: i64) -> AudioOwnerClock<'a> {
    plan.audio_gain_owners(AudioSample(at)..AudioSample(at + 1), Default::default())
        .unwrap()
        .spans()[0]
        .owners()
        .iter()
        .find(|owner| owner.instance().node == id(name))
        .unwrap()
        .clone()
}

#[test]
fn cropped_group_input_keeps_hidden_siblings_and_signed_root_grid() {
    let doc = document(
        48_000,
        &["prefix", "crop"],
        vec![
            ("prefix", hold(1)),
            ("a", hold(4)),
            ("b", hold(8)),
            ("c", hold(4)),
            ("group", sequence(&["a", "b", "c"])),
            (
                "crop",
                retime(
                    "group",
                    8,
                    4..12,
                    PitchPolicy::FollowSpeed,
                    RetimePurpose::Partition,
                ),
            ),
        ],
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let clock = owner(&plan, "group", 3);
    let input = plan
        .audio_owner_input(&clock, AudioSample(3), Default::default())
        .unwrap();
    assert_eq!(input.scope(), AudioOwnerInputScope::Owner);
    let context = input.context();
    assert_eq!(context.instance().node, id("group"));
    assert_eq!(context.root_samples(), AudioSample(-3)..AudioSample(13));
    assert_eq!(context.visible_samples(), AudioSample(1)..AudioSample(9));
    let query = context
        .owners(context.root_samples(), Default::default())
        .unwrap();
    let leaves: Vec<_> = query
        .spans()
        .iter()
        .map(|span| {
            (
                span.samples(),
                span.owners().last().unwrap().instance().node.clone(),
            )
        })
        .collect();
    assert_eq!(
        leaves,
        vec![
            (AudioSample(-3)..AudioSample(1), id("a")),
            (AudioSample(1)..AudioSample(9), id("b")),
            (AudioSample(9)..AudioSample(13), id("c")),
        ]
    );
    assert_eq!(
        query.spans()[0].owners()[0]
            .sampling()
            .local_at(AudioSample(-2))
            .unwrap(),
        ratio(1, 1)
    );
    assert!(
        context
            .audio(AudioSample(-4)..AudioSample(-3), Default::default())
            .is_err()
    );
    assert!(
        context
            .processing(AudioSample(13)..AudioSample(14), Default::default())
            .is_err()
    );
    // Reusing an observation in the same affine owner interval is allowed.
    assert!(
        plan.audio_owner_input(&clock, AudioSample(8), Default::default())
            .is_ok()
    );
    assert!(
        plan.audio_owner_input(&clock, AudioSample(0), Default::default())
            .is_err()
    );
}

#[test]
fn descendant_input_retains_the_complete_enclosing_preserve_output() {
    let doc = document(
        48_000,
        &["crop"],
        vec![
            ("a", hold(4)),
            ("b", hold(4)),
            ("group", sequence(&["a", "b"])),
            (
                "stage",
                retime(
                    "group",
                    16,
                    0..8,
                    PitchPolicy::Preserve,
                    RetimePurpose::Edit,
                ),
            ),
            (
                "crop",
                retime(
                    "stage",
                    4,
                    6..10,
                    PitchPolicy::FollowSpeed,
                    RetimePurpose::Partition,
                ),
            ),
        ],
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    for name in ["group", "a"] {
        let input = plan
            .audio_owner_input(&owner(&plan, name, 0), AudioSample(0), Default::default())
            .unwrap();
        assert_eq!(input.scope(), AudioOwnerInputScope::EnclosingProcessor);
        assert_eq!(input.context().instance().node, id("stage"));
        assert_eq!(
            input.context().root_samples(),
            AudioSample(-6)..AudioSample(10)
        );
        let processing = input
            .context()
            .processing(AudioSample(-5)..AudioSample(-4), Default::default())
            .unwrap();
        assert!(matches!(
            processing.spans[0].content,
            super::super::super::AudioSignalContent::Stage(_)
        ));
    }
}

#[test]
fn retained_binding_is_not_reconstructed_from_its_nominal_descendant_clock() {
    let nodes = || {
        vec![
            ("a", hold(2)),
            ("b", hold(2)),
            ("group", sequence(&["a", "b"])),
            (
                "stage",
                retime("group", 8, 0..4, PitchPolicy::Preserve, RetimePurpose::Edit),
            ),
        ]
    };
    let old = document(48_000, &["stage"], nodes());
    let mut moved = nodes();
    moved.push(("prefix", hold(1)));
    let current = document(48_000, &["prefix", "stage"], moved);
    let current = bind(
        &current,
        &old,
        "stage",
        AudioClockRoot::ProjectRootRoundEven,
        Some(ratio(3, 1)),
    );
    let plan = RenderPlan::compile(&current).unwrap();
    let input = plan
        .audio_owner_input(
            &owner(&plan, "group", 1),
            AudioSample(1),
            Default::default(),
        )
        .unwrap();
    assert_eq!(input.scope(), AudioOwnerInputScope::EnclosingProcessor);
    assert!(matches!(
        input.owner().origin(),
        AudioOwnerClockOrigin::Retained { .. }
    ));
    let query = input
        .context()
        .processing(AudioSample(1)..AudioSample(2), Default::default())
        .unwrap();
    assert!(matches!(
        query.spans[0].content,
        super::super::super::AudioSignalContent::Bound(_)
    ));
    assert_eq!(
        input.owner().sampling().local_at(AudioSample(1)).unwrap(),
        ratio(3, 2)
    );
    let retained = input
        .retained_at(AudioSample(1), Default::default())
        .unwrap()
        .unwrap();
    assert!(retained.belongs_to(&plan));
    assert_eq!(retained.reference_at(AudioSample(1)).unwrap(), ratio(3, 1));
    assert_eq!(
        retained.reference_at(AudioSample(-2)).unwrap(),
        ExactRatio::ZERO
    );
    assert_eq!(
        retained.reference_samples_per_output_sample(),
        ExactRatio::ONE
    );
    let crate::AudioBoundDomain::Root(reference) = retained.domain() else {
        panic!("expected root reference");
    };
    assert_eq!(reference.root_samples(), AudioSample(0)..AudioSample(8));
    assert_eq!(
        input.context().root_samples(),
        AudioSample(1)..AudioSample(9)
    );
    let whole = retained
        .owners(AudioSample(-3)..AudioSample(7), Default::default())
        .unwrap();
    assert_eq!(
        whole.spans().first().unwrap().support(),
        AudioOwnerSupport::Inactive
    );
    assert_eq!(
        whole.spans().last().unwrap().support(),
        AudioOwnerSupport::Inactive
    );
    let values = observations(&whole);
    assert_eq!(
        values[&-2]
            .iter()
            .map(|owner| owner.instance.node.clone())
            .collect::<Vec<_>>(),
        vec![id("stage"), id("group"), id("a")]
    );
    assert_eq!(values[&-2][0].local, ExactRatio::ZERO);
    assert_eq!(values[&1][0].local, ratio(3, 1));
    assert_eq!(values[&1][1].local, ratio(3, 2));
    assert_eq!(values[&2].last().unwrap().instance.node, id("b"));
    for at in -3..7 {
        let part = retained
            .owners(AudioSample(at)..AudioSample(at + 1), Default::default())
            .unwrap();
        assert_eq!(observations(&part)[&at], values[&at]);
    }
    assert!(
        retained
            .owners(
                AudioSample(-3)..AudioSample(7),
                AudioQueryLimits {
                    maximum_spans: 1,
                    maximum_work: 10_000
                }
            )
            .is_err()
    );
    assert!(
        retained
            .owners(
                AudioSample(-3)..AudioSample(7),
                AudioQueryLimits {
                    maximum_spans: 100,
                    maximum_work: whole.work() - 1
                }
            )
            .is_err()
    );
    let fades = retained
        .fades(AudioSample(-3)..AudioSample(7), Default::default())
        .unwrap();
    assert_eq!(fades.spans.first().unwrap().samples.start, AudioSample(-3));
    assert_eq!(fades.spans.last().unwrap().samples.end, AudioSample(7));
    assert!(
        retained
            .fades(
                AudioSample(-3)..AudioSample(7),
                AudioQueryLimits {
                    maximum_spans: 1,
                    maximum_work: 10_000
                }
            )
            .is_err()
    );
    assert!(
        retained
            .fades(
                AudioSample(-3)..AudioSample(7),
                AudioQueryLimits {
                    maximum_spans: 100,
                    maximum_work: fades.work - 1
                }
            )
            .is_err()
    );
    let crate::AudioBoundDomain::Root(domain) = retained.domain() else {
        panic!("expected the actual RootRoundEven reference domain");
    };
    let at = AudioSample(3);
    let candidates = domain
        .owner_input_candidates_at(at, Default::default())
        .unwrap();
    let member = selected_candidate(&candidates, "group");
    let selected = &candidates.query().spans()[0].owners()[member];
    let expected = domain
        .owner_input(selected, at, Default::default())
        .unwrap();
    assert!(
        plan.audio_owner_input(selected, at, Default::default())
            .is_err(),
        "the live retained seed must not be replaced by root lookup"
    );
    let capture = candidates.capture(member, Default::default()).unwrap();
    assert_captured_input(capture.input().unwrap(), &expected);
}

#[test]
fn repeat_play_gap_and_whole_repeat_have_distinct_contexts() {
    let doc = document(
        48_000,
        &["repeat"],
        vec![("voice", hold(4)), ("repeat", repeat("voice", 3, 2))],
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let first = owner(&plan, "voice", 0);
    let boundary = first.treatment_boundary(0).unwrap();
    assert_eq!(boundary.before_stage(), 0);
    assert!(boundary.matches(&first));
    assert!(first.treatment_boundary(1).is_err());
    assert!(
        plan.audio_owner_input(&first, AudioSample(6), Default::default())
            .is_err()
    );
    let second = owner(&plan, "voice", 6);
    assert!(!boundary.matches(&second), "different Repeat occurrence");
    let other_plan = RenderPlan::compile(&doc).unwrap();
    assert!(!boundary.matches(&owner(&other_plan, "voice", 0)));
    let input = plan
        .audio_owner_input(&second, AudioSample(6), Default::default())
        .unwrap();
    assert_eq!(
        input.context().root_samples(),
        AudioSample(6)..AudioSample(10)
    );
    assert_eq!(input.context().instance(), second.instance());
    let whole = owner(&plan, "repeat", 6);
    assert!(
        whole
            .treatment_boundary(0)
            .unwrap()
            .matches(&owner(&plan, "repeat", 0)),
        "one whole-Repeat boundary spans its changing play clock"
    );
    let input = plan
        .audio_owner_input(&whole, AudioSample(6), Default::default())
        .unwrap();
    assert_eq!(
        input.context().root_samples(),
        AudioSample(0)..AudioSample(16)
    );
    assert_eq!(
        input
            .context()
            .audio(input.context().root_samples(), Default::default())
            .unwrap()
            .spans
            .len(),
        5
    );
    let query = plan
        .audio_owners(AudioSample(10)..AudioSample(11), Default::default())
        .unwrap();
    let gap = query.spans()[0].owners().last().unwrap();
    assert!(!whole.treatment_boundary(0).unwrap().matches(gap));
    let input = plan
        .audio_owner_input(gap, AudioSample(10), Default::default())
        .unwrap();
    assert_eq!(input.scope(), AudioOwnerInputScope::Owner);
    assert_eq!(
        input.context().root_samples(),
        AudioSample(10)..AudioSample(12)
    );
    assert_eq!(input.context().gap_after(), gap.gap_after());
}

#[test]
fn foreign_owner_and_exhausted_work_reject_without_a_partial_context() {
    let doc = document(
        48_000,
        &["repeat"],
        vec![
            ("voice", hold(1)),
            ("repeat", repeat("voice", 1_000_000_000, 0)),
        ],
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let other = RenderPlan::compile(&doc).unwrap();
    let at = AudioSample(999_999_999);
    let clock = owner(&plan, "voice", at.0);
    assert!(
        other
            .audio_owner_input(&clock, at, Default::default())
            .is_err()
    );
    let input = plan
        .audio_owner_input(&clock, at, Default::default())
        .unwrap();
    assert!(input.work() < 200);
    assert!(
        plan.audio_owner_input(
            &clock,
            at,
            AudioQueryLimits {
                maximum_spans: 100,
                maximum_work: input.work() - 1
            }
        )
        .is_err()
    );
    assert_eq!(
        plan.audio_owner_input(
            &clock,
            at,
            AudioQueryLimits {
                maximum_spans: 100,
                maximum_work: input.work()
            }
        )
        .unwrap()
        .work(),
        input.work()
    );
}

#[test]
fn moved_preserve_input_keeps_its_point_ceil_reference_labels() {
    let old = document(
        32_000,
        &["stage"],
        vec![
            ("prefix", hold(1)),
            ("voice", hold(2)),
            ("group", sequence(&["prefix", "voice"])),
            (
                "stage",
                retime("group", 6, 0..3, PitchPolicy::Preserve, RetimePurpose::Edit),
            ),
        ],
    );
    let current = document(32_000, &["voice"], vec![("voice", hold(2))]);
    let bound = bind(
        &current,
        &old,
        "voice",
        AudioClockRoot::PreserveInputPointCeil { stage: id("stage") },
        None,
    );
    let plan = RenderPlan::compile(&bound).unwrap();
    let input = plan
        .audio_owner_input(
            &owner(&plan, "voice", 0),
            AudioSample(0),
            Default::default(),
        )
        .unwrap();
    let probe = input
        .retained_probe_at(AudioSample(0), Default::default())
        .unwrap();
    assert!(probe.work() > 0);
    assert_eq!(probe.work(), probe.input().unwrap().work());
    let work = probe.work();
    let exact = input
        .retained_probe_at(
            AudioSample(0),
            AudioQueryLimits {
                maximum_work: work,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(exact.work(), work);
    assert!(exact.input().is_some());
    assert!(matches!(
        input.retained_probe_at(
            AudioSample(0),
            AudioQueryLimits {
                maximum_work: work - 1,
                ..Default::default()
            },
        ),
        Err(PlanError::AudioQueryLimit(_))
    ));
    let retained = probe.into_input().unwrap();
    let crate::AudioBoundDomain::Point(domain) = retained.domain() else {
        panic!("expected PointCeil reference");
    };
    assert_eq!(
        domain.reference_samples(),
        crate::ReferenceSample(2)..crate::ReferenceSample(5)
    );
    assert_eq!(retained.reference_at(AudioSample(0)).unwrap(), ratio(2, 1));
    assert_eq!(retained.reference_at(AudioSample(-1)).unwrap(), ratio(1, 1));
    assert_eq!(
        retained.reference_samples_per_output_sample(),
        ExactRatio::ONE
    );
    let owners = retained
        .owners(AudioSample(0)..AudioSample(1), Default::default())
        .unwrap();
    let clock = &owners.spans()[0].owners()[0];
    assert_eq!(
        clock.sampling().local_at(AudioSample(0)).unwrap(),
        ratio(1, 3)
    );
    assert_eq!(clock.sampling().local_frames_per_sample(), ratio(2, 3));
    assert!(
        input
            .retained_at(AudioSample(3), Default::default())
            .is_err()
    );
}

#[test]
fn retained_probe_reports_positive_none_work_and_preserves_guards() {
    let doc = document(48_000, &["voice"], vec![("voice", hold(4))]);
    let plan = RenderPlan::compile(&doc).unwrap();
    let input = plan
        .audio_owner_input(
            &owner(&plan, "voice", 1),
            AudioSample(1),
            Default::default(),
        )
        .unwrap();
    let probe = input
        .retained_probe_at(AudioSample(1), Default::default())
        .unwrap();
    assert!(probe.input().is_none());
    let work = probe.work();
    assert!(work > 0, "an absent retained operand still required a walk");
    assert!(probe.into_input().is_none());
    let exact = AudioQueryLimits {
        maximum_work: work,
        ..Default::default()
    };
    let exact_probe = input.retained_probe_at(AudioSample(1), exact).unwrap();
    assert_eq!(exact_probe.work(), work);
    assert!(exact_probe.into_input().is_none());
    assert!(input.retained_at(AudioSample(1), exact).unwrap().is_none());
    assert!(matches!(
        input.retained_probe_at(
            AudioSample(1),
            AudioQueryLimits {
                maximum_work: work - 1,
                ..exact
            },
        ),
        Err(PlanError::AudioQueryLimit(_))
    ));
    for at in [AudioSample(-1), AudioSample(4)] {
        assert!(matches!(
            input.retained_probe_at(at, Default::default()),
            Err(PlanError::AudioRangeOutOfRange)
        ));
        assert!(matches!(
            input.retained_at(at, Default::default()),
            Err(PlanError::AudioRangeOutOfRange)
        ));
        // Limit validation remains before the existing range guard.
        assert!(matches!(
            input.retained_probe_at(
                at,
                AudioQueryLimits {
                    maximum_work: 0,
                    ..Default::default()
                },
            ),
            Err(PlanError::InvalidAudioLimits)
        ));
    }
}

fn selected_candidate(candidates: &crate::AudioOwnerInputCandidates<'_, '_>, name: &str) -> usize {
    candidates.query().spans()[0]
        .owners()
        .iter()
        .position(|clock| clock.instance().node == id(name))
        .unwrap()
}

/// Test-only full owner-query heap count. Exhaustive destructuring catches new
/// fields; actual capacities and the existing field-aware clock visitor count
/// owned storage without adding a public measurement API.
fn candidate_query_heap(query: &AudioOwnerQuery<'_>) -> usize {
    let AudioOwnerQuery {
        plan: _,
        samples: _,
        spans,
        lookup: _,
        work: _,
    } = query;
    let mut bytes = spans.capacity() * std::mem::size_of::<AudioOwnerSpan<'_>>();
    for span in spans {
        let AudioOwnerSpan {
            samples: _,
            owners,
            support: _,
            original_is_silent: _,
        } = span;
        bytes += owners.capacity() * std::mem::size_of::<AudioOwnerClock<'_>>();
        for clock in owners {
            bytes += crate::audio_allocated::measure(clock, 65_536).unwrap().0;
        }
    }
    bytes
}

fn assert_captured_input(
    actual: &crate::AudioOwnerInput<'_>,
    expected: &crate::AudioOwnerInput<'_>,
) {
    assert_eq!(actual.observed_at(), expected.observed_at());
    assert_eq!(actual.scope(), expected.scope());
    assert!(
        actual
            .owner()
            .same_at(expected.owner(), actual.observed_at())
    );
    assert_eq!(actual.work(), expected.work());
    assert_eq!(
        actual.context().root_samples(),
        expected.context().root_samples()
    );
    assert_eq!(
        actual.context().visible_samples(),
        expected.context().visible_samples()
    );
    assert_eq!(actual.context().instance(), expected.context().instance());
    // Compare every private captured seed value without manufacturing one.
    // Debug is an oracle representation in this test, never runtime identity.
    assert_eq!(
        format!("{:?}", actual.context().seed),
        format!("{:?}", expected.context().seed),
    );
    assert_eq!(
        crate::audio_allocated::measure(actual.context(), 65_536)
            .unwrap()
            .0,
        crate::audio_allocated::measure(expected.context(), 65_536)
            .unwrap()
            .0,
    );
}

#[test]
fn root_candidates_capture_the_actual_member_once_with_equal_query_heap() {
    let doc = document(48_000, &["voice"], vec![("voice", hold(4))]);
    let plan = RenderPlan::compile(&doc).unwrap();
    let query = plan
        .audio_gain_owners(AudioSample(1)..AudioSample(2), Default::default())
        .unwrap();
    let expected = plan
        .audio_owner_input(
            &owner(&plan, "voice", 1),
            AudioSample(1),
            Default::default(),
        )
        .unwrap();
    let candidates = plan
        .audio_owner_input_candidates_at(AudioSample(1), Default::default())
        .unwrap();
    assert_eq!(candidates.query().spans.capacity(), query.spans.capacity());
    assert_eq!(
        candidate_query_heap(candidates.query()),
        candidate_query_heap(&query)
    );
    assert_eq!(candidates.work(), query.work());
    let member = selected_candidate(&candidates, "voice");
    let capture = candidates.capture(member, Default::default()).unwrap();
    assert_eq!(capture.query_work(), query.work());
    assert!(capture.capture_work() > 0);
    assert_eq!(
        capture.work(),
        capture.query_work() + capture.capture_work()
    );
    assert_captured_input(capture.input().unwrap(), &expected);
    assert!(capture.into_input().is_some());
}

#[test]
fn candidates_report_absence_and_preserve_combined_and_capture_work_caps() {
    let doc = document(48_000, &["voice"], vec![("voice", hold(4))]);
    let plan = RenderPlan::compile(&doc).unwrap();
    let at = AudioSample(1);
    let candidates = plan
        .audio_owner_input_candidates_at(at, Default::default())
        .unwrap();
    let q = candidates.work();
    let member = selected_candidate(&candidates, "voice");
    let measured = candidates.capture(member, Default::default()).unwrap();
    let c = measured.capture_work();
    assert!(q > 0 && c > 0);
    for shortage in [0, 1] {
        let limit = q + c - shortage;
        let candidates = plan
            .audio_owner_input_candidates_at(
                at,
                AudioQueryLimits {
                    maximum_work: limit,
                    ..Default::default()
                },
            )
            .unwrap();
        let result = candidates.capture(member, Default::default());
        if shortage == 0 {
            assert_eq!(result.unwrap().work(), q + c);
        } else {
            assert!(matches!(
                result,
                Err(PlanError::AudioQueryLimit("structural work"))
            ));
        }
    }
    let candidates = plan
        .audio_owner_input_candidates_at(
            at,
            AudioQueryLimits {
                maximum_work: q,
                ..Default::default()
            },
        )
        .unwrap();
    let absent = candidates.without_selection();
    assert!(absent.input().is_none());
    assert_eq!(absent.query_work(), q);
    assert_eq!(absent.capture_work(), 0);
    assert_eq!(absent.work(), q);
    let candidates = plan
        .audio_owner_input_candidates_at(
            at,
            AudioQueryLimits {
                maximum_work: q,
                ..Default::default()
            },
        )
        .unwrap();
    // Internally exhausted capture is an existing walk error, not invalid
    // caller limits. No AudioQueryLimits(maximum_work=0) is constructed.
    assert!(matches!(
        candidates.capture(member, Default::default()),
        Err(PlanError::AudioQueryLimit("structural work"))
    ));
    let candidates = plan
        .audio_owner_input_candidates_at(at, Default::default())
        .unwrap();
    assert_eq!(
        candidates
            .capture(
                member,
                AudioQueryLimits {
                    maximum_work: c,
                    ..Default::default()
                },
            )
            .unwrap()
            .capture_work(),
        c
    );
}

#[test]
fn domain_candidates_keep_hidden_negative_support_and_actual_seed() {
    let doc = document(
        48_000,
        &["prefix", "crop"],
        vec![
            ("prefix", hold(1)),
            ("a", hold(4)),
            ("b", hold(8)),
            ("c", hold(4)),
            ("group", sequence(&["a", "b", "c"])),
            (
                "crop",
                retime(
                    "group",
                    8,
                    4..12,
                    PitchPolicy::FollowSpeed,
                    RetimePurpose::Partition,
                ),
            ),
        ],
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let input = plan
        .audio_owner_input(
            &owner(&plan, "group", 3),
            AudioSample(3),
            Default::default(),
        )
        .unwrap();
    let domain = input.context();
    assert_eq!(domain.root_samples(), AudioSample(-3)..AudioSample(13));
    let at = AudioSample(-2);
    let candidates = domain
        .owner_input_candidates_at(at, Default::default())
        .unwrap();
    let query = domain
        .gain_owners(at..AudioSample(-1), Default::default())
        .unwrap();
    assert_eq!(
        candidate_query_heap(candidates.query()),
        candidate_query_heap(&query)
    );
    let member = selected_candidate(&candidates, "group");
    let actual_clock = &candidates.query().spans()[0].owners()[member];
    let expected = domain
        .owner_input(actual_clock, at, Default::default())
        .unwrap();
    let capture = candidates.capture(member, Default::default()).unwrap();
    assert_captured_input(capture.input().unwrap(), &expected);
    assert!(
        plan.audio_owner_input_candidates_at(at, Default::default())
            .is_err()
    );
}

#[test]
fn candidate_capture_keeps_whole_repeat_observation_and_foreign_plan_separate() {
    let doc = document(
        48_000,
        &["repeat"],
        vec![("voice", hold(4)), ("repeat", repeat("voice", 3, 2))],
    );
    let plan = RenderPlan::compile(&doc).unwrap();
    let other = RenderPlan::compile(&doc).unwrap();
    let at = AudioSample(6);
    let candidates = plan
        .audio_owner_input_candidates_at(at, Default::default())
        .unwrap();
    let member = selected_candidate(&candidates, "repeat");
    let expected = plan
        .audio_owner_input(&owner(&plan, "repeat", 6), at, Default::default())
        .unwrap();
    let capture = candidates.capture(member, Default::default()).unwrap();
    let input = capture.into_input().unwrap();
    assert_captured_input(&input, &expected);
    assert_eq!(
        input.context().root_samples(),
        AudioSample(0)..AudioSample(16)
    );
    assert!(
        input
            .owner()
            .treatment_boundary(0)
            .unwrap()
            .matches(&owner(&plan, "repeat", 0))
    );
    assert!(!input.owner().same_at(&owner(&plan, "repeat", 0), at));
    assert!(input.owner().belongs_to(&plan));
    assert!(!input.owner().belongs_to(&other));
    assert!(
        other
            .audio_owner_input(input.owner(), at, Default::default())
            .is_err()
    );
    let candidates = plan
        .audio_owner_input_candidates_at(at, Default::default())
        .unwrap();
    assert!(matches!(
        candidates.capture(usize::MAX, Default::default()),
        Err(PlanError::InvalidPlan(
            "audio input candidate index out of range"
        ))
    ));
}
