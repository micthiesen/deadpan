//! Actual structural edits and independently specified Follow owner clocks.
use super::*;

fn timing(name: &str) -> AudioTimingId {
    AudioTimingId {
        allocation: revision(name),
        ordinal: 0,
    }
}
fn changed(before: &ProjectDocument, name: &str, command: Command) -> ProjectDocument {
    let request = CommandRequest {
        project_id: before.project_id().clone(),
        expected_revision: before.revision_id().clone(),
        new_revision: revision(name),
        command,
    };
    let raw = serde_json::to_string(&request).unwrap();
    let request: CommandRequest = serde_json::from_str(&raw).unwrap();
    let edit = apply(before, &request).unwrap();
    let after = edit.forward.apply(before).unwrap();
    assert_eq!(edit.inverse.apply(&after).unwrap(), *before);
    ProjectDocument::from_json(&after.to_json().unwrap()).unwrap()
}
fn posed(sample: &deadpan_plan::PictureSample) -> Vec<FramingPose> {
    sample
        .framing
        .iter()
        .filter_map(|layer| layer.pose)
        .collect()
}
fn same_frame(before: &RenderPlan, old: i64, after: &RenderPlan, new: i64) {
    let a = before.picture(ProjectFrame(old)).unwrap();
    let b = after.picture(ProjectFrame(new)).unwrap();
    assert_eq!(a.picture, b.picture);
    assert_eq!(a.picture_context, b.picture_context);
    assert_eq!(posed(&a), posed(&b));
}
fn paste_ids(slice: &CapturedEditSlice, name: &str) -> SlicePasteIdentities {
    let need = slice.identity_requirements().unwrap();
    SlicePasteIdentities {
        authored: OccurrenceIdentities {
            nodes: (0..need.nodes)
                .map(|n| id(&format!("{name}-n{n}")))
                .collect(),
            marks: (0..need.marks)
                .map(|n| MarkId::new(format!("{name}-m{n}")).unwrap())
                .collect(),
        },
        aliases: (0..need.aliases)
            .map(|n| id(&format!("{name}-a{n}")))
            .collect(),
    }
}
fn follow_owner(
    before: ProjectDocument,
    owner: &str,
    curve: framing_zoom::ZoomCurve,
) -> ProjectDocument {
    let mut value = serde_json::to_value(before).unwrap();
    value["nodes"][owner]["framing"]["value"]["scale_transition"] =
        serde_json::to_value(FollowScaleTransition {
            factor: ratio(2, 1),
            curve,
        })
        .unwrap();
    ProjectDocument::from_json(&value.to_string()).unwrap()
}

#[test]
fn raw_follow_curves_reach_live_and_fallback_plan_poses() {
    for (curve, live, fallback) in [
        (framing_zoom::ZoomCurve::Step, ratio(2, 1), ratio(1, 1)),
        (framing_zoom::ZoomCurve::Linear, ratio(9, 4), ratio(13, 8)),
        (
            framing_zoom::ZoomCurve::Smoothstep,
            ratio(267, 128),
            ratio(431, 256),
        ),
    ] {
        let before = with_target(
            document(&["a"], vec![("a", source(4, 0, 4004))]),
            2002,
            &[("a", 2)],
        );
        let before = follow_owner(before, "a", curve);
        let plan = RenderPlan::compile(&before).unwrap();
        assert_eq!(
            pose(&plan, 0, 0).scale,
            live,
            "live at owner 1/2 of four frames"
        );
        assert_eq!(pose(&plan, 0, 0).center_x, grid(1, 5));
        assert_eq!(
            pose(&plan, 2, 0).scale,
            fallback,
            "fallback at owner 5/2 of four frames"
        );
        assert_eq!(pose(&plan, 2, 0).center_x, ratio(1, 2));
    }
}

#[test]
fn nested_raw_follows_use_distinct_source_repeat_and_group_clocks_including_gap() {
    let before = with_target(
        document(
            &["g"],
            vec![
                ("g", BeatNode::sequence("Group", vec![id("lead"), id("r")])),
                ("lead", hold(2)),
                ("r", repeat("a", 2, 2, "plays")),
                ("a", source(2, 0, 2002)),
            ],
        ),
        2002,
        &[("a", 1), ("r", 1), ("g", 2)],
    );
    let before = ["a", "r", "g"].into_iter().fold(before, |doc, owner| {
        follow_owner(doc, owner, framing_zoom::ZoomCurve::Linear)
    });
    let plan = RenderPlan::compile(&before).unwrap();
    let picture = plan.picture(ProjectFrame(2)).unwrap();
    let layer = |owner: &str| {
        picture
            .framing
            .iter()
            .find(|layer| layer.instance.node == id(owner))
            .unwrap()
    };
    assert_eq!(layer("a").local_position, ratio(1, 2));
    assert_eq!(layer("a").effective_duration, ratio(2, 1));
    assert_eq!(layer("a").pose.unwrap().scale, ratio(5, 4));
    assert_eq!(layer("r").local_position, ratio(1, 2));
    assert_eq!(layer("r").effective_duration, ratio(6, 1));
    assert_eq!(layer("r").pose.unwrap().scale, grid(13, 12));
    assert_eq!(layer("g").local_position, ratio(5, 2));
    assert_eq!(layer("g").effective_duration, ratio(8, 1));
    assert_eq!(layer("g").pose.unwrap().scale, ratio(21, 8));
    assert_eq!(ticks(&picture.picture), ratio(1001, 2));
    let gap = plan.picture(ProjectFrame(4)).unwrap();
    assert_eq!(gap.picture, Picture::Background);
    assert!(gap.gap_after.is_some());
    let gap_repeat = gap
        .framing
        .iter()
        .find(|layer| layer.instance.node == id("r"))
        .unwrap();
    assert_eq!(gap_repeat.local_position, ratio(5, 2));
    assert_eq!(gap_repeat.pose.unwrap().scale, grid(17, 12));
    let gap_group = gap
        .framing
        .iter()
        .find(|layer| layer.instance.node == id("g"))
        .unwrap();
    assert_eq!(gap_group.pose.unwrap().scale, ratio(25, 16));
    let second = plan.picture(ProjectFrame(6)).unwrap();
    assert_eq!(ticks(&second.picture), ratio(1001, 2));
    assert_eq!(
        second
            .framing
            .iter()
            .find(|layer| layer.instance.node == id("r"))
            .unwrap()
            .pose
            .unwrap()
            .scale,
        ratio(7, 4)
    );
}

#[test]
fn captured_hold_follow_animates_fallback_and_keeps_geometry_after_provider_change() {
    let mut recipe = background(4);
    recipe.video = HoldVideo::Freeze {
        asset: asset_id("video"),
        timestamp: SourceTimestamp {
            ticks: 1001,
            time_base: clock(),
        },
    };
    let captured = retained_picture_context();
    recipe.picture_context = Some(captured.clone());
    let before = follow_owner(
        with_target(
            document(&["h"], vec![("h", node(NodeKind::Hold { recipe }))]),
            8008,
            &[("h", 2)],
        ),
        "h",
        framing_zoom::ZoomCurve::Linear,
    );
    let plan = RenderPlan::compile(&before).unwrap();
    for frame in 0..4 {
        let picture = plan.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(picture.picture_context.as_deref(), Some(&captured));
        assert_eq!(
            pose(&plan, frame, 0).center_x,
            ratio(1, 2),
            "capture suppresses live center"
        );
        assert_eq!(
            pose(&plan, frame, 0).scale,
            ratio(9 + 2 * i128::from(frame), 8)
        );
    }
    let after = changed(
        &before,
        "hold-provider",
        Command::SetHoldProvider {
            node: id("h"),
            video: HoldVideo::Background,
        },
    );
    let next = RenderPlan::compile(&after).unwrap();
    assert_eq!(next.duration(), plan.duration());
    for frame in 0..4 {
        let picture = next.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(picture.picture, Picture::Background);
        assert_eq!(picture.picture_context.as_deref(), Some(&captured));
        assert_eq!(
            posed(&picture),
            posed(&plan.picture(ProjectFrame(frame)).unwrap())
        );
    }
    assert_eq!(
        next.audio_duration().unwrap(),
        plan.audio_duration().unwrap()
    );
}

#[test]
fn cropped_copy_and_duplicate_retain_original_follow_phase_and_target() {
    let before = animated(
        &with_target(
            document(&["a"], vec![("a", source(8, 0, 8008))]),
            8008,
            &[("a", 2)],
        ),
        "a",
    );
    let slice =
        CapturedEditSlice::capture(&before, &id("root"), range(2, 6), timing("capture-follow"))
            .unwrap();
    slice.validate_capture(&before).unwrap();
    let after = changed(
        &before,
        "paste-follow",
        Command::SpliceSlice {
            parent: id("root"),
            index: 1,
            identities: paste_ids(&slice, "paste-follow"),
            timing: timing("paste-follow"),
            slice,
        },
    );
    let old = RenderPlan::compile(&before).unwrap();
    let pasted = RenderPlan::compile(&after).unwrap();
    for frame in 0..8 {
        same_frame(&old, frame, &pasted, frame);
    }
    for frame in 0..4 {
        same_frame(&old, frame + 2, &pasted, frame + 8);
    }
    let full = CapturedEditSlice::capture_selection(
        &before,
        &id("root"),
        &SliceCaptureSelection::Child { node: id("a") },
        timing("duplicate-capture"),
    )
    .unwrap();
    let doubled = changed(
        &before,
        "duplicate-follow",
        Command::Duplicate {
            parent: id("root"),
            selection: SliceCaptureSelection::Child { node: id("a") },
            identities: paste_ids(&full, "duplicate-follow"),
            split_identities: SplitIdentities::default(),
            timing: timing("duplicate-follow"),
        },
    );
    let doubled = RenderPlan::compile(&doubled).unwrap();
    for frame in 0..8 {
        same_frame(&old, frame, &doubled, frame + 8);
    }
}

fn qualified_pair() -> ProjectDocument {
    let mut left = source(8, -4004, 20020);
    let mut right = left.clone();
    for (node, start) in [(&mut left, -4), (&mut right, -12)] {
        let NodeKind::Source { source } = &mut node.kind else {
            unreachable!()
        };
        source.edit_window = Some(SourceEditWindow::new(ExactRatio::ZERO, ratio(8, 1)).unwrap());
        source.video_mapping = SourceVideoMapping::SelectedPlacement {
            start: ratio(start, 1),
            frames: ratio(24, 1),
            selection: ExactFrameRange::new(ExactRatio::ZERO, ratio(8, 1)).unwrap(),
            endpoints: EndpointPolicy::HoldAdjacent,
        };
        source.audio = Some(SourceAudio {
            asset: asset_id("video"),
            span: span(-4004, 20020),
        });
        source.link = LinkRelation::Linked;
        source.audio_mapping = SourceAudioMapping::SelectedPlacement {
            start: ratio(start, 1),
            frames: ratio(24, 1),
            selection: ExactFrameRange::new(ExactRatio::ZERO, ratio(8, 1)).unwrap(),
        };
    }
    let before = with_target(
        document(&["left", "right"], vec![("left", left), ("right", right)]),
        20020,
        &[("left", 2), ("right", 2)],
    );
    let before = ["left", "right"].into_iter().fold(before, |doc, owner| {
        follow_owner(doc, owner, framing_zoom::ZoomCurve::Linear)
    });
    let mut value = serde_json::to_value(&before).unwrap();
    value["assets"]["video"]["video"] = serde_json::to_value(span(-4004, 20020)).unwrap();
    value["assets"]["video"]["audio"] = serde_json::to_value(span(-4004, 20020)).unwrap();
    value["assets"]["video"]["source_qualification"] =
        serde_json::to_value(SourceQualificationId::new("b".repeat(64)).unwrap()).unwrap();
    value["assets"]["video"]["frame_count"] = serde_json::to_value(duration(24)).unwrap();
    ProjectDocument::from_json(&value.to_string()).unwrap()
}

fn same_source_frame(
    before: &RenderPlan,
    old: i64,
    after: &RenderPlan,
    new: i64,
    expected_selection: (i64, i64),
) {
    let a = before.picture(ProjectFrame(old)).unwrap();
    let b = after.picture(ProjectFrame(new)).unwrap();
    let (
        Picture::Source {
            asset: a_asset,
            point: a_point,
            span: a_span,
            endpoints: a_policy,
            ..
        },
        Picture::Source {
            asset: b_asset,
            point: b_point,
            span: b_span,
            selection,
            endpoints: b_policy,
        },
    ) = (&a.picture, &b.picture)
    else {
        panic!("expected source pictures")
    };
    assert_eq!(
        (a_asset, a_point, a_span, a_policy),
        (b_asset, b_point, b_span, b_policy)
    );
    // The qualified pair is one contiguous Original: global old frame f
    // independently samples Original tick 1001*(f+1/2).
    assert_eq!(b_point.ticks, ratio(1001 * (2 * i128::from(old) + 1), 2));
    assert_eq!(b_point.time_base, clock());
    assert_eq!(
        selection.start(),
        SourcePoint {
            ticks: ratio(i128::from(expected_selection.0), 1),
            time_base: clock(),
        }
    );
    assert_eq!(
        selection.end(),
        SourcePoint {
            ticks: ratio(i128::from(expected_selection.1), 1),
            time_base: clock(),
        }
    );
    assert_eq!(a.picture_context, b.picture_context);
    assert_eq!(posed(&a), posed(&b));
}

fn audio_point(plan: &RenderPlan, frame: i64) -> SourcePoint {
    let at = ratio(2 * i128::from(frame) + 1, 2);
    let query = plan
        .audio(
            AudioSample(0)..plan.audio_duration().unwrap(),
            deadpan_plan::AudioQueryLimits::default(),
        )
        .unwrap();
    query
        .spans
        .into_iter()
        .find(|span| {
            span.project_extent.start.compare(at).is_le()
                && span.project_extent.end.compare(at).is_gt()
                && matches!(span.content, deadpan_plan::AudioContent::Source { .. })
        })
        .expect("linked Source audio")
        .source_point_at_project_frame(at)
        .unwrap()
}

#[test]
fn source_trim_roll_and_slip_keep_follow_phase_on_surviving_material() {
    let before = qualified_pair();
    let old = RenderPlan::compile(&before).unwrap();
    let resolved = before
        .source_trim(
            &id("root"),
            &id("left"),
            SourceTrimEdge::In,
            2,
            SourceTrimMode::Ripple,
        )
        .unwrap();
    assert_eq!(resolved.applied_delta_frames, 2);
    let trimmed = changed(
        &before,
        "trim-follow",
        Command::TrimSource {
            parent: id("root"),
            node: id("left"),
            edge: SourceTrimEdge::In,
            delta_frames: 2,
            mode: SourceTrimMode::Ripple,
            wrapper: resolved.needs_wrapper.then(|| id("trim-crop")),
            timing: timing("trim-follow"),
        },
    );
    let trimmed = RenderPlan::compile(&trimmed).unwrap();
    assert_eq!(trimmed.duration(), duration(14));
    for frame in 0..14 {
        same_source_frame(
            &old,
            frame + 2,
            &trimmed,
            frame,
            if frame < 6 { (0, 8008) } else { (8008, 16016) },
        );
        assert_eq!(audio_point(&old, frame + 2), audio_point(&trimmed, frame));
    }

    let resolved = before
        .source_trim(
            &id("root"),
            &id("left"),
            SourceTrimEdge::Out,
            2,
            SourceTrimMode::Ripple,
        )
        .unwrap();
    assert_eq!(resolved.applied_delta_frames, 2);
    let grown = changed(
        &before,
        "grow-follow",
        Command::TrimSource {
            parent: id("root"),
            node: id("left"),
            edge: SourceTrimEdge::Out,
            delta_frames: 2,
            mode: SourceTrimMode::Ripple,
            wrapper: resolved.needs_wrapper.then(|| id("grow-crop")),
            timing: timing("grow-follow"),
        },
    );
    let grown = RenderPlan::compile(&grown).unwrap();
    for frame in 0..8 {
        same_source_frame(&old, frame, &grown, frame, (0, 10010));
    }
    for frame in 8..10 {
        assert_eq!(pose(&grown, frame, 0).scale, ratio(4, 1));
    }
    for frame in 8..16 {
        same_source_frame(&old, frame, &grown, frame + 2, (8008, 16016));
    }

    let resolved = before
        .source_roll(&id("root"), &id("left"), &id("right"), 2)
        .unwrap();
    assert_eq!(resolved.applied_delta_frames, 2);
    let rolled = changed(
        &before,
        "roll-follow",
        Command::RollSources {
            parent: id("root"),
            left: id("left"),
            right: id("right"),
            delta_frames: 2,
            left_wrapper: resolved.left.needs_wrapper.then(|| id("roll-left-crop")),
            right_wrapper: resolved.right.needs_wrapper.then(|| id("roll-right-crop")),
            timing: timing("roll-follow"),
        },
    );
    let rolled = RenderPlan::compile(&rolled).unwrap();
    assert_eq!(rolled.duration(), old.duration());
    for frame in (0..8).chain(10..16) {
        same_source_frame(
            &old,
            frame,
            &rolled,
            frame,
            if frame < 8 { (0, 10010) } else { (8008, 16016) },
        );
        assert_eq!(audio_point(&old, frame), audio_point(&rolled, frame));
    }

    let slipped = changed(
        &before,
        "slip-follow",
        Command::SlipSource {
            parent: id("root"),
            node: id("left"),
            delta_frames: 1,
        },
    );
    assert_eq!(
        slipped.nodes()[&id("left")].framing,
        before.nodes()[&id("left")].framing
    );
    let slipped = RenderPlan::compile(&slipped).unwrap();
    assert_eq!(slipped.duration(), old.duration());
    for frame in 0..8 {
        let a = old.picture(ProjectFrame(frame)).unwrap();
        let b = slipped.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(
            ticks(&b.picture),
            ticks(&a.picture).checked_add(ratio(1001, 1)).unwrap()
        );
        assert_eq!(
            a.framing[0].pose.unwrap().scale,
            b.framing[0].pose.unwrap().scale
        );
        let old_audio = audio_point(&old, frame);
        let new_audio = audio_point(&slipped, frame);
        assert_eq!(new_audio.time_base, old_audio.time_base);
        assert_eq!(
            new_audio.ticks,
            old_audio.ticks.checked_add(ratio(1001, 1)).unwrap()
        );
    }
    assert_eq!(pose(&old, 4, 0).center_x, grid(1, 5));
    assert_eq!(pose(&slipped, 4, 0).center_x, grid(4, 5));
}

#[test]
fn repeat_occurrence_isolation_and_explode_keep_follow_contexts_and_gap_clock() {
    let before = with_target(
        document(
            &["r"],
            vec![
                ("r", repeat("a", 2, 2, "repeat-follow-plays")),
                ("a", source(2, 0, 2002)),
            ],
        ),
        2002,
        &[("r", 2), ("a", 1)],
    );
    let before = ["r", "a"].into_iter().fold(before, |doc, owner| {
        follow_owner(doc, owner, framing_zoom::ZoomCurve::Linear)
    });
    let NodeKind::Repeat { iterations, .. } = &before.nodes()[&id("r")].kind else {
        unreachable!()
    };
    let isolated = changed(
        &before,
        "isolate-follow",
        Command::EditOccurrence {
            instance: InstancePath {
                node: id("a"),
                repeats: vec![RepeatInstance {
                    node: id("r"),
                    iteration: iterations.at(1).unwrap(),
                }],
            },
            edit: OccurrenceEdit::Rename {
                label: "Independent second play".into(),
            },
            identities: OccurrenceIdentities {
                nodes: (0..16)
                    .map(|n| id(&format!("isolate-follow-{n}")))
                    .collect(),
                marks: Vec::new(),
            },
        },
    );
    let old = RenderPlan::compile(&before).unwrap();
    let next = RenderPlan::compile(&isolated).unwrap();
    for frame in 0..6 {
        same_frame(&old, frame, &next, frame);
    }
    let need = isolated.explode_requirements(&id("r")).unwrap();
    let exploded = changed(
        &isolated,
        "explode-follow",
        Command::Explode {
            node: id("r"),
            identities: OccurrenceIdentities {
                nodes: (0..need.nodes)
                    .map(|n| id(&format!("explode-follow-{n}")))
                    .collect(),
                marks: (0..need.marks)
                    .map(|n| MarkId::new(format!("explode-mark-{n}")).unwrap())
                    .collect(),
            },
            timing: timing("explode-follow"),
        },
    );
    let exploded = RenderPlan::compile(&exploded).unwrap();
    for frame in 0..6 {
        same_frame(&old, frame, &exploded, frame);
    }
}

#[test]
fn inserted_freeze_captures_follow_view_and_preserves_source_path_after_provider_change() {
    let before = animated(
        &with_target(
            document(&["a"], vec![("a", source(8, 0, 8008))]),
            8008,
            &[("a", 2)],
        ),
        "a",
    );
    let old = RenderPlan::compile(&before).unwrap();
    let entry = old.picture(ProjectFrame(1)).unwrap();
    let context = CapturedFraming::capture(
        entry.picture_context.as_deref(),
        CapturedCanvas {
            width: 1920,
            height: 1080,
            fit: CapturedFit::Fit,
            layers: entry.framing[..entry.framing.len() - 1]
                .iter()
                .map(|layer| layer.pose)
                .collect(),
        },
    )
    .unwrap();
    assert_eq!(entry.framing[0].pose.unwrap().scale, ratio(19, 8));
    let after = changed(
        &before,
        "freeze-follow",
        Command::InsertTime {
            at: ProjectFrame(2),
            id: id("pause"),
            hold: HoldRecipe {
                duration: duration(3),
                video: HoldVideo::Freeze {
                    asset: asset_id("video"),
                    timestamp: SourceTimestamp {
                        ticks: 1001,
                        time_base: clock(),
                    },
                },
                audio: HoldAudio::Silence,
                picture_context: Some(context.clone()),
            },
            identities: SplitIdentities {
                nodes: (0..16).map(|n| id(&format!("freeze-follow-{n}"))).collect(),
            },
            timing: timing("freeze-follow"),
        },
    );
    let next = RenderPlan::compile(&after).unwrap();
    for frame in 0..2 {
        same_frame(&old, frame, &next, frame);
    }
    for frame in 2..8 {
        same_frame(&old, frame, &next, frame + 3);
    }
    for frame in 2..5 {
        let held = next.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(held.picture_context.as_deref(), Some(&context));
        assert_eq!(ticks(&held.picture), ratio(1001, 1));
    }
    let changed_provider = changed(
        &after,
        "freeze-provider",
        Command::SetHoldProvider {
            node: id("pause"),
            video: HoldVideo::Background,
        },
    );
    let changed_plan = RenderPlan::compile(&changed_provider).unwrap();
    for frame in 2..5 {
        let held = changed_plan.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(held.picture, Picture::Background);
        assert_eq!(held.picture_context.as_deref(), Some(&context));
    }
    for frame in 5..11 {
        same_frame(&next, frame, &changed_plan, frame);
    }
}
