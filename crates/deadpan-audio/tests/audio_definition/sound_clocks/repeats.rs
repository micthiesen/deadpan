use super::*;

fn timing(name: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: RevisionId::new(name).unwrap(),
        ordinal: 0,
    }
}

fn wrap(document: &ProjectDocument, child: &str, name: &str) -> ProjectDocument {
    edit(
        document,
        name,
        Command::RepeatSelection {
            parent: id("root"),
            selection: SliceCaptureSelection::Child { node: id(child) },
            plays: 2,
            identities: deadpan_core::RepeatSelectionIdentities {
                repeat: id("repeat"),
                group: None,
                split: SplitIdentities::default(),
            },
            timing: timing(name),
        },
    )
}

fn count(document: &ProjectDocument, name: &str, plays: u32) -> ProjectDocument {
    edit(
        document,
        name,
        Command::SetRepeatPlays {
            node: id("repeat"),
            plays,
            timing: timing(name),
        },
    )
}

fn render(document: &ProjectDocument, start_frame: i64, end_frame: i64) -> StereoPcm {
    let start = frame_sample_boundary(start_frame);
    let end = frame_sample_boundary(end_frame);
    let mut renderer = StageAudio::new(compile(document, false));
    let mut provider = ClockProvider::new(document);
    let mut result = Vec::new();
    let mut cursor = start;
    while cursor < end {
        let size = u32::try_from((end - cursor).min(1024)).unwrap();
        result.extend(bus(&mut renderer, &mut provider, cursor, size).samples);
        cursor += i64::from(size);
    }
    result
}

fn without_clocks(document: &ProjectDocument) -> ProjectDocument {
    let mut wire = serde_json::to_value(document).unwrap();
    wire["audio_bindings"] =
        serde_json::to_value(deadpan_core::AudioBindingState::default()).unwrap();
    ProjectDocument::from_json(&wire.to_string()).unwrap()
}

#[test]
fn repeat_sound_pcm_copies_initial_plays_and_allocates_later_births_at_their_own_phase() {
    let before = ntsc(AudioEdgePolicy::Hard);
    let original = render(&before, 2, 6);
    let wrapped = wrap(&before, "owner", "wrapped");
    assert_eq!(render(&wrapped, 2, 6), original);
    assert_eq!(render(&wrapped, 6, 10), original[..6406]);
    let grown = count(&wrapped, "grown", 3);
    assert_eq!(render(&grown, 2, 6), original);
    assert_eq!(render(&grown, 6, 10), original[..6406]);
    let born = render(&grown, 10, 14);
    assert_eq!(born, render(&without_clocks(&grown), 10, 14));
    assert_ne!(born, original[..6406]);

    let NodeKind::Hold { recipe: gap } = hold(1, HoldAudio::Silence).kind else {
        panic!()
    };
    let gapped = edit(
        &grown,
        "gapped",
        Command::SetRepeatGaps {
            node: id("repeat"),
            gap: Some(gap),
            branches: vec![],
            timing: timing("gapped"),
        },
    );
    assert_eq!(render(&gapped, 6, 7), vec![[0.0; 2]; 1601]);
    assert_eq!(render(&gapped, 11, 12), vec![[0.0; 2]; 1601]);
    let moved_second = render(&gapped, 7, 11);
    let moved_third = render(&gapped, 12, 16);
    // Current silent gap envelopes are independently re-evaluated; the retained
    // interior PCM keeps each occurrence's own pre-gap recipe context.
    assert_eq!(&moved_second[128..6200], &original[128..6200]);
    assert_eq!(&moved_third[128..6200], &born[128..6200]);
    assert_eq!(moved_second[6406], [0.0; 2]);
    assert_eq!(moved_third[6406], [0.0; 2]);
    let start = frame_sample_boundary(12);
    for (offset, size) in [(0, 128), (1550, 200), (6300, 107)] {
        let actual = bus(
            &mut StageAudio::new(compile(&gapped, false)),
            &mut ClockProvider::new(&gapped),
            start + offset,
            size,
        );
        assert_eq!(
            actual.samples,
            moved_third[offset as usize..offset as usize + size as usize]
        );
    }
}

#[test]
fn repeat_sound_isolation_copy_and_recopy_keep_independent_sample_offset() {
    let mut wire = serde_json::to_value(ntsc(AudioEdgePolicy::Hard)).unwrap();
    wire["beat_sounds"]["owner"]["effect"]["offset"] = serde_json::json!(37);
    // Leave enough owner time for the independently offset complete recipe.
    let recipe = voice_recipe(FrameRate::new(30_000, 1001).unwrap(), 4800);
    wire["beat_sounds"]["owner"]["effect"]["source"] = serde_json::to_value(recipe.source).unwrap();
    wire["beat_sounds"]["owner"]["effect"]["mapping"] =
        serde_json::to_value(recipe.mapping).unwrap();
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let wrapped = wrap(&before, "owner", "wrapped-offset");
    let expected = render(&wrapped, 2, 6);
    assert!(expected.iter().any(|sample| *sample != [0.0; 2]));
    let isolated = edit(
        &wrapped,
        "isolated",
        Command::KeepFirstPlayAttachments {
            node: id("repeat"),
            identities: OccurrenceIdentities {
                nodes: vec![id("isolated-owner")],
                marks: vec![],
            },
        },
    );
    assert_eq!(render(&isolated, 2, 6), expected);
    assert_eq!(render(&isolated, 6, 10), vec![[0.0; 2]; 6406]);
    let slice = capture_repeat_slice(&isolated, "root", &id("repeat"), "capture-first");
    let copied = paste_repeat_slice(&isolated, "root", 3, &slice, "copy-first");
    let copied_repeat = copied.children(&id("root")).nth(3).unwrap().clone();
    let actual = render(&copied, 12, 16);
    assert_eq!(actual, expected);
    assert_eq!(render(&copied, 16, 20), vec![[0.0; 2]; 6406]);
    let slice = capture_repeat_slice(&copied, "root", &copied_repeat, "capture-second");
    let recopied = paste_repeat_slice(&copied, "root", 4, &slice, "copy-second");
    assert_eq!(render(&recopied, 20, 24), expected[..6406]);
    assert_eq!(render(&recopied, 24, 28), vec![[0.0; 2]; 6407]);
    for events in recopied.beat_sounds().values() {
        assert_eq!(events[&sound_id("effect")].offset, AudioSample(37));
    }
}

#[test]
fn repeat_sound_wrap_retains_complete_nested_preserve_recipe_context() {
    let mut wire = serde_json::to_value(ntsc(AudioEdgePolicy::Hard)).unwrap();
    wire["nodes"]["inner"] =
        serde_json::to_value(retime("owner", 6, 0..4, PitchPolicy::Preserve)).unwrap();
    wire["nodes"]["outer"] =
        serde_json::to_value(retime("inner", 4, 0..6, PitchPolicy::Preserve)).unwrap();
    wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["lead", "outer", "tail"]);
    let before = ProjectDocument::from_json(&wire.to_string()).unwrap();
    let expected = render(&before, 2, 6);
    let wrapped = wrap(&before, "outer", "wrapped-preserve");
    assert_eq!(render(&wrapped, 2, 6), expected);
    assert_eq!(render(&wrapped, 6, 10), expected[..6406]);
    let copied_slice = capture_repeat_slice(&wrapped, "root", &id("repeat"), "capture-preserve");
    let copied = paste_repeat_slice(&wrapped, "root", 3, &copied_slice, "copy-preserve");
    assert_eq!(render(&copied, 12, 16), expected);
    assert_eq!(render(&copied, 16, 20), expected[..6406]);
}
