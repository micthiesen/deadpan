//! Real checked clocks exercise the same admitted boundary copy as the caller.
use super::*;
use crate::{AdmittedAudioTreatmentBoundary, AudioSoundContextAdmission};
use deadpan_core::{AudioTreatments, ClipGain, GainDb, NodeId, ProjectDocument, RevisionId};
use serde_json::Value;
use std::{alloc::Layout, cell::RefCell, rc::Rc};

#[derive(Default)]
struct Ledger {
    live: usize,
    peak: usize,
    calls: usize,
    work: usize,
    cap: usize,
    fail_reserve: Option<usize>,
    fail_work: Option<usize>,
    rotate: Option<&'static str>,
    issuer: usize,
}
struct Policy<const M: bool>(Rc<RefCell<Ledger>>);
struct Lease {
    ledger: Rc<RefCell<Ledger>>,
    bytes: usize,
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.ledger.borrow_mut().live -= self.bytes;
    }
}
impl<const M: bool> AudioSoundContextAdmission for Policy<M> {
    const METERED: bool = M;
    type Error = PlanError;
    type Issuer = usize;
    type Lease = Lease;

    fn issuer(&self) -> usize {
        self.0.borrow().issuer
    }
    fn spend(&mut self, n: usize) -> Result<(), PlanError> {
        assert!(M, "false mode must not spend");
        let mut v = self.0.borrow_mut();
        if v.fail_work.is_some_and(|limit| v.work + n > limit) {
            return Err(PlanError::AudioQueryLimit("boundary test work"));
        }
        v.work += n;
        if v.rotate == Some("spend") {
            v.issuer += 1;
        }
        Ok(())
    }
    fn reserve(&mut self, layout: Layout) -> Result<Lease, PlanError> {
        assert!(M, "false mode must not reserve");
        let mut v = self.0.borrow_mut();
        v.calls += 1;
        if v.fail_reserve == Some(v.calls) || v.live + layout.size() > v.cap {
            return Err(PlanError::AudioQueryLimit("boundary test memory"));
        }
        v.live += layout.size();
        v.peak = v.peak.max(v.live);
        if v.rotate == Some("reserve") {
            v.issuer += 1;
        }
        Ok(Lease {
            ledger: Rc::clone(&self.0),
            bytes: layout.size(),
        })
    }
    fn merge(&mut self, target: &mut Lease, source: &mut Lease) {
        assert!(M, "false mode must not merge");
        assert!(Rc::ptr_eq(&target.ledger, &self.0));
        assert!(Rc::ptr_eq(&source.ledger, &self.0));
        target.bytes += source.bytes;
        source.bytes = 0;
        let mut v = self.0.borrow_mut();
        if v.rotate == Some("merge") {
            v.issuer += 1;
        }
    }
}
fn policy<const M: bool>(cap: usize) -> Policy<M> {
    Policy(Rc::new(RefCell::new(Ledger {
        cap,
        issuer: 17,
        ..Default::default()
    })))
}
fn fixtures() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/owner-query-baseline-v1/snapshot.json"
    ))
    .unwrap()
}
fn same_owner(a: &AudioOwnerClock<'_>, b: &AudioOwnerClock<'_>) {
    let AudioOwnerClock {
        plan,
        definition,
        instance,
        kind,
        gap_after,
        play,
        sampling,
        origin,
    } = a;
    assert!(std::ptr::eq(*plan, b.plan));
    assert_eq!(definition, &b.definition);
    assert_eq!(instance, &b.instance);
    assert_eq!(kind, &b.kind);
    assert_eq!(gap_after, &b.gap_after);
    assert_eq!(play, &b.play);
    assert_eq!(sampling.anchor(), b.sampling.anchor());
    assert_eq!(sampling.local_at_anchor(), b.sampling.local_at_anchor());
    assert_eq!(
        sampling.local_frames_per_sample(),
        b.sampling.local_frames_per_sample()
    );
    assert_eq!(origin, &b.origin);
}
fn check_value<const M: bool>(
    owner: &AudioOwnerClock<'_>,
    at: AudioSample,
    result: &AdmittedAudioTreatmentBoundary<'_, usize, Lease>,
) {
    let old = owner.treatment_boundary(0).unwrap();
    same_owner(old.owner(), result.owner());
    assert_eq!(old.before_stage(), result.before_stage());
    assert_eq!(old.matches(owner), result.matches(owner));
    assert_eq!(
        old.matches_clock_at(owner, at),
        result.matches_clock_at(owner, at)
    );
    assert_eq!(result.issuer(), 17);
    assert_eq!(result.canonical_units(), 0);
    assert_eq!(result.work(), result.additional_units());
    assert_eq!(result.was_forwarded(), M);
}
fn check_copy(owner: &AudioOwnerClock<'_>, at: AudioSample) {
    let mut false_policy = policy::<false>(0);
    let plain = owner
        .treatment_boundary_admitted(
            0,
            AudioQueryLimits {
                maximum_work: 1,
                ..Default::default()
            },
            &mut false_policy,
        )
        .unwrap();
    check_value::<false>(owner, at, &plain);
    assert_eq!(plain.work(), 0);
    assert!(plain.lease().is_none());
    drop(plain);
    assert_eq!(false_policy.0.borrow().work, 0);
    assert_eq!(false_policy.0.borrow().calls, 0);

    let mut p = policy::<true>(usize::MAX);
    let mut value = owner
        .treatment_boundary_admitted(0, Default::default(), &mut p)
        .unwrap();
    check_value::<true>(owner, at, &value);
    let work = value.work();
    let peak = p.0.borrow().peak;
    let calls = p.0.borrow().calls;
    assert!(work > 1 && peak > 0 && calls > 0);
    assert_eq!(p.0.borrow().work, work);
    assert_eq!(p.0.borrow().live, peak);
    let original_lease = std::ptr::from_ref(value.lease().unwrap());
    value.with_lease_mut(|lease| {
        assert_eq!(
            std::ptr::from_mut(lease.unwrap()).cast_const(),
            original_lease
        );
    });
    drop(value);
    assert_eq!(p.0.borrow().live, 0);
    assert_eq!(
        p.0.borrow().work,
        work,
        "dropping memory never refunds work"
    );

    let limits = AudioQueryLimits {
        maximum_work: work,
        ..Default::default()
    };
    let mut exact = policy::<true>(peak);
    let value = owner
        .treatment_boundary_admitted(0, limits, &mut exact)
        .unwrap();
    assert_eq!(exact.0.borrow().peak, peak);
    drop(value);
    assert_eq!(exact.0.borrow().live, 0);

    let mut short = policy::<true>(peak - 1);
    assert!(matches!(
        owner.treatment_boundary_admitted(0, limits, &mut short),
        Err(PlanError::AudioQueryLimit("boundary test memory"))
    ));
    assert_eq!(short.0.borrow().live, 0);
    let mut short = policy::<true>(usize::MAX);
    assert!(matches!(
        owner.treatment_boundary_admitted(
            0,
            AudioQueryLimits {
                maximum_work: work - 1,
                ..Default::default()
            },
            &mut short
        ),
        Err(PlanError::AudioQueryLimit("structural work"))
    ));
    assert_eq!(short.0.borrow().live, 0);
    assert!(short.0.borrow().work > 0 && short.0.borrow().work < work);
    for fail in 1..=calls {
        let mut refusing = policy::<true>(usize::MAX);
        refusing.0.borrow_mut().fail_reserve = Some(fail);
        assert!(matches!(
            owner.treatment_boundary_admitted(0, Default::default(), &mut refusing),
            Err(PlanError::AudioQueryLimit("boundary test memory"))
        ));
        assert_eq!(refusing.0.borrow().live, 0, "reservation {fail}");
        assert!(refusing.0.borrow().work > 0);
    }
    for fail in 0..work {
        let mut refusing = policy::<true>(usize::MAX);
        refusing.0.borrow_mut().fail_work = Some(fail);
        assert!(matches!(
            owner.treatment_boundary_admitted(0, Default::default(), &mut refusing),
            Err(PlanError::AudioQueryLimit("boundary test work"))
        ));
        assert_eq!(refusing.0.borrow().live, 0, "work {fail}");
        assert_eq!(refusing.0.borrow().work, fail);
    }
    for point in ["spend", "reserve", "merge"] {
        if point == "merge" && calls < 2 {
            continue;
        }
        let mut wrong = policy::<true>(usize::MAX);
        wrong.0.borrow_mut().rotate = Some(point);
        assert!(matches!(
            owner.treatment_boundary_admitted(0, Default::default(), &mut wrong),
            Err(PlanError::AudioMetadataIssuerMismatch)
        ));
        assert_eq!(wrong.0.borrow().live, 0, "wrong issuer after {point}");
    }
}

#[test]
fn actual_boundary_normal_retained_repeat_and_gap_copies_both_modes() {
    let all = fixtures();
    let mut retained = 0;
    let mut repeated = 0;
    let mut gaps = 0;
    let mut normal = 0;
    for fixture in all["fixtures"].as_array().unwrap() {
        let doc = ProjectDocument::from_json(&fixture["document"].to_string()).unwrap();
        let plan = RenderPlan::compile(&doc).unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            let at = AudioSample(case["observation_window"]["start"].as_i64().unwrap());
            let query = plan
                .audio_gain_owners(at..AudioSample(at.0 + 1), Default::default())
                .unwrap();
            for owner in query.spans().iter().flat_map(|v| v.owners()) {
                check_copy(owner, at);
                retained += usize::from(matches!(
                    owner.origin,
                    AudioOwnerClockOrigin::Retained { .. }
                ));
                repeated += usize::from(!owner.instance.repeats.is_empty());
                gaps += usize::from(owner.kind == AudioOwnerKind::DefaultGap);
                normal += usize::from(matches!(owner.origin, AudioOwnerClockOrigin::Current));
            }
        }
    }
    assert!(retained > 0 && repeated > 0 && gaps > 0 && normal > 0);
}

#[test]
fn stage_error_precedes_invalid_limits_and_all_policy_activity() {
    let mut fixture = fixtures()["fixtures"][0]["document"].clone();
    let treatment = AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(3000).unwrap(), false, vec![], vec![]).unwrap(),
    );
    fixture["nodes"]["root"]["audio_treatments"] = serde_json::to_value(treatment).unwrap();
    let doc = ProjectDocument::from_json(&fixture.to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    let query = plan
        .audio_gain_owners(AudioSample(0)..AudioSample(1), Default::default())
        .unwrap();
    let owner = &query.spans()[0].owners()[0];
    let end = owner.treatments().unwrap().order().len();
    assert!(end > 0);
    let invalid = AudioQueryLimits {
        maximum_spans: 0,
        maximum_work: 0,
    };
    let mut p = policy::<true>(0);
    assert!(matches!(
        owner.treatment_boundary(end + 1),
        Err(PlanError::InvalidPlan(
            "audio treatment boundary out of range"
        ))
    ));
    assert!(matches!(
        owner.treatment_boundary_admitted(end + 1, invalid, &mut p),
        Err(PlanError::InvalidPlan(
            "audio treatment boundary out of range"
        ))
    ));
    assert!(matches!(
        owner.treatment_boundary_admitted(end, invalid, &mut p),
        Err(PlanError::InvalidAudioLimits)
    ));
    assert_eq!(p.0.borrow().work, 0);
    assert_eq!(p.0.borrow().calls, 0);
    for stage in 0..=end {
        let mut p = policy::<true>(usize::MAX);
        let admitted = owner
            .treatment_boundary_admitted(stage, Default::default(), &mut p)
            .unwrap();
        assert_eq!(
            admitted.before_stage(),
            owner.treatment_boundary(stage).unwrap().before_stage()
        );
    }
}

fn identity_rejected(base: &AudioOwnerClock<'_>, candidate: &AudioOwnerClock<'_>) {
    assert!(!base.matches_treatment_owner(candidate));
    assert!(!base.treatment_boundary(0).unwrap().matches(candidate));
}

#[test]
fn borrowed_identity_isolates_every_old_field_and_ignores_play_and_sampling() {
    let doc =
        ProjectDocument::from_json(&fixtures()["fixtures"][0]["document"].to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    let foreign = RenderPlan::compile(&doc).unwrap();
    let query = plan
        .audio_gain_owners(AudioSample(0)..AudioSample(1), Default::default())
        .unwrap();
    let base = &query.spans()[0].owners()[0];
    let mut candidate = base.clone();
    candidate.plan = &foreign;
    identity_rejected(base, &candidate);
    let mut candidate = base.clone();
    candidate.definition = Some(AudioDefinitionSelector::Node {
        node: NodeId::new("different").unwrap(),
    });
    identity_rejected(base, &candidate);
    let mut candidate = base.clone();
    candidate.instance.node = NodeId::new("different").unwrap();
    identity_rejected(base, &candidate);
    let iteration = IterationId {
        allocation: RevisionId::new("other").unwrap(),
        ordinal: 2,
    };
    let mut candidate = base.clone();
    candidate.instance.repeats.push(RepeatInstance {
        node: NodeId::new("repeat").unwrap(),
        iteration: iteration.clone(),
    });
    identity_rejected(base, &candidate);
    let mut candidate = base.clone();
    candidate.kind = AudioOwnerKind::DefaultGap;
    identity_rejected(base, &candidate);
    let mut candidate = base.clone();
    candidate.gap_after = Some(iteration.clone());
    identity_rejected(base, &candidate);
    let mut retained = base.clone();
    retained.origin = AudioOwnerClockOrigin::Retained {
        definition: None,
        instance: base.instance.clone(),
        kind: AudioOwnerKind::Node,
        gap_after: None,
    };
    identity_rejected(base, &retained);
    for field in 0..5 {
        let mut candidate = retained.clone();
        let AudioOwnerClockOrigin::Retained {
            definition,
            instance,
            kind,
            gap_after,
        } = &mut candidate.origin
        else {
            unreachable!()
        };
        match field {
            0 => {
                *definition = Some(AudioDefinitionSelector::Node {
                    node: NodeId::new("different").unwrap(),
                })
            }
            1 => instance.node = NodeId::new("different").unwrap(),
            2 => instance.repeats.push(RepeatInstance {
                node: NodeId::new("repeat").unwrap(),
                iteration: iteration.clone(),
            }),
            3 => *kind = AudioOwnerKind::DefaultGap,
            4 => *gap_after = Some(iteration.clone()),
            _ => unreachable!(),
        }
        identity_rejected(&retained, &candidate);
    }
    let mut candidate = base.clone();
    candidate.play = Some(77);
    assert!(base.matches_treatment_owner(&candidate));
    assert!(
        base.treatment_boundary(0)
            .unwrap()
            .matches_clock_at(&candidate, AudioSample(0))
    );
    candidate.sampling = AudioSampleMap::new(
        AudioSample(0),
        ExactRatio::integer(99),
        ExactRatio::integer(2),
    )
    .unwrap();
    assert!(base.matches_treatment_owner(&candidate));
    assert!(base.treatment_boundary(0).unwrap().matches(&candidate));
    assert!(
        !base
            .treatment_boundary(0)
            .unwrap()
            .matches_clock_at(&candidate, AudioSample(0))
    );
    let mut p = policy::<true>(usize::MAX);
    let admitted = base
        .treatment_boundary_admitted(0, Default::default(), &mut p)
        .unwrap();
    assert!(admitted.matches(&candidate));
    assert!(!admitted.matches_clock_at(&candidate, AudioSample(0)));
}
