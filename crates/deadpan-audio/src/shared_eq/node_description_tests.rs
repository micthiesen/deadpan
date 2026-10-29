use super::*;
use crate::stages::authored_eq::eq_admission_fixture as fixture;
use deadpan_core::*;

fn request<'request, 'plan>(
    incoming: &'request Contexts<'plan>,
    owner: &'request AudioOwnerClock<'plan>,
    at: AudioSample,
    kind: Temporal,
    original_only: bool,
) -> Request<'request, 'plan> {
    Request {
        incoming,
        owner,
        at,
        kind,
        stage: kind.boundary(owner),
        original_only,
    }
}

fn domain<'context, 'plan>(context: &'context Context<'plan>) -> &'context AudioDomain<'plan> {
    let Context::Domain(domain) = context else {
        panic!("expected a checked Domain");
    };
    domain
}

#[test]
fn root_and_actual_domain_descriptions_keep_literal_context_stop_and_kind() {
    let plan = fixture::fixture();
    let owner = fixture::owner(&plan);
    let stage = fixture::stage(&owner);
    let input = plan
        .audio_owner_input(&owner, AudioSample(128), Default::default())
        .unwrap();
    for original in [
        Context::Root,
        Context::Domain(Arc::new(input.context().clone())),
    ] {
        for original_only in [false, true] {
            for kind in [Temporal::Equalizer(stage), Temporal::Wet(TailEffect::Delay)] {
                let cancelled = AtomicBool::new(false);
                let work = RefCell::new(ReadWork::default());
                let control = fixture::control(&cancelled, &work);
                // Incoming Sounds are intentionally ignored by the inherited
                // new-node rule, which resolves current sound history anew.
                let incoming = Contexts {
                    original: original.clone(),
                    sounds: Sounds::Root,
                };
                let description = describe(
                    &plan,
                    request(&incoming, &owner, AudioSample(128), kind, original_only),
                    control,
                )
                .unwrap();
                assert!(description.kind == kind);
                assert_eq!(description.boundary.before_stage(), kind.boundary(&owner));
                assert!(
                    description
                        .boundary
                        .matches_clock_at(&owner, AudioSample(128))
                );
                assert_eq!(description.stop.before, kind.boundary(&owner));
                assert!(description.stop.owner == OwnerKey::new(&owner));
                let original = domain(&description.context.original);
                assert!(original.belongs_to(&plan));
                assert_eq!(original.instance().node, NodeId::new("root").unwrap());
                assert_eq!(original.root_samples(), AudioSample(0)..AudioSample(512));
                assert_eq!(original.visible_samples(), AudioSample(0)..AudioSample(512));
                match (&description.context.sounds, original_only) {
                    (Sounds::None, true) => {}
                    (Sounds::Owner(sounds), false) => {
                        assert!(sounds.belongs_to(&plan));
                        assert_eq!(sounds.owner().node, NodeId::new("root").unwrap());
                        assert_eq!(sounds.samples(), AudioSample(0)..AudioSample(512));
                    }
                    _ => panic!("wrong sound-history selection"),
                }
            }
        }
    }
}

#[test]
fn empty_original_support_keeps_current_sound_fallback_and_missing_error() {
    let plan = fixture::fixture();
    let owner = fixture::owner(&plan);
    let incoming = Contexts {
        original: Context::Root,
        sounds: Sounds::None,
    };
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = fixture::control(&cancelled, &work);
    let description = describe(
        &plan,
        request(
            &incoming,
            &owner,
            AudioSample(600),
            Temporal::Wet(TailEffect::Delay),
            false,
        ),
        control,
    )
    .unwrap();
    let Sounds::Owner(sound) = &description.context.sounds else {
        panic!("fallback needs the freshly resolved current sound context");
    };
    let original = domain(&description.context.original);
    assert_eq!(
        original.root_samples(),
        sound.processing_domain().root_samples()
    );
    assert_eq!(original.instance(), sound.processing_domain().instance());
    let result = describe(
        &plan,
        request(
            &incoming,
            &owner,
            AudioSample(600),
            Temporal::Wet(TailEffect::Delay),
            true,
        ),
        control,
    );
    assert!(matches!(
        result,
        Err(StageAudioError::Plan(PlanError::InvalidPlan(
            "missing shared EQ Original context"
        )))
    ));
}

fn retained_plan() -> Arc<RenderPlan> {
    let id = |name: &str| NodeId::new(name).unwrap();
    let revision = |name: &str| RevisionId::new(name).unwrap();
    let document = ProjectDocument::new(
        ProjectId::new("node-description-retained").unwrap(),
        revision("current"),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(48_000, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(document).unwrap();
    wire["nodes"] = serde_json::to_value(BTreeMap::from([
        (id("root"), BeatNode::sequence("Root", vec![id("tone")])),
        (
            id("tone"),
            BeatNode::hold(
                "Tone",
                HoldRecipe {
                    picture_context: None,
                    duration: FrameDuration::new(512).unwrap(),
                    video: HoldVideo::Background,
                    audio: HoldAudio::Tone {
                        frequency_hz: 440,
                        level: GainDb::new(-6000).unwrap(),
                    },
                },
            ),
        ),
    ]))
    .unwrap();
    let old = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let timing = AudioTimingId {
        allocation: revision("saved"),
        ordinal: 0,
    };
    wire["audio_bindings"] = serde_json::to_value(
        AudioBindingState::new(
            vec![AudioTimingRecord {
                id: timing.clone(),
                layout: FrozenAudioLayout::capture(&old).unwrap(),
            }],
            BTreeMap::from([(
                id("tone"),
                OwnedAudioBinding {
                    reanchors: vec![],
                    lattice: AudioPlacementTemplate {
                        reference_local_offset: ExactRatio::ZERO,
                        gap_after: None,
                        reference: AudioReferenceClock {
                            recipe: AudioRecipeKind::Node,
                            timing,
                            root: AudioClockRoot::ProjectRootRoundEven,
                            physical: id("tone"),
                        },
                        arguments: vec![],
                        births: vec![],
                    },
                    resume: None,
                },
            )]),
        )
        .unwrap(),
    )
    .unwrap();
    Arc::new(RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap())
}

#[test]
fn incoming_retained_description_clones_the_exact_input_arc() {
    let plan = retained_plan();
    let query = plan
        .audio_gain_owners(AudioSample(128)..AudioSample(129), Default::default())
        .unwrap();
    let tone = query.spans()[0]
        .owners()
        .iter()
        .find(|clock| clock.instance().node.as_str() == "tone")
        .unwrap();
    let input = plan
        .audio_owner_input(tone, AudioSample(128), Default::default())
        .unwrap();
    let retained = Arc::new(
        input
            .retained_at(AudioSample(128), Default::default())
            .unwrap()
            .unwrap(),
    );
    let owners = retained
        .owners(AudioSample(128)..AudioSample(129), Default::default())
        .unwrap();
    let owner = owners.spans()[0]
        .owners()
        .iter()
        .find(|clock| clock.instance().node.as_str() == "tone")
        .unwrap();
    let incoming = Contexts {
        original: Context::Retained(Arc::clone(&retained)),
        sounds: Sounds::None,
    };
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = fixture::control(&cancelled, &work);
    let description = describe(
        &plan,
        request(
            &incoming,
            owner,
            AudioSample(128),
            Temporal::Wet(TailEffect::Reverb),
            true,
        ),
        control,
    )
    .unwrap();
    let Context::Retained(actual) = &description.context.original else {
        panic!("incoming retained consumer must remain opaque");
    };
    assert!(Arc::ptr_eq(actual, &retained));
    assert!(matches!(description.context.sounds, Sounds::None));
    assert_eq!(description.stop.before, 0);
    assert!(description.stop.owner == OwnerKey::new(owner));
    // The initial incoming owner query remains; owner_context itself returns
    // the Arc immediately without adding a root/domain query.
    assert_eq!(work.borrow().plan_work, owners.work());
}

#[test]
fn metadata_query_work_removes_one_actual_membership_query() {
    let plan = fixture::fixture();
    let owner = fixture::owner(&plan);
    let incoming = Contexts {
        original: Context::Root,
        sounds: Sounds::None,
    };
    let kind = Temporal::Equalizer(fixture::stage(&owner));
    let cancelled = AtomicBool::new(false);
    // Independent legacy query oracle measures the actual removed query.
    // Qualified predecessor total 28 and final None probe 9 remain the anchors.
    let removed = plan
        .audio_gain_owners(AudioSample(128)..AudioSample(129), Default::default())
        .unwrap()
        .work();
    let input = plan
        .audio_owner_input(&owner, AudioSample(128), Default::default())
        .unwrap();
    let probe = input
        .retained_probe_at(AudioSample(128), Default::default())
        .unwrap();
    assert!(probe.input().is_none());
    assert_eq!(probe.work(), 9);
    assert!(removed > 0 && removed < 19);
    let needed = 28 - removed;
    for shortage in [0, 1] {
        let remaining = needed - shortage;
        let work = RefCell::new(ReadWork::default());
        let before = MAX_PLAN_WORK_PER_READ - remaining;
        work.borrow_mut().plan_work = before;
        let result = describe(
            &plan,
            request(&incoming, &owner, AudioSample(128), kind, false),
            fixture::control(&cancelled, &work),
        );
        if shortage == 0 {
            assert!(result.is_ok());
            assert_eq!(work.borrow().plan_work - before + removed, 28);
            assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
        } else {
            assert!(matches!(
                result,
                Err(StageAudioError::Plan(PlanError::AudioQueryLimit(
                    "structural work"
                )))
            ));
            assert_eq!(work.borrow().plan_work - before + removed, 19);
            assert_eq!(MAX_PLAN_WORK_PER_READ - work.borrow().plan_work, 8);
        }
    }
}

#[test]
fn overflow_cancellation_and_late_boundary_failure_keep_existing_order() {
    let plan = fixture::fixture();
    let owner = fixture::owner(&plan);
    let incoming = Contexts {
        original: Context::Root,
        sounds: Sounds::None,
    };
    let kind = Temporal::Wet(TailEffect::Delay);
    for original_only in [false, true] {
        let cancelled = AtomicBool::new(false);
        let work = RefCell::new(ReadWork::default());
        let control = fixture::control(&cancelled, &work);
        assert!(
            describe(
                &plan,
                request(
                    &incoming,
                    &owner,
                    AudioSample(i64::MAX),
                    kind,
                    original_only
                ),
                control,
            )
            .is_err()
        );
        assert_eq!(work.borrow().plan_work > 0, !original_only);
    }
    let cancelled = AtomicBool::new(true);
    let work = RefCell::new(ReadWork::default());
    assert!(
        describe(
            &plan,
            request(&incoming, &owner, AudioSample(128), kind, false),
            fixture::control(&cancelled, &work),
        )
        .is_err()
    );
    assert_eq!(work.borrow().plan_work, 0);

    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let mut expired = fixture::control(&cancelled, &work);
    expired.deadline = Instant::now();
    assert!(matches!(
        describe(
            &plan,
            request(&incoming, &owner, AudioSample(128), kind, false),
            expired,
        ),
        Err(StageAudioError::Timeout)
    ));
    assert_eq!(work.borrow().plan_work, 0);

    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = fixture::control(&cancelled, &work);
    describe(
        &plan,
        request(&incoming, &owner, AudioSample(128), kind, false),
        control,
    )
    .unwrap();
    let valid_work = work.borrow().plan_work;
    let work = RefCell::new(ReadWork::default());
    let mut invalid = request(&incoming, &owner, AudioSample(128), kind, false);
    invalid.stage = usize::MAX;
    assert!(matches!(
        describe(&plan, invalid, fixture::control(&cancelled, &work)),
        Err(StageAudioError::Plan(PlanError::InvalidPlan(
            "audio treatment boundary out of range"
        )))
    ));
    assert_eq!(work.borrow().plan_work, valid_work);
}

#[test]
fn captured_retained_context_bills_the_probe_once_before_owner_query() {
    let plan = retained_plan();
    let owners = plan
        .audio_gain_owners(AudioSample(128)..AudioSample(129), Default::default())
        .unwrap();
    let owner = owners.spans()[0]
        .owners()
        .iter()
        .find(|clock| clock.instance().node.as_str() == "tone")
        .unwrap();
    let input = plan
        .audio_owner_input(owner, AudioSample(128), Default::default())
        .unwrap();
    let probe = input
        .retained_probe_at(AudioSample(128), Default::default())
        .unwrap();
    let retained = probe.input().unwrap();
    let retained_owners = retained
        .owners(AudioSample(128)..AudioSample(129), Default::default())
        .unwrap();
    let expected = input.work() + probe.work() + retained_owners.work();
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let result = Context::Root
        .owner_context(
            &plan,
            owner,
            AudioSample(128),
            fixture::control(&cancelled, &work),
        )
        .unwrap();
    assert!(matches!(result, Context::Retained(_)));
    assert_eq!(work.borrow().plan_work, expected);
}

#[test]
fn absent_member_keeps_query_work_without_requesting_capture_headroom() {
    let plan = fixture::fixture();
    let owners = plan
        .audio_gain_owners(AudioSample(128)..AudioSample(129), Default::default())
        .unwrap();
    let tone = owners.spans()[0]
        .owners()
        .iter()
        .find(|clock| clock.instance().node.as_str() == "tone")
        .unwrap();
    let input = plan
        .audio_owner_input(tone, AudioSample(128), Default::default())
        .unwrap();
    let domain = input.context();
    let q = domain
        .gain_owners(AudioSample(128)..AudioSample(129), Default::default())
        .unwrap()
        .work();
    let incoming = Context::Domain(Arc::new(domain.clone()));
    let key = OwnerKey::new(&fixture::owner(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ - q;
    let result = original_context(
        &plan,
        &incoming,
        &key,
        AudioSample(128)..AudioSample(129),
        AudioSample(128),
        fixture::control(&cancelled, &work),
    )
    .unwrap();
    assert!(result.is_none());
    assert_eq!(work.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
}

#[test]
fn candidate_root_and_domain_contexts_match_legacy_and_save_only_query_work() {
    let plan = fixture::fixture();
    let owner = fixture::owner(&plan);
    let input = plan
        .audio_owner_input(&owner, AudioSample(128), Default::default())
        .unwrap();
    for incoming in [
        Context::Root,
        Context::Domain(Arc::new(input.context().clone())),
    ] {
        let cancelled = AtomicBool::new(false);
        let old_work = RefCell::new(ReadWork::default());
        let old_control = fixture::control(&cancelled, &old_work);
        let query = incoming
            .owners(&plan, AudioSample(128)..AudioSample(129), old_control)
            .unwrap();
        let q = query.work();
        let key = OwnerKey::new(&owner);
        let selected = query
            .spans()
            .iter()
            .flat_map(|span| span.owners())
            .find(|clock| OwnerKey::new(clock) == key)
            .unwrap();
        let expected = incoming
            .owner_context(&plan, selected, AudioSample(128), old_control)
            .unwrap();
        let new_work = RefCell::new(ReadWork::default());
        let actual = original_context(
            &plan,
            &incoming,
            &key,
            AudioSample(128)..AudioSample(129),
            AudioSample(128),
            fixture::control(&cancelled, &new_work),
        )
        .unwrap()
        .unwrap();
        assert_eq!(new_work.borrow().plan_work + q, old_work.borrow().plan_work);
        assert_eq!(
            domain(&actual).root_samples(),
            domain(&expected).root_samples()
        );
        assert_eq!(
            domain(&actual).visible_samples(),
            domain(&expected).visible_samples()
        );
        assert_eq!(domain(&actual).instance(), domain(&expected).instance());
    }
}

#[path = "original_sharing_tests.rs"]
mod original_sharing;

#[path = "legacy_query_receipts.rs"]
mod legacy_query_receipts;

#[path = "query_admission_tests.rs"]
mod query_admission;

#[path = "capture_admission_tests.rs"]
mod capture_admission;
