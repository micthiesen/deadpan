//! Explode keeps every decoded 30000/1001 sample: raw, edge-faded and limited.
use super::*;
use deadpan_audio::LimitedAudio;

fn explode(document: &ProjectDocument, repeat: &str, name: &str) -> ProjectDocument {
    let needs = document.explode_requirements(&id(repeat)).unwrap();
    let result = edit(
        document,
        name,
        Command::Explode {
            node: id(repeat),
            identities: OccurrenceIdentities {
                nodes: (0..needs.nodes)
                    .map(|n| id(&format!("{name}-node-{n}")))
                    .collect(),
                marks: (0..needs.marks)
                    .map(|n| MarkId::new(format!("{name}-mark-{n}")).unwrap())
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: revision(name),
                ordinal: 0,
            },
        },
    );
    assert!(matches!(
        result.nodes()[&id(repeat)].kind,
        NodeKind::Sequence { .. }
    ));
    assert_eq!(result.duration().unwrap(), document.duration().unwrap());
    result
}

/// The whole timeline through all three output stages, in irregular blocks.
pub(super) fn whole(document: &ProjectDocument, provider: &mut Provider) -> [Vec<[f32; 2]>; 3] {
    let plan = Arc::new(RenderPlan::compile(document).unwrap());
    provider.revisions.insert(document.revision_id().clone());
    let total = plan.audio_duration().unwrap().0;
    let mut raw = StageAudio::new(Arc::clone(&plan));
    let mut faded = StageAudio::new(Arc::clone(&plan));
    let mut limited = LimitedAudio::new(Arc::clone(&plan));
    let mut output = [Vec::new(), Vec::new(), Vec::new()];
    let mut start = 0;
    while start < total {
        let count = u32::try_from((total - start).min(251)).unwrap();
        let cancelled = AtomicBool::new(false);
        output[0].extend(
            raw.read(provider, AudioSample(start), count, TIMEOUT, &cancelled)
                .unwrap()
                .samples,
        );
        output[1].extend(
            faded
                .read_edge_faded(provider, AudioSample(start), count, TIMEOUT, &cancelled)
                .unwrap()
                .samples,
        );
        output[2].extend(
            limited
                .read(provider, AudioSample(start), count, TIMEOUT, &cancelled)
                .unwrap_or_else(|error| {
                    panic!(
                        "{} limited read at {start}: {error:?}",
                        document.revision_id()
                    )
                })
                .samples,
        );
        start += i64::from(count);
    }
    assert!(
        output[0]
            .iter()
            .flatten()
            .any(|sample| sample.abs() > 0.001)
    );
    output
}

#[track_caller]
pub(super) fn same_pcm(before: &ProjectDocument, after: &ProjectDocument) {
    let mut provider = Provider::new();
    let old = whole(before, &mut provider);
    let new = whole(after, &mut provider);
    for (stage, (old, new)) in ["raw", "edge-faded", "limited"]
        .iter()
        .zip(old.iter().zip(&new))
    {
        assert_eq!(old.len(), new.len(), "{stage} length");
        if let Some(index) = old.iter().zip(new).position(|(a, b)| a != b) {
            panic!(
                "{stage} sample {index} differs: {:?} != {:?}",
                old[index], new[index]
            );
        }
    }
}

pub(super) fn escalated(mut node: BeatNode, gain: i32) -> BeatNode {
    let NodeKind::Repeat { escalation, .. } = &mut node.kind else {
        unreachable!()
    };
    *escalation = Some(RepeatEscalation {
        gain_step: GainDb::new(gain).unwrap(),
        zoom: None,
    });
    node
}

pub(super) fn with_gap(mut node: BeatNode, recipe: HoldRecipe) -> BeatNode {
    let NodeKind::Repeat { gap, .. } = &mut node.kind else {
        unreachable!()
    };
    *gap = Some(recipe);
    node
}

#[test]
fn exploded_default_plays_gaps_preserve_and_escalation_keep_every_ntsc_sample() {
    let original = document(
        ntsc(),
        &["lead", "repeat", "tail"],
        vec![
            ("lead", silence(1)),
            ("a", source(ntsc(), 2)),
            ("b", source(ntsc(), 5)),
            ("p", preserve("b", 3, 4)),
            ("group", BeatNode::sequence("Group", vec![id("a"), id("p")])),
            (
                "repeat",
                escalated(with_gap(repeat("group", 4), room(2, 100..321)), -3000),
            ),
            ("tail", source(ntsc(), 3)),
        ],
    );
    // An override for the second play changes its gain; the rest stay shared.
    let overridden = edit(
        &original,
        "louder-second",
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("a"),
                repeats: vec![RepeatInstance {
                    node: id("repeat"),
                    iteration: play(1),
                }],
            },
            edit: OccurrenceEdit::SetAudioTreatments {
                treatments: AudioTreatments::from_clip_gain(
                    ClipGain::new(GainDb::new(-6000).unwrap(), false, Vec::new(), Vec::new())
                        .unwrap(),
                ),
            },
            identities: OccurrenceIdentities {
                nodes: (0..4).map(|n| id(&format!("override-{n}"))).collect(),
                marks: Vec::new(),
            },
        },
    );
    same_pcm(&overridden, &explode(&overridden, "repeat", "explode"));
    same_pcm(&original, &explode(&original, "repeat", "explode-plain"));
}

#[test]
fn exploded_born_plays_keep_definition_clocks_and_isolated_gap_phase() {
    let original = document(
        ntsc(),
        &["x", "y"],
        vec![("x", source(ntsc(), 3)), ("y", source(ntsc(), 2))],
    );
    // A pause inside x creates retained suffix clocks before the Repeat exists.
    let paused = insert_pause(&original, 1, 1, "pause");
    let selection = SliceCaptureSelection::Child { node: id("y") };
    let plan = paused.repeat_selection(&id("root"), &selection, 3).unwrap();
    let repeated = edit(
        &paused,
        "wrap",
        Command::RepeatSelection {
            parent: id("root"),
            selection,
            plays: 3,
            identities: RepeatSelectionIdentities {
                repeat: id("wrapped"),
                group: plan.needs_group.then(|| id("body")),
                split: SplitIdentities {
                    nodes: (0..plan.required_split_ids)
                        .map(|n| id(&format!("split-{n}")))
                        .collect(),
                },
            },
            timing: AudioTimingId {
                allocation: revision("wrap"),
                ordinal: 0,
            },
        },
    );
    let NodeKind::Repeat { iterations, .. } = &repeated.nodes()[&id("wrapped")].kind else {
        panic!("wrapped")
    };
    let plays: Vec<_> = (0..3).map(|n| iterations.at(n).unwrap()).collect();
    assert!(
        !repeated.audio_bindings().bindings().is_empty(),
        "born plays use retained definition clocks"
    );
    let gapped = edit(
        &repeated,
        "gaps",
        Command::SetRepeatGaps {
            node: id("wrapped"),
            gap: Some(room(2, 100..321)),
            branches: vec![RepeatGapHold {
                after: plays[1].clone(),
                id: id("branch"),
                hold: room(1, 200..500),
            }],
            timing: AudioTimingId {
                allocation: revision("gaps"),
                ordinal: 0,
            },
        },
    );
    same_pcm(&repeated, &explode(&repeated, "wrapped", "explode-born"));
    same_pcm(&gapped, &explode(&gapped, "wrapped", "explode-gapped"));
}

#[test]
fn exploding_inner_then_outer_repeat_keeps_nested_occurrence_samples() {
    let original = document(
        ntsc(),
        &["lead", "outer"],
        vec![
            ("lead", source(ntsc(), 1)),
            ("a", source(ntsc(), 2)),
            ("inner", with_gap(repeat("a", 2), room(1, 100..321))),
            (
                "outer",
                BeatNode {
                    kind: NodeKind::Repeat {
                        child: id("inner"),
                        iterations: IterationOrder::new(revision("outer-plays"), 3).unwrap(),
                        gap: Some(gap(1, HoldAudio::Silence)),
                        escalation: None,
                    },
                    ..repeat("inner", 1)
                },
            ),
        ],
    );
    let inner = explode(&original, "inner", "explode-inner");
    same_pcm(&original, &inner);
    let outer = explode(&inner, "outer", "explode-outer");
    same_pcm(&original, &outer);
    let both_ways = explode(
        &explode(&original, "outer", "outer-first"),
        "inner",
        "then-inner",
    );
    same_pcm(&original, &both_ways);
}

fn qualified(document: &ProjectDocument) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("b".repeat(64));
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn exploded_beat_sounds_and_root_sounds_keep_every_ntsc_sample() {
    let original = qualified(&document(
        ntsc(),
        &["lead", "repeat"],
        vec![
            ("lead", source(ntsc(), 2)),
            ("a", source(ntsc(), 3)),
            (
                "repeat",
                escalated(with_gap(repeat("a", 3), gap(2, HoldAudio::Silence)), 2000),
            ),
        ],
    ));
    let span = audio(1000..2000).span;
    let mapping = SourceAudioMapping::natural_rate(span, ntsc()).unwrap();
    let beat = edit(
        &original,
        "beat-sound",
        Command::SetBeatSound {
            owner: id("a"),
            id: SoundId::new("blip").unwrap(),
            event: BeatSound {
                label: "Blip".into(),
                source: audio(1000..2000),
                mapping,
                offset: AudioSample(311),
                gain_millidecibels: -3000,
                start_edge: AudioEdgePolicy::Automatic,
                end_edge: AudioEdgePolicy::Automatic,
                overflow: SoundOverflowPolicy::Reject,
            },
        },
    );
    let rooted = edit(
        &beat,
        "root-sound",
        Command::SetSound {
            id: SoundId::new("over").unwrap(),
            event: SoundEvent {
                owner: id("root"),
                label: "Over the gaps".into(),
                source: audio(3000..9000),
                mapping: SourceAudioMapping::natural_rate(audio(3000..9000).span, ntsc()).unwrap(),
                offset: AudioSample(4000),
                gain_millidecibels: 0,
                start_edge: AudioEdgePolicy::Automatic,
                end_edge: AudioEdgePolicy::Hard,
                overflow: SoundOverflowPolicy::Reject,
            },
        },
    );
    let exploded = explode(&rooted, "repeat", "explode-sounds");
    assert_eq!(
        exploded.beat_sounds().len(),
        3,
        "every play owns its own copy of the beat sound"
    );
    same_pcm(&rooted, &exploded);
}
