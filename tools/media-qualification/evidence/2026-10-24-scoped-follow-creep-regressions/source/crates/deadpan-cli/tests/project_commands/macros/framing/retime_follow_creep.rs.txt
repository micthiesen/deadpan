//! Retime owner scale, fractional Original points and decoded ordinals differ.
use super::follow_creep::follow;
use super::*;
use deadpan_core::{
    AttentionTarget, AudioSample, BeatNode, FollowScaleTransition, FrameDuration, Framing,
    FramingClock, HoldAudio, HoldRecipe, HoldVideo, InstancePath, NodeKind, RepeatEditBranch,
    RepeatEditStep, RepeatInstance, RetimePurpose, ScopedNodeTarget, SemanticScopedContext,
    SourcePoint, SourceSpan, SourceTimestamp, SourceVideo, SourceVideoMapping, Subtree,
    TargetCorrection, TargetId, TargetPoint, TargetSample, TrackState,
};

const Q: i128 = 1_i128 << 32;

fn round_even(n: i128, d: i128) -> i128 {
    assert!(n >= 0 && d > 0);
    let whole = n / d;
    let remainder = n % d;
    whole + i128::from(remainder * 2 > d || (remainder * 2 == d && whole % 2 != 0))
}

fn grid(value: ExactRatio) -> ExactRatio {
    ratio(round_even(value.numerator() * Q, value.denominator()), Q)
}

fn scale(frame: i64, low: ExactRatio, high: ExactRatio) -> ExactRatio {
    let p = round_even((2 * i128::from(frame) + 1) * Q, 24);
    let low = round_even(low.numerator() * Q, low.denominator());
    let high = round_even(high.numerator() * Q, high.denominator());
    ratio(round_even(low * (Q - p) + high * p, Q), Q)
}

fn retained(before: &ProjectDocument, after: &ProjectDocument) -> Result {
    let mut expected = serde_json::to_value(before)?;
    expected["revision_id"] = json!(after.revision_id());
    expected["nodes"]["retime"]["framing"] =
        serde_json::to_value(&after.nodes()[&node("retime")].framing)?;
    assert_eq!(serde_json::to_value(after)?, expected);
    assert_eq!(after.duration()?.frames(), 132);
    let baseline = RenderPlan::compile(before)?;
    let edited = RenderPlan::compile(after)?;
    let end = baseline.audio_duration()?;
    assert_eq!(end, edited.audio_duration()?);
    assert_eq!(
        baseline
            .audio(AudioSample(0)..end, Default::default())?
            .spans,
        edited.audio(AudioSample(0)..end, Default::default())?.spans
    );
    for frame in 0..132 {
        let a = baseline.picture(ProjectFrame(frame))?;
        let b = edited.picture(ProjectFrame(frame))?;
        assert_eq!(
            b.instance.node,
            node(match frame {
                0..4 => "early",
                4..7 => "blank",
                7..12 => "late",
                _ => "camera-source",
            })
        );
        assert_eq!(a.picture, b.picture, "picture {frame}");
        assert_eq!(a.picture_context, b.picture_context);
        assert_eq!(a.instance, b.instance);
        assert_eq!(a.local_position, b.local_position);
        assert_eq!(a.framing.len(), b.framing.len());
        let mut owner_layers = 0;
        for (old, new) in a.framing.iter().zip(&b.framing) {
            let mut expected = old.clone();
            if new.instance.node == node("retime") && !new.escalation {
                owner_layers += 1;
                assert_eq!(new.local_position, ratio(2 * i128::from(frame) + 1, 2));
                assert_eq!(new.effective_duration, ratio(12, 1));
                expected.pose = new.pose;
            }
            assert_eq!(&expected, new, "layer {frame}");
        }
        assert_eq!(owner_layers, usize::from(frame < 12));
    }
    for frame in [-1, 132] {
        assert!(baseline.picture(ProjectFrame(frame)).is_err());
        assert!(edited.picture(ProjectFrame(frame)).is_err());
    }
    Ok(())
}

#[test]
fn whole_retime_follow_creep_keeps_nonzero_map_fractional_target_clock_and_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = qualified_project(scratch.path())?;
    let input = scratch.path().join("retime-follow.json");
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
    let point = |x| {
        TargetPoint {
            center: [x, 500_000],
        }
        .into()
    };
    for id in ["subject", "other"] {
        let samples = if id == "subject" {
            vec![
                (10, 400_000, TrackState::Tracked),
                (11, 500_000, TrackState::Tracked),
                (12, 500_000, TrackState::Lost),
                (70, 700_000, TrackState::Tracked),
                (71, 600_000, TrackState::Tracked),
                (72, 600_000, TrackState::Lost),
            ]
            .into_iter()
            .map(|(i, x, state)| TargetSample {
                at: stamp(i).ticks,
                region: point(x),
                confidence: if state == TrackState::Lost { 0 } else { 1000 },
                state,
            })
            .collect()
        } else {
            vec![]
        };
        command(
            &package,
            &input,
            Command::SetTarget {
                id: TargetId::new(id)?,
                target: AttentionTarget {
                    label: id.into(),
                    asset: asset.clone(),
                    span: original.assets()[&asset].video.unwrap(),
                    region: point(if id == "subject" { 400_000 } else { 500_000 }),
                    samples,
                    corrections: if id == "subject" {
                        vec![
                            TargetCorrection {
                                at: stamp(13).ticks,
                                region: point(600_000),
                            },
                            TargetCorrection {
                                at: stamp(72).ticks,
                                region: point(450_000),
                            },
                        ]
                    } else {
                        vec![]
                    },
                    provenance: None,
                    display_aspect: None,
                },
            },
            &format!("target-{id}"),
        )?;
    }
    drop(store);
    let mut nodes = BTreeMap::new();
    for (name, span) in ["early", "late"].into_iter().zip(spans) {
        let mut child = original.nodes()[&node("camera-source")].clone();
        let NodeKind::Source { source } = &mut child.kind else {
            panic!("Source")
        };
        source.duration = FrameDuration::new(4)?;
        source.edit_window = None;
        source.video = SourceVideo::Stream {
            asset: asset.clone(),
            span,
        };
        source.video_mapping = SourceVideoMapping::FitBeat;
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
    nodes.insert(
        node("blank"),
        BeatNode::hold(
            "Fallback",
            HoldRecipe {
                duration: FrameDuration::new(2)?,
                picture_context: None,
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            },
        ),
    );
    nodes.insert(
        node("body"),
        BeatNode::sequence(
            "Different Original intervals",
            vec![node("early"), node("blank"), node("late")],
        ),
    );
    let mut owner = BeatNode::sequence("Nonzero Retime", vec![]);
    owner.kind = NodeKind::Retime {
        child: node("body"),
        duration: FrameDuration::new(12)?,
        mapping: deadpan_core::FrameRange::new(ProjectFrame(1), ProjectFrame(9))?,
        pitch: PitchPolicy::Preserve,
        purpose: RetimePurpose::Edit,
    };
    owner.framing = Some(follow("subject", ratio(2, 1), ratio(1, 2)));
    nodes.insert(node("retime"), owner);
    let before = command(
        &package,
        &input,
        Command::Insert {
            parent: original.root().clone(),
            index: 0,
            subtree: Subtree {
                root: node("retime"),
                nodes,
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
        "retime",
    )?;
    let instruction = zoom(
        ZoomKind::Creep {
            from: None,
            to: ratio(4, 1),
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
        Some((0, 12, false)),
        Some((1, 11, false)),
        Some((12, 0, true)),
    ] {
        let cursor = selection
            .filter(|(_, _, extending)| *extending)
            .map_or(2, |(_, head, _)| head);
        let error = reject(
            &package,
            &input,
            &invocation(
                &before,
                bank.version,
                "c",
                "retime",
                cursor,
                selection,
                "range-refused",
            ),
        )?;
        let expected = if selection.is_some_and(|(a, b, _)| a.abs_diff(b) == 12) {
            deadpan_core::framing_zoom::FOLLOW_CREEP_REFUSAL
        } else {
            deadpan_core::framing_zoom::RANGED_FOLLOW_REFUSAL
        };
        assert!(
            error["error"]["message"]
                .as_str()
                .unwrap()
                .ends_with(expected),
            "{error}"
        );
    }
    let request = invocation(&before, bank.version, "c", "retime", 2, None, "creep");
    let predicted = preview_framing(&package, &request, "retime")?;
    let frozen = state(&package)?;
    let dry = invoke(&package, &input, &request, true)?;
    assert_eq!(state(&package)?, frozen);
    let committed = invoke(&package, &input, &request, false)?;
    assert_eq!(
        comparable_trace(&dry, before.revision_id()),
        comparable_trace(&committed, before.revision_id())
    );
    let after = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    let mut expected = follow("subject", ratio(2, 1), ratio(1, 2));
    if let FramingValue::Follow {
        scale_transition, ..
    } = &mut expected.value
    {
        *scale_transition = Some(FollowScaleTransition {
            factor: ratio(2, 1),
            curve: ZoomCurve::Linear,
        });
    }
    assert_eq!(predicted, (Some(expected.clone()), 132));
    assert_eq!(after.nodes()[&node("retime")].framing, Some(expected));
    retained(&before, &after)?;
    let plan = RenderPlan::compile(&after)?;
    let expected_ordinals = [
        Some(11),
        Some(12),
        Some(12),
        Some(13),
        None,
        None,
        None,
        Some(70),
        Some(70),
        Some(71),
        Some(72),
        Some(72),
    ];
    let target_centers = [
        500_000, 500_000, 500_000, 600_000, 0, 0, 0, 700_000, 633_333, 600_000, 450_000, 450_000,
    ];
    for (slot, ordinal) in expected_ordinals.into_iter().enumerate() {
        let frame = i64::try_from(slot)?;
        let sample = plan.picture(ProjectFrame(frame))?;
        let shown = pose(&after, "retime", frame);
        if let Some(ordinal) = ordinal {
            let (span, numerator) = if frame < 4 {
                (spans[0], 2 * frame + 4)
            } else {
                (spans[1], 2 * frame - 14)
            };
            let point = SourcePoint {
                ticks: ratio(
                    i128::from(span.start().ticks) * 12
                        + i128::from(span.end().ticks - span.start().ticks) * i128::from(numerator),
                    12,
                ),
                time_base: span.start().time_base,
            };
            assert_eq!(sample.local_position, ratio(i128::from(numerator), 3));
            assert_eq!(sample.picture.follow_point(), Some((&asset, point)));
            let selected = sample.picture.select_source_frame(&index)?;
            assert_eq!(selected.identity.0, ordinal);
            if numerator % 3 != 0 {
                assert!(
                    point.ticks.compare_integer(selected.pts).is_gt(),
                    "fractional source point at {frame}"
                );
            }
            let x = ratio(target_centers[slot], 1_000_000);
            let x = if frame >= 7 {
                x.checked_sub(ratio(1, 2))?
                    .checked_mul(ratio(5, 4))?
                    .checked_add(ratio(1, 2))?
            } else {
                x
            };
            assert_eq!(shown.center_x, grid(x), "fractional target at {frame}");
            assert_eq!(shown.scale, scale(frame, ratio(2, 1), ratio(4, 1)));
        } else {
            assert!(matches!(sample.picture, deadpan_plan::Picture::Background));
            assert_eq!([shown.center_x, shown.center_y], [ratio(1, 3), ratio(2, 3)]);
            assert_eq!(shown.scale, scale(frame, ratio(1, 2), ratio(1, 1)));
        }
    }
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.undo(after.revision_id(), RevisionId::new("undo")?)?;
    same_document(&writer.snapshot()?, &before)?;
    writer.checkpoint()?;
    drop(writer);
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.redo(&RevisionId::new("undo")?, RevisionId::new("redo")?)?;
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
            "retime",
            2,
            None,
            "path-refused",
        ),
    )?;
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .ends_with(deadpan_core::framing_zoom::PATH_REFUSAL)
    );
    let reset = command(
        &package,
        &input,
        Command::SetFraming {
            node: node("retime"),
            framing: Some(follow("other", ratio(1, 1), ratio(1, 2))),
        },
        "other-follow",
    )?;
    let request = invocation(&reset, bank.version, "s", "retime", 8, None, "staged");
    let frozen = state(&package)?;
    let dry = invoke(&package, &input, &request, true)?;
    assert_eq!(state(&package)?, frozen);
    let committed = invoke(&package, &input, &request, false)?;
    assert_eq!(
        comparable_trace(&dry, reset.revision_id()),
        comparable_trace(&committed, reset.revision_id())
    );
    let staged = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert!(matches!(&staged.nodes()[&node("retime")].framing,
        Some(Framing {clock:FramingClock::OwnerOutput,value:FramingValue::Follow {target,scale,fallback,scale_transition:Some(FollowScaleTransition {factor,curve:ZoomCurve::Linear})}})
        if target.as_str()=="other"&&*scale==ratio(2,1)&&fallback.scale==ratio(2,1)&&*factor==ratio(2,1)));
    retained(&reset, &staged)?;
    for frame in 0..12 {
        assert_eq!(
            pose(&staged, "retime", frame).scale,
            scale(frame, ratio(2, 1), ratio(4, 1))
        );
    }
    let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    writer.undo(staged.revision_id(), RevisionId::new("staged-undo")?)?;
    same_document(&writer.snapshot()?, &reset)?;
    drop(writer);
    let plain = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    invoke(
        &package,
        &input,
        &envelope(
            &plain,
            bank.version,
            json!({"type":"apply","program":{"instructions":[instruction]},"parent":plain.root(),"selected_child":"retime","cursor":8,"visual_selection":null,"new_revision":"inline"}),
        ),
        false,
    )?;
    let inline = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
    assert!(matches!(&inline.nodes()[&node("retime")].framing,
        Some(Framing {value:FramingValue::Follow {target,scale,scale_transition:Some(FollowScaleTransition {factor,..}),..},..})
        if target.as_str()=="other"&&*scale==ratio(1,1)&&*factor==ratio(4,1)));
    retained(&plain, &inline)?;
    command(
        &package,
        &input,
        Command::SetFraming {
            node: node("retime"),
            framing: Some(follow("subject", ratio(2, 1), ratio(1, 2))),
        },
        "fixed-inner",
    )?;
    let nested = command(
        &package,
        &input,
        Command::WrapRepeat {
            node: node("retime"),
            id: node("outer"),
            plays: 2,
            gap: None,
            anchor_policy: Default::default(),
        },
        "outer",
    )?;
    let NodeKind::Repeat { iterations, .. } = &nested.nodes()[&node("outer")].kind else {
        panic!("Repeat")
    };
    let iteration = iterations.at(1).unwrap();
    for branch in [
        RepeatEditBranch::Default,
        RepeatEditBranch::Play {
            iteration: iteration.clone(),
        },
    ] {
        let explicit_play = matches!(branch, RepeatEditBranch::Play { .. });
        let scoped = SemanticScopedContext {
            root: node("outer"),
            target: ScopedNodeTarget {
                node: node("retime"),
                repeats: vec![RepeatEditStep {
                    repeat: node("outer"),
                    branch,
                }],
            },
            presentation: InstancePath {
                node: node("retime"),
                repeats: vec![RepeatInstance {
                    node: node("outer"),
                    iteration: iteration.clone(),
                }],
            },
        };
        let mut request = invocation(
            &nested,
            bank.version,
            "c",
            "outer",
            14,
            None,
            "scoped-refused",
        );
        request["operation"]["scoped"] = serde_json::to_value(scoped)?;
        if explicit_play {
            let result = invoke(&package, &input, &request, false)?;
            let (after, mapped, frames) = assert_scoped_follow_commit(&package, &nested, &result)?;
            assert_eq!(frames, (12..24).collect::<Vec<_>>());
            assert!(matches!(&after.nodes()[&mapped.target.node].framing,
                Some(Framing { value: FramingValue::Follow { target, scale, scale_transition: Some(FollowScaleTransition { factor, .. }), .. }, .. })
                if target.as_str() == "subject" && *scale == ratio(2, 1) && *factor == ratio(2, 1)));
            let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
            writer.undo(after.revision_id(), RevisionId::new("scoped-retime-undo")?)?;
            same_document(&writer.snapshot()?, &nested)?;
            drop(writer);
        } else {
            let error = reject(&package, &input, &request)?;
            assert!(
                error["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("single-play"),
                "{error}"
            );
        }
    }
    assert_eq!(
        ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?,
        bank
    );
    Ok(())
}
