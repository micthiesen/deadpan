use super::*;
use deadpan_core::*;
use deadpan_plan::LookupStats;

#[derive(Clone, Copy, Debug)]
enum BindingCase {
    Source,
    Unrelated,
    Preserve,
    ShiftAtUnity,
}

fn bound_fixture(case: BindingCase) -> (Arc<RenderPlan>, Provider) {
    let (base, provider) = fixture_document_at_rate(FrameRate::new(48_000, 1).unwrap());
    let id = |name: &str| NodeId::new(name).unwrap();
    let mut wire = serde_json::to_value(base).unwrap();
    let source = wire["nodes"]["a"].clone();
    let pitch = match case {
        BindingCase::Preserve => Some(PitchPolicy::Preserve),
        BindingCase::ShiftAtUnity => Some(PitchPolicy::Shift { semitones: 3 }),
        _ => None,
    };
    let mut nodes = BTreeMap::from([
        (
            "root",
            serde_json::to_value(BeatNode::sequence(
                "Root",
                vec![
                    id(if pitch.is_some() { "stage" } else { "a" }),
                    id("silent"),
                ],
            ))
            .unwrap(),
        ),
        ("a", source),
        (
            "silent",
            serde_json::to_value(BeatNode::hold(
                "Silent sibling",
                HoldRecipe {
                    picture_context: None,
                    duration: FrameDuration::new(128).unwrap(),
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                },
            ))
            .unwrap(),
        ),
    ]);
    if let Some(pitch) = pitch {
        nodes.insert(
            "stage",
            serde_json::to_value(BeatNode {
                framing: None,
                label: "Processing".into(),
                audio_treatments: Default::default(),
                audio_editorial_edges: Default::default(),
                audio_edges: Default::default(),
                kind: NodeKind::Retime {
                    child: id("a"),
                    duration: FrameDuration::new(if matches!(case, BindingCase::Preserve) {
                        768
                    } else {
                        512
                    })
                    .unwrap(),
                    mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(512)).unwrap(),
                    pitch,
                    purpose: RetimePurpose::Edit,
                },
                cutaways: Vec::new(),
                captions: Vec::new(),
            })
            .unwrap(),
        );
    }
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let timing = AudioTimingId {
        allocation: RevisionId::new("policy-reuse-timing").unwrap(),
        ordinal: 0,
    };
    let bound_owner = if matches!(case, BindingCase::Source) {
        "a"
    } else {
        "silent"
    };
    let bindings = AudioBindingState::new(
        vec![AudioTimingRecord {
            id: timing.clone(),
            layout: FrozenAudioLayout::capture(&document).unwrap(),
        }],
        BTreeMap::from([(
            id(bound_owner),
            OwnedAudioBinding {
                reanchors: Vec::new(),
                lattice: AudioPlacementTemplate {
                    reference_local_offset: ExactRatio::ZERO,
                    gap_after: None,
                    reference: AudioReferenceClock {
                        recipe: AudioRecipeKind::Node,
                        timing,
                        root: AudioClockRoot::ProjectRootRoundEven,
                        physical: id(bound_owner),
                    },
                    arguments: Vec::new(),
                    births: Vec::new(),
                },
                resume: None,
            },
        )]),
    )
    .unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    (Arc::new(RenderPlan::compile(&document).unwrap()), provider)
}

fn root_input(plan: &RenderPlan) -> deadpan_plan::AudioOwnerInput<'_> {
    let owners = plan
        .audio_gain_owners(AudioSample(0)..AudioSample(1), Default::default())
        .unwrap();
    let owner = owners.spans()[0]
        .owners()
        .iter()
        .find(|owner| owner.instance().node.as_str() == "root")
        .unwrap();
    plan.audio_owner_input(owner, AudioSample(0), Default::default())
        .unwrap()
}

fn removed_lookup(old: LookupStats, new: LookupStats) -> LookupStats {
    LookupStats {
        visited_nodes: old.visited_nodes.checked_sub(new.visited_nodes).unwrap(),
        sequence_comparisons: old
            .sequence_comparisons
            .checked_sub(new.sequence_comparisons)
            .unwrap(),
        iteration_run_comparisons: old
            .iteration_run_comparisons
            .checked_sub(new.iteration_run_comparisons)
            .unwrap(),
    }
}

fn assert_trace_delta(old: &[DomainPolicyTrace], new: &[DomainPolicyTrace]) -> usize {
    assert_eq!(old.len(), new.len());
    old.iter()
        .zip(new)
        .map(|(old, new)| {
            assert_eq!(old.domain_instance, new.domain_instance);
            assert_eq!(old.domain_root_samples, new.domain_root_samples);
            assert_eq!(old.domain_visible_samples, new.domain_visible_samples);
            assert_eq!(old.samples, new.samples);
            assert_eq!(old.processing_work, new.processing_work);
            assert_eq!(old.processing_lookup, new.processing_lookup);
            assert_eq!(old.content_only_eligible, new.content_only_eligible);
            let removed = old.policy_work.checked_sub(new.policy_work).unwrap();
            if removed == 0 {
                assert_eq!(old.policy_lookup, new.policy_lookup);
            } else {
                assert!(old.content_only_eligible);
                assert!(!old.samples.is_empty());
                // Plan's immutable full-private oracle proves that the first
                // Leaf/Bound tape equals this actual OLD second policy walk.
                assert_eq!(removed, old.processing_work);
                assert_eq!(
                    removed_lookup(old.policy_lookup, new.policy_lookup),
                    old.processing_lookup
                );
            }
            removed
        })
        .sum()
}

#[test]
fn binding_domain_reuses_one_walk_without_changing_queries_or_pcm() {
    for case in [BindingCase::Source, BindingCase::Unrelated] {
        for edge_fades in [false, true] {
            let (plan, mut provider) = bound_fixture(case);
            let (_, mut old_provider) = bound_fixture(case);
            let input = root_input(&plan);
            let domain = input.context();
            let cancelled = AtomicBool::new(false);
            let work = RefCell::new(ReadWork::default());
            let old_work = RefCell::new(ReadWork::default());
            let mut reader = StageAudio::new(Arc::clone(&plan));
            let mut old_reader = StageAudio::new(Arc::clone(&plan));
            let (request, trace) = with_domain_policy_trace(false, || {
                reader
                    .prepare_original_domain(
                        domain,
                        AudioSample(0),
                        64,
                        work_control(&cancelled, &work),
                        edge_fades,
                    )
                    .unwrap()
            });
            let (old, old_trace) = with_domain_policy_trace(true, || {
                old_reader
                    .prepare_original_domain(
                        domain,
                        AudioSample(0),
                        64,
                        work_control(&cancelled, &old_work),
                        edge_fades,
                    )
                    .unwrap()
            });
            assert_eq!(trace.len(), 1);
            let removed = assert_trace_delta(&old_trace, &trace);
            assert_eq!(removed, old.queries().processing.work);
            assert!(removed > 0, "must execute a binding-policy reuse");
            assert_eq!(
                old_work.borrow().plan_work - work.borrow().plan_work,
                removed
            );
            let new_queries = request.queries();
            let old_queries = old.queries();
            assert_eq!(new_queries.flattened, old_queries.flattened);
            assert_eq!(new_queries.processing, old_queries.processing);
            assert_eq!(new_queries.policy.suppressed, old_queries.policy.suppressed);
            assert_eq!(new_queries.policy.contents, old_queries.policy.contents);
            assert_eq!(new_queries.fades, old_queries.fades);
            if matches!(case, BindingCase::Source) {
                assert!(
                    new_queries
                        .processing
                        .spans
                        .iter()
                        .any(|span| matches!(span.content, AudioSignalContent::Bound(_)))
                );
            }
            assert_eq!(provider.calls, 0);
            assert_eq!(old_provider.calls, 0);
            let (block, read_trace) = with_domain_policy_trace(false, || {
                reader
                    .read_prepared_original(
                        &mut provider,
                        request,
                        work_control(&cancelled, &work),
                        0,
                    )
                    .unwrap()
            });
            let (old_block, old_read_trace) = with_domain_policy_trace(true, || {
                old_reader
                    .read_prepared_original(
                        &mut old_provider,
                        old,
                        work_control(&cancelled, &old_work),
                        0,
                    )
                    .unwrap()
            });
            same_blocks(&block, &old_block);
            assert_eq!(provider.calls, old_provider.calls);
            assert_eq!(work.borrow().source_checks, old_work.borrow().source_checks);
            assert_eq!(
                old_work.borrow().plan_work - work.borrow().plan_work,
                removed + assert_trace_delta(&old_read_trace, &read_trace)
            );
        }
    }
}

#[test]
fn exposed_stage_fallback_preserves_exact_and_one_short_global_budgets() {
    for case in [BindingCase::Preserve, BindingCase::ShiftAtUnity] {
        for edge_fades in [false, true] {
            let (plan, provider) = bound_fixture(case);
            let input = root_input(&plan);
            let domain = input.context();
            let first = domain
                .processing(AudioSample(0)..AudioSample(64), Default::default())
                .unwrap();
            assert!(
                first
                    .spans
                    .iter()
                    .any(|span| matches!(span.content, AudioSignalContent::Stage(_)))
            );
            let cancelled = AtomicBool::new(false);
            let reader = StageAudio::new(Arc::clone(&plan));
            let measured = RefCell::new(ReadWork::default());
            previous_queries(
                &plan,
                Some(domain),
                AudioSample(0),
                64,
                work_control(&cancelled, &measured),
                edge_fades,
            )
            .unwrap();
            let needed = measured.borrow().plan_work;
            for remaining in [needed, needed - 1] {
                let work = RefCell::new(ReadWork {
                    plan_work: MAX_PLAN_WORK_PER_READ - remaining,
                    ..Default::default()
                });
                let old_work = RefCell::new(ReadWork {
                    plan_work: MAX_PLAN_WORK_PER_READ - remaining,
                    ..Default::default()
                });
                let (actual, trace) = with_domain_policy_trace(false, || {
                    reader.prepare_original_domain(
                        domain,
                        AudioSample(0),
                        64,
                        work_control(&cancelled, &work),
                        edge_fades,
                    )
                });
                let (old, old_trace) = with_domain_policy_trace(true, || {
                    reader.prepare_original_domain(
                        domain,
                        AudioSample(0),
                        64,
                        work_control(&cancelled, &old_work),
                        edge_fades,
                    )
                });
                assert_eq!(actual.is_ok(), remaining == needed);
                assert_eq!(actual.is_ok(), old.is_ok());
                assert_eq!(
                    actual.err().map(|error| error.to_string()),
                    old.err().map(|error| error.to_string())
                );
                assert_eq!(trace, old_trace);
                assert_eq!(work.borrow().plan_work, old_work.borrow().plan_work);
            }
            assert_eq!(provider.calls, 0);
        }
    }
}

#[test]
fn failed_first_processing_query_keeps_old_partial_bill_and_error() {
    let (plan, provider) = bound_fixture(BindingCase::Source);
    let input = root_input(&plan);
    let domain = input.context();
    let range = AudioSample(0)..AudioSample(64);
    let flattened = domain.audio(range.clone(), Default::default()).unwrap();
    let processing = domain.processing(range, Default::default()).unwrap();
    assert!(processing.work > 1);
    let remaining = flattened.work + processing.work - 1;
    let cancelled = AtomicBool::new(false);
    let reader = StageAudio::new(Arc::clone(&plan));
    let mut failures = Vec::new();
    for legacy in [true, false] {
        let work = RefCell::new(ReadWork {
            plan_work: MAX_PLAN_WORK_PER_READ - remaining,
            ..Default::default()
        });
        let initial = work.borrow().plan_work;
        let (result, trace) = with_domain_policy_trace(legacy, || {
            reader.prepare_original_domain(
                domain,
                AudioSample(0),
                64,
                work_control(&cancelled, &work),
                true,
            )
        });
        assert!(
            trace.is_empty(),
            "no successful policy bill may be recorded"
        );
        assert_eq!(work.borrow().plan_work - initial, flattened.work);
        failures.push(result.err().unwrap().to_string());
    }
    assert_eq!(failures[0], failures[1]);
    assert_eq!(provider.calls, 0);
}

#[test]
fn root_preparation_with_bindings_keeps_the_original_query_and_work_path() {
    let (plan, provider) = bound_fixture(BindingCase::Source);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let old_work = RefCell::new(ReadWork::default());
    let reader = StageAudio::new(Arc::clone(&plan));
    let (request, trace) = with_domain_policy_trace(false, || {
        reader
            .prepare_original_root(
                &plan,
                AudioSample(0),
                64,
                work_control(&cancelled, &work),
                true,
            )
            .unwrap()
    });
    let old = previous_queries(
        &plan,
        None,
        AudioSample(0),
        64,
        work_control(&cancelled, &old_work),
        true,
    )
    .unwrap();
    same_queries(request.queries(), &old);
    assert_eq!(work.borrow().plan_work, old_work.borrow().plan_work);
    assert!(trace.is_empty());
    assert_eq!(provider.calls, 0);
}

#[test]
fn eligible_domain_preparation_obeys_its_exact_new_cumulative_budget() {
    for case in [BindingCase::Source, BindingCase::Unrelated] {
        for edge_fades in [false, true] {
            let (plan, provider) = bound_fixture(case);
            let input = root_input(&plan);
            let domain = input.context();
            let reader = StageAudio::new(Arc::clone(&plan));
            let cancelled = AtomicBool::new(false);
            let measured = RefCell::new(ReadWork::default());
            let (reference, reference_trace) = with_domain_policy_trace(false, || {
                reader
                    .prepare_original_domain(
                        domain,
                        AudioSample(0),
                        64,
                        work_control(&cancelled, &measured),
                        edge_fades,
                    )
                    .unwrap()
            });
            assert_eq!(reference_trace.len(), 1);
            assert!(reference_trace[0].content_only_eligible);
            let queries = reference.queries();
            let first_work = queries.processing.work;
            let policy_work = queries.policy.work;
            let fade_work = queries.fades.map_or(0, |query| query.work);
            let needed = measured.borrow().plan_work;
            assert_eq!(reference_trace[0].processing_work, first_work);
            assert_eq!(reference_trace[0].policy_work, policy_work);
            assert_eq!(
                needed,
                queries.flattened.work + first_work + policy_work + fade_work,
                "each actual query is billed once"
            );
            assert!(needed > 1);
            for remaining in [needed, needed - 1] {
                let work = RefCell::new(ReadWork {
                    plan_work: MAX_PLAN_WORK_PER_READ - remaining,
                    ..Default::default()
                });
                let initial = work.borrow().plan_work;
                let (actual, trace) = with_domain_policy_trace(false, || {
                    reader.prepare_original_domain(
                        domain,
                        AudioSample(0),
                        64,
                        work_control(&cancelled, &work),
                        edge_fades,
                    )
                });
                if remaining == needed {
                    let actual = actual.unwrap();
                    same_queries(actual.queries(), &reference.queries);
                    assert_eq!(trace, reference_trace);
                    assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
                } else {
                    assert!(actual.is_err(), "one short cannot finish preparation");
                    let completed = queries.flattened.work
                        + first_work
                        + if edge_fades { policy_work } else { 0 };
                    assert_eq!(work.borrow().plan_work, initial + completed);
                    assert!(work.borrow().plan_work < MAX_PLAN_WORK_PER_READ);
                    assert_eq!(trace.len(), usize::from(edge_fades));
                    if edge_fades {
                        assert_eq!(trace, reference_trace);
                    }
                }
            }
            let old_work = RefCell::new(ReadWork {
                plan_work: MAX_PLAN_WORK_PER_READ - needed,
                ..Default::default()
            });
            let (old, _) = with_domain_policy_trace(true, || {
                reader.prepare_original_domain(
                    domain,
                    AudioSample(0),
                    64,
                    work_control(&cancelled, &old_work),
                    edge_fades,
                )
            });
            assert!(
                old.is_err(),
                "the actual old extra walk exceeds the new boundary"
            );
            assert!(old_work.borrow().plan_work <= MAX_PLAN_WORK_PER_READ);
            assert_eq!(provider.calls, 0);
        }
    }
}
