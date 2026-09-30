use std::ops::Range;

use deadpan_core::*;
use deadpan_plan::*;
use serde_json::json;

fn id() -> SoundId {
    SoundId::new("sound").unwrap()
}
fn node(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn duration(frames: i64) -> FrameDuration {
    FrameDuration::new(frames).unwrap()
}
fn insert(at: i64, frames: i64, rate: FrameRate) -> RootSoundEdit {
    RootSoundEdit {
        grid: RootSoundGrid::root(rate),
        operation: RootSoundOperation::Insert {
            at: ProjectFrame(at),
            duration: duration(frames),
        },
        cuts: Default::default(),
    }
}
fn delete(start: i64, end: i64, rate: FrameRate) -> RootSoundEdit {
    RootSoundEdit {
        grid: RootSoundGrid::root(rate),
        operation: RootSoundOperation::Delete {
            range: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
        },
        cuts: Default::default(),
    }
}
fn document(
    parts: &[(i64, bool)],
    rate: FrameRate,
    recipe_extent: i64,
    edits: Option<Vec<RootSoundEdit>>,
    selection: Option<Range<ExactRatio>>,
) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("routed-sound").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: rate,
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(empty).unwrap();
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: 1_000_000,
            time_base,
        },
    )
    .unwrap();
    wire["assets"] = json!({ "sound": AssetRecord { label: "sound".into(), content_hash: "a".repeat(64), audio: Some(span), video: None, frame_count: None, still_image: false, source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()) } });
    wire["assets"]["picture"] = json!(AssetRecord {
        label: "picture".into(),
        content_hash: "c".repeat(64),
        audio: None,
        video: None,
        frame_count: None,
        still_image: true,
        source_qualification: None
    });
    let mut children = Vec::new();
    for (index, &(frames, held)) in parts.iter().enumerate() {
        let name = format!("part{index}");
        children.push(node(&name));
        let kind = if held {
            NodeKind::Hold {
                recipe: HoldRecipe {
                    duration: duration(frames),
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                    picture_context: None,
                },
            }
        } else {
            NodeKind::Source {
                source: SourceNode {
                    duration: duration(frames),
                    video: SourceVideo::Still {
                        asset: AssetId::new("picture").unwrap(),
                    },
                    audio: None,
                    audio_mapping: SourceAudioMapping::FitBeat,
                    video_mapping: SourceVideoMapping::FitBeat,
                    audio_offset: AudioSample(0),
                    link: LinkRelation::Independent,
                },
            }
        };
        wire["nodes"][&name] = json!(BeatNode {
            label: name.clone(),
            kind,
            framing: None,
            audio_treatments: Default::default(),
            audio_edges: Default::default()
        });
    }
    wire["nodes"]["root"] = json!(BeatNode::sequence("root", children));
    let selection = selection.unwrap_or(ExactRatio::ZERO..ExactRatio::integer(recipe_extent));
    wire["sounds"] = json!({"sound": SoundEvent {
        owner: node("root"), label: "event".into(), source: SourceAudio { asset: AssetId::new("sound").unwrap(), span },
        mapping: SourceAudioMapping::SelectedPlacement { start: ExactRatio::ZERO, frames: SourceAudioMapping::natural_rate(span, rate).unwrap().duration_frames(FrameDuration::ZERO).unwrap(), selection: ExactFrameRange { start: selection.start, end: selection.end } },
        offset: AudioSample(0), gain_millidecibels: 0, start_edge: AudioEdgePolicy::Automatic, end_edge: AudioEdgePolicy::Automatic, overflow: SoundOverflowPolicy::Reject,
    }});
    if let Some(edits) = edits {
        wire["sound_routes"] = json!({"sound": RootSoundRoute { recipe_extent: duration(recipe_extent), recipe_grid: RootSoundGrid::root(rate), edits }});
    }
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}
fn gates(plan: &RenderPlan, range: Range<i64>) -> AudioSoundGateQuery {
    plan.root_sound(&id())
        .unwrap()
        .gate_fades(
            AudioSample(range.start)..AudioSample(range.end),
            AudioQueryLimits::default(),
        )
        .unwrap()
}

#[test]
fn selected_range_finds_subframe_support_without_filling_routed_gaps() {
    let rate = FrameRate::new(24, 1).unwrap();
    let selection = Some(ExactRatio::new(1, 4).unwrap()..ExactRatio::new(1, 2).unwrap());
    for routed in [false, true] {
        let doc = document(
            &[(if routed { 3 } else { 2 }, true)],
            rate,
            2,
            routed.then(|| vec![insert(0, 1, rate)]),
            selection.clone(),
        );
        let plan = RenderPlan::compile(&doc).unwrap();
        let sound = plan.root_sound(&id()).unwrap();
        let shift = if routed { 2000 } else { 0 };
        assert!(!sound.selects_sample(AudioSample(shift)).unwrap());
        assert!(
            sound
                .selects_range(AudioSample(shift)..AudioSample(shift + 2000))
                .unwrap()
        );
        assert!(
            !sound
                .selects_range(AudioSample(shift)..AudioSample(shift + 500))
                .unwrap()
        );
        assert!(
            sound
                .selects_range(AudioSample(shift + 499)..AudioSample(shift + 501))
                .unwrap()
        );
        assert!(
            !sound
                .selects_range(AudioSample(shift + 1000)..AudioSample(shift + 2000))
                .unwrap()
        );
        assert!(
            !sound
                .selects_range(AudioSample(shift + 750)..AudioSample(shift + 750))
                .unwrap()
        );
        assert!(
            sound
                .selects_range(AudioSample(-1)..AudioSample(0))
                .is_err()
        );
        assert!(sound.selects_range(AudioSample(2)..AudioSample(1)).is_err());
        let end = plan.audio_duration().unwrap();
        assert!(!sound.selects_range(end..end).unwrap());
        assert!(sound.selects_range(end..AudioSample(end.0 + 1)).is_err());
        if routed {
            assert!(
                !sound
                    .selects_range(AudioSample(0)..AudioSample(2000))
                    .unwrap()
            );
        }
    }
    for route in [None, Some(vec![])] {
        let sampleless = document(
            &[(2, true)],
            rate,
            2,
            route,
            Some(ExactRatio::new(2501, 10_000).unwrap()..ExactRatio::new(2502, 10_000).unwrap()),
        );
        let plan = RenderPlan::compile(&sampleless).unwrap();
        let sound = plan.root_sound(&id()).unwrap();
        assert_eq!(sound.audible_samples(), AudioSample(500)..AudioSample(500));
        assert!(
            !sound
                .selects_range(AudioSample(0)..AudioSample(2000))
                .unwrap()
        );
    }
}

#[test]
fn allowing_current_holds_never_restores_samples_deleted_from_the_route() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let doc = document(
        &[(100, true), (20, true), (100, true)],
        rate,
        200,
        Some(vec![insert(100, 20, rate)]),
        None,
    );
    let mut wire = serde_json::to_value(doc).unwrap();
    wire["sound_allowances"] = json!({"sound": SoundHoldAllowances::try_from((0..3).map(|index| SoundHoldIssuer::Node {
        instance: InstancePath { node: node(&format!("part{index}")), repeats: vec![] }
    }).collect::<Vec<_>>()).unwrap()});
    let plan =
        RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap();
    let sound = plan.root_sound(&id()).unwrap();
    for at in 0..220 {
        assert_eq!(
            sound.selects_sample(AudioSample(at)).unwrap(),
            !(100..120).contains(&at)
        );
    }
    let full = gates(&plan, 0..220);
    let gains = factors(&full);
    assert!(gains[..100].iter().all(|gain| *gain != ExactRatio::ZERO));
    assert!(gains[100..120].iter().all(|gain| *gain == ExactRatio::ZERO));
    assert!(gains[120..].iter().all(|gain| *gain != ExactRatio::ZERO));
    for range in [98..122, 99..101, 119..121, 170..190] {
        assert_eq!(
            factors(&gates(&plan, range.clone())),
            gains[range.start as usize..range.end as usize]
        );
    }
}
fn factors(query: &AudioSoundGateQuery) -> Vec<ExactRatio> {
    query
        .spans
        .iter()
        .flat_map(|span| {
            (0..span.samples.end.0 - span.samples.start.0).map(move |offset| {
                if span.length == 0 {
                    return ExactRatio::ZERO;
                }
                let width = span.length.min(192);
                let at = span.progress_at_start + u64::try_from(offset).unwrap();
                let edge = |distance: u64| (2 * distance.min(96) + 1).min(width);
                let left = if span.start_edge == AudioEdgePolicy::Hard {
                    width
                } else {
                    edge(at)
                };
                let right = if span.end_edge == AudioEdgePolicy::Hard {
                    width
                } else {
                    edge(span.length - 1 - at)
                };
                ExactRatio::new(i128::from(left.min(right)), i128::from(width)).unwrap()
            })
        })
        .collect()
}

#[test]
fn equal_ntsc_replacement_retains_last_suffix_sample_without_intermediate_clipping() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let replacement = RootSoundEdit {
        grid: RootSoundGrid::root(rate),
        operation: RootSoundOperation::Replace {
            range: FrameRange::new(ProjectFrame(1), ProjectFrame(2)).unwrap(),
            duration: duration(1),
        },
        cuts: Default::default(),
    };
    let direct = RenderPlan::compile(&document(
        &[(3, false)],
        rate,
        3,
        Some(vec![replacement]),
        None,
    ))
    .unwrap();
    let sound = direct.root_sound(&id()).unwrap();
    for sample in 0..4805 {
        assert_eq!(
            sound.selects_sample(AudioSample(sample)).unwrap(),
            !(1602..3203).contains(&sample)
        );
    }
    let sampled = sound
        .routed_input()
        .unwrap()
        .route()
        .query(
            AudioSample(4804)..AudioSample(4805),
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(
        sampled.spans[0]
            .sampling
            .unwrap()
            .local_at(AudioSample(4804))
            .unwrap()
            .checked_div(
                sound
                    .routed_input()
                    .unwrap()
                    .route()
                    .recipe_grid()
                    .frames_per_sample()
            )
            .unwrap(),
        ExactRatio::integer(4804)
    );
    let all = factors(&gates(&direct, 0..4805));
    assert!(all[4804].compare_integer(0).is_gt());
    assert!(all[1601].compare_integer(1).is_lt());
    assert!(all[3203].compare_integer(1).is_lt());
    assert!(
        all[1602..3203]
            .iter()
            .all(|factor| *factor == ExactRatio::ZERO)
    );
    // The forbidden Delete+Insert composition loses sample 4804: deletion
    // clips its translated label at the shorter intermediate endpoint 3203.
    let intermediate = RenderPlan::compile(&document(
        &[(3, false)],
        rate,
        3,
        Some(vec![delete(1, 2, rate), insert(1, 1, rate)]),
        None,
    ))
    .unwrap();
    assert!(
        !intermediate
            .root_sound(&id())
            .unwrap()
            .selects_sample(AudioSample(4804))
            .unwrap()
    );
}

#[test]
fn replacement_uses_final_boundaries_for_shorter_and_longer_sound_islands() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    for inserted in [1, 3, 5] {
        let edit = RootSoundEdit {
            grid: RootSoundGrid::root(rate),
            operation: RootSoundOperation::Replace {
                range: FrameRange::new(ProjectFrame(1), ProjectFrame(4)).unwrap(),
                duration: duration(inserted),
            },
            cuts: RootSoundCutEdges {
                before: AudioEdgePolicy::Hard,
                after: AudioEdgePolicy::Hard,
            },
        };
        let plan = RenderPlan::compile(&document(
            &[(3 + inserted, false)],
            rate,
            6,
            Some(vec![edit]),
            None,
        ))
        .unwrap();
        let sound = plan.root_sound(&id()).unwrap();
        let old = rate.audio_boundary(ProjectFrame(4)).unwrap();
        let start = rate.audio_boundary(ProjectFrame(1 + inserted)).unwrap();
        let sampled = sound
            .routed_input()
            .unwrap()
            .route()
            .query(start..AudioSample(start.0 + 1), AudioQueryLimits::default())
            .unwrap();
        assert_eq!(
            sampled.spans[0]
                .sampling
                .unwrap()
                .local_at(start)
                .unwrap()
                .checked_div(
                    sound
                        .routed_input()
                        .unwrap()
                        .route()
                        .recipe_grid()
                        .frames_per_sample()
                )
                .unwrap(),
            ExactRatio::integer(old.0)
        );
        assert_eq!(
            factors(&gates(&plan, start.0..start.0 + 1)),
            vec![ExactRatio::ONE]
        );
    }
}

#[test]
fn identity_route_matches_unrouted_tiny_hold_islands_and_shuffled_queries() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let parts = [
        (10, false),
        (10, true),
        (10, false),
        (10, true),
        (960, false),
    ];
    let direct = RenderPlan::compile(&document(&parts, rate, 1000, None, None)).unwrap();
    let routed = RenderPlan::compile(&document(&parts, rate, 1000, Some(vec![]), None)).unwrap();
    let expected = factors(&gates(&direct, 0..1000));
    assert_eq!(factors(&gates(&routed, 0..1000)), expected);
    for range in [20..30, 500..800, 0..1, 27..42, 999..1000, 0..1000] {
        assert_eq!(
            factors(&gates(&routed, range.clone())),
            expected[range.start as usize..range.end as usize]
        );
    }
}

#[test]
fn deleting_onset_cuts_envelope_but_keeps_complete_recipe_and_source_phase() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let doc = document(
        &[(20, false)],
        rate,
        60,
        Some(vec![delete(0, 40, rate)]),
        None,
    );
    let mut wire = serde_json::to_value(doc).unwrap();
    wire["nodes"]["root"]["audio_edges"] = json!(AudioEdgePolicies {
        node_start: AudioEdgePolicy::Hard,
        ..Default::default()
    });
    let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    let sound = plan.root_sound(&id()).unwrap();
    let route = sound.routed_input().unwrap();
    let AudioRoutedRootInput::Source(raw) = route.input() else {
        panic!("raw input")
    };
    assert_eq!(raw.extent(), ExactRatio::integer(60));
    let routed = route
        .route()
        .query(AudioSample(0)..AudioSample(1), AudioQueryLimits::default())
        .unwrap();
    assert_eq!(
        routed.spans[0]
            .sampling
            .unwrap()
            .local_at(AudioSample(0))
            .unwrap(),
        ExactRatio::integer(40)
    );
    let source = raw
        .query(
            AudioSample(40)..AudioSample(41),
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(
        source.spans[0].source_point(AudioSample(40)).unwrap().ticks,
        ExactRatio::integer(40)
    );
    assert_eq!(
        source.spans[0].allocated_samples,
        AudioSample(0)..AudioSample(60)
    );
    let envelope = gates(&plan, 0..20);
    assert_eq!(envelope.spans[0].length, 20);
    assert_eq!(envelope.spans[0].progress_at_start, 0);
    assert_eq!(envelope.spans[0].start_edge, AudioEdgePolicy::Automatic);
}

#[test]
fn two_ntsc_insertions_preserve_each_old_physical_clock() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let plan = RenderPlan::compile(&document(
        &[(12, false)],
        rate,
        10,
        Some(vec![insert(2, 1, rate), insert(4, 1, rate)]),
        None,
    ))
    .unwrap();
    let sound = plan.root_sound(&id()).unwrap();
    let route = sound.routed_input().unwrap();
    let query = route
        .route()
        .query(
            AudioSample(8008)..AudioSample(8009),
            AudioQueryLimits::default(),
        )
        .unwrap();
    let recipe_frame = query.spans[0]
        .sampling
        .unwrap()
        .local_at(AudioSample(8008))
        .unwrap();
    let old_label = recipe_frame
        .checked_div(route.route().recipe_grid().frames_per_sample())
        .unwrap();
    assert_eq!(old_label, ExactRatio::integer(4804));
    assert_ne!(
        old_label,
        ExactRatio::integer(4805),
        "flattening the two frame edits loses a physical sample"
    );
    let AudioRoutedRootInput::Source(raw) = route.input() else {
        panic!("raw input")
    };
    assert_eq!(
        raw.query(
            AudioSample(4804)..AudioSample(4805),
            AudioQueryLimits::default()
        )
        .unwrap()
        .spans[0]
            .source_point(AudioSample(4804))
            .unwrap()
            .ticks,
        ExactRatio::integer(4804)
    );
}

#[test]
fn forty_chronological_cuts_are_not_a_nested_map_depth_limit() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let plan = RenderPlan::compile(&document(
        &[(50, false)],
        rate,
        10,
        Some((0..40).map(|_| insert(0, 1, rate)).collect()),
        None,
    ))
    .unwrap();
    let sound = plan.root_sound(&id()).unwrap();
    let route = sound.routed_input().unwrap();
    let query = route
        .route()
        .query(
            AudioSample(64080)..AudioSample(64081),
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(
        query.spans[0]
            .sampling
            .unwrap()
            .local_at(AudioSample(64080))
            .unwrap(),
        ExactRatio::ZERO
    );
    assert_eq!(gates(&plan, 64080..64081).spans[0].progress_at_start, 0);
    assert!(
        sound
            .gate_fades(
                AudioSample(64080)..AudioSample(64081),
                AudioQueryLimits {
                    maximum_spans: 1,
                    maximum_work: 1
                }
            )
            .is_err()
    );
}

#[test]
fn current_hold_owns_exact_coincident_boundary_after_odd_sample_translation() {
    // At 32kfps each frame is 1.5 samples. Insertion transports original
    // onset frame1 / label2 to exact frame2 / label4. The current Hold ends at
    // that exact frame2, but its own physical boundary is label3.
    let rate = FrameRate::new(32_000, 1).unwrap();
    let selection = ExactRatio::integer(1)..ExactRatio::integer(3);
    let doc = document(
        &[(1, false), (1, true), (2, false)],
        rate,
        3,
        Some(vec![insert(0, 1, rate)]),
        Some(selection),
    );
    let mut wire = serde_json::to_value(doc).unwrap();
    wire["nodes"]["part1"]["audio_edges"] = json!(AudioEdgePolicies {
        node_end: AudioEdgePolicy::Hard,
        ..Default::default()
    });
    let doc = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&doc).unwrap();
    let query = gates(&plan, 4..5);
    assert_eq!(query.spans[0].length, 3);
    assert_eq!(
        query.spans[0].progress_at_start, 1,
        "Hold label3 owns the coincident envelope onset; source sample4 retains its progress"
    );
    assert_eq!(query.spans[0].start_edge, AudioEdgePolicy::Hard);
    assert_eq!(factors(&gates(&plan, 2..4)), vec![ExactRatio::ZERO; 2]);
}

#[test]
fn hundreds_of_fragmented_cuts_have_indexed_cold_envelope_queries() {
    let rate = FrameRate::new(48_000, 1).unwrap();
    let edits = (0..600)
        .map(|index| insert(index * 11 + 5, 1, rate))
        .collect();
    let plan = RenderPlan::compile(&document(
        &[(10_600, false)],
        rate,
        10_000,
        Some(edits),
        None,
    ))
    .unwrap();
    let sound = plan.root_sound(&id()).unwrap();
    let query = sound
        .gate_fades(
            AudioSample(6596)..AudioSample(6597),
            AudioQueryLimits {
                maximum_spans: 4,
                maximum_work: 128,
            },
        )
        .unwrap();
    assert!(query.work <= 128);
    assert_eq!(query.spans[0].progress_at_start, 1);
    assert_eq!(query.spans[0].length, 4005);
    let source = sound
        .routed_input()
        .unwrap()
        .route()
        .query(
            AudioSample(6596)..AudioSample(6597),
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert_eq!(
        source.spans[0]
            .sampling
            .unwrap()
            .local_at(AudioSample(6596))
            .unwrap(),
        ExactRatio::integer(5996)
    );
}

#[test]
fn current_hold_masks_physical_drift_even_when_semantic_onset_is_later() {
    let rate = FrameRate::new(32_000, 1).unwrap();
    let doc = document(
        &[(1, false), (4, true), (9, false)],
        rate,
        10,
        Some((0..4).map(|_| insert(1, 1, rate)).collect()),
        Some(ExactRatio::integer(2)..ExactRatio::integer(10)),
    );
    let mut wire = serde_json::to_value(doc).unwrap();
    wire["sounds"]["sound"]["start_edge"] = json!("hard");
    let plan =
        RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap();
    let sound = plan.root_sound(&id()).unwrap();
    // Four suffix shifts B(2)-B(1)=1 move old label3 to label7. Its semantic
    // onset frame6 is after Hold end frame5, whose current physical end is8.
    let retained = sound
        .routed_input()
        .unwrap()
        .route()
        .query(AudioSample(7)..AudioSample(8), AudioQueryLimits::default())
        .unwrap();
    assert!(retained.spans[0].sampling.is_some());
    assert_eq!(factors(&gates(&plan, 7..8)), [ExactRatio::ZERO]);
    assert_eq!(
        factors(&gates(&plan, 8..9)),
        [ExactRatio::new(1, 11).unwrap()],
        "the actual Hold cut is Automatic; the semantically distinct retained Hard onset cannot override it"
    );
}

#[test]
fn extreme_destination_clips_support_before_narrowing_retained_labels() {
    let rate = FrameRate::new(240_000, 7).unwrap();
    let output_frames = 6_588_122_883_467_697_005_i64;
    let recipe_frames = output_frames - 1;
    let mut wire = serde_json::to_value(document(&[(100, false)], rate, 100, None, None)).unwrap();
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base,
        },
        SourceTimestamp {
            ticks: i64::MAX,
            time_base,
        },
    )
    .unwrap();
    let mut asset: AssetRecord = serde_json::from_value(wire["assets"]["sound"].clone()).unwrap();
    asset.audio = Some(span);
    wire["assets"]["sound"] = json!(asset);
    let mut event: SoundEvent = serde_json::from_value(wire["sounds"]["sound"].clone()).unwrap();
    event.source.span = span;
    event.mapping = SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::ZERO,
        frames: SourceAudioMapping::natural_rate(span, rate)
            .unwrap()
            .duration_frames(FrameDuration::ZERO)
            .unwrap(),
        selection: ExactFrameRange {
            start: ExactRatio::ZERO,
            end: ExactRatio::integer(recipe_frames),
        },
    };
    wire["sounds"]["sound"] = json!(event);
    let mut part: BeatNode = serde_json::from_value(wire["nodes"]["part0"].clone()).unwrap();
    let NodeKind::Source { source } = &mut part.kind else {
        panic!("source")
    };
    source.duration = duration(output_frames);
    wire["nodes"]["part0"] = json!(part);
    wire["sound_routes"] = json!({"sound": RootSoundRoute {
        recipe_extent: duration(recipe_frames), recipe_grid: RootSoundGrid::root(rate), edits: vec![insert(1, 1, rate)],
    }});
    let plan =
        RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap();
    assert_eq!(plan.audio_duration().unwrap(), AudioSample(i64::MAX));
    // Old endpoint MAX-1 shifts by2. Its virtual envelope end is MAX+1,
    // while the valid current allocation clips at MAX before conversion.
    let query = gates(&plan, i64::MAX - 2..i64::MAX);
    assert_eq!(query.spans.len(), 1);
    assert_eq!(
        query.spans[0].samples,
        AudioSample(i64::MAX - 2)..AudioSample(i64::MAX)
    );
    assert_eq!(query.spans[0].length - query.spans[0].progress_at_start, 3);
    let sound = plan.root_sound(&id()).unwrap();
    let route = sound.routed_input().unwrap().route();
    let retained = route
        .query(
            AudioSample(i64::MAX - 2)..AudioSample(i64::MAX),
            AudioQueryLimits::default(),
        )
        .unwrap();
    assert!(retained.spans.iter().all(|span| span.sampling.is_some()));
}
