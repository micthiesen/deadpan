//! Whole-Hold semantic authoring shares the Source path without altering providers.
use super::follow_creep::follow;
use super::*;
use deadpan_core::{
    AttentionTarget, CapturedCanvas, CapturedFit, CapturedFraming, FollowScaleTransition, Framing,
    FramingClock, HoldAudio, HoldRecipe, HoldVideo, InstancePath, IterationId, NodeKind,
    RepeatEditBranch, RepeatEditStep, RepeatInstance, ScopedNodeTarget, SemanticScopedContext,
    Subtree, TargetId, TargetPoint,
};

fn assert_recipe_and_picture(
    before: &ProjectDocument,
    after: &ProjectDocument,
    owner: &str,
    frame: i64,
) -> Result {
    let mut expected = before.nodes()[&node(owner)].clone();
    expected.framing = after.nodes()[&node(owner)].framing.clone();
    assert_eq!(after.nodes()[&node(owner)], expected);
    assert_eq!(before.duration()?, after.duration()?);
    let a = RenderPlan::compile(before)?.picture(ProjectFrame(frame))?;
    let b = RenderPlan::compile(after)?.picture(ProjectFrame(frame))?;
    assert_eq!(a.picture, b.picture);
    assert_eq!(a.picture_context, b.picture_context);
    Ok(())
}

fn fallback_scale(frame: i64) -> ExactRatio {
    // Independent positive integer Q32 oracle: round each endpoint first,
    // then round the frame-midpoint blend, with ties going to the even unit.
    fn round_even(numerator: i128, denominator: i128) -> i128 {
        let whole = numerator / denominator;
        let remainder = numerator % denominator;
        whole
            + i128::from(
                remainder * 2 > denominator || (remainder * 2 == denominator && whole % 2 != 0),
            )
    }
    let grid = 1_i128 << 32;
    let low = round_even(3 * grid, 5);
    let high = round_even(6 * grid, 5);
    let progress = 2 * i128::from(frame) + 1;
    ratio(
        round_even(low * (16 - progress) + high * progress, 16),
        grid,
    )
}

fn assert_owner_boundaries(
    before: &ProjectDocument,
    after: &ProjectDocument,
    owner: &str,
    boundaries: &[(i64, &str)],
) -> Result {
    let baseline = RenderPlan::compile(before)?;
    let edited = RenderPlan::compile(after)?;
    for &(frame, provider) in boundaries {
        let a = baseline.picture(ProjectFrame(frame))?;
        let b = edited.picture(ProjectFrame(frame))?;
        assert_eq!(b.instance.node, node(provider), "provider at {frame}");
        assert_eq!(a.instance, b.instance, "instance at {frame}");
        assert_eq!(
            a.local_position, b.local_position,
            "provider clock at {frame}"
        );
        assert_eq!(a.picture, b.picture, "picture at {frame}");
        assert_eq!(a.picture_context, b.picture_context, "capture at {frame}");
        assert_eq!(a.framing.len(), b.framing.len(), "layers at {frame}");
        let mut owner_layers = 0;
        for (old, new) in a.framing.iter().zip(&b.framing) {
            if new.instance.node == node(owner) && !new.escalation {
                owner_layers += 1;
                let mut expected = old.clone();
                // All eight changed poses have independent scale assertions below.
                expected.pose = new.pose;
                assert_eq!(&expected, new, "owner layer at {frame}");
            } else {
                assert_eq!(old, new, "unchanged layer at {frame}");
            }
        }
        assert_eq!(
            owner_layers,
            usize::from(provider == owner),
            "owner at {frame}"
        );
    }
    assert!(baseline.picture(ProjectFrame(144)).is_err());
    assert!(edited.picture(ProjectFrame(144)).is_err());
    Ok(())
}

#[test]
fn whole_hold_keep_follow_creep_replays_live_and_captured_fallback_with_durable_undo_and_scoped_replay()
-> Result {
    let scratch = tempfile::tempdir()?;
    let package = qualified_project(scratch.path())?;
    let input = scratch.path().join("hold-follow.json");
    let original = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let asset = AssetId::new("camera")?;
    let span = original.assets()[&asset].video.unwrap();
    let receipt = ProjectStore::open(&package, AccessMode::ReadOnly)?
        .registered_source(original.revision_id(), &asset)?;
    let stream = receipt.snapshot().video().unwrap().interpretation();
    let visible = stream.visible_bounds()?;
    let measured_aspect = deadpan_core::display_aspect(
        [visible[2], visible[3]],
        [stream.sample_aspect_num, stream.sample_aspect_den],
        stream.rotation_quarter_turns,
    )
    .ok_or("fixture aspect is unavailable")?;
    assert!(
        measured_aspect
            .compare(ratio(
                i128::from(original.presentation_basis().width),
                i128::from(original.presentation_basis().height)
            ))
            .is_ge()
    );
    for (name, x) in [("left", 300_000), ("right", 700_000)] {
        command(
            &package,
            &input,
            Command::SetTarget {
                id: TargetId::new(name)?,
                target: AttentionTarget {
                    label: name.into(),
                    asset: asset.clone(),
                    span,
                    region: TargetPoint {
                        center: [x, 400_000],
                    }
                    .into(),
                    samples: vec![],
                    corrections: vec![],
                    provenance: None,
                    display_aspect: Some(measured_aspect),
                },
            },
            &format!("target-{name}"),
        )?;
    }
    let basis = original.presentation_basis();
    let captured = CapturedFraming::new(vec![CapturedCanvas {
        width: basis.width,
        height: basis.height,
        fit: CapturedFit::Fit,
        // The left target at x=.3 is outside this central [.375,.625] crop.
        // Its fallback remains explicit even when other captured views can resolve.
        layers: vec![Some(FramingPose::new(
            ratio(1, 2),
            ratio(1, 2),
            ratio(4, 1),
        )?)],
    }])?;
    for (index, name, target, scale, fallback) in [
        (1, "live", "left", ratio(3, 2), ratio(3, 5)),
        (2, "other", "right", ratio(2, 1), ratio(1, 2)),
        (3, "captured", "left", ratio(3, 2), ratio(3, 5)),
    ] {
        let mut held = deadpan_core::BeatNode::hold(
            name,
            HoldRecipe {
                duration: deadpan_core::FrameDuration::new(8)?,
                picture_context: (name == "captured").then(|| captured.clone()),
                video: HoldVideo::Freeze {
                    asset: asset.clone(),
                    timestamp: span.start(),
                },
                audio: HoldAudio::Silence,
            },
        );
        held.framing = Some(follow(target, scale, fallback));
        command(
            &package,
            &input,
            Command::Insert {
                parent: original.root().clone(),
                index,
                subtree: Subtree {
                    root: node(name),
                    nodes: BTreeMap::from([(node(name), held)]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
            &format!("insert-{name}"),
        )?;
    }
    let before = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
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
            "h",
            serde_json::to_value([instruction.clone()])?,
        ),
        false,
    )?;
    let bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;
    // Full and interior ranges retain explicit local Follow windows.
    for selection in [
        Some((120, 128, false)),
        Some((121, 127, false)),
        Some((128, 120, true)),
    ] {
        assert_ranged_follow_preview(
            &package,
            &input,
            &invocation(
                &before,
                bank.version,
                "h",
                "live",
                if selection.is_some_and(|(_, _, extending)| extending) {
                    120
                } else {
                    123
                },
                selection,
                "range-preview",
            ),
            "live",
            120,
            8,
            selection.map(|(a, b, _)| (a, b)).unwrap(),
        )?;
    }
    let request = invocation(&before, bank.version, "h", "live", 123, None, "live-creep");
    let frozen = state(&package)?;
    let prediction = preview_framing(&package, &request, "live")?;
    let dry = invoke(&package, &input, &request, true)?;
    assert_eq!(state(&package)?, frozen);
    let committed = invoke(&package, &input, &request, false)?;
    assert_eq!(
        comparable_trace(&dry, before.revision_id()),
        comparable_trace(&committed, before.revision_id())
    );
    let live = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let mut expected = follow("left", ratio(3, 2), ratio(3, 5));
    if let FramingValue::Follow {
        scale_transition, ..
    } = &mut expected.value
    {
        *scale_transition = Some(FollowScaleTransition {
            factor: ratio(2, 1),
            curve: ZoomCurve::Linear,
            range: None,
        });
    }
    assert_eq!(prediction, (Some(expected.clone()), 144));
    assert_eq!(live.nodes()[&node("live")].framing, Some(expected));
    assert_recipe_and_picture(&before, &live, "live", 123)?;
    assert_owner_boundaries(
        &before,
        &live,
        "live",
        &[
            (119, "camera-source"),
            (120, "live"),
            (127, "live"),
            (128, "other"),
        ],
    )?;
    for frame in 0..8 {
        let shown = pose(&live, "live", 120 + frame);
        assert_eq!(
            shown.center_x,
            FramingPose::new(ratio(3, 10), ratio(2, 5), ExactRatio::ONE)?
                .quantized()?
                .center_x
        );
        // Independent exact endpoint blend for midpoint frame sampling over8.
        assert_eq!(shown.scale, ratio(3 * (17 + 2 * i128::from(frame)), 32));
    }
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.undo(live.revision_id(), RevisionId::new("undo-live")?)?;
    same_document(&writer.snapshot()?, &before)?;
    drop(writer);
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.redo(
        &RevisionId::new("undo-live")?,
        RevisionId::new("redo-live")?,
    )?;
    same_document(&writer.snapshot()?, &live)?;
    let redone = writer.snapshot()?;
    drop(writer);
    reject(
        &package,
        &input,
        &invocation(
            &redone,
            bank.version,
            "h",
            "live",
            123,
            None,
            "existing-transition",
        ),
    )?;

    // The same saved program resolves the invoked Hold's current target/base.
    invoke(
        &package,
        &input,
        &invocation(
            &redone,
            bank.version,
            "h",
            "other",
            131,
            None,
            "other-creep",
        ),
        false,
    )?;
    let other = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert!(matches!(&other.nodes()[&node("other")].framing,
        Some(Framing{clock:FramingClock::OwnerOutput,value:FramingValue::Follow{target,scale,fallback,
            scale_transition:Some(FollowScaleTransition {factor,curve:ZoomCurve::Linear, range: None })}})
        if target.as_str()=="right" && *scale==ratio(2,1) && fallback.scale==ratio(1,2) && *factor==ratio(3,2)));
    assert_recipe_and_picture(&redone, &other, "other", 131)?;
    invoke(
        &package,
        &input,
        &invocation(
            &other,
            bank.version,
            "h",
            "captured",
            139,
            None,
            "captured-creep",
        ),
        false,
    )?;
    let after = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_recipe_and_picture(&other, &after, "captured", 139)?;
    assert_owner_boundaries(
        &other,
        &after,
        "captured",
        &[(135, "other"), (136, "captured"), (143, "captured")],
    )?;
    for frame in 0..8 {
        let shown = pose(&after, "captured", 136 + frame);
        assert_eq!([shown.center_x, shown.center_y], [ratio(1, 3), ratio(2, 3)]);
        assert_eq!(shown.scale, fallback_scale(frame));
    }

    // Native dot replays this same inline semantic Apply boundary. The CLI has
    // no persistent dot state; this verifies re-resolution rather than inventing one.
    let reset = command(
        &package,
        &input,
        Command::SetFraming {
            node: node("live"),
            framing: Some(follow("right", ratio(2, 1), ratio(1, 2))),
        },
        "rebind-live",
    )?;
    let inline = envelope(
        &reset,
        bank.version,
        json!({"type":"apply","program":{"instructions":[instruction]},
        "parent":reset.root(),"selected_child":"live","cursor":123,"visual_selection":null,"new_revision":"inline-creep"}),
    );
    invoke(&package, &input, &inline, false)?;
    let inlined = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert_eq!(
        inlined.nodes()[&node("live")].framing,
        other.nodes()[&node("other")].framing
    );
    assert_recipe_and_picture(&reset, &inlined, "live", 123)?;

    let plain = command(
        &package,
        &input,
        Command::SetFraming {
            node: node("other"),
            framing: Some(follow("right", ratio(2, 1), ratio(1, 2))),
        },
        "plain-other",
    )?;
    assert_eq!(plain.duration()?, inlined.duration()?);
    let wrapped = command(
        &package,
        &input,
        Command::WrapRepeat {
            node: node("other"),
            id: node("repeated-hold"),
            plays: 2,
            gap: None,
            anchor_policy: Default::default(),
        },
        "wrap-other",
    )?;
    let NodeKind::Repeat { iterations, .. } = &wrapped.nodes()[&node("repeated-hold")].kind else {
        panic!("Repeat")
    };
    let iteration: IterationId = iterations.at(1).unwrap();
    let scoped = SemanticScopedContext {
        root: node("repeated-hold"),
        target: ScopedNodeTarget {
            node: node("other"),
            repeats: vec![RepeatEditStep {
                repeat: node("repeated-hold"),
                branch: RepeatEditBranch::Play {
                    iteration: iteration.clone(),
                },
            }],
        },
        presentation: InstancePath {
            node: node("other"),
            repeats: vec![RepeatInstance {
                node: node("repeated-hold"),
                iteration,
            }],
        },
    };
    let mut request = invocation(
        &wrapped,
        bank.version,
        "h",
        "repeated-hold",
        137,
        None,
        "scoped-creep",
    );
    request["operation"]["scoped"] = serde_json::to_value(scoped)?;
    let result = invoke(&package, &input, &request, false)?;
    let (after, mapped, frames) = assert_scoped_follow_commit(&package, &wrapped, &result)?;
    assert_eq!(frames, (136..144).collect::<Vec<_>>());
    assert!(matches!(&after.nodes()[&mapped.target.node].framing,
        Some(Framing { value: FramingValue::Follow { target, scale, scale_transition: Some(FollowScaleTransition { factor, .. }), .. }, .. })
        if target.as_str() == "right" && *scale == ratio(2, 1) && *factor == ratio(3, 2)));
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.undo(after.revision_id(), RevisionId::new("scoped-hold-undo")?)?;
    same_document(&writer.snapshot()?, &wrapped)?;
    drop(writer);
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?,
        bank
    );
    Ok(())
}
