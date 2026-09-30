use super::*;

fn ripple(document: &ProjectDocument, node: &str, name: &str) -> ProjectDocument {
    edit(
        document,
        name,
        Command::DeleteRipple {
            node: id(node),
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    )
}

fn retained_pcm(
    document: &ProjectDocument,
    provider: &mut Provider,
    start: i64,
    count: usize,
) -> Vec<[f32; 2]> {
    let mut actual = renderer(document, provider);
    let mut result = Vec::with_capacity(count);
    for offset in (0..count).step_by(256) {
        result.extend(read(
            &mut actual,
            provider,
            start + i64::try_from(offset).unwrap(),
            u32::try_from((count - offset).min(256)).unwrap(),
        ));
    }
    result
}

#[track_caller]
fn assert_identical(actual: &[[f32; 2]], oracle: &[[f32; 2]], offset: usize) {
    assert_eq!(actual.len(), oracle.len());
    for (index, (actual, expected)) in actual.iter().zip(oracle).enumerate() {
        assert_eq!(
            actual,
            expected,
            "retained suffix sample {}",
            offset + index
        );
    }
}

// Read the endpoint first on a cold renderer, then every sample in reverse
// chunk order with irregular boundaries, and finally revisit the entry.
#[track_caller]
fn check_retained(
    document: &ProjectDocument,
    provider: &mut Provider,
    start: i64,
    oracle: &[[f32; 2]],
) {
    assert!(oracle.iter().flatten().all(|sample| sample.is_finite()));
    assert!(oracle.iter().flatten().any(|sample| sample.abs() > 0.001));
    let mut actual = renderer(document, provider);
    let last = oracle.len().saturating_sub(256);
    assert_identical(
        &read(
            &mut actual,
            provider,
            start + i64::try_from(last).unwrap(),
            u32::try_from(oracle.len() - last).unwrap(),
        ),
        &oracle[last..],
        last,
    );
    let mut chunks = Vec::new();
    let mut offset = 0;
    for length in [53, 197, 1, 256, 97].into_iter().cycle() {
        if offset == oracle.len() {
            break;
        }
        let length = length.min(oracle.len() - offset);
        chunks.push((offset, length));
        offset += length;
    }
    for (offset, length) in chunks.into_iter().rev() {
        assert_identical(
            &read(
                &mut actual,
                provider,
                start + i64::try_from(offset).unwrap(),
                u32::try_from(length).unwrap(),
            ),
            &oracle[offset..offset + length],
            offset,
        );
    }
    assert_identical(&read(&mut actual, provider, start, 128), &oracle[..128], 0);
}

fn source_fixture() -> ProjectDocument {
    document(
        ntsc(),
        &["prefix", "voice"],
        vec![("prefix", silence(1)), ("voice", source(ntsc(), 4))],
    )
}

#[test]
fn delete_preserves_nonzero_ntsc_source_suffix_phase() {
    let before = source_fixture();
    let after = ripple(&before, "prefix", "delete-prefix");
    // B(1)=1602, B(5)=8008, B(0)=0, B(4)=6406. Both suffixes have
    // 6406 samples. The old entry is 2/5 of a 48 kHz mix sample, or
    // 147/400 of a 44.1 kHz source sample after source sample 100.
    assert_eq!(
        ntsc().audio_boundary(ProjectFrame(1)).unwrap(),
        AudioSample(1602)
    );
    assert_eq!(
        ntsc().audio_boundary(ProjectFrame(5)).unwrap(),
        AudioSample(8008)
    );
    assert_eq!(
        ntsc().audio_boundary(ProjectFrame(4)).unwrap(),
        AudioSample(6406)
    );
    let mut provider = Provider::new();
    let entry = expected(&provider, ratio(2, 5), 128);
    let oracle = retained_pcm(&before, &mut provider, 1602, 6406);
    assert_close(&oracle[..128], &entry);
    check_retained(&after, &mut provider, 0, &oracle);
}

#[test]
fn historical_delete_keeps_its_unbound_phase_for_replay() {
    let before = source_fixture();
    let after = edit(
        &before,
        "historical-delete",
        Command::Delete { node: id("prefix") },
    );
    let mut provider = Provider::new();
    let retained = expected(&provider, ratio(2, 5), 128);
    let reset = expected(&provider, ExactRatio::ZERO, 128);
    assert_ne!(retained, reset);
    check_reads(&before, &mut provider, 1602, &retained);
    check_reads(&after, &mut provider, 0, &reset);
}

#[test]
fn nested_delete_preserves_inner_and_outer_source_suffixes() {
    let before = document(
        ntsc(),
        &["group", "outer"],
        vec![
            ("group", BeatNode::sequence("Group", vec![id("nested")])),
            (
                "nested",
                BeatNode::sequence("Nested", vec![id("prefix"), id("voice")]),
            ),
            ("prefix", silence(1)),
            ("voice", source(ntsc(), 4)),
            ("outer", source(ntsc(), 5)),
        ],
    );
    let after = ripple(&before, "prefix", "nested-delete");
    assert!(after.nodes().contains_key(&id("group")));
    assert!(after.nodes().contains_key(&id("nested")));
    let mut provider = Provider::new();
    // The inner voice moves [1,5) to [0,4); the outer sibling moves
    // [5,10) to [4,9). Their sample lengths remain 6406 and 8008.
    for (old, new, count, phase) in [
        (1602, 0, 6406, ratio(2, 5)),
        (8008, 6406, 8008, ExactRatio::ZERO),
    ] {
        let oracle = retained_pcm(&before, &mut provider, old, count);
        assert_close(&oracle[..128], &expected(&provider, phase, 128));
        check_retained(&after, &mut provider, new, &oracle);
    }
}

#[test]
fn delete_preserves_roomtone_loop_phase_through_the_final_sample() {
    let before = document(
        ntsc(),
        &["prefix", "room"],
        vec![
            ("prefix", silence(1)),
            ("room", BeatNode::hold("Room tone", room(4, 700..921))),
        ],
    );
    let after = ripple(&before, "prefix", "room-delete");
    let mut provider = Provider::new();
    let oracle = retained_pcm(&before, &mut provider, 1602, 6406);
    // The authored loop extent is 221*160/147 mix samples with a 96-sample
    // overlap. Its frame-1 entry retains the independent 2/5 sample phase.
    let wave = room_reference(&provider, 700..921, 6407);
    let entry = sample_reference(&wave, ratio(2, 5), ExactRatio::ONE, 128);
    assert_close(&oracle[..128], &entry);
    check_retained(&after, &mut provider, 0, &oracle);
}

#[test]
fn repeated_deletes_keep_preexisting_insert_time_bindings() {
    let original = source_fixture();
    let inserted = insert_pause(&original, 1, 2, "inserted");
    let once = ripple(&inserted, "prefix", "first-delete");
    let twice = ripple(&once, "inserted-pause", "second-delete");
    assert!(!inserted.audio_bindings().is_empty());
    let mut provider = Provider::new();
    let oracle = retained_pcm(&original, &mut provider, 1602, 6406);
    assert_close(&oracle[..128], &expected(&provider, ratio(2, 5), 128));
    // Intermediate [2,6) allocates one additional sample. Check all 6406
    // retained old samples there, then the complete final [0,4) suffix.
    check_retained(&inserted, &mut provider, 4805, &oracle);
    check_retained(&once, &mut provider, 3203, &oracle);
    check_retained(&twice, &mut provider, 0, &oracle);
}

#[test]
fn delete_preserves_repeat_play_and_roomtone_gap_entries() {
    let mut repeated = repeat("voice", 2);
    let NodeKind::Repeat { gap, .. } = &mut repeated.kind else {
        unreachable!()
    };
    *gap = Some(room(1, 700..921));
    let before = document(
        ntsc(),
        &["prefix", "repeat"],
        vec![
            ("prefix", silence(1)),
            ("repeat", repeated),
            ("voice", source(ntsc(), 4)),
        ],
    );
    let after = ripple(&before, "prefix", "repeat-delete");
    let mut provider = Provider::new();
    for (old, new) in [(1602, 0), (9610, 8008)] {
        let oracle = retained_pcm(&before, &mut provider, old, 6406);
        assert_close(&oracle[..128], &expected(&provider, ratio(2, 5), 128));
        check_retained(&after, &mut provider, new, &oracle);
    }
    let gap = retained_pcm(&before, &mut provider, 8008, 1602);
    let wave = room_reference(&provider, 700..921, 1602);
    assert_close(&gap[..128], &wave[..128]);
    check_retained(&after, &mut provider, 6406, &gap);
}

#[test]
fn delete_before_preserve_crop_retains_full_preparation_history() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let before = document(
        rate,
        &["prefix", "crop", "tail"],
        vec![
            ("prefix", silence(7)),
            ("crop", partition("stage", 64..1536)),
            ("stage", preserve("voice", 512, 1536)),
            ("voice", source(rate, 512)),
            ("tail", silence(11)),
        ],
    );
    let after = ripple(&before, "prefix", "preserve-delete");
    let mut provider = Provider::new();
    // One frame is one mix sample. The source context is exactly
    // [100,ceil(100+512*147/160)) = [100,571). Build the entire 1536-point
    // 1/3-speed history independently, then take the authored [64,1536) crop.
    let mut input = Vec::new();
    for start in [0, 256] {
        input.extend(
            provider
                .prepared
                .prepare(
                    ResampleRecipe::new(
                        100..571,
                        ExactRatio::integer(100),
                        AudioSample(0),
                        ratio(147, 160),
                        AudioSample(0)..AudioSample(512),
                    )
                    .unwrap(),
                    AudioSample(start),
                    256,
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples,
        );
    }
    let full = stretched(&input, 1536);
    let oracle = retained_pcm(&before, &mut provider, 7, 1472);
    assert_close(&oracle, &full[64..1536]);
    check_retained(&after, &mut provider, 0, &oracle);
    check_silence(&after, &mut provider, 1472..1483);
}

#[test]
fn delete_preserves_fractional_ntsc_preserve_output_entry() {
    let before = document(
        ntsc(),
        &["prefix", "stage"],
        vec![
            ("prefix", silence(1)),
            ("stage", preserve("voice", 4, 5)),
            ("voice", source(ntsc(), 4)),
        ],
    );
    let after = ripple(&before, "prefix", "ntsc-preserve-delete");
    let mut provider = Provider::new();
    // Four NTSC frames have exact extent 6406.4 mix samples, retained on
    // PointCeil as 6407 input points. At 44.1 kHz the source context is
    // [100,ceil(100+6406.4*147/160)) = [100,5986). Five output frames are
    // exactly 8008 points. The authored Preserve rate remains 4/5.
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
    let input = StereoPcm::new(
        input.iter().map(|point| point[0]).collect(),
        input.iter().map(|point| point[1]).collect(),
    )
    .unwrap();
    let recipe =
        CanonicalRecipe::with_rate(input.frames(), 8008, StretchRate::new(4, 5).unwrap(), 0)
            .unwrap();
    let mut dsp = CanonicalStretch::new(recipe, input).unwrap();
    let mut left = vec![0.; 8008];
    let mut right = vec![0.; 8008];
    for (left, right) in left.chunks_mut(256).zip(right.chunks_mut(256)) {
        assert_eq!(
            dsp.read(left, right, &AtomicBool::new(false)).unwrap(),
            left.len()
        );
    }
    let full: Vec<_> = left.into_iter().zip(right).map(|(l, r)| [l, r]).collect();
    // Original [1,6) is [1602,9610), so its entry into the complete
    // Preserve output is independently 1602-1601.6 = 2/5 of a point.
    // Deleted [0,5) also has 8008 samples and must retain that old entry.
    let entry = sample_reference(&full, ratio(2, 5), ExactRatio::ONE, 128);
    let oracle = retained_pcm(&before, &mut provider, 1602, 8008);
    assert_close(&oracle[..128], &entry);
    check_retained(&after, &mut provider, 0, &oracle);
}
