use super::*;

#[path = "occurrence_pause/mixed.rs"]
mod mixed;

fn repeated(child: &str, plays: u32, room: &SourceAudio) -> BeatNode {
    let NodeKind::Hold { recipe: gap } = room_tone(2, room.clone()).kind else {
        unreachable!()
    };
    let mut node = BeatNode::sequence("Repeat", vec![]);
    node.kind = NodeKind::Repeat {
        child: id(child),
        iterations: IterationOrder::new(RevisionId::new(child).unwrap(), plays).unwrap(),
        gap: Some(gap),
        escalation: None,
    };
    node
}

fn fixture(nested: bool) -> (ProjectDocument, FixtureProvider, Vec<Range<i64>>) {
    let (original, provider) = mono_original(4);
    let mut wire = serde_json::to_value(&original).unwrap();
    let room = audio_at_rate(321, 987, 44_100);
    wire["nodes"]["lead"] = serde_json::to_value(hold(1)).unwrap();
    wire["nodes"]["tail"] = wire["nodes"]["source"].clone();
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["lead", "repeat", "tail"]);
    let allocations = if nested {
        wire["nodes"]["repeat"] = serde_json::to_value(repeated("body", 2, &room)).unwrap();
        wire["nodes"]["body"] =
            serde_json::to_value(BeatNode::sequence("body", vec![id("inner"), id("room")]))
                .unwrap();
        wire["nodes"]["inner"] = serde_json::to_value(repeated("source", 2, &room)).unwrap();
        wire["nodes"]["room"] = serde_json::to_value(room_tone(2, room)).unwrap();
        vec![
            0..4,
            4..6,
            6..10,
            10..12,
            12..14,
            14..18,
            18..20,
            20..24,
            24..26,
            26..30,
        ]
    } else {
        wire["nodes"]["repeat"] = serde_json::to_value(repeated("source", 3, &room)).unwrap();
        vec![0..4, 4..6, 6..10, 10..12, 12..16, 16..20]
    };
    (
        ProjectDocument::from_json(&wire.to_string()).unwrap(),
        provider,
        allocations
            .into_iter()
            .map(|range| range.start + 1..range.end + 1)
            .collect(),
    )
}

fn pause(document: &ProjectDocument, at: i64) -> ProjectDocument {
    pause_within(document, at, None)
}

fn pause_within(
    document: &ProjectDocument,
    at: i64,
    within: Option<InstancePath>,
) -> ProjectDocument {
    let target = match &within {
        Some(instance) => document
            .pause_insertion_within(ProjectFrame(at), instance)
            .unwrap(),
        None => match document.pause_insertion_target(ProjectFrame(at)).unwrap() {
            PauseInsertionTarget::Occurrence(target) => target,
            _ => panic!("expected Repeat at {at}"),
        },
    };
    let revision = RevisionId::new("occurrence-pause").unwrap();
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision.clone(),
        command: Command::InsertOccurrenceTime {
            at: ProjectFrame(at),
            within,
            target: None,
            hold: HoldRecipe {
                duration: duration(1),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
                picture_context: None,
            },
            id: id("pause"),
            ai: false,
            all_plays: false,
            timing: AudioTimingId {
                allocation: revision,
                ordinal: 0,
            },
            identities: OccurrencePauseIdentities {
                isolation: OccurrenceIdentities {
                    nodes: (0..target.isolation_nodes)
                        .map(|n| id(&format!("isolate-{n}")))
                        .collect(),
                    marks: (0..target.isolation_marks)
                        .map(|n| MarkId::new(format!("mark-{n}")).unwrap())
                        .collect(),
                },
                split: SplitIdentities {
                    nodes: (0..target.split_nodes)
                        .map(|n| id(&format!("split-{n}")))
                        .collect(),
                },
                container: target.container.then(|| id("container")),
                gap: target.gap.then(|| id("gap")),
            },
        },
    };
    let transaction = apply(document, &request).unwrap_or_else(|e| panic!("pause at {at}: {e}"));
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(transaction.duration_delta, 1);
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    ProjectDocument::from_json(&after.to_json().unwrap()).unwrap()
}

#[test]
fn occurrence_pause_in_permitted_gap_keeps_root_sound_pcm_and_silences_only_new_time() {
    let (base, mut provider, _) = fixture(false);
    let mut wire = serde_json::to_value(&base).unwrap();
    let mute = AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::UNITY, true, vec![], vec![]).unwrap(),
    );
    for node in ["source", "tail"] {
        wire["nodes"][node]["audio_treatments"] = serde_json::to_value(&mute).unwrap();
    }
    wire["nodes"]["repeat"]["kind"]["gap"]["audio"] =
        serde_json::to_value(HoldAudio::Silence).unwrap();
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("c".repeat(64));
    let source = audio_at_rate(100, 27_000, 44_100);
    let eq = EqualizerRecipe::new(
        [3000, -6000, 3000].map(|db| GainDb::new(db).unwrap()),
        None,
        None,
    )
    .unwrap();
    wire["sounds"] = serde_json::json!({"effect": SoundEvent {
        owner: id("root"), label: "Root sound".into(),
        mapping: SourceAudioMapping::natural_rate(source.span, base.presentation_basis().frame_rate).unwrap(), source,
        offset: AudioSample(137), treatments: AudioTreatments::default().with_equalizer(Some(eq)).unwrap(),
        start_edge: AudioEdgePolicy::Hard, end_edge: AudioEdgePolicy::Hard, overflow: SoundOverflowPolicy::Reject,
    }});
    let NodeKind::Repeat { iterations, .. } = &base.nodes()[&id("repeat")].kind else {
        panic!()
    };
    let permissions = SoundHoldAllowances::try_from(
        (0..2)
            .map(|play| SoundHoldIssuer::RepeatGap {
                instance: InstancePath {
                    node: id("repeat"),
                    repeats: vec![],
                },
                gap_after: iterations.at(play).unwrap(),
            })
            .collect::<Vec<_>>(),
    )
    .unwrap();
    wire["sound_allowances"] = serde_json::json!({"effect": permissions});
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let after = pause(&before, 6);
    let mut read = |document: &ProjectDocument, start: usize, count: u32| {
        provider.revisions.insert(document.revision_id().clone());
        StageAudio::new(Arc::new(RenderPlan::compile(document).unwrap()))
            .prepare_authored_bus(
                &mut provider,
                AudioSample(start as i64),
                count,
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap()
            .samples
    };
    let inserted_samples = ntsc_boundary(7) - ntsc_boundary(6);
    for frame in [1, 5, 6, 10, 17] {
        let start = ntsc_boundary(frame) + 512;
        let expected = read(&before, start, 256);
        assert!(expected.iter().flatten().any(|value| value.abs() > 0.001));
        let shift = if frame >= 6 { inserted_samples } else { 0 };
        let actual = read(&after, start + shift, 256);
        for (sample, expected) in actual.iter().zip(expected) {
            for channel in 0..2 {
                assert!(
                    (sample[channel] - expected[channel]).abs() < 1e-6,
                    "frame={frame}"
                );
            }
        }
    }
    assert!(
        read(&after, ntsc_boundary(6), inserted_samples as u32)
            .iter()
            .all(|sample| *sample == [0.; 2])
    );
}
