//! Atomic edited-slice placement, with destination allocations derived directly
//! from absolute frame boundaries and independent retained source support.
use super::*;

fn identities(slice: &CapturedEditSlice, name: &str) -> SlicePasteIdentities {
    let required = slice.identity_requirements().unwrap();
    SlicePasteIdentities {
        authored: OccurrenceIdentities {
            nodes: (0..required.nodes)
                .map(|i| id(&format!("{name}-node-{i}")))
                .collect(),
            marks: (0..required.marks)
                .map(|i| MarkId::new(format!("{name}-mark-{i}")).unwrap())
                .collect(),
        },
        aliases: (0..required.aliases)
            .map(|i| id(&format!("{name}-alias-{i}")))
            .collect(),
    }
}

fn splits(count: usize, name: &str) -> SplitIdentities {
    SplitIdentities {
        nodes: (0..count)
            .map(|i| id(&format!("{name}-split-{i}")))
            .collect(),
    }
}

fn interior(
    document: &ProjectDocument,
    parent: &str,
    target: &NodeId,
    at: i64,
    slice: &CapturedEditSlice,
    name: &str,
) -> ProjectDocument {
    let query = document
        .slice_splice_interior(&id(parent), target, frames(at), slice)
        .unwrap();
    let result = edit(
        document,
        name,
        Command::SpliceSliceAt {
            parent: id(parent),
            target: target.clone(),
            at: frames(at),
            slice: slice.clone(),
            identities: identities(slice, name),
            split_identities: splits(query.required_ids, name),
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    );
    assert_eq!(
        result.duration().unwrap().frames(),
        document.duration().unwrap().frames() + slice.duration().frames()
    );
    result
}

fn replace(
    document: &ProjectDocument,
    parent: &str,
    selected: Range<i64>,
    slice: &CapturedEditSlice,
    name: &str,
) -> ProjectDocument {
    let range = FrameRange::new(ProjectFrame(selected.start), ProjectFrame(selected.end)).unwrap();
    let query = document
        .slice_replacement(&id(parent), range, slice)
        .unwrap();
    let result = edit(
        document,
        name,
        Command::ReplaceSlice {
            parent: id(parent),
            range,
            slice: slice.clone(),
            identities: identities(slice, name),
            split_identities: splits(query.required_ids, name),
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    );
    assert_eq!(
        result.duration().unwrap().frames(),
        document.duration().unwrap().frames() - (selected.end - selected.start)
            + slice.duration().frames()
    );
    result
}

fn count(start: i64, end: i64) -> usize {
    usize::try_from(boundary(end) - boundary(start)).unwrap()
}

// Source natural-rate extent is 1601.6 mix points per project frame. The
// admitted provider endpoint is ceil(100 + frames*1471.47) original samples.
// Keep that complete endpoint for reconstruction, including the final point.
// A transferred RootRoundEven operand additionally needs its retained discrete
// root support, checked separately in the complete-owner terminal fixture below.
pub(super) fn bounded_source(
    provider: &Provider,
    host_frames: i64,
    phase: ExactRatio,
    count: usize,
) -> Vec<[f32; 2]> {
    let extent = ratio(i128::from(host_frames) * 8008, 5);
    let end = i64::try_from(
        ratio(10_000 + i128::from(host_frames) * 147147, 100)
            .ceil()
            .unwrap(),
    )
    .unwrap();
    let mut result: Vec<_> = (0..count)
        .step_by(256)
        .flat_map(|offset| {
            provider
                .prepared
                .prepare(
                    ResampleRecipe::new(
                        100..end,
                        ExactRatio::integer(100)
                            .checked_add(phase.checked_mul(ratio(147, 160)).unwrap())
                            .unwrap(),
                        AudioSample(0),
                        ratio(147, 160),
                        AudioSample(0)..AudioSample(i64::try_from(count).unwrap()),
                    )
                    .unwrap(),
                    AudioSample(i64::try_from(offset).unwrap()),
                    u32::try_from((count - offset).min(256)).unwrap(),
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples
        })
        .collect();
    for (index, sample) in result.iter_mut().enumerate() {
        if phase
            .checked_add(ExactRatio::integer(i64::try_from(index).unwrap()))
            .unwrap()
            .checked_sub(extent)
            .unwrap()
            .compare_integer(0)
            .is_ge()
        {
            *sample = [0.; 2];
        }
    }
    result
}

fn observed_against(
    document: &ProjectDocument,
    provider: &mut Provider,
    start: i64,
    oracle: &[[f32; 2]],
) -> Vec<[f32; 2]> {
    let observed = pcm(document, provider, start, oracle.len());
    assert_close(&observed, oracle);
    cold_reverse(document, provider, start, &observed);
    observed
}

fn source_slice(selected: Range<i64>) -> CapturedEditSlice {
    let source = document(ntsc(), &["captured"], vec![("captured", source(ntsc(), 9))]);
    capture(&source, "root", selected)
}

pub(super) fn prior_edits() -> ProjectDocument {
    let original = document(
        ntsc(),
        &["lead", "voice", "tail"],
        vec![
            ("lead", silence(1)),
            ("voice", source(ntsc(), 7)),
            ("tail", source(ntsc(), 5)),
        ],
    );
    let paused = insert_pause(&original, 1, 1, "placement-prior-pause");
    cut(&paused, 3..5, "placement-prior-cut")
}

#[test]
fn equal_replacement_keeps_the_final_4804_sample_without_a_shorter_intermediate_clock() {
    let original = document(ntsc(), &["voice"], vec![("voice", source(ntsc(), 3))]);
    let slice = source_slice(2..3);
    let replaced = replace(&original, "root", 1..2, &slice, "equal-middle");
    let mut provider = Provider::new();
    let prefix = pcm(&original, &mut provider, 0, 1602);
    let suffix = pcm(&original, &mut provider, 3203, 1602);
    let oracle = bounded_source(&provider, 3, ExactRatio::integer(3203), 1602);
    assert_close(&suffix, &oracle);
    assert_ne!(
        suffix[1601], [0.; 2],
        "old sample4804 is real source content"
    );
    cold_reverse(&replaced, &mut provider, 0, &prefix);
    cold_reverse(&replaced, &mut provider, 3203, &suffix);
    let inserted = bounded_source(&provider, 9, ExactRatio::integer(3203), 1601);
    observed_against(&replaced, &mut provider, 1602, &inserted);
}

#[test]
fn nested_interior_keeps_source_and_roomtone_suffixes_and_prior_copied_phase() {
    let captured = prior_edits();
    let slice = capture(&captured, "root", 4..6);
    for roomtone in [false, true] {
        let voice = if roomtone {
            BeatNode::hold("Room", room(4, 700..921))
        } else {
            source(ntsc(), 4)
        };
        let original = document(
            ntsc(),
            &["lead", "group", "tail"],
            vec![
                ("lead", source(ntsc(), 1)),
                ("group", BeatNode::sequence("Group", vec![id("voice")])),
                ("voice", voice),
                ("tail", source(ntsc(), 5)),
            ],
        );
        let inserted = interior(
            &original,
            "group",
            &id("voice"),
            1,
            &slice,
            "nested-interior-copy",
        );
        let mut provider = Provider::new();
        let prefix = pcm(&original, &mut provider, 0, 3203);
        let suffix = pcm(&original, &mut provider, 3203, 4805);
        let suffix_oracle = if roomtone {
            let full = room_reference(&provider, 700..921, 6407);
            sampled_reference(&full, ratio(8007, 5), 4805)
        } else {
            bounded_source(&provider, 4, ratio(8007, 5), 4805)
        };
        assert_close(&suffix, &suffix_oracle);
        cold_reverse(&inserted, &mut provider, 0, &prefix);
        cold_reverse(&inserted, &mut provider, 6406, &suffix);
        let copied = bounded_source(&provider, 7, ratio(32032, 5), 3203);
        observed_against(&inserted, &mut provider, 3203, &copied);
        let tail = pcm(&original, &mut provider, 8008, 8008);
        cold_reverse(&inserted, &mut provider, 11211, &tail);
    }
}

#[test]
fn shorter_equal_and_longer_replacements_use_exact_new_allocations_and_old_support() {
    for roomtone in [false, true] {
        for duration in [1, 2, 4] {
            let voice = if roomtone {
                BeatNode::hold("Room", room(6, 700..921))
            } else {
                source(ntsc(), 6)
            };
            let original = document(
                ntsc(),
                &["lead", "group", "tail"],
                vec![
                    ("lead", source(ntsc(), 1)),
                    ("group", BeatNode::sequence("Group", vec![id("voice")])),
                    ("voice", voice),
                    ("tail", source(ntsc(), 5)),
                ],
            );
            let slice = source_slice(1..1 + duration);
            let replaced = replace(&original, "group", 2..4, &slice, "resized-replacement");
            let mut provider = Provider::new();
            let prefix = pcm(&original, &mut provider, 0, 3203);
            cold_reverse(&replaced, &mut provider, 0, &prefix);
            let copied = bounded_source(
                &provider,
                9,
                ExactRatio::integer(1602),
                count(2, 2 + duration),
            );
            observed_against(&replaced, &mut provider, 3203, &copied);
            // The old suffix enters at B(4)-1601.6=4804.4. The longer
            // replacement gives it 4804 samples; the other placements give 4805.
            // Derive every result from that phase and its actual interval.
            let suffix_count = count(2 + duration, 5 + duration);
            assert_eq!(suffix_count, if duration == 4 { 4804 } else { 4805 });
            let suffix = if roomtone {
                let full = room_reference(&provider, 700..921, 9610);
                sampled_reference(&full, ratio(24022, 5), suffix_count)
            } else {
                bounded_source(&provider, 6, ratio(24022, 5), suffix_count)
            };
            let observed =
                observed_against(&replaced, &mut provider, boundary(2 + duration), &suffix);
            assert_ne!(observed[suffix_count - 1], [0.; 2]);
            let tail = pcm(&original, &mut provider, 11211, 8008);
            cold_reverse(&replaced, &mut provider, boundary(5 + duration), &tail);
        }
    }
}

#[test]
fn interior_in_a_previously_cut_fragment_composes_both_retained_histories() {
    let original = prior_edits();
    let slice = capture(&original, "root", 4..6);
    let NodeKind::Sequence { children } = &original.nodes()[original.root()].kind else {
        unreachable!()
    };
    let inserted = interior(&original, "root", &children[3], 1, &slice, "bound-interior");
    let mut provider = Provider::new();
    let prefix = pcm(&original, &mut provider, 0, 6406);
    cold_reverse(&inserted, &mut provider, 0, &prefix);
    let copied = bounded_source(&provider, 7, ratio(32032, 5), 3204);
    observed_against(&inserted, &mut provider, 6406, &copied);
    let resumed = bounded_source(&provider, 7, ratio(32032, 5), 4804);
    observed_against(&inserted, &mut provider, 9610, &resumed);
    let tail = pcm(&original, &mut provider, 11211, 8008);
    cold_reverse(&inserted, &mut provider, 14414, &tail);
}

#[test]
fn copied_nested_partitions_remain_editable_by_interior_and_range_replacement() {
    let original = document(
        ntsc(),
        &["outer"],
        vec![
            ("outer", partition("inner", 1..7)),
            ("inner", partition("voice", 1..8)),
            ("voice", source(ntsc(), 8)),
        ],
    );
    let slice = capture(&original, "root", 1..5);
    let first = paste(&destination(1, 5), &slice, 1, "nested-paste");
    let parent = "nested-paste-node-0";
    let NodeKind::Sequence { children } = &first.nodes()[&id(parent)].kind else {
        unreachable!()
    };
    let one = source_slice(0..1);
    let inserted = interior(&first, parent, &children[0], 2, &one, "partition-interior");
    let mut provider = Provider::new();
    let prefix = pcm(&first, &mut provider, 0, 4805);
    cold_reverse(&inserted, &mut provider, 0, &prefix);
    let copied = bounded_source(&provider, 9, ExactRatio::ZERO, 1601);
    observed_against(&inserted, &mut provider, 4805, &copied);
    // Captured Source entry is 4805.2. Advancing B(3)-B(1)=3203 makes
    // the split resume 8008.2; the new allocation has 3204 supported samples.
    let resumed = bounded_source(&provider, 8, ratio(40041, 5), 3204);
    observed_against(&inserted, &mut provider, 6406, &resumed);
    let old_tail = pcm(&first, &mut provider, 8008, 8008);
    cold_reverse(&inserted, &mut provider, 9610, &old_tail);
    let two = source_slice(0..2);
    let replaced = replace(&inserted, parent, 2..5, &two, "partition-replacement");
    let retained_prefix = pcm(&inserted, &mut provider, 0, 3203);
    cold_reverse(&replaced, &mut provider, 0, &retained_prefix);
    let replacement = bounded_source(&provider, 9, ExactRatio::ZERO, 3203);
    observed_against(&replaced, &mut provider, 3203, &replacement);
    let retained_suffix = pcm(&inserted, &mut provider, 8008, 1602);
    let suffix_oracle = bounded_source(&provider, 8, ratio(48051, 5), 1602);
    assert_close(&retained_suffix, &suffix_oracle);
    cold_reverse(&replaced, &mut provider, 6406, &retained_suffix);
    cold_reverse(&replaced, &mut provider, 8008, &old_tail);
}

#[test]
fn interior_and_replacement_keep_imported_repeat_and_complete_preserve_contexts() {
    let repeated = repeated_history();
    let preserved = document(
        ntsc(),
        &["lead", "window"],
        vec![
            ("lead", silence(1)),
            ("window", partition("stage", 2..11)),
            ("stage", preserve("voice", 4, 12)),
            ("voice", source(ntsc(), 4)),
        ],
    );
    for (captured, selected) in [(&repeated, 5..19), (&preserved, 1..10)] {
        let slice = capture(captured, "root", selected.clone());
        let start = selected.start;
        let mut provider = Provider::new();
        let saved = pcm(
            captured,
            &mut provider,
            boundary(start),
            count(start, selected.end),
        );
        if start == 1 {
            let full = preserve_reference(&provider);
            assert_close(
                &saved,
                &sampled_reference(&full, ratio(16018, 5), saved.len()),
            );
        } else {
            assert_close(&saved[..128], &expected(&provider, ExactRatio::ZERO, 128));
        }
        for replacement in [false, true] {
            let original = document(
                ntsc(),
                &["voice", "tail"],
                vec![("voice", source(ntsc(), 7)), ("tail", source(ntsc(), 5))],
            );
            let placed = if replacement {
                replace(
                    &original,
                    "root",
                    start..start + 1,
                    &slice,
                    "composite-replacement",
                )
            } else {
                interior(
                    &original,
                    "root",
                    &id("voice"),
                    start,
                    &slice,
                    "composite-interior",
                )
            };
            let prefix = pcm(
                &original,
                &mut provider,
                0,
                usize::try_from(boundary(start)).unwrap(),
            );
            cold_reverse(&placed, &mut provider, 0, &prefix);
            cold_reverse(&placed, &mut provider, boundary(start), &saved);
            let removed = i64::from(replacement);
            let suffix_start = start + slice.duration().frames();
            let suffix_end = 7 + slice.duration().frames() - removed;
            let mut suffix = bounded_source(
                &provider,
                7,
                ExactRatio::integer(boundary(start + removed)),
                count(suffix_start, suffix_end),
            );
            // Source7 was sampled on the old absolute root grid: [0,B(7))
            // is [0,11211). Its geometric extent is 11211.2 mix points, but
            // point11211 is outside that discrete operand. Keep the complete
            // destination allocation and explicitly suppress exhausted labels.
            let retained_start = boundary(start + removed);
            for (offset, sample) in suffix.iter_mut().enumerate() {
                if retained_start + i64::try_from(offset).unwrap() >= 11211 {
                    *sample = [0.; 2];
                }
            }
            let plan = RenderPlan::compile(&placed).unwrap();
            let last = AudioSample(boundary(suffix_end) - 1);
            let query = plan
                .audio_processing(last..AudioSample(last.0 + 1), Default::default())
                .unwrap();
            let span = &query.spans[0];
            let deadpan_plan::AudioSignalContent::Bound(bound) = &span.content else {
                panic!("retained suffix must carry its old sampling grid")
            };
            let deadpan_plan::AudioBoundDomain::Root(domain) = bound.raw_domain().unwrap() else {
                panic!("destination Source owns a RootRoundEven grid")
            };
            assert_eq!(
                domain.root_extent(),
                ExactRatio::ZERO..ExactRatio::integer(7)
            );
            assert_eq!(domain.root_samples(), AudioSample(0)..AudioSample(11211));
            assert_eq!(
                bound
                    .reference_at_offset(last.0 - span.allocated_samples.start.0)
                    .unwrap(),
                ExactRatio::integer(
                    boundary(start + removed) + i64::try_from(suffix.len()).unwrap() - 1
                )
            );
            let observed =
                observed_against(&placed, &mut provider, boundary(suffix_start), &suffix);
            let mut terminal_reader = renderer(&placed, &mut provider);
            let terminal = terminal_reader
                .read(&mut provider, last, 1, TIMEOUT, &AtomicBool::new(false))
                .unwrap();
            if retained_start + i64::try_from(suffix.len()).unwrap() - 1 == 11211 {
                assert_eq!(observed[observed.len() - 1], [0.; 2]);
                assert_eq!(terminal.suppressed, vec![last..AudioSample(last.0 + 1)]);
                if start == 5 && !replacement {
                    assert_eq!(
                        terminal.suppressed,
                        vec![AudioSample(33633)..AudioSample(33634)]
                    );
                }
            } else {
                assert_ne!(observed[observed.len() - 1], [0.; 2]);
                assert!(terminal.suppressed.is_empty());
            }
            let tail = pcm(&original, &mut provider, 11211, 8008);
            cold_reverse(&placed, &mut provider, boundary(suffix_end), &tail);
        }
    }
}

#[test]
fn partial_endpoint_replacement_across_repeat_and_preserve_keeps_both_outer_joins() {
    let mut repeated = repeat("repeated-voice", 2);
    let NodeKind::Repeat { gap, .. } = &mut repeated.kind else {
        unreachable!()
    };
    *gap = Some(room(1, 700..921));
    let original = document(
        ntsc(),
        &["lead", "repeat", "stage", "tail"],
        vec![
            ("lead", source(ntsc(), 3)),
            ("repeat", repeated),
            ("repeated-voice", source(ntsc(), 2)),
            ("stage", preserve("stage-voice", 4, 12)),
            ("stage-voice", source(ntsc(), 4)),
            ("tail", source(ntsc(), 4)),
        ],
    );
    assert_eq!(original.duration().unwrap(), frames(24));
    let captured = prior_edits();
    let slice = capture(&captured, "root", 4..6);
    let replaced = replace(&original, "root", 1..22, &slice, "across-composites");
    let mut provider = Provider::new();
    let prefix = pcm(&original, &mut provider, 0, 1602);
    cold_reverse(&replaced, &mut provider, 0, &prefix);
    let copied = bounded_source(&provider, 7, ratio(32032, 5), 3203);
    observed_against(&replaced, &mut provider, 1602, &copied);
    let suffix = pcm(&original, &mut provider, 35235, 3203);
    assert_close(
        &suffix,
        &bounded_source(&provider, 4, ExactRatio::integer(3203), 3203),
    );
    assert_ne!(suffix[3202], [0.; 2]);
    cold_reverse(&replaced, &mut provider, 4805, &suffix);
    for removed in ["repeat", "repeated-voice", "stage", "stage-voice"] {
        assert!(!replaced.nodes().contains_key(&id(removed)));
    }
    let mut cold = renderer(&replaced, &mut provider);
    assert_eq!(read(&mut cold, &mut provider, 7880, 128), suffix[3075..]);
    assert_eq!(cold.cached_stage_count(), 0);
}
