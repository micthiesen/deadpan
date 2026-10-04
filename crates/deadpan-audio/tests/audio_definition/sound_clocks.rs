//! Exercise command-created clocks through the ordinary authored audio bus.
use super::*;

struct ClockProvider {
    fixture: FixtureProvider,
    revision: RevisionId,
}

impl ClockProvider {
    fn new(document: &ProjectDocument) -> Self {
        Self {
            fixture: FixtureProvider::new(),
            revision: document.revision_id().clone(),
        }
    }
}

impl AudioSourceProvider for ClockProvider {
    fn source(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        assert_eq!(revision, &self.revision);
        self.fixture.source(
            project,
            &RevisionId::new("definition-revision").unwrap(),
            asset,
            cancelled,
        )
    }

    fn source_for_context(
        &mut self,
        project: &ProjectId,
        revision: &RevisionId,
        asset: &AssetId,
        expected: &AssetRecord,
        cancelled: &AtomicBool,
    ) -> Result<&PreparedSource, PreparationError> {
        assert_eq!(revision, &self.revision);
        assert_eq!(
            expected.source_qualification.as_ref().unwrap().as_str(),
            "c".repeat(64)
        );
        self.fixture.source_for_context(
            project,
            &RevisionId::new("definition-revision").unwrap(),
            asset,
            expected,
            cancelled,
        )
    }
}

fn edit(document: &ProjectDocument, revision: &str, command: Command) -> ProjectDocument {
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(revision).unwrap(),
        command,
    };
    let edit = apply(document, &request).unwrap();
    let after = edit.forward.apply(document).unwrap();
    assert_eq!(edit.inverse.apply(&after).unwrap(), *document);
    ProjectDocument::from_json(&after.to_json().unwrap()).unwrap()
}

fn insert_prefix(document: &ProjectDocument) -> ProjectDocument {
    let NodeKind::Hold { recipe } = hold(1, HoldAudio::Silence).kind else {
        unreachable!()
    };
    edit(
        document,
        "moved",
        Command::InsertTime {
            at: ProjectFrame(0),
            hold: recipe,
            id: id("inserted"),
            identities: SplitIdentities::default(),
            timing: AudioTimingId {
                allocation: RevisionId::new("moved").unwrap(),
                ordinal: 0,
            },
        },
    )
}

fn remove_prefix(document: &ProjectDocument) -> ProjectDocument {
    edit(
        document,
        "returned",
        Command::DeleteRipple {
            node: id("inserted"),
            timing: AudioTimingId {
                allocation: RevisionId::new("returned").unwrap(),
                ordinal: 0,
            },
        },
    )
}

fn ntsc(edge: AudioEdgePolicy) -> ProjectDocument {
    let rate = FrameRate::new(30_000, 1001).unwrap();
    let base = document(
        rate,
        &["lead", "owner", "tail"],
        [
            ("lead", hold(2, HoldAudio::Silence)),
            ("owner", picture_only(rate, 4)),
            // Keep the event's historical edge separate from current Hold gates.
            ("tail", picture_only(rate, 2)),
        ],
        BTreeMap::new(),
    );
    let mut wire = serde_json::to_value(base).unwrap();
    wire["assets"]["media"]["source_qualification"] = serde_json::json!("c".repeat(64));
    let mut recipe = voice_recipe(rate, 6408);
    recipe.mapping = SourceAudioMapping::SelectedPlacement {
        start: ExactRatio::ZERO,
        frames: recipe.mapping.duration_frames(FrameDuration::ZERO).unwrap(),
        selection: ExactFrameRange {
            start: ExactRatio::ZERO,
            end: ExactRatio::integer(4),
        },
    };
    wire["beat_sounds"] = serde_json::json!({"owner": {"effect": BeatSound {
        label: "Effect".into(), source: recipe.source, mapping: recipe.mapping, offset: recipe.offset,
        gain_millidecibels: 0, start_edge: edge, end_edge: edge, overflow: SoundOverflowPolicy::Reject,
    }}});
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn command_created_clocks_keep_ntsc_pcm_and_edge_progress_through_move_and_return() {
    for edge in [AudioEdgePolicy::Hard, AudioEdgePolicy::Automatic] {
        let before = ntsc(edge);
        let original = bus(
            &mut StageAudio::new(compile(&before, false)),
            &mut FixtureProvider::new(),
            3203,
            6407,
        );
        assert_ne!(original.samples[6406], [0.0; 2]);
        let moved = insert_prefix(&before);
        let mut provider = ClockProvider::new(&moved);
        let mut renderer = StageAudio::new(compile(&moved, false));
        let actual = bus(&mut renderer, &mut provider, 4805, 6406);
        assert_eq!(actual.samples.len(), 6406);
        for (index, (actual, expected)) in actual.samples.iter().zip(&original.samples).enumerate()
        {
            assert_eq!(actual, expected, "{edge:?} moved sample {index}");
        }
        assert!(provider.fixture.context_calls > 0);

        let returned = remove_prefix(&moved);
        let mut expected = original.samples;
        expected[6406] = [0.0; 2];
        let plan = compile(&returned, false);
        let mut provider = ClockProvider::new(&returned);
        let full = bus(
            &mut StageAudio::new(plan.clone()),
            &mut provider,
            3203,
            6407,
        );
        assert_eq!(full.samples, expected);
        // The picture-only Original imposes no Hold mask. Bus suppression is
        // the intersection across contributions, not this event's route gaps.
        assert!(full.suppressed.is_empty());
        for (offset, count) in [(6300, 107), (0, 256), (1590, 37), (3200, 201)] {
            let mut cold = StageAudio::new(plan.clone());
            assert_eq!(
                bus(&mut cold, &mut provider, 3203 + offset, count).samples,
                expected[offset as usize..offset as usize + count as usize]
            );
        }
    }
}

#[test]
fn sound_clock_current_coincident_hold_owns_the_automatic_fade_boundary() {
    let before = ntsc(AudioEdgePolicy::Hard);
    let raw = bus(
        &mut StageAudio::new(compile(&before, false)),
        &mut FixtureProvider::new(),
        3203,
        6407,
    );
    let mut wire = serde_json::to_value(ntsc(AudioEdgePolicy::Automatic)).unwrap();
    wire["nodes"]["tail"] = serde_json::to_value(hold(2, HoldAudio::Silence)).unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let moved = insert_prefix(&before);
    let mut provider = ClockProvider::new(&moved);
    let mut renderer = StageAudio::new(compile(&moved, false));
    let actual = bus(&mut renderer, &mut provider, 4805 + 6310, 96);
    for (index, sample) in actual.samples.iter().enumerate() {
        // The current Hold starts at B(7)=11211. The old virtual edge moved
        // to 11212; exact coincidence combines policy but the current Hold
        // owns its physical label, so its final gain is 1/192, not 3/192.
        let gain = (2 * (95 - index) + 1) as f32 / 192.0;
        assert_eq!(
            *sample,
            raw.samples[6310 + index].map(|value| value * gain),
            "current Hold ramp sample {index}"
        );
    }
}

#[test]
fn saved_clocks_keep_complete_nested_preserve_while_current_gain_and_source_admission_stay_live() {
    let before = saved(&occurrence_document(), "owner", 3840);
    let moved = insert_prefix(&before);
    let plan = compile(&moved, false);
    let mut provider = ClockProvider::new(&moved);
    let mut renderer = StageAudio::new(plan.clone());
    let passage = stretch(&stretch(&raw_voice(3840), 2560, 3, 2), 1280, 2, 1);
    let expected: Vec<_> = passage
        .iter()
        .copied()
        .chain(passage.iter().copied())
        .collect();
    assert_eq!(
        bus(&mut renderer, &mut provider, 18, 2560).samples,
        expected
    );
    assert!(provider.fixture.context_calls > 0);
    for (offset, count) in [(2440, 120), (0, 128), (1240, 128)] {
        let mut cold = StageAudio::new(plan.clone());
        assert_eq!(
            bus(&mut cold, &mut provider, 18 + offset, count).samples,
            expected[offset as usize..offset as usize + count as usize]
        );
    }
    provider.fixture.unavailable = true;
    assert!(matches!(
        renderer.prepare_authored_bus(
            &mut provider,
            AudioSample(18),
            128,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Preparation(
            PreparationError::SourceUnavailable(_)
        ))
    ));

    // Owner gain is applied after the retained raw processing graph.
    let mut wire = serde_json::to_value(&moved).unwrap();
    wire["nodes"]["owner"]["audio_treatments"] = serde_json::to_value(trim(
        0,
        vec![GainRange::new(ExactRatio::ZERO, ExactRatio::integer(1920)).unwrap()],
    ))
    .unwrap();
    let muted = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut expected = expected;
    expected[..640].fill([0.0; 2]);
    expected[1280..1920].fill([0.0; 2]);
    assert_eq!(
        bus(
            &mut StageAudio::new(compile(&muted, false)),
            &mut ClockProvider::new(&muted),
            18,
            2560
        )
        .samples,
        expected
    );
}

#[test]
fn sound_clocks_translate_processed_ntsc_extent_through_nested_preserve() {
    let mut wire = serde_json::to_value(ntsc(AudioEdgePolicy::Hard)).unwrap();
    wire["nodes"]["inner"] =
        serde_json::to_value(retime("owner", 6, 0..4, PitchPolicy::Preserve)).unwrap();
    wire["nodes"]["outer"] =
        serde_json::to_value(retime("inner", 4, 0..6, PitchPolicy::Preserve)).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["lead", "outer", "tail"]);
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let mut provider = ClockProvider::new(&before);
    let original = bus(
        &mut StageAudio::new(compile(&before, false)),
        &mut provider,
        3203,
        6407,
    );
    assert!(original.samples.iter().any(|sample| *sample != [0.0; 2]));
    let moved = insert_prefix(&before);
    provider.revision = moved.revision_id().clone();
    assert_eq!(
        bus(
            &mut StageAudio::new(compile(&moved, false)),
            &mut provider,
            4805,
            6406
        )
        .samples,
        original.samples[..6406]
    );
    let returned = remove_prefix(&moved);
    provider.revision = returned.revision_id().clone();
    let plan = compile(&returned, false);
    let mut expected = original.samples;
    expected[6406] = [0.0; 2];
    let full = bus(
        &mut StageAudio::new(plan.clone()),
        &mut provider,
        3203,
        6407,
    );
    assert_eq!(full.samples, expected);
    assert!(full.suppressed.is_empty());
    for (offset, count) in [(6300, 107), (0, 128), (1601, 128), (3180, 97)] {
        assert_eq!(
            bus(
                &mut StageAudio::new(plan.clone()),
                &mut provider,
                3203 + offset,
                count
            )
            .samples,
            expected[offset as usize..offset as usize + count as usize]
        );
    }
}

#[test]
fn retained_sound_and_current_bus_share_the_pcm_residency_cap() {
    let moved = insert_prefix(&ntsc(AudioEdgePolicy::Hard));
    let mut renderer = StageAudio::with_limits(
        compile(&moved, false),
        StageLimits {
            maximum_resident_frames: 1280,
            ..Default::default()
        },
    )
    .unwrap();
    let mut provider = ClockProvider::new(&moved);
    assert!(matches!(
        renderer.prepare_authored_bus(
            &mut provider,
            AudioSample(4805),
            256,
            TIMEOUT,
            &AtomicBool::new(false)
        ),
        Err(StageAudioError::Limit(_))
    ));
    assert_eq!(provider.fixture.calls, 0);
    // A failed reservation leaves the current plan and controller usable.
    assert_eq!(renderer.plan().metadata().revision_id, *moved.revision_id());
    assert_eq!(
        bus(&mut renderer, &mut provider, 4805, 32).samples.len(),
        32
    );
}
