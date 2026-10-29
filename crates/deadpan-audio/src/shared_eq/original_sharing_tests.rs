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
            return Err(StageAudioError::Limit("test Original metadata bytes"));
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
fn actual_probe_and_return_sites_preserve_work_and_exact_peak_refusal() {
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
        // Independent legacy probe amount separates unchanged deferred owner
        // work from the P now forwarded by the actual admitted constructor.
        let old_probe_work = input
            .retained_probe_at(AudioSample(128), control.query_limits().unwrap())
            .unwrap()
            .work();
        let deferred_owner_work = old_work
            .borrow()
            .plan_work
            .checked_sub(old_probe_work)
            .unwrap();
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
            deferred_owner_work + a.bytes.borrow().inspections
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
                .extend(Layout::new::<
                    Option<deadpan_plan::AudioOwnerRetainedInput<'_>>,
                >())
                .unwrap()
                .0
                .pad_to_align();
            assert!(a.bytes.borrow().calls > 1, "actual probe and final shell");
            assert!(a.bytes.borrow().live >= layout.size());
            // Private Plan backing-Arc tests observe the actual opaque value's
            // destruction before Core/Plan leases; no public weak escape.
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
fn actual_numerical_query_capture_probe_and_share_keep_one_exact_peak_budget() {
    let plan = retained_plan();
    let at = AudioSample(128);
    let query = plan
        .audio_gain_owners(at..AudioSample(129), Default::default())
        .unwrap();
    let owner = query.spans()[0]
        .owners()
        .iter()
        .find(|owner| owner.instance().node.as_str() == "tone")
        .unwrap();
    let key = OwnerKey::new(owner);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = fixture::control(&cancelled, &work);
    let mut ample = policy(usize::MAX, control);
    let result = original_context_admitted(
        &plan,
        &Context::Root,
        &key,
        at..AudioSample(129),
        at,
        control,
        &mut ample,
    )
    .unwrap()
    .unwrap();
    let original = result.carrier.as_ref().unwrap();
    assert!(original.retained().is_some());
    assert!(original.leases()[1].is_some(), "actual admitted Core graph");
    let peak = ample.bytes.borrow().peak;
    let live = ample.bytes.borrow().live;
    assert!(live > 0 && peak > live);
    let spent = work.borrow().plan_work;
    drop(result);
    assert_eq!(ample.bytes.borrow().live, 0);
    assert_eq!(work.borrow().plan_work, spent);

    for cap in [peak - 1, peak] {
        let work = RefCell::new(ReadWork::default());
        let control = fixture::control(&cancelled, &work);
        let mut bounded = policy(cap, control);
        let result = original_context_admitted(
            &plan,
            &Context::Root,
            &key,
            at..AudioSample(129),
            at,
            control,
            &mut bounded,
        );
        if cap == peak {
            assert!(result.as_ref().unwrap().is_some());
            assert_eq!(bounded.bytes.borrow().peak, peak);
            assert_eq!(work.borrow().plan_work, spent);
        } else {
            assert!(matches!(
                result,
                Err(StageAudioError::Limit("test Original metadata bytes"))
            ));
        }
        let accepted = work.borrow().plan_work;
        assert!(accepted > 0 && accepted <= spent);
        drop(result);
        assert_eq!(bounded.bytes.borrow().live, 0);
        assert_eq!(work.borrow().plan_work, accepted);
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

#[test]
fn retained_core_and_sound_charges_survive_real_graph_reads_and_failed_admission() {
    let treatments = AudioTreatments::default()
        .with_equalizer(Some(
            EqualizerRecipe::new(
                [3000, -6000, 1500].map(|gain| GainDb::new(gain).unwrap()),
                None,
                None,
            )
            .unwrap(),
        ))
        .unwrap();
    let plan = retained_plan_with_treatments(Some(treatments));
    let query = plan
        .audio_gain_owners(AudioSample(128)..AudioSample(129), Default::default())
        .unwrap();
    let owner = query.spans()[0]
        .owners()
        .iter()
        .find(|owner| owner.instance().node.as_str() == "tone")
        .unwrap();
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = fixture::control(&cancelled, &work);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let incoming = Contexts {
        original: Context::Root,
        sounds: Sounds::None,
    };
    {
        let mut admission = sound_context_admission::Admission::new(&mut renderer, control);
        let description = describe_admitted(
            &plan,
            request(
                &incoming,
                owner,
                AudioSample(128),
                Temporal::Equalizer(0),
                false,
            ),
            control,
            &mut admission,
        )
        .unwrap();
        let original = description.original.as_ref().unwrap();
        assert!(original.retained().is_some());
        let [plan_lease, core, buffer] = original.leases();
        assert!(plan_lease.is_some() && core.is_some());
        let sound = description.sound.as_ref().unwrap().lease();
        assert!(sound.is_some());
        let total = [plan_lease, core, buffer, sound]
            .into_iter()
            .flatten()
            .map(sound_context_admission::Lease::frames)
            .sum::<u64>();
        assert_eq!(work.borrow().sound_context_frames, total);
        drop(description);
        assert_eq!(work.borrow().sound_context_frames, 0);
    }
    // This path creates a real native EQ and must run in the optimized Audio
    // qualification profile, with its ordinary preparation deadline.
    let mut provider = fixture::NoSources(0);
    let observed = renderer
        .read_shared_input(
            &mut provider,
            AudioSample(128),
            32,
            control,
            InputSelection::default(),
        )
        .unwrap();
    assert!(
        observed
            .samples
            .iter()
            .flatten()
            .any(|sample| *sample != 0.0)
    );
    assert_eq!(provider.0, 0);
    assert_eq!(renderer.active_frames, 0);
    assert_eq!(work.borrow().sound_context_frames, 0);
    let mut refused_renderer = StageAudio::new(Arc::clone(&plan));
    refused_renderer.limits.maximum_resident_frames = u32::try_from(
        crate::Equalizer::preparation_bytes_bound().div_ceil(std::mem::size_of::<[f32; 2]>()),
    )
    .unwrap();
    let refused = refused_renderer.read_shared_input(
        &mut provider,
        AudioSample(128),
        32,
        control,
        InputSelection::default(),
    );
    assert!(refused.is_err());
    assert_eq!(refused_renderer.active_frames, 0);
    assert_eq!(work.borrow().sound_context_frames, 0);
}
