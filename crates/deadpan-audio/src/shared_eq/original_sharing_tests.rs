use super::*;
use deadpan_plan::AudioSoundContextAdmission;
use std::{alloc::Layout, rc::Rc, sync::Weak};

#[derive(Default)]
struct Bytes<'plan> {
    cap: usize,
    live: usize,
    peak: usize,
    calls: usize,
    inspections: usize,
    domain: Option<Weak<AudioDomain<'plan>>>,
    retained: Option<Weak<deadpan_plan::AudioOwnerRetainedInput<'plan>>>,
}
struct Policy<'plan, 'work> {
    bytes: Rc<RefCell<Bytes<'plan>>>,
    control: WorkControl<'work>,
}
struct Token<'plan> {
    bytes: Rc<RefCell<Bytes<'plan>>>,
    amount: usize,
}
impl Drop for Token<'_> {
    fn drop(&mut self) {
        if self.amount == 0 {
            return;
        }
        let mut b = self.bytes.borrow_mut();
        if let Some(value) = &b.domain {
            assert!(value.upgrade().is_none());
        }
        if let Some(value) = &b.retained {
            assert!(value.upgrade().is_none());
        }
        b.live -= self.amount;
    }
}
impl<'plan> AudioSoundContextAdmission for Policy<'plan, '_> {
    type Error = StageAudioError;
    type Issuer = usize;
    type Lease = Token<'plan>;
    fn issuer(&self) -> usize {
        Rc::as_ptr(&self.bytes) as usize
    }
    fn spend(&mut self, n: usize) -> Result<(), Self::Error> {
        self.control.spend_plan_work(n)?;
        self.bytes.borrow_mut().inspections += n;
        Ok(())
    }
    fn reserve(&mut self, layout: Layout) -> Result<Token<'plan>, Self::Error> {
        self.control.check()?;
        let mut b = self.bytes.borrow_mut();
        b.calls += 1;
        let next = b.live.checked_add(layout.size()).unwrap();
        if next > b.cap {
            return Err(StageAudioError::Limit("test final-sharing bytes"));
        }
        b.live = next;
        b.peak = b.peak.max(next);
        Ok(Token {
            bytes: Rc::clone(&self.bytes),
            amount: layout.size(),
        })
    }
    fn merge(&mut self, a: &mut Self::Lease, b: &mut Self::Lease) {
        a.amount += b.amount;
        b.amount = 0;
    }
}
fn policy<'plan, 'work>(cap: usize, control: WorkControl<'work>) -> Policy<'plan, 'work> {
    Policy {
        bytes: Rc::new(RefCell::new(Bytes {
            cap,
            ..Default::default()
        })),
        control,
    }
}
#[test]
fn actual_domain_and_accepted_retained_return_sites_admit_exact_bytes_and_keep_queries_once() {
    for retained in [false, true] {
        let plan = if retained {
            retained_plan()
        } else {
            fixture::fixture()
        };
        let query = plan
            .audio_gain_owners(AudioSample(128)..AudioSample(129), Default::default())
            .unwrap();
        let owner = query.spans()[0]
            .owners()
            .iter()
            .find(|owner| owner.instance().node.as_str() == if retained { "tone" } else { "root" })
            .unwrap();
        let input = plan
            .audio_owner_input(owner, AudioSample(128), Default::default())
            .unwrap();
        let cancelled = AtomicBool::new(false);
        let old_work = RefCell::new(ReadWork::default());
        let old = Context::finish_paid_owner_input(
            &input,
            owner,
            AudioSample(128),
            fixture::control(&cancelled, &old_work),
        )
        .unwrap();
        assert_eq!(matches!(old, Context::Retained(_)), retained);
        let work = RefCell::new(ReadWork::default());
        let control = fixture::control(&cancelled, &work);
        let mut a = policy(usize::MAX, control);
        let carrier = Context::finish_paid_owner_input_admitted(
            &input,
            owner,
            AudioSample(128),
            control,
            &mut a,
        )
        .unwrap();
        assert_eq!(carrier.retained().is_some(), retained);
        assert_eq!(
            work.borrow().plan_work,
            old_work.borrow().plan_work + a.bytes.borrow().inspections
        );
        if let Some(domain) = carrier.domain() {
            let Context::Domain(old) = &old else {
                panic!("Domain branch");
            };
            assert_eq!(domain.root_samples(), old.root_samples());
            assert_eq!(domain.visible_samples(), old.visible_samples());
            assert_eq!(domain.instance(), old.instance());
            a.bytes.borrow_mut().domain = Some(Arc::downgrade(domain));
        } else {
            let value = carrier.retained().unwrap();
            let Context::Retained(old) = &old else {
                panic!("Retained branch");
            };
            assert_eq!(
                value.reference_at(AudioSample(128)).unwrap(),
                old.reference_at(AudioSample(128)).unwrap()
            );
            assert_eq!(
                value.reference_samples_per_output_sample(),
                old.reference_samples_per_output_sample()
            );
            let before = value
                .owners(AudioSample(128)..AudioSample(129), Default::default())
                .unwrap();
            let expected = old
                .owners(AudioSample(128)..AudioSample(129), Default::default())
                .unwrap();
            assert_eq!(before.work(), expected.work());
            assert_eq!(
                before.spans()[0].owners().len(),
                expected.spans()[0].owners().len()
            );
            let layout = Layout::new::<[usize; 2]>()
                .extend(Layout::new::<deadpan_plan::AudioOwnerRetainedInput<'_>>())
                .unwrap()
                .0
                .pad_to_align();
            assert_eq!(
                a.bytes.borrow().calls,
                1,
                "only the moved retained Arc shell"
            );
            assert_eq!(a.bytes.borrow().live, layout.size());
            a.bytes.borrow_mut().retained = Some(Arc::downgrade(value));
        }
        let peak = a.bytes.borrow().peak;
        assert!(peak > 0);
        drop(carrier);
        assert_eq!(a.bytes.borrow().live, 0);
        for cap in [peak - 1, peak] {
            let work = RefCell::new(ReadWork::default());
            let control = fixture::control(&cancelled, &work);
            let mut a = policy(cap, control);
            let result = Context::finish_paid_owner_input_admitted(
                &input,
                owner,
                AudioSample(128),
                control,
                &mut a,
            );
            assert_eq!(result.is_ok(), cap == peak);
            drop(result);
            assert_eq!(a.bytes.borrow().live, 0);
        }
    }
}
#[test]
fn description_retains_both_charges_through_boundary_failure_cancel_and_absent_sides() {
    let plan = fixture::fixture();
    let owner = fixture::owner(&plan);
    let incoming = Contexts {
        original: Context::Root,
        sounds: Sounds::None,
    };
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = fixture::control(&cancelled, &work);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    {
        let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
        let description = describe_admitted(
            &plan,
            request(
                &incoming,
                &owner,
                AudioSample(128),
                Temporal::Wet(TailEffect::Delay),
                false,
            ),
            control,
            &mut admission,
        )
        .unwrap();
        assert!(description.original.is_some() && description.sound.is_some());
        let total = description
            .original
            .as_ref()
            .unwrap()
            .lease()
            .unwrap()
            .frames()
            + description
                .sound
                .as_ref()
                .unwrap()
                .lease()
                .unwrap()
                .frames();
        assert_eq!(work.borrow().sound_context_frames, total);
        drop(description);
        assert_eq!(work.borrow().sound_context_frames, 0);
        let fallback = describe_admitted(
            &plan,
            request(
                &incoming,
                &owner,
                AudioSample(600),
                Temporal::Wet(TailEffect::Delay),
                false,
            ),
            control,
            &mut admission,
        )
        .unwrap();
        assert!(fallback.original.is_none() && fallback.sound.is_some());
        drop(fallback);
        assert_eq!(work.borrow().sound_context_frames, 0);
        let mut invalid = request(
            &incoming,
            &owner,
            AudioSample(128),
            Temporal::Wet(TailEffect::Delay),
            false,
        );
        invalid.stage = usize::MAX;
        assert!(describe_admitted(&plan, invalid, control, &mut admission).is_err());
        assert_eq!(work.borrow().sound_context_frames, 0);
        cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(
            describe_admitted(
                &plan,
                request(
                    &incoming,
                    &owner,
                    AudioSample(128),
                    Temporal::Wet(TailEffect::Delay),
                    false
                ),
                control,
                &mut admission
            )
            .is_err()
        );
        assert_eq!(work.borrow().sound_context_frames, 0);
    }
    assert_eq!(renderer.active_frames, 0);
}
