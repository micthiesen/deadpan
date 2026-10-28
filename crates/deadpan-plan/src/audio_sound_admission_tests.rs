use super::*;
use deadpan_core::*;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Weak};

fn fixture(preserve: bool) -> RenderPlan {
    let id = |s: &str| NodeId::new(s).unwrap();
    let doc = ProjectDocument::new(
        ProjectId::new("sound-admission").unwrap(),
        RevisionId::new("revision").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(48_000, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let tone = BeatNode::hold(
        "Tone",
        HoldRecipe {
            duration: FrameDuration::new(128).unwrap(),
            picture_context: None,
            video: HoldVideo::Background,
            audio: HoldAudio::Tone {
                frequency_hz: 440,
                level: GainDb::new(-6000).unwrap(),
            },
        },
    );
    let mut nodes = BTreeMap::from([(id("tone"), tone)]);
    let mut repeat = nodes[&id("tone")].clone();
    repeat.kind = NodeKind::Repeat {
        child: id(if preserve { "preserve" } else { "tone" }),
        iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 2).unwrap(),
        gap: None,
        escalation: None,
    };
    if preserve {
        let mut stage = nodes[&id("tone")].clone();
        stage.kind = NodeKind::Retime {
            child: id("tone"),
            duration: FrameDuration::new(64).unwrap(),
            mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(128)).unwrap(),
            pitch: PitchPolicy::Preserve,
            purpose: RetimePurpose::Edit,
        };
        nodes.insert(id("preserve"), stage);
    }
    nodes.insert(id("repeat"), repeat);
    nodes.insert(id("root"), BeatNode::sequence("Root", vec![id("repeat")]));
    let mut wire = serde_json::to_value(doc).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap()
}
#[derive(Default)]
struct State<'plan> {
    live: usize,
    peak: usize,
    cap: usize,
    calls: usize,
    work: usize,
    denied: bool,
    cancel: bool,
    watch: Option<Weak<AudioSoundOwnerContext<'plan>>>,
    domain: Option<Weak<AudioDomain<'plan>>>,
}
struct Policy<'plan>(Rc<RefCell<State<'plan>>>);
struct Token<'plan> {
    state: Rc<RefCell<State<'plan>>>,
    bytes: usize,
}
impl Drop for Token<'_> {
    fn drop(&mut self) {
        if self.bytes == 0 {
            return;
        }
        let mut s = self.state.borrow_mut();
        if let Some(weak) = &s.watch {
            assert!(weak.upgrade().is_none(), "context must drop before charge");
        }
        if let Some(weak) = &s.domain {
            assert!(weak.upgrade().is_none(), "Domain must drop before charge");
        }
        s.watch = None;
        s.domain = None;
        s.live -= self.bytes;
    }
}
impl<'plan> AudioSoundContextAdmission for Policy<'plan> {
    type Error = PlanError;
    type Issuer = usize;
    type Lease = Token<'plan>;
    fn issuer(&self) -> usize {
        Rc::as_ptr(&self.0) as usize
    }
    fn spend(&mut self, n: usize) -> Result<(), PlanError> {
        let mut s = self.0.borrow_mut();
        if s.cancel {
            return Err(PlanError::AudioQueryLimit("test cancelled"));
        }
        s.work += n;
        Ok(())
    }
    fn reserve(&mut self, layout: Layout) -> Result<Token<'plan>, PlanError> {
        let mut s = self.0.borrow_mut();
        s.calls += 1;
        let next = s.live.checked_add(layout.size()).unwrap();
        if next > s.cap {
            s.denied = true;
            return Err(PlanError::AudioQueryLimit("test heap bytes"));
        }
        s.live = next;
        s.peak = s.peak.max(next);
        Ok(Token {
            state: Rc::clone(&self.0),
            bytes: layout.size(),
        })
    }
    fn merge(&mut self, a: &mut Token<'plan>, b: &mut Token<'plan>) {
        assert!(Rc::ptr_eq(&a.state, &self.0) && Rc::ptr_eq(&b.state, &self.0));
        a.bytes += b.bytes;
        b.bytes = 0;
    }
}
fn policy<'plan>(cap: usize) -> Policy<'plan> {
    Policy(Rc::new(RefCell::new(State {
        cap,
        ..State::default()
    })))
}
fn owner(plan: &RenderPlan) -> InstancePath {
    plan.audio_gain_owners(AudioSample(1)..AudioSample(2), AudioQueryLimits::default())
        .unwrap()
        .spans()[0]
        .owners()
        .last()
        .unwrap()
        .instance()
        .clone()
}
#[test]
fn actual_constructor_exact_peak_one_short_and_drop_order() {
    for preserve in [false, true] {
        let plan = fixture(preserve);
        let owner = owner(&plan);
        let mut a = policy(usize::MAX);
        let result = plan
            .audio_sound_instance_context_admitted(&owner, AudioQueryLimits::default(), &mut a)
            .unwrap()
            .into_shared(&mut a)
            .unwrap()
            .with_processing_domain(&mut a)
            .unwrap();
        let legacy = plan
            .audio_sound_instance_context(&owner, AudioQueryLimits::default())
            .unwrap();
        assert_eq!(result.context().owner(), legacy.owner());
        assert_eq!(result.context().samples(), legacy.samples());
        assert_eq!(
            result.context().processing_domain().root_samples(),
            legacy.processing_domain().root_samples()
        );
        assert!(a.0.borrow().work > legacy.construction_work());
        let peak = a.0.borrow().peak;
        assert!(peak > 0 && a.0.borrow().live > 0);
        a.0.borrow_mut().watch = Some(Arc::downgrade(result.shared()));
        a.0.borrow_mut().domain = Some(Arc::downgrade(result.processing_domain().unwrap()));
        drop(result);
        assert_eq!(a.0.borrow().live, 0);

        let mut exact = policy(peak);
        let result = plan
            .audio_sound_instance_context_admitted(&owner, AudioQueryLimits::default(), &mut exact)
            .unwrap()
            .into_shared(&mut exact)
            .unwrap()
            .with_processing_domain(&mut exact)
            .unwrap();
        assert_eq!(exact.0.borrow().peak, peak);
        drop(result);
        assert_eq!(exact.0.borrow().live, 0);

        let mut short = policy(peak - 1);
        let result = plan
            .audio_sound_instance_context_admitted(&owner, AudioQueryLimits::default(), &mut short)
            .and_then(|v| v.into_shared(&mut short))
            .and_then(|v| v.with_processing_domain(&mut short));
        assert!(result.is_err());
        assert!(short.0.borrow().denied);
        assert_eq!(short.0.borrow().live, 0);
        assert!(short.0.borrow().work > 0);
    }
}
#[test]
fn same_type_different_issuer_refuses_before_any_new_policy_call() {
    for fallback in [false, true] {
        let plan = fixture(true);
        let owner = owner(&plan);
        let mut a = policy(usize::MAX);
        let mut b = policy(usize::MAX);
        let value = plan
            .audio_sound_instance_context_admitted(&owner, AudioQueryLimits::default(), &mut a)
            .unwrap()
            .into_shared(&mut a)
            .unwrap();
        a.0.borrow_mut().watch = Some(Arc::downgrade(value.shared()));
        let work = a.0.borrow().work;
        let failed = if fallback {
            value.with_processing_domain(&mut b)
        } else {
            value.into_shared(&mut b)
        };
        assert!(matches!(
            failed,
            Err(PlanError::AudioMetadataIssuerMismatch)
        ));
        assert_eq!(a.0.borrow().live, 0);
        assert_eq!(a.0.borrow().work, work);
        assert_eq!(
            (b.0.borrow().calls, b.0.borrow().work, b.0.borrow().live),
            (0, 0, 0)
        );
    }
}
#[test]
fn failure_after_shared_value_and_cancel_drop_values_before_charge() {
    for cancel in [false, true] {
        let plan = fixture(true);
        let owner = owner(&plan);
        let mut a = policy(usize::MAX);
        let value = plan
            .audio_sound_instance_context_admitted(&owner, AudioQueryLimits::default(), &mut a)
            .unwrap()
            .into_shared(&mut a)
            .unwrap();
        let work = a.0.borrow().work;
        {
            let mut state = a.0.borrow_mut();
            state.watch = Some(Arc::downgrade(value.shared()));
            if cancel {
                state.cancel = true;
            } else {
                state.cap = state.live;
            }
        }
        assert!(value.with_processing_domain(&mut a).is_err());
        assert_eq!(a.0.borrow().live, 0);
        assert!(a.0.borrow().work >= work);
    }
}
#[test]
fn changed_closed_shape_is_a_typed_failure_before_fallback_reservation() {
    let plan = fixture(false);
    let owner = owner(&plan);
    let mut a = policy(usize::MAX);
    let mut value = plan
        .audio_sound_instance_context_admitted(&owner, AudioQueryLimits::default(), &mut a)
        .unwrap()
        .into_shared(&mut a)
        .unwrap();
    let context = Arc::get_mut(value.values.shared.as_mut().unwrap()).unwrap();
    Arc::get_mut(&mut context.policy).unwrap().seed.envelope =
        Some(ExactRatio::ZERO..ExactRatio::ONE);
    let calls = a.0.borrow().calls;
    assert!(matches!(
        value.with_processing_domain(&mut a),
        Err(PlanError::AudioSoundContextShapeChanged)
    ));
    assert_eq!(a.0.borrow().calls, calls);
    assert_eq!(a.0.borrow().live, 0);
}
#[test]
fn legacy_root_work_receipt_has_no_new_inspection_scan() {
    let plan = fixture(false);
    let root = InstancePath {
        node: NodeId::new("root").unwrap(),
        repeats: Vec::new(),
    };
    assert_eq!(
        plan.audio_sound_instance_context(&root, AudioQueryLimits::default())
            .unwrap()
            .construction_work(),
        1
    );
}

#[test]
fn handoff_mismatch_cannot_invoke_transfer_or_change_either_ledger() {
    let plan = fixture(false);
    let owner = owner(&plan);
    let mut a = policy(usize::MAX);
    let b = policy(usize::MAX);
    let mut value = plan
        .audio_sound_instance_context_admitted(&owner, AudioQueryLimits::default(), &mut a)
        .unwrap()
        .into_shared(&mut a)
        .unwrap();
    let before = (a.0.borrow().live, a.0.borrow().calls, a.0.borrow().work);
    let mut invoked = false;
    assert!(matches!(
        value.transfer_charge(b.issuer(), |_| invoked = true),
        Err(PlanError::AudioMetadataIssuerMismatch)
    ));
    assert!(!invoked);
    assert_eq!(
        (a.0.borrow().live, a.0.borrow().calls, a.0.borrow().work),
        before
    );
    assert_eq!(
        (b.0.borrow().live, b.0.borrow().calls, b.0.borrow().work),
        (0, 0, 0)
    );
    drop(value);
    assert_eq!(a.0.borrow().live, 0);
}

#[test]
fn policy_identity_change_after_reserve_refuses_before_buffer_construction() {
    struct Shifting<'plan> {
        policy: Policy<'plan>,
        epoch: usize,
    }
    impl<'plan> AudioSoundContextAdmission for Shifting<'plan> {
        type Error = PlanError;
        type Issuer = (usize, usize);
        type Lease = Token<'plan>;
        fn issuer(&self) -> Self::Issuer {
            (self.policy.issuer(), self.epoch)
        }
        fn spend(&mut self, n: usize) -> Result<(), PlanError> {
            self.policy.spend(n)
        }
        fn reserve(&mut self, layout: Layout) -> Result<Self::Lease, PlanError> {
            let lease = self.policy.reserve(layout)?;
            self.epoch += 1;
            Ok(lease)
        }
        fn merge(&mut self, a: &mut Self::Lease, b: &mut Self::Lease) {
            self.policy.merge(a, b)
        }
    }
    let plan = fixture(false);
    let owner = owner(&plan);
    let mut shifting = Shifting {
        policy: policy(usize::MAX),
        epoch: 0,
    };
    assert!(matches!(
        plan.audio_sound_instance_context_admitted(
            &owner,
            AudioQueryLimits::default(),
            &mut shifting
        ),
        Err(PlanError::AudioMetadataIssuerMismatch)
    ));
    assert_eq!(shifting.policy.0.borrow().calls, 1);
    assert_eq!(shifting.policy.0.borrow().live, 0);
}
