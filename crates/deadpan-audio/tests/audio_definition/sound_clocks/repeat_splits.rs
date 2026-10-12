//! Split whole retained Repeat inputs while keeping their concrete play clocks.
use super::*;

fn partial_inside_repeat(treated: bool, nested: bool) {
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
    let moved = insert_prefix(&ProjectDocument::from_json(&wire.to_string()).unwrap());
    let before = wrap(&moved, if nested { "outer" } else { "owner" }, "wrapped");
    assert_eq!(before.duration().unwrap().frames(), 13);
    let selection = SliceCaptureSelection::Range {
        range: FrameRange::new(ProjectFrame(4), ProjectFrame(6)).unwrap(),
    };
    let plan = before.repeat_selection(&id("root"), &selection, 2).unwrap();
    let repeated = edit(
        &before,
        "split-existing-repeat",
        Command::RepeatSelection {
            parent: id("root"),
            selection,
            plays: 2,
            identities: deadpan_core::RepeatSelectionIdentities {
                repeat: id("inner-repeat"),
                group: Some(id("partial-group")),
                split: SplitIdentities {
                    nodes: (0..plan.required_split_ids)
                        .map(|n| id(&format!("repeat-split-{n}")))
                        .collect(),
                },
            },
            timing: timing("split-existing-repeat"),
        },
    );
    assert_eq!(repeated.duration().unwrap().frames(), 15);
    let compare = |a: &[[f32; 2]], b: &[[f32; 2]], context: &str| {
        super::super::partial::compare_copy(a, b, treated, context)
    };
    for (old_start, old_end, new_start, new_end) in
        [(0, 4, 0, 4), (4, 6, 4, 6), (4, 6, 6, 8), (6, 13, 8, 15)]
    {
        compare(
            &render(&before, old_start, old_end),
            &render(&repeated, new_start, new_end),
            "partial selection inside existing Repeat",
        );
    }
    let selected = render(&before, 4, 6);
    assert!(selected.iter().flatten().any(|v| v.abs() > 0.001));
    let grown = edit(
        &repeated,
        "grown-inner",
        Command::SetRepeatPlays {
            node: id("inner-repeat"),
            plays: 3,
            timing: timing("grown-inner"),
        },
    );
    compare(
        &selected,
        &render(&grown, 4, 6),
        "retained first inner play",
    );
    compare(
        &selected,
        &render(&grown, 6, 8),
        "retained second inner play",
    );
    compare(
        &render(&without_clocks(&grown), 8, 10),
        &render(&grown, 8, 10),
        "later-born inner play",
    );
    let slice = capture_repeat_slice(&grown, "root", &id("inner-repeat"), "nested-capture");
    let end = grown.duration().unwrap().frames();
    let copied = paste_repeat_slice(
        &grown,
        "root",
        grown.children(&id("root")).count(),
        &slice,
        "nested-copy",
    );
    let copied_top = copied.children(&id("root")).last().unwrap().clone();
    let slice = capture_repeat_slice(&copied, "root", &copied_top, "nested-recapture");
    let twice = paste_repeat_slice(
        &copied,
        "root",
        copied.children(&id("root")).count(),
        &slice,
        "nested-recopy",
    );
    // Each play owns its origin-based sample rounding. Moving the complete
    // Repeat can change a play's allocation by one sample, not its input phase.
    for offset in [0, 2, 4] {
        let expected = render(&grown, 4 + offset, 6 + offset);
        compare(
            &expected,
            &render(&twice, end + offset, end + offset + 2),
            "copied split Repeat play",
        );
        compare(
            &expected,
            &render(&twice, end + 6 + offset, end + 8 + offset),
            "recopied split Repeat play",
        );
    }
    let deleted = edit(
        &twice,
        "delete-initial-repeat",
        Command::DeleteRipple {
            node: id("inner-repeat"),
            timing: timing("delete-initial-repeat"),
        },
    );
    for offset in [0, 2, 4] {
        compare(
            &render(&grown, 4 + offset, 6 + offset),
            &render(&deleted, end + offset, end + offset + 2),
            "recopy after original Repeat deletion",
        );
    }
    let NodeKind::Hold { recipe } = hold(1, HoldAudio::Silence).kind else {
        unreachable!()
    };
    let gapped = edit(
        &grown,
        "gap-inner",
        Command::SetRepeatGaps {
            node: id("inner-repeat"),
            gap: Some(recipe),
            branches: vec![],
            timing: timing("gap-inner"),
        },
    );
    for (old, new) in [(4, 4), (6, 7), (8, 10)] {
        // Current silent-gap envelopes are deliberate. Verify retained input
        // away from those edges, including the later-born play.
        compare(
            &render(&grown, old, old + 2)[512..1024],
            &render(&gapped, new, new + 2)[512..1024],
            "split Repeat gap input",
        );
    }
    for start in [6, 9] {
        assert!(
            render(&gapped, start, start + 1)
                .iter()
                .all(|frame| *frame == [0.0; 2])
        );
    }
}

#[test]
fn partial_existing_repeat_keeps_dry_sound_clocks() {
    partial_inside_repeat(false, false);
}

#[test]
fn partial_existing_repeat_keeps_equalized_sound_clocks() {
    partial_inside_repeat(true, false);
}

#[test]
fn partial_existing_repeat_keeps_nested_preserve_clocks() {
    partial_inside_repeat(false, true);
}

#[test]
fn partial_existing_repeat_keeps_equalized_nested_preserve_clocks() {
    partial_inside_repeat(true, true);
}
