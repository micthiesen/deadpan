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
