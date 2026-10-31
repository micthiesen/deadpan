//! The retained Point path makes a separate owning signal copy before recursion.
use super::*;
use crate::plan::audio_owners::admission::{self, Budget as QueryBudget, Owned};
use crate::{AudioDefinitionSelector, AudioPointDomain, AudioRootPlacement};

fn point(plan: &RenderPlan) -> AudioPointDomain<'_> {
    let definition = plan
        .audio_definition(AudioDefinitionSelector::Node {
            node: deadpan_core::NodeId::new("leaf").unwrap(),
        })
        .unwrap();
    let placement = AudioRootPlacement::new(
        ExactRatio::integer(-1),
        ExactRatio::ONE,
        ExactRatio::ZERO..definition.exact_duration(),
    )
    .unwrap();
    let mut result = definition
        .in_point_clock(placement, ExactRatio::ZERO)
        .unwrap();
    result.set_evaluation(
        Some(AudioDefinitionSelector::Node {
            node: deadpan_core::NodeId::new("leaf").unwrap(),
        }),
        vec![deadpan_core::RepeatInstance {
            node: deadpan_core::NodeId::new("repeat").unwrap(),
            iteration: deadpan_core::IterationId {
                allocation: deadpan_core::RevisionId::new("play-allocation").unwrap(),
                ordinal: 0,
            },
        }],
        None,
    );
    result
}

fn cloned<'plan, A: AudioSoundContextAdmission>(
    point: &AudioPointDomain<'plan>,
    policy: &mut A,
) -> Result<Owned<crate::AudioSignal<'plan>, A::Issuer, A::Lease>, A::Error> {
    let mut local = crate::plan::audio::Budget {
        remaining: 65_536,
        lookup: LookupStats::default(),
    };
    let mut budget = QueryBudget::new(&mut local, policy);
    admission::outer::<_, A>(point.signal_admitted(&mut budget))
}

struct MergePolicy {
    inner: Policy<true>,
    merges: usize,
    rotate_at: Option<usize>,
}
impl AudioSoundContextAdmission for MergePolicy {
    const METERED: bool = true;
    type Error = PlanError;
    type Issuer = usize;
    type Lease = Lease;
    fn issuer(&self) -> usize {
        self.inner.issuer()
    }
    fn spend(&mut self, n: usize) -> Result<(), PlanError> {
        self.inner.spend(n)
    }
    fn reserve(&mut self, layout: Layout) -> Result<Lease, PlanError> {
        self.inner.reserve(layout)
    }
    fn merge(&mut self, target: &mut Lease, source: &mut Lease) {
        self.inner.merge(target, source);
        self.merges += 1;
        if self.rotate_at == Some(self.merges) {
            self.inner.rotate(usize::MAX);
        }
    }
}

#[test]
fn point_clone_matches_actual_clone_and_retains_exact_requested_layouts() {
    let plan = crate::audio_allocated::query_storage_tests::fixture();
    let point = point(&plan);
    let old = format!("{:?}", point.signal());
    let mut unmetered = Policy::<false>::new(0);
    let plain = cloned(&point, &mut unmetered).unwrap();
    assert_eq!(format!("{:?}", plain.value), old);
    drop(plain);
    assert_eq!(
        (
            unmetered.0.borrow().forwarded,
            unmetered.0.borrow().reservations
        ),
        (0, 0)
    );
    let mut ample = Policy::<true>::new(usize::MAX);
    let value = cloned(&point, &mut ample).unwrap();
    assert_eq!(format!("{:?}", value.value), old);
    let peak = ample.0.borrow().peak;
    let work = ample.0.borrow().forwarded;
    assert!(peak > 0 && work > 0);
    assert!(ample.0.borrow().live > 0);
    let ledger = Rc::clone(&ample.0);
    drop(ample);
    drop(value);
    assert_eq!(ledger.borrow().live, 0);
    let mut exact = Policy::<true>::new(peak);
    let value = cloned(&point, &mut exact).unwrap();
    assert_eq!(exact.0.borrow().peak, peak);
    assert_eq!(exact.0.borrow().forwarded, work);
    drop(value);
    assert_eq!(exact.0.borrow().live, 0);
    let mut short = Policy::<true>::new(peak - 1);
    assert!(cloned(&point, &mut short).is_err());
    assert_eq!(short.0.borrow().live, 0);
    assert!(short.0.borrow().forwarded > 0);
}

#[test]
fn point_clone_rejects_issuer_rotation_at_every_actual_merge() {
    let plan = crate::audio_allocated::query_storage_tests::fixture();
    let point = point(&plan);
    let mut ample = MergePolicy {
        inner: Policy::new(usize::MAX),
        merges: 0,
        rotate_at: None,
    };
    drop(cloned(&point, &mut ample).unwrap());
    assert!(ample.merges > 0);
    for merge in 1..=ample.merges {
        let mut rotating = MergePolicy {
            inner: Policy::new(usize::MAX),
            merges: 0,
            rotate_at: Some(merge),
        };
        let ledger = Rc::clone(&rotating.inner.0);
        assert!(matches!(
            cloned(&point, &mut rotating),
            Err(PlanError::AudioMetadataIssuerMismatch)
        ));
        assert_eq!(ledger.borrow().live, 0);
        assert_eq!(rotating.inner.0.borrow().live, 0);
    }
}
