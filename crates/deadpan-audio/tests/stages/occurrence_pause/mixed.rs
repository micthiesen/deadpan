use super::*;

#[derive(Debug)]
struct SourceOccurrence {
    outer: u32,
    inner: u32,
    node: &'static str,
    inner_repeat: &'static str,
}

impl SourceOccurrence {
    fn presentation(&self, document: &ProjectDocument) -> InstancePath {
        let iteration = |repeat: &str, ordinal| {
            let NodeKind::Repeat { iterations, .. } = &document.nodes()[&id(repeat)].kind else {
                panic!("{repeat} is not a Repeat")
            };
            RepeatInstance {
                node: id(repeat),
                iteration: iterations.at(ordinal).unwrap(),
            }
        };
        InstancePath {
            node: id(self.node),
            repeats: vec![
                iteration("repeat", self.outer),
                iteration(self.inner_repeat, self.inner),
            ],
        }
    }

    fn target(&self, document: &ProjectDocument, outer_default: bool) -> ScopedNodeTarget {
        let presentation = self.presentation(document);
        ScopedNodeTarget {
            node: presentation.node,
            repeats: presentation
                .repeats
                .into_iter()
                .enumerate()
                .map(|(level, step)| RepeatEditStep {
                    repeat: step.node,
                    branch: if outer_default == (level == 0) {
                        RepeatEditBranch::Default
                    } else {
                        RepeatEditBranch::Play {
                            iteration: step.iteration,
                        }
                    },
                })
                .collect(),
        }
    }
}

fn scoped_pause(
    document: &ProjectDocument,
    at: i64,
    target: ScopedNodeTarget,
    within: InstancePath,
    growth: i64,
) -> ProjectDocument {
    assert!(target.matches_instance(document, &within).unwrap());
    let required = document
        .pause_insertion_scoped(ProjectFrame(at), &target, &within)
        .unwrap();
    let revision = RevisionId::new("mixed-pause").unwrap();
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision.clone(),
        command: Command::InsertOccurrenceTime {
            at: ProjectFrame(at),
            within: Some(within),
            target: Some(target),
            hold: HoldRecipe {
                duration: duration(1),
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
                picture_context: None,
            },
            id: id("mixed-pause"),
            ai: false,
            all_plays: false,
            timing: AudioTimingId {
                allocation: revision,
                ordinal: 0,
            },
            identities: OccurrencePauseIdentities {
                isolation: OccurrenceIdentities {
                    nodes: (0..required.isolation_nodes)
                        .map(|index| id(&format!("mixed-isolate-{index}")))
                        .collect(),
                    marks: (0..required.isolation_marks)
                        .map(|index| MarkId::new(format!("mixed-mark-{index}")).unwrap())
                        .collect(),
                },
                split: SplitIdentities {
                    nodes: (0..required.split_nodes)
                        .map(|index| id(&format!("mixed-split-{index}")))
                        .collect(),
                },
                container: required.container.then(|| id("mixed-container")),
                gap: required.gap.then(|| id("mixed-gap")),
            },
        },
    };
    let transaction = apply(document, &request).unwrap();
    assert_eq!(transaction.duration_delta, growth);
    let after = transaction.forward.apply(document).unwrap();
    assert_eq!(
        after.duration().unwrap().frames(),
        document.duration().unwrap().frames() + growth
    );
    assert_eq!(transaction.inverse.apply(&after).unwrap(), *document);
    ProjectDocument::from_json(&after.to_json().unwrap()).unwrap()
}

fn record_authored(
    document: &ProjectDocument,
    provider: &mut FixtureProvider,
    pieces: &[u32],
) -> Recorded {
    provider.revisions.insert(document.revision_id().clone());
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(document).unwrap()));
    let total = ntsc_boundary(document.duration().unwrap().frames());
    let mut result = Recorded {
        samples: Vec::with_capacity(total),
        suppressed: vec![false; total],
    };
    for count in pieces.iter().copied().cycle() {
        let start = result.samples.len();
        if start == total {
            break;
        }
        let block = renderer
            .prepare_authored_bus(
                provider,
                AudioSample(start as i64),
                count.min((total - start) as u32),
                TIMEOUT,
                &AtomicBool::new(false),
            )
            .unwrap();
        result.samples.extend(block.samples);
        for range in &block.suppressed {
            result.suppressed[range.start.0 as usize..range.end.0 as usize].fill(true);
        }
    }
    result
}

#[test]
fn mixed_play_default_pause_moves_root_sound_after_inner_repeat_not_outer_repeat_end() {
    let (base, mut provider, _) = fixture(true);
    let mut wire = serde_json::to_value(&base).unwrap();
    let NodeKind::Hold { recipe: gap } = room_tone(1, audio_at_rate(321, 987, 44_100)).kind else {
        panic!()
    };
    // Each outer body is inner Repeat 9 + following room 2, then a one-frame
    // inter-play gap. The selected second inner Repeat is exactly [13,22).
    wire["nodes"]["repeat"] =
        serde_json::to_value(repeated("body", 3, &audio_at_rate(321, 987, 44_100))).unwrap();
    wire["nodes"]["repeat"]["kind"]["gap"] = serde_json::to_value(&gap).unwrap();
    wire["nodes"]["inner"]["kind"]["gap"] = serde_json::to_value(gap).unwrap();
    let mute = AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::UNITY, true, vec![], vec![]).unwrap(),
    );
    for node in ["source", "tail", "room", "inner", "repeat"] {
        wire["nodes"][node]["audio_treatments"] = serde_json::to_value(&mute).unwrap();
    }
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("c".repeat(64));
    let source = audio_at_rate(100, 987, 44_100);
    wire["sounds"] = serde_json::json!({"effect": SoundEvent {
        owner: id("root"), label: "Following inner Repeat".into(),
        mapping: SourceAudioMapping::natural_rate(source.span, base.presentation_basis().frame_rate).unwrap(), source,
        offset: AudioSample(ntsc_boundary(23) as i64), treatments: AudioTreatments::default(),
        start_edge: AudioEdgePolicy::Hard, end_edge: AudioEdgePolicy::Hard, overflow: SoundOverflowPolicy::Reject,
    }});
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(before.duration().unwrap().frames(), 40);
    let shown = SourceOccurrence {
        outer: 1,
        inner: 0,
        node: "source",
        inner_repeat: "inner",
    };
    let after = scoped_pause(
        &before,
        14,
        shown.target(&before, false),
        shown.presentation(&before),
        2,
    );
    let old = record_authored(&before, &mut provider, &[257]);
    let actual = record_authored(&after, &mut provider, &[127, 3, 251]);
    assert!(
        old.samples[ntsc_boundary(23)..ntsc_boundary(24)]
            .iter()
            .flatten()
            .any(|v| v.abs() > 0.01)
    );
    // The root journal retains its sampled bus at the authored insertion cut,
    // not a new resampling phase at the sound onset. On NTSC these two-frame
    // sample translations can differ by one at different frame origins.
    let root_shift = ntsc_boundary(24) - ntsc_boundary(22);
    let translated = ntsc_boundary(23) + root_shift..ntsc_boundary(24) + root_shift;
    assert_resumed(&actual, &old, translated.clone(), ntsc_boundary(23));
    assert!(
        actual.samples[..translated.start]
            .iter()
            .all(|sample| *sample == [0.; 2])
    );
    assert!(
        actual.samples[translated.end..]
            .iter()
            .all(|sample| *sample == [0.; 2])
    );
    assert_pause(&actual, ntsc_boundary(14)..ntsc_boundary(15));
    assert_pause(&actual, ntsc_boundary(20)..ntsc_boundary(21));
    assert_eq!(record_authored(&after, &mut provider, &[1, 239]), actual);
}
