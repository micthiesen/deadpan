//! Logical OLD outputs and accepted admission receipts remain separate evidence.
use super::*;
use crate::AdmittedAudioFadeQuery;
use crate::plan::audio_owners::tests::legacy_snapshot_v1::fade_query_baseline as oracle;
use std::sync::OnceLock;

fn frozen() -> &'static Value {
    static BODY: OnceLock<Value> = OnceLock::new();
    BODY.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../tests/fixtures/fade-query-baseline-v1/snapshot.json"
        ))
        .unwrap()
    })
}

fn samples(value: &Value) -> Range<AudioSample> {
    AudioSample(value["start"].as_i64().unwrap())..AudioSample(value["end"].as_i64().unwrap())
}

fn limits(value: &Value) -> AudioQueryLimits {
    AudioQueryLimits {
        maximum_work: value["maximum_work"].as_u64().unwrap().try_into().unwrap(),
        maximum_spans: value["maximum_spans"].as_u64().unwrap().try_into().unwrap(),
    }
}

fn admitted<A: AudioSoundContextAdmission>(
    parent: Parent<'_, '_>,
    range: Range<AudioSample>,
    limits: AudioQueryLimits,
    admission: &mut A,
) -> Result<AdmittedAudioFadeQuery<A::Issuer, A::Lease>, A::Error> {
    match parent {
        Parent::Root(plan) => plan.audio_fades_admitted(range, limits, admission),
        Parent::Domain(domain) => domain.fades_admitted(range, limits, admission),
    }
}

fn visit(mut callback: impl FnMut(Parent<'_, '_>, &Value)) {
    let mut count = 0;
    oracle::visit_cases(|parent, descriptor| {
        let case = frozen()["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == descriptor["name"])
            .unwrap();
        for (key, value) in descriptor.as_object().unwrap() {
            assert_eq!(&case[key], value, "checked fade provenance {key}");
        }
        callback(parent, case);
        count += 1;
    });
    assert_eq!(count, 80);
}

fn full(case: &Value) -> &Value {
    let first = &case["observations"][0];
    assert_eq!(first["name"], "default_full");
    first
}

fn assert_true(
    value: &AdmittedAudioFadeQuery<usize, Lease>,
    old: &Value,
    admission: &Policy<true>,
) {
    let query = oracle::query_snapshot(value.query());
    assert_eq!(query.as_object().unwrap().len(), 3);
    assert_eq!(query["spans"], old["spans"]);
    assert_eq!(query["lookup"], old["lookup"]);
    assert_eq!(
        value.canonical_units(),
        old["work"].as_u64().unwrap() as usize
    );
    assert_eq!(
        value.work(),
        value.canonical_units() + value.additional_units()
    );
    assert_eq!(query["work"].as_u64().unwrap() as usize, value.work());
    assert!(value.was_forwarded());
    assert_eq!(admission.0.borrow().forwarded, value.work());
}

#[test]
fn false_replays_every_complete_root_and_domain_old_fade_observation() {
    let mut calls = 0;
    visit(|parent, case| {
        for row in case["observations"].as_array().unwrap() {
            let mut admission = Policy::<false>::new(0);
            let result = admitted(
                parent,
                samples(&row["request"]["samples"]),
                limits(&row["request"]["limits"]),
                &mut admission,
            );
            match result {
                Ok(value) => {
                    assert_eq!(row["output"]["result"], "Ok", "{}", row["name"]);
                    assert_eq!(
                        oracle::query_snapshot(value.query()),
                        row["output"]["query"]
                    );
                    assert_eq!(value.canonical_units(), value.query().work);
                    assert_eq!(value.work(), value.query().work);
                    assert_eq!(value.additional_units(), 0);
                    assert!(!value.was_forwarded());
                    drop(value);
                }
                Err(error) => {
                    assert_eq!(
                        oracle::result_snapshot(&Err(error)),
                        row["output"],
                        "{} {}",
                        case["name"],
                        row["name"]
                    );
                }
            }
            let state = admission.0.borrow();
            assert_eq!(
                (
                    state.forwarded,
                    state.work_calls,
                    state.reservations,
                    state.live
                ),
                (0, 0, 0, 0)
            );
            calls += 1;
        }
    });
    assert_eq!(calls, 2_354);
}

#[test]
fn true_all_old_successes_keep_literal_fields_canonical_work_and_actual_receipts() {
    let mut calls = 0;
    visit(|parent, case| {
        for row in case["observations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["output"]["result"] == "Ok")
        {
            let mut admission = Policy::<true>::new(usize::MAX);
            let range = samples(&row["request"]["samples"]);
            let empty = range.is_empty();
            let mut requested_limits = limits(&row["request"]["limits"]);
            // The unchanged work maximum admits new fees; OLD span caps stay literal.
            requested_limits.maximum_work = AudioQueryLimits::default().maximum_work;
            let value = admitted(parent, range, requested_limits, &mut admission).unwrap();
            assert_true(&value, &row["output"]["query"], &admission);
            if empty {
                assert_eq!((value.work(), value.additional_units()), (0, 0));
                let state = admission.0.borrow();
                assert_eq!(
                    (state.reservations, state.work_calls, state.live),
                    (0, 0, 0)
                );
            } else {
                assert!(admission.0.borrow().live > 0);
                assert!(value.additional_units() > 0);
            }
            drop(value);
            assert_eq!(admission.0.borrow().live, 0);
            calls += 1;
        }
    });
    assert_eq!(calls, 1_046);
}

#[test]
fn every_full_parent_keeps_exact_total_work_and_requested_peak_boundaries() {
    visit(|parent, case| {
        let row = full(case);
        let range = samples(&row["request"]["samples"]);
        let mut generous = Policy::<true>::new(usize::MAX);
        let mut observer = Observer::new(&mut generous);
        let value = admitted(parent, range.clone(), Default::default(), &mut observer).unwrap();
        let allocations = observer.reservation_layouts.clone();
        let prefixes = observer.reservations.clone();
        drop(observer);
        assert_true(&value, &row["output"]["query"], &generous);
        let work = value.work();
        let peak = generous.0.borrow().peak;
        assert!(work > 1 && peak > 0);
        drop(value);
        assert_eq!(generous.0.borrow().live, 0);

        let mut exact = Policy::<true>::new(peak);
        let value = admitted(
            parent,
            range.clone(),
            AudioQueryLimits {
                maximum_work: work,
                ..Default::default()
            },
            &mut exact,
        )
        .unwrap();
        assert_true(&value, &row["output"]["query"], &exact);
        assert_eq!(exact.0.borrow().peak, peak);
        drop(value);
        assert_eq!(exact.0.borrow().live, 0);

        let mut short_work = Policy::<true>::new(peak);
        let result = admitted(
            parent,
            range.clone(),
            AudioQueryLimits {
                maximum_work: work - 1,
                ..Default::default()
            },
            &mut short_work,
        );
        assert!(matches!(
            result,
            Err(PlanError::AudioQueryLimit("structural work"))
        ));
        let state = short_work.0.borrow();
        assert!(state.forwarded > 0 && state.forwarded < work);
        assert_eq!(state.live, 0);
        drop(state);

        let mut short_memory = Policy::<true>::new(peak - 1);
        let refused = allocations
            .iter()
            .position(|(live, size)| live.checked_add(*size).unwrap() > peak - 1)
            .expect("the exact peak has an actual reservation");
        assert!(matches!(
            admitted(parent, range, Default::default(), &mut short_memory),
            Err(PlanError::AudioQueryLimit("test wide allocation refusal"))
        ));
        assert_eq!(short_memory.0.borrow().reservations, refused + 1);
        assert_eq!(short_memory.0.borrow().forwarded, prefixes[refused]);
        assert_eq!(short_memory.0.borrow().live, 0);
    });
}

#[test]
fn limits_and_range_rejections_precede_callbacks_but_empty_positive_limits_are_free() {
    let mut checked = 0;
    visit(|parent, case| {
        for row in case["observations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| {
                row["output"]["result"] == "Error"
                    && matches!(
                        row["output"]["error"]["variant"].as_str(),
                        Some("InvalidAudioLimits" | "AudioRangeOutOfRange")
                    )
            })
        {
            let mut admission = Policy::<true>::new(0);
            let result = admitted(
                parent,
                samples(&row["request"]["samples"]),
                limits(&row["request"]["limits"]),
                &mut admission,
            );
            match result {
                Ok(_) => panic!("invalid fade request succeeded"),
                Err(error) => assert_eq!(oracle::result_snapshot(&Err(error)), row["output"]),
            }
            let state = admission.0.borrow();
            assert_eq!(
                (
                    state.forwarded,
                    state.work_calls,
                    state.reservations,
                    state.live
                ),
                (0, 0, 0, 0)
            );
            checked += 1;
        }
        let start = samples(&full(case)["request"]["samples"]).start;
        let mut admission = Policy::<true>::new(0);
        let value = admitted(
            parent,
            start..start,
            AudioQueryLimits {
                maximum_work: 1,
                maximum_spans: 1,
            },
            &mut admission,
        )
        .unwrap();
        assert_eq!(
            (
                value.work(),
                value.canonical_units(),
                value.additional_units()
            ),
            (0, 0, 0)
        );
        drop(value);
        let state = admission.0.borrow();
        assert_eq!(
            (
                state.work_calls,
                state.reservations,
                state.forwarded,
                state.live
            ),
            (0, 0, 0, 0)
        );
    });
    assert_eq!(checked, 960);
}

#[test]
fn full_fade_query_outlives_the_short_constructor_admission_borrow() {
    let mut count = 0;
    visit(|parent, case| {
        let row = full(case);
        let (value, state) = {
            let mut admission = Policy::<true>::new(usize::MAX);
            let state = Rc::clone(&admission.0);
            let value = admitted(
                parent,
                samples(&row["request"]["samples"]),
                Default::default(),
                &mut admission,
            )
            .unwrap();
            (value, state)
        };
        assert!(state.borrow().live > 0);
        let observed = oracle::query_snapshot(value.query());
        let old = &row["output"]["query"];
        assert_eq!(observed.as_object().unwrap().len(), 3);
        assert_eq!(observed["spans"], old["spans"]);
        assert_eq!(observed["lookup"], old["lookup"]);
        assert_eq!(
            value.canonical_units(),
            old["work"].as_u64().unwrap() as usize
        );
        assert_eq!(observed["work"].as_u64().unwrap() as usize, value.work());
        assert_eq!(
            value.work(),
            value.canonical_units() + value.additional_units()
        );
        assert!(value.was_forwarded());
        assert_eq!(state.borrow().forwarded, value.work());
        drop(value);
        assert_eq!(state.borrow().live, 0);
        count += 1;
    });
    assert_eq!(count, 80);
}

struct Observer<'a> {
    policy: &'a mut Policy<true>,
    spends: Vec<usize>,
    reservations: Vec<usize>,
    reservation_layouts: Vec<(usize, usize)>,
    merges: Vec<usize>,
}

impl<'a> Observer<'a> {
    fn new(policy: &'a mut Policy<true>) -> Self {
        Self {
            policy,
            spends: Vec::new(),
            reservations: Vec::new(),
            reservation_layouts: Vec::new(),
            merges: Vec::new(),
        }
    }
}

impl AudioSoundContextAdmission for Observer<'_> {
    type Error = PlanError;
    type Issuer = usize;
    type Lease = Lease;

    fn issuer(&self) -> usize {
        self.policy.issuer()
    }

    fn spend(&mut self, units: usize) -> Result<(), PlanError> {
        self.policy.spend(units)?;
        self.spends.push(units);
        Ok(())
    }

    fn reserve(&mut self, layout: Layout) -> Result<Lease, PlanError> {
        let before = self.policy.0.borrow().live;
        let lease = self.policy.reserve(layout)?;
        self.reservations.push(self.policy.0.borrow().forwarded);
        self.reservation_layouts.push((before, layout.size()));
        Ok(lease)
    }

    fn merge(&mut self, target: &mut Lease, source: &mut Lease) {
        self.merges.push(self.policy.0.borrow().forwarded);
        self.policy.merge(target, source);
    }
}

fn focused(case: &Value) -> bool {
    matches!(
        case["name"].as_str().unwrap(),
        "processing_fixture/leaf/source_fit_beat/root"
            | "fade/translation/moved_bound"
            | "fade/opaque_preserve/unbound"
            | "fade/point/selected_origin_current_crop"
            | "fade/editorial/exact_hard_true"
            | "fade/gap_bound/domain"
    )
}

#[test]
fn actual_fade_callback_refusal_cancellation_and_rotation_preserve_every_accepted_prefix() {
    let mut checked = 0;
    visit(|parent, case| {
        if !focused(case) {
            return;
        }
        let range = samples(&full(case)["request"]["samples"]);
        let mut baseline = Policy::<true>::new(usize::MAX);
        let mut observer = Observer::new(&mut baseline);
        let value = admitted(parent, range.clone(), Default::default(), &mut observer).unwrap();
        let total = value.work();
        let spends = observer.spends.clone();
        let reservations = observer.reservations.clone();
        let merges = observer.merges.clone();
        assert_eq!(spends.iter().sum::<usize>(), total);
        assert!(!spends.is_empty() && !reservations.is_empty());
        drop(value);
        assert_eq!(observer.policy.0.borrow().live, 0);
        for (index, accepted) in reservations.iter().copied().enumerate() {
            let mut refusing = Policy::<true>::new(usize::MAX);
            refusing.0.borrow_mut().refuse_reservation = Some(index + 1);
            assert!(matches!(
                admitted(parent, range.clone(), Default::default(), &mut refusing),
                Err(PlanError::AudioQueryLimit("test wide reservation refusal"))
            ));
            assert_eq!(refusing.0.borrow().reservations, index + 1);
            assert_eq!(refusing.0.borrow().forwarded, accepted);
            assert_eq!(refusing.0.borrow().live, 0);
        }
        let mut prefix = 0;
        let mut cancellation = std::collections::BTreeSet::new();
        let mut after_spends = Vec::new();
        for (index, units) in spends.iter().copied().enumerate() {
            let mut refusing = Policy::<true>::new(usize::MAX);
            refusing.0.borrow_mut().refuse_work_call = Some(index + 1);
            assert!(matches!(
                admitted(parent, range.clone(), Default::default(), &mut refusing),
                Err(PlanError::AudioQueryLimit("test wide work refusal"))
            ));
            assert_eq!(refusing.0.borrow().work_calls, index + 1);
            assert_eq!(refusing.0.borrow().forwarded, prefix);
            assert_eq!(refusing.0.borrow().live, 0);
            cancellation.insert(prefix);
            prefix += units;
            after_spends.push(prefix);
        }
        assert_eq!(prefix, total);
        for accepted in cancellation {
            let mut cancelled = Policy::<true>::new(usize::MAX);
            cancelled.0.borrow_mut().cancel_after = Some(accepted);
            assert!(matches!(
                admitted(parent, range.clone(), Default::default(), &mut cancelled),
                Err(PlanError::AudioQueryLimit("test wide cancellation"))
            ));
            assert_eq!(cancelled.0.borrow().forwarded, accepted);
            assert_eq!(cancelled.0.borrow().live, 0);
        }
        for (reservation, accepted_prefixes) in [(false, after_spends), (true, reservations)] {
            for (index, accepted) in accepted_prefixes.into_iter().enumerate() {
                let mut rotating = Policy::<true>::new(usize::MAX);
                let previous = Rc::clone(&rotating.0);
                if reservation {
                    rotating.0.borrow_mut().rotate_reservation = Some(index + 1);
                } else {
                    rotating.0.borrow_mut().rotate_work_call = Some(index + 1);
                }
                assert!(matches!(
                    admitted(parent, range.clone(), Default::default(), &mut rotating),
                    Err(PlanError::AudioMetadataIssuerMismatch)
                ));
                assert!(!Rc::ptr_eq(&previous, &rotating.0));
                assert_eq!(previous.borrow().forwarded, accepted);
                assert_eq!(previous.borrow().live, 0);
                assert_eq!(rotating.0.borrow().forwarded, 0);
                assert_eq!(rotating.0.borrow().live, 0);
            }
        }
        assert!(!merges.is_empty());
        for (index, accepted) in merges.into_iter().enumerate() {
            let mut policy = Policy::<true>::new(usize::MAX);
            let previous = Rc::clone(&policy.0);
            let mut rotating = MergeRotating {
                policy: &mut policy,
                at: index + 1,
                seen: 0,
            };
            assert!(matches!(
                admitted(parent, range.clone(), Default::default(), &mut rotating),
                Err(PlanError::AudioMetadataIssuerMismatch)
            ));
            assert_eq!(rotating.seen, index + 1);
            assert!(!Rc::ptr_eq(&previous, &policy.0));
            assert_eq!(previous.borrow().forwarded, accepted);
            assert_eq!(previous.borrow().live, 0);
            assert_eq!(policy.0.borrow().forwarded, 0);
            assert_eq!(policy.0.borrow().live, 0);
        }
        checked += 1;
    });
    assert_eq!(checked, 6);
}

struct MergeRotating<'a> {
    policy: &'a mut Policy<true>,
    at: usize,
    seen: usize,
}

impl AudioSoundContextAdmission for MergeRotating<'_> {
    type Error = PlanError;
    type Issuer = usize;
    type Lease = Lease;

    fn issuer(&self) -> usize {
        self.policy.issuer()
    }

    fn spend(&mut self, units: usize) -> Result<(), PlanError> {
        self.policy.spend(units)
    }

    fn reserve(&mut self, layout: Layout) -> Result<Lease, PlanError> {
        self.policy.reserve(layout)
    }

    fn merge(&mut self, target: &mut Lease, source: &mut Lease) {
        self.policy.merge(target, source);
        self.seen += 1;
        if self.seen == self.at {
            self.policy.rotate(usize::MAX);
        }
    }
}

#[path = "fade_query_resource_tests.rs"]
mod resources;
