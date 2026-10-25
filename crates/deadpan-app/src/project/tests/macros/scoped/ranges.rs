//! Real writer admission, isolation and atomicity of scoped Time framing macros.
use super::*;
use deadpan_core::{
    AttentionTarget, AudioSample, FollowScaleRange, FollowScaleTransition, FrameRange, Framing,
    FramingClock, FramingCurve, FramingPose, FramingValue, SemanticOriginalContext,
    SemanticVisualSelection, SourceFrameId, SourceSpan, SourceTimestamp, TargetId, TargetRegion,
};

fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

fn visual(anchor: i64, head: i64) -> SemanticVisualSelection {
    SemanticVisualSelection::Time {
        anchor: ProjectFrame(anchor),
        head: ProjectFrame(head),
        extending: false,
    }
}

fn ranged(
    update: &ProjectUpdate,
    target: &Target,
    serial: u64,
    instruction: SemanticInstruction,
) -> Operation {
    let mut operation = apply(update, target, serial, instruction);
    let Operation::Apply { context, .. } = &mut operation else {
        unreachable!()
    };
    context.visual_selection = Some(visual(440, 380));
    operation
}

fn named(update: &ProjectUpdate, target: &Target, serial: u64, register: char) -> Operation {
    let mut context = context_for(update, target);
    context.visual_selection = Some(visual(440, 380));
    Operation::Run {
        id: id(update, serial),
        register,
        count: 1,
        scope: target.scope.clone(),
        context,
    }
}

fn mapped<'a>(update: &'a ProjectUpdate, before: &Target) -> &'a crate::project::scoped::Commit {
    let committed = receipt(update)
        .committed()
        .expect("one atomic writer receipt");
    let scoped = committed
        .scoped
        .as_ref()
        .expect("native scoped continuation");
    let workspace = update.workspace.as_ref().unwrap();
    assert_eq!(&scoped.before, before);
    assert!(committed.preserve_cursor);
    assert_eq!(committed.cursor, Some(before.cursor));
    assert_eq!(committed.selected_node.as_ref(), Some(&before.root));
    assert_eq!(committed.scope, before.scope);
    assert_eq!(&scoped.revision, workspace.document.revision_id());
    assert_ne!(scoped.target.node, before.target.node);
    assert_eq!(scoped.target.repeats.len(), 2);
    assert!(
        scoped
            .target
            .matches_instance(&workspace.document, scoped.presentation.as_ref().unwrap())
            .unwrap()
    );
    let result_visual = match &receipt(update).outcome {
        Outcome::Applied {
            visual_selection, ..
        }
        | Outcome::Executed {
            visual_selection, ..
        } => visual_selection,
        _ => panic!("framing must execute"),
    };
    assert_eq!(*result_visual, Some(visual(440, 380)));
    for (old, new) in before.target.repeats.iter().zip(&scoped.target.repeats) {
        assert_eq!(old.branch, new.branch, "stable selected iteration");
    }
    scoped
}

/// Compare all canonical fields except occurrence IDs, which isolation remaps.
/// Only this source layer's scale can differ; its resolved live/fallback center
/// must stay identical to the retained qualified baseline.
fn pictures(
    before: &Workspace,
    after: &Workspace,
    selected: &NodeId,
    expected: impl Fn(i64) -> ExactRatio,
) {
    let mut aliases = BTreeMap::new();
    let mut alias = |new: &NodeId, old: &NodeId| {
        if new != old {
            assert!(!before.document.nodes().contains_key(new));
            if let Some(previous) = aliases.insert(new.clone(), old.clone()) {
                assert_eq!(&previous, old, "one physical clone has one original");
            }
        }
    };
    assert_eq!(before.plan.duration().frames(), 720);
    assert_eq!(before.plan.duration(), after.plan.duration());
    for frame in 0..720 {
        let old = before.plan.picture(ProjectFrame(frame)).unwrap();
        let new = after.plan.picture(ProjectFrame(frame)).unwrap();
        assert_eq!(old.picture, new.picture, "provider {frame}");
        assert_eq!(
            old.picture_context, new.picture_context,
            "provider context {frame}"
        );
        assert_eq!(old.local_position, new.local_position);
        assert_eq!(old.gap_after, new.gap_after);
        assert_eq!(old.captions, new.captions);
        assert_eq!(old.framing.len(), new.framing.len());
        for (old, new) in old.framing.iter().zip(&new.framing) {
            alias(&new.instance.node, &old.instance.node);
            assert_eq!(old.instance.repeats.len(), new.instance.repeats.len());
            for (old, new) in old.instance.repeats.iter().zip(&new.instance.repeats) {
                assert_eq!(old.iteration, new.iteration);
                alias(&new.node, &old.node);
            }
            assert_eq!(old.local_position, new.local_position);
            assert_eq!(old.duration, new.duration);
            assert_eq!(old.effective_duration, new.effective_duration);
            assert_eq!(old.escalation, new.escalation);
            let mut pose = old.pose;
            if !new.escalation && new.instance.node == *selected {
                assert!((360..480).contains(&frame));
                assert_eq!(
                    new.local_position,
                    ratio(i128::from(2 * (frame - 360) + 1), 2)
                );
                pose = Some(FramingPose {
                    scale: expected(frame - 360),
                    ..pose.unwrap_or_default()
                });
            }
            assert_eq!(pose, new.pose, "resolved owner pose {frame}");
        }
    }
    for (node, value) in before.document.nodes() {
        assert_eq!(
            &after.document.nodes()[node],
            value,
            "shared template {node}"
        );
    }
    assert_eq!(before.document.targets(), after.document.targets());
    assert_eq!(
        before.document.gap_overrides(),
        after.document.gap_overrides()
    );
    assert_eq!(after.document.overrides().len(), 2);
    let end = before.plan.audio_duration().unwrap();
    assert_eq!(end, after.plan.audio_duration().unwrap());
    let old = before
        .plan
        .audio(AudioSample(0)..end, Default::default())
        .unwrap();
    let new = after
        .plan
        .audio(AudioSample(0)..end, Default::default())
        .unwrap();
    assert_eq!(old.spans.len(), new.spans.len());
    assert_eq!(
        normalize(serde_json::json!(new.spans), &aliases),
        serde_json::json!(old.spans),
        "complete audio semantics after independently validated physical clone aliases"
    );
}

fn normalize(value: serde_json::Value, aliases: &BTreeMap<NodeId, NodeId>) -> serde_json::Value {
    use serde_json::Value;
    let replace = |text: String| {
        aliases
            .iter()
            .find(|(new, _)| new.as_str() == text)
            .map_or_else(|| text.clone(), |(_, old)| old.as_str().to_owned())
    };
    match value {
        Value::String(text) => Value::String(replace(text)),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| normalize(item, aliases))
                .collect(),
        ),
        Value::Object(items) => Value::Object(
            items
                .into_iter()
                .map(|(key, value)| (replace(key), normalize(value, aliases)))
                .collect(),
        ),
        other => other,
    }
}

fn history(service: &ProjectService, before: &Workspace, after: &ProjectUpdate) {
    let changed = after.workspace.as_ref().unwrap();
    let path = before.path.clone();
    let bank = stored_bank(&path);
    let undone = command(
        service,
        ProjectRequest::Undo {
            expected_revision: changed.document.revision_id().clone(),
        },
    );
    assert_same_content(
        &undone.workspace.as_ref().unwrap().document,
        &before.document,
    );
    let redone = command(
        service,
        ProjectRequest::Redo {
            expected_revision: undone
                .workspace
                .as_ref()
                .unwrap()
                .document
                .revision_id()
                .clone(),
        },
    );
    assert_same_content(
        &redone.workspace.as_ref().unwrap().document,
        &changed.document,
    );
    command(service, ProjectRequest::Close);
    let reopened = command(service, ProjectRequest::Open(path.clone()));
    assert_eq!(
        reopened.workspace.as_ref().unwrap().document,
        redone.workspace.as_ref().unwrap().document
    );
    assert_eq!(stored_bank(&path), bank);
}

#[test]
fn native_scoped_time_range_static_keep_and_center_preserve_prefix_suffix_and_durable_history() {
    for center in [SemanticZoomCenter::Keep, SemanticZoomCenter::OriginalCenter] {
        let scratch = tempfile::tempdir().unwrap();
        let (service, opened, root) = setup_source(&scratch.path().join("Documents"));
        let before = opened.workspace.as_ref().unwrap().clone();
        let target = capture_scope(&opened, &root, 400);
        let stable = rows(&before.path);
        let bank = stored_bank(&before.path);
        let applied = send(
            &service,
            ranged(
                &opened,
                &target,
                1,
                SemanticInstruction::Zoom {
                    kind: ZoomKind::Smash {
                        scale: ExactRatio::integer(2),
                    },
                    center,
                },
            ),
        );
        let scoped = mapped(&applied, &target);
        let after = applied.workspace.as_ref().unwrap();
        let framing = after.document.nodes()[&scoped.target.node]
            .framing
            .as_ref()
            .unwrap();
        assert_eq!(framing.clock, FramingClock::OwnerOutput);
        let FramingValue::Envelope { envelope } = &framing.value else {
            panic!("ranged static recipe");
        };
        assert_eq!(envelope.initial, FramingPose::identity());
        assert_eq!(envelope.segments.len(), 3);
        assert_eq!(
            envelope
                .segments
                .iter()
                .map(|segment| segment.end)
                .collect::<Vec<_>>(),
            vec![ratio(1, 6), ratio(2, 3), ExactRatio::ONE]
        );
        assert!(
            envelope
                .segments
                .iter()
                .all(|segment| segment.curve == FramingCurve::Step)
        );
        assert_eq!(envelope.segments[0].pose.scale, ExactRatio::integer(2));
        assert_eq!(envelope.segments[1].pose, FramingPose::identity());
        assert_eq!(envelope.segments[2].pose, FramingPose::identity());
        pictures(&before, after, &scoped.target.node, |j| {
            if (20..80).contains(&j) {
                ExactRatio::integer(2)
            } else {
                ExactRatio::ONE
            }
        });
        assert_eq!(
            rows(&before.path),
            (stable.0 + 1, stable.1 + 1, stable.2 + 1)
        );
        assert_eq!(stored_bank(&before.path), bank);
        history(&service, &before, &applied);
    }
}

fn follow_fixture(service: &ProjectService, mut opened: ProjectUpdate) -> (ProjectUpdate, NodeId) {
    for _ in 0..2 {
        opened = command(
            service,
            ProjectRequest::Undo {
                expected_revision: opened
                    .workspace
                    .as_ref()
                    .unwrap()
                    .document
                    .revision_id()
                    .clone(),
            },
        );
    }
    let workspace = opened.workspace.as_ref().unwrap();
    let Some(SingleSourceState::Ready {
        asset,
        node: source,
        ..
    }) = &workspace.single_source
    else {
        panic!()
    };
    let asset = asset.clone();
    let source = source.clone();
    let span = workspace.document.assets()[&asset].video.unwrap();
    let target = TargetId::new("macro-range-follow").unwrap();
    let saved = command(
        service,
        ProjectRequest::Target(crate::project::targets::Operation::Save {
            ticket: 1,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            id: target.clone(),
            target: Box::new(AttentionTarget {
                label: "Live subject, then fallback".into(),
                asset: asset.clone(),
                span: SourceSpan::new(
                    span.start(),
                    SourceTimestamp {
                        ticks: workspace.sources[&asset]
                            .video_index
                            .as_ref()
                            .unwrap()
                            .frames()[45]
                            .pts,
                        time_base: span.start().time_base,
                    },
                )
                .unwrap(),
                region: TargetRegion {
                    center: [400_000, 500_000],
                    size: [100_000, 100_000],
                }
                .into(),
                samples: vec![],
                corrections: vec![],
                provenance: None,
                display_aspect: None,
            }),
        }),
    );
    assert!(saved.error.is_none(), "{:?}", saved.error);
    assert!(
        saved
            .targets
            .as_ref()
            .unwrap()
            .reply
            .as_ref()
            .unwrap()
            .1
            .is_none()
    );
    let followed = edited(
        service,
        saved.workspace.as_ref().unwrap(),
        ProjectEdit::SetFraming {
            node: source.clone(),
            framing: Some(Framing {
                clock: FramingClock::OwnerOutput,
                value: FramingValue::Follow {
                    target,
                    scale: ratio(11, 10),
                    fallback: FramingPose::new(ratio(3, 5), ratio(1, 2), ratio(9, 10)).unwrap(),
                    scale_transition: None,
                },
            }),
        },
    );
    let inner = edited(
        service,
        followed.workspace.as_ref().unwrap(),
        ProjectEdit::WrapRepeat {
            node: source,
            plays: 2,
        },
    );
    let outer = edited(
        service,
        inner.workspace.as_ref().unwrap(),
        ProjectEdit::WrapRepeat {
            node: inner
                .committed
                .as_ref()
                .unwrap()
                .selected_node
                .clone()
                .unwrap(),
            plays: 3,
        },
    );
    let root = outer
        .committed
        .as_ref()
        .unwrap()
        .selected_node
        .clone()
        .unwrap();
    (outer, root)
}

fn round(n: i128, d: i128) -> i128 {
    let a = n / d;
    let r = n % d;
    a + i128::from(2 * r > d || (2 * r == d && a % 2 != 0))
}

fn follow_scale(j: i64) -> ExactRatio {
    let q = i128::from(deadpan_core::FRAMING_NUMERIC_SCALE);
    let base = if j < 45 { ratio(11, 10) } else { ratio(9, 10) };
    if j >= 80 {
        // This fixture has lost its target by Out. The fallback terminal
        // remains an exact authored endpoint, not a resolved-target Q32 pose.
        return base.checked_mul(ExactRatio::integer(2)).unwrap();
    }
    let fixed = |value: ExactRatio| round(value.numerator() * q, value.denominator());
    let value = if j < 20 {
        fixed(base)
    } else {
        let from = fixed(base.checked_mul(ratio(15, 11)).unwrap());
        let to = fixed(base.checked_mul(ExactRatio::integer(2)).unwrap());
        let progress = round(i128::from(2 * (j - 20) + 1) * q, 120);
        from + round((to - from) * progress, q)
    };
    ratio(value, q)
}

#[test]
fn native_scoped_time_range_named_call_preserves_live_follow_and_fallback_with_one_receipt() {
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened, _) = setup_source(&scratch.path().join("Documents"));
    let (opened, root) = follow_fixture(&service, opened);
    let inner = send(
        &service,
        save(
            &opened,
            1,
            'z',
            program(vec![SemanticInstruction::Zoom {
                kind: ZoomKind::Creep {
                    from: Some(ratio(3, 2)),
                    to: ratio(11, 5),
                    curve: ZoomCurve::Linear,
                },
                center: SemanticZoomCenter::Keep,
            }]),
        ),
    );
    let saved = send(
        &service,
        save(
            &inner,
            2,
            'a',
            program(vec![SemanticInstruction::Call {
                register: RegisterName::new('z').unwrap(),
                count: NonZeroU32::new(1).unwrap(),
            }]),
        ),
    );
    let before = saved.workspace.as_ref().unwrap().clone();
    let target = capture_scope(&saved, &root, 400);
    let stable = rows(&before.path);
    let bank = stored_bank(&before.path);
    let applied = send(&service, named(&saved, &target, 3, 'a'));
    let scoped = mapped(&applied, &target);
    let after = applied.workspace.as_ref().unwrap();
    assert!(
        matches!(&after.document.nodes()[&scoped.target.node].framing,Some(Framing {
        clock:FramingClock::OwnerOutput,value:FramingValue::Follow {scale,fallback,scale_transition:Some(FollowScaleTransition {
            factor,curve:ZoomCurve::Linear,range:Some(FollowScaleRange {start,end,from_factor})
        }),..}
    }) if *scale==ratio(11,10)&&fallback.scale==ratio(9,10)&&*factor==ExactRatio::integer(2)
        &&*start==ratio(1,6)&&*end==ratio(2,3)&&*from_factor==ratio(15,11))
    );
    pictures(&before, after, &scoped.target.node, follow_scale);
    assert_eq!(
        rows(&before.path),
        (stable.0 + 1, stable.1 + 1, stable.2 + 1)
    );
    assert_eq!(stored_bank(&before.path), bank);
    history(&service, &before, &applied);
}

#[test]
fn native_scoped_time_range_service_refuses_invalid_contexts_and_late_unsupported_call_atomically()
{
    let scratch = tempfile::tempdir().unwrap();
    let (service, opened, root) = setup_source(&scratch.path().join("Documents"));
    let leaf = send(
        &service,
        save(&opened, 1, 'b', program(vec![SemanticInstruction::Split])),
    );
    let saved = send(
        &service,
        save(
            &leaf,
            2,
            'a',
            program(vec![
                smash(2),
                SemanticInstruction::Call {
                    register: RegisterName::new('b').unwrap(),
                    count: NonZeroU32::new(1).unwrap(),
                },
            ]),
        ),
    );
    let before = saved.workspace.as_ref().unwrap();
    let target = capture_scope(&saved, &root, 400);
    let stable = rows(&before.path);
    let bank = stored_bank(&before.path);
    for case in 0..5 {
        let mut operation = ranged(&saved, &target, 10 + case, smash(3));
        let Operation::Apply { context, .. } = &mut operation else {
            unreachable!()
        };
        match case {
            0 => {
                context.visual_selection = Some(SemanticVisualSelection::Node {
                    node: target.target.node.clone(),
                })
            }
            1 => {
                let Some(SingleSourceState::Ready {
                    asset,
                    qualification,
                    ..
                }) = &before.single_source
                else {
                    panic!()
                };
                context.original = Some(SemanticOriginalContext {
                    asset: asset.clone(),
                    qualification: qualification.clone(),
                    ordinal: SourceFrameId(1),
                });
            }
            2 => context.visual_selection = Some(visual(400, 400)),
            3 => context.visual_selection = Some(visual(400, 500)),
            4 => {
                context.visual_selection = Some(SemanticVisualSelection::Ranges {
                    ranges: vec![FrameRange::new(ProjectFrame(380), ProjectFrame(440)).unwrap()],
                })
            }
            _ => unreachable!(),
        }
        let refused = command(&service, ProjectRequest::Macro(operation));
        assert!(
            refused.macros.as_ref().unwrap().result.is_err(),
            "case {case}"
        );
        assert_eq!(
            refused.workspace.as_ref().unwrap().document,
            before.document
        );
        assert_eq!(rows(&before.path), stable);
        assert_eq!(stored_bank(&before.path), bank);
    }
    let refused = command(
        &service,
        ProjectRequest::Macro(named(&saved, &target, 20, 'a')),
    );
    assert!(refused.macros.as_ref().unwrap().result.is_err());
    assert_eq!(
        refused.workspace.as_ref().unwrap().document,
        before.document,
        "early Zoom is not committed before the unsupported called leaf"
    );
    assert_eq!(rows(&before.path), stable);
    assert_eq!(stored_bank(&before.path), bank);
}
