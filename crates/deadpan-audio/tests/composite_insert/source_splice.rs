use super::*;

fn splice(
    document: &ProjectDocument,
    parent: &str,
    index: usize,
    duration: i64,
    name: &str,
) -> ProjectDocument {
    let NodeKind::Source { source } =
        source(document.presentation_basis().frame_rate, duration).kind
    else {
        unreachable!()
    };
    edit(
        document,
        name,
        Command::SpliceSource {
            parent: id(parent),
            index,
            source,
            id: id(name),
            label: "Copied moment".into(),
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    )
}

#[test]
fn fresh_ntsc_source_uses_project_phase_while_repeat_fragments_keep_distinct_entries() {
    let original = document(
        ntsc(),
        &["repeat"],
        vec![("a", source(ntsc(), 2)), ("repeat", repeat("a", 2))],
    );
    let divided = edit(
        &original,
        "cut",
        Command::Split {
            node: id("repeat"),
            at: frames(1),
            identities: SplitIdentities {
                nodes: (0..12).map(|i| id(&format!("cut-{i}"))).collect(),
            },
        },
    );
    let first = splice(&divided, "root", 1, 1, "first");
    let second = splice(&first, "root", 2, 1, "second");
    let mut provider = Provider::new();
    // A newly pasted Source uses its actual zero-origin mix grid, not phase
    // zero and not any old suffix owner's retained sample entry.
    let plus_two_fifths = expected(&provider, ratio(2, 5), 128);
    let minus_one_fifth = expected(&provider, ratio(-1, 5), 128);
    check_reads(&first, &mut provider, 1602, &plus_two_fifths);
    check_reads(&second, &mut provider, 1602, &plus_two_fifths);
    check_reads(&second, &mut provider, 3203, &minus_one_fifth);
    // The partial first play and the next whole play have different old
    // physical entries. Reusing one resume for both changes the next play.
    for (old, new, again, phase) in [
        (1602, 3203, 4805, ExactRatio::integer(1602)),
        (3203, 4805, 6406, ratio(-1, 5)),
    ] {
        let oracle = expected(&provider, phase, 128);
        check_reads(&original, &mut provider, old, &oracle);
        check_reads(&first, &mut provider, new, &oracle);
        check_reads(&second, &mut provider, again, &oracle);
    }
    assert!(!first.audio_bindings().bindings().contains_key(&id("first")));
    assert!(
        second
            .audio_bindings()
            .bindings()
            .contains_key(&id("first"))
    );
    assert!(
        !second
            .audio_bindings()
            .bindings()
            .contains_key(&id("second"))
    );
}

#[test]
fn nested_group_edges_keep_ownership_ancestor_suffix_clocks_and_live_recipe_edits() {
    let original = document(
        ntsc(),
        &["prefix", "outer", "root-tail"],
        vec![
            ("prefix", source(ntsc(), 1)),
            (
                "outer",
                BeatNode::sequence("Outer", vec![id("inner"), id("outer-tail")]),
            ),
            (
                "inner",
                BeatNode::sequence("Inner", vec![id("lead"), id("repeat")]),
            ),
            ("lead", source(ntsc(), 2)),
            ("repeat", repeat("a", 2)),
            ("a", source(ntsc(), 2)),
            ("outer-tail", source(ntsc(), 1)),
            ("root-tail", source(ntsc(), 2)),
        ],
    );
    let first = splice(&original, "inner", 0, 1, "first");
    let second = splice(&first, "inner", 3, 1, "second");
    let NodeKind::Sequence { children } = &second.nodes()[&id("inner")].kind else {
        panic!()
    };
    assert_eq!(
        children,
        &[id("first"), id("lead"), id("repeat"), id("second")]
    );
    let mut provider = Provider::new();
    for (old, first_start, second_start, phase) in [
        (1602, 3203, 3203, ratio(2, 5)),
        (4805, 6406, 6406, ratio(1, 5)),
        (8008, 9610, 9610, ExactRatio::ZERO),
        (11211, 12813, 14414, ratio(-1, 5)),
        (12813, 14414, 16016, ratio(1, 5)),
    ] {
        let oracle = expected(&provider, phase, 128);
        check_reads(&original, &mut provider, old, &oracle);
        check_reads(&first, &mut provider, first_start, &oracle);
        check_reads(&second, &mut provider, second_start, &oracle);
    }
    let NodeKind::Source { source: recipe } = &second.nodes()[&id("first")].kind else {
        panic!()
    };
    let edited = edit(
        &second,
        "offset",
        Command::SetSourceAudioMapping {
            node: id("first"),
            mapping: recipe.audio_mapping,
            offset: AudioSample(1),
        },
    );
    let live = expected(&provider, ratio(637, 5), 128); // 128 + 2/5 - 1
    check_reads(&edited, &mut provider, 1730, &live);
    let suffix = expected(&provider, ratio(-1, 5), 128);
    check_reads(&edited, &mut provider, 14414, &suffix);
}

#[test]
fn source_splice_leaves_shifted_preserve_preparation_and_roomtone_on_their_own_clocks() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let original = document(
        rate,
        &["group", "crop"],
        vec![
            ("group", BeatNode::sequence("Group", vec![])),
            ("crop", partition("stage", 64..384)),
            ("stage", preserve("a", 128, 384)),
            ("a", source(rate, 128)),
        ],
    );
    let inserted = splice(&original, "group", 0, 93, "moment");
    let mut provider = Provider::new();
    let input = provider
        .prepared
        .prepare(
            ResampleRecipe::new(
                100..218,
                ExactRatio::integer(100),
                AudioSample(0),
                ratio(147, 160),
                AudioSample(0)..AudioSample(128),
            )
            .unwrap(),
            AudioSample(0),
            128,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples;
    let oracle = stretched(&input, 384);
    let mut actual = renderer(&inserted, &mut provider);
    assert_close(
        &read(&mut actual, &mut provider, 93 + 256, 64),
        &oracle[320..],
    );
    assert_close(&read(&mut actual, &mut provider, 93, 256), &oracle[64..320]);
    assert!(
        inserted.audio_bindings().bindings()[&id("a")]
            .reanchors
            .is_empty()
    );

    let mut repeated = repeat("a", 3);
    let NodeKind::Repeat { gap, .. } = &mut repeated.kind else {
        panic!()
    };
    *gap = Some(room(1, 700..921));
    let original = document(
        ntsc(),
        &["group", "repeat"],
        vec![
            ("group", BeatNode::sequence("Group", vec![id("lead")])),
            ("lead", source(ntsc(), 2)),
            ("repeat", repeated),
            ("a", source(ntsc(), 1)),
        ],
    );
    let inserted = splice(&original, "group", 1, 1, "room-moment");
    let wave = room_reference(&provider, 700..921, 1700);
    for (old, new, phase) in [(4805, 6406, ratio(1, 5)), (8008, 9610, ExactRatio::ZERO)] {
        let oracle = sample_reference(&wave, phase, ExactRatio::ONE, 128);
        check_reads(&original, &mut provider, old, &oracle);
        check_reads(&inserted, &mut provider, new, &oracle);
    }
}

#[test]
fn pasted_selected_audio_ends_on_its_exact_interval_after_another_splice() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let original = document(
        rate,
        &["prefix", "tail"],
        vec![("prefix", silence(1)), ("tail", silence(5))],
    );
    let NodeKind::Source { mut source } = source(rate, 66).kind else {
        panic!()
    };
    source.audio_mapping = SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::ZERO,
        frames: source.audio_mapping.duration_frames(frames(66)).unwrap(),
        selection: ExactFrameRange::new(ExactRatio::ZERO, ratio(129, 2)).unwrap(),
    };
    let first = edit(
        &original,
        "selected",
        Command::SpliceSource {
            parent: id("root"),
            index: 1,
            source,
            id: id("selected"),
            label: "Exact audio out".into(),
            timing: AudioTimingId {
                allocation: revision("selected"),
                ordinal: 0,
            },
        },
    );
    let second = splice(&first, "root", 1, 1, "before-selected");
    let mut provider = Provider::new();
    // Physical selection [0,64.5) includes integer points 0..65. Its filter
    // taps end at ceil(100 + 64.5*147/160)=160, independently of beat rounding.
    let oracle = provider
        .prepared
        .prepare(
            ResampleRecipe::new(
                100..160,
                ExactRatio::integer(100),
                AudioSample(0),
                ratio(147, 160),
                AudioSample(0)..AudioSample(65),
            )
            .unwrap(),
            AudioSample(0),
            65,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples;
    for (document, start) in [(&first, 1), (&second, 2)] {
        let mut actual = renderer(document, &mut provider);
        let whole = read(&mut actual, &mut provider, start, 66);
        assert_close(&whole[..65], &oracle);
        assert_eq!(whole[65], [0.; 2]);
        assert_eq!(read(&mut actual, &mut provider, start + 61, 5), whole[61..]);
    }
}
