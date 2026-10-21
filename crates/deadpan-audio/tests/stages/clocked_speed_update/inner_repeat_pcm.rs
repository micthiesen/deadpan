//! Each physical Repeat carrier is prepared separately before the literal
//! retained-clock transfer. A resampled repeated bus is not this oracle.
use super::*;

fn changed(document: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
    let transaction = apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new(name).unwrap(),
            command,
        },
    )
    .unwrap();
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    ProjectDocument::from_json(&after.to_json().unwrap()).unwrap()
}

fn repeat_fixture() -> (ProjectDocument, [IterationId; 3]) {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let spoken = |frames| {
        let mut node = source(rate, frames, 0..1);
        let NodeKind::Source { source } = &mut node.kind else {
            unreachable!()
        };
        let audio = audio_at_rate(3100, 23_100, 44_100);
        source.audio_mapping = SourceAudioMapping::natural_rate(audio.span, rate).unwrap();
        source.audio = Some(audio);
        node
    };
    let room = |frames| room_tone(frames, audio_at_rate(3100, 3260, 44_100));
    let iterations = IterationOrder::new(RevisionId::new("repeat-plays").unwrap(), 3).unwrap();
    let plays = [
        iterations.at(0).unwrap(),
        iterations.at(1).unwrap(),
        iterations.at(2).unwrap(),
    ];
    let mut repeated = BeatNode::sequence("Repeat", vec![]);
    repeated.kind = NodeKind::Repeat {
        child: id("default-body"),
        iterations,
        gap: Some(HoldRecipe {
            picture_context: None,
            duration: duration(1),
            video: HoldVideo::Background,
            audio: HoldAudio::RoomTone {
                source: audio_at_rate(3100, 3260, 44_100),
            },
        }),
        escalation: None,
    };
    let initial = document_with_asset(
        rate,
        &["a", "group", "b"],
        [
            ("a", hold(2)),
            (
                "group",
                BeatNode::sequence("Group", vec![id("lead"), id("repeat"), id("tail")]),
            ),
            ("lead", hold(1)),
            ("tail", hold(1)),
            ("repeat", repeated),
            (
                "default-body",
                BeatNode::sequence("Default", vec![id("default-source"), id("default-room")]),
            ),
            ("default-source", spoken(2)),
            ("default-room", room(1)),
            ("b", hold(2)),
        ],
        BTreeMap::new(),
        audio_at_rate(0, 44_117, 44_100).span,
    );
    let owned = changed(
        &initial,
        "repeat-owned-middle",
        Command::SetPlayOverride {
            node: id("repeat"),
            iteration: plays[1].clone(),
            subtree: Subtree {
                root: id("owned-body"),
                nodes: BTreeMap::from([
                    (
                        id("owned-body"),
                        BeatNode::sequence("Owned", vec![id("owned-source"), id("owned-room")]),
                    ),
                    (id("owned-source"), spoken(3)),
                    (id("owned-room"), room(1)),
                ]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    );
    let grouped = changed(
        &owned,
        "repeat-owned-gap",
        Command::SetGapOverride {
            node: id("repeat"),
            iteration: plays[0].clone(),
            subtree: Subtree {
                root: id("owned-gap"),
                nodes: BTreeMap::from([(id("owned-gap"), room(2))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    );
    assert_eq!(grouped.durations().unwrap()[&id("group")], duration(15));
    let wrapped = changed(
        &grouped,
        "repeat-wrap-speed",
        Command::WrapRetime {
            node: id("group"),
            id: id("slow"),
            duration: duration(30),
            pitch: PitchPolicy::FollowSpeed,
        },
    );
    let before = insert_pause(&wrapped, 3, 3, "repeat-exact-pause");
    assert_eq!(before.derived_clocks()[&id("slow")].extent, ratio(33, 1));
    assert_eq!(before.derived_clocks()[&id("slow")].child_end, ratio(33, 2));
    assert_eq!(before.duration().unwrap(), duration(37));
    (before, plays)
}

struct Carrier {
    owner: &'static str,
    play: usize,
    gap: bool,
    old: Range<i64>,
    before: Range<i64>,
    after: Range<i64>,
}

fn carriers() -> [Carrier; 8] {
    // 48000Hz / (30000/1001fps) = 8008/5 samples/frame.
    // Old root origins4,8,10,14,20,22,24,28 have scale2. The exact pause
    // shifts each current origin by3. Updating33->18 maps current root x
    // to 2+(x-2)*6/11. Round each absolute boundary once, ties to even.
    [
        Carrier {
            owner: "default-source",
            play: 0,
            gap: false,
            old: 6406..12813,
            before: 11211..17618,
            after: 7571..11066,
        },
        Carrier {
            owner: "default-room",
            play: 0,
            gap: false,
            old: 12813..16016,
            before: 17618..20821,
            after: 11066..12813,
        },
        Carrier {
            owner: "owned-gap",
            play: 0,
            gap: false,
            old: 16016..22422,
            before: 20821..27227,
            after: 12813..16307,
        },
        Carrier {
            owner: "owned-source",
            play: 1,
            gap: false,
            old: 22422..32032,
            before: 27227..36837,
            after: 16307..21549,
        },
        Carrier {
            owner: "owned-room",
            play: 1,
            gap: false,
            old: 32032..35235,
            before: 36837..40040,
            after: 21549..23296,
        },
        Carrier {
            owner: "repeat",
            play: 1,
            gap: true,
            old: 35235..38438,
            before: 40040..43243,
            after: 23296..25043,
        },
        Carrier {
            owner: "default-source",
            play: 2,
            gap: false,
            old: 38438..44845,
            before: 43243..49650,
            after: 25043..28538,
        },
        Carrier {
            owner: "default-room",
            play: 2,
            gap: false,
            old: 44845..48048,
            before: 49650..52853,
            after: 28538..30285,
        },
    ]
}

fn bound<'a>(
    plan: &'a RenderPlan,
    spec: &Carrier,
    live: &Range<i64>,
    plays: &[IterationId; 3],
    step: ExactRatio,
) -> deadpan_plan::AudioBound<'a> {
    let query = plan
        .audio_processing(
            AudioSample(live.start)..AudioSample(live.start + 1),
            Default::default(),
        )
        .unwrap();
    assert_eq!(query.spans.len(), 1, "{} play{}", spec.owner, spec.play);
    let span = &query.spans[0];
    assert_eq!(
        span.allocated_samples,
        AudioSample(live.start)..AudioSample(live.end)
    );
    assert_eq!(span.instance.node, id(spec.owner));
    if !spec.gap {
        assert_eq!(
            span.instance.repeats.last().unwrap().iteration,
            plays[spec.play]
        );
    }
    let AudioSignalContent::Bound(bound) = &span.content else {
        panic!("physical retained carrier")
    };
    assert_eq!(
        bound.reference_at_offset(0).unwrap(),
        ratio(spec.old.start.into(), 1)
    );
    assert_eq!(bound.reference_samples_per_output_sample(), step);
    let AudioBoundDomain::Root(domain) = bound.raw_domain().unwrap() else {
        panic!("physical root carrier")
    };
    assert_eq!(
        domain.root_samples(),
        AudioSample(spec.old.start)..AudioSample(spec.old.end)
    );
    if spec.gap {
        assert_eq!(domain.gap_after(), Some(&plays[spec.play]));
    }
    bound.as_ref().clone()
}

fn raw_carrier(
    plan: &Arc<RenderPlan>,
    provider: &mut FixtureProvider,
    spec: &Carrier,
    plays: &[IterationId; 3],
) -> Recorded {
    let binding = bound(plan, spec, &spec.before, plays, ExactRatio::ONE);
    let AudioBoundDomain::Root(domain) = binding.raw_domain().unwrap() else {
        unreachable!()
    };
    let mut renderer = StageAudio::new(Arc::clone(plan));
    let mut output = Recorded {
        samples: vec![],
        suppressed: vec![],
    };
    while output.samples.len() < (spec.old.end - spec.old.start) as usize {
        let at = spec.old.start + output.samples.len() as i64;
        let count = (spec.old.end - at).min(239) as u32;
        let block = renderer
            .read_domain(
                provider,
                &domain,
                AudioSample(at),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        output
            .suppressed
            .extend((at..at + i64::from(count)).map(|at| {
                block
                    .suppressed
                    .iter()
                    .any(|range| range.contains(&AudioSample(at)))
            }));
        output.samples.extend(block.samples);
    }
    assert!(
        output
            .samples
            .iter()
            .flatten()
            .any(|value| value.abs() > 0.001),
        "{} must carry known native fixture audio",
        spec.owner
    );
    output
}

fn transferred(spec: &Carrier, raw: &Recorded, live: Range<i64>, step: ExactRatio) -> Recorded {
    let recipe = RootSignalTransfer::new(
        AudioSample(spec.old.start)..AudioSample(spec.old.end),
        ratio(spec.old.start.into(), 1),
        SignalSample(live.start),
        step,
        SignalSample(live.start)..SignalSample(live.end),
    )
    .unwrap();
    let mut output = Recorded {
        samples: vec![],
        suppressed: vec![],
    };
    while output.samples.len() < (live.end - live.start) as usize {
        let at = live.start + output.samples.len() as i64;
        let count = (live.end - at).min(127) as u32;
        let block = recipe
            .render::<SignalTransferError>(
                SignalSample(at),
                count,
                &AtomicBool::new(false),
                |start, count| {
                    let offset = (start.0 - spec.old.start) as usize;
                    Ok(RootSignalBlock {
                        start,
                        samples: raw.samples[offset..offset + count as usize].to_vec(),
                        suppressed: (0..count as usize)
                            .filter(|index| raw.suppressed[offset + index])
                            .map(|index| {
                                AudioSample(start.0 + index as i64)
                                    ..AudioSample(start.0 + index as i64 + 1)
                            })
                            .collect(),
                    })
                },
            )
            .unwrap();
        output
            .suppressed
            .extend((at..at + i64::from(count)).map(|at| {
                block
                    .suppressed
                    .iter()
                    .any(|range| range.contains(&SignalSample(at)))
            }));
        output.samples.extend(block.samples);
    }
    output
}

fn read_window(
    document: &ProjectDocument,
    provider: &mut FixtureProvider,
    window: Range<i64>,
    chunks: &[u32],
) -> Recorded {
    provider.revisions.insert(document.revision_id().clone());
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(document).unwrap()));
    let mut output = Recorded {
        samples: vec![],
        suppressed: vec![],
    };
    for requested in chunks.iter().copied().cycle() {
        let at = window.start + output.samples.len() as i64;
        if at == window.end {
            break;
        }
        let count = requested.min((window.end - at) as u32);
        let block = renderer
            .read(
                provider,
                AudioSample(at),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        output
            .suppressed
            .extend((at..at + i64::from(count)).map(|at| {
                block
                    .suppressed
                    .iter()
                    .any(|range| range.contains(&AudioSample(at)))
            }));
        output.samples.extend(block.samples);
    }
    output
}

fn same_inner_bindings(before: &ProjectDocument, after: &ProjectDocument) {
    for owner in [
        "default-source",
        "default-room",
        "owned-source",
        "owned-room",
        "owned-gap",
    ] {
        assert_eq!(
            after.audio_bindings().bindings()[&id(owner)],
            before.audio_bindings().bindings()[&id(owner)]
        );
    }
    assert_eq!(
        after.audio_bindings().gap_bindings()[&id("repeat")],
        before.audio_bindings().gap_bindings()[&id("repeat")]
    );
}

#[test]
fn clocked_speed_update_inner_repeat_transfers_each_default_owned_and_gap_carrier() {
    let (before, plays) = repeat_fixture();
    let (_, mut provider) = mono_original(2);
    provider.revisions.insert(before.revision_id().clone());
    let plan = Arc::new(RenderPlan::compile(&before).unwrap());
    let specs = carriers();
    // These are eight separately resolved physical recipes. Sources use
    // natural-rate3100..23100@44100 input; room tone loops3100..3260@44100.
    // read_domain bypasses only each carrier's own retained binding, so native
    // preparation and the following literal11/6 transfer remain separate.
    let raw: Vec<_> = specs
        .iter()
        .map(|spec| raw_carrier(&plan, &mut provider, spec, &plays))
        .collect();
    let mut old_expected = Recorded {
        samples: vec![],
        suppressed: vec![],
    };
    let mut new_expected = Recorded {
        samples: vec![],
        suppressed: vec![],
    };
    for (spec, raw) in specs.iter().zip(&raw) {
        let old = transferred(spec, raw, spec.before.clone(), ExactRatio::ONE);
        let new = transferred(spec, raw, spec.after.clone(), ratio(11, 6));
        old_expected.samples.extend(old.samples);
        old_expected.suppressed.extend(old.suppressed);
        new_expected.samples.extend(new.samples);
        new_expected.suppressed.extend(new.suppressed);
    }
    same(
        &read_window(&before, &mut provider, 11211..52853, &[251, 3]),
        &old_expected,
        "separate physical pre-update Repeat carriers",
    );
    let after = changed(
        &before,
        "repeat-speed18",
        Command::SetRetime {
            node: id("slow"),
            duration: duration(18),
            pitch: PitchPolicy::FollowSpeed,
        },
    );
    assert_eq!(after.duration().unwrap(), duration(22));
    same_inner_bindings(&before, &after);
    assert!(
        !after
            .audio_bindings()
            .bindings()
            .contains_key(&id("repeat"))
    );
    assert_eq!(after.nodes()[&id("repeat")], before.nodes()[&id("repeat")]);
    assert_eq!(after.overrides(), before.overrides());
    assert_eq!(after.gap_overrides(), before.gap_overrides());
    let after_plan = RenderPlan::compile(&after).unwrap();
    for spec in &specs {
        bound(&after_plan, spec, &spec.after, &plays, ratio(11, 6));
    }
    same(
        &read_window(&after, &mut provider, 7571..30285, &[127, 3, 251]),
        &new_expected,
        "per-carrier11/6 transfer including every seam",
    );
    same(
        &read_window(&after, &mut provider, 7571..30285, &[1, 239]),
        &new_expected,
        "Repeat updated PCM/mask chunk parity",
    );
    let returned = changed(
        &after,
        "repeat-speed-return33",
        Command::SetRetime {
            node: id("slow"),
            duration: duration(33),
            pitch: PitchPolicy::FollowSpeed,
        },
    );
    same_inner_bindings(&before, &returned);
    same(
        &read_window(&returned, &mut provider, 11211..52853, &[131, 7]),
        &old_expected,
        "second public speed update preserves each physical clock",
    );
}
