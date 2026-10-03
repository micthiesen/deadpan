//! Actual decoded PCM checks for immutable edited-content capture. Raw timing
//! uses StageAudio::read; gain ownership is checked separately on the authored
//! bus. This does not qualify mastering, acoustic quality or device delivery.
use super::*;

#[path = "edited_slice/move_range.rs"]
mod move_range;
#[path = "edited_slice/placement.rs"]
mod placement;
#[path = "edited_slice/replacement_children.rs"]
mod replacement_children;

fn boundary(frame: i64) -> i64 {
    ntsc().audio_boundary(ProjectFrame(frame)).unwrap().0
}

fn capture(document: &ProjectDocument, parent: &str, range: Range<i64>) -> CapturedEditSlice {
    let before = document.clone();
    let slice = CapturedEditSlice::capture(
        document,
        &id(parent),
        FrameRange::new(ProjectFrame(range.start), ProjectFrame(range.end)).unwrap(),
        AudioTimingId {
            allocation: revision("capture-only"),
            ordinal: 0,
        },
    )
    .unwrap();
    assert_eq!(*document, before, "capture must be history-neutral");
    assert_eq!(slice.duration(), frames(range.end - range.start));
    slice
}

fn paste(
    document: &ProjectDocument,
    slice: &CapturedEditSlice,
    index: usize,
    name: &str,
) -> ProjectDocument {
    let required = slice.identity_requirements().unwrap();
    let after = edit(
        document,
        name,
        Command::SpliceSlice {
            parent: id("root"),
            index,
            slice: slice.clone(),
            identities: SlicePasteIdentities {
                authored: OccurrenceIdentities {
                    nodes: (0..required.nodes)
                        .map(|index| id(&format!("{name}-node-{index}")))
                        .collect(),
                    marks: (0..required.marks)
                        .map(|index| MarkId::new(format!("{name}-mark-{index}")).unwrap())
                        .collect(),
                },
                aliases: (0..required.aliases)
                    .map(|index| id(&format!("{name}-alias-{index}")))
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
        document.duration().unwrap().frames() + slice.duration().frames()
    );
    after
}

fn cut(document: &ProjectDocument, selected: Range<i64>, name: &str) -> ProjectDocument {
    let range = FrameRange::new(ProjectFrame(selected.start), ProjectFrame(selected.end)).unwrap();
    let required = document
        .range_deletion(&id("root"), range)
        .unwrap()
        .required_ids;
    edit(
        document,
        name,
        Command::DeleteRange {
            parent: id("root"),
            range,
            identities: SplitIdentities {
                nodes: (0..required).map(|i| id(&format!("{name}-{i}"))).collect(),
            },
            timing: AudioTimingId {
                allocation: revision(name),
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
    let mut result = Vec::with_capacity(count);
    for offset in (0..count).step_by(256) {
        result.extend(read(
            &mut reader,
            provider,
            start + i64::try_from(offset).unwrap(),
            u32::try_from((count - offset).min(256)).unwrap(),
        ));
    }
    result
}

fn source_reference(provider: &Provider, phase: ExactRatio, count: usize) -> Vec<[f32; 2]> {
    (0..count)
        .step_by(256)
        .flat_map(|offset| {
            expected(
                provider,
                phase
                    .checked_add(ExactRatio::integer(i64::try_from(offset).unwrap()))
                    .unwrap(),
                u32::try_from((count - offset).min(256)).unwrap(),
            )
        })
        .collect()
}

fn sampled_reference(input: &[[f32; 2]], phase: ExactRatio, count: usize) -> Vec<[f32; 2]> {
    (0..count)
        .step_by(256)
        .flat_map(|offset| {
            sample_reference(
                input,
                phase
                    .checked_add(ExactRatio::integer(i64::try_from(offset).unwrap()))
                    .unwrap(),
                ExactRatio::ONE,
                u32::try_from((count - offset).min(256)).unwrap(),
            )
        })
        .collect()
}

#[track_caller]
fn exact(actual: &[[f32; 2]], expected: &[[f32; 2]], offset: usize) {
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(actual, expected, "retained sample {}", offset + index);
    }
}

#[track_caller]
fn cold_reverse(
    document: &ProjectDocument,
    provider: &mut Provider,
    start: i64,
    expected: &[[f32; 2]],
) {
    assert!(expected.iter().flatten().all(|sample| sample.is_finite()));
    assert!(expected.iter().flatten().any(|sample| sample.abs() > 0.001));
    let mut reader = renderer(document, provider);
    let tail = expected.len().saturating_sub(173);
    exact(
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
        exact(
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
    exact(
        &read(&mut reader, provider, start, 128),
        &expected[..128],
        0,
    );
}

fn destination(prefix: i64, suffix: i64) -> ProjectDocument {
    document(
        ntsc(),
        &["prefix", "suffix"],
        vec![
            ("prefix", source(ntsc(), prefix)),
            ("suffix", source(ntsc(), suffix)),
        ],
    )
}

#[test]
fn nested_nonzero_source_and_roomtone_crop_keep_phase_and_exact_destination_allocation() {
    for roomtone in [false, true] {
        let leaf = if roomtone {
            BeatNode::hold("Room tone", room(7, 700..921))
        } else {
            source(ntsc(), 7)
        };
        let original = document(
            ntsc(),
            &["lead", "group"],
            vec![
                ("lead", silence(1)),
                ("group", BeatNode::sequence("Group", vec![id("voice")])),
                ("voice", leaf),
            ],
        );
        let slice = capture(&original, "group", 2..4);
        let destination = destination(4, 5);
        let pasted = paste(&destination, &slice, 1, "fractional-paste");
        let mut provider = Provider::new();
        // Source capture [B(2),B(4)) has 3203 samples. Destination [B(4),B(6))
        // has 3204: the additional point still has full owner support. Its
        // coordinate is entry B(2)-1601.6=1601.4 plus the physical offset.
        assert_eq!(boundary(4) - boundary(2), 3203);
        assert_eq!(boundary(6) - boundary(4), 3204);
        let oracle = if roomtone {
            let full = room_reference(&provider, 700..921, 11212);
            sampled_reference(&full, ratio(8007, 5), 3204)
        } else {
            source_reference(&provider, ratio(8007, 5), 3204)
        };
        let observed = pcm(&pasted, &mut provider, 6406, 3204);
        assert_close(&observed, &oracle);
        assert_ne!(
            observed[3203], [0.; 2],
            "extra allocated point has real support"
        );
        cold_reverse(&pasted, &mut provider, 6406, &observed);
        let prefix = pcm(&destination, &mut provider, 0, 6406);
        let suffix = pcm(&destination, &mut provider, 6406, 8008);
        cold_reverse(&pasted, &mut provider, 0, &prefix);
        cold_reverse(&pasted, &mut provider, 9610, &suffix);
    }
}

#[test]
fn immutable_capture_survives_source_change_and_deletion_and_pastes_independently() {
    let original = document(ntsc(), &["voice"], vec![("voice", source(ntsc(), 7))]);
    let slice = capture(&original, "root", 1..6);
    let snapshot = slice.clone();
    let NodeKind::Source {
        source: original_source,
    } = &original.nodes()[&id("voice")].kind
    else {
        unreachable!()
    };
    let changed = edit(
        &original,
        "changed-original",
        Command::SetSourceAudioMapping {
            node: id("voice"),
            mapping: original_source.audio_mapping,
            offset: AudioSample(701),
        },
    );
    let deleted = cut(&changed, 0..7, "deleted-original");
    assert_eq!(slice, snapshot);
    assert!(!deleted.nodes().contains_key(&id("voice")));
    let first = paste(&deleted, &slice, 0, "first-copy");
    let both = paste(&first, &slice, 1, "second-copy");
    let first_ids: BTreeSet<_> = first
        .nodes()
        .keys()
        .filter(|key| !deleted.nodes().contains_key(*key))
        .cloned()
        .collect();
    let second_ids: BTreeSet<_> = both
        .nodes()
        .keys()
        .filter(|key| !first.nodes().contains_key(*key))
        .cloned()
        .collect();
    assert!(first_ids.is_disjoint(&second_ids));
    let mut provider = Provider::new();
    let oracle = source_reference(&provider, ExactRatio::integer(1602), 8008);
    let saved = pcm(&original, &mut provider, 1602, 8008);
    assert_close(&saved, &oracle);
    cold_reverse(&both, &mut provider, 0, &saved);
    cold_reverse(&both, &mut provider, 8008, &saved);
    let copied_source = first_ids
        .iter()
        .find(|key| matches!(both.nodes()[*key].kind, NodeKind::Source { .. }))
        .unwrap();
    let changed_copy = edit(
        &both,
        "changed-first-copy",
        Command::SetSourceAudioMapping {
            node: copied_source.clone(),
            mapping: original_source.audio_mapping,
            offset: AudioSample(907),
        },
    );
    cold_reverse(&changed_copy, &mut provider, 8008, &saved);
    assert_ne!(pcm(&changed_copy, &mut provider, 0, 128), saved[..128]);
    assert_eq!(slice, snapshot);
}

#[test]
fn crop_retains_prior_insert_and_delete_resume_terms() {
    let original = document(
        ntsc(),
        &["lead", "voice", "tail"],
        vec![
            ("lead", silence(1)),
            ("voice", source(ntsc(), 7)),
            ("tail", source(ntsc(), 5)),
        ],
    );
    let paused = insert_pause(&original, 1, 1, "prior-pause");
    let cut = cut(&paused, 3..5, "prior-cut");
    assert!(!cut.audio_bindings().is_empty());
    let slice = capture(&cut, "root", 4..6);
    let destination = destination(5, 5);
    let pasted = paste(&destination, &slice, 0, "bound-paste");
    let mut provider = Provider::new();
    // Pause retains old frame1's +2/5 phase. Delete resumes local 4805.4;
    // advancing from current B(3) to B(4) adds 1601, yielding 6406.4.
    let oracle = source_reference(&provider, ratio(32032, 5), 3203);
    assert_close(&pcm(&cut, &mut provider, 6406, 3203), &oracle);
    let observed = pcm(&pasted, &mut provider, 0, 3203);
    assert_close(&observed, &oracle);
    cold_reverse(&pasted, &mut provider, 0, &observed);
    let old = pcm(&destination, &mut provider, 0, 16016);
    cold_reverse(&pasted, &mut provider, 3203, &old);
}

fn preserve_reference(provider: &Provider) -> Vec<[f32; 2]> {
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
    stretched(&input, 19220)
}

#[test]
fn preserve_partition_copy_keeps_complete_input_history_on_cold_final_read() {
    let original = document(
        ntsc(),
        &["lead", "window"],
        vec![
            ("lead", silence(1)),
            ("window", partition("stage", 2..11)),
            ("stage", preserve("voice", 4, 12)),
            ("voice", source(ntsc(), 4)),
        ],
    );
    let slice = capture(&original, "root", 1..10);
    let destination = destination(4, 5);
    let pasted = paste(&destination, &slice, 1, "preserve-copy");
    let mut provider = Provider::new();
    let full = preserve_reference(&provider);
    // Full output is 19220 PointCeil samples. The window retains phase
    // 2*1601.6 + (B(1)-1601.6) = 3203.6, independently of paste frame4.
    let oracle = sampled_reference(&full, ratio(16018, 5), 14415);
    let observed = pcm(&pasted, &mut provider, 6406, 14415);
    assert_close(&observed, &oracle);
    cold_reverse(&pasted, &mut provider, 6406, &observed);
    let mut cold = renderer(&pasted, &mut provider);
    assert_close(&read(&mut cold, &mut provider, 20757, 64), &oracle[14351..]);
    assert_eq!(cold.cached_stage_count(), 1);
    let prefix = pcm(&destination, &mut provider, 0, 6406);
    let suffix = pcm(&destination, &mut provider, 6406, 8008);
    cold_reverse(&pasted, &mut provider, 0, &prefix);
    cold_reverse(&pasted, &mut provider, 20821, &suffix);
}

#[test]
fn terminal_source_crop_allocates_the_destination_interval_without_invented_support() {
    let original = document(
        ntsc(),
        &["lead", "voice"],
        vec![("lead", silence(1)), ("voice", source(ntsc(), 2))],
    );
    let slice = capture(&original, "root", 1..3);
    let destination = destination(4, 5);
    let pasted = paste(&destination, &slice, 1, "terminal-copy");
    let mut provider = Provider::new();
    // Complete Source host support ends at ceil(100+3203.2*147/160)=3043.
    // Retained local entry is +0.4; the last of the 3204 output points is
    // outside the 3203.2-point owner support and must be zero.
    let oracle: Vec<_> = (0..3204)
        .step_by(256)
        .flat_map(|start| {
            provider
                .prepared
                .prepare(
                    ResampleRecipe::new(
                        100..3043,
                        ratio(80294, 800),
                        AudioSample(0),
                        ratio(147, 160),
                        AudioSample(0)..AudioSample(3204),
                    )
                    .unwrap(),
                    AudioSample(start),
                    u32::try_from((3204 - start).min(256)).unwrap(),
                    TIMEOUT,
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples
        })
        .collect();
    let observed = pcm(&pasted, &mut provider, 6406, 3204);
    assert_close(&observed[..3203], &oracle[..3203]);
    assert_ne!(observed[3202], [0.; 2]);
    assert_eq!(observed[3203], [0.; 2]);
    cold_reverse(&pasted, &mut provider, 6406, &observed);
}

fn with_uncaptured_repeat(document: &ProjectDocument) -> ProjectDocument {
    let captured = capture_unbound_audio_bindings(
        document,
        AudioTimingId {
            allocation: revision("before-repeat"),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut bindings = captured.bindings().clone();
    let lattice = &mut bindings.get_mut(&id("voice")).unwrap().lattice;
    // The outer Repeat does not exist in this old timing layout. An explicit
    // Run is legal here and retains its complete initial support through later
    // growth, reordering and retirement. All four original plays reuse phase0.
    lattice.births.push(AudioBirthClause {
        repeat: id("repeat"),
        survivors: AudioBirthSurvivors::Run {
            allocation: revision("plays"),
            first: 0,
            count: 4,
        },
        definition_root: id("voice"),
    });
    let state = AudioBindingState::new_with_gaps(
        captured
            .timings()
            .iter()
            .map(|(id, layout)| AudioTimingRecord {
                id: id.clone(),
                layout: layout.clone(),
            })
            .collect(),
        bindings,
        captured.gap_bindings().clone(),
    )
    .unwrap();
    let mut wire = serde_json::to_value(document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(state).unwrap();
    let mut repeated = repeat("voice", 4);
    let NodeKind::Repeat { gap, .. } = &mut repeated.kind else {
        unreachable!()
    };
    *gap = Some(room(1, 100..321));
    wire["nodes"]["repeat"] = serde_json::to_value(repeated).unwrap();
    wire["nodes"]["root"] = serde_json::to_value(BeatNode::sequence(
        "Root",
        vec![id("lead"), id("repeat"), id("tail")],
    ))
    .unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn repeated_history() -> ProjectDocument {
    let original = document(
        ntsc(),
        &["lead", "voice", "tail"],
        vec![
            ("lead", source(ntsc(), 5)),
            ("voice", source(ntsc(), 2)),
            ("tail", source(ntsc(), 6)),
        ],
    );
    let original = with_uncaptured_repeat(&original);
    let isolated = edit(
        &original,
        "isolated-gap",
        Command::IsolateGap {
            node: id("repeat"),
            iteration: play(0),
            id: id("independent-gap"),
            timing: AudioTimingId {
                allocation: revision("isolated-gap"),
                ordinal: 0,
            },
        },
    );
    let overridden = edit(
        &isolated,
        "override-play",
        Command::SetPlayOverride {
            node: id("repeat"),
            iteration: play(1),
            subtree: Subtree {
                root: id("override"),
                nodes: BTreeMap::from([(
                    id("override"),
                    BeatNode::hold("Override room", room(2, 900..1121)),
                )]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    );
    let changed_gap = set_gap(&overridden, "changed-gap", 4, room(1, 700..921));
    let paused = insert_pause(&changed_gap, 5, 1, "repeat-pause");
    let grown = set_gap(&paused, "new-plays", 6, room(1, 700..921));
    let reordered = edit(
        &grown,
        "reordered-plays",
        Command::MovePlays {
            node: id("repeat"),
            start: 4,
            end: 6,
            destination: 1,
        },
    );
    let reduced = set_gap(&reordered, "retired-play", 5, room(1, 700..921));
    let result = cut(&reduced, 1..2, "repeat-prior-cut");
    assert_eq!(result.duration().unwrap(), frames(25));
    let NodeKind::Repeat { iterations, .. } = &result.nodes()[&id("repeat")].kind else {
        unreachable!()
    };
    assert_eq!(iterations.at(0), Some(play(0)));
    assert_eq!(iterations.at(3), Some(play(1)));
    assert_eq!(iterations.at(4), Some(play(2)));
    assert_eq!(iterations.position(&play(3)), None);
    assert_eq!(iterations.segment_count(), 3);
    result
}

#[test]
fn copied_repeat_keeps_reordered_births_sparse_overrides_retired_runs_and_copy_of_copy() {
    let original = repeated_history();
    let slice = capture(&original, "root", 5..19);
    let NodeKind::Sequence { children } = &original.nodes()[original.root()].kind else {
        unreachable!()
    };
    let appended = paste(&original, &slice, children.len(), "repeat-copy");
    let mut provider = Provider::new();
    let saved = pcm(&original, &mut provider, 8008, 22422);
    assert_close(&saved[..128], &expected(&provider, ExactRatio::ZERO, 128));
    let first_gap = room_reference(&provider, 100..321, 1602);
    assert_close(
        &saved[3203..3331],
        &sample_reference(&first_gap, ratio(-1, 5), ExactRatio::ONE, 128),
    );
    // A 20-frame move is exactly 32032 samples. Every internal play and gap
    // boundary keeps the same allocation width, so the complete vector agrees.
    cold_reverse(&appended, &mut provider, 40040, &saved);
    let old_all = pcm(&original, &mut provider, 0, 40040);
    cold_reverse(&appended, &mut provider, 0, &old_all);
    let pasted_repeat = appended
        .nodes()
        .iter()
        .find_map(|(key, node)| {
            (!original.nodes().contains_key(key) && matches!(node.kind, NodeKind::Repeat { .. }))
                .then_some(key)
        })
        .unwrap();
    let NodeKind::Repeat { iterations, .. } = &appended.nodes()[pasted_repeat].kind else {
        unreachable!()
    };
    let fresh_plays: BTreeSet<_> = (0..iterations.len())
        .map(|index| iterations.at(index).unwrap())
        .collect();
    assert!(
        fresh_plays
            .iter()
            .all(|iteration| iteration.allocation == revision("repeat-copy"))
    );
    let copied_voice = appended
        .nodes()
        .iter()
        .find_map(|(key, node)| {
            (!original.nodes().contains_key(key) && matches!(node.kind, NodeKind::Source { .. }))
                .then_some(key)
        })
        .unwrap();
    let retained_run = appended.audio_bindings().bindings()[copied_voice]
        .lattice
        .births
        .iter()
        .find_map(|clause| {
            if let AudioBirthSurvivors::Run {
                allocation,
                first,
                count,
            } = &clause.survivors
            {
                Some((allocation, *first, *count))
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(retained_run.0, &revision("repeat-copy"));
    assert_eq!(
        retained_run.2, 4,
        "retired birth support must remain complete"
    );
    assert_eq!(
        (retained_run.1..retained_run.1 + retained_run.2)
            .filter(|ordinal| fresh_plays.contains(&IterationId {
                allocation: retained_run.0.clone(),
                ordinal: *ordinal
            }))
            .count(),
        3
    );
    assert_eq!(appended.overrides()[pasted_repeat].len(), 1);
    assert_eq!(appended.gap_overrides()[pasted_repeat].len(), 1);
    let copied_again = capture(&appended, "root", 25..39);
    let twice = paste(&appended, &copied_again, 0, "repeat-copy-again");
    cold_reverse(&twice, &mut provider, 0, &saved);
    // The 14-frame prepend changes rounding at individual destination joins.
    // Each old suffix owner still enters on its own captured sample, including
    // both surviving plays, definition births, default and overridden gaps.
    for old_frame in [0, 5, 7, 8, 10, 11, 13, 14, 16, 17, 19, 25] {
        let retained_entry = pcm(&appended, &mut provider, boundary(old_frame), 128);
        check_reads(
            &twice,
            &mut provider,
            boundary(old_frame + 14),
            &retained_entry,
        );
    }
    let second_repeat = twice
        .nodes()
        .iter()
        .find_map(|(key, node)| {
            (!appended.nodes().contains_key(key) && matches!(node.kind, NodeKind::Repeat { .. }))
                .then_some(key)
        })
        .unwrap();
    let NodeKind::Repeat { iterations, .. } = &twice.nodes()[second_repeat].kind else {
        unreachable!()
    };
    assert!(
        (0..iterations.len()).all(|index| !fresh_plays.contains(&iterations.at(index).unwrap()))
    );
    let independently_changed = edit(
        &twice,
        "silenced-first-repeat",
        Command::SetRepeat {
            node: pasted_repeat.clone(),
            plays: 5,
            gap: Some(gap(1, HoldAudio::Silence)),
        },
    );
    cold_reverse(&independently_changed, &mut provider, 0, &saved);
}

fn gain(trim: i32, envelopes: Vec<GainEnvelope>) -> AudioTreatments {
    AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(trim).unwrap(), false, envelopes, vec![]).unwrap(),
    )
}

#[test]
fn unrelated_repeats_with_equal_raw_play_ids_keep_distinct_historical_clocks() {
    let original = document(
        ntsc(),
        &["lead", "repeat", "other-repeat", "tail"],
        vec![
            ("lead", source(ntsc(), 5)),
            ("repeat", repeat("voice", 2)),
            ("voice", source(ntsc(), 2)),
            ("other-repeat", repeat("other-voice", 2)),
            (
                "other-voice",
                BeatNode::hold("Other room", room(3, 700..921)),
            ),
            ("tail", source(ntsc(), 5)),
        ],
    );
    let paused = insert_pause(&original, 5, 1, "two-repeat-pause");
    let retained = cut(&paused, 1..2, "two-repeat-cut");
    let slice = capture(&retained, "root", 5..15);
    let NodeKind::Sequence { children } = &retained.nodes()[retained.root()].kind else {
        unreachable!()
    };
    let copied = paste(&retained, &slice, children.len(), "two-repeats-copied");
    let mut provider = Provider::new();
    let saved = pcm(&retained, &mut provider, 8008, 16016);
    let source_entry = expected(&provider, ExactRatio::ZERO, 128);
    assert_close(&saved[..128], &source_entry);
    let room = room_reference(&provider, 700..921, 4805);
    // The independent second Repeat originally starts at frame9: its entry
    // is B(9)-9*1601.6=-2/5, despite identical raw play allocation/ordinal IDs.
    assert_close(
        &saved[6406..6534],
        &sample_reference(&room, ratio(-2, 5), ExactRatio::ONE, 128),
    );
    cold_reverse(&copied, &mut provider, 32032, &saved);
}

fn authored(
    document: &ProjectDocument,
    provider: &mut Provider,
    start: i64,
    count: u32,
) -> Vec<[f32; 2]> {
    let mut reader = renderer(document, provider);
    reader
        .prepare_authored_bus(
            provider,
            AudioSample(start),
            count,
            TIMEOUT,
            &AtomicBool::new(false),
        )
        .unwrap()
        .samples
}

#[test]
fn contents_exclude_unselected_parent_gain_while_owned_group_retains_it() {
    let envelope = GainEnvelope::new(
        GainClock::OwnerOutput,
        GainRange::new(ExactRatio::ZERO, ExactRatio::integer(7)).unwrap(),
        GainDb::new(-6000).unwrap(),
        vec![
            GainSegment::new(
                ExactRatio::integer(7),
                GainDb::new(6000).unwrap(),
                GainCurve::Linear,
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let mut voice = source(ntsc(), 7);
    voice.audio_treatments = gain(-3000, vec![envelope]);
    let mut group = BeatNode::sequence("Treated group", vec![id("voice")]);
    group.audio_treatments = gain(6000, vec![]);
    let original = document(
        ntsc(),
        &["lead", "group"],
        vec![("lead", silence(1)), ("group", group), ("voice", voice)],
    );
    let contents = capture(&original, "group", 2..4);
    let owned_group = capture(&original, "root", 1..8);
    let content_paste = paste(&destination(7, 5), &contents, 1, "content-gain-copy");
    let group_paste = paste(&destination(6, 5), &owned_group, 1, "group-gain-copy");
    let mut provider = Provider::new();
    // Destination shifts by five frames, exactly 8008 samples, preserving
    // nominal owner progress as well as raw sampling phase. Interior windows
    // avoid asserting anything about automatic fades at the new slice edges.
    let at = boundary(2) + 512;
    let old = authored(&original, &mut provider, at, 128);
    let content = authored(&content_paste, &mut provider, at + 8008, 128);
    let whole_group = authored(&group_paste, &mut provider, at + 8008, 128);
    assert_eq!(whole_group, old);
    let raw = pcm(&content_paste, &mut provider, at + 8008, 128);
    let oracle: Vec<_> = raw
        .iter()
        .enumerate()
        .map(|(index, sample)| {
            // Original voice starts at exact sample1601.6; gain is evaluated in
            // its seven-frame owner clock with documented Q32 progress rounding.
            let local = ratio(i128::from(at + i64::try_from(index).unwrap()) * 5 - 8008, 5);
            let progress = local
                .checked_div(ratio(56056, 5))
                .unwrap()
                .checked_mul(ExactRatio::integer(
                    i64::try_from(GAIN_NUMERIC_SCALE).unwrap(),
                ))
                .unwrap()
                .round_even()
                .unwrap();
            let millidb = -9000. + 12000. * progress as f64 / GAIN_NUMERIC_SCALE as f64;
            sample.map(|value| (f64::from(value) * 10_f64.powf(millidb / 20000.)) as f32)
        })
        .collect();
    assert_close(&content, &oracle);
    let parent_factor = 10_f64.powf(6000. / 20000.);
    let with_parent: Vec<_> = content
        .iter()
        .map(|sample| sample.map(|value| (f64::from(value) * parent_factor) as f32))
        .collect();
    assert_close(&old, &with_parent);
    assert_ne!(content, old);
}
