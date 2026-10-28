use super::*;
use deadpan_core::*;
use std::{
    alloc::Layout,
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    sync::{Arc, Weak},
};

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
    let mut tone = BeatNode::hold(
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
    tone.audio_editorial_edges = AudioEditorialEdges {
        start: true,
        end: true,
    };
    let mut nodes = BTreeMap::from([(id("tone"), tone)]);
    let mut repeat = nodes[&id("tone")].clone();
    repeat.kind = NodeKind::Repeat {
        child: id(if preserve { "preserve" } else { "tone" }),
        iterations: IterationOrder::new(RevisionId::new("plays").unwrap(), 2).unwrap(),
        gap: Some(HoldRecipe {
            duration: FrameDuration::new(8).unwrap(),
            picture_context: None,
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        }),
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
    cap: usize,
    live: usize,
    peak: usize,
    calls: usize,
    work: usize,
    fail_after: Option<usize>,
    domain: Option<Weak<AudioDomain<'plan>>>,
    retained: Option<Weak<AudioOwnerRetainedInput<'plan>>>,
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
        let mut state = self.state.borrow_mut();
        if let Some(value) = &state.domain {
            assert!(value.upgrade().is_none(), "Domain before lease");
        }
        if let Some(value) = &state.retained {
            assert!(value.upgrade().is_none(), "Retained before lease");
        }
        state.live -= self.bytes;
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
        let mut state = self.0.borrow_mut();
        if state.fail_after.is_some_and(|limit| state.work >= limit) {
            return Err(PlanError::AudioQueryLimit("test cancellation"));
        }
        state.work += n;
        Ok(())
    }
    fn reserve(&mut self, layout: Layout) -> Result<Token<'plan>, PlanError> {
        let mut state = self.0.borrow_mut();
        state.calls += 1;
        let next = state.live.checked_add(layout.size()).unwrap();
        if next > state.cap {
            return Err(PlanError::AudioQueryLimit("test heap bytes"));
        }
        state.live = next;
        state.peak = state.peak.max(next);
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
        ..Default::default()
    })))
}
fn parity(source: &AudioDomain<'_>, target: &AudioDomain<'_>) {
    assert_eq!(source.root_samples(), target.root_samples());
    assert_eq!(source.visible_samples(), target.visible_samples());
    assert_eq!(source.instance(), target.instance());
    assert_eq!(source.definition(), target.definition());
    assert_eq!(source.placement(), target.placement());
    assert_eq!(source.sample_grid(), target.sample_grid());
    assert_eq!(
        serde_json::to_value(
            source
                .audio(source.root_samples(), Default::default())
                .unwrap()
                .spans
        )
        .unwrap(),
        serde_json::to_value(
            target
                .audio(target.root_samples(), Default::default())
                .unwrap()
                .spans
        )
        .unwrap()
    );
}
#[test]
fn finite_domain_actual_repeat_gap_preserve_definition_exact_bytes_and_drop_order() {
    for preserve in [false, true] {
        let plan = fixture(preserve);
        let mut domains = vec![
            plan.audio_domain_at(AudioSample(1), Default::default())
                .unwrap(),
        ];
        if !preserve {
            domains.push(
                plan.audio_domain_at(AudioSample(130), Default::default())
                    .unwrap(),
            );
        }
        for selector in [
            AudioDefinitionSelector::Node {
                node: NodeId::new("tone").unwrap(),
            },
            AudioDefinitionSelector::RepeatGap {
                repeat: NodeId::new("repeat").unwrap(),
            },
        ] {
            let definition = plan.audio_definition(selector).unwrap();
            domains.push(
                definition
                    .in_root_clock(
                        AudioRootPlacement::new(
                            ExactRatio::integer(-3),
                            ExactRatio::integer(2),
                            ExactRatio::ZERO..definition.exact_duration(),
                        )
                        .unwrap(),
                    )
                    .unwrap(),
            );
        }
        for source in domains {
            let mut admitted = policy(usize::MAX);
            let value = source
                .clone_shared_admitted(Default::default(), &mut admitted)
                .unwrap();
            parity(&source, value.domain().unwrap());
            let peak = admitted.0.borrow().peak;
            assert!(peak > 0 && admitted.0.borrow().work > 0);
            assert_eq!(
                admitted.0.borrow().live,
                peak,
                "new copy has no hidden transient buffer"
            );
            admitted.0.borrow_mut().domain = Some(Arc::downgrade(value.domain().unwrap()));
            drop(value);
            assert_eq!(admitted.0.borrow().live, 0);
            let mut exact = policy(peak);
            let value = source
                .clone_shared_admitted(Default::default(), &mut exact)
                .unwrap();
            assert_eq!(exact.0.borrow().peak, peak);
            drop(value);
            assert_eq!(exact.0.borrow().live, 0);
            let mut short = policy(peak - 1);
            assert!(
                source
                    .clone_shared_admitted(Default::default(), &mut short)
                    .is_err()
            );
            assert_eq!(short.0.borrow().live, 0);
            let mut cancelled = policy(usize::MAX);
            cancelled.0.borrow_mut().fail_after = Some(3);
            assert!(
                source
                    .clone_shared_admitted(Default::default(), &mut cancelled)
                    .is_err()
            );
            assert_eq!(cancelled.0.borrow().live, 0);
        }
    }
}
#[test]
fn joint_carriers_validate_every_issuer_before_callback_or_counter_change() {
    let plan = fixture(false);
    let domain = plan
        .audio_domain_at(AudioSample(1), Default::default())
        .unwrap();
    let owner = domain.instance().clone();
    let mut a = policy(usize::MAX);
    let mut b = policy(usize::MAX);
    let mut original = domain
        .clone_shared_admitted(Default::default(), &mut a)
        .unwrap();
    let mut sound = plan
        .audio_sound_instance_context_admitted(&owner, Default::default(), &mut b)
        .unwrap()
        .into_shared(&mut b)
        .unwrap();
    let before = (
        a.0.borrow().live,
        b.0.borrow().live,
        a.0.borrow().calls,
        b.0.borrow().calls,
    );
    for issuer in [a.issuer(), b.issuer()] {
        let mut called = false;
        assert!(matches!(
            transfer_context_charges(issuer, Some(&mut original), Some(&mut sound), |_, _| {
                called = true
            }),
            Err(PlanError::AudioMetadataIssuerMismatch)
        ));
        assert!(!called);
        assert_eq!(
            (
                a.0.borrow().live,
                b.0.borrow().live,
                a.0.borrow().calls,
                b.0.borrow().calls
            ),
            before
        );
    }
    let mut called = false;
    transfer_context_charges(a.issuer(), Some(&mut original), None, |left, right| {
        assert!(left.is_some() && right.is_none());
        called = true;
    })
    .unwrap();
    assert!(called);
    transfer_context_charges(
        a.issuer(),
        None,
        None::<&mut AdmittedSoundContext<'_, usize, Token<'_>>>,
        |left, right| assert!(left.is_none() && right.is_none()),
    )
    .unwrap();
    drop((original, sound));
    assert_eq!(a.0.borrow().live, 0);
    assert_eq!(b.0.borrow().live, 0);
}
