use super::controlled_reads::{Provider, fixture_document_at_rate};
use super::*;
use deadpan_core::*;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn frames(value: i64) -> FrameDuration {
    FrameDuration::new(value).unwrap()
}
fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

fn source(rate: FrameRate, start: i64, end: i64, duration: i64) -> BeatNode {
    let audio = SourceAudio {
        asset: AssetId::new("media").unwrap(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: start,
                time_base: SourceTimeBase::new(1, 48_000).unwrap(),
            },
            SourceTimestamp {
                ticks: end,
                time_base: SourceTimeBase::new(1, 48_000).unwrap(),
            },
        )
        .unwrap(),
    };
    BeatNode {
        framing: None,
        label: "Source".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                duration: frames(duration),
                video: SourceVideo::Blank,
                video_mapping: SourceVideoMapping::FitBeat,
                audio_mapping: SourceAudioMapping::natural_rate(audio.span, rate).unwrap(),
                audio: Some(audio),
                audio_offset: AudioSample(0),
                link: LinkRelation::Independent,
            },
        },
    }
}

fn hold(duration: i64) -> BeatNode {
    BeatNode::hold(
        "Silence",
        HoldRecipe {
            duration: frames(duration),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}

fn preserve(child: &str, selected: i64, duration: i64) -> BeatNode {
    BeatNode {
        framing: None,
        label: "Preserve".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id(child),
            duration: frames(duration),
            mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(selected)).unwrap(),
            pitch: PitchPolicy::Preserve,
            purpose: RetimePurpose::Edit,
        },
    }
}

fn document(
    rate: FrameRate,
    children: &[&str],
    nodes: Vec<(&str, BeatNode)>,
) -> (ProjectDocument, Provider) {
    let (base, provider) = fixture_document_at_rate(rate);
    let mut wire = serde_json::to_value(base).unwrap();
    let mut nodes = nodes
        .into_iter()
        .map(|(key, node)| (id(key), node))
        .collect::<BTreeMap<_, _>>();
    nodes.insert(
        id("root"),
        BeatNode::sequence("Root", children.iter().map(|key| id(key)).collect()),
    );
    wire["nodes"] = serde_json::to_value(nodes).unwrap();
    (
        ProjectDocument::from_json(&wire.to_string()).unwrap(),
        provider,
    )
}

fn lattice(owner: &str, clock: AudioClockRoot) -> AudioPlacementTemplate {
    AudioPlacementTemplate {
        reference: AudioReferenceClock {
            timing: AudioTimingId {
                allocation: revision("timing"),
                ordinal: 0,
            },
            root: clock,
            physical: id(owner),
        },
        arguments: vec![],
        births: vec![],
    }
}

fn bind(
    old: &ProjectDocument,
    current: &ProjectDocument,
    bindings: Vec<(&str, AudioPlacementTemplate, Option<AudioResume>)>,
) -> ProjectDocument {
    let state = AudioBindingState::new(
        vec![AudioTimingRecord {
            id: AudioTimingId {
                allocation: revision("timing"),
                ordinal: 0,
            },
            layout: FrozenAudioLayout::capture(old).unwrap(),
        }],
        bindings
            .into_iter()
            .map(|(owner, lattice, resume)| (id(owner), OwnedAudioBinding { lattice, resume }))
            .collect(),
    )
    .unwrap();
    let mut wire = serde_json::to_value(current).unwrap();
    wire["audio_bindings"] = serde_json::to_value(state).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn render(
    renderer: &mut StageAudio,
    provider: &mut Provider,
    start: i64,
    count: u32,
) -> TimeMappedBlock {
    renderer
        .read(
            provider,
            AudioSample(start),
            count,
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )
        .unwrap()
}

fn stretch(input: &[[f32; 2]], output: u32, numerator: u64, denominator: u64) -> Vec<[f32; 2]> {
    let input = StereoPcm::new(
        input.iter().map(|v| v[0]).collect(),
        input.iter().map(|v| v[1]).collect(),
    )
    .unwrap();
    let recipe = CanonicalRecipe::with_rate(
        input.frames(),
        output,
        StretchRate::new(numerator, denominator).unwrap(),
        0,
    )
    .unwrap();
    let mut dsp = CanonicalStretch::new(recipe, input).unwrap();
    let mut left = vec![0.; output as usize];
    let mut right = vec![0.; output as usize];
    for (left, right) in left
        .chunks_mut(MAX_OUTPUT_FRAMES as usize)
        .zip(right.chunks_mut(MAX_OUTPUT_FRAMES as usize))
    {
        assert_eq!(
            dsp.read(left, right, &AtomicBool::new(false)).unwrap(),
            left.len()
        );
    }
    left.into_iter().zip(right).map(|(l, r)| [l, r]).collect()
}

fn edit(document: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
    let applied = apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision(name),
            command,
        },
    )
    .unwrap();
    let changed = applied.forward.apply(document).unwrap();
    assert_eq!(applied.inverse.apply(&changed).unwrap(), *document);
    changed
}

fn decoded(provider: &Provider, selected: Range<i64>) -> Vec<[f32; 2]> {
    let count = u32::try_from(selected.end - selected.start).unwrap();
    provider
        .prepared
        .prepare(
            ResampleRecipe::new(
                selected.clone(),
                ExactRatio::integer(selected.start),
                AudioSample(0),
                ExactRatio::ONE,
                AudioSample(0)..AudioSample(i64::from(count)),
            )
            .unwrap(),
            AudioSample(0),
            count,
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples
}

#[test]
fn normal_root_binding_retains_fractional_ntsc_source_phase_and_block_parity() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let (old, mut provider) = document(
        rate,
        &["prefix", "a"],
        vec![("prefix", hold(1)), ("a", source(rate, 0, 6406, 4))],
    );
    let mut wire = serde_json::to_value(&old).unwrap();
    wire["nodes"]["prefix"] = serde_json::to_value(hold(2)).unwrap();
    let current = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let bound = bind(
        &old,
        &current,
        vec![(
            "a",
            lattice("a", AudioClockRoot::ProjectRootRoundEven),
            None,
        )],
    );
    let expected = provider
        .prepared
        .prepare(
            ResampleRecipe::new(
                0..6406,
                ratio(2, 5),
                AudioSample(0),
                ExactRatio::ONE,
                AudioSample(0)..AudioSample(256),
            )
            .unwrap(),
            AudioSample(0),
            256,
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples;
    let plan = Arc::new(RenderPlan::compile(&bound).unwrap());
    let mut renderer = StageAudio::new(plan);
    let full = render(&mut renderer, &mut provider, 3203, 256);
    assert_eq!(full.samples, expected);
    assert!(full.suppressed.is_empty());
    for (offset, count) in [(137, 119), (0, 61), (61, 76)] {
        assert_eq!(
            render(&mut renderer, &mut provider, 3203 + offset, count).samples,
            full.samples[offset as usize..offset as usize + count as usize]
        );
    }
    let mut unbound = StageAudio::new(Arc::new(RenderPlan::compile(&current).unwrap()));
    assert_ne!(
        render(&mut unbound, &mut provider, 3203, 256).samples,
        full.samples
    );

    let split = edit(
        &bound,
        "split",
        Command::Split {
            node: id("a"),
            at: frames(2),
            identities: SplitIdentities {
                nodes: (0..8).map(|n| id(&format!("cut-{n}"))).collect(),
            },
        },
    );
    let mut split_renderer = StageAudio::new(Arc::new(RenderPlan::compile(&split).unwrap()));
    for start in [3203, 6300, 6406, 9400] {
        let expected = renderer
            .read_edge_faded(
                &mut provider,
                AudioSample(start),
                128,
                Duration::from_secs(10),
                &AtomicBool::new(false),
            )
            .unwrap();
        let actual = split_renderer
            .read_edge_faded(
                &mut provider,
                AudioSample(start),
                128,
                Duration::from_secs(10),
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(
            actual.samples, expected.samples,
            "transparent bound partition at {start}"
        );
    }
}

#[test]
fn translated_binding_retains_two_sample_automatic_envelope() {
    let rate = FrameRate::new(32_000, 1).unwrap();
    let (old, mut provider) = document(rate, &["a"], vec![("a", source(rate, 0, 2, 1))]);
    let mut original = StageAudio::new(Arc::new(RenderPlan::compile(&old).unwrap()));
    let expected = original
        .read_edge_faded(
            &mut provider,
            AudioSample(0),
            1,
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(expected.samples, vec![[0.375, -0.5]]);
    let mut wire = serde_json::to_value(&old).unwrap();
    wire["nodes"]["prefix"] = serde_json::to_value(hold(1)).unwrap();
    wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Root", vec![id("prefix"), id("a")])).unwrap();
    let current = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let bound = bind(
        &old,
        &current,
        vec![(
            "a",
            lattice("a", AudioClockRoot::ProjectRootRoundEven),
            None,
        )],
    );
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(&bound).unwrap()));
    let actual = renderer
        .read_edge_faded(
            &mut provider,
            AudioSample(2),
            1,
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(
        actual.samples, expected.samples,
        "translation must not replace an intrinsic two-sample fade with a one-sample fade"
    );
}

#[test]
fn resumed_binding_retains_old_envelope_progress_and_exhaustion() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let (old, mut provider) = document(rate, &["a"], vec![("a", source(rate, 0, 400, 400))]);
    let bound = bind(
        &old,
        &old,
        vec![(
            "a",
            lattice("a", AudioClockRoot::ProjectRootRoundEven),
            Some(AudioResume {
                local_boundary: ExactRatio::ZERO,
                phase: AudioLocalPhase {
                    constant: ExactRatio::integer(64),
                    terms: vec![],
                },
            }),
        )],
    );
    let mut original = StageAudio::new(Arc::new(RenderPlan::compile(&old).unwrap()));
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(&bound).unwrap()));
    let expected = original
        .read_edge_faded(
            &mut provider,
            AudioSample(64),
            128,
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )
        .unwrap();
    let actual = renderer
        .read_edge_faded(
            &mut provider,
            AudioSample(0),
            128,
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(
        actual.samples[0], expected.samples[0],
        "resume transports existing envelope progress, not just PCM phase"
    );
    assert_eq!(actual.samples, expected.samples);
    let mut expected = original
        .read_edge_faded(
            &mut provider,
            AudioSample(384),
            16,
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples;
    expected.extend(vec![[0.; 2]; 16]);
    let actual = renderer
        .read_edge_faded(
            &mut provider,
            AudioSample(320),
            32,
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(actual.samples, expected);
    assert_eq!(actual.suppressed, vec![AudioSample(336)..AudioSample(352)]);
}

#[test]
fn capture_only_preserve_keeps_the_consuming_output_envelope() {
    let rate = FrameRate::new(32_000, 1).unwrap();
    let (old, mut provider) = document(
        rate,
        &["stage"],
        vec![("a", source(rate, 0, 2, 1)), ("stage", preserve("a", 1, 2))],
    );
    let mut before = StageAudio::new(Arc::new(RenderPlan::compile(&old).unwrap()));
    let expected = before
        .read_edge_faded(
            &mut provider,
            AudioSample(0),
            3,
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )
        .unwrap();
    for bindings in [
        vec![(
            "a",
            lattice(
                "a",
                AudioClockRoot::PreserveInputPointCeil { stage: id("stage") },
            ),
            None,
        )],
        vec![(
            "stage",
            lattice("stage", AudioClockRoot::ProjectRootRoundEven),
            None,
        )],
        vec![
            (
                "a",
                lattice(
                    "a",
                    AudioClockRoot::PreserveInputPointCeil { stage: id("stage") },
                ),
                None,
            ),
            (
                "stage",
                lattice("stage", AudioClockRoot::ProjectRootRoundEven),
                None,
            ),
        ],
    ] {
        let bound = bind(&old, &old, bindings);
        let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(&bound).unwrap()));
        let actual = renderer
            .read_edge_faded(
                &mut provider,
                AudioSample(0),
                3,
                Duration::from_secs(10),
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(
            actual.samples, expected.samples,
            "capture cannot substitute the two-point input envelope for the three-point output envelope"
        );
    }
}

#[test]
fn captured_source_keeps_current_coincident_ancestor_hard_edges() {
    let rate = FrameRate::new(32_000, 1).unwrap();
    let (old, mut provider) = document(rate, &["a"], vec![("a", source(rate, 0, 2, 1))]);
    let mut wire = serde_json::to_value(&old).unwrap();
    let mut root = old.nodes()[&id("root")].clone();
    root.audio_edges.node_start = AudioEdgePolicy::Hard;
    root.audio_edges.node_end = AudioEdgePolicy::Hard;
    wire["nodes"]["root"] = serde_json::to_value(root).unwrap();
    let current = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let bound = bind(
        &old,
        &current,
        vec![(
            "a",
            lattice("a", AudioClockRoot::ProjectRootRoundEven),
            None,
        )],
    );
    let expected = decoded(&provider, 0..2);
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(&bound).unwrap()));
    let actual = renderer
        .read_edge_faded(
            &mut provider,
            AudioSample(0),
            2,
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(
        actual.samples, expected,
        "the current Sequence owns coincident Hard boundaries outside the raw Source"
    );
}

#[test]
fn fresh_follow_speed_keeps_two_millisecond_post_mapping_fades() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let (old, mut provider) = document(rate, &["a"], vec![("a", source(rate, 0, 400, 400))]);
    for output in [200, 800] {
        let mut wrapper = preserve("a", 400, output);
        let NodeKind::Retime { pitch, .. } = &mut wrapper.kind else {
            unreachable!()
        };
        *pitch = PitchPolicy::FollowSpeed;
        let mut wire = serde_json::to_value(&old).unwrap();
        wire["nodes"]["speed"] = serde_json::to_value(wrapper).unwrap();
        wire["nodes"]["root"] =
            serde_json::to_value(BeatNode::sequence("Root", vec![id("speed")])).unwrap();
        let current = ProjectDocument::from_json(&wire.to_string()).unwrap();
        let bound = bind(
            &old,
            &current,
            vec![(
                "a",
                lattice("a", AudioClockRoot::ProjectRootRoundEven),
                None,
            )],
        );
        let mut before = StageAudio::new(Arc::new(RenderPlan::compile(&current).unwrap()));
        let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(&bound).unwrap()));
        for start in [0, 80, output - 128] {
            let expected = before
                .read_edge_faded(
                    &mut provider,
                    AudioSample(start),
                    128.min((output - start) as u32),
                    Duration::from_secs(10),
                    &AtomicBool::new(false),
                )
                .unwrap();
            let actual = renderer
                .read_edge_faded(
                    &mut provider,
                    AudioSample(start),
                    128.min((output - start) as u32),
                    Duration::from_secs(10),
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert_eq!(
                actual.samples, expected.samples,
                "fresh rate {output}/400 must retain a 96-output-sample maximum fade"
            );
        }
    }
}

#[test]
fn bound_hidden_negative_support_is_not_capped_to_the_old_project() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let partition = BeatNode {
        framing: None,
        label: "Hidden start".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id("a"),
            duration: frames(3),
            mapping: FrameRange::new(ProjectFrame(1), ProjectFrame(4)).unwrap(),
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Partition,
        },
    };
    let (old, mut provider) = document(
        rate,
        &["partition"],
        vec![("a", source(rate, 0, 6406, 4)), ("partition", partition)],
    );
    let mut wire = serde_json::to_value(&old).unwrap();
    wire["nodes"].as_object_mut().unwrap().remove("partition");
    wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Root", vec![id("a")])).unwrap();
    let current = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let bound = bind(
        &old,
        &current,
        vec![(
            "a",
            lattice("a", AudioClockRoot::ProjectRootRoundEven),
            None,
        )],
    );
    let plan = Arc::new(RenderPlan::compile(&bound).unwrap());
    let query = plan
        .audio_processing(AudioSample(0)..AudioSample(1), Default::default())
        .unwrap();
    let AudioSignalContent::Bound(binding) = &query.spans[0].content else {
        panic!("expected bound Source")
    };
    let AudioBoundDomain::Root(raw) = binding.raw_domain().unwrap() else {
        panic!("expected root operand")
    };
    assert_eq!(raw.root_samples().start, AudioSample(-1602));
    let expected = provider
        .prepared
        .prepare(
            ResampleRecipe::new(
                0..6406,
                ratio(-2, 5),
                AudioSample(0),
                ExactRatio::ONE,
                AudioSample(0)..AudioSample(128),
            )
            .unwrap(),
            AudioSample(0),
            128,
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples;
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let actual = render(&mut renderer, &mut provider, 0, 128);
    assert_eq!(actual.samples, expected);
    assert!(actual.suppressed.is_empty());
}

#[test]
fn new_ordinary_edit_crop_excludes_filter_context_from_bound_raw_source() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let (old, mut provider) = document(
        rate,
        &["prefix", "a"],
        vec![("prefix", hold(1)), ("a", source(rate, 0, 6406, 4))],
    );
    let crop = BeatNode {
        framing: None,
        label: "Authored trim".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id("a"),
            duration: frames(1),
            mapping: FrameRange::new(ProjectFrame(1), ProjectFrame(2)).unwrap(),
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Edit,
        },
    };
    let mut wire = serde_json::to_value(&old).unwrap();
    wire["nodes"].as_object_mut().unwrap().remove("prefix");
    wire["nodes"]["crop"] = serde_json::to_value(crop).unwrap();
    wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Root", vec![id("crop")])).unwrap();
    let current = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let bound = bind(
        &old,
        &current,
        vec![(
            "a",
            lattice("a", AudioClockRoot::ProjectRootRoundEven),
            None,
        )],
    );
    let plan = Arc::new(RenderPlan::compile(&bound).unwrap());
    let query = plan
        .audio_processing(AudioSample(0)..AudioSample(1), Default::default())
        .unwrap();
    let AudioSignalContent::Bound(binding) = &query.spans[0].content else {
        panic!("expected bound Source")
    };
    assert_eq!(
        binding.reference_at_offset(0).unwrap(),
        ExactRatio::integer(3204)
    );
    // Retain the old PCM phase (1602.4), but the new authored trim limits
    // filter taps to ceil([1601.6,3203.2)) = [1602,3204).
    let expected = provider
        .prepared
        .prepare(
            ResampleRecipe::new(
                1602..3204,
                ratio(8012, 5),
                AudioSample(0),
                ExactRatio::ONE,
                AudioSample(0)..AudioSample(128),
            )
            .unwrap(),
            AudioSample(0),
            128,
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples;
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let actual = render(&mut renderer, &mut provider, 0, 128);
    assert_eq!(
        actual.samples[0], expected[0],
        "an authored Edit cannot admit excluded pre-trim sinc taps through a binding"
    );
    assert_eq!(actual.samples, expected);
}

#[test]
fn bound_source_retains_nonzero_point_origin_inside_current_preserve() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let mut old_stage = preserve("sequence", 2, 3);
    let NodeKind::Retime { mapping, .. } = &mut old_stage.kind else {
        unreachable!()
    };
    *mapping = FrameRange::new(ProjectFrame(1), ProjectFrame(3)).unwrap();
    let (old, mut provider) = document(
        rate,
        &["stage"],
        vec![
            ("prefix", hold(2)),
            ("a", source(rate, 0, 512, 1)),
            (
                "sequence",
                BeatNode::sequence("Words", vec![id("prefix"), id("a")]),
            ),
            ("stage", old_stage),
        ],
    );
    let mut wire = serde_json::to_value(&old).unwrap();
    wire["nodes"]["stage"] = serde_json::to_value(preserve("sequence", 3, 4)).unwrap();
    let current = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let bound = bind(
        &old,
        &current,
        vec![(
            "a",
            lattice(
                "a",
                AudioClockRoot::PreserveInputPointCeil { stage: id("stage") },
            ),
            None,
        )],
    );
    let resolved = bound
        .audio_bindings()
        .resolve(
            &id("a"),
            &InstancePath {
                node: id("a"),
                repeats: vec![],
            },
            10_000,
        )
        .unwrap();
    assert_eq!(resolved.lattice.grid_origin, ExactRatio::ONE);
    assert_eq!(
        resolved.lattice.sample_boundary(ExactRatio::ZERO).unwrap(),
        1602
    );
    let recipe = ResampleRecipe::new(
        0..512,
        ratio(2, 5),
        AudioSample(0),
        ExactRatio::ONE,
        AudioSample(0)..AudioSample(1601),
    )
    .unwrap();
    let mut input = vec![[0.; 2]; 3204];
    for start in (0..1601).step_by(MAX_OUTPUT_FRAMES as usize) {
        input.extend(
            provider
                .prepared
                .prepare(
                    recipe.clone(),
                    AudioSample(start),
                    u32::try_from(1601 - start).unwrap().min(MAX_OUTPUT_FRAMES),
                    Duration::from_secs(10),
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples,
        );
    }
    assert_eq!(input.len(), 4805);
    // The Source placement owns 512 points at this phase. Later raw points
    // are source absence, not permission to read the sinc filter's tail.
    input[3716..].fill([0.; 2]);
    let mut expected = stretch(&input, 6407, 3, 4);
    expected[..4271].fill([0.; 2]);
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(&bound).unwrap()));
    for start in [4260, 4511, 6200, 4300] {
        let block = render(&mut renderer, &mut provider, start, 128);
        assert_eq!(
            block.samples,
            expected[start as usize..start as usize + 128],
            "selected PointCeil origin at {start}"
        );
    }
}

#[test]
fn capture_only_zero_point_source_absence_preserves_processed_decay() {
    let rate = FrameRate::new(96_000, 1).unwrap();
    let mut absent = source(rate, 0, 1, 1);
    let NodeKind::Source {
        source: absent_source,
    } = &mut absent.kind
    else {
        unreachable!()
    };
    absent_source.audio = None;
    absent_source.audio_mapping = SourceAudioMapping::FitBeat;
    absent_source.video = SourceVideo::Still {
        asset: AssetId::new("media").unwrap(),
    };
    let (old, mut provider) = document(
        rate,
        &["stage"],
        vec![
            ("a", source(rate, 0, 1, 1)),
            ("absent", absent),
            ("b", source(rate, 1, 2, 1)),
            (
                "sequence",
                BeatNode::sequence("Words", vec![id("a"), id("absent"), id("b")]),
            ),
            ("stage", preserve("sequence", 3, 24)),
        ],
    );
    let bound = bind(
        &old,
        &old,
        vec![(
            "absent",
            lattice(
                "absent",
                AudioClockRoot::PreserveInputPointCeil { stage: id("stage") },
            ),
            None,
        )],
    );
    let resolved = bound
        .audio_bindings()
        .resolve(
            &id("absent"),
            &InstancePath {
                node: id("absent"),
                repeats: vec![],
            },
            10_000,
        )
        .unwrap();
    assert_eq!(
        resolved.lattice.sample_boundary(ExactRatio::ZERO).unwrap(),
        1
    );
    assert_eq!(
        resolved.lattice.sample_boundary(ExactRatio::ONE).unwrap(),
        1
    );
    let expected = stretch(&decoded(&provider, 0..2), 12, 1, 8);
    assert!(expected[4..8].iter().any(|sample| *sample != [0.; 2]));
    let mut original = StageAudio::new(Arc::new(RenderPlan::compile(&old).unwrap()));
    let original = render(&mut original, &mut provider, 0, 12);
    assert_eq!(original.samples, expected);
    assert!(original.suppressed.is_empty());
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(&bound).unwrap()));
    let actual = render(&mut renderer, &mut provider, 0, 12);
    assert_eq!(actual.samples, expected);
    assert!(actual.suppressed.is_empty());
    for (start, count) in [(8, 4), (4, 4), (0, 4)] {
        let block = render(&mut renderer, &mut provider, start, count);
        assert_eq!(
            block.samples,
            expected[start as usize..start as usize + count as usize]
        );
        assert!(block.suppressed.is_empty());
    }
}

#[test]
fn bound_hold_with_no_old_input_point_still_suppresses_new_preserve_output() {
    let rate = FrameRate::new(192_000, 1).unwrap();
    let (old, mut provider) = document(
        rate,
        &["inner"],
        vec![
            ("a", source(rate, 0, 1, 1)),
            ("silent", hold(1)),
            ("b", source(rate, 1, 2, 1)),
            (
                "sequence",
                BeatNode::sequence("Words", vec![id("a"), id("silent"), id("b")]),
            ),
            ("inner", preserve("sequence", 3, 24)),
        ],
    );
    let mut wire = serde_json::to_value(&old).unwrap();
    wire["nodes"]["outer"] = serde_json::to_value(preserve("inner", 24, 48)).unwrap();
    wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Root", vec![id("outer")])).unwrap();
    let current = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let bound = bind(
        &old,
        &current,
        vec![
            (
                "silent",
                lattice(
                    "silent",
                    AudioClockRoot::PreserveInputPointCeil { stage: id("inner") },
                ),
                None,
            ),
            (
                "inner",
                lattice("inner", AudioClockRoot::ProjectRootRoundEven),
                None,
            ),
        ],
    );
    let resolved = bound
        .audio_bindings()
        .resolve(
            &id("silent"),
            &InstancePath {
                node: id("silent"),
                repeats: vec![],
            },
            10_000,
        )
        .unwrap();
    assert_eq!(
        resolved.lattice.sample_boundary(ExactRatio::ZERO).unwrap(),
        1
    );
    assert_eq!(
        resolved.lattice.sample_boundary(ExactRatio::ONE).unwrap(),
        1
    );
    let mut inner = stretch(&[[0.75, -1.]], 6, 1, 8);
    inner[2..4].fill([0.; 2]);
    let mut expected = stretch(&inner, 12, 1, 2);
    expected[4..8].fill([0.; 2]);
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(&bound).unwrap()));
    let full = render(&mut renderer, &mut provider, 0, 12);
    assert_eq!(full.samples, expected);
    assert_eq!(full.suppressed, vec![AudioSample(4)..AudioSample(8)]);
    for (start, count) in [(8, 4), (0, 5), (5, 3)] {
        assert_eq!(
            render(&mut renderer, &mut provider, start, count).samples,
            expected[start as usize..start as usize + count as usize]
        );
    }
}

#[test]
fn cached_intrinsic_depth_is_readmitted_before_dependency_access() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let (doc, mut provider) = document(
        rate,
        &["outer"],
        vec![
            ("a", source(rate, 0, 128, 128)),
            ("inner", preserve("a", 128, 192)),
            ("outer", preserve("inner", 192, 256)),
        ],
    );
    let plan = Arc::new(RenderPlan::compile(&doc).unwrap());
    let mut renderer = StageAudio::with_limits(
        Arc::clone(&plan),
        StageLimits {
            maximum_depth: 2,
            ..Default::default()
        },
    )
    .unwrap();
    render(&mut renderer, &mut provider, 0, 128);
    let stage = plan
        .audio_processing(AudioSample(0)..AudioSample(1), Default::default())
        .unwrap()
        .spans
        .remove(0)
        .content;
    let AudioSignalContent::Stage(stage) = stage else {
        panic!("expected outer stage")
    };
    let work = RefCell::new(ReadWork::default());
    let cancelled = AtomicBool::new(false);
    let control = WorkControl {
        cancelled: &cancelled,
        deadline: Instant::now() + Duration::from_secs(10),
        work: &work,
    };
    let calls = provider.calls;
    assert!(matches!(
        renderer.prepare_stage(&stage, &mut provider, control, 2),
        Err(StageAudioError::Limit("nested stage depth"))
    ));
    assert_eq!(provider.calls, calls);
    assert_eq!(renderer.cached_stage_count(), 2);
}

#[test]
fn reordered_survivors_keep_old_phase_and_a_fresh_play_uses_definition_phase() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let repeat = BeatNode {
        framing: None,
        label: "Repeat".into(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: id("a"),
            iterations: IterationOrder::new(revision("plays"), 2).unwrap(),
            gap: None,
        },
    };
    let (old, mut provider) = document(
        rate,
        &["prefix", "repeat"],
        vec![
            ("prefix", hold(1)),
            ("a", source(rate, 0, 3203, 2)),
            ("repeat", repeat),
        ],
    );
    let mut template = lattice("a", AudioClockRoot::ProjectRootRoundEven);
    template.arguments.push(AudioRepeatArgument {
        reference_repeat: id("repeat"),
        value: AudioRepeatValue::Live {
            repeat: id("repeat"),
        },
    });
    template.births.push(AudioBirthClause {
        repeat: id("repeat"),
        survivors: AudioBirthSurvivors::CapturedRepeat {
            repeat: id("repeat"),
        },
        definition_root: id("a"),
    });
    let bound = bind(&old, &old, vec![("a", template, None)]);
    let moved = edit(
        &bound,
        "move",
        Command::MovePlays {
            node: id("repeat"),
            start: 1,
            end: 2,
            destination: 0,
        },
    );
    let grown = edit(
        &moved,
        "birth",
        Command::InsertPlays {
            node: id("repeat"),
            index: 0,
            count: 1,
        },
    );
    let plan = Arc::new(RenderPlan::compile(&grown).unwrap());
    let definition = plan
        .audio_definition(AudioDefinitionSelector::RepeatDefault {
            repeat: id("repeat"),
        })
        .unwrap();
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    for (start, phase) in [
        (1602, ExactRatio::ZERO),
        (4805, ratio(1, 5)),
        (8008, ratio(2, 5)),
    ] {
        let expected = provider
            .prepared
            .prepare(
                ResampleRecipe::new(
                    0..3203,
                    phase,
                    AudioSample(0),
                    ExactRatio::ONE,
                    AudioSample(0)..AudioSample(64),
                )
                .unwrap(),
                AudioSample(0),
                64,
                Duration::from_secs(10),
                &AtomicBool::new(false),
            )
            .unwrap()
            .samples;
        assert_eq!(
            render(&mut renderer, &mut provider, start, 64).samples,
            expected
        );
    }
    let fresh = render(&mut renderer, &mut provider, 1602, 64).samples;
    let canonical = renderer
        .read_definition(
            &mut provider,
            &definition,
            SignalSample(0),
            64,
            Duration::from_secs(10),
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(canonical.samples, fresh);
}

#[test]
fn bound_preserve_evaluates_changed_room_tone_without_old_silence() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let (old, mut provider) = document(
        rate,
        &["outer"],
        vec![
            ("a", source(rate, 0, 128, 128)),
            ("silent", hold(128)),
            ("b", source(rate, 256, 384, 128)),
            (
                "sequence",
                BeatNode::sequence("Words", vec![id("a"), id("silent"), id("b")]),
            ),
            ("inner", preserve("sequence", 384, 576)),
            ("outer", preserve("inner", 576, 768)),
        ],
    );
    let NodeKind::Source {
        source: room_source,
    } = source(rate, 1024, 1152, 128).kind
    else {
        unreachable!()
    };
    let mut changed_hold = hold(128);
    let NodeKind::Hold { recipe } = &mut changed_hold.kind else {
        unreachable!()
    };
    recipe.audio = HoldAudio::RoomTone {
        source: room_source.audio.unwrap(),
    };
    let mut wire = serde_json::to_value(&old).unwrap();
    wire["nodes"]["silent"] = serde_json::to_value(changed_hold).unwrap();
    wire["revision_id"] = serde_json::to_value(revision("room-policy")).unwrap();
    let changed = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let bound = bind(
        &old,
        &changed,
        vec![
            (
                "inner",
                lattice(
                    "inner",
                    AudioClockRoot::PreserveInputPointCeil { stage: id("outer") },
                ),
                None,
            ),
            (
                "silent",
                lattice(
                    "silent",
                    AudioClockRoot::PreserveInputPointCeil { stage: id("inner") },
                ),
                None,
            ),
        ],
    );
    let room_input = decoded(&provider, 1024..1152);
    let room = RoomTone::new(
        RoomToneRecipe::new(ExactRatio::integer(128), 128).unwrap(),
        &room_input,
        &AtomicBool::new(false),
    )
    .unwrap()
    .render(AudioSample(0), 128, &AtomicBool::new(false))
    .unwrap()
    .samples;
    let input: Vec<_> = decoded(&provider, 0..128)
        .into_iter()
        .chain(room)
        .chain(decoded(&provider, 256..384))
        .collect();
    let expected = stretch(&stretch(&input, 576, 2, 3), 768, 3, 4);
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(&bound).unwrap()));
    for start in [256, 0, 512, 256] {
        let block = render(&mut renderer, &mut provider, start, 256);
        assert_eq!(
            block.samples,
            expected[start as usize..start as usize + 256]
        );
        assert!(block.suppressed.is_empty());
    }
    assert!(
        expected[256..512]
            .iter()
            .flatten()
            .any(|value| *value != 0.)
    );
    assert_eq!(renderer.cached_stage_count(), 3);
}

#[test]
fn bound_reads_share_cancellation_work_and_complete_cached_dependencies() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let (old, mut provider) = document(
        rate,
        &["a", "stage"],
        vec![
            ("a", source(rate, 0, 128, 128)),
            ("b", source(rate, 128, 256, 128)),
            ("stage", preserve("b", 128, 192)),
        ],
    );
    let bound = bind(
        &old,
        &old,
        vec![
            (
                "stage",
                lattice("stage", AudioClockRoot::ProjectRootRoundEven),
                None,
            ),
            (
                "b",
                lattice(
                    "b",
                    AudioClockRoot::PreserveInputPointCeil { stage: id("stage") },
                ),
                None,
            ),
        ],
    );
    let plan = Arc::new(RenderPlan::compile(&bound).unwrap());
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let cancelled = AtomicBool::new(false);
    let work = RefCell::new(ReadWork::default());
    let control = WorkControl {
        cancelled: &cancelled,
        deadline: Instant::now() + Duration::from_secs(10),
        work: &work,
    };
    let first = renderer
        .read_controlled(&mut provider, AudioSample(0), 64, control, false, 0)
        .unwrap();
    for _ in 0..2 {
        let block = renderer
            .read_controlled(&mut provider, AudioSample(128), 192, control, false, 0)
            .unwrap();
        assert_eq!(block.dependencies, first.dependencies);
        assert_eq!(block.relative_depth, 3);
    }
    assert_eq!(renderer.cached_stage_count(), 1);
    assert_eq!(work.borrow().prepared_stages, 1);
    assert!(work.borrow().plan_work > 0);

    let calls = provider.calls;
    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(
        renderer
            .read_controlled(&mut provider, AudioSample(128), 64, control, false, 0)
            .err()
            .unwrap()
            .is_cancelled()
    );
    cancelled.store(false, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(provider.calls, calls);

    let exhausted = RefCell::new(ReadWork {
        plan_work: MAX_PLAN_WORK_PER_READ,
        ..Default::default()
    });
    assert!(matches!(
        renderer.read_controlled(
            &mut provider,
            AudioSample(128),
            64,
            WorkControl {
                work: &exhausted,
                ..control
            },
            false,
            0
        ),
        Err(StageAudioError::Limit("plan work per read"))
    ));
    assert_eq!(provider.calls, calls);

    let first_query_work = plan
        .audio(AudioSample(128)..AudioSample(192), Default::default())
        .unwrap()
        .work;
    assert!(first_query_work > 0);
    let first_only = RefCell::new(ReadWork {
        plan_work: MAX_PLAN_WORK_PER_READ - first_query_work,
        ..Default::default()
    });
    assert!(matches!(
        renderer.read_controlled(
            &mut provider,
            AudioSample(128),
            64,
            WorkControl {
                work: &first_only,
                ..control
            },
            false,
            0
        ),
        Err(StageAudioError::Limit("plan work per read"))
    ));
    assert_eq!(
        first_only.borrow().plan_work,
        MAX_PLAN_WORK_PER_READ,
        "the first query is charged before the second receives any work allowance"
    );
    assert_eq!(provider.calls, calls);

    let processing = plan
        .audio_processing(AudioSample(128)..AudioSample(192), Default::default())
        .unwrap();
    let AudioSignalContent::Bound(binding) = &processing.spans[0].content else {
        panic!("expected bound Preserve")
    };
    let AudioBoundDomain::Root(domain) = binding.raw_domain().unwrap() else {
        panic!("expected root operand")
    };
    let first_query_work = domain
        .audio(AudioSample(128)..AudioSample(192), Default::default())
        .unwrap()
        .work;
    let domain_first_only = RefCell::new(ReadWork {
        plan_work: MAX_PLAN_WORK_PER_READ - first_query_work,
        ..Default::default()
    });
    assert!(matches!(
        renderer.read_domain_controlled(
            &mut provider,
            &domain,
            AudioSample(128),
            64,
            WorkControl {
                work: &domain_first_only,
                ..control
            },
            0
        ),
        Err(StageAudioError::Limit("plan work per read"))
    ));
    assert_eq!(domain_first_only.borrow().plan_work, MAX_PLAN_WORK_PER_READ);
    assert_eq!(provider.calls, calls);

    let mut cold = StageAudio::new(Arc::clone(&plan));
    let used = RefCell::new(ReadWork {
        prepared_stages: cold.limits.maximum_prepared_stages,
        ..Default::default()
    });
    assert!(matches!(
        cold.read_controlled(
            &mut provider,
            AudioSample(128),
            64,
            WorkControl {
                work: &used,
                ..control
            },
            false,
            0
        ),
        Err(StageAudioError::Limit("prepared stages per read"))
    ));
    assert_eq!(
        provider.calls, calls,
        "shared preparation admission precedes decoder access"
    );

    // The cached preparation must still re-admit a complete media dependency,
    // even though the same read observed it earlier through an unbound Source.
    provider.prepared = changed_wav();
    assert!(matches!(
        renderer.read_controlled(&mut provider, AudioSample(128), 64, control, false, 0),
        Err(StageAudioError::Preparation(
            PreparationError::IndexMismatch
        ))
    ));
}

#[test]
fn expanded_edit_reveals_current_source_without_reclocking_its_binding() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let mut crop = preserve("sequence", 128, 128);
    let NodeKind::Retime { mapping, .. } = &mut crop.kind else {
        unreachable!()
    };
    *mapping = FrameRange::new(ProjectFrame(128), ProjectFrame(256)).unwrap();
    let (old, mut provider) = document(
        rate,
        &["crop"],
        vec![
            ("a", source(rate, 0, 128, 128)),
            ("b", source(rate, 128, 256, 128)),
            (
                "sequence",
                BeatNode::sequence("Words", vec![id("a"), id("b")]),
            ),
            ("crop", crop),
        ],
    );
    let mut wire = serde_json::to_value(&old).unwrap();
    wire["nodes"]["crop"] = serde_json::to_value(preserve("sequence", 256, 256)).unwrap();
    let current = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let bound = bind(
        &old,
        &current,
        vec![(
            "a",
            lattice("a", AudioClockRoot::ProjectRootRoundEven),
            None,
        )],
    );
    let plan = Arc::new(RenderPlan::compile(&bound).unwrap());
    let query = plan
        .audio_processing(AudioSample(0)..AudioSample(1), Default::default())
        .unwrap();
    let AudioSignalContent::Bound(bound) = &query.spans[0].content else {
        panic!("expected bound Source")
    };
    // The current Edit exposes the complete owned Source, so its physical
    // support is current. Its historical physical origin stays negative: the
    // old enclosing crop must not freeze an empty body after being expanded.
    let AudioBoundDomain::Root(domain) = bound.raw_domain().unwrap() else {
        panic!("current owned Source has a physical root domain")
    };
    assert_eq!(domain.root_samples(), AudioSample(-128)..AudioSample(0));
    assert_eq!(
        bound.reference_at_offset(0).unwrap(),
        ExactRatio::integer(-128)
    );
    let source_only = crate::SequenceAudio::new(Arc::clone(&plan));
    assert!(matches!(
        source_only.read_sources(
            &mut provider,
            AudioSample(0),
            64,
            Duration::from_secs(10),
            &AtomicBool::new(false)
        ),
        Err(crate::SequenceAudioError::Unsupported { .. })
    ));
    assert_eq!(provider.calls, 0);
    let expected = decoded(&provider, 0..64);
    let mut renderer = StageAudio::new(Arc::clone(&plan));
    let block = render(&mut renderer, &mut provider, 0, 64);
    assert_eq!(block.samples, expected);
    assert!(block.suppressed.is_empty());
    assert!(provider.calls > 0);
}

fn changed_wav() -> crate::PreparedSource {
    use deadpan_media::audio_session::{AudioSession, AudioSessionLimits};
    use deadpan_media::source_index::SourceContentIdentity;
    use sha2::{Digest, Sha256};
    let mut bytes = std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav"),
    )
    .unwrap();
    // Alter a PCM payload byte without changing any container/index geometry.
    let last = bytes.last_mut().unwrap();
    *last ^= 1;
    let cancelled = AtomicBool::new(false);
    let session = AudioSession::open_verified(
        &mut std::io::Cursor::new(&bytes),
        SourceContentIdentity::new(Sha256::digest(&bytes).into(), bytes.len() as u64).unwrap(),
        0,
        AudioSessionLimits::default(),
        &cancelled,
    )
    .unwrap();
    let index = session.index().clone();
    crate::PreparedSource::with_layout(
        session,
        &index,
        AudioChannelLayout::Native {
            channels: 2,
            mask: 3,
        },
        &cancelled,
    )
    .unwrap()
}
