//! Actual top policy Point arm, with its existing admission ledger.
use super::*;
use crate::plan::audio_policy::query::point_trace::{self, Event, Phase};

mod fixture {
    use super::*;
    use deadpan_core::*;
    use std::collections::BTreeMap;

    fn id(value: &str) -> NodeId {
        NodeId::new(value).unwrap()
    }

    #[derive(Debug, Clone, Copy)]
    struct Observed {
        event: Event,
        live: usize,
        forwarded: usize,
        reservations: usize,
    }
    type Observations = Rc<RefCell<Vec<Observed>>>;

    fn observing(policy: &Policy<true>) -> (point_trace::Scope, Observations) {
        let state = Rc::clone(&policy.0);
        let events = Rc::new(RefCell::new(Vec::new()));
        let target = Rc::clone(&events);
        let scope = point_trace::Scope::start(move |event| {
            let state = state.borrow();
            target.borrow_mut().push(Observed {
                event,
                live: state.live,
                forwarded: state.forwarded,
                reservations: state.reservations,
            });
        });
        (scope, events)
    }

    #[derive(Debug)]
    struct PointReservation {
        point: Option<Event>,
        layout: Layout,
        before: usize,
        after: usize,
    }
    struct PointObserver<'a> {
        inner: Observer<'a>,
        events: Observations,
        reservations: Vec<PointReservation>,
    }
    impl AudioSoundContextAdmission for PointObserver<'_> {
        const METERED: bool = true;
        type Error = PlanError;
        type Issuer = usize;
        type Lease = Lease;
        fn issuer(&self) -> usize {
            self.inner.issuer()
        }
        fn spend(&mut self, units: usize) -> Result<(), PlanError> {
            self.inner.spend(units)
        }
        fn reserve(&mut self, layout: Layout) -> Result<Lease, PlanError> {
            let point = self.events.borrow().last().map(|event| event.event);
            let before = self.inner.policy.0.borrow().live;
            let lease = self.inner.reserve(layout)?;
            let after = self.inner.policy.0.borrow().live;
            self.reservations.push(PointReservation {
                point,
                layout,
                before,
                after,
            });
            Ok(lease)
        }
        fn merge(&mut self, into: &mut Lease, from: &mut Lease) {
            self.inner.merge(into, from);
        }
    }

    #[test]
    fn frozen_point_case_observes_actual_point_arm_and_keeps_the_complete_old_policy() {
        let mut checked = 0;
        visit_seed(|parent, case| {
            if case["name"] != "wide/nonunity_shift_fractional_point_ceil_source44100/root" {
                return;
            }
            let row = defaults(case)
                .find(|row| row["name"] == "default_full")
                .unwrap();
            let range = samples(&row["request"]["samples"]);
            let mut admission = Policy::<true>::new(usize::MAX);
            let (scope, events) = observing(&admission);
            let query =
                policy_admitted(parent, range.clone(), Default::default(), &mut admission).unwrap();
            assert_true(&query, &row["output"]["policy"], &admission, (0, 0));
            drop(scope);
            let events = events.borrow();
            assert!(events.iter().any(|observed| {
                observed.event.phase == Phase::BeforeClone
                    && observed.event.context.origin != 0
                    && observed.event.context.reference_support[0]
                        < observed.event.context.reference_support[1]
                    && observed.event.context.selection[0] < observed.event.context.selection[1]
                    && observed.live > 0
            }));
            assert!(
                events
                    .iter()
                    .any(|observed| observed.event.phase == Phase::AfterClone && observed.live > 0)
            );
            assert!(
                events
                    .iter()
                    .any(|observed| observed.event.phase == Phase::AfterOffset)
            );
            drop(query);
            assert_eq!(admission.0.borrow().live, 0);
            drop(events);

            let false_events = Rc::new(RefCell::new(Vec::new()));
            let target = Rc::clone(&false_events);
            let scope = point_trace::Scope::start(move |event| target.borrow_mut().push(event));
            let mut unmetered = Policy::<false>::new(0);
            let query = policy_admitted(parent, range, Default::default(), &mut unmetered).unwrap();
            assert_eq!(
                oracle::query_snapshot(query.query()),
                row["output"]["policy"]
            );
            drop(query);
            drop(scope);
            assert!(
                false_events.borrow().is_empty(),
                "ordinary compatibility does not emit admitted Point events"
            );
            no_callbacks(&unmetered);
            checked += 1;
        });
        assert_eq!(checked, 1);
    }

    #[test]
    fn supplemental_signed_point_repeat_retains_actual_clone_and_offset_buffers() {
        let plan = fixture::signed_repeated_plan();
        let parent = Parent::Root(&plan);
        let range = AudioSample(0)..plan.audio_duration().unwrap();
        // Supplemental current ordinary parity, separate from the frozen OLD oracle above.
        let old = plan
            .audio_policy(range.clone(), Default::default())
            .unwrap();
        assert!(old.work > 0 && !old.suppressed.is_empty());
        let old = oracle::query_snapshot(&old);
        let mut admission = Policy::<true>::new(usize::MAX);
        let (scope, events) = observing(&admission);
        let mut observer = PointObserver {
            inner: Observer {
                policy: &mut admission,
                spends: Vec::new(),
                reservations: Vec::new(),
                floor: 0,
            },
            events: Rc::clone(&events),
            reservations: Vec::new(),
        };
        let query =
            policy_admitted(parent, range.clone(), Default::default(), &mut observer).unwrap();
        let reservations = observer.reservations;
        drop(observer.inner);
        drop(scope);
        assert_true(&query, &old, &admission, (0, 0));
        let events = events.borrow();
        assert!(!events.is_empty());
        assert_eq!(events.len() % 4, 0);
        let expected_range_layout = Layout::array::<Range<i64>>(1).unwrap();
        for frame in events.chunks_exact(4) {
            let [before, cloned, input, offset] = frame else {
                unreachable!()
            };
            assert_eq!(
                [
                    before.event.phase,
                    cloned.event.phase,
                    input.event.phase,
                    offset.event.phase
                ],
                [
                    Phase::BeforeClone,
                    Phase::AfterClone,
                    Phase::BeforeOffset,
                    Phase::AfterOffset
                ]
            );
            assert!(
                frame
                    .iter()
                    .all(|event| event.event.context == before.event.context)
            );
            assert!(before.event.context.origin < 0);
            assert!(before.event.context.reference_support[0] < 0);
            assert!(before.event.context.selection[0] < before.event.context.selection[1]);
            assert!(before.live > 0);
            assert!(
                cloned.live > before.live,
                "actual repeated-path Signal clone retains additional owning storage"
            );
            assert_eq!(before.event.canonical, cloned.event.canonical);
            assert!(cloned.event.additional > before.event.additional);
            assert_eq!(input.event.ranges, 1);
            assert_eq!(offset.event.ranges, 1);
            assert_eq!(input.event.first, input.event.last);
            assert_eq!(offset.event.first, offset.event.last);
            let input_range = input.event.first.unwrap();
            let shifted = offset.event.first.unwrap();
            assert_eq!(
                shifted,
                [
                    input_range[0]
                        .checked_add(before.event.context.origin)
                        .unwrap(),
                    input_range[1]
                        .checked_add(before.event.context.origin)
                        .unwrap(),
                ]
            );
            assert!(input.live >= cloned.live + expected_range_layout.size());
            assert!(
                reservations.iter().any(|reservation| {
                    reservation.point.is_some_and(|event| {
                        event.phase == Phase::BeforeOffset && event.context == input.event.context
                    }) && reservation.layout == expected_range_layout
                        && reservation.before >= input.live
                        && reservation.after == reservation.before + expected_range_layout.size()
                }),
                "real offset output allocation overlaps the charged recursive input and Point frame"
            );
        }
        let total = query.work();
        let peak = admission.0.borrow().peak;
        drop(events);
        drop(query);
        assert_eq!(admission.0.borrow().live, 0);

        let mut exact = Policy::<true>::new(peak);
        let query = policy_admitted(
            parent,
            range.clone(),
            AudioQueryLimits {
                maximum_work: total,
                ..Default::default()
            },
            &mut exact,
        )
        .unwrap();
        assert_true(&query, &old, &exact, (0, 0));
        assert_eq!(query.work(), total);
        assert_eq!(exact.0.borrow().peak, peak);
        drop(query);
        assert_eq!(exact.0.borrow().live, 0);
        let mut short = Policy::<true>::new(peak - 1);
        assert!(matches!(
            policy_admitted(parent, range.clone(), Default::default(), &mut short),
            Err(PlanError::AudioQueryLimit("test wide allocation refusal"))
        ));
        assert_eq!(short.0.borrow().live, 0);
        let mut short_work = Policy::<true>::new(usize::MAX);
        match policy_admitted(
            parent,
            range,
            AudioQueryLimits {
                maximum_work: total - 1,
                ..Default::default()
            },
            &mut short_work,
        ) {
            Ok(_) => panic!("one-short signed Point policy total completed"),
            Err(error) => work_limit_error(error, false),
        }
        assert_eq!(short_work.0.borrow().live, 0);
        assert!(short_work.0.borrow().forwarded < total);
    }

    #[test]
    fn signed_point_offset_reservation_refusal_and_rotation_release_old_and_current_ledgers() {
        let plan = fixture::signed_repeated_plan();
        for rotate in [false, true] {
            let mut admission = Policy::<true>::new(usize::MAX);
            let original = Rc::clone(&admission.0);
            let observed = Rc::new(RefCell::new(Vec::new()));
            let target = Rc::clone(&observed);
            let state = Rc::clone(&original);
            let mut armed = false;
            let scope = point_trace::Scope::start(move |event| {
                let mut state = state.borrow_mut();
                target.borrow_mut().push(Observed {
                    event,
                    live: state.live,
                    forwarded: state.forwarded,
                    reservations: state.reservations,
                });
                if !armed && event.phase == Phase::BeforeOffset && event.context.origin < 0 {
                    assert_eq!(event.ranges, 1);
                    assert!(state.live > 0);
                    let next = state.reservations + 1;
                    if rotate {
                        state.rotate_reservation = Some(next);
                    } else {
                        state.refuse_reservation = Some(next);
                    }
                    armed = true;
                }
            });
            let result = plan.audio_policy_admitted(
                AudioSample(0)..plan.audio_duration().unwrap(),
                Default::default(),
                &mut admission,
            );
            match result {
                Ok(_) => panic!("Point offset reservation injection did not refuse"),
                Err(PlanError::AudioMetadataIssuerMismatch) => assert!(rotate),
                Err(PlanError::AudioQueryLimit("test wide reservation refusal")) => {
                    assert!(!rotate)
                }
                Err(error) => panic!("unrelated Point refusal: {error:?}"),
            }
            drop(scope);
            let observed = observed.borrow();
            let before = observed
                .iter()
                .find(|event| {
                    event.event.phase == Phase::BeforeOffset && event.event.context.origin < 0
                })
                .unwrap();
            assert!(before.live > 0);
            assert_eq!(
                original.borrow().forwarded,
                before.forwarded + 1,
                "actual pre-reservation fee remains spent without a refund"
            );
            assert_eq!(original.borrow().reservations, before.reservations + 1);
            assert_eq!(original.borrow().live, 0);
            assert_eq!(admission.0.borrow().live, 0);
            assert!(
                !observed
                    .iter()
                    .any(|event| event.event.phase == Phase::AfterOffset)
            );
            if rotate {
                assert!(!Rc::ptr_eq(&original, &admission.0));
                assert_eq!(admission.0.borrow().forwarded, 0);
            }
        }
    }
    fn frames(value: i64) -> FrameDuration {
        FrameDuration::new(value).unwrap()
    }
    fn hold(length: i64) -> BeatNode {
        BeatNode::hold(
            "Voice",
            HoldRecipe {
                duration: frames(length),
                video: HoldVideo::Background,
                picture_context: None,
                audio: HoldAudio::Silence,
            },
        )
    }
    fn retime(child: &str, length: i64, start: i64, end: i64, pitch: PitchPolicy) -> BeatNode {
        BeatNode {
            label: "Timing".into(),
            framing: None,
            audio_treatments: Default::default(),
            audio_editorial_edges: Default::default(),
            audio_edges: Default::default(),
            kind: NodeKind::Retime {
                child: id(child),
                duration: frames(length),
                mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
                pitch,
                purpose: RetimePurpose::Edit,
            },
            cutaways: Vec::new(),
            captions: Vec::new(),
        }
    }
    fn document(rate: u32, children: &[&str], nodes: Vec<(&str, BeatNode)>) -> ProjectDocument {
        let document = ProjectDocument::new(
            ProjectId::new("fade-query").unwrap(),
            RevisionId::new("current").unwrap(),
            PresentationBasis {
                width: 16,
                height: 16,
                frame_rate: FrameRate::new(rate, 1).unwrap(),
                color_policy: ColorPolicy::SdrRec709,
            },
            id("root"),
        )
        .unwrap();
        let mut nodes: BTreeMap<_, _> = nodes
            .into_iter()
            .map(|(name, node)| (id(name), node))
            .collect();
        nodes.insert(
            id("root"),
            BeatNode::sequence("Root", children.iter().map(|name| id(name)).collect()),
        );
        let mut wire = serde_json::to_value(document).unwrap();
        wire["nodes"] = serde_json::to_value(nodes).unwrap();
        ProjectDocument::from_json(&wire.to_string()).unwrap()
    }
    fn bind(
        current: &ProjectDocument,
        reference: &ProjectDocument,
        physical: &str,
        clock: AudioClockRoot,
        phase: Option<ExactRatio>,
    ) -> ProjectDocument {
        let timing = AudioTimingId {
            allocation: RevisionId::new("fade-clock").unwrap(),
            ordinal: 0,
        };
        let binding = OwnedAudioBinding {
            reanchors: Vec::new(),
            lattice: AudioPlacementTemplate {
                reference_local_offset: deadpan_core::ExactRatio::ZERO,
                gap_after: None,
                reference: AudioReferenceClock {
                    recipe: AudioRecipeKind::Node,
                    timing: timing.clone(),
                    root: clock,
                    physical: id(physical),
                },
                arguments: vec![],
                births: vec![],
            },
            resume: phase.map(|constant| AudioResume {
                local_boundary: ExactRatio::ZERO,
                phase: AudioLocalPhase {
                    constant,
                    terms: vec![],
                },
            }),
        };
        let state = AudioBindingState::new(
            vec![AudioTimingRecord {
                id: timing,
                layout: FrozenAudioLayout::capture(reference).unwrap(),
            }],
            BTreeMap::from([(id(physical), binding)]),
        )
        .unwrap();
        let mut wire = serde_json::to_value(current).unwrap();
        wire["audio_bindings"] = serde_json::to_value(state).unwrap();
        ProjectDocument::from_json(&wire.to_string()).unwrap()
    }

    pub(super) fn signed_repeated_plan() -> RenderPlan {
        let reference = document(
            32_000,
            &["preserve"],
            vec![
                ("voice", hold(3)),
                ("preserve", retime("voice", 2, 1, 2, PitchPolicy::Preserve)),
            ],
        );
        let mut repeated = hold(1);
        repeated.kind = NodeKind::Repeat {
            child: id("voice"),
            iterations: IterationOrder::new(RevisionId::new("policy-point-plays").unwrap(), 2)
                .unwrap(),
            gap: None,
            escalation: None,
        };
        let current = document(
            32_000,
            &["repeat"],
            vec![("voice", hold(3)), ("repeat", repeated)],
        );
        let bound = bind(
            &current,
            &reference,
            "voice",
            AudioClockRoot::PreserveInputPointCeil {
                stage: id("preserve"),
            },
            None,
        );
        RenderPlan::compile(&bound).unwrap()
    }
}
