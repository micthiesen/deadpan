use std::ops::Range;

use deadpan_core::*;
use deadpan_plan::*;
use serde_json::json;

fn node(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}
fn sound() -> SoundId {
    SoundId::new("sound").unwrap()
}

fn document(parts: &[(i64, bool)], rate: u32) -> ProjectDocument {
    let empty = ProjectDocument::new(
        ProjectId::new("sound-gate").unwrap(),
        RevisionId::new("initial").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(rate, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        node("root"),
    )
    .unwrap();
    let mut wire = serde_json::to_value(empty).unwrap();
    let span = SourceSpan::new(
        SourceTimestamp {
            ticks: 0,
            time_base: SourceTimeBase::new(1, 48_000).unwrap(),
        },
        SourceTimestamp {
            ticks: 100_000,
            time_base: SourceTimeBase::new(1, 48_000).unwrap(),
        },
    )
    .unwrap();
    wire["assets"] = json!({
        "sound": AssetRecord { label: "sound".into(), content_hash: "a".repeat(64), audio: Some(span), video: None, frame_count: None, still_image: false, source_qualification: Some(SourceQualificationId::new("b".repeat(64)).unwrap()) },
        "picture": AssetRecord { label: "picture".into(), content_hash: "c".repeat(64), audio: None, video: None, frame_count: None, still_image: true, source_qualification: None },
    });
    let mut children = Vec::new();
    for (index, &(frames, hold)) in parts.iter().enumerate() {
        let name = format!("part{index}");
        children.push(node(&name));
        let duration = FrameDuration::new(frames).unwrap();
        let kind = if hold {
            NodeKind::Hold {
                recipe: HoldRecipe {
                    duration,
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                    picture_context: None,
                },
            }
        } else {
            NodeKind::Source {
                source: SourceNode {
                    duration,
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
    let total: i64 = parts.iter().map(|part| part.0).sum();
    wire["sounds"] = json!({"sound": SoundEvent {
        owner: node("root"), label: "event".into(), source: SourceAudio { asset: AssetId::new("sound").unwrap(), span },
        mapping: SourceAudioMapping::SelectedPlacement { start: ExactRatio::ZERO, frames: SourceAudioMapping::natural_rate(span, FrameRate::new(rate,1).unwrap()).unwrap().duration_frames(FrameDuration::ZERO).unwrap(), selection: ExactFrameRange { start: ExactRatio::ZERO, end: ExactRatio::integer(total) } },
        offset: AudioSample(0), gain_millidecibels: 0, start_edge: AudioEdgePolicy::Automatic, end_edge: AudioEdgePolicy::Automatic, overflow: SoundOverflowPolicy::Reject,
    }});
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

fn query(plan: &RenderPlan, range: Range<i64>) -> AudioSoundGateQuery {
    plan.root_sound(&sound())
        .unwrap()
        .gate_fades(
            AudioSample(range.start)..AudioSample(range.end),
            AudioQueryLimits::default(),
        )
        .unwrap()
}

#[test]
fn custom_allowances_select_one_contribution_and_exact_repeat_issuers() {
    let mut wire = serde_json::to_value(document(&[(100, true)], 48_000)).unwrap();
    let allocation = RevisionId::new("repeat-plays").unwrap();
    let plays = IterationOrder::new(allocation.clone(), 3).unwrap();
    let play = |ordinal| IterationId {
        allocation: allocation.clone(),
        ordinal,
    };
    wire["nodes"]["root"] = json!(BeatNode::sequence("root", vec![node("repeat")]));
    wire["nodes"]["repeat"] = json!(BeatNode {
        label: "Three silent plays".into(),
        framing: None,
        audio_treatments: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Repeat {
            child: node("part0"),
            iterations: plays,
            gap: Some(HoldRecipe {
                duration: FrameDuration::new(50).unwrap(),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
                picture_context: None
            }),
        },
    });
    wire["sounds"]["sound"]["mapping"]["selection"]["end"] = json!(ExactRatio::integer(400));
    wire["sounds"]["other"] = wire["sounds"]["sound"].clone();
    let gap = SoundHoldIssuer::RepeatGap {
        instance: InstancePath {
            node: node("repeat"),
            repeats: vec![],
        },
        gap_after: play(0),
    };
    let held_play = SoundHoldIssuer::Node {
        instance: InstancePath {
            node: node("part0"),
            repeats: vec![RepeatInstance {
                node: node("repeat"),
                iteration: play(1),
            }],
        },
    };
    wire["sound_allowances"] =
        json!({"sound": SoundHoldAllowances::try_from(vec![gap, held_play]).unwrap()});
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let plan = RenderPlan::compile(&document).unwrap();
    let whole = query(&plan, 0..400);
    assert_eq!(
        whole
            .spans
            .iter()
            .map(|s| (s.samples.clone(), s.length))
            .collect::<Vec<_>>(),
        vec![
            (AudioSample(0)..AudioSample(100), 0),
            (AudioSample(100)..AudioSample(250), 150),
            (AudioSample(250)..AudioSample(400), 0),
        ]
    );
    let gains = factors(&whole);
    for range in [0..100, 99..101, 148..152, 174..176, 249..251, 399..400] {
        assert_eq!(
            factors(&query(&plan, range.clone())),
            gains[range.start as usize..range.end as usize]
        );
    }
    let other = plan.root_sound(&SoundId::new("other").unwrap()).unwrap();
    assert!(
        other
            .gate_fades(AudioSample(0)..AudioSample(400), Default::default())
            .unwrap()
            .spans
            .iter()
            .all(|s| s.length == 0)
    );
    let original = plan
        .audio_hold_policy(AudioSample(0)..AudioSample(400), Default::default())
        .unwrap();
    assert_eq!(
        original.rules.len(),
        5,
        "custom contribution policy cannot remove Original Hold rules"
    );
    assert!(
        original
            .rules
            .iter()
            .all(|rule| rule.issuer.sound_issuer().is_some())
    );
    let voice = plan.root_sound(&sound()).unwrap();
    assert!(
        voice.selects_sample(AudioSample(0)).unwrap(),
        "selection is distinct from current suppression"
    );
    assert!(voice.selects_sample(AudioSample(399)).unwrap());
    assert!(voice.selects_sample(AudioSample(400)).is_err());
    assert!(voice.selects_sample(AudioSample(-1)).is_err());
    assert!(
        voice
            .gate_fades(
                AudioSample(100)..AudioSample(250),
                AudioQueryLimits {
                    maximum_spans: 16,
                    maximum_work: 1
                }
            )
            .is_err()
    );
}

// Exact sample-centered envelope factors avoid testing a duplicate f32 mixer.
fn factors(query: &AudioSoundGateQuery) -> Vec<ExactRatio> {
    let mut result = Vec::new();
    for span in &query.spans {
        for offset in 0..u64::try_from(span.samples.end.0 - span.samples.start.0).unwrap() {
            if span.length == 0 {
                result.push(ExactRatio::ZERO);
                continue;
            }
            let progress = span.progress_at_start + offset;
            let width = span.length.min(192);
            let edge = |distance: u64| (2 * distance.min(96) + 1).min(width);
            let left = if span.start_edge == AudioEdgePolicy::Hard {
                width
            } else {
                edge(progress)
            };
            let right = if span.end_edge == AudioEdgePolicy::Hard {
                width
            } else {
                edge(span.length - 1 - progress)
            };
            result.push(ExactRatio::new(i128::from(left.min(right)), i128::from(width)).unwrap());
        }
    }
    result
}

#[test]
fn holds_create_independent_edges_but_ordinary_cuts_do_not() {
    let plan = RenderPlan::compile(&document(
        &[(500, false), (500, false), (100, true), (1000, false)],
        48_000,
    ))
    .unwrap();
    let whole = query(&plan, 0..2100);
    assert_eq!(
        whole
            .spans
            .iter()
            .map(|span| (span.samples.clone(), span.length))
            .collect::<Vec<_>>(),
        [
            (AudioSample(0)..AudioSample(1000), 1000),
            (AudioSample(1000)..AudioSample(1100), 0),
            (AudioSample(1100)..AudioSample(2100), 1000),
        ]
    );
    let gains = factors(&whole);
    assert_eq!(gains[499], ExactRatio::ONE);
    assert_eq!(gains[500], ExactRatio::ONE);
    assert_eq!(gains[999], ExactRatio::new(1, 192).unwrap());
    assert_eq!(gains[1100], ExactRatio::new(1, 192).unwrap());
    for range in [998..1103, 499..501, 1100..1101, 2000..2100, 0..1] {
        assert_eq!(
            factors(&query(&plan, range.clone())),
            gains[range.start as usize..range.end as usize]
        );
    }
}

#[test]
fn short_surviving_islands_keep_one_envelope_across_query_and_source_cuts() {
    let plan = RenderPlan::compile(&document(
        &[
            (100, true),
            (50, false),
            (50, false),
            (50, false),
            (100, true),
            (1, false),
            (100, true),
            (3, false),
        ],
        48_000,
    ))
    .unwrap();
    let gains = factors(&query(&plan, 0..454));
    assert_eq!(gains[100], ExactRatio::new(1, 150).unwrap());
    assert_eq!(gains[174], ExactRatio::new(149, 150).unwrap());
    assert_eq!(gains[249], ExactRatio::new(1, 150).unwrap());
    assert_eq!(gains[350], ExactRatio::ONE);
    assert_eq!(
        &gains[451..454],
        &[
            ExactRatio::new(1, 3).unwrap(),
            ExactRatio::ONE,
            ExactRatio::new(1, 3).unwrap()
        ]
    );
    for at in [100, 149, 150, 174, 175, 199, 200, 249, 350, 451, 452, 453] {
        assert_eq!(
            factors(&query(&plan, at..at + 1)),
            gains[at as usize..at as usize + 1]
        );
    }
}

#[test]
fn hard_hold_edges_and_exact_event_coincidence_are_preserved() {
    let doc = document(&[(100, false), (100, true), (100, false)], 48_000);
    let mut wire = serde_json::to_value(doc).unwrap();
    wire["nodes"]["part1"]["audio_edges"] = json!(AudioEdgePolicies {
        node_start: AudioEdgePolicy::Hard,
        node_end: AudioEdgePolicy::Hard,
        ..Default::default()
    });
    let plan =
        RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap();
    let spans = query(&plan, 0..300).spans;
    assert_eq!(spans[0].end_edge, AudioEdgePolicy::Hard);
    assert_eq!(spans[2].start_edge, AudioEdgePolicy::Hard);
    // An event beginning exactly at the gate end can explicitly retain Hard.
    wire["nodes"]["part1"]
        .as_object_mut()
        .unwrap()
        .remove("audio_edges");
    wire["sounds"]["sound"]["start_edge"] = json!("hard");
    wire["sounds"]["sound"]["mapping"]["selection"]["start"] = json!(ExactRatio::integer(200));
    let plan =
        RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap();
    assert_eq!(
        query(&plan, 200..201).spans[0].start_edge,
        AudioEdgePolicy::Hard
    );
    // Equal rounded sample labels do not imply exact boundary coincidence.
    wire["sounds"]["sound"]["mapping"]["selection"]["start"] =
        json!(ExactRatio::new(799, 4).unwrap());
    let plan =
        RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap();
    assert_eq!(
        query(&plan, 200..201).spans[0].start_edge,
        AudioEdgePolicy::Automatic
    );
}

#[test]
fn event_edges_near_holds_shorten_the_combined_envelope_once() {
    let doc = document(&[(1000, false), (100, true), (1000, false)], 48_000);
    let mut wire = serde_json::to_value(doc).unwrap();
    wire["sounds"]["sound"]["mapping"]["selection"] = json!(ExactFrameRange {
        start: ExactRatio::integer(980),
        end: ExactRatio::integer(1120)
    });
    let plan =
        RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap();
    let gates = query(&plan, 979..1121);
    let audible: Vec<_> = gates.spans.iter().filter(|span| span.length > 0).collect();
    assert_eq!(audible.len(), 2);
    assert_eq!(audible[0].length, 20);
    assert_eq!(audible[1].length, 20);
    assert_eq!(
        factors(&query(&plan, 980..981)),
        [ExactRatio::new(1, 20).unwrap()]
    );
    assert_eq!(
        factors(&query(&plan, 1119..1120)),
        [ExactRatio::new(1, 20).unwrap()]
    );
}

#[test]
fn adjacent_and_sampleless_holds_have_no_phantom_audible_islands() {
    let doc = document(&[(2, false), (1, true), (1, true), (2, false)], 96_000);
    let plan = RenderPlan::compile(&doc).unwrap();
    let gates = query(&plan, 0..3);
    assert_eq!(
        gates
            .spans
            .iter()
            .map(|span| (span.samples.clone(), span.length))
            .collect::<Vec<_>>(),
        [
            (AudioSample(0)..AudioSample(1), 1),
            (AudioSample(1)..AudioSample(2), 0),
            (AudioSample(2)..AudioSample(3), 1),
        ]
    );
    assert_eq!(
        factors(&gates),
        [ExactRatio::ONE, ExactRatio::ZERO, ExactRatio::ONE]
    );
    let plan = RenderPlan::compile(&document(&[(1, true), (3, false)], 96_000)).unwrap();
    assert_eq!(
        factors(&query(&plan, 0..2)),
        [
            ExactRatio::new(1, 2).unwrap(),
            ExactRatio::new(1, 2).unwrap()
        ]
    );
}

#[test]
fn query_context_and_work_are_bounded_and_tiny_budgets_fail() {
    let parts = vec![(1, false); 2000];
    let plan = RenderPlan::compile(&document(&parts, 48_000)).unwrap();
    let sound = plan.root_sound(&sound()).unwrap();
    let query = sound
        .gate_fades(
            AudioSample(1000)..AudioSample(1001),
            AudioQueryLimits {
                maximum_spans: 1,
                maximum_work: 65_536,
            },
        )
        .unwrap();
    assert_eq!(query.spans.len(), 1);
    assert_eq!(factors(&query), [ExactRatio::ONE]);
    assert!(query.work < 20_000);
    assert!(
        sound
            .gate_fades(
                AudioSample(1000)..AudioSample(1001),
                AudioQueryLimits {
                    maximum_spans: 1,
                    maximum_work: 1
                }
            )
            .is_err()
    );
}

#[test]
fn rounded_root_terminal_does_not_replace_the_exact_event_edge_with_hard() {
    // Fifty frames at 72 kHz occupy 33 1/3 mix samples, rounded down to 33.
    let plan = RenderPlan::compile(&document(&[(50, false)], 72_000)).unwrap();
    let whole = query(&plan, 0..33);
    assert_eq!(whole.spans[0].length, 33);
    assert_eq!(whole.spans[0].end_edge, AudioEdgePolicy::Automatic);
    assert_eq!(
        factors(&query(&plan, 32..33)),
        [ExactRatio::new(1, 33).unwrap()]
    );
    assert_eq!(
        factors(&query(&plan, 0..1)),
        [ExactRatio::new(1, 33).unwrap()]
    );
}

#[test]
fn root_owner_hard_applies_only_at_exact_event_coincidence() {
    let mut wire = serde_json::to_value(document(&[(1000, false)], 48_000)).unwrap();
    wire["nodes"]["root"]["audio_edges"] = json!(AudioEdgePolicies {
        node_start: AudioEdgePolicy::Hard,
        node_end: AudioEdgePolicy::Hard,
        ..Default::default()
    });
    let plan =
        RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap();
    let full = query(&plan, 0..1000);
    assert_eq!(full.spans[0].start_edge, AudioEdgePolicy::Hard);
    assert_eq!(full.spans[0].end_edge, AudioEdgePolicy::Hard);
    assert!(factors(&full).iter().all(|gain| *gain == ExactRatio::ONE));

    // Both rounded endpoints still match root boundaries, but neither exact
    // event endpoint belongs to that constraint. Root Hard must not leak in.
    wire["sounds"]["sound"]["mapping"]["selection"] = json!(ExactFrameRange {
        start: ExactRatio::new(1, 4).unwrap(),
        end: ExactRatio::new(3999, 4).unwrap(),
    });
    let plan =
        RenderPlan::compile(&ProjectDocument::from_json(&wire.to_string()).unwrap()).unwrap();
    let full = query(&plan, 0..1000);
    assert_eq!(full.spans[0].start_edge, AudioEdgePolicy::Automatic);
    assert_eq!(full.spans[0].end_edge, AudioEdgePolicy::Automatic);
    assert_eq!(
        factors(&query(&plan, 0..1)),
        [ExactRatio::new(1, 192).unwrap()]
    );
    assert_eq!(
        factors(&query(&plan, 999..1000)),
        [ExactRatio::new(1, 192).unwrap()]
    );
}
