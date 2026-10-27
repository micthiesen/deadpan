use super::*;
use deadpan_core::*;

fn work_control<'a>(cancelled: &'a AtomicBool, work: &'a RefCell<ReadWork>) -> WorkControl<'a> {
    WorkControl {
        cancelled,
        deadline: Instant::now() + Duration::from_secs(10),
        work,
    }
}

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn gain(value: i32, muted: bool, dynamic: bool) -> ClipGain {
    let envelopes = if dynamic {
        let range = GainRange::new(ExactRatio::ZERO, ExactRatio::integer(32)).unwrap();
        vec![
            GainEnvelope::new(
                GainClock::OwnerOutput,
                range,
                GainDb::UNITY,
                vec![GainSegment::new(range.end(), GainDb::UNITY, GainCurve::Step).unwrap()],
            )
            .unwrap(),
        ]
    } else {
        vec![]
    };
    ClipGain::new(GainDb::new(value).unwrap(), muted, envelopes, vec![]).unwrap()
}

pub(in crate::stages::shared_components) fn plan(
    dynamic: bool,
    muted: bool,
    gain_first: bool,
) -> RenderPlan {
    let root = ProjectDocument::new(
        ProjectId::new("gain-program").unwrap(),
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
    let sends =
        TailSends::default().with_level(TailEffect::Delay, Some(GainDb::new(-3000).unwrap()));
    let mut leaf = BeatNode::hold(
        "Leaf",
        HoldRecipe {
            picture_context: None,
            duration: FrameDuration::new(4).unwrap(),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    );
    leaf.audio_treatments = AudioTreatments::from_clip_gain(gain(24000, muted, dynamic))
        .with_balance(Some(StereoBalance::new(-37).unwrap()))
        .unwrap()
        .with_tail_sends(sends);
    let repeat = |name: &str, child: &str, step: i32| {
        let mut node = BeatNode::sequence(name, vec![]);
        node.kind = NodeKind::Repeat {
            child: id(child),
            iterations: IterationOrder::new(RevisionId::new(name).unwrap(), 2).unwrap(),
            gap: None,
            escalation: Some(RepeatEscalation {
                gain_step: GainDb::new(step).unwrap(),
                zoom: None,
            }),
        };
        node
    };
    let mut inner = repeat("inner", "leaf", 3000);
    let order = if gain_first {
        vec![
            AudioTreatmentStage::ClipGain,
            AudioTreatmentStage::Saturation,
            AudioTreatmentStage::Equalizer,
        ]
    } else {
        vec![
            AudioTreatmentStage::Saturation,
            AudioTreatmentStage::Equalizer,
            AudioTreatmentStage::ClipGain,
        ]
    };
    inner.audio_treatments = AudioTreatments::with_equalizer_stages(
        order,
        Some(gain(-24000, false, false)),
        Some(Saturation::new(GainDb::new(9000).unwrap()).unwrap()),
        None,
        Some(
            EqualizerRecipe::new(
                [
                    GainDb::new(3000).unwrap(),
                    GainDb::UNITY,
                    GainDb::new(-3000).unwrap(),
                ],
                None,
                None,
            )
            .unwrap(),
        ),
    )
    .unwrap()
    .with_tail_sends(sends)
    .with_wet_sends(sends);
    let mut outer = repeat("outer", "inner", -3000);
    outer.audio_treatments =
        AudioTreatments::from_clip_gain(gain(6000, false, false)).with_tail_sends(sends);
    let mut root_node = BeatNode::sequence("Root", vec![id("outer")]);
    root_node.audio_treatments = AudioTreatments::from_clip_gain(gain(-6000, false, false));
    let nodes = BTreeMap::from([
        (id("leaf"), leaf),
        (id("inner"), inner),
        (id("outer"), outer),
        (id("root"), root_node),
    ]);
    let mut wire = serde_json::to_value(root).unwrap();
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    RenderPlan::compile(&document).unwrap()
}

pub(in crate::stages::shared_components) type Calls =
    Vec<(String, u8, usize, i64, Option<usize>, [bool; 2])>;

fn render(
    plan: &RenderPlan,
    send: bool,
    skipped: bool,
    stopped: bool,
) -> (MixedBlock, Calls, usize) {
    render_selection(plan, send, skipped, stopped, None, Permissions::default())
}

pub(in crate::stages::shared_components) fn render_selection(
    plan: &RenderPlan,
    send: bool,
    skipped: bool,
    stopped: bool,
    selected: Option<usize>,
    aliases: Permissions,
) -> (MixedBlock, Calls, usize) {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = work_control(&cancelled, &work);
    let query = plan
        .audio_gain_owners(AudioSample(12)..AudioSample(16), Default::default())
        .unwrap();
    assert_eq!(query.spans().len(), 1);
    let layers: Vec<_> = query.spans()[0]
        .owners()
        .iter()
        .cloned()
        .map(|clock| {
            let until = clock.treatments().map_or(0, |recipe| recipe.order().len());
            (clock, until)
        })
        .collect();
    assert_eq!(
        layers
            .iter()
            .map(|(c, _)| c.escalation_millidecibels())
            .sum::<i64>(),
        0
    );
    let root = &layers[0].0;
    let inner = layers
        .iter()
        .find(|(clock, _)| clock.instance().node == id("inner"))
        .unwrap();
    let stop = stopped.then(|| OwnerKey::new(&inner.0));
    let skip = if skipped {
        vec![authored_gain::identity(root)]
    } else {
        vec![]
    };
    let mut components =
        Components::new(AudioSample(12), 4, send.then_some(TailEffect::Delay), &skip);
    let original = components
        .route(
            (0, Contribution::Original),
            layers.clone(),
            0..4,
            stop.as_ref(),
            control,
        )
        .unwrap();
    let sound = components
        .route(
            (
                1,
                Contribution::Sound(
                    TailSends::default()
                        .with_level(TailEffect::Delay, Some(GainDb::new(-6000).unwrap())),
                ),
            ),
            layers,
            0..4,
            stop.as_ref(),
            control,
        )
        .unwrap();
    let reserved = components.resident_frames().unwrap();
    components.allocate();
    for at in 0..4 {
        if at != 1 {
            components
                .add(original, at, ([0.17, -0.23], [0.01, -0.02]), 0, control)
                .unwrap();
        }
        if at != 3 {
            components
                .add(sound, at, ([-0.31, 0.19], [0.02, 0.03]), 1500, control)
                .unwrap();
        }
    }
    let prepared = Program::prepare(
        &components.routes[sound],
        &components.owners,
        components.send,
        true,
        control,
    )
    .unwrap();
    let dynamic = components.routes[sound].layers.iter().any(|(clock, _)| {
        clock
            .treatments()
            .and_then(|r| r.clip_gain())
            .is_some_and(|g| !g.envelopes().is_empty())
    });
    assert_eq!(prepared.is_none(), dynamic);
    if let Some(program) = prepared {
        assert!(program.checkpoints.len() <= program.checkpoints.capacity());
        let lower = program.checkpoints.capacity()
            * authored_gain::PreparedTransfer::bytes(0).unwrap()
            + std::mem::size_of::<Option<Program>>();
        assert!(
            Program::resident_bytes(
                &components.routes[sound],
                &components.owners,
                components.send
            )
            .unwrap()
                >= lower
        );
        assert!(reserved * 8 >= lower as u64);
    }
    let mut calls = Vec::new();
    let block = components
        .finish(
            selected,
            aliases,
            control,
            |clock, temporal, sample, permissions, voice| {
                assert!(permissions.contains(0) || permissions.contains(1));
                calls.push((
                    clock.instance().node.as_str().into(),
                    match temporal {
                        Temporal::Equalizer(_) => 0,
                        Temporal::Wet(_) => 1,
                    },
                    match temporal {
                        Temporal::Equalizer(stage) => stage,
                        Temporal::Wet(TailEffect::Reverb) => 0,
                        Temporal::Wet(TailEffect::Delay) => 1,
                    },
                    sample.0,
                    voice,
                    [permissions.contains(0), permissions.contains(1)],
                ));
                let base = if temporal == Temporal::Wet(TailEffect::Delay) {
                    0.031
                } else {
                    -0.127
                };
                let value = base + voice.unwrap_or(0) as f64 * 0.013 + sample.0 as f64 * 0.001;
                Ok([value, -value])
            },
        )
        .unwrap();
    // Blocks may visit different samples in a different order. Within each
    // sample the exact stage, concrete clock, contribution and permission
    // sequence must remain unchanged. Stable sorting retains that sequence.
    calls.sort_by_key(|call| call.3);
    let spent = work.borrow().plan_work;
    (block, calls, spent)
}

#[test]
fn constant_blocks_preserve_selected_contributions_and_aliases() {
    for send in [false, true] {
        for selected in [Some(0), Some(1), Some(17)] {
            for alias in [None, Some(0), Some(1)] {
                let mut aliases = Permissions::default();
                if let Some(alias) = alias {
                    aliases.insert(alias).unwrap();
                }
                let (actual, calls, _) = render_selection(
                    &plan(false, false, true),
                    send,
                    false,
                    false,
                    selected,
                    aliases,
                );
                let (expected, expected_calls, _) = render_selection(
                    &plan(true, false, true),
                    send,
                    false,
                    false,
                    selected,
                    aliases,
                );
                assert_eq!(
                    actual
                        .samples
                        .iter()
                        .map(|s| s.map(f64::to_bits))
                        .collect::<Vec<_>>(),
                    expected
                        .samples
                        .iter()
                        .map(|s| s.map(f64::to_bits))
                        .collect::<Vec<_>>(),
                    "send={send} selected={selected:?} alias={alias:?}"
                );
                assert_eq!(actual.suppressed, expected.suppressed);
                assert_eq!(calls, expected_calls);
            }
        }
    }
}

#[test]
fn constant_routes_match_live_clocks_through_shared_saturation_eq_wet_and_sends() {
    for gain_first in [false, true] {
        for muted in [false, true] {
            let constant = plan(false, muted, gain_first);
            let dynamic = plan(true, muted, gain_first);
            for send in [false, true] {
                for (skip, stop) in [(false, false), (true, false), (false, true)] {
                    let (actual, calls, _) = render(&constant, send, skip, stop);
                    let (expected, expected_calls, _) = render(&dynamic, send, skip, stop);
                    assert_eq!(
                        actual
                            .samples
                            .iter()
                            .map(|frame| frame.map(f64::to_bits))
                            .collect::<Vec<_>>(),
                        expected
                            .samples
                            .iter()
                            .map(|frame| frame.map(f64::to_bits))
                            .collect::<Vec<_>>()
                    );
                    assert_eq!(actual.suppressed, expected.suppressed);
                    assert_eq!(calls, expected_calls);
                }
            }
        }
    }
}

#[test]
fn inactive_routes_do_not_prepare_checkpoints_or_consume_gain_work() {
    let plan = plan(false, false, true);
    let query = plan
        .audio_gain_owners(AudioSample(12)..AudioSample(16), Default::default())
        .unwrap();
    let layers = query.spans()[0]
        .owners()
        .iter()
        .cloned()
        .map(|clock| {
            let until = clock.treatments().map_or(0, |r| r.order().len());
            (clock, until)
        })
        .collect();
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = work_control(&cancelled, &work);
    let mut components = Components::new(AudioSample(12), 4, None, &[]);
    let id = components
        .route((0, Contribution::Original), layers, 0..4, None, control)
        .unwrap();
    let before = work.borrow().plan_work;
    assert!(
        Program::prepare(
            &components.routes[id],
            &components.owners,
            None,
            true,
            control
        )
        .unwrap()
        .is_none()
    );
    assert_eq!(before, work.borrow().plan_work);
    components.allocate();
    let block = components
        .finish(None, Permissions::default(), control, |_, _, _, _, _| {
            panic!("inactive temporal stage")
        })
        .unwrap();
    assert_eq!(block.samples, vec![[0.0; 2]; 4]);
    assert_eq!(block.suppressed, vec![AudioSample(12)..AudioSample(16)]);
}

#[test]
fn unity_block_omits_the_transfer_pass_under_the_unchanged_work_limit() {
    const FRAMES: u32 = 1024;
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = work_control(&cancelled, &work);
    let mut components = Components::new(AudioSample(0), FRAMES, None, &[]);
    let route = components
        .route(
            (0, Contribution::Original),
            vec![],
            0..FRAMES as usize,
            None,
            control,
        )
        .unwrap();
    components.allocate();
    for at in 0..FRAMES as usize {
        components
            .add(route, at, ([0.125, -0.375], [0.0; 2]), 0, control)
            .unwrap();
    }
    // The output must still be mixed, but no transfer exists to dispatch at
    // each sample. A scalar identity-check at every checkpoint fails here.
    work.borrow_mut().plan_work = MAX_PLAN_WORK_PER_READ - 32;
    let actual = components
        .finish(None, Permissions::default(), control, |_, _, _, _, _| {
            panic!("unity input has no temporal effect")
        })
        .unwrap();
    assert_eq!(actual.samples, vec![[0.125, -0.375]; FRAMES as usize]);
    assert!(actual.suppressed.is_empty());
    assert!(work.borrow().plan_work <= MAX_PLAN_WORK_PER_READ);
}

#[test]
fn unity_block_admission_rejects_nonfinite_input() {
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = work_control(&cancelled, &work);
    let mut components = Components::new(AudioSample(0), 1, None, &[]);
    let route = components
        .route((0, Contribution::Original), vec![], 0..1, None, control)
        .unwrap();
    components.allocate();
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(matches!(
            components.add(route, 0, ([bad, 0.0], [0.0; 2]), 0, control),
            Err(StageAudioError::Preparation(
                PreparationError::InvalidSamples
            ))
        ));
    }
}

#[test]
fn constant_owner_chain_does_real_preparation_once_instead_of_per_sample_appends() {
    const FRAMES: u32 = 256;
    const DEPTH: usize = deadpan_core::MAX_GAIN_LAYERS;
    let mut document = serde_json::to_value(
        ProjectDocument::new(
            ProjectId::new("linear-program").unwrap(),
            RevisionId::new("revision").unwrap(),
            PresentationBasis {
                width: 16,
                height: 16,
                frame_rate: FrameRate::new(48_000, 1).unwrap(),
                color_policy: ColorPolicy::SdrRec709,
            },
            id("root"),
        )
        .unwrap(),
    )
    .unwrap();
    let leaf = BeatNode::hold(
        "Leaf",
        HoldRecipe {
            picture_context: None,
            duration: FrameDuration::new(i64::from(FRAMES)).unwrap(),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    );
    let mut nodes = BTreeMap::from([(id("leaf"), leaf)]);
    let mut child = id("leaf");
    for index in 0..DEPTH {
        let key = if index + 1 == DEPTH {
            id("root")
        } else {
            id(&format!("group-{index}"))
        };
        let mut node = BeatNode::sequence("Same label", vec![child]);
        node.audio_treatments = AudioTreatments::from_clip_gain(gain(
            if index % 2 == 0 { 24000 } else { -24000 },
            false,
            false,
        ));
        nodes.insert(key.clone(), node);
        child = key;
    }
    document["nodes"] = serde_json::to_value(nodes).unwrap();
    let document = ProjectDocument::from_json(&document.to_string()).unwrap();
    let plan = RenderPlan::compile(&document).unwrap();
    let query = plan
        .audio_gain_owners(
            AudioSample(0)..AudioSample(i64::from(FRAMES)),
            Default::default(),
        )
        .unwrap();
    let owners = query.spans()[0].owners();
    assert_eq!(owners.len(), DEPTH + 1);
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = work_control(&cancelled, &work);
    let layers = owners
        .iter()
        .cloned()
        .map(|clock| {
            let until = clock.treatments().map_or(0, |r| r.order().len());
            (clock, until)
        })
        .collect();
    let mut components = Components::new(AudioSample(0), FRAMES, None, &[]);
    let route = components
        .route(
            (0, Contribution::Sound(Default::default())),
            layers,
            0..FRAMES as usize,
            None,
            control,
        )
        .unwrap();
    components.allocate();
    for at in 0..FRAMES as usize {
        components
            .add(route, at, ([0.125, -0.375], [0.0; 2]), 1500, control)
            .unwrap();
    }
    let actual = components
        .finish(None, Permissions::default(), control, |_, _, _, _, _| {
            panic!("memoryless chain")
        })
        .unwrap();
    let compiled_work = work.borrow().plan_work;
    let live_work = RefCell::new(ReadWork::default());
    let live_control = work_control(&cancelled, &live_work);
    for (at, frame) in actual.samples.iter().enumerate() {
        let expected = authored_gain::beat_sound_gain(
            owners,
            AudioSample(at as i64),
            1500,
            live_control,
            true,
        )
        .unwrap()
        .apply_wide([0.125, -0.375])
        .unwrap();
        assert_eq!(frame.map(f64::to_bits), expected.map(f64::to_bits));
    }
    // The legacy evaluator actually traversed each recipe at every sample.
    // This bound includes real compilation plus every new transfer application.
    assert!(compiled_work < FRAMES as usize * 2, "{compiled_work}");
    assert!(live_work.borrow().plan_work >= FRAMES as usize * DEPTH);
}
