use super::*;
use crate::plan::audio_owners::admission::{self, Unmetered};
use deadpan_core::{AudioSample, ProjectDocument};
use std::{alloc::Layout, cell::RefCell, rc::Rc};

#[derive(Default)]
struct Counts {
    live: usize,
    peak: usize,
    calls: usize,
    forwarded: usize,
    cap: usize,
    refuse_at: Option<usize>,
}
struct Policy(Rc<RefCell<Counts>>);
struct Lease {
    counts: Rc<RefCell<Counts>>,
    bytes: usize,
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.counts.borrow_mut().live -= self.bytes;
    }
}
impl AudioSoundContextAdmission for Policy {
    const METERED: bool = true;
    type Error = PlanError;
    type Issuer = *const RefCell<Counts>;
    type Lease = Lease;
    fn issuer(&self) -> Self::Issuer {
        Rc::as_ptr(&self.0)
    }
    fn spend(&mut self, units: usize) -> std::result::Result<(), PlanError> {
        self.0.borrow_mut().forwarded += units;
        Ok(())
    }
    fn reserve(&mut self, layout: Layout) -> std::result::Result<Lease, PlanError> {
        let mut state = self.0.borrow_mut();
        state.calls += 1;
        if state.refuse_at == Some(state.calls)
            || state.live.checked_add(layout.size()).is_none()
            || state.live + layout.size() > state.cap
        {
            return Err(PlanError::AudioQueryLimit("raw domain residency"));
        }
        state.live += layout.size();
        state.peak = state.peak.max(state.live);
        Ok(Lease {
            counts: Rc::clone(&self.0),
            bytes: layout.size(),
        })
    }
    fn merge(&mut self, target: &mut Lease, source: &mut Lease) {
        assert!(Rc::ptr_eq(&self.0, &target.counts));
        assert!(Rc::ptr_eq(&self.0, &source.counts));
        target.bytes += source.bytes;
        source.bytes = 0;
    }
}
fn policy(cap: usize) -> Policy {
    Policy(Rc::new(RefCell::new(Counts {
        cap,
        ..Counts::default()
    })))
}
fn with_bound(name: &str, run: impl FnOnce(&RenderPlan, &AudioBound<'_>)) {
    let frozen: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/fixtures/owner-capture-baseline-v1/snapshot.json"
    ))
    .unwrap();
    let fixture = frozen["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["fixture"] == name)
        .unwrap();
    let case = &fixture["cases"][0];
    let doc = ProjectDocument::from_json(&fixture["document"].to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    let at = AudioSample(case["observation_window"]["start"].as_i64().unwrap());
    let member = case["selected_member_index"].as_u64().unwrap() as usize;
    let input = plan
        .audio_owner_input_candidates_at(at, Default::default())
        .unwrap()
        .capture(member, Default::default())
        .unwrap()
        .into_input()
        .unwrap();
    let retained = input.retained_at(at, Default::default()).unwrap().unwrap();
    run(&plan, retained.bound());
}
fn logical(plan: &RenderPlan, value: &AudioBoundDomain<'_>) -> serde_json::Value {
    let mut result =
        crate::plan::audio_owners::tests::legacy_snapshot_v1::capture_baseline::bound_domain(
            plan, value,
        );
    if let AudioBoundDomain::Point(point) = value {
        result["actual_private_point"] = point.probe_baseline_snapshot(plan);
    }
    result
}
const ROOT: &str =
    "actual_source_split_with_preexisting_lattice_retained_root_round_even_clipped_window";
const POINT: &str = "nonunity_shift_fractional_point_ceil_source44100";

#[test]
fn actual_root_and_fractional_point_constructors_match_legacy_without_canonical_work() {
    for fixture in [ROOT, POINT] {
        with_bound(fixture, |plan, bound| {
            let expected = logical(plan, &bound.raw_domain().unwrap());
            let mut local = Budget {
                remaining: 65_536,
                lookup: super::super::super::LookupStats::default(),
            };
            let mut false_policy = Unmetered;
            let mut budget = QueryBudget::new(&mut local, &mut false_policy);
            let actual = bound.raw_domain_admitted(&mut budget).unwrap();
            assert_eq!(budget.canonical_used, 0);
            assert_eq!(budget.additional_used, 0);
            assert_eq!(logical(plan, &actual.value), expected);
            drop(actual);

            let mut local = Budget {
                remaining: 65_536,
                lookup: super::super::super::LookupStats::default(),
            };
            let mut policy = policy(usize::MAX);
            let mut budget = QueryBudget::new(&mut local, &mut policy);
            let actual = bound.raw_domain_admitted(&mut budget).unwrap();
            assert_eq!(budget.canonical_used, 0);
            let additional = budget.additional_used;
            assert!(additional > 0);
            assert_eq!(budget.admission.0.borrow().forwarded, additional);
            assert_eq!(logical(plan, &actual.value), expected);
            assert!(budget.admission.0.borrow().live > 0);
            drop(actual);
            assert_eq!(budget.admission.0.borrow().live, 0);
            assert_eq!(budget.admission.0.borrow().forwarded, additional);
        });
    }
}

#[test]
fn actual_raw_construction_peak_exact_and_one_byte_short_release_all_leases() {
    for fixture in [ROOT, POINT] {
        with_bound(fixture, |plan, bound| {
            let expected = logical(plan, &bound.raw_domain().unwrap());
            let mut policy = policy(usize::MAX);
            let mut local = Budget {
                remaining: 65_536,
                lookup: super::super::super::LookupStats::default(),
            };
            let actual = admission::outer::<_, Policy>(
                bound.raw_domain_admitted(&mut QueryBudget::new(&mut local, &mut policy)),
            )
            .unwrap();
            let peak = policy.0.borrow().peak;
            let retained = policy.0.borrow().live;
            let calls = policy.0.borrow().calls;
            assert!(
                peak > retained,
                "temporary definition and replacement overlap"
            );
            assert!(calls > 1);
            drop(actual);
            assert_eq!(policy.0.borrow().live, 0);
            for cap in [peak, peak - 1] {
                let mut policy = self::policy(cap);
                let mut local = Budget {
                    remaining: 65_536,
                    lookup: super::super::super::LookupStats::default(),
                };
                let actual = admission::outer::<_, Policy>(
                    bound.raw_domain_admitted(&mut QueryBudget::new(&mut local, &mut policy)),
                );
                if cap == peak {
                    let actual = actual.unwrap();
                    assert_eq!(logical(plan, &actual.value), expected);
                    drop(actual);
                } else {
                    assert!(matches!(
                        actual,
                        Err(PlanError::AudioQueryLimit("raw domain residency"))
                    ));
                }
                assert_eq!(policy.0.borrow().live, 0);
                assert!(policy.0.borrow().forwarded > 0, "no work refund");
            }
        });
    }
}

#[test]
fn refusal_at_every_real_reservation_releases_partly_built_root_and_point() {
    for fixture in [ROOT, POINT] {
        with_bound(fixture, |_, bound| {
            let mut ample = policy(usize::MAX);
            let mut local = Budget {
                remaining: 65_536,
                lookup: super::super::super::LookupStats::default(),
            };
            drop(
                admission::outer::<_, Policy>(
                    bound.raw_domain_admitted(&mut QueryBudget::new(&mut local, &mut ample)),
                )
                .unwrap(),
            );
            let calls = ample.0.borrow().calls;
            for at in 1..=calls {
                let mut policy = policy(usize::MAX);
                policy.0.borrow_mut().refuse_at = Some(at);
                let mut local = Budget {
                    remaining: 65_536,
                    lookup: super::super::super::LookupStats::default(),
                };
                let failed = admission::outer::<_, Policy>(
                    bound.raw_domain_admitted(&mut QueryBudget::new(&mut local, &mut policy)),
                );
                assert!(matches!(
                    failed,
                    Err(PlanError::AudioQueryLimit("raw domain residency"))
                ));
                assert_eq!(policy.0.borrow().live, 0);
                assert!(policy.0.borrow().forwarded > 0);
            }
        });
    }
}
