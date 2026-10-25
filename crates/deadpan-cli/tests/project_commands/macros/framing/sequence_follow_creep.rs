//! A whole ordinary group follows each current provider on one owner clock.
use super::follow_creep::follow;
use super::*;
use deadpan_core::{
    AttentionTarget, AudioSample, BeatNode, FollowScaleTransition, FrameDuration, Framing,
    FramingClock, HoldAudio, HoldRecipe, HoldVideo, InstancePath, NodeKind, RepeatEditBranch,
    RepeatEditStep, RepeatInstance, ScopedNodeTarget, SemanticScopedContext, SourcePoint,
    SourceSpan, SourceTimestamp, SourceVideo, SourceVideoMapping, Subtree, TargetId, TargetPoint,
    TargetSample, TrackState,
};

fn grid(numerator: i128, denominator: i128) -> ExactRatio {
    let scale = 1_i128 << 32;
    let numerator = numerator * scale;
    let units = numerator / denominator;
    let remainder = numerator % denominator;
    let up = remainder * 2 > denominator || (remainder * 2 == denominator && units % 2 != 0);
    ratio(units + i128::from(up), scale)
}

fn only_group_framing(before: &ProjectDocument, after: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(before)?;
    expected["revision_id"] = json!(after.revision_id());
    expected["nodes"]["group"]["framing"] =
        serde_json::to_value(&after.nodes()[&node("group")].framing)?;
    assert_eq!(serde_json::to_value(after)?, expected);
    let baseline = RenderPlan::compile(before)?;
    let edited = RenderPlan::compile(after)?;
    let end = baseline.audio_duration()?;
    assert_eq!(end, edited.audio_duration()?);
    assert_eq!(
        baseline
            .audio(AudioSample(0)..end, Default::default())?
            .spans,
        edited.audio(AudioSample(0)..end, Default::default())?.spans,
    );
    for frame in 0..130 {
        let a = baseline.picture(ProjectFrame(frame))?;
        let b = edited.picture(ProjectFrame(frame))?;
        let provider = match frame {
            0..2 => "prefix",
            2..6 => "early",
            6..10 => "late",
            _ => "camera-source",
        };
        assert_eq!(b.instance.node, node(provider), "provider at {frame}");
        assert_eq!(a.instance, b.instance);
        assert_eq!(a.local_position, b.local_position);
        assert_eq!(a.picture, b.picture);
        assert_eq!(a.picture_context, b.picture_context);
        assert_eq!(a.framing.len(), b.framing.len());
        let mut owner_layers = 0;
        for (old, new) in a.framing.iter().zip(&b.framing) {
            let mut expected = old.clone();
            if new.instance.node == node("group") && !new.escalation {
                owner_layers += 1;
                assert_eq!(new.local_position, ratio(2 * i128::from(frame - 2) + 1, 2));
                assert_eq!(new.effective_duration, ratio(8, 1));
                expected.pose = new.pose;
            }
            assert_eq!(&expected, new, "layer at {frame}");
        }
        assert_eq!(owner_layers, usize::from((2..10).contains(&frame)));
    }
    for frame in [-1, 130] {
        assert!(baseline.picture(ProjectFrame(frame)).is_err());
        assert!(edited.picture(ProjectFrame(frame)).is_err());
    }
    Ok(())
}

#[test]
fn whole_sequence_follow_creep_crosses_source_cuts_and_replays_staged_keep_with_one_undo() -> Result
{
    let scratch = tempfile::tempdir()?;
    let package = qualified_project(scratch.path())?;
    let input = scratch.path().join("sequence-follow.json");
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let original = store.snapshot()?;
    let asset = AssetId::new("camera")?;
    let index = store.source_video_index(original.revision_id(), &asset)?;
    let stamp = |ordinal: usize| SourceTimestamp {
        ticks: index.frames()[ordinal].pts,
        time_base: index.time_base(),
    };
    let spans = [
        SourceSpan::new(stamp(10), stamp(14))?,
        SourceSpan::new(stamp(70), stamp(74))?,
    ];
    let sample_times = [
        stamp(10).ticks,
        stamp(14).ticks,
        stamp(70).ticks,
        stamp(74).ticks,
    ];
    let original_span = original.assets()[&asset].video.unwrap();
    drop(store);
    for (id, centers) in [
        ("subject", [200_000, 800_000]),
        ("other", [700_000, 300_000]),
    ] {
        let point = |x| {
            TargetPoint {
                center: [x, 500_000],
            }
            .into()
        };
        command(
            &package,
            &input,
            Command::SetTarget {
                id: TargetId::new(id)?,
                target: AttentionTarget {
                    label: id.into(),
                    asset: asset.clone(),
                    span: original_span,
                    region: point(centers[0]),
                    samples: sample_times
                        .iter()
                        .enumerate()
                        .map(|(i, at)| TargetSample {
                            at: *at,
                            region: point(centers[usize::from(i >= 2)]),
                            confidence: 1000,
                            state: TrackState::Tracked,
                        })
                        .collect(),
                    corrections: vec![],
                    provenance: None,
                    display_aspect: None,
                },
            },
            &format!("target-{id}"),
        )?;
    }
    let prefix = BeatNode::hold(
        "Before group",
        HoldRecipe {
            duration: FrameDuration::new(2)?,
            picture_context: None,
            video: HoldVideo::Freeze {
                asset: asset.clone(),
                timestamp: original_span.start(),
            },
            audio: HoldAudio::Silence,
        },
    );
    command(
        &package,
        &input,
        Command::Insert {
            parent: original.root().clone(),
            index: 0,
            subtree: Subtree {
                root: node("prefix"),
                nodes: BTreeMap::from([(node("prefix"), prefix)]),
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
        "prefix",
    )?;
    let mut nodes = BTreeMap::new();
    for (name, span) in ["early", "late"].into_iter().zip(spans) {
        let mut child = original.nodes()[&node("camera-source")].clone();
        let NodeKind::Source { source } = &mut child.kind else {
            panic!("Source fixture")
        };
        source.duration = FrameDuration::new(4)?;
        source.edit_window = None;
        source.video = SourceVideo::Stream {
            asset: asset.clone(),
            span,
        };
        source.video_mapping = SourceVideoMapping::FitBeat;
        // The qualified fixture deliberately uses video-only registration.
        assert!(source.audio.is_none());
        if name == "late" {
            child.framing = Some(Framing::static_pose(FramingPose::new(
                ratio(1, 2),
                ratio(1, 2),
                ratio(5, 4),
            )?)?);
        }
        nodes.insert(node(name), child);
    }
    let mut group = BeatNode::sequence(
        "Two disjoint Original intervals",
        vec![node("early"), node("late")],
    );
    group.framing = Some(follow("subject", ratio(3, 2), ratio(3, 4)));
    nodes.insert(node("group"), group);
    let before = command(
        &package,
        &input,
        Command::Insert {
            parent: original.root().clone(),
            index: 1,
            subtree: Subtree {
                root: node("group"),
                nodes,
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
        "group",
    )?;
    assert_eq!(before.duration()?.frames(), 130);
    let instruction = zoom(
        ZoomKind::Creep {
            from: None,
            to: ratio(3, 1),
            curve: ZoomCurve::Linear,
        },
        SemanticZoomCenter::Keep,
    );
    invoke(
        &package,
        &input,
        &save(
            &before,
            0,
            "c",
            serde_json::to_value([instruction.clone()])?,
        ),
        false,
    )?;
    let staged = [
        zoom(
            ZoomKind::Smash { scale: ratio(2, 1) },
            SemanticZoomCenter::Keep,
        ),
        I::Call {
            register: RegisterName::new('c')?,
            count: count(1),
        },
    ];
    invoke(
        &package,
        &input,
        &save(&before, 1, "s", serde_json::to_value(staged)?),
        false,
    )?;
    let bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;
    for selection in [
        Some((2, 10, false)),
        Some((3, 9, false)),
        Some((10, 2, true)),
    ] {
        let cursor = selection
            .filter(|(_, _, extending)| *extending)
            .map_or(5, |(_, head, _)| head);
        let error = reject(
            &package,
            &input,
            &invocation(
                &before,
                bank.version,
                "c",
                "group",
                cursor,
                selection,
                "range-refused",
            ),
        )?;
        let expected = if selection.is_some_and(|(anchor, head, _)| anchor.abs_diff(head) == 8) {
            deadpan_core::framing_zoom::FOLLOW_CREEP_REFUSAL
        } else {
            deadpan_core::framing_zoom::RANGED_FOLLOW_REFUSAL
        };
        assert!(
            error["error"]["message"]
                .as_str()
                .unwrap()
                .ends_with(expected),
            "{selection:?}: {error}"
        );
    }
    let request = invocation(
        &before,
        bank.version,
        "c",
        "group",
        5,
        None,
        "sequence-creep",
    );
    let frozen = state(&package)?;
    let predicted = preview_framing(&package, &request, "group")?;
    let dry = invoke(&package, &input, &request, true)?;
    assert_eq!(state(&package)?, frozen);
    let committed = invoke(&package, &input, &request, false)?;
    assert_eq!(
        comparable_trace(&dry, before.revision_id()),
        comparable_trace(&committed, before.revision_id())
    );
    let after = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let mut expected = follow("subject", ratio(3, 2), ratio(3, 4));
    if let FramingValue::Follow {
        scale_transition, ..
    } = &mut expected.value
    {
        *scale_transition = Some(FollowScaleTransition {
            factor: ratio(2, 1),
            curve: ZoomCurve::Linear,
        });
    }
    assert_eq!(predicted, (Some(expected.clone()), 130));
    assert_eq!(after.nodes()[&node("group")].framing, Some(expected));
    only_group_framing(&before, &after)?;
    let plan = RenderPlan::compile(&after)?;
    for frame in 0..8 {
        let shown = pose(&after, "group", 2 + frame);
        assert_eq!(shown.scale, ratio(3 * (17 + 2 * i128::from(frame)), 32));
        // The target switches with Original time, then passes the late child's
        // independently retained 5/4 crop: .8 becomes .875 in the group input.
        assert_eq!(
            shown.center_x,
            if frame < 4 { grid(1, 5) } else { grid(7, 8) }
        );
        let span = spans[usize::from(frame >= 4)];
        let local = frame % 4;
        let expected_point = SourcePoint {
            ticks: ratio(
                i128::from(span.start().ticks) * 8
                    + i128::from(span.end().ticks - span.start().ticks)
                        * (2 * i128::from(local) + 1),
                8,
            ),
            time_base: span.start().time_base,
        };
        assert_eq!(
            plan.picture(ProjectFrame(2 + frame))?
                .picture
                .follow_point(),
            Some((&asset, expected_point))
        );
    }
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.undo(after.revision_id(), RevisionId::new("sequence-undo")?)?;
    same_document(&writer.snapshot()?, &before)?;
    writer.checkpoint()?;
    drop(writer);
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.redo(
        &RevisionId::new("sequence-undo")?,
        RevisionId::new("sequence-redo")?,
    )?;
    same_document(&writer.snapshot()?, &after)?;
    assert_eq!(writer.registers()?, bank);
    writer.validate()?;
    let redone = writer.snapshot()?;
    drop(writer);
    let error = reject(
        &package,
        &input,
        &invocation(
            &redone,
            bank.version,
            "c",
            "group",
            5,
            None,
            "animated-refused",
        ),
    )?;
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .ends_with(deadpan_core::framing_zoom::PATH_REFUSAL)
    );

    // A staged scale edit and nested call must resolve the group's current
    // Follow again, retaining Keep rather than a previously sampled subject.
    let reset = command(
        &package,
        &input,
        Command::SetFraming {
            node: node("group"),
            framing: Some(follow("other", ratio(5, 4), ratio(1, 2))),
        },
        "other-subject",
    )?;
    let request = invocation(
        &reset,
        bank.version,
        "s",
        "group",
        7,
        None,
        "staged-sequence",
    );
    let frozen = state(&package)?;
    let dry = invoke(&package, &input, &request, true)?;
    assert_eq!(state(&package)?, frozen);
    let committed = invoke(&package, &input, &request, false)?;
    assert_eq!(
        comparable_trace(&dry, reset.revision_id()),
        comparable_trace(&committed, reset.revision_id())
    );
    let staged = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert!(matches!(&staged.nodes()[&node("group")].framing,
        Some(Framing { clock: FramingClock::OwnerOutput, value: FramingValue::Follow {
            target, scale, fallback, scale_transition: Some(FollowScaleTransition { factor, curve: ZoomCurve::Linear }) } })
        if target.as_str() == "other" && *scale == ratio(2, 1) && fallback.scale == ratio(2, 1) && *factor == ratio(3, 2)));
    only_group_framing(&reset, &staged)?;
    for frame in 0..8 {
        assert_eq!(
            pose(&staged, "group", 2 + frame).scale,
            ratio(33 + 2 * i128::from(frame), 16)
        );
    }
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.undo(staged.revision_id(), RevisionId::new("staged-undo")?)?;
    same_document(&writer.snapshot()?, &reset)?;
    drop(writer);
    let plain = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    // Native dot uses this same inline Apply boundary; the CLI itself has no
    // persistent dot register. Resolve the current target/base on invocation.
    invoke(
        &package,
        &input,
        &envelope(
            &plain,
            bank.version,
            json!({
                "type":"apply", "program":{"instructions":[instruction]},
                "parent":plain.root(), "selected_child":"group", "cursor":7,
                "visual_selection":null, "new_revision":"inline-sequence"
            }),
        ),
        false,
    )?;
    let inlined = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert!(matches!(&inlined.nodes()[&node("group")].framing,
        Some(Framing { value: FramingValue::Follow { target, scale, scale_transition: Some(FollowScaleTransition { factor, .. }), .. }, .. })
        if target.as_str() == "other" && *scale == ratio(5, 4) && *factor == ratio(12, 5)));
    only_group_framing(&plain, &inlined)?;

    command(
        &package,
        &input,
        Command::WrapRepeat {
            node: node("group"),
            id: node("repeat"),
            plays: 2,
            gap: None,
            anchor_policy: Default::default(),
        },
        "repeat-group",
    )?;
    let repeated = command(
        &package,
        &input,
        Command::SetFraming {
            node: node("repeat"),
            framing: Some(follow("subject", ratio(3, 2), ratio(3, 4))),
        },
        "repeat-follow",
    )?;
    // The whole ordinary Repeat is now also admitted. Its Sequence child still
    // carries its independent path; only the selected outer owner changes.
    invoke(
        &package,
        &input,
        &invocation(
            &repeated,
            bank.version,
            "c",
            "repeat",
            11,
            None,
            "repeat-creep",
        ),
        false,
    )?;
    let animated = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let mut expected = serde_json::to_value(&repeated)?;
    expected["revision_id"] = json!(animated.revision_id());
    expected["nodes"]["repeat"]["framing"] =
        serde_json::to_value(&animated.nodes()[&node("repeat")].framing)?;
    assert_eq!(serde_json::to_value(&animated)?, expected);
    assert!(matches!(&animated.nodes()[&node("repeat")].framing,
        Some(Framing { value: FramingValue::Follow { target, scale, scale_transition: Some(FollowScaleTransition { factor, .. }), .. }, .. })
        if target.as_str() == "subject" && *scale == ratio(3, 2) && *factor == ratio(2, 1)));
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.undo(
        animated.revision_id(),
        RevisionId::new("repeat-creep-undo")?,
    )?;
    same_document(&writer.snapshot()?, &repeated)?;
    drop(writer);
    let retimed = command(
        &package,
        &input,
        Command::WrapRetime {
            node: node("repeat"),
            id: node("retime"),
            duration: FrameDuration::new(16)?,
            pitch: PitchPolicy::FollowSpeed,
        },
        "retime-group",
    )?;
    assert_eq!(retimed.duration()?.frames(), repeated.duration()?.frames());
    let retimed = command(
        &package,
        &input,
        Command::SetFraming {
            node: node("retime"),
            framing: Some(follow("subject", ratio(3, 2), ratio(3, 4))),
        },
        "retime-follow",
    )?;
    invoke(
        &package,
        &input,
        &invocation(
            &retimed,
            bank.version,
            "c",
            "retime",
            11,
            None,
            "retime-creep",
        ),
        false,
    )?;
    let animated = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let mut expected = serde_json::to_value(&retimed)?;
    expected["revision_id"] = json!(animated.revision_id());
    expected["nodes"]["retime"]["framing"] =
        serde_json::to_value(&animated.nodes()[&node("retime")].framing)?;
    assert_eq!(serde_json::to_value(&animated)?, expected);
    assert!(matches!(&animated.nodes()[&node("retime")].framing,
        Some(Framing { value: FramingValue::Follow { target, scale, scale_transition: Some(FollowScaleTransition { factor, .. }), .. }, .. })
        if target.as_str() == "subject" && *scale == ratio(3, 2) && *factor == ratio(2, 1)));
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.undo(
        animated.revision_id(),
        RevisionId::new("retime-creep-undo")?,
    )?;
    same_document(&writer.snapshot()?, &retimed)?;
    drop(writer);
    let NodeKind::Repeat { iterations, .. } = &retimed.nodes()[&node("repeat")].kind else {
        panic!("Repeat")
    };
    let iteration = iterations.at(1).unwrap();
    let scoped = SemanticScopedContext {
        root: node("retime"),
        target: ScopedNodeTarget {
            node: node("group"),
            repeats: vec![RepeatEditStep {
                repeat: node("repeat"),
                branch: RepeatEditBranch::Play {
                    iteration: iteration.clone(),
                },
            }],
        },
        presentation: InstancePath {
            node: node("group"),
            repeats: vec![RepeatInstance {
                node: node("repeat"),
                iteration,
            }],
        },
    };
    let mut request = invocation(
        &retimed,
        bank.version,
        "c",
        "retime",
        11,
        None,
        "scoped-creep",
    );
    request["operation"]["scoped"] = serde_json::to_value(scoped)?;
    // Existing animation still refuses implicit replacement. Use a fixed Follow
    // to exercise creation inside this concrete play.
    let plain = command(
        &package,
        &input,
        Command::SetFraming {
            node: node("group"),
            framing: Some(follow("other", ratio(5, 4), ratio(1, 2))),
        },
        "plain-scoped-group",
    )?;
    request["expected_revision"] = json!(plain.revision_id());
    let result = invoke(&package, &input, &request, false)?;
    let (after, mapped, frames) = assert_scoped_follow_commit(&package, &plain, &result)?;
    assert_eq!(frames, (10..18).collect::<Vec<_>>());
    assert!(matches!(&after.nodes()[&mapped.target.node].framing,
        Some(Framing { value: FramingValue::Follow { target, scale, scale_transition: Some(FollowScaleTransition { factor, .. }), .. }, .. })
        if target.as_str() == "other" && *scale == ratio(5, 4) && *factor == ratio(12, 5)));
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.undo(
        after.revision_id(),
        RevisionId::new("scoped-sequence-undo")?,
    )?;
    same_document(&writer.snapshot()?, &plain)?;
    drop(writer);
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?,
        bank
    );
    Ok(())
}
