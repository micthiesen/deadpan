//! Real decoded PCM, including cold seeks through partial Preserve contexts.
use super::*;

fn wrap(before: &ProjectDocument, start: i64, end: i64, plays: u32) -> ProjectDocument {
    let selection = SliceCaptureSelection::Range {
        range: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
    };
    let plan = before
        .repeat_selection(&id("root"), &selection, plays)
        .unwrap();
    edit(
        before,
        "repeated-range",
        Command::RepeatSelection {
            parent: id("root"),
            selection,
            plays,
            identities: RepeatSelectionIdentities {
                repeat: id("new-repeat"),
                group: plan.needs_group.then(|| id("body")),
                split: SplitIdentities {
                    nodes: (0..plan.required_split_ids)
                        .map(|n| id(&format!("split-{n}")))
                        .collect(),
                },
            },
            timing: AudioTimingId {
                allocation: revision("repeated-range"),
                ordinal: 0,
            },
        },
    )
}

fn pcm(
    document: &ProjectDocument,
    provider: &mut Provider,
    start: i64,
    count: usize,
) -> Vec<[f32; 2]> {
    let mut reader = renderer(document, provider);
    (0..count)
        .step_by(193)
        .flat_map(|offset| {
            read(
                &mut reader,
                provider,
                start + i64::try_from(offset).unwrap(),
                u32::try_from((count - offset).min(193)).unwrap(),
            )
        })
        .collect()
}

fn retained(
    document: &ProjectDocument,
    provider: &mut Provider,
    start: i64,
    expected: &[[f32; 2]],
) {
    assert!(expected.iter().flatten().all(|sample| sample.is_finite()));
    assert!(expected.iter().flatten().any(|sample| sample.abs() > 0.001));
    let mut reader = renderer(document, provider);
    // Reverse, irregular reads start at the cold tail, then revisit the entry.
    for offset in (0..expected.len()).step_by(113).rev() {
        let count = (expected.len() - offset).min(113);
        assert_eq!(
            read(
                &mut reader,
                provider,
                start + i64::try_from(offset).unwrap(),
                u32::try_from(count).unwrap()
            ),
            expected[offset..offset + count],
            "retained sample {offset}"
        );
    }
    let count = expected.len().min(97);
    assert_eq!(
        read(&mut reader, provider, start, u32::try_from(count).unwrap()),
        expected[..count]
    );
}

#[test]
fn partial_preserve_repeat_retains_complete_intrinsic_history_on_every_play() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let before = document(
        rate,
        &["stage"],
        vec![
            ("stage", preserve("voice", 128, 384)),
            ("voice", source(rate, 128)),
        ],
    );
    let after = wrap(&before, 120, 240, 3);
    let mut provider = Provider::new();
    let prefix = pcm(&before, &mut provider, 0, 120);
    let body = pcm(&before, &mut provider, 120, 120);
    let suffix = pcm(&before, &mut provider, 240, 144);
    retained(&after, &mut provider, 0, &prefix);
    for start in [120, 240, 360] {
        retained(&after, &mut provider, start, &body);
    }
    retained(&after, &mut provider, 480, &suffix);
    assert_eq!(after.duration().unwrap(), frames(624));
}
