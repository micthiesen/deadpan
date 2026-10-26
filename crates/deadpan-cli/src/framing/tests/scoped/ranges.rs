use std::{num::NonZeroU32, sync::Arc};

#[test]
fn semantic_scoped_range_static_keep_and_original_center_restore_the_exact_prefix_and_suffix()
-> Result {
    let mut fixture = fixture()?;
    let initial = FramingPose::new(q(1, 4), q(3, 4), q(5, 4))?;
    commit(
        &mut fixture.store,
        Command::SetFraming {
            node: node("source"),
            framing: Some(Framing {
                clock: FramingClock::RetainedOutput {
                    offset: q(-3, 2),
                    duration: q(121, 2),
                },
                value: FramingValue::Static { pose: initial },
            }),
        },
        "range-static",
    )?;
    let document = fixture.store.snapshot()?;
    let bank = fixture.store.registers()?;
    let before = RenderPlan::compile(&document)?;
    for (center, expected_center, name) in [
        (
            SemanticZoomCenter::Keep,
            [q(1, 4), q(3, 4)],
            "range-static-keep",
        ),
        (
            SemanticZoomCenter::OriginalCenter,
            [q(1, 2), q(1, 2)],
            "range-static-center",
        ),
    ] {
        let captured = context(&document, 1, 84, 60);
        let planned = crate::macros::plan_program(
            &fixture.store,
            &document,
            &bank,
            &captured,
            &SemanticProgram::new(vec![SemanticInstruction::Zoom {
                kind: ZoomKind::Smash { scale: q(2, 1) },
                center,
            }])?,
            revision(name),
        )?;
        let scoped = planned.context.scoped.as_ref().unwrap();
        assert_eq!(
            planned.document.nodes()[&scoped.target.node]
                .framing
                .as_ref()
                .unwrap()
                .clock,
            FramingClock::OwnerOutput
        );
        let after = RenderPlan::compile(&planned.document)?;
        assert_timing_unchanged(&before, &after)?;
        let changed = FramingPose::new(expected_center[0], expected_center[1], q(2, 1))?;
        for at in 0..96 {
            assert_eq!(
                owner_pose(&after, at)?,
                if (60..84).contains(&at) {
                    changed
                } else {
                    initial
                },
                "{center:?} at {at}"
            );
        }
        assert_eq!(fixture.store.snapshot()?, document);
        assert_eq!(fixture.store.registers()?, bank);
    }
    Ok(())
}

use deadpan_core::{
    AudioChange, AudioSample, FrameRange, GainDb, RegisterName, RegisterValue,
    SemanticVisualSelection, SourceTimestamp, TargetSample, TrackState, framing_zoom::ZoomCurve,
};
use deadpan_plan::AudioQueryLimits;

use super::*;

fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

fn visual(anchor: i64, head: i64) -> SemanticVisualSelection {
    SemanticVisualSelection::Time {
        anchor: ProjectFrame(anchor),
        head: ProjectFrame(head),
        extending: false,
    }
}

fn commit(store: &mut ProjectStore, command: Command, name: &str) -> Result {
    let document = store.snapshot()?;
    store.commit(&CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(name),
        command,
    })?;
    Ok(())
}

/// Qualification uses real media once. Every operation under test then has only
/// the retained document/index metadata: the linked Original has been deleted.
fn fixture() -> Result<Fixture> {
    let mut fixture = Fixture::source("cfr-bframes.mp4")?;
    let document = fixture.store.snapshot()?;
    assert_eq!(document.duration()?, frames(120));
    let span = document.assets()[&asset("original")].video.unwrap();
    assert_eq!(span.start().ticks, 0);
    assert_eq!(span.end().ticks, 120_120);
    let region = |x| {
        TargetRegion {
            center: [x, 500_000],
            size: [100_000, 100_000],
        }
        .into()
    };
    commit(
        &mut fixture.store,
        Command::SetTarget {
            id: TargetId::new("moving")?,
            target: AttentionTarget {
                label: "Moving subject".into(),
                asset: asset("original"),
                // The last quarter deliberately falls back, so both live and
                // fallback scale bases are exercised by the same transition.
                span: SourceSpan::new(
                    span.start(),
                    SourceTimestamp {
                        ticks: 90_090,
                        time_base: span.start().time_base,
                    },
                )?,
                region: region(250_000),
                samples: vec![
                    TargetSample {
                        at: 0,
                        region: region(250_000),
                        confidence: 1000,
                        state: TrackState::Tracked,
                    },
                    TargetSample {
                        at: 60_060,
                        region: region(650_000),
                        confidence: 1000,
                        state: TrackState::Tracked,
                    },
                ],
                corrections: vec![],
                provenance: None,
                display_aspect: None,
            },
        },
        "range-target",
    )?;
    commit(
        &mut fixture.store,
        Command::SetFraming {
            node: node("source"),
            framing: Some(Framing {
                clock: FramingClock::RetainedOutput {
                    offset: ExactRatio::ONE,
                    duration: ExactRatio::integer(12),
                },
                value: FramingValue::Follow {
                    centering: Default::default(),
                    target: TargetId::new("moving")?,
                    scale: q(3, 2),
                    fallback: FramingPose::new(q(9, 10), q(4, 5), q(5, 4))?,
                    scale_transition: None,
                },
            }),
        },
        "range-follow",
    )?;
    commit(
        &mut fixture.store,
        Command::WrapRepeat {
            node: node("source"),
            id: node("repeat"),
            plays: 2,
            gap: None,
            anchor_policy: Default::default(),
        },
        "range-repeat",
    )?;
    commit(
        &mut fixture.store,
        Command::WrapRetime {
            node: node("repeat"),
            id: node("retime"),
            duration: frames(96),
            pitch: PitchPolicy::Preserve,
        },
        "range-retime",
    )?;
    std::fs::remove_file(&fixture.source)?;
    Ok(fixture)
}

fn context(document: &ProjectDocument, ordinal: u32, anchor: i64, head: i64) -> SemanticContext {
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&node("repeat")].kind else {
        panic!("Repeat");
    };
    let play = iterations.at(ordinal).unwrap();
    SemanticContext {
        hold_duration: None,
        selected_hold_duration: None,
        mixed_hold_duration: None,
        shared_hold_duration: None,
        original: None,
        scoped: Some(SemanticScopedContext {
            root: node("retime"),
            target: ScopedNodeTarget {
                node: node("source"),
                repeats: vec![RepeatEditStep {
                    repeat: node("repeat"),
                    branch: RepeatEditBranch::Play {
                        iteration: play.clone(),
                    },
                }],
            },
            presentation: InstancePath {
                node: node("source"),
                repeats: vec![RepeatInstance {
                    node: node("repeat"),
                    iteration: play,
                }],
            },
        }),
        parent: document.root().clone(),
        cursor: ProjectFrame(if ordinal == 0 { 1 } else { 49 }),
        selected_child: Some(node("retime")),
        visual_selection: Some(visual(anchor, head)),
    }
}

fn creep() -> SemanticInstruction {
    SemanticInstruction::Zoom {
        kind: ZoomKind::Creep {
            from: Some(q(9, 5)),
            to: q(12, 5),
            curve: ZoomCurve::Linear,
        },
        center: SemanticZoomCenter::Keep,
    }
}

fn owner_pose(plan: &RenderPlan, at: i64) -> Result<FramingPose> {
    let picture = plan.picture(ProjectFrame(at))?;
    Ok(picture.framing[0].pose.unwrap())
}

fn expected_linear_scale(at: i64) -> ExactRatio {
    // Root [60,84) is a 24-frame ramp, sampled at root frame centres.
    // Spell out the documented Q32 ties-to-even math independently of both
    // the new occurrence-map API and the framing builder/evaluator.
    let grid = i128::from(deadpan_core::FRAMING_NUMERIC_SCALE);
    let start = q(9 * grid, 5).round_even().unwrap();
    let end = q(12 * grid, 5).round_even().unwrap();
    let t = q(i128::from(2 * at - 119) * grid, 48).round_even().unwrap();
    q(
        q(start * (grid - t) + end * t, grid).round_even().unwrap(),
        grid,
    )
}

fn assert_timing_unchanged(before: &RenderPlan, after: &RenderPlan) -> Result {
    assert_eq!(before.duration(), after.duration());
    for at in 0..before.duration().frames() {
        let before = before.picture(ProjectFrame(at))?;
        let after = after.picture(ProjectFrame(at))?;
        assert_eq!(before.picture, after.picture, "provider at {at}");
        assert_eq!(before.local_position, after.local_position, "clock at {at}");
        assert_eq!(before.picture_context, after.picture_context);
        assert_eq!(before.gap_after, after.gap_after);
        assert_eq!(before.captions, after.captions);
        assert_eq!(before.framing.len(), after.framing.len());
        for (left, right) in before.framing.iter().zip(&after.framing) {
            assert_eq!(left.local_position, right.local_position);
            assert_eq!(left.duration, right.duration);
            assert_eq!(left.effective_duration, right.effective_duration);
            assert_eq!(left.escalation, right.escalation);
        }
    }
    let end = before.audio_duration()?;
    assert_eq!(end, after.audio_duration()?);
    let left = before.audio(AudioSample(0)..end, AudioQueryLimits::default())?;
    let right = after.audio(AudioSample(0)..end, AudioQueryLimits::default())?;
    assert_eq!(left.spans.len(), right.spans.len());
    for (left, right) in left.spans.iter().zip(&right.spans) {
        assert_eq!(left.samples, right.samples);
        assert_eq!(left.allocated_samples, right.allocated_samples);
        assert_eq!(left.project_extent, right.project_extent);
        assert_eq!(left.envelope_extent, right.envelope_extent);
        assert_eq!(left.envelope_samples, right.envelope_samples);
        assert_eq!(left.envelope, right.envelope);
        assert_eq!(left.transform, right.transform);
        assert_eq!(left.grid, right.grid);
        assert_eq!(left.sampling, right.sampling);
        assert_eq!(left.content, right.content);
        assert_eq!(left.retimes.len(), right.retimes.len());
        for (left, right) in left.retimes.iter().zip(&right.retimes) {
            assert_eq!(left.child_start, right.child_start);
            assert_eq!(
                left.child_frames_per_local_frame,
                right.child_frames_per_local_frame
            );
            assert_eq!(left.pitch, right.pitch);
        }
    }
    Ok(())
}

#[test]
fn semantic_scoped_range_follow_maps_exact_bounds_and_commits_one_durable_edit() -> Result {
    let mut fixture = fixture()?;
    let document = fixture.store.snapshot()?;
    let bank = fixture.store.registers()?;
    let forward = context(&document, 1, 60, 84);
    let scoped = forward.scoped.as_ref().unwrap();
    let state = resolve(
        &fixture.store,
        &document,
        &SemanticFramingSite {
            node: scoped.target.node.clone(),
            presentation: Some(scoped.presentation.clone()),
            frame: forward.cursor,
            original_center: false,
            root_range: Some(range(60, 84)),
        },
    )?;
    // 240 intrinsic input frames retime to 96 root frames. The selected
    // second Play owns [48,96), so [60,84) is [30,90) of its 120-frame Source.
    assert_eq!(state.duration, frames(120));
    assert_eq!(state.normalized_range, Some((q(1, 4), q(3, 4))));
    let program = SemanticProgram::new(vec![creep()])?;
    let planned = crate::macros::plan_program(
        &fixture.store,
        &document,
        &bank,
        &forward,
        &program,
        revision("range-applied"),
    )?;
    let backward = crate::macros::plan_program(
        &fixture.store,
        &document,
        &bank,
        &context(&document, 1, 84, 60),
        &program,
        revision("range-backward"),
    )?;
    let mapped = planned.context.scoped.as_ref().unwrap();
    let framing = planned.document.nodes()[&mapped.target.node]
        .framing
        .as_ref()
        .unwrap();
    let backward_owner = &backward.context.scoped.as_ref().unwrap().target.node;
    assert_eq!(
        Some(framing),
        backward.document.nodes()[backward_owner].framing.as_ref()
    );
    assert_eq!(framing.clock, FramingClock::OwnerOutput);
    assert_ne!(mapped.target.node, node("source"));
    assert_eq!(
        planned.document.nodes()[&node("source")],
        document.nodes()[&node("source")]
    );
    let FramingValue::Follow {
        target,
        scale,
        fallback,
        scale_transition,
        ..
    } = &framing.value
    else {
        panic!("range Keep must preserve live Follow");
    };
    assert_eq!(target, &TargetId::new("moving")?);
    assert_eq!(*scale, q(3, 2));
    assert_eq!(fallback.scale, q(5, 4));
    assert!(scale_transition.is_some());
    // Exact owner boundaries include the deliberate jump at In and terminal
    // live/fallback factors. These are independent of cursor-centre sampling.
    for (local, live, fallback_scale) in [
        (29, q(3, 2), q(5, 4)),
        (30, q(9, 5), q(3, 2)),
        (90, q(12, 5), q(2, 1)),
    ] {
        let evaluated = framing.evaluate_with_follow_exact(q(local, 1), q(120, 1))?;
        assert_eq!(evaluated.follow.unwrap().scale, live);
        assert_eq!(evaluated.pose.scale, fallback_scale);
    }
    let before = RenderPlan::compile(&document)?;
    let after = RenderPlan::compile(&planned.document)?;
    assert_timing_unchanged(&before, &after)?;
    for at in 0..60 {
        assert_eq!(
            owner_pose(&before, at)?,
            owner_pose(&after, at)?,
            "prefix {at}"
        );
    }
    assert_ne!(
        owner_pose(&after, 61)?.center_x,
        owner_pose(&after, 70)?.center_x
    );
    assert_eq!(
        owner_pose(&after, 61)?.center_x,
        owner_pose(&before, 61)?.center_x
    );
    assert_eq!(
        owner_pose(&after, 70)?.center_x,
        owner_pose(&before, 70)?.center_x
    );
    assert!(owner_pose(&after, 60)?.scale.compare(q(9, 5)).is_gt());
    assert!(owner_pose(&after, 83)?.scale.compare(q(12, 5)).is_lt());
    for at in 60..84 {
        assert_eq!(
            owner_pose(&after, at)?.scale,
            expected_linear_scale(at),
            "exact ramp {at}"
        );
    }
    assert_eq!(
        owner_pose(&after, 61)?.center_x,
        FramingPose::new(q(475, 1000), q(1, 2), q(1, 1))?
            .quantized()?
            .center_x
    );
    assert_eq!(
        owner_pose(&after, 70)?.center_x,
        FramingPose::new(q(625, 1000), q(1, 2), q(1, 1))?
            .quantized()?
            .center_x
    );
    for at in 84..96 {
        assert_eq!(
            owner_pose(&after, at)?,
            FramingPose::new(q(9, 10), q(4, 5), q(2, 1))?
        );
    }
    assert_eq!(fixture.store.snapshot()?, document, "planning is read-only");
    planned.commit_compound(&mut fixture.store)?;
    assert_eq!(fixture.store.snapshot()?, planned.document);
    assert!(planned.commit_compound(&mut fixture.store).is_err());
    assert_eq!(fixture.store.snapshot()?, planned.document);
    assert_eq!(fixture.store.registers()?, bank);
    fixture
        .store
        .undo(&revision("range-applied"), revision("range-undone"))?;
    let undone = fixture.store.snapshot()?;
    assert_eq!(undone.nodes(), document.nodes());
    assert_eq!(undone.overrides(), document.overrides());
    fixture
        .store
        .redo(&revision("range-undone"), revision("range-redone"))?;
    assert_eq!(fixture.store.snapshot()?.nodes(), planned.document.nodes());
    let path = fixture._scratch.path().join("framing.deadpan");
    drop(fixture.store);
    let mut reopened = ProjectStore::open(&path, deadpan_store::AccessMode::ReadWrite)?;
    assert_eq!(reopened.head_revision()?, revision("range-redone"));
    reopened.undo(&revision("range-redone"), revision("range-reopened-undo"))?;
    assert_eq!(reopened.snapshot()?.nodes(), document.nodes());
    reopened.redo(
        &revision("range-reopened-undo"),
        revision("range-reopened-redo"),
    )?;
    assert_eq!(reopened.snapshot()?.nodes(), planned.document.nodes());
    assert_eq!(reopened.registers()?, bank);
    Ok(())
}

#[test]
fn semantic_scoped_range_refusals_leave_document_registers_and_history_untouched() -> Result {
    let fixture = fixture()?;
    let document = fixture.store.snapshot()?;
    let bank = fixture.store.registers()?;
    let history = fixture.store.history_availability()?;
    let mut cases = vec![];
    for (anchor, head) in [(47, 84), (60, 97), (60, 60), (0, 96)] {
        cases.push(context(&document, 1, anchor, head));
    }
    let mut wrong_play = context(&document, 0, 60, 84);
    wrong_play.cursor = ProjectFrame(49);
    cases.push(wrong_play);
    let mut missing_repeat = context(&document, 1, 60, 84);
    missing_repeat
        .scoped
        .as_mut()
        .unwrap()
        .target
        .repeats
        .clear();
    cases.push(missing_repeat);
    let mut stale_play = context(&document, 1, 60, 84);
    stale_play.scoped.as_mut().unwrap().presentation.repeats[0]
        .iteration
        .ordinal = 99;
    cases.push(stale_play);
    let mut missing_node = context(&document, 1, 60, 84);
    missing_node.scoped.as_mut().unwrap().target.node = node("missing");
    missing_node.scoped.as_mut().unwrap().presentation.node = node("missing");
    cases.push(missing_node);
    for selection in [
        SemanticVisualSelection::Node {
            node: node("source"),
        },
        SemanticVisualSelection::Nodes {
            nodes: vec![node("source")],
        },
        SemanticVisualSelection::Ranges {
            ranges: vec![range(60, 70), range(72, 84)],
        },
    ] {
        let mut context = context(&document, 1, 60, 84);
        context.visual_selection = Some(selection);
        cases.push(context);
    }
    for context in cases {
        let result = crate::macros::plan_program(
            &fixture.store,
            &document,
            &bank,
            &context,
            &SemanticProgram::new(vec![creep()])?,
            revision("refused-range"),
        );
        assert!(result.is_err(), "accepted invalid context {context:?}");
        assert_eq!(fixture.store.snapshot()?, document);
        assert_eq!(fixture.store.registers()?, bank);
        assert_eq!(fixture.store.history_availability()?, history);
    }
    // The first leaf would isolate and frame one Play. The unsupported later
    // leaf must discard that entire staged compound, including its identities.
    let unsupported = [
        SemanticInstruction::SetFraming { framing: None },
        SemanticInstruction::SetAudio {
            change: AudioChange::Trim {
                gain: GainDb::UNITY,
            },
        },
        SemanticInstruction::InsertPause {
            length: deadpan_core::PauseLength::Frames {
                frames: NonZeroU32::new(5).unwrap(),
            },
            black: false,
        },
        SemanticInstruction::Retime {
            speed: q(2, 1),
            pitch: PitchPolicy::Preserve,
            wrap: false,
        },
        SemanticInstruction::Duplicate {
            selector: deadpan_core::SemanticSelector::VisualSelection,
        },
        SemanticInstruction::Call {
            register: RegisterName::new('z')?,
            count: NonZeroU32::new(1).unwrap(),
        },
    ];
    for leaf in unsupported {
        assert!(
            crate::macros::plan_program(
                &fixture.store,
                &document,
                &bank,
                &context(&document, 1, 60, 84),
                &SemanticProgram::new(vec![creep(), leaf.clone()])?,
                revision("refused-leaf"),
            )
            .is_err(),
            "accepted unsupported late leaf {leaf:?}"
        );
        assert_eq!(fixture.store.snapshot()?, document);
        assert_eq!(fixture.store.registers()?, bank);
        assert_eq!(fixture.store.history_availability()?, history);
    }
    Ok(())
}

#[test]
fn semantic_scoped_range_named_call_resolves_fresh_range_and_play() -> Result {
    let mut fixture = fixture()?;
    let document = fixture.store.snapshot()?;
    let register = RegisterName::new('a')?;
    fixture.store.save_register(
        document.project_id(),
        document.revision_id(),
        register,
        RegisterValue::Macro {
            program: Arc::new(SemanticProgram::new(vec![creep()])?),
        },
    )?;
    let bank = fixture.store.registers()?;
    let call = SemanticProgram::new(vec![SemanticInstruction::Call {
        register,
        count: NonZeroU32::new(1).unwrap(),
    }])?;
    for (ordinal, start, end, expected) in [
        (0, 6, 18, (q(1, 8), q(3, 8))),
        (1, 72, 90, (q(1, 2), q(7, 8))),
    ] {
        let context = context(&document, ordinal, start, end);
        let planned = crate::macros::plan_program(
            &fixture.store,
            &document,
            &bank,
            &context,
            &call,
            revision("fresh-call"),
        )?;
        let scoped = planned.context.scoped.as_ref().unwrap();
        let framing = planned.document.nodes()[&scoped.target.node]
            .framing
            .as_ref()
            .unwrap();
        let FramingValue::Follow {
            scale_transition: Some(transition),
            ..
        } = &framing.value
        else {
            panic!("Follow transition")
        };
        let window = transition.range.unwrap();
        assert_eq!((window.start, window.end), expected);
        assert_eq!(
            scoped.presentation.repeats[0].iteration,
            context.scoped.as_ref().unwrap().presentation.repeats[0].iteration
        );
        assert_eq!(
            planned.document.nodes()[&node("source")],
            document.nodes()[&node("source")]
        );
        assert_timing_unchanged(
            &RenderPlan::compile(&document)?,
            &RenderPlan::compile(&planned.document)?,
        )?;
    }
    assert_eq!(fixture.store.snapshot()?, document);
    assert_eq!(fixture.store.registers()?, bank);
    Ok(())
}

#[test]
fn semantic_scoped_range_rejects_fractional_allocation_overlap_and_maps_cropped_owner() -> Result {
    let fixture = fixture()?;
    let original = fixture.store.snapshot()?;
    let bank = fixture.store.registers()?;
    // An integral 95-frame Retime puts the second play's first structural
    // boundary at 47.5. Cursor frame 47 sees it, but root boundary 47 is not
    // owned by that play and must never be clamped forward to 47.5.
    let mut changed = original.nodes()[&node("retime")].clone();
    let NodeKind::Retime { duration, .. } = &mut changed.kind else {
        panic!("Retime")
    };
    *duration = frames(95);
    let mut wire = serde_json::to_value(&original)?;
    wire["nodes"]["retime"] = serde_json::to_value(&changed)?;
    let fractional = ProjectDocument::from_json(&wire.to_string())?;
    let mut captured = context(&fractional, 1, 47, 60);
    captured.cursor = ProjectFrame(47);
    let scoped = captured.scoped.as_ref().unwrap();
    let site = SemanticFramingSite {
        node: scoped.target.node.clone(),
        presentation: Some(scoped.presentation.clone()),
        frame: captured.cursor,
        original_center: false,
        root_range: Some(range(47, 60)),
    };
    assert!(resolve(&fixture.store, &fractional, &site).is_err());
    assert!(
        crate::macros::plan_program(
            &fixture.store,
            &fractional,
            &bank,
            &captured,
            &SemanticProgram::new(vec![creep()])?,
            revision("fractional-refused"),
        )
        .is_err()
    );
    let valid = resolve(
        &fixture.store,
        &fractional,
        &SemanticFramingSite {
            root_range: Some(range(48, 60)),
            ..site
        },
    )?;
    assert_eq!(valid.normalized_range, Some((q(1, 95), q(5, 19))));

    // Crop twenty input frames from either end of the two-play input, then
    // fit its remaining 200 frames into 80. Second play is only [0,100) of
    // the original 120-frame owner, so the root endpoint maps to 5/6, not 1.
    let NodeKind::Retime {
        duration, mapping, ..
    } = &mut changed.kind
    else {
        panic!("Retime")
    };
    *duration = frames(80);
    *mapping = range(20, 220);
    wire["nodes"]["retime"] = serde_json::to_value(&changed)?;
    let cropped = ProjectDocument::from_json(&wire.to_string())?;
    let captured = context(&cropped, 1, 40, 80);
    let scoped = captured.scoped.as_ref().unwrap();
    let site = SemanticFramingSite {
        node: scoped.target.node.clone(),
        presentation: Some(scoped.presentation.clone()),
        frame: captured.cursor,
        original_center: false,
        root_range: Some(range(40, 80)),
    };
    let state = resolve(&fixture.store, &cropped, &site)?;
    assert_eq!(state.duration, frames(120));
    assert_eq!(state.normalized_range, Some((q(0, 1), q(5, 6))));
    assert!(
        resolve(
            &fixture.store,
            &cropped,
            &SemanticFramingSite {
                root_range: Some(range(39, 80)),
                ..site
            }
        )
        .is_err()
    );
    assert_eq!(fixture.store.snapshot()?, original);
    assert_eq!(fixture.store.registers()?, bank);
    Ok(())
}

#[test]
fn semantic_scoped_range_reisolates_an_existing_override_beneath_an_outer_play() -> Result {
    let mut fixture = fixture()?;
    let document = fixture.store.snapshot()?;
    let bank = fixture.store.registers()?;
    let mut whole = context(&document, 1, 60, 84);
    whole.visual_selection = None;
    let first = crate::macros::plan_program(
        &fixture.store,
        &document,
        &bank,
        &whole,
        &SemanticProgram::new(vec![SemanticInstruction::Zoom {
            kind: ZoomKind::Smash { scale: q(2, 1) },
            center: SemanticZoomCenter::Keep,
        }])?,
        revision("nested-existing-override"),
    )?;
    let existing = first.context.scoped.as_ref().unwrap().clone();
    first.commit_compound(&mut fixture.store)?;
    commit(
        &mut fixture.store,
        Command::WrapRepeat {
            node: node("retime"),
            id: node("outer"),
            plays: 2,
            gap: None,
            anchor_policy: Default::default(),
        },
        "nested-outer",
    )?;
    let document = fixture.store.snapshot()?;
    let NodeKind::Repeat { iterations, .. } = &document.nodes()[&node("outer")].kind else {
        panic!("outer Repeat");
    };
    let play = iterations.at(0).unwrap();
    let mut nested = whole;
    nested.selected_child = Some(node("outer"));
    nested.visual_selection = Some(visual(60, 84));
    let mut scoped = existing;
    scoped.root = node("outer");
    scoped.target.repeats.insert(
        0,
        RepeatEditStep {
            repeat: node("outer"),
            branch: RepeatEditBranch::Play {
                iteration: play.clone(),
            },
        },
    );
    scoped.presentation.repeats.insert(
        0,
        RepeatInstance {
            node: node("outer"),
            iteration: play,
        },
    );
    nested.scoped = Some(scoped.clone());
    let planned = crate::macros::plan_program(
        &fixture.store,
        &document,
        &fixture.store.registers()?,
        &nested,
        &SemanticProgram::new(vec![creep()])?,
        revision("nested-ranged"),
    )?;
    let mapped = planned.context.scoped.as_ref().unwrap();
    assert_ne!(mapped.target.node, scoped.target.node);
    assert_eq!(mapped.target.repeats.len(), 2);
    assert_ne!(
        mapped.target.repeats[1].repeat,
        scoped.target.repeats[1].repeat
    );
    let framing = planned.document.nodes()[&mapped.target.node]
        .framing
        .as_ref()
        .unwrap();
    let FramingValue::Follow {
        scale_transition: Some(transition),
        ..
    } = &framing.value
    else {
        panic!("Follow transition");
    };
    let window = transition.range.unwrap();
    assert_eq!((window.start, window.end), (q(1, 4), q(3, 4)));
    let before = RenderPlan::compile(&document)?;
    let after = RenderPlan::compile(&planned.document)?;
    assert_timing_unchanged(&before, &after)?;
    // Outer play 2 still sees the pre-existing inner override, while only
    // inner play 2 of outer play 1 gets the new range.
    for at in (0..48).chain(96..192) {
        assert_eq!(
            owner_pose(&before, at)?,
            owner_pose(&after, at)?,
            "other play {at}"
        );
    }
    assert_eq!(fixture.store.snapshot()?, document);
    planned.commit_compound(&mut fixture.store)?;
    fixture
        .store
        .undo(&revision("nested-ranged"), revision("nested-undone"))?;
    assert_eq!(fixture.store.snapshot()?.nodes(), document.nodes());
    assert_eq!(fixture.store.snapshot()?.overrides(), document.overrides());
    Ok(())
}
