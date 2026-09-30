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

fn splice_interior(document: &ProjectDocument, target: NodeId, name: &str) -> ProjectDocument {
    let NodeKind::Source { source } = source(ntsc(), 1).kind else {
        unreachable!()
    };
    edit(
        document,
        name,
        Command::SpliceSourceAt {
            parent: id("group"),
            target,
            at: frames(1),
            source,
            id: id(name),
            label: "Interior Original moment".into(),
            identities: SplitIdentities {
                nodes: (0..3).map(|i| id(&format!("{name}-{i}"))).collect(),
            },
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    )
}

#[test]
fn interior_ntsc_source_splice_and_fragment_refinement_keep_nonzero_pcm_at_both_joins() {
    let original = document(
        ntsc(),
        &["prefix", "group", "tail"],
        vec![
            ("prefix", source(ntsc(), 1)),
            ("group", BeatNode::sequence("Group", vec![id("lead")])),
            ("lead", source(ntsc(), 4)),
            ("tail", source(ntsc(), 2)),
        ],
    );
    let first = splice_interior(&original, id("lead"), "first");
    let NodeKind::Sequence { children } = &first.nodes()[&id("group")].kind else {
        panic!("group")
    };
    let second = splice_interior(&first, children[2].clone(), "second");
    let mut provider = Provider::new();
    // Each fresh insertion uses its new absolute grid. The physical lead
    // starts at sample 1601.6, so its two retained resumes have phases 1601.4
    // and 3202.4. Rounding the latter to the old frame-3 boundary loses a sample.
    for (document, start, phase) in [
        (&first, 3203, ratio(-1, 5)),
        (&second, 3203, ratio(-1, 5)),
        (&second, 6406, ratio(-2, 5)),
        (&original, 3203, ratio(8007, 5)),
        (&first, 4805, ratio(8007, 5)),
        (&second, 4805, ratio(8007, 5)),
        (&first, 6406, ratio(16012, 5)),
        (&second, 8008, ratio(16012, 5)),
        (&original, 8008, ExactRatio::ZERO),
        (&first, 9610, ExactRatio::ZERO),
        (&second, 11211, ExactRatio::ZERO),
    ] {
        let oracle = expected(&provider, phase, 128);
        assert!(oracle.iter().flatten().any(|sample| sample.abs() > 0.001));
        check_reads(document, &mut provider, start, &oracle);
    }
    let mut original_audio = renderer(&original, &mut provider);
    let mut first_audio = renderer(&first, &mut provider);
    let mut second_audio = renderer(&second, &mut provider);
    // The old suffix is bit-identical, separately from the handwritten phase
    // oracle above. These blocks all contain decoded fixture audio.
    assert_eq!(
        read(&mut original_audio, &mut provider, 3203, 128),
        read(&mut first_audio, &mut provider, 4805, 128)
    );
    assert_eq!(
        read(&mut first_audio, &mut provider, 6406, 128),
        read(&mut second_audio, &mut provider, 8008, 128)
    );
    for document in [&first, &second] {
        let prefix = expected(&provider, ExactRatio::ZERO, 128);
        let entry = expected(&provider, ratio(2, 5), 128);
        check_reads(document, &mut provider, 0, &prefix);
        check_reads(document, &mut provider, 1602, &entry);
    }
}

#[test]
fn interior_ntsc_roomtone_splice_keeps_nonzero_hold_recipe_phase() {
    let original = document(
        ntsc(),
        &["prefix", "group"],
        vec![
            ("prefix", source(ntsc(), 1)),
            ("group", BeatNode::sequence("Group", vec![id("lead")])),
            ("lead", BeatNode::hold("Room tone", room(4, 700..921))),
        ],
    );
    let inserted = splice_interior(&original, id("lead"), "room-insert");
    let mut provider = Provider::new();
    let wave = room_reference(&provider, 700..921, 2000);
    let oracle = sample_reference(&wave, ratio(8007, 5), ExactRatio::ONE, 128);
    assert!(oracle.iter().flatten().any(|sample| sample.abs() > 0.001));
    check_reads(&original, &mut provider, 3203, &oracle);
    check_reads(&inserted, &mut provider, 4805, &oracle);
    let mut before = renderer(&original, &mut provider);
    let mut after = renderer(&inserted, &mut provider);
    assert_eq!(
        read(&mut before, &mut provider, 3203, 128),
        read(&mut after, &mut provider, 4805, 128)
    );
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
