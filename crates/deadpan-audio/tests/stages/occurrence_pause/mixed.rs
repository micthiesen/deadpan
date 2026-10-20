use super::*;

#[derive(Debug)]
struct SourceOccurrence {
    range: Range<i64>,
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

/// Authored layout, independent of the render plan:
/// lead 1; three bodies of (Source 4, gap 2, Source 4, room 2),
/// separated by two-frame gaps; then a four-frame tail.
fn mixed_fixture(
    overrides: bool,
) -> (
    ProjectDocument,
    FixtureProvider,
    Vec<Range<i64>>,
    Vec<SourceOccurrence>,
) {
    let (base, provider, _) = fixture(true);
    let mut wire = serde_json::to_value(&base).unwrap();
    let room = audio_at_rate(321, 987, 44_100);
    wire["nodes"]["repeat"] = serde_json::to_value(repeated("body", 3, &room)).unwrap();
    if overrides {
        wire["nodes"]["owned-body"] = serde_json::to_value(BeatNode::sequence(
            "Owned outer play",
            vec![id("owned-inner"), id("owned-room")],
        ))
        .unwrap();
        wire["nodes"]["owned-inner"] =
            serde_json::to_value(repeated("owned-source", 2, &room)).unwrap();
        wire["nodes"]["owned-room"] = wire["nodes"]["room"].clone();
        for (name, start) in [
            ("owned-source", 8000),
            ("inner-override", 14_000),
            ("owned-inner-override", 24_000),
        ] {
            let mut node = base.nodes()[&id("source")].clone();
            let NodeKind::Source { source } = &mut node.kind else {
                panic!()
            };
            let audio = audio_at_rate(start, start + 20_000, 44_100);
            source.audio_mapping =
                SourceAudioMapping::natural_rate(audio.span, base.presentation_basis().frame_rate)
                    .unwrap();
            source.audio = Some(audio);
            wire["nodes"][name] = serde_json::to_value(node).unwrap();
        }
        for (repeat, allocation, root) in [
            ("repeat", "body", "owned-body"),
            ("inner", "source", "inner-override"),
            ("owned-inner", "owned-source", "owned-inner-override"),
        ] {
            wire["overrides"][repeat] = serde_json::to_value(
                PlayOverrides::try_from(vec![PlayOverride {
                    iteration: IterationId {
                        allocation: RevisionId::new(allocation).unwrap(),
                        ordinal: 1,
                    },
                    root: id(root),
                }])
                .unwrap(),
            )
            .unwrap();
        }
    }
    let mut allocations = Vec::from_iter(std::iter::once(0..1));
    let mut sources = Vec::new();
    for outer in 0..3 {
        let start = 1 + 14 * i64::from(outer);
        for inner in 0..2 {
            let range = start + 6 * i64::from(inner)..start + 6 * i64::from(inner) + 4;
            allocations.push(range.clone());
            sources.push(SourceOccurrence {
                range,
                outer,
                inner,
                node: match (overrides && outer == 1, overrides && inner == 1) {
                    (true, true) => "owned-inner-override",
                    (true, false) => "owned-source",
                    (false, true) => "inner-override",
                    (false, false) => "source",
                },
                inner_repeat: if overrides && outer == 1 {
                    "owned-inner"
                } else {
                    "inner"
                },
            });
            // The inner gap exists only between the two plays.
            if inner == 0 {
                allocations.push(start + 4..start + 6);
            }
        }
        allocations.push(start + 10..start + 12);
        if outer != 2 {
            allocations.push(start + 12..start + 14);
        }
    }
    allocations.push(41..45);
    let document = ProjectDocument::from_json(&wire.to_string()).unwrap();
    assert_eq!(document.duration().unwrap().frames(), 45);
    (document, provider, allocations, sources)
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

fn assert_moved_support(
    actual: &Recorded,
    old: &Recorded,
    range: Range<i64>,
    shift: i64,
    continues: bool,
    label: &str,
) {
    assert_moved(actual, old, range.start, range.end, shift, continues, label);
    let before = ntsc_boundary(range.start)..ntsc_boundary(range.end);
    let after = ntsc_boundary(range.start + shift)..ntsc_boundary(range.end + shift);
    for (offset, sample) in after.enumerate() {
        let previous = before.start + offset;
        let expected = if previous < before.end || continues {
            old.suppressed[previous]
        } else {
            true
        };
        assert_eq!(
            actual.suppressed[sample], expected,
            "{label}: suppression old {previous}, new {sample}"
        );
    }
}

/// The expected cuts come only from the authored fixture and scope choices.
/// Each Source, gap, room-tone beat and tail retains its own old PCM support.
fn assert_mixed_pcm(
    before: &ProjectDocument,
    after: &ProjectDocument,
    provider: &mut FixtureProvider,
    allocations: &[Range<i64>],
    cuts: &[i64],
    label: &str,
) {
    let old = record(before, provider, false, &[256]);
    assert!(old.samples.iter().flatten().any(|value| value.abs() > 0.01));
    let actual = record(after, provider, false, &[127, 3, 251]);
    assert_eq!(actual.samples.len(), ntsc_boundary(45 + cuts.len() as i64));
    let mut shift = 0;
    for allocation in allocations {
        let label = format!("{label} allocation {allocation:?}");
        if let Some(cut) = cuts.iter().find(|cut| allocation.contains(cut)) {
            assert_moved_support(&actual, &old, allocation.start..*cut, shift, true, &label);
            assert_pause(
                &actual,
                ntsc_boundary(cut + shift)..ntsc_boundary(cut + shift + 1),
            );
            shift += 1;
            assert_moved_support(&actual, &old, *cut..allocation.end, shift, false, &label);
        } else {
            assert_moved_support(&actual, &old, allocation.clone(), shift, false, &label);
        }
    }
    assert_eq!(shift, cuts.len() as i64);
    assert_eq!(record(after, provider, false, &[1, 239]), actual, "{label}");
}

#[test]
fn mixed_default_play_pauses_keep_each_44100_allocation_from_first_and_last_presentations() {
    let (before, mut provider, allocations, sources) = mixed_fixture(false);
    for outer_default in [true, false] {
        for chosen in 0..2 {
            for representative in [0, 1] {
                let (outer, inner) = if outer_default {
                    (representative * 2, chosen)
                } else {
                    (chosen * 2, representative)
                };
                let shown = sources
                    .iter()
                    .find(|source| source.outer == outer && source.inner == inner)
                    .unwrap();
                for local in [1, 3] {
                    let cuts: Vec<_> = sources
                        .iter()
                        .filter(|source| {
                            if outer_default {
                                source.inner == inner
                            } else {
                                source.outer == outer
                            }
                        })
                        .map(|source| source.range.start + local)
                        .collect();
                    let after = scoped_pause(
                        &before,
                        shown.range.start + local,
                        shown.target(&before, outer_default),
                        shown.presentation(&before),
                        if outer_default { 3 } else { 2 },
                    );
                    assert_mixed_pcm(
                        &before,
                        &after,
                        &mut provider,
                        &allocations,
                        &cuts,
                        &format!(
                            "outer_default={outer_default} shown={outer}/{inner} local={local}"
                        ),
                    );
                }
            }
        }
    }
}

#[test]
fn mixed_pause_defaults_exclude_overrides_and_selected_owned_branches_grow_once() {
    let (before, mut provider, allocations, sources) = mixed_fixture(true);
    // Outer Default excludes its owned middle play. Inner Play can select the
    // already owned second child; that child grows once in each default body.
    for inner in 0..2 {
        for outer in [0, 2] {
            let shown = sources
                .iter()
                .find(|source| source.outer == outer && source.inner == inner)
                .unwrap();
            for local in [1, 3] {
                let cuts: Vec<_> = sources
                    .iter()
                    .filter(|source| source.outer != 1 && source.inner == inner)
                    .map(|source| source.range.start + local)
                    .collect();
                let after = scoped_pause(
                    &before,
                    shown.range.start + local,
                    shown.target(&before, true),
                    shown.presentation(&before),
                    2,
                );
                assert_mixed_pcm(
                    &before,
                    &after,
                    &mut provider,
                    &allocations,
                    &cuts,
                    &format!("outer Default inner Play shown={outer}/{inner} local={local}"),
                );
            }
        }
    }
    // Every inner Repeat has an owned second play. Inner Default excludes it,
    // including when the selected outer branch was already independently owned.
    for outer in 0..3 {
        let shown = sources
            .iter()
            .find(|source| source.outer == outer && source.inner == 0)
            .unwrap();
        for local in [1, 3] {
            let cut = shown.range.start + local;
            let after = scoped_pause(
                &before,
                cut,
                shown.target(&before, false),
                shown.presentation(&before),
                1,
            );
            assert_mixed_pcm(
                &before,
                &after,
                &mut provider,
                &allocations,
                &[cut],
                &format!("outer Play inner Default shown={outer}/0 local={local}"),
            );
        }
    }
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
        for range in block.suppressed {
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
        range: 13..17,
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
