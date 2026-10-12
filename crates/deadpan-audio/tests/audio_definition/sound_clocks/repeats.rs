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

fn partial_repeat(treated: bool, nested: bool) {
    let mut wire = serde_json::to_value(ntsc(AudioEdgePolicy::Hard)).unwrap();
    if nested {
        wire["nodes"]["inner"] =
            serde_json::to_value(retime("owner", 6, 0..4, PitchPolicy::Preserve)).unwrap();
        wire["nodes"]["outer"] =
            serde_json::to_value(retime("inner", 4, 0..6, PitchPolicy::Preserve)).unwrap();
        wire["nodes"]["root"]["kind"]["children"] = serde_json::json!(["lead", "outer", "tail"]);
    }
    if treated {
        let eq = EqualizerRecipe::new(
            [6000, -6000, 3000].map(|db| GainDb::new(db).unwrap()),
            None,
            None,
        )
        .unwrap();
        let treatment =
            serde_json::to_value(AudioTreatments::default().with_equalizer(Some(eq)).unwrap())
                .unwrap();
        wire["nodes"]["owner"]["audio_treatments"] = treatment.clone();
        wire["beat_sounds"]["owner"]["effect"]["treatments"] = treatment;
    }
    let before = insert_prefix(&ProjectDocument::from_json(&wire.to_string()).unwrap());
    let selection = SliceCaptureSelection::Range {
        range: FrameRange::new(ProjectFrame(4), ProjectFrame(6)).unwrap(),
    };
    let plan = before.repeat_selection(&id("root"), &selection, 2).unwrap();
    let repeated = edit(
        &before,
        "partial-repeat",
        Command::RepeatSelection {
            parent: id("root"),
            selection,
            plays: 2,
            identities: deadpan_core::RepeatSelectionIdentities {
                repeat: id("repeat"),
                group: Some(id("partial-group")),
                split: SplitIdentities {
                    nodes: (0..plan.required_split_ids)
                        .map(|n| id(&format!("partial-split-{n}")))
                        .collect(),
                },
            },
            timing: timing("partial-repeat"),
        },
    );
    assert_eq!(repeated.beat_sounds().len(), 3);
    let selected = render(&before, 4, 6);
    assert!(selected.iter().flatten().any(|v| v.abs() > 0.001));
    for (old_start, old_end, new_start, new_end) in
        [(3, 4, 3, 4), (4, 6, 4, 6), (4, 6, 6, 8), (6, 7, 8, 9)]
    {
        super::partial::compare_copy(
            &render(&before, old_start, old_end),
            &render(&repeated, new_start, new_end),
            treated,
            "partial Repeat initial fragments",
        );
    }
    let grown = count(&repeated, "grown-partial", 3);
    super::partial::compare_copy(
        &selected,
        &render(&grown, 4, 6),
        treated,
        "first retained play",
    );
    super::partial::compare_copy(
        &selected,
        &render(&grown, 6, 8),
        treated,
        "second retained play",
    );
    let born = render(&grown, 8, 10);
    super::partial::compare_copy(
        &render(&without_clocks(&grown), 8, 10),
        &born,
        treated,
        "new play clock",
    );
    if !treated {
        assert_ne!(born, selected);
    }
    let NodeKind::Hold { recipe: gap } = hold(1, HoldAudio::Silence).kind else {
        unreachable!()
    };
    let gapped = edit(
        &grown,
        "gapped-partial",
        Command::SetRepeatGaps {
            node: id("repeat"),
            gap: Some(gap),
            branches: vec![],
            timing: timing("gapped-partial"),
        },
    );
    for (start, end) in [(6, 7), (9, 10)] {
        assert!(
            render(&gapped, start, end)
                .iter()
                .all(|frame| *frame == [0.0; 2])
        );
    }
    // Current silent-gap fades are intentional; compare interior saved input.
    super::partial::compare_copy(
        &selected[512..1024],
        &render(&gapped, 7, 9)[512..1024],
        treated,
        "gapped saved play",
    );
    super::partial::compare_copy(
        &born[512..1024],
        &render(&gapped, 10, 12)[512..1024],
        treated,
        "gapped new play",
    );
    let read = |document: &ProjectDocument, frame: i64| {
        bus(
            &mut StageAudio::new(compile(document, false)),
            &mut ClockProvider::new(document),
            frame_sample_boundary(frame) + 512,
            256,
        )
        .samples
    };
    let slice = capture_repeat_slice(&gapped, "root", &id("repeat"), "partial-capture");
    let copied = paste_repeat_slice(&gapped, "root", 6, &slice, "partial-copy");
    let copied_top = copied.children(&id("root")).last().unwrap().clone();
    let slice = capture_repeat_slice(&copied, "root", &copied_top, "partial-recapture");
    let twice = paste_repeat_slice(&copied, "root", 7, &slice, "partial-recopy");
    for (old, new, again) in [(4, 15, 23), (7, 18, 26), (10, 21, 29)] {
        let expected = read(&gapped, old);
        super::partial::compare_copy(
            &expected,
            &read(&twice, new),
            treated,
            "partial Repeat copy",
        );
        super::partial::compare_copy(
            &expected,
            &read(&twice, again),
            treated,
            "partial Repeat recopy",
        );
    }
}

#[test]
fn partial_repeat_keeps_dry_sound_fragments_and_new_play_phase() {
    partial_repeat(false, false);
}

#[test]
fn partial_repeat_keeps_event_and_shared_equalizer_history() {
    partial_repeat(true, false);
}

#[test]
fn partial_repeat_keeps_complete_nested_preserve_context() {
    partial_repeat(false, true);
}

#[test]
fn partial_repeat_keeps_equalized_nested_preserve_context() {
    partial_repeat(true, true);
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
