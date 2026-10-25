//! Repeat-wide Follow scale and per-provider target clocks stay independent.
use super::follow_creep::follow;
use super::*;
use deadpan_core::{
    AttentionTarget, AudioSample, AudioTimingId, FollowScaleTransition, FrameDuration, Framing,
    FramingClock, GainDb, HoldAudio, HoldRecipe, HoldVideo, InstancePath, NodeKind,
    RepeatEditBranch, RepeatEditStep, RepeatEscalation, RepeatGapHold, RepeatInstance,
    ScopedNodeTarget, SemanticScopedContext, SourcePoint, SourceSpan, SourceTimestamp, SourceVideo,
    SourceVideoMapping, Subtree, TargetCorrection, TargetId, TargetPoint, TargetSample, TrackState,
    ZoomProgression, ZoomStep,
};

const Q: i128 = 1_i128 << 32;

fn round_even(n: i128, d: i128) -> i128 {
    assert!(n >= 0 && d > 0);
    let whole = n / d;
    let rem = n % d;
    whole + i128::from(rem * 2 > d || (rem * 2 == d && whole % 2 != 0))
}

fn grid(value: ExactRatio) -> ExactRatio {
    ratio(round_even(value.numerator() * Q, value.denominator()), Q)
}

fn scale(frame: i64, low: ExactRatio, high: ExactRatio) -> ExactRatio {
    let progress = round_even((2 * i128::from(frame) + 1) * Q, 34);
    let low = round_even(low.numerator() * Q, low.denominator());
    let high = round_even(high.numerator() * Q, high.denominator());
    ratio(round_even(low * (Q - progress) + high * progress, Q), Q)
}

fn location(frame: i64, owned_first: bool) -> (u32, Option<i64>) {
    if owned_first {
        return match frame {
            0..4 => (0, Some(frame)),
            4..7 => (0, None),
            7..11 => (1, Some(frame - 7)),
            11..13 => (1, None),
            13..17 => (2, Some(frame - 13)),
            _ => panic!("outside Repeat"),
        };
    }
    match frame {
        0..4 => (0, Some(frame)),
        4..6 => (0, None),
        6..10 => (1, Some(frame - 6)),
        10..13 => (1, None),
        13..17 => (2, Some(frame - 13)),
        _ => panic!("outside Repeat"),
    }
}

fn target_x(local: i64) -> ExactRatio {
    match local {
        0 => ratio(1, 2),     // midpoint interpolation .4 -> .6
        1 | 2 => ratio(3, 5), // Lost record retains last confident .6
        3 => ratio(9, 20),    // manual correction at Original picture 3
        _ => unreachable!(),
    }
}

fn escalation_center(x: ExactRatio, zoom: ExactRatio, targeted: bool) -> ExactRatio {
    if !targeted {
        return ratio(1, 2);
    }
    // Independent fill constraint: quantize the resolved center, clamp the
    // view inside the canvas, then quantize that constrained center.
    let x = grid(x);
    let margin = ExactRatio::ONE
        .checked_div(zoom)
        .unwrap()
        .checked_div(ratio(2, 1))
        .unwrap();
    let upper = ExactRatio::ONE.checked_sub(margin).unwrap();
    grid(if x.compare(margin).is_lt() {
        margin
    } else if x.compare(upper).is_gt() {
        upper
    } else {
        x
    })
}

fn assert_only_repeat(
    before: &ProjectDocument,
    after: &ProjectDocument,
    escalation: bool,
    targeted: bool,
    source: SourceSpan,
) -> Result {
    let mut expected = serde_json::to_value(before)?;
    expected["revision_id"] = json!(after.revision_id());
    expected["nodes"]["repeat"]["framing"] =
        serde_json::to_value(&after.nodes()[&node("repeat")].framing)?;
    assert_eq!(serde_json::to_value(after)?, expected);
    assert_eq!(after.duration()?.frames(), 137);
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
    let asset = AssetId::new("camera")?;
    let NodeKind::Repeat { iterations, .. } = &before.nodes()[&node("repeat")].kind else {
        panic!("Repeat")
    };
    for frame in 0..137 {
        let a = baseline.picture(ProjectFrame(frame))?;
        let b = edited.picture(ProjectFrame(frame))?;
        assert_eq!(a.picture, b.picture, "picture {frame}");
        assert_eq!(a.picture_context, b.picture_context, "capture {frame}");
        assert_eq!(a.instance, b.instance, "instance {frame}");
        assert_eq!(a.local_position, b.local_position, "provider clock {frame}");
        assert_eq!(a.gap_after, b.gap_after);
        assert_eq!(a.framing.len(), b.framing.len());
        let mut outer = 0;
        for (old, new) in a.framing.iter().zip(&b.framing) {
            let mut expected = old.clone();
            if new.instance.node == node("repeat") && !new.escalation {
                outer += 1;
                assert_eq!(new.local_position, ratio(2 * i128::from(frame) + 1, 2));
                assert_eq!(new.effective_duration, ratio(17, 1));
                expected.pose = new.pose;
            }
            assert_eq!(&expected, new, "layer {frame}");
        }
        assert_eq!(outer, usize::from(frame < 17));
        if frame >= 17 {
            assert_eq!(b.instance.node, node("camera-source"));
            continue;
        }
        let (play, local) = location(frame, targeted);
        let owned = if targeted {
            (4..7).contains(&frame)
        } else {
            (10..13).contains(&frame)
        };
        assert_eq!(
            b.instance.node,
            node(if owned {
                "owned-gap"
            } else if local.is_some() {
                "body"
            } else {
                "repeat"
            })
        );
        if let Some(local) = local {
            assert_eq!(
                b.instance.repeats.last().unwrap().iteration,
                iterations.at(play).unwrap()
            );
            let point = SourcePoint {
                ticks: ratio(
                    i128::from(source.start().ticks) * 8
                        + i128::from(source.end().ticks - source.start().ticks)
                            * (2 * i128::from(local) + 1),
                    8,
                ),
                time_base: source.start().time_base,
            };
            assert_eq!(b.picture.follow_point(), Some((&asset, point)));
        } else if owned {
            assert_eq!(b.gap_after, None);
            assert_eq!(
                b.instance.repeats.last().unwrap().iteration,
                iterations.at(play).unwrap()
            );
            assert!(matches!(b.picture, deadpan_plan::Picture::Background));
        } else {
            assert_eq!(b.gap_after, Some(iterations.at(play).unwrap()));
            assert_eq!(
                b.picture.follow_point(),
                Some((
                    &asset,
                    SourcePoint {
                        ticks: ratio(
                            i128::from(source.start().ticks) * 4
                                + i128::from(source.end().ticks - source.start().ticks),
                            4
                        ),
                        time_base: source.start().time_base,
                    }
                ))
            );
        }
        let inner = b
            .framing
            .iter()
            .filter(|layer| layer.escalation)
            .collect::<Vec<_>>();
        assert_eq!(inner.len(), usize::from(escalation && play > 0));
        let x = if let Some(local) = local {
            target_x(local)
        } else {
            ratio(3, 5)
        };
        let zoom = ratio(8 + i128::from(play), 8);
        if let Some(inner) = inner.first() {
            assert_eq!(inner.instance.node, node("repeat"));
            let shown = inner.pose.unwrap();
            assert_eq!(shown.scale, zoom);
            assert_eq!(
                shown.center_x,
                if owned {
                    ratio(1, 2)
                } else {
                    escalation_center(x, zoom, targeted)
                }
            );
        }
        let shown = pose(after, "repeat", frame);
        if owned {
            assert_eq!([shown.center_x, shown.center_y], [ratio(1, 3), ratio(2, 3)]);
            assert_eq!(shown.scale, scale(frame, ratio(1, 2), ratio(1, 1)));
        } else {
            let center = if escalation && play > 0 {
                grid(
                    x.checked_sub(escalation_center(x, zoom, targeted))?
                        .checked_mul(zoom)?
                        .checked_add(ratio(1, 2))?,
                )
            } else {
                grid(x)
            };
            assert_eq!(shown.center_x, center, "target clock at {frame}");
            assert_eq!(shown.scale, scale(frame, ratio(2, 1), ratio(4, 1)));
        }
    }
    for frame in [-1, 137] {
        assert!(baseline.picture(ProjectFrame(frame)).is_err());
        assert!(edited.picture(ProjectFrame(frame)).is_err());
    }
    Ok(())
}

#[test]
fn whole_repeat_follow_creep_preserves_plays_gaps_escalation_and_invocation_relative_history()
-> Result {
    for variant in ["plain", "centered", "targeted"] {
        let scratch = tempfile::tempdir()?;
        let package = qualified_project(scratch.path())?;
        let input = scratch.path().join("repeat-follow.json");
        let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
        let original = store.snapshot()?;
        let asset = AssetId::new("camera")?;
        let index = store.source_video_index(original.revision_id(), &asset)?;
        let stamp = |i: usize| SourceTimestamp {
            ticks: index.frames()[i].pts,
            time_base: index.time_base(),
        };
        let source = SourceSpan::new(stamp(0), stamp(4))?;
        let times = [
            stamp(0).ticks,
            stamp(1).ticks,
            stamp(2).ticks,
            stamp(3).ticks,
        ];
        let freeze = stamp(1);
        drop(store);
        let point = |x| {
            TargetPoint {
                center: [x, 500_000],
            }
            .into()
        };
        for id in ["subject", "other"] {
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
                        samples: if id == "subject" {
                            vec![
                                TargetSample {
                                    at: times[0],
                                    region: point(400_000),
                                    confidence: 1000,
                                    state: TrackState::Tracked,
                                },
                                TargetSample {
                                    at: times[1],
                                    region: point(600_000),
                                    confidence: 1000,
                                    state: TrackState::Tracked,
                                },
                                TargetSample {
                                    at: times[2],
                                    region: point(600_000),
                                    confidence: 0,
                                    state: TrackState::Lost,
                                },
                            ]
                        } else {
                            vec![]
                        },
                        corrections: if id == "subject" {
                            vec![TargetCorrection {
                                at: times[3],
                                region: point(450_000),
                            }]
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
        let mut body = original.nodes()[&node("camera-source")].clone();
        let NodeKind::Source {
            source: input_source,
        } = &mut body.kind
        else {
            panic!("Source")
        };
        input_source.duration = FrameDuration::new(4)?;
        input_source.edit_window = None;
        input_source.video = SourceVideo::Stream {
            asset: asset.clone(),
            span: source,
        };
        input_source.video_mapping = SourceVideoMapping::FitBeat;
        assert!(input_source.audio.is_none());
        command(
            &package,
            &input,
            Command::Insert {
                parent: original.root().clone(),
                index: 0,
                subtree: Subtree {
                    root: node("body"),
                    nodes: BTreeMap::from([(node("body"), body)]),
                    overrides: Default::default(),
                    gap_overrides: Default::default(),
                },
            },
            "body",
        )?;
        let gap = HoldRecipe {
            duration: FrameDuration::new(2)?,
            picture_context: None,
            video: HoldVideo::Freeze {
                asset: asset.clone(),
                timestamp: freeze,
            },
            audio: HoldAudio::Silence,
        };
        let wrapped = command(
            &package,
            &input,
            Command::WrapRepeat {
                node: node("body"),
                id: node("repeat"),
                plays: 3,
                gap: Some(gap.clone()),
                anchor_policy: Default::default(),
            },
            "repeat",
        )?;
        let NodeKind::Repeat { iterations, .. } = &wrapped.nodes()[&node("repeat")].kind else {
            panic!("Repeat")
        };
        command(
            &package,
            &input,
            Command::SetRepeatGaps {
                node: node("repeat"),
                gap: Some(gap),
                branches: vec![RepeatGapHold {
                    // Swap gap ownership in the targeted variant to exercise
                    // nonzero escalation on both default and owned gaps.
                    after: iterations.at(u32::from(variant != "targeted")).unwrap(),
                    id: node("owned-gap"),
                    hold: HoldRecipe {
                        duration: FrameDuration::new(3)?,
                        picture_context: None,
                        video: HoldVideo::Background,
                        audio: HoldAudio::Silence,
                    },
                }],
                timing: AudioTimingId {
                    allocation: RevisionId::new("gaps")?,
                    ordinal: 0,
                },
            },
            "gaps",
        )?;
        let escalation = variant != "plain";
        let targeted = variant == "targeted";
        if escalation {
            command(
                &package,
                &input,
                Command::SetRepeatEscalation {
                    node: node("repeat"),
                    escalation: Some(RepeatEscalation {
                        gain_step: GainDb::new(500)?,
                        zoom: Some(ZoomStep {
                            step: ratio(1, 8),
                            progression: ZoomProgression::Add,
                            target: targeted.then(|| TargetId::new("subject").unwrap()),
                        }),
                    }),
                },
                "escalation",
            )?;
        }
        let before = command(
            &package,
            &input,
            Command::SetFraming {
                node: node("repeat"),
                framing: Some(follow("subject", ratio(2, 1), ratio(1, 2))),
            },
            "follow",
        )?;
        assert_eq!(before.node_duration(&node("repeat"))?.frames(), 17);
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
            Some((0, 17, false)),
            Some((1, 16, false)),
            Some((17, 0, true)),
        ] {
            let cursor = selection
                .filter(|(_, _, extending)| *extending)
                .map_or(2, |(_, head, _)| head);
            assert_ranged_follow_preview(
                &package,
                &input,
                &invocation(
                    &before,
                    bank.version,
                    "c",
                    "repeat",
                    cursor,
                    selection,
                    "range-preview",
                ),
                "repeat",
                0,
                17,
                selection.map(|(a, b, _)| (a, b)).unwrap(),
            )?;
        }
        let request = invocation(&before, bank.version, "c", "repeat", 2, None, "creep");
        let predicted = preview_framing(&package, &request, "repeat")?;
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
                range: None,
            });
        }
        assert_eq!(predicted, (Some(expected.clone()), 137));
        assert_eq!(after.nodes()[&node("repeat")].framing, Some(expected));
        assert_only_repeat(&before, &after, escalation, targeted, source)?;
        let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
        writer.undo(after.revision_id(), RevisionId::new("undo")?)?;
        same_document(&writer.snapshot()?, &before)?;
        writer.checkpoint()?;
        drop(writer);
        let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
        writer.redo(&RevisionId::new("undo")?, RevisionId::new("redo")?)?;
        same_document(&writer.snapshot()?, &after)?;
        assert_eq!(writer.registers()?, bank);
        let redone = writer.snapshot()?;
        writer.validate()?;
        drop(writer);
        let error = reject(
            &package,
            &input,
            &invocation(
                &redone,
                bank.version,
                "c",
                "repeat",
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
                node: node("repeat"),
                framing: Some(follow("other", ratio(1, 1), ratio(1, 2))),
            },
            "other-follow",
        )?;
        let request = invocation(&reset, bank.version, "s", "repeat", 7, None, "staged");
        let frozen = state(&package)?;
        let dry = invoke(&package, &input, &request, true)?;
        assert_eq!(state(&package)?, frozen);
        let committed = invoke(&package, &input, &request, false)?;
        assert_eq!(
            comparable_trace(&dry, reset.revision_id()),
            comparable_trace(&committed, reset.revision_id())
        );
        let staged = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        assert!(matches!(&staged.nodes()[&node("repeat")].framing,
            Some(Framing { clock: FramingClock::OwnerOutput, value: FramingValue::Follow { target, scale, fallback,
                scale_transition: Some(FollowScaleTransition { factor, curve: ZoomCurve::Linear, range: None }) } })
            if target.as_str() == "other" && *scale == ratio(2, 1) && fallback.scale == ratio(2, 1) && *factor == ratio(2, 1)));
        for frame in 0..17 {
            assert_eq!(
                pose(&staged, "repeat", frame).scale,
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
                json!({
                    "type":"apply", "program":{"instructions":[instruction]}, "parent":plain.root(),
                    "selected_child":"repeat", "cursor":7, "visual_selection":null, "new_revision":"inline"
                }),
            ),
            false,
        )?;
        let inlined = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        assert!(matches!(&inlined.nodes()[&node("repeat")].framing,
            Some(Framing { value: FramingValue::Follow { target, scale, scale_transition: Some(FollowScaleTransition { factor, .. }), .. }, .. })
            if target.as_str() == "other" && *scale == ratio(1, 1) && *factor == ratio(4, 1)));
        if variant == "plain" {
            // The selected ordinary Retime is now an authoring owner too; its
            // independently animated Repeat child and all timing stay intact.
            command(
                &package,
                &input,
                Command::WrapRetime {
                    node: node("repeat"),
                    id: node("retime"),
                    duration: FrameDuration::new(17)?,
                    pitch: PitchPolicy::FollowSpeed,
                },
                "retime",
            )?;
            let retimed = command(
                &package,
                &input,
                Command::SetFraming {
                    node: node("retime"),
                    framing: Some(follow("subject", ratio(2, 1), ratio(1, 2))),
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
                    2,
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
                if target.as_str() == "subject" && *scale == ratio(2, 1) && *factor == ratio(2, 1)));
            let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
            writer.undo(
                animated.revision_id(),
                RevisionId::new("retime-creep-undo")?,
            )?;
            same_document(&writer.snapshot()?, &retimed)?;
            drop(writer);
            command(
                &package,
                &input,
                Command::SetFraming {
                    node: node("repeat"),
                    framing: Some(follow("subject", ratio(2, 1), ratio(1, 2))),
                },
                "inner-fixed",
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
            let scoped = SemanticScopedContext {
                root: node("outer"),
                target: ScopedNodeTarget {
                    node: node("repeat"),
                    repeats: vec![RepeatEditStep {
                        repeat: node("outer"),
                        branch: RepeatEditBranch::Play {
                            iteration: iteration.clone(),
                        },
                    }],
                },
                presentation: InstancePath {
                    node: node("repeat"),
                    repeats: vec![RepeatInstance {
                        node: node("outer"),
                        iteration,
                    }],
                },
            };
            let mut request =
                invocation(&nested, bank.version, "c", "outer", 20, None, "inner-creep");
            request["operation"]["scoped"] = serde_json::to_value(scoped)?;
            let result = invoke(&package, &input, &request, false)?;
            let (after, mapped, frames) = assert_scoped_follow_commit(&package, &nested, &result)?;
            assert_eq!(frames, (17..34).collect::<Vec<_>>());
            assert!(matches!(&after.nodes()[&mapped.target.node].framing,
                Some(Framing { value: FramingValue::Follow { target, scale, scale_transition: Some(FollowScaleTransition { factor, .. }), .. }, .. })
                if target.as_str() == "subject" && *scale == ratio(2, 1) && *factor == ratio(2, 1)));
            let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
            writer.undo(after.revision_id(), RevisionId::new("scoped-repeat-undo")?)?;
            same_document(&writer.snapshot()?, &nested)?;
            drop(writer);
        }
        assert_eq!(
            ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?,
            bank
        );
    }
    Ok(())
}
