use super::*;

#[test]
fn nested_fragment_split_composes_a_previously_shifted_source_clock() {
    let original = document(
        ntsc(),
        &["prefix", "group"],
        vec![
            ("prefix", source(ntsc(), 1)),
            ("group", BeatNode::sequence("Group", vec![id("lead")])),
            ("lead", source(ntsc(), 4)),
        ],
    );
    let first = insert_pause(&original, 2, 1, "first-fragment");
    // The first right fragment covers current [3,6). Split it again at 4,
    // preserving its current resumed clock rather than recapturing the source.
    let second = insert_pause(&first, 4, 1, "second-fragment");
    let mut provider = Provider::new();
    // At frame 3 the first resume has local phase 3203-1601.6=1601.4.
    // Moving to current frame 4 adds B(4)-B(3)=1601 samples, so phase is
    // 3202.4, not B(3)-1601.6=3203.4. The second pause resumes it at B(5).
    let oracle = expected(&provider, ratio(16012, 5), 128);
    check_reads(&first, &mut provider, 6406, &oracle);
    check_reads(&second, &mut provider, 8008, &oracle);
    let earlier = expected(&provider, ratio(8007, 5), 128);
    check_reads(&first, &mut provider, 4805, &earlier);
    check_reads(&second, &mut provider, 4805, &earlier);
    check_silence(&second, &mut provider, 3203..4805);
    check_silence(&second, &mut provider, 6406..8008);
}

#[test]
fn nested_pause_resumes_each_level_at_its_own_ntsc_sample_and_stays_editable() {
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
    let first = insert_pause(&original, 2, 1, "first-nested");
    let second = insert_pause(&first, 3, 1, "second-nested");
    let longer = edit(
        &second,
        "longer-nested",
        Command::SetHoldDuration {
            node: id("first-nested-pause"),
            duration: frames(2),
        },
    );
    let mut provider = Provider::new();
    // The split Source begins at frame 1, i.e. sample 1601.6. Its old
    // continuation at B(2)=3203 therefore has phase 1601.4, not 1602.
    // Later branches begin at frames 3,5,7,8. Their rounded entries and
    // sub-sample phases are independent of this first resume.
    for (old, new, again, resized, phase) in [
        (3203, 4805, 6406, 8008, ratio(8007, 5)),
        (4805, 6406, 8008, 9610, ratio(1, 5)),
        (8008, 9610, 11211, 12813, ExactRatio::ZERO),
        (11211, 12813, 14414, 16016, ratio(-1, 5)),
        (12813, 14414, 16016, 17618, ratio(1, 5)),
    ] {
        let oracle = expected(&provider, phase, 128);
        for (document, start) in [
            (&original, old),
            (&first, new),
            (&second, again),
            (&longer, resized),
        ] {
            check_reads(document, &mut provider, start, &oracle);
        }
    }
    let prefix = expected(&provider, ExactRatio::ZERO, 128);
    let entered = expected(&provider, ratio(2, 5), 128);
    for document in [&first, &second, &longer] {
        check_reads(document, &mut provider, 0, &prefix);
        check_reads(document, &mut provider, 1602, &entered);
    }
    check_silence(&first, &mut provider, 3203..4805);
    check_silence(&second, &mut provider, 3203..6406);
    check_silence(&longer, &mut provider, 3203..8008);
    assert_eq!(longer.node_duration(&id("inner")).unwrap(), frames(9));
    assert_eq!(longer.node_duration(&id("outer")).unwrap(), frames(10));
}

#[test]
fn nested_sequence_pause_preserves_later_preserve_input_and_roomtone_gap_clocks() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let original = document(
        rate,
        &["outer", "tail"],
        vec![
            (
                "outer",
                BeatNode::sequence("Outer", vec![id("inner"), id("crop")]),
            ),
            ("inner", BeatNode::sequence("Inner", vec![id("lead")])),
            ("lead", source(rate, 16)),
            ("crop", partition("stage", 64..384)),
            ("stage", preserve("a", 128, 384)),
            ("a", source(rate, 128)),
            ("tail", silence(11)),
        ],
    );
    let first = insert_pause(&original, 7, 93, "nested-preserve");
    let second = insert_pause(&first, 104, 5, "nested-preserve-again");
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
    for (document, start) in [(&original, 16), (&first, 109), (&second, 114)] {
        let mut actual = renderer(document, &mut provider);
        assert_close(
            &read(&mut actual, &mut provider, start + 256, 64),
            &oracle[320..],
        );
        assert_close(
            &read(&mut actual, &mut provider, start, 256),
            &oracle[64..320],
        );
        assert_eq!(actual.cached_stage_count(), 1);
    }
    assert!(
        first.audio_bindings().bindings()[&id("a")]
            .reanchors
            .is_empty()
    );
    assert_eq!(
        first.audio_bindings().bindings()[&id("a")],
        second.audio_bindings().bindings()[&id("a")]
    );
    check_silence(&first, &mut provider, 7..100);
    check_silence(&second, &mut provider, 104..109);

    // An implicit gap in a later sibling Repeat is also a distinct physical
    // allocation. The pause lives in a nested group, outside that Repeat.
    let mut repeated = repeat("a", 3);
    let NodeKind::Repeat { gap: recipe, .. } = &mut repeated.kind else {
        panic!()
    };
    *recipe = Some(room(1, 700..921));
    let old = document(
        ntsc(),
        &["group"],
        vec![
            (
                "group",
                BeatNode::sequence("Group", vec![id("lead"), id("repeat")]),
            ),
            ("lead", BeatNode::hold("Room", room(2, 100..321))),
            ("repeat", repeated),
            ("a", source(ntsc(), 1)),
        ],
    );
    let paused = insert_pause(&old, 1, 1, "nested-room");
    let wave = room_reference(&provider, 700..921, 1700);
    for (old_start, new_start, phase) in [(4805, 6406, ratio(1, 5)), (8008, 9610, ExactRatio::ZERO)]
    {
        let oracle = sample_reference(&wave, phase, ExactRatio::ONE, 128);
        check_reads(&old, &mut provider, old_start, &oracle);
        check_reads(&paused, &mut provider, new_start, &oracle);
    }
    check_silence(&paused, &mut provider, 1602..3203);
}
