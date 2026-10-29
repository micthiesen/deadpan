use super::*;
use deadpan_plan::AudioSoundContextAdmission;
use std::alloc::Layout;

struct RecordingAdmission<'controller, 'work> {
    inner: sound_context_admission::Admission<'controller, 'work>,
    forwarded: usize,
}
impl<'work> AudioSoundContextAdmission for RecordingAdmission<'_, 'work> {
    type Error = StageAudioError;
    type Issuer = sound_context_admission::Issuer<'work>;
    type Lease = sound_context_admission::Lease<'work>;
    fn issuer(&self) -> Self::Issuer {
        self.inner.issuer()
    }
    fn spend(&mut self, amount: usize) -> Result<(), Self::Error> {
        self.forwarded += amount;
        self.inner.spend(amount)
    }
    fn reserve(&mut self, layout: Layout) -> Result<Self::Lease, Self::Error> {
        self.inner.reserve(layout)
    }
    fn merge(&mut self, target: &mut Self::Lease, source: &mut Self::Lease) {
        self.inner.merge(target, source)
    }
}

fn same_observation(plan: &RenderPlan, left: &AudioOwnerQuery<'_>, right: &AudioOwnerQuery<'_>) {
    assert_eq!(left.samples(), right.samples());
    assert_eq!(left.lookup(), right.lookup());
    assert_eq!(left.spans().len(), right.spans().len());
    for (a, b) in left.spans().iter().zip(right.spans()) {
        assert_eq!(a.samples(), b.samples());
        assert_eq!(a.support(), b.support());
        assert_eq!(a.original_is_silent(), b.original_is_silent());
        assert_eq!(a.owners().len(), b.owners().len());
        for (x, y) in a.owners().iter().zip(b.owners()) {
            assert!(x.belongs_to(plan));
            assert!(y.belongs_to(plan));
            assert_eq!(x.instance(), y.instance());
            assert_eq!(x.definition(), y.definition());
            assert_eq!(x.kind(), y.kind());
            assert_eq!(x.gap_after(), y.gap_after());
            assert_eq!(x.sampling(), y.sampling());
            assert_eq!(x.origin(), y.origin());
            assert_eq!(x.escalation_millidecibels(), y.escalation_millidecibels());
            assert_eq!(x.treatments(), y.treatments());
        }
    }
}

#[test]
fn legacy_and_admitted_root_and_domain_queries_match_and_bill_once() {
    let plan = eq_admission_fixture::fixture();
    let owner = eq_admission_fixture::owner(&plan);
    let input = plan
        .audio_owner_input(&owner, AudioSample(128), Default::default())
        .unwrap();
    let domain = Context::Domain(Arc::new(input.context().clone()));
    let cancelled = AtomicBool::new(false);
    let range = AudioSample(128)..AudioSample(129);

    for context in [Context::Root, domain] {
        let old_work = RefCell::new(ReadWork::default());
        let old_control = eq_admission_fixture::control(&cancelled, &old_work);
        let old = context.owners(&plan, range.clone(), old_control).unwrap();
        let canonical_work = old.work();
        assert_eq!(old_work.borrow().plan_work, canonical_work);

        // Unmetered admission preserves the complete legacy public result and
        // charges the same canonical amount through WorkControl.
        let unmetered_work = RefCell::new(ReadWork::default());
        let unmetered_control = eq_admission_fixture::control(&cancelled, &unmetered_work);
        let mut unmetered = sound_context_admission::Unmetered;
        let unmetered_guard = context
            .owners_admitted(&plan, range.clone(), unmetered_control, &mut unmetered)
            .unwrap();
        same_observation(&plan, &old, unmetered_guard.query());
        assert_eq!(unmetered_guard.canonical_units(), canonical_work);
        assert_eq!(unmetered_guard.additional_units(), 0);
        assert_eq!(unmetered_work.borrow().plan_work, canonical_work);
        drop(unmetered_guard);
        assert_eq!(unmetered_work.borrow().sound_context_frames, 0);

        // Metered admission forwards the full raw query work exactly once.
        let work = RefCell::new(ReadWork::default());
        let control = eq_admission_fixture::control(&cancelled, &work);
        let mut renderer = StageAudio::new(Arc::clone(&plan));
        let guard = {
            let mut admission = RecordingAdmission {
                inner: sound_context_admission::Admission::new(&mut renderer, control),
                forwarded: 0,
            };
            let guard = context
                .owners_admitted(&plan, range.clone(), control, &mut admission)
                .unwrap();
            same_observation(&plan, &old, guard.query());
            assert_eq!(guard.canonical_units(), canonical_work);
            assert_eq!(
                guard.canonical_units() + guard.additional_units(),
                guard.work()
            );
            assert!(guard.additional_units() > 0);
            assert_eq!(admission.forwarded, guard.work());
            assert_eq!(work.borrow().plan_work, admission.forwarded);
            guard
        };
        let billed = work.borrow().plan_work;
        assert!(work.borrow().sound_context_frames > 0);
        drop(guard);
        assert_eq!(work.borrow().sound_context_frames, 0);
        assert_eq!(
            work.borrow().plan_work,
            billed,
            "dropping a lease never refunds work"
        );
    }
}
