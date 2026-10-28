//! Cold observations are separate from the actual reader masks.
use super::*;

fn cold_control<'a>(cancelled: &'a AtomicBool, work: &'a RefCell<ReadWork>) -> WorkControl<'a> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + TIMEOUT,
        work,
    }
}

#[test]
fn prepared_routed_source_and_occurrence_policy_matches_real_wav_reads() {
    for occurrence in [false, true] {
        let plan = Arc::new(qualified_plan(false));
        let domain = domain(&plan, occurrence);
        let mut stage = StageAudio::new(Arc::clone(&plan));
        let cancelled = AtomicBool::new(false);
        let preparation = PreparationBudget::new(TIMEOUT, &cancelled).unwrap();
        // Cover a route gap and the source recipe's final support separately.
        for start in [AudioSample(16), AudioSample(domain.samples().end.0 - 96)] {
            let prepared = stage
                .preflight_routed_root(&domain, start, 96, preparation.control())
                .unwrap();
            let cold_work = RefCell::new(ReadWork::default());
            assert_eq!(prepared.policy_support(), policy_ranges::Support::Complete);
            assert_eq!(cold_work.borrow().plan_work, 0);
            let mut predicted = Vec::new();
            assert_eq!(
                prepared
                    .visit_policy(cold_control(&cancelled, &cold_work), |range| {
                        predicted.push(range);
                        Ok(())
                    })
                    .unwrap(),
                policy_ranges::Support::Complete,
            );
            assert_eq!(cold_work.borrow().source_checks, 0);
            assert_eq!(cold_work.borrow().prepared_frames, 0);
            let mut provider = Provider::new();
            stage.active_frames += 96 * 3;
            let actual = stage
                .read_routed_root_borrowed(&mut provider, &prepared, preparation.control())
                .unwrap();
            stage.active_frames -= 96 * 3;
            assert_eq!(merged_suppression(predicted), actual.suppressed);
            assert!(actual.exhausted.is_empty());
            assert!(provider.calls > 0);
            if start == AudioSample(16) {
                assert_eq!(actual.suppressed, vec![AudioSample(32)..AudioSample(64)]);
                assert_eq!(cold_work.borrow().plan_work, 8);
            } else {
                assert!(
                    !actual.suppressed.is_empty(),
                    "recipe exhaustion remains policy"
                );
            }
        }
    }
}

#[test]
fn direct_signal_and_current_occurrence_forwarders_match_real_masks() {
    let plan = Arc::new(qualified_plan(false));
    let mut stage = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let budget = PreparationBudget::new(TIMEOUT, &cancelled).unwrap();
    let signal = plan.audio_signal();
    let end = signal.sample_count().unwrap();
    let start = SignalSample(end.0 - 96);
    let query = signal.query(start..end, Default::default()).unwrap();
    let policy = signal.policy(start..end, Default::default()).unwrap();
    let direct = sound_events::PreparedSoundInput::Direct {
        signal: query,
        policy,
    };
    let cold_work = RefCell::new(ReadWork::default());
    assert_eq!(direct.policy_support(), policy_ranges::Support::Complete);
    let mut predicted = Vec::new();
    assert_eq!(
        direct
            .visit_policy(cold_control(&cancelled, &cold_work), |range| {
                predicted.push(range);
                Ok(())
            })
            .unwrap(),
        policy_ranges::Support::Complete,
    );
    let mut provider = Provider::new();
    let actual = stage
        .read_prepared_root_sound(
            &mut provider,
            direct,
            AudioSample(start.0),
            budget.control(),
        )
        .unwrap();
    assert_eq!(merged_suppression(predicted), actual.suppressed);
    assert!(actual.exhausted.is_empty());

    let routed = domain(&plan, true);
    let deadpan_plan::AudioRoutedRootInput::Occurrence(voice) = routed.input() else {
        panic!("expected occurrence");
    };
    let samples = AudioSample(voice.samples().end.0 - 96)..voice.samples().end;
    let queries = stage
        .source_occurrence_queries(voice, samples.clone(), budget.control())
        .unwrap();
    assert!(queries.flattened.spans.is_empty());
    let current = beat_sounds::PreparedBeatInput::Current(Box::new(queries));
    let cold_work = RefCell::new(ReadWork::default());
    assert_eq!(current.policy_support(), policy_ranges::Support::Complete);
    let mut predicted = Vec::new();
    assert_eq!(
        current
            .visit_policy(
                samples.clone(),
                cold_control(&cancelled, &cold_work),
                |range| {
                    predicted.push(range);
                    Ok(())
                }
            )
            .unwrap(),
        policy_ranges::Support::Complete,
    );
    let actual = stage
        .read_prepared_beat_sound(&mut provider, current, samples, budget.control())
        .unwrap();
    let mut expected = actual.suppressed;
    expected.extend(actual.exhausted);
    assert_eq!(merged_suppression(predicted), merged_suppression(expected));
    assert_eq!(cold_work.borrow().source_checks, 0);
    assert_eq!(cold_work.borrow().prepared_frames, 0);
}

#[test]
fn routed_exact_budget_partial_work_and_cancel_do_not_become_permissions() {
    let plan = Arc::new(qualified_plan(false));
    let domain = domain(&plan, false);
    let stage = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let budget = PreparationBudget::new(TIMEOUT, &cancelled).unwrap();
    let prepared = stage
        .preflight_routed_root(&domain, AudioSample(16), 96, budget.control())
        .unwrap();
    let work = RefCell::new(ReadWork {
        plan_work: MAX_PLAN_WORK_PER_READ - 8,
        ..Default::default()
    });
    assert_eq!(
        prepared
            .visit_policy(cold_control(&cancelled, &work), |_| Ok(()))
            .unwrap(),
        policy_ranges::Support::Complete,
    );
    assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
    work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ - 7;
    let mut partial = Vec::new();
    assert!(matches!(
        prepared.visit_policy(cold_control(&cancelled, &work), |range| {
            partial.push(range);
            Ok(())
        }),
        Err(StageAudioError::Limit("plan work per read"))
    ));
    assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
    assert_eq!(partial, vec![AudioSample(32)..AudioSample(64)]);
    // This observation is incomplete and must be discarded by the caller.
    partial.clear();
    work.borrow_mut().plan_work = 0;
    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(matches!(
        prepared.visit_policy(cold_control(&cancelled, &work), |range| {
            partial.push(range);
            Ok(())
        }),
        Err(StageAudioError::Preparation(PreparationError::Cancelled))
    ));
    assert!(partial.is_empty());
    assert_eq!(work.borrow().plan_work, 0);
}

#[test]
fn projected_signed_root_is_unsupported_before_any_policy_observation() {
    use deadpan_plan::{AudioProjectedRoot, AudioRootPlacement};
    let plan = Arc::new(qualified_plan(true));
    let occurrence = domain(&plan, true);
    let deadpan_plan::AudioRoutedRootInput::Occurrence(voice) = occurrence.input() else {
        panic!("expected occurrence");
    };
    let projection = Arc::clone(voice.processing_projection().unwrap());
    let root = AudioProjectedRoot::new(
        Arc::clone(&projection),
        AudioRootPlacement::new(
            ExactRatio::integer(-1),
            ExactRatio::ONE,
            ExactRatio::ZERO..ExactRatio::integer(projection.output_frames()),
        )
        .unwrap(),
    )
    .unwrap();
    let extent = root.extent();
    let step = ExactRatio::new(30_000, 48_000 * 1001).unwrap();
    let grid = AudioSampleGrid::new(
        ExactRatio::ZERO.checked_sub(extent.start).unwrap(),
        step,
        AudioBoundaryRule::RoundEven,
    )
    .unwrap();
    let route = SoundRoute::identity(extent.end.checked_sub(extent.start).unwrap()).unwrap();
    let count = route.nodes().len();
    let routed = AudioRoutedRoot::new(
        root,
        AudioSoundRoute::<AudioSample>::new(route, vec![grid; count]).unwrap(),
    )
    .unwrap();
    assert!(routed.samples().start.0 < 0);
    let stage = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let budget = PreparationBudget::new(TIMEOUT, &cancelled).unwrap();
    let prepared = stage
        .preflight_routed_root(&routed, routed.samples().start, 16, budget.control())
        .unwrap();
    let work = RefCell::new(ReadWork::default());
    let mut observations = 0;
    assert_eq!(
        prepared
            .visit_policy(cold_control(&cancelled, &work), |_| {
                observations += 1;
                Ok(())
            })
            .unwrap(),
        policy_ranges::Support::Unsupported,
    );
    assert_eq!(observations, 0);
    assert_eq!(work.borrow().plan_work, 0);
    assert_eq!(work.borrow().source_checks, 0);
    assert_eq!(work.borrow().prepared_frames, 0);
    assert_eq!(
        prepared.policy_support(),
        policy_ranges::Support::Unsupported
    );
    let root_input = sound_events::PreparedSoundInput::Routed(prepared);
    assert_eq!(
        root_input.policy_support(),
        policy_ranges::Support::Unsupported
    );
    assert_eq!(
        root_input
            .visit_policy(cold_control(&cancelled, &work), |_| {
                observations += 1;
                Ok(())
            })
            .unwrap(),
        policy_ranges::Support::Unsupported,
    );
    let sound_events::PreparedSoundInput::Routed(query) = root_input else {
        panic!("expected routed input");
    };
    let beat_input = beat_sounds::PreparedBeatInput::Routed {
        plan: Arc::clone(&plan),
        query,
    };
    assert_eq!(
        beat_input.policy_support(),
        policy_ranges::Support::Unsupported
    );
    assert_eq!(
        beat_input
            .visit_policy(routed.samples(), cold_control(&cancelled, &work), |_| {
                observations += 1;
                Ok(())
            })
            .unwrap(),
        policy_ranges::Support::Unsupported,
    );
    assert_eq!(observations, 0);
    assert_eq!(work.borrow().plan_work, 0);
}

#[test]
fn routed_policy_translation_keeps_signed_labels_exact_and_checks_overflow() {
    let span = PreparedRootSpan {
        destination: AudioSample(-20)..AudioSample(-10),
        old: Some(-100..-90),
        input: None,
    };
    assert_eq!(
        span.translate_policy(&(-100..-90), AudioSample(-98)..AudioSample(-95))
            .unwrap(),
        AudioSample(-18)..AudioSample(-15),
    );
    let overflow = PreparedRootSpan {
        destination: AudioSample(i64::MAX)..AudioSample(i64::MAX),
        old: Some(i64::MIN..0),
        input: None,
    };
    assert!(
        overflow
            .translate_policy(&(i64::MIN..0), AudioSample(0)..AudioSample(1))
            .is_err()
    );
}
