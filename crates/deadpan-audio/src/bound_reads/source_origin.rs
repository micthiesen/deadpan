use super::*;

#[path = "source_origin/gain.rs"]
mod gain;

fn partition(child: &str, start: i64, end: i64) -> BeatNode {
    BeatNode {
        framing: None,
        label: "Retained physical source".into(),
        audio_treatments: Default::default(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Retime {
            child: id(child),
            duration: frames(end - start),
            mapping: FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap(),
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Partition,
        },
    }
}

fn with_bindings(document: &ProjectDocument, bindings: &AudioBindingState) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["audio_bindings"] = serde_json::to_value(bindings).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

// This fixture models the future physical edit without introducing a command.
// Its patch atomically changes the source, its parent, and the retained clocks.
fn prefix_patch(
    document: &ProjectDocument,
    parent: &str,
    prefix: i64,
    retained: &AudioBindingState,
) -> DocumentPatch {
    let mut node = document.nodes()[&id("a")].clone();
    let NodeKind::Source { source } = &mut node.kind else {
        panic!("fixture source");
    };
    let duration = source.duration;
    let shift = ExactRatio::integer(prefix);
    let mapping = source.audio_mapping;
    let selection = mapping.selection_frames(duration).unwrap();
    source.audio_mapping = SourceAudioMapping::SelectedPlacement {
        start: mapping.start_frames().checked_add(shift).unwrap(),
        frames: mapping.duration_frames(duration).unwrap(),
        // An explicit dormant selection keeps equal endpoints when rebased.
        selection: ExactFrameRange {
            start: selection.start.checked_add(shift).unwrap(),
            end: selection.end.checked_add(shift).unwrap(),
        },
    };
    source.duration = frames(duration.frames() + prefix);
    // Blank picture has no source clock. The independent sample offset is
    // deliberately retained, not folded into the translated frame mapping.
    let mut parent_node = document.nodes()[&id(parent)].clone();
    match &mut parent_node.kind {
        NodeKind::Sequence { children } => {
            let child = children
                .iter_mut()
                .find(|child| **child == id("a"))
                .unwrap();
            *child = id("physical-window");
        }
        NodeKind::Retime { child, .. } => {
            assert_eq!(*child, id("a"));
            *child = id("physical-window");
        }
        _ => panic!("fixture parent"),
    }
    let bindings = AudioBindingState::new_with_gaps(
        retained
            .timings()
            .iter()
            .map(|(id, layout)| AudioTimingRecord {
                id: id.clone(),
                layout: layout.clone(),
            })
            .collect(),
        retained
            .bindings()
            .iter()
            .map(|(owner, binding)| {
                (
                    owner.clone(),
                    if *owner == id("a") {
                        binding.rebase_local(shift).unwrap()
                    } else {
                        binding.clone()
                    },
                )
            })
            .collect(),
        retained.gap_bindings().clone(),
    )
    .unwrap();
    DocumentPatch {
        project_id: document.project_id().clone(),
        from_revision: document.revision_id().clone(),
        to_revision: revision(&format!("physical-prefix-{prefix}")),
        presentation: None,
        nodes: BTreeMap::from([
            (
                id("a"),
                ValueChange {
                    before: Some(document.nodes()[&id("a")].clone()),
                    after: Some(node),
                },
            ),
            (
                id(parent),
                ValueChange {
                    before: Some(document.nodes()[&id(parent)].clone()),
                    after: Some(parent_node),
                },
            ),
            (
                id("physical-window"),
                ValueChange {
                    before: None,
                    after: Some(partition("a", prefix, prefix + duration.frames())),
                },
            ),
        ]),
        assets: BTreeMap::new(),
        marks: BTreeMap::new(),
        sounds: BTreeMap::new(),
        beat_sounds: BTreeMap::new(),
        sound_routes: BTreeMap::new(),
        sound_allowances: BTreeMap::new(),
        overrides: BTreeMap::new(),
        gap_overrides: BTreeMap::new(),
        audio_lineage: BTreeMap::new(),
        audio_bindings: Some(ValueChange {
            before: Some(document.audio_bindings().clone()),
            after: Some(bindings),
        }),
    }
}

fn pcm(
    document: &ProjectDocument,
    provider: &mut Provider,
    range: Range<i64>,
    chunk: u32,
    edge_faded: bool,
) -> Vec<[f32; 2]> {
    let mut renderer = StageAudio::new(Arc::new(RenderPlan::compile(document).unwrap()));
    let mut result = Vec::new();
    let mut cursor = range.start;
    while cursor < range.end {
        let count = u32::try_from((range.end - cursor).min(i64::from(chunk))).unwrap();
        let samples = if edge_faded {
            renderer
                .read_edge_faded(
                    provider,
                    AudioSample(cursor),
                    count,
                    Duration::from_secs(10),
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples
        } else {
            render(&mut renderer, provider, cursor, count).samples
        };
        result.extend(samples);
        cursor += i64::from(count);
    }
    result
}

// The expected source coordinate and output extent are authored test constants.
// No RenderPlan or binding resolver participates in this oracle.
fn source_oracle(
    provider: &Provider,
    source: Range<i64>,
    phase: ExactRatio,
    count: u32,
) -> Vec<[f32; 2]> {
    let recipe = ResampleRecipe::new(
        source,
        phase,
        AudioSample(0),
        ExactRatio::ONE,
        AudioSample(0)..AudioSample(i64::from(count)),
    )
    .unwrap();
    let mut result = Vec::new();
    let mut cursor = 0;
    while cursor < count {
        let frames = (count - cursor).min(251);
        result.extend(
            provider
                .prepared
                .prepare(
                    recipe.clone(),
                    AudioSample(i64::from(cursor)),
                    frames,
                    Duration::from_secs(10),
                    &AtomicBool::new(false),
                )
                .unwrap()
                .samples,
        );
        cursor += frames;
    }
    result
}

#[test]
fn physical_prefix_keeps_plain_captured_and_previously_bound_ntsc_pcm() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let mut audio = source(rate, 0, 6000, 4);
    let NodeKind::Source { source } = &mut audio.kind else {
        unreachable!();
    };
    source.audio_offset = AudioSample(7);
    let (original, mut provider) =
        document(rate, &["lead", "a"], vec![("lead", hold(1)), ("a", audio)]);
    let captured = capture_unbound_audio_bindings(
        &original,
        AudioTimingId {
            allocation: revision("before-physical-prefix"),
            ordinal: 0,
        },
    )
    .unwrap();
    let mut moved_wire = serde_json::to_value(&original).unwrap();
    moved_wire["nodes"]["lead"] = serde_json::to_value(hold(2)).unwrap();
    let moved = ProjectDocument::from_json(&moved_wire.to_string()).unwrap();
    let previously_bound = with_bindings(&moved, &captured);

    // 48000 * 1001/30000 = 8008/5 samples/frame. B(1)=1602,
    // so the first selected sample is 2/5 past the exact source start.
    // Moving the beat to frame 2 keeps this phase only with its old binding.
    let source_pcm = source_oracle(&provider, 0..6000, ratio(2, 5), 6000);
    assert_ne!(
        source_pcm,
        source_oracle(&provider, 0..6000, ExactRatio::ZERO, 6000)
    );
    for (label, before, retained, start, end) in [
        ("plain", &original, original.audio_bindings(), 1609, 8008),
        ("capture-before-rebase", &original, &captured, 1609, 8008),
        ("previously-bound", &previously_bound, &captured, 3210, 9610),
    ] {
        let mut expected = vec![[0.0; 2]; end as usize];
        expected[start as usize..start as usize + 6000].copy_from_slice(&source_pcm);
        assert_eq!(
            pcm(before, &mut provider, 0..end, 251, false),
            expected,
            "{label}"
        );
        let faded = pcm(before, &mut provider, 0..end, 251, true);
        for prefix in [1, 3] {
            let patch = prefix_patch(before, "root", prefix, retained);
            let after = patch.apply(before).unwrap();
            assert_eq!(after.duration().unwrap(), before.duration().unwrap());
            assert_eq!(after.audio_bindings().timings(), retained.timings());
            let NodeKind::Source { source } = &after.nodes()[&id("a")].kind else {
                unreachable!();
            };
            assert_eq!(source.audio_offset, AudioSample(7));
            for chunk in [193, 239] {
                assert_eq!(
                    pcm(&after, &mut provider, 0..end, chunk, false),
                    expected,
                    "{label}, prefix {prefix}, chunk {chunk}"
                );
                assert_eq!(
                    pcm(&after, &mut provider, 0..end, chunk, true),
                    faded,
                    "edge envelope: {label}, prefix {prefix}, chunk {chunk}"
                );
            }
            let restored = patch.inverse().apply(&after).unwrap();
            assert_eq!(&restored, before);
            assert_eq!(pcm(&restored, &mut provider, 0..end, 193, false), expected);
        }
    }
}

fn historical_placement(ordinal: u32) -> AudioPlacementTemplate {
    let mut placement = lattice("a", AudioClockRoot::ProjectRootRoundEven);
    placement.reference.timing.ordinal = ordinal;
    placement
}

#[test]
fn physical_prefix_keeps_symbolic_resume_and_chronological_reanchor_pcm() {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let (old, mut provider) = document(rate, &["a"], vec![("a", source(rate, 0, 8197, 6))]);
    let layout_at = |lead| {
        let mut wire = serde_json::to_value(&old).unwrap();
        wire["nodes"]["lead"] = serde_json::to_value(hold(lead)).unwrap();
        wire["nodes"]["root"] =
            serde_json::to_value(BeatNode::sequence("Root", vec![id("lead"), id("a")])).unwrap();
        ProjectDocument::from_json(&wire.to_string()).unwrap()
    };
    let first = layout_at(1);
    let second = layout_at(2);
    let mut wire = serde_json::to_value(layout_at(4)).unwrap();
    wire["nodes"]["suffix"] = serde_json::to_value(partition("a", 2, 6)).unwrap();
    wire["nodes"]["root"] =
        serde_json::to_value(BeatNode::sequence("Root", vec![id("lead"), id("suffix")])).unwrap();
    let current = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let bindings = AudioBindingState::new(
        [&old, &first, &second]
            .into_iter()
            .enumerate()
            .map(|(ordinal, document)| AudioTimingRecord {
                id: historical_placement(u32::try_from(ordinal).unwrap())
                    .reference
                    .timing,
                layout: FrozenAudioLayout::capture(document).unwrap(),
            })
            .collect(),
        BTreeMap::from([(
            id("a"),
            OwnedAudioBinding {
                lattice: historical_placement(0),
                resume: Some(AudioResume {
                    local_boundary: ExactRatio::ZERO,
                    phase: AudioLocalPhase {
                        constant: ratio(64 * 5, 8008),
                        terms: vec![AudioPhaseTerm {
                            placement: historical_placement(0),
                            from_local: ExactRatio::ZERO,
                            to_local: ExactRatio::ONE,
                        }],
                    },
                }),
                reanchors: [(1, 2, 7), (2, 4, 8)]
                    .into_iter()
                    .map(|(ordinal, start, end)| AudioReanchorStep {
                        anchor: Default::default(),
                        placement: historical_placement(ordinal),
                        window: Some(
                            ExactFrameRange::new(
                                ExactRatio::integer(start),
                                ExactRatio::integer(end),
                            )
                            .unwrap(),
                        ),
                    })
                    .collect(),
            },
        )]),
    )
    .unwrap();
    let before = with_bindings(&current, &bindings);
    let instance = InstancePath {
        node: id("a"),
        repeats: vec![],
    };
    let resolved = before
        .audio_bindings()
        .resolve(&id("a"), &instance, 1000)
        .unwrap();
    // Independent boundaries: symbolic term B(1)-B(0)=1602, then
    // B(2)-B(1)=1601 and B(4)-B(3)=1601, plus 64 retained samples.
    // Collapsing the two chronological grids would yield one extra sample.
    assert_eq!(
        resolved.resume.as_ref().unwrap().local_boundary,
        ExactRatio::integer(2)
    );
    assert_eq!(
        resolved.resume.as_ref().unwrap().reference_local_delta,
        ratio(4868 * 5, 8008)
    );
    let oracle = source_oracle(&provider, 0..8197, ExactRatio::integer(4868), 256);
    assert_eq!(pcm(&before, &mut provider, 6406..6662, 193, false), oracle);
    assert_ne!(
        oracle,
        source_oracle(&provider, 0..8197, ExactRatio::integer(4869), 256)
    );
    let baseline = pcm(&before, &mut provider, 6406..12813, 251, false);
    let faded = pcm(&before, &mut provider, 6406..12813, 251, true);
    for prefix in [1, 3] {
        let patch = prefix_patch(&before, "suffix", prefix, before.audio_bindings());
        let after = patch.apply(&before).unwrap();
        let translated = after
            .audio_bindings()
            .resolve(&id("a"), &instance, 1000)
            .unwrap();
        assert_eq!(
            translated.resume.as_ref().unwrap().local_boundary,
            ExactRatio::integer(2 + prefix)
        );
        assert_eq!(
            translated.resume.as_ref().unwrap().reference_local_delta,
            resolved.resume.as_ref().unwrap().reference_local_delta
        );
        assert_eq!(
            after.audio_bindings().timings(),
            before.audio_bindings().timings()
        );
        for local in [0, 1, 2, 6] {
            assert_eq!(
                resolved
                    .lattice
                    .sample_boundary(ExactRatio::integer(local))
                    .unwrap(),
                translated
                    .lattice
                    .sample_boundary(ExactRatio::integer(local + prefix))
                    .unwrap()
            );
        }
        for chunk in [193, 239] {
            assert_eq!(
                pcm(&after, &mut provider, 6406..12813, chunk, false),
                baseline,
                "prefix {prefix}, chunk {chunk}"
            );
            assert_eq!(
                pcm(&after, &mut provider, 6406..12813, chunk, true),
                faded,
                "edge envelope: prefix {prefix}, chunk {chunk}"
            );
        }
        let restored = patch.inverse().apply(&after).unwrap();
        assert_eq!(restored, before);
        assert_eq!(
            pcm(&restored, &mut provider, 6406..12813, 193, false),
            baseline
        );
    }
}

#[path = "source_origin/endpoints.rs"]
mod endpoints;
