//! Child fields describe local work even after earlier shared-budget activity.
use super::*;
use crate::plan::audio_owners::admission::Budget as QueryBudget;
use crate::plan::audio_owners::tests::legacy_snapshot_v1::flattened_query_baseline as flat;
use crate::{AudioBoundaryRule, SignalSample};

fn limits(budget: &QueryBudget<'_, Policy<true>>) -> AudioQueryLimits {
    AudioQueryLimits {
        maximum_work: budget.remaining,
        ..Default::default()
    }
}

#[test]
fn actual_children_have_local_work_and_lookup_after_nonzero_prefix() {
    let mut roots = 0;
    let mut domains = 0;
    flat::visit_cases(|parent, descriptor| {
        let samples = AudioSample(descriptor["window"]["start"].as_i64().unwrap())
            ..AudioSample(descriptor["window"]["end"].as_i64().unwrap());
        let mut admission = Policy::<true>::new(usize::MAX);
        let mut local = crate::plan::audio::Budget {
            remaining: 65_536,
            lookup: LookupStats {
                visited_nodes: 9,
                sequence_comparisons: 8,
                iteration_run_comparisons: 7,
            },
        };
        {
            let mut budget = QueryBudget::new(&mut local, &mut admission);
            budget.canonical(3).unwrap();
            budget.additional(2).unwrap();
            match parent {
                Parent::Root(plan) => {
                    let signal = plan.audio_signal();
                    let range = SignalSample(samples.start.0)..SignalSample(samples.end.0);
                    for stops in [(false, false), (false, true)] {
                        let fresh = limits(&budget);
                        let ordinary = signal
                            .query_inner(
                                range.clone(),
                                fresh,
                                AudioBoundaryRule::RoundEven,
                                stops.0,
                                stops.1,
                            )
                            .unwrap();
                        let start = budget.checkpoint();
                        let query = signal
                            .structural_child_query(
                                range.clone(),
                                fresh,
                                AudioBoundaryRule::RoundEven,
                                stops,
                                &mut budget,
                            )
                            .unwrap();
                        let usage = budget.usage_since(start).unwrap();
                        assert_eq!(usage.canonical, ordinary.work);
                        assert_eq!(usage.lookup, ordinary.lookup);
                        assert_eq!(query.query.lookup, usage.lookup);
                        assert_eq!(query.query.work, usage.work);
                        assert_eq!(usage.work, usage.canonical + usage.additional);
                        assert_eq!(query.query.spans.len(), ordinary.spans.len());
                        drop(query);
                    }
                    roots += 1;
                }
                Parent::Domain(domain) => {
                    let fresh = limits(&budget);
                    let ordinary = domain.audio(samples.clone(), fresh).unwrap();
                    let start = budget.checkpoint();
                    let query = domain
                        .audio_child_query(samples.clone(), fresh, &mut budget)
                        .unwrap();
                    let usage = budget.usage_since(start).unwrap();
                    assert_eq!(usage.canonical, ordinary.work);
                    assert_eq!(usage.lookup, ordinary.lookup);
                    assert_eq!(query.query.lookup, usage.lookup);
                    assert_eq!(query.query.work, usage.work);
                    assert_eq!(query.query.spans.len(), ordinary.spans.len());
                    let fresh = limits(&budget);
                    let ordinary = domain
                        .processing_inner(samples.clone(), fresh, false)
                        .unwrap();
                    let start = budget.checkpoint();
                    let processing = domain
                        .processing_child_query(samples, fresh, false, &mut budget)
                        .unwrap();
                    let usage = budget.usage_since(start).unwrap();
                    assert_eq!(usage.canonical, ordinary.work);
                    assert_eq!(usage.lookup, ordinary.lookup);
                    assert_eq!(processing.query.lookup, usage.lookup);
                    assert_eq!(processing.query.work, usage.work);
                    assert_eq!(processing.query.spans.len(), ordinary.spans.len());
                    drop(processing);
                    drop(query);
                    domains += 1;
                }
            }
            assert_eq!(
                budget.canonical_used + budget.additional_used,
                65_536 - budget.remaining
            );
            assert_eq!(
                budget.admission.0.borrow().forwarded,
                65_536 - budget.remaining
            );
        }
        assert_eq!(admission.0.borrow().live, 0);
    });
    assert_eq!((roots, domains), (2, 4));
}

#[test]
fn empty_child_headers_have_local_cost_without_replaying_the_prefix() {
    let plan = crate::audio_allocated::query_storage_tests::fixture();
    let signal = plan.audio_signal();
    let mut admission = Policy::<true>::new(usize::MAX);
    let mut local = crate::plan::audio::Budget {
        remaining: 65_536,
        lookup: LookupStats::default(),
    };
    {
        let mut budget = QueryBudget::new(&mut local, &mut admission);
        budget.canonical(7).unwrap();
        budget.additional(4).unwrap();
        let start = budget.checkpoint();
        let value = signal
            .structural_child_query(
                SignalSample(0)..SignalSample(0),
                limits(&budget),
                AudioBoundaryRule::RoundEven,
                (false, false),
                &mut budget,
            )
            .unwrap();
        let usage = budget.usage_since(start).unwrap();
        assert_eq!(usage.canonical, 0);
        assert!(usage.additional > 0);
        assert_eq!(value.query.work, usage.additional);
        assert_eq!(usage.lookup, LookupStats::default());
        assert!(value.query.spans.is_empty());
        assert_eq!(value.query.project_id, plan.metadata.project_id);
        assert_eq!(value.query.revision_id, plan.metadata.revision_id);
        assert_eq!(budget.admission.0.borrow().forwarded, 11 + value.query.work);
        assert!(budget.admission.0.borrow().live > 0);
        drop(value);
    }
    assert_eq!(admission.0.borrow().live, 0);
    assert!(admission.0.borrow().peak > 0);
}
