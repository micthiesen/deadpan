use super::*;

fn isolate(document: &ProjectDocument, name: &str, ordinal: u32) -> ProjectDocument {
    edit(
        document,
        name,
        Command::IsolateGap {
            node: id("repeat"),
            iteration: play(ordinal),
            id: id("isolated-gap"),
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    )
}

fn check_node_reads(
    document: &ProjectDocument,
    provider: &mut Provider,
    start: i64,
    expected: &[[f32; 2]],
) {
    let mut audio = renderer(document, provider);
    let whole = read(&mut audio, provider, start, 128);
    assert_close(&whole, expected);
    for (offset, count) in [(91, 37), (0, 51), (51, 40)] {
        assert_eq!(
            read(&mut audio, provider, start + offset, count),
            whole[offset as usize..(offset + i64::from(count)) as usize]
        );
    }
    let mut fresh = renderer(document, provider);
    assert_eq!(read(&mut fresh, provider, start + 91, 37), whole[91..]);
    let plan = Arc::new(RenderPlan::compile(document).unwrap());
    let domain = plan
        .audio_domain_at(AudioSample(start), AudioQueryLimits::default())
        .unwrap();
    let mut seeded = StageAudio::new(Arc::clone(&plan));
    let block = seeded
        .read_domain(
            provider,
            &domain,
            AudioSample(start),
            128,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert!(block.gap_after.is_none());
    assert_eq!(block.samples, whole);
}

#[test]
fn isolated_gap_keeps_decoded_ntsc_phase_and_live_owned_recipe() {
    let original = document(
        ntsc(),
        &["lead", "repeat"],
        vec![
            ("lead", silence(1)),
            ("repeat", repeated(3, room(2, 100..321))),
            ("child", silence(1)),
        ],
    );
    let mut provider = Provider::new();
    let waveform = room_reference(&provider, 100..321, 3204);
    // Old gap starts at frame 2: B(2)=3203, exact origin=3203.2.
    let expected = sample_reference(&waveform, ratio(-1, 5), ExactRatio::ONE, 128);
    let mut before = renderer(&original, &mut provider);
    assert_close(&read(&mut before, &mut provider, 3203, 128), &expected);
    let isolated = isolate(&original, "isolate", 0);
    check_node_reads(&isolated, &mut provider, 3203, &expected);
    // Parent defaults remain live only for the other gaps.
    let changed = set_gap(&isolated, "default-silence", 3, gap(2, HoldAudio::Silence));
    check_node_reads(&changed, &mut provider, 3203, &expected);
    let mut current = renderer(&changed, &mut provider);
    assert_eq!(
        read(&mut current, &mut provider, 8008, 128),
        vec![[0.; 2]; 128]
    );
    // Shortening the independent Hold retains its full sampling phase.
    let shortened = edit(
        &changed,
        "shorten",
        Command::SetHoldDuration {
            node: id("isolated-gap"),
            duration: frames(1),
        },
    );
    check_node_reads(&shortened, &mut provider, 3203, &expected);
}

#[test]
fn materialized_gap_edges_match_independent_fades_and_explicit_hard_choices() {
    for hard in [false, true] {
        let mut original = document(
            ntsc(),
            &["lead", "repeat"],
            vec![
                ("lead", silence(1)),
                ("repeat", repeated(3, room(2, 100..321))),
                ("child", silence(1)),
            ],
        );
        if hard {
            original = edit(
                &original,
                "hard",
                Command::SetAudioEdge {
                    node: id("repeat"),
                    edge: AudioBoundaryKind::RepeatGapStart,
                    policy: AudioEdgePolicy::Hard,
                },
            );
        }
        let isolated = isolate(&original, "isolate-edge", 0);
        let mut provider = Provider::new();
        let waveform = room_reference(&provider, 100..321, 3204);
        let mut expected = sample_reference(&waveform, ratio(-1, 5), ExactRatio::ONE, 128);
        if !hard {
            for (index, sample) in expected.iter_mut().enumerate().take(96) {
                let gain = (index as f32 + 0.5) / 96.;
                for value in sample {
                    *value *= gain;
                }
            }
        }
        let mut before = renderer(&original, &mut provider);
        let mut after = renderer(&isolated, &mut provider);
        let mut faded = |audio: &mut StageAudio, start| {
            audio
                .read_edge_faded(
                    &mut provider,
                    AudioSample(start),
                    128,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples
        };
        assert_close(&faded(&mut after, 3203), &expected);
        for start in [3100, 3203, 6300, 7900] {
            assert_eq!(faded(&mut after, start), faded(&mut before, start));
        }
    }
}

#[test]
fn materialized_gap_retains_distinct_outer_play_clocks_and_definition_birth() {
    let outer = BeatNode {
        label: "Outer".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: id("repeat"),
            iterations: IterationOrder::new(revision("outer-plays"), 2).unwrap(),
            gap: None,
        },
    };
    let original = document(
        ntsc(),
        &["lead", "outer"],
        vec![
            ("lead", silence(1)),
            ("outer", outer),
            ("repeat", repeated(3, room(2, 100..321))),
            ("child", silence(1)),
        ],
    );
    let isolated = isolate(&original, "isolate-outer", 0);
    let mut provider = Provider::new();
    let waveform = room_reference(&provider, 100..321, 3204);
    for (start, phase) in [(3203, ratio(-1, 5)), (14414, ratio(-2, 5))] {
        check_node_reads(
            &isolated,
            &mut provider,
            start,
            &sample_reference(&waveform, phase, ExactRatio::ONE, 128),
        );
    }
    let grown = edit(
        &isolated,
        "grow-outer",
        Command::SetRepeat {
            node: id("outer"),
            plays: 3,
            gap: None,
        },
    );
    // New outer play uses the inner definition's PointCeil grid: the surviving
    // inner gap begins at ceil(1601.6), so its retained phase is +0.4 samples.
    check_node_reads(
        &grown,
        &mut provider,
        25626,
        &sample_reference(&waveform, ratio(2, 5), ExactRatio::ONE, 128),
    );
}

#[test]
fn materializing_a_born_gap_keeps_its_canonical_clock_and_discards_old_outer_window() {
    let original = document(
        ntsc(),
        &["lead", "repeat"],
        vec![
            ("lead", silence(1)),
            ("repeat", repeated(1, room(2, 100..321))),
            ("child", silence(1)),
        ],
    );
    let captured = capture(&original);
    let mut state = serde_json::to_value(captured.audio_bindings()).unwrap();
    let lattice = state["gap_bindings"]["repeat"]["lattice"].clone();
    state["gap_bindings"]["repeat"]["reanchors"] = serde_json::json!([{
        "placement": lattice,
        "window": ExactFrameRange::new(ExactRatio::integer(900), ExactRatio::integer(901)).unwrap(),
    }]);
    let captured = install(
        &captured,
        AudioBindingState::from_json(&state.to_string()).unwrap(),
    );
    let grown = set_gap(&captured, "grow", 3, room(2, 100..321));
    let isolated = isolate(&grown, "isolate-born", 0);
    let mut provider = Provider::new();
    let waveform = room_reference(&provider, 100..321, 3204);
    check_node_reads(&isolated, &mut provider, 3203, &waveform[..128]);
    let binding = &isolated.audio_bindings().bindings()[&id("isolated-gap")];
    assert!(matches!(
        binding.lattice.reference.root,
        AudioClockRoot::GapDefinitionPointCeil { .. }
    ));
    assert_eq!(binding.reanchors[0].window, None);
}

#[test]
fn isolated_gap_under_real_preserve_matches_the_unedited_pcm_and_fresh_seeks() {
    let original = document(
        ntsc(),
        &["preserve"],
        vec![
            ("preserve", preserve("repeat", 7, 9)),
            ("repeat", repeated(3, room(2, 100..321))),
            ("child", silence(1)),
        ],
    );
    let isolated = isolate(&original, "isolate-preserve", 0);
    let mut provider = Provider::new();
    let mut before = renderer(&original, &mut provider);
    let mut after = renderer(&isolated, &mut provider);
    // Include the silence/room-tone edges as well as the Preserve history.
    for start in [0, 1400, 2200, 5400, 8600, 12000, 14200] {
        let expected = read(&mut before, &mut provider, start, 128);
        assert_close(&read(&mut after, &mut provider, start, 128), &expected);
        let mut fresh = renderer(&isolated, &mut provider);
        assert_eq!(
            read(&mut fresh, &mut provider, start, 128),
            read(&mut after, &mut provider, start, 128)
        );
    }
}
