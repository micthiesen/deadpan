use super::*;

#[path = "delete_range/children.rs"]
mod children;
#[path = "delete_range/nested_partition.rs"]
mod nested_partition;

fn cut(document: &ProjectDocument, parent: &str, range: Range<i64>, name: &str) -> ProjectDocument {
    let range = FrameRange::new(ProjectFrame(range.start), ProjectFrame(range.end)).unwrap();
    let required = document
        .range_deletion(&id(parent), range)
        .unwrap()
        .required_ids;
    let after = edit(
        document,
        name,
        Command::DeleteRange {
            parent: id(parent),
            range,
            identities: SplitIdentities {
                nodes: (0..required)
                    .map(|index| id(&format!("{name}-{index}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    );
    assert_eq!(
        after.duration().unwrap().frames(),
        document.duration().unwrap().frames() - (range.end().0 - range.start().0)
    );
    after
}

fn pcm(
    document: &ProjectDocument,
    provider: &mut Provider,
    start: i64,
    count: usize,
) -> Vec<[f32; 2]> {
    let mut reader = renderer(document, provider);
    let mut samples = Vec::with_capacity(count);
    for offset in (0..count).step_by(256) {
        samples.extend(read(
            &mut reader,
            provider,
            start + i64::try_from(offset).unwrap(),
            u32::try_from((count - offset).min(256)).unwrap(),
        ));
    }
    samples
}

#[track_caller]
fn identical(actual: &[[f32; 2]], expected: &[[f32; 2]], offset: usize) {
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(actual, expected, "retained sample {}", offset + index);
    }
}

#[track_caller]
fn retained(
    document: &ProjectDocument,
    provider: &mut Provider,
    start: i64,
    expected: &[[f32; 2]],
) {
    assert!(expected.iter().flatten().all(|sample| sample.is_finite()));
    assert!(expected.iter().flatten().any(|sample| sample.abs() > 0.001));
    let mut reader = renderer(document, provider);
    // Endpoint first on a cold renderer, followed by all samples in reverse
    // irregular chunks, then an entry reread. No time or sample tolerance.
    let tail = expected.len().saturating_sub(256);
    identical(
        &read(
            &mut reader,
            provider,
            start + i64::try_from(tail).unwrap(),
            u32::try_from(expected.len() - tail).unwrap(),
        ),
        &expected[tail..],
        tail,
    );
    let mut chunks = Vec::new();
    let mut offset = 0;
    for count in [193, 1, 47, 256, 113].into_iter().cycle() {
        if offset == expected.len() {
            break;
        }
        let count = count.min(expected.len() - offset);
        chunks.push((offset, count));
        offset += count;
    }
    for (offset, count) in chunks.into_iter().rev() {
        identical(
            &read(
                &mut reader,
                provider,
                start + i64::try_from(offset).unwrap(),
                u32::try_from(count).unwrap(),
            ),
            &expected[offset..offset + count],
            offset,
        );
    }
    identical(
        &read(&mut reader, provider, start, 128),
        &expected[..128],
        0,
    );
}

#[test]
fn interior_range_delete_preserves_every_ntsc_prefix_and_suffix_sample() {
    let before = document(ntsc(), &["voice"], vec![("voice", source(ntsc(), 7))]);
    let after = cut(&before, "root", 1..3, "interior-cut");
    let mut provider = Provider::new();
    // B(1)=1602, B(3)=4805, B(7)=11211, B(5)=8008.
    // Keep [0,1602) and move [4805,11211) to [1602,8008).
    let prefix = pcm(&before, &mut provider, 0, 1602);
    let suffix = pcm(&before, &mut provider, 4805, 6406);
    assert_close(&prefix[..128], &expected(&provider, ExactRatio::ZERO, 128));
    assert_close(
        &suffix[..128],
        &expected(&provider, ExactRatio::integer(4805), 128),
    );
    retained(&after, &mut provider, 0, &prefix);
    retained(&after, &mut provider, 1602, &suffix);
}

#[test]
fn nested_range_delete_preserves_roomtone_source_and_outer_siblings() {
    for roomtone in [false, true] {
        let leaf = if roomtone {
            BeatNode::hold("Room tone", room(7, 700..921))
        } else {
            source(ntsc(), 7)
        };
        let before = document(
            ntsc(),
            &["lead", "group", "outer"],
            vec![
                ("lead", source(ntsc(), 1)),
                ("group", BeatNode::sequence("Group", vec![id("voice")])),
                ("voice", leaf),
                ("outer", source(ntsc(), 5)),
            ],
        );
        let after = cut(&before, "group", 2..4, "nested-cut");
        let mut provider = Provider::new();
        // Voice begins at exact 1601.6. Its retained global frame 4 entry is
        // sample 6406, or 4804.4 local points, independently of split bindings.
        let suffix = pcm(&before, &mut provider, 6406, 6407);
        let entry = if roomtone {
            let wave = room_reference(&provider, 700..921, 11212);
            sample_reference(&wave, ratio(24022, 5), ExactRatio::ONE, 128)
        } else {
            expected(&provider, ratio(24022, 5), 128)
        };
        assert_close(&suffix[..128], &entry);
        let prefix = pcm(&before, &mut provider, 0, 3203);
        let outer = pcm(&before, &mut provider, 12813, 8008);
        retained(&after, &mut provider, 0, &prefix);
        retained(&after, &mut provider, 3203, &suffix);
        retained(&after, &mut provider, 9610, &outer);
    }
}

#[test]
fn range_delete_retains_clocks_from_an_earlier_pause_insertion() {
    let original = document(
        ntsc(),
        &["lead", "voice"],
        vec![("lead", silence(1)), ("voice", source(ntsc(), 7))],
    );
    let before = insert_pause(&original, 1, 1, "prior-pause");
    assert!(!before.audio_bindings().is_empty());
    let after = cut(&before, "root", 3..5, "bound-cut");
    let mut provider = Provider::new();
    let prefix = pcm(&before, &mut provider, 0, 4805);
    let suffix = pcm(&before, &mut provider, 8008, 6406);
    // InsertTime retained original frame 1's 2/5 phase. At the later cut,
    // 4805 more physical samples give 4805.4 local source points.
    assert_close(&suffix[..128], &expected(&provider, ratio(24027, 5), 128));
    retained(&after, &mut provider, 0, &prefix);
    retained(&after, &mut provider, 4805, &suffix);
}

#[test]
fn range_delete_preserves_every_repeated_play_and_roomtone_gap_sample() {
    let mut repeated = repeat("voice", 2);
    let NodeKind::Repeat { gap, .. } = &mut repeated.kind else {
        unreachable!()
    };
    *gap = Some(room(5, 700..921));
    let before = document(
        ntsc(),
        &["lead", "repeat"],
        vec![
            ("lead", source(ntsc(), 7)),
            ("repeat", repeated),
            ("voice", source(ntsc(), 5)),
        ],
    );
    let after = cut(&before, "root", 1..3, "repeat-cut");
    let mut provider = Provider::new();
    // The composite suffix moves frames [7,22) to [5,20). Each five-frame
    // play and gap allocates 8008 samples, giving 24024 retained samples.
    let suffix = pcm(&before, &mut provider, 11211, 24024);
    assert_close(&suffix[..128], &expected(&provider, ratio(-1, 5), 128));
    retained(&after, &mut provider, 8008, &suffix);
}

#[test]
fn range_delete_preserves_ntsc_preserve_suffix_full_history() {
    let before = document(
        ntsc(),
        &["lead", "stage"],
        vec![
            ("lead", source(ntsc(), 7)),
            ("stage", preserve("voice", 4, 12)),
            ("voice", source(ntsc(), 4)),
        ],
    );
    let after = cut(&before, "root", 1..3, "preserve-cut");
    let mut provider = Provider::new();
    // Independently construct 6407 PointCeil input points and 19220 full
    // canonical Preserve points at 1/3 speed. Source support ends at 5986.
    let mut input = Vec::new();
    for start in (0..6407).step_by(256) {
        input.extend(
            provider
                .prepared
                .prepare(
                    ResampleRecipe::new(
                        100..5986,
                        ExactRatio::integer(100),
                        AudioSample(0),
                        ratio(147, 160),
                        AudioSample(0)..AudioSample(6407),
                    )
                    .unwrap(),
                    AudioSample(start),
                    u32::try_from((6407 - start).min(256)).unwrap(),
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples,
        );
    }
    let full = stretched(&input, 19220);
    // Frame 7 enters at 11211-11211.2=-1/5 of the full output point clock.
    let entry = sample_reference(&full, ratio(-1, 5), ExactRatio::ONE, 128);
    let suffix = pcm(&before, &mut provider, 11211, 19219);
    assert_close(&suffix[..128], &entry);
    retained(&after, &mut provider, 8008, &suffix);
}

#[test]
fn aligned_range_delete_retains_suffix_and_terminal_prefix_without_extra_capture() {
    let before = document(
        ntsc(),
        &["lead", "voice"],
        vec![("lead", silence(1)), ("voice", source(ntsc(), 5))],
    );
    let range = FrameRange::new(ProjectFrame(0), ProjectFrame(1)).unwrap();
    assert_eq!(
        before
            .range_deletion(&id("root"), range)
            .unwrap()
            .required_ids,
        0
    );
    assert!(before.audio_bindings().is_empty());
    let after = cut(&before, "root", 0..1, "aligned-cut");
    let mut provider = Provider::new();
    // B(1)=1602 and B(6)=9610. The five-frame suffix retains all 8008
    // samples at [0,8008), including its independently computed 2/5 entry.
    let suffix = pcm(&before, &mut provider, 1602, 8008);
    assert_close(&suffix[..128], &expected(&provider, ratio(2, 5), 128));
    retained(&after, &mut provider, 0, &suffix);
    assert_eq!(after.audio_bindings().timings().len(), 1);
    assert!(
        after
            .audio_bindings()
            .timings()
            .contains_key(&AudioTimingId {
                allocation: revision("aligned-cut"),
                ordinal: 0,
            })
    );

    let terminal = document(
        ntsc(),
        &["voice", "tail"],
        vec![("voice", source(ntsc(), 5)), ("tail", silence(1))],
    );
    let range = FrameRange::new(ProjectFrame(5), ProjectFrame(6)).unwrap();
    assert_eq!(
        terminal
            .range_deletion(&id("root"), range)
            .unwrap()
            .required_ids,
        0
    );
    let shortened = cut(&terminal, "root", 5..6, "terminal-cut");
    let prefix = pcm(&terminal, &mut provider, 0, 8008);
    assert_close(&prefix[..128], &expected(&provider, ExactRatio::ZERO, 128));
    retained(&shortened, &mut provider, 0, &prefix);
    assert!(terminal.audio_bindings().is_empty());
    assert_eq!(shortened.audio_bindings(), terminal.audio_bindings());
}
