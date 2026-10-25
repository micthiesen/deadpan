use deadpan_core::{
    InstancePath, MarkId, NodeKind, OccurrenceIdentities, RepeatEditBranch, RepeatEditStep,
    RepeatInstance, ScopedNodeEdit, ScopedNodeTarget, SemanticContext, SemanticInstruction,
    SemanticProgram, SemanticScopedContext, SemanticZoomCenter, framing_zoom::ZoomKind,
};

use super::*;

mod ranges;

#[test]
fn semantic_scoped_framing_matches_the_exact_play_and_returns_intrinsic_owner_duration() -> Result {
    let fixture = Fixture::source("cfr-bframes.mp4")?;
    let document = fixture.store.snapshot()?;
    let first_pose = FramingPose::new(q(1, 4), q(1, 2), q(3, 2))?;
    let document = staged(
        &document,
        Command::SetFraming {
            node: node("source"),
            framing: Some(Framing::static_pose(first_pose)?),
        },
        "default-framing",
    );
    let repeated = staged(
        &document,
        Command::WrapRepeat {
            node: node("source"),
            id: node("repeat"),
            plays: 2,
            gap: None,
            anchor_policy: Default::default(),
        },
        "repeated",
    );
    let NodeKind::Repeat { iterations, .. } = &repeated.nodes()[&node("repeat")].kind else {
        panic!("Repeat");
    };
    let instance = |ordinal| InstancePath {
        node: node("source"),
        repeats: vec![RepeatInstance {
            node: node("repeat"),
            iteration: iterations.at(ordinal).unwrap(),
        }],
    };
    let first_instance = instance(0);
    let second_instance = instance(1);
    let scoped_site = |instance: &InstancePath, frame| SemanticFramingSite {
        node: instance.node.clone(),
        frame: ProjectFrame(frame),
        original_center: true,
        presentation: Some(instance.clone()),
        root_range: None,
    };
    // Both plays still share one node here. Node-only lookup would accept this
    // wrong concrete occurrence at the second play's cursor.
    assert!(
        resolve(
            &fixture.store,
            &repeated,
            &scoped_site(&first_instance, 121)
        )
        .is_err()
    );
    assert!(resolve(&fixture.store, &repeated, &site("source", 121, true)).is_err());
    assert_eq!(
        resolve(
            &fixture.store,
            &repeated,
            &scoped_site(&second_instance, 121)
        )?
        .current,
        first_pose
    );
    let target = ScopedNodeTarget {
        node: node("source"),
        repeats: vec![RepeatEditStep {
            repeat: node("repeat"),
            branch: RepeatEditBranch::Play {
                iteration: iterations.at(1).unwrap(),
            },
        }],
    };
    let second_pose = FramingPose::new(q(3, 4), q(1, 2), ExactRatio::integer(2))?;
    let edit = ScopedNodeEdit::SetFraming {
        framing: Some(Framing::static_pose(second_pose)?),
    };
    let needs = repeated.scoped_edit_requirements(&target, &edit)?;
    let request = CommandRequest {
        project_id: repeated.project_id().clone(),
        expected_revision: repeated.revision_id().clone(),
        new_revision: revision("isolated"),
        command: Command::EditScoped {
            target,
            edit,
            identities: OccurrenceIdentities {
                nodes: (0..needs.nodes)
                    .map(|index| node(&format!("copy-{index}")))
                    .collect(),
                marks: (0..needs.marks)
                    .map(|index| MarkId::new(format!("mark-{index}")).unwrap())
                    .collect(),
            },
        },
    };
    let prepared = deadpan_core::prepare_scoped_edit(&repeated, &request)?;
    let second_instance = prepared.map_instance(&repeated, &second_instance)?;
    assert_ne!(second_instance.node, first_instance.node);
    let retimed = staged(
        &prepared.document,
        Command::WrapRetime {
            node: node("repeat"),
            id: node("retime"),
            duration: frames(96),
            pitch: PitchPolicy::Preserve,
        },
        "retimed",
    );
    std::fs::remove_file(&fixture.source)?;
    for (instance, at, expected) in [
        (&first_instance, 1, first_pose),
        (&second_instance, 49, second_pose),
    ] {
        let state = resolve(&fixture.store, &retimed, &scoped_site(instance, at))?;
        assert_eq!(state.current, expected);
        assert_eq!(state.original_center, Some([q(1, 2), q(1, 2)]));
        // Each play occupies 48 root frames, but its authored Source owns 120.
        // Native Camera builds its whole-owner path over the latter extent.
        assert_eq!(state.duration, frames(120));
    }
    assert!(resolve(&fixture.store, &retimed, &scoped_site(&first_instance, 49)).is_err());
    assert_eq!(fixture.store.head_revision()?, revision("registered"));
    Ok(())
}

#[test]
fn semantic_scoped_zoom_keep_preserves_live_follow_and_retained_clock_after_isolation() -> Result {
    let fixture = Fixture::source("cfr-bframes.mp4")?;
    let document = fixture.store.snapshot()?;
    let target = TargetId::new("subject")?;
    let targeted = staged(
        &document,
        Command::SetTarget {
            id: target.clone(),
            target: AttentionTarget {
                label: "Subject".into(),
                asset: asset("original"),
                span: document.assets()[&asset("original")].video.unwrap(),
                region: TargetRegion {
                    center: [250_000, 500_000],
                    size: [100_000, 100_000],
                }
                .into(),
                samples: vec![],
                corrections: vec![],
                provenance: None,
                display_aspect: None,
            },
        },
        "follow-target",
    );
    let clock = FramingClock::RetainedOutput {
        offset: ExactRatio::ONE,
        duration: ExactRatio::integer(12),
    };
    let fallback = FramingPose::new(q(9, 10), q(1, 2), ExactRatio::integer(2))?;
    let followed = staged(
        &targeted,
        Command::SetFraming {
            node: node("source"),
            framing: Some(Framing {
                clock,
                value: FramingValue::Follow {
                    target: target.clone(),
                    scale: ExactRatio::integer(2),
                    fallback,
                    scale_transition: None,
                },
            }),
        },
        "follow-framed",
    );
    let repeated = staged(
        &followed,
        Command::WrapRepeat {
            node: node("source"),
            id: node("repeat"),
            plays: 2,
            gap: None,
            anchor_policy: Default::default(),
        },
        "follow-repeated",
    );
    let NodeKind::Repeat { iterations, .. } = &repeated.nodes()[&node("repeat")].kind else {
        panic!("Repeat");
    };
    let play = iterations.at(1).unwrap();
    let presentation = InstancePath {
        node: node("source"),
        repeats: vec![RepeatInstance {
            node: node("repeat"),
            iteration: play.clone(),
        }],
    };
    let retimed = staged(
        &repeated,
        Command::WrapRetime {
            node: node("repeat"),
            id: node("retime"),
            duration: frames(96),
            pitch: PitchPolicy::Preserve,
        },
        "follow-retimed",
    );
    let context = SemanticContext {
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
                    branch: RepeatEditBranch::Play { iteration: play },
                }],
            },
            presentation: presentation.clone(),
        }),
        parent: retimed.root().clone(),
        cursor: ProjectFrame(49),
        selected_child: Some(node("retime")),
        visual_selection: None,
    };
    let program = SemanticProgram::new(vec![SemanticInstruction::Zoom {
        kind: ZoomKind::Smash {
            scale: ExactRatio::integer(3),
        },
        center: SemanticZoomCenter::Keep,
    }])?;
    let bank = fixture.store.registers()?;
    std::fs::remove_file(&fixture.source)?;
    let entry = resolve(
        &fixture.store,
        &retimed,
        &SemanticFramingSite {
            node: node("source"),
            presentation: Some(presentation),
            frame: ProjectFrame(49),
            original_center: false,
            root_range: None,
        },
    )?;
    assert_eq!(
        entry.current,
        FramingPose::new(q(1, 4), q(1, 2), ExactRatio::integer(2))?
    );
    assert_ne!(entry.current, fallback);
    let plan = crate::macros::plan_program(
        &fixture.store,
        &retimed,
        &bank,
        &context,
        &program,
        revision("follow-zoomed"),
    )?;
    let mapped = plan.context.scoped.as_ref().unwrap();
    assert_ne!(mapped.target.node, node("source"));
    let framing = plan.document.nodes()[&mapped.target.node]
        .framing
        .as_ref()
        .unwrap();
    assert_eq!(framing.clock, clock);
    assert_eq!(
        framing.value,
        FramingValue::Follow {
            target,
            scale: ExactRatio::integer(3),
            fallback: FramingPose {
                scale: ExactRatio::integer(3),
                ..fallback
            },
            scale_transition: None,
        }
    );
    let state = resolve(
        &fixture.store,
        &plan.document,
        &SemanticFramingSite {
            node: mapped.target.node.clone(),
            presentation: Some(mapped.presentation.clone()),
            frame: ProjectFrame(49),
            original_center: false,
            root_range: None,
        },
    )?;
    assert_eq!(
        state.current,
        FramingPose::new(q(1, 4), q(1, 2), ExactRatio::integer(3))?
    );
    assert_ne!(state.current.center_x, fallback.center_x);
    assert_eq!(
        plan.document.nodes()[&node("source")],
        retimed.nodes()[&node("source")]
    );
    let before = RenderPlan::compile(&retimed)?;
    let after = RenderPlan::compile(&plan.document)?;
    for at in 0..48 {
        let expected = before.picture(ProjectFrame(at))?;
        let actual = after.picture(ProjectFrame(at))?;
        assert_eq!(actual.picture, expected.picture);
        assert_eq!(actual.framing, expected.framing);
    }
    assert_eq!(fixture.store.snapshot()?, document);
    assert_eq!(fixture.store.registers()?, bank);
    Ok(())
}
