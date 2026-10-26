//! Validated nested Play authority retains live Follow through exact isolation.
use super::follow_creep::follow;
use super::*;
use deadpan_core::{
    AttentionTarget, BeatNode, CapturedCanvas, CapturedFit, CapturedFraming, FrameDuration,
    Framing, FramingClock, HoldAudio, HoldRecipe, HoldVideo, InstancePath, IterationOrder,
    NodeKind, RepeatEditBranch, RepeatEditStep, RepeatEscalation, RepeatInstance, RetimePurpose,
    ScopedNodeTarget, SemanticScopedContext, SourcePoint, SourceSpan, SourceTimestamp, SourceVideo,
    SourceVideoMapping, Subtree, TargetCorrection, TargetId, TargetPoint, TargetSample, TrackState,
    ZoomProgression, ZoomStep,
};

const Q: i128 = 1_i128 << 32;

fn round(n: i128, d: i128) -> i128 {
    let whole = n / d;
    let remainder = n % d;
    whole + i128::from(remainder * 2 > d || (remainder * 2 == d && whole % 2 != 0))
}

fn grid(x: ExactRatio) -> ExactRatio {
    ratio(round(x.numerator() * Q, x.denominator()), Q)
}

fn scale(k: i64, low: ExactRatio, high: ExactRatio) -> ExactRatio {
    let p = round((2 * i128::from(k) + 1) * Q, 18);
    let low = round(low.numerator() * Q, low.denominator());
    let high = round(high.numerator() * Q, high.denominator());
    ratio(round(low * (Q - p) + high * p, Q), Q)
}

fn authority(document: &ProjectDocument, outer: u32, inner: u32) -> SemanticScopedContext {
    let path = [("outer", outer), ("inner", inner)].map(|(id, play)| {
        let NodeKind::Repeat { iterations, .. } = &document.nodes()[&node(id)].kind else {
            panic!("Repeat")
        };
        RepeatInstance {
            node: node(id),
            iteration: iterations.at(play).unwrap(),
        }
    });
    SemanticScopedContext {
        root: node("outer"),
        target: ScopedNodeTarget {
            node: node("owner"),
            repeats: path
                .iter()
                .map(|entry| RepeatEditStep {
                    repeat: entry.node.clone(),
                    branch: RepeatEditBranch::Play {
                        iteration: entry.iteration.clone(),
                    },
                })
                .collect(),
        },
        presentation: InstancePath {
            node: node("owner"),
            repeats: path.to_vec(),
        },
    }
}

fn request(
    document: &ProjectDocument,
    version: u64,
    register: &str,
    scoped: &SemanticScopedContext,
    frame: i64,
    revision: &str,
) -> Value {
    let mut value = invocation(document, version, register, "outer", frame, None, revision);
    value["operation"]["scoped"] = serde_json::to_value(scoped).unwrap();
    value
}

fn plan(package: &Path, request: &Value) -> Result<deadpan_cli::macros::HostSemanticPlan> {
    let parsed = Request::from_json(&serde_json::to_vec(request)?)?;
    deadpan_cli::macros::prepare(&ProjectStore::open(package, AccessMode::ReadOnly)?, &parsed)?
        .plan
        .ok_or_else(|| "missing scoped semantic plan".into())
}

#[test]
fn scoped_follow_creep_all_five_owners_keep_nested_play_clocks_isolation_and_durable_history()
-> Result {
    for kind in ["source", "hold", "sequence", "repeat", "retime"] {
        let scratch = tempfile::tempdir()?;
        let package = qualified_project(scratch.path())?;
        let input = scratch.path().join("scoped-follow.json");
        let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
        let original = store.snapshot()?;
        let asset = AssetId::new("camera")?;
        let index = store.source_video_index(original.revision_id(), &asset)?;
        let stamp = |i: usize| SourceTimestamp {
            ticks: index.frames()[i].pts,
            time_base: index.time_base(),
        };
        let tick_step = index.frames()[1].pts - index.frames()[0].pts;
        assert!(
            index.frames().iter().enumerate().all(|(i, frame)| frame.pts
                == index.frames()[0].pts + i64::try_from(i).unwrap() * tick_step)
        );
        let receipt = store.registered_source(original.revision_id(), &asset)?;
        let stream = receipt.snapshot().video().unwrap().interpretation();
        let visible = stream.visible_bounds()?;
        let aspect = deadpan_core::display_aspect(
            [visible[2], visible[3]],
            [stream.sample_aspect_num, stream.sample_aspect_den],
            stream.rotation_quarter_turns,
        )
        .ok_or("no measured aspect")?;
        let basis = original.presentation_basis();
        assert!(
            aspect
                .compare(ratio(i128::from(basis.width), i128::from(basis.height)))
                .is_ge()
        );
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
                                    at: stamp(0).ticks,
                                    region: point(400_000),
                                    confidence: 1000,
                                    state: TrackState::Tracked,
                                },
                                TargetSample {
                                    at: stamp(1).ticks,
                                    region: point(600_000),
                                    confidence: 1000,
                                    state: TrackState::Tracked,
                                },
                                TargetSample {
                                    at: stamp(2).ticks,
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
                                at: stamp(3).ticks,
                                region: point(700_000),
                            }]
                        } else {
                            vec![]
                        },
                        provenance: None,
                        display_aspect: Some(aspect),
                    },
                },
                &format!("target-{id}"),
            )?;
        }
        let source = |start: usize, frames: i64| -> Result<BeatNode> {
            let mut node = original.nodes()[&node("camera-source")].clone();
            let NodeKind::Source { source } = &mut node.kind else {
                panic!("Source")
            };
            source.duration = FrameDuration::new(frames)?;
            source.edit_window = None;
            source.video = SourceVideo::Stream {
                asset: asset.clone(),
                span: SourceSpan::new(stamp(start), stamp(start + usize::try_from(frames)?))?,
            };
            source.video_mapping = SourceVideoMapping::FitBeat;
            assert!(source.audio.is_none());
            Ok(node)
        };
        let blank = |frames| -> Result<BeatNode> {
            Ok(BeatNode::hold(
                "Background",
                HoldRecipe {
                    duration: FrameDuration::new(frames)?,
                    picture_context: None,
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                },
            ))
        };
        let mut nodes = BTreeMap::new();
        let mut owner = match kind {
            "source" => source(0, 6)?,
            "hold" => BeatNode::hold(
                "Clipped captured Follow",
                HoldRecipe {
                    duration: FrameDuration::new(6)?,
                    picture_context: Some(CapturedFraming::new(vec![CapturedCanvas {
                        width: basis.width,
                        height: basis.height,
                        fit: CapturedFit::Fit,
                        layers: vec![Some(FramingPose::new(
                            ratio(1, 2),
                            ratio(1, 2),
                            ratio(4, 1),
                        )?)],
                    }])?),
                    video: HoldVideo::Freeze {
                        asset: asset.clone(),
                        timestamp: stamp(3),
                    },
                    audio: HoldAudio::Silence,
                },
            ),
            "sequence" => {
                nodes.insert(node("early"), source(0, 2)?);
                nodes.insert(node("blank"), blank(2)?);
                let mut late = source(4, 2)?;
                late.framing = Some(Framing::static_pose(FramingPose::new(
                    ratio(1, 2),
                    ratio(1, 2),
                    ratio(4, 1),
                )?)?);
                nodes.insert(node("late"), late);
                BeatNode::sequence(
                    "Sequence owner",
                    vec![node("early"), node("blank"), node("late")],
                )
            }
            "repeat" => {
                nodes.insert(node("provider"), source(0, 2)?);
                let mut owner = BeatNode::sequence("Repeat owner", vec![]);
                owner.kind = NodeKind::Repeat {
                    child: node("provider"),
                    iterations: IterationOrder::new(RevisionId::new("owner-plays")?, 2)?,
                    gap: Some(HoldRecipe {
                        duration: FrameDuration::new(2)?,
                        picture_context: None,
                        video: HoldVideo::Freeze {
                            asset: asset.clone(),
                            timestamp: stamp(2),
                        },
                        audio: HoldAudio::Silence,
                    }),
                    escalation: Some(RepeatEscalation {
                        gain_step: Default::default(),
                        zoom: Some(ZoomStep {
                            step: ratio(1, 8),
                            progression: ZoomProgression::Add,
                            target: None,
                        }),
                    }),
                };
                owner
            }
            "retime" => {
                nodes.insert(node("provider"), source(0, 4)?);
                let mut owner = BeatNode::sequence("Retime owner", vec![]);
                owner.kind = NodeKind::Retime {
                    child: node("provider"),
                    duration: FrameDuration::new(6)?,
                    mapping: deadpan_core::FrameRange::new(ProjectFrame(0), ProjectFrame(4))?,
                    pitch: PitchPolicy::FollowSpeed,
                    purpose: RetimePurpose::Edit,
                };
                owner
            }
            _ => unreachable!(),
        };
        owner.framing = Some(follow("subject", ratio(2, 1), ratio(1, 2)));
        nodes.insert(node("owner"), owner);
        let mut inner = blank(1)?;
        inner.kind = NodeKind::Repeat {
            child: node("owner"),
            iterations: IterationOrder::new(RevisionId::new("inner-plays")?, 2)?,
            gap: None,
            escalation: None,
        };
        nodes.insert(node("inner"), inner);
        let mut speed = blank(1)?;
        speed.kind = NodeKind::Retime {
            child: node("inner"),
            duration: FrameDuration::new(18)?,
            mapping: deadpan_core::FrameRange::new(ProjectFrame(0), ProjectFrame(12))?,
            pitch: PitchPolicy::FollowSpeed,
            purpose: RetimePurpose::Edit,
        };
        nodes.insert(node("speed"), speed);
        let mut outer = blank(1)?;
        outer.kind = NodeKind::Repeat {
            child: node("speed"),
            iterations: IterationOrder::new(RevisionId::new("outer-plays")?, 2)?,
            gap: Some(match blank(2)?.kind {
                NodeKind::Hold { recipe } => recipe,
                _ => unreachable!(),
            }),
            escalation: None,
        };
        nodes.insert(node("outer"), outer);
        drop(store);
        let before = command(
            &package,
            &input,
            Command::Insert {
                parent: original.root().clone(),
                index: 0,
                subtree: Subtree {
                    root: node("outer"),
                    nodes,
                    overrides: Default::default(),
                    gap_overrides: Default::default(),
                },
            },
            "fixture",
        )?;
        assert_eq!(before.node_duration(&node("owner"))?.frames(), 6);
        assert_eq!(before.node_duration(&node("outer"))?.frames(), 38);
        let creep = zoom(
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
            &save(&before, 0, "c", serde_json::to_value([creep.clone()])?),
            false,
        )?;
        invoke(
            &package,
            &input,
            &save(
                &before,
                1,
                "s",
                serde_json::to_value([
                    zoom(
                        ZoomKind::Smash { scale: ratio(2, 1) },
                        SemanticZoomCenter::Keep,
                    ),
                    I::Call {
                        register: RegisterName::new('c')?,
                        count: count(1),
                    },
                ])?,
            ),
            false,
        )?;
        let bank = ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?;
        let scope = authority(&before, 1, 1);
        for variant in 0..7 {
            let mut bad = request(&before, bank.version, "c", &scope, 30, "refused");
            match variant {
                0 => {
                    bad["operation"]["scoped"]["target"]["repeats"][0]["branch"] =
                        json!({"type":"default"})
                }
                1 => {
                    bad["operation"]["scoped"]["target"]["repeats"][1]["branch"] =
                        json!({"type":"default"})
                }
                2 => bad["operation"]["scoped"]["target"]["repeats"] = json!([]),
                3 => {
                    bad["operation"]["scoped"]["presentation"]["repeats"][0]["iteration"] =
                        serde_json::to_value(
                            authority(&before, 0, 1).presentation.repeats[0]
                                .iteration
                                .clone(),
                        )?
                }
                4 => {
                    bad["operation"]["visual_selection"] =
                        json!({"type":"time","anchor":28,"head":38,"extending":false})
                }
                5 => {
                    bad["operation"]["scoped"] = Value::Null;
                    bad["operation"]["selected_child"] = json!("owner");
                }
                6 => bad["operation"]["scoped"]["root"] = json!("inner"),
                _ => unreachable!(),
            }
            reject(&package, &input, &bad)?;
        }
        let call = request(&before, bank.version, "c", &scope, 30, "creep");
        let predicted = plan(&package, &call)?;
        let frozen = state(&package)?;
        let dry = invoke(&package, &input, &call, true)?;
        assert_eq!(state(&package)?, frozen);
        let result = invoke(&package, &input, &call, false)?;
        assert_eq!(dry["duration_delta"], result["duration_delta"]);
        assert_eq!(
            dry["trace"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| &row["instruction"])
                .collect::<Vec<_>>(),
            result["trace"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| &row["instruction"])
                .collect::<Vec<_>>()
        );
        let (after, mapped, frames) = assert_scoped_follow_commit(&package, &before, &result)?;
        assert_eq!(frames, (29..38).collect::<Vec<_>>(), "{kind}");
        let predicted_scope = predicted.context.scoped.as_ref().unwrap();
        assert_eq!(
            after.nodes()[&mapped.target.node].framing,
            predicted.document.nodes()[&predicted_scope.target.node].framing
        );
        let output = RenderPlan::compile(&after)?;
        for k in 0..9 {
            let local = ratio(2 * i128::from(k) + 1, 3);
            let sample = output.picture(ProjectFrame(29 + k))?;
            let layer = sample
                .framing
                .iter()
                .find(|layer| !layer.escalation && layer.instance == mapped.presentation)
                .unwrap();
            assert_eq!(layer.local_position, local);
            assert_eq!(layer.effective_duration, ratio(6, 1));
            let (point, clipped, escalated) = match kind {
                "source" => (Some(local), false, false),
                "hold" => (Some(ratio(3, 1)), true, false),
                "sequence" if local.compare_integer(2).is_lt() => (Some(local), false, false),
                "sequence" if local.compare_integer(4).is_lt() => (None, true, false),
                "sequence" => (Some(local), true, false),
                "repeat" if local.compare_integer(2).is_lt() => (Some(local), false, false),
                "repeat" if local.compare_integer(4).is_lt() => (Some(ratio(2, 1)), false, false),
                "repeat" => (Some(local.checked_sub(ratio(4, 1))?), false, true),
                "retime" => (Some(local.checked_mul(ratio(2, 3))?), false, false),
                _ => unreachable!(),
            };
            let shown = layer.pose.unwrap();
            if let Some(coordinate) = point {
                let expected = SourcePoint {
                    ticks: ratio(i128::from(index.frames()[0].pts), 1)
                        .checked_add(coordinate.checked_mul(ratio(i128::from(tick_step), 1))?)?,
                    time_base: index.time_base(),
                };
                assert_eq!(sample.picture.follow_point(), Some((&asset, expected)));
                let ordinal = coordinate.floor();
                assert_eq!(
                    i128::from(sample.picture.select_source_frame(&index)?.identity.0),
                    ordinal
                );
                if !clipped {
                    let units = if coordinate.compare_integer(1).is_lt() {
                        400_000 + round(200_000 * coordinate.numerator(), coordinate.denominator())
                    } else if coordinate.compare_integer(3).is_lt() {
                        600_000
                    } else {
                        700_000
                    };
                    let x = ratio(units, 1_000_000);
                    let x = if escalated {
                        x.checked_sub(ratio(1, 2))?
                            .checked_mul(ratio(9, 8))?
                            .checked_add(ratio(1, 2))?
                    } else {
                        x
                    };
                    assert_eq!(shown.center_x, grid(x), "{kind}/{k}");
                }
            } else {
                assert!(sample.picture.follow_point().is_none());
            }
            if clipped {
                assert_eq!([shown.center_x, shown.center_y], [ratio(1, 3), ratio(2, 3)]);
                assert_eq!(shown.scale, scale(k, ratio(1, 2), ratio(1, 1)));
            } else {
                assert_eq!(shown.scale, scale(k, ratio(2, 1), ratio(4, 1)));
            }
        }
        let retune = request(&after, bank.version, "c", &mapped, 30, "animated-preview");
        let predicted = plan(&package, &retune)?;
        let predicted_scope = predicted.context.scoped.as_ref().unwrap();
        assert_eq!(
            predicted.document.nodes()[&predicted_scope.target.node].framing,
            after.nodes()[&mapped.target.node].framing,
        );
        let frozen = state(&package)?;
        invoke(&package, &input, &retune, true)?;
        assert_eq!(state(&package)?, frozen);
        let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
        writer.undo(after.revision_id(), RevisionId::new("undo")?)?;
        same_document(&writer.snapshot()?, &before)?;
        writer.checkpoint()?;
        drop(writer);
        let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
        writer.redo(&RevisionId::new("undo")?, RevisionId::new("redo")?)?;
        same_document(&writer.snapshot()?, &after)?;
        assert_eq!(writer.registers()?, bank);
        writer.undo(&RevisionId::new("redo")?, RevisionId::new("fresh")?)?;
        same_document(&writer.snapshot()?, &before)?;
        drop(writer);
        // The saved body contains Keep, never captured IDs. Rebind a different
        // concrete play to another current Follow before the staged call.
        let fresh = command(
            &package,
            &input,
            Command::SetFraming {
                node: node("owner"),
                framing: Some(follow("other", ratio(1, 1), ratio(1, 2))),
            },
            "other-follow",
        )?;
        let next_scope = authority(&fresh, 0, 0);
        let staged = request(&fresh, bank.version, "s", &next_scope, 1, "staged");
        let frozen = state(&package)?;
        invoke(&package, &input, &staged, true)?;
        assert_eq!(state(&package)?, frozen);
        let result = invoke(&package, &input, &staged, false)?;
        let (staged, mapped, frames) = assert_scoped_follow_commit(&package, &fresh, &result)?;
        assert_eq!(frames, (0..9).collect::<Vec<_>>());
        assert!(
            matches!(&staged.nodes()[&mapped.target.node].framing, Some(Framing { clock: FramingClock::OwnerOutput, value: FramingValue::Follow { target, scale, fallback, centering: deadpan_core::FollowCentering::Unconstrained, scale_transition: Some(_) } }) if target.as_str() == "other" && *scale == ratio(2, 1) && fallback.scale == ratio(2, 1))
        );
        let mut writer = ProjectStore::open(&package, AccessMode::ReadWrite)?;
        writer.undo(staged.revision_id(), RevisionId::new("staged-undo")?)?;
        same_document(&writer.snapshot()?, &fresh)?;
        drop(writer);
        let restored = ProjectStore::open(&package, AccessMode::ReadOnly)?.snapshot()?;
        let inline = envelope(
            &restored,
            bank.version,
            json!({"type":"apply","program":{"instructions":[creep]},"parent":restored.root(),"selected_child":"outer","cursor":1,"scoped":next_scope,"new_revision":"inline"}),
        );
        let result = invoke(&package, &input, &inline, false)?;
        let (inlined, mapped, _) = assert_scoped_follow_commit(&package, &restored, &result)?;
        assert!(
            matches!(&inlined.nodes()[&mapped.target.node].framing, Some(Framing { value: FramingValue::Follow { target, scale, scale_transition: Some(transition), .. }, .. }) if target.as_str() == "other" && *scale == ratio(1, 1) && transition.factor == ratio(4, 1))
        );
        assert_eq!(
            ProjectStore::open(&package, AccessMode::ReadOnly)?.registers()?,
            bank
        );
    }
    Ok(())
}
