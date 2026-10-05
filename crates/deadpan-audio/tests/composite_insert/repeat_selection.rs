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
fn ntsc_range_repeat_keeps_first_play_and_suffix_samples_exact() {
    let before = document(ntsc(), &["voice"], vec![("voice", source(ntsc(), 8))]);
    let after = wrap(&before, 1, 3, 3);
    let mut provider = Provider::new();
    // B(3)=4805, B(7)=11211, B(8)=12813, B(12)=19219.
    let first = pcm(&before, &mut provider, 0, 4805);
    let suffix = pcm(&before, &mut provider, 4805, 8008);
    retained(&after, &mut provider, 0, &first);
    for start in [4805, 8008] {
        retained(&after, &mut provider, start, &first[1602..]);
    }
    retained(&after, &mut provider, 11211, &suffix);
    assert_eq!(after.duration().unwrap(), frames(12));
    assert_close(
        &first[1602..1730],
        &expected(&provider, ExactRatio::integer(1602), 128),
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

#[test]
fn resizing_repeated_range_preserves_surviving_pcm_and_suffix() {
    let before = document(ntsc(), &["voice"], vec![("voice", source(ntsc(), 8))]);
    let repeated = wrap(&before, 1, 3, 3);
    let after = edit(
        &repeated,
        "fewer-plays",
        Command::SetRepeatPlays {
            node: id("new-repeat"),
            plays: 2,
            timing: AudioTimingId {
                allocation: revision("fewer-plays"),
                ordinal: 0,
            },
        },
    );
    let mut provider = Provider::new();
    let retained_plays = pcm(&repeated, &mut provider, 0, 8008);
    let suffix = pcm(&before, &mut provider, 4805, 8008);
    retained(&after, &mut provider, 0, &retained_plays);
    retained(&after, &mut provider, 8008, &suffix);
    assert_eq!(after.duration().unwrap(), frames(10));
}

fn set_gaps(
    before: &ProjectDocument,
    name: &str,
    gap: Option<i64>,
    branches: &[(u32, &str, i64)],
) -> ProjectDocument {
    let NodeKind::Repeat { iterations, .. } = &before.nodes()[&id("new-repeat")].kind else {
        panic!("wrapped Repeat")
    };
    let iterations = iterations.clone();
    edit(
        before,
        name,
        Command::SetRepeatGaps {
            node: id("new-repeat"),
            gap: gap.map(|duration| super::gap(duration, HoldAudio::Silence)),
            branches: branches
                .iter()
                .map(|(after, node, duration)| RepeatGapHold {
                    after: iterations.at(*after).unwrap(),
                    id: id(node),
                    hold: super::gap(*duration, HoldAudio::Silence),
                })
                .collect(),
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    )
}

fn assert_silent(document: &ProjectDocument, provider: &mut Provider, start: i64, count: usize) {
    let samples = pcm(document, provider, start, count);
    assert!(
        samples.iter().flatten().all(|sample| *sample == 0.0),
        "silence at {start}"
    );
}

/// At 30000/1001 a frame is 1,601.6 samples, so shifting a play by whole
/// frames moves its rounded start by a varying number of samples. Every play
/// must still be the identical retained samples, placed at its new rounded
/// start, with silent gaps and an exactly retained suffix.
#[test]
fn ntsc_gap_changes_move_each_play_and_the_suffix_as_identical_samples() {
    let before = document(ntsc(), &["voice"], vec![("voice", source(ntsc(), 8))]);
    let repeated = wrap(&before, 1, 3, 3);
    let mut provider = Provider::new();
    // B(1)=1602, B(3)=4805: one play is 3,203 samples.
    let body = pcm(&before, &mut provider, 1602, 3203);
    let prefix = pcm(&before, &mut provider, 0, 1602);
    let suffix = pcm(&before, &mut provider, 4805, 8008);
    // Plays at frames 1, 5 and 8 after a 2-frame gap then a 1-frame gap:
    // B(5)=8008, B(8)=12813, B(10)=16016; gaps [4805, 8008) and [11211, 12813).
    let gapped = set_gaps(&repeated, "gaps", Some(2), &[(1, "short", 1)]);
    assert_eq!(gapped.duration().unwrap(), frames(15));
    retained(&gapped, &mut provider, 0, &prefix);
    for start in [1602, 8008, 12813] {
        retained(&gapped, &mut provider, start, &body);
    }
    assert_silent(&gapped, &mut provider, 4805, 3203);
    assert_silent(&gapped, &mut provider, 11211, 1602);
    retained(&gapped, &mut provider, 16016, &suffix);
    // Changing the gaps again moves the same samples: plays at frames 1, 6
    // and 9 (B(6)=9610, B(9)=14414), suffix at B(11)=17618. Without
    // reanchoring the interior, play 3 accumulated one rounding and landed
    // at 14,415 (12,813 + 1,602) instead of its own clock's 14,414.
    let wider = set_gaps(&gapped, "wider", Some(3), &[(1, "shorter", 1)]);
    assert!(!wider.nodes().contains_key(&id("short")));
    for start in [1602, 9610, 14414] {
        retained(&wider, &mut provider, start, &body);
    }
    retained(&wider, &mut provider, 17618, &suffix);
    // Removing every gap restores the original play and suffix samples.
    let none = set_gaps(&wider, "none", None, &[]);
    assert_eq!(none.duration().unwrap(), frames(12));
    for start in [1602, 4805, 8008] {
        retained(&none, &mut provider, start, &body);
    }
    retained(&none, &mut provider, 11211, &suffix);
}
