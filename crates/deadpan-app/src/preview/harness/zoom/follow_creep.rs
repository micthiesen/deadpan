//! Follow animation through production commands, recording and Camera keys.
use super::*;
use crate::preview::copied::Content;
use deadpan_core::{
    FollowScaleTransition, SemanticInstruction as I, SemanticZoomCenter,
    framing_zoom::{ZoomCurve, ZoomKind},
};

fn idle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Follow creep receipt finishes", |app| {
        !app.service.is_busy() && !app.macros.is_pending() && app.camera_pending.is_none()
    })?;
    d.settled()
}

fn undo_to(d: &mut Driver<'_>, expected: &Option<Framing>) -> Result<(), String> {
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "One Undo restores the complete Follow recipe",
        framing(d) == *expected,
        json!(expected),
        json!(framing(d)),
    )
}

fn camera(d: &mut Driver<'_>) -> Result<(), String> {
    d.chord(&[Key::Comma, Key::F])?;
    d.wait_for("Camera opens on animated Follow", |app| {
        app.camera.is_some()
    })?;
    d.settled()
}

fn scale_adjustment(d: &mut Driver<'_>, animated: &Option<Framing>) -> Result<(), String> {
    let before = shown_pose(d).ok_or("Follow has no installed pose")?;
    let revision = d.revision();
    camera(d)?;
    let entry = d.app().camera.as_ref().unwrap().harness_state();
    // 1.05^70 leaves this picture's live scale within the pose range but
    // pushes the authored terminal endpoint above 64. Refuse the entire nudge.
    d.chord(&[Key::Num7, Key::Num0, Key::Plus])?;
    d.settled()?;
    let rejected = d.app().camera.as_ref().unwrap().harness_state();
    d.check(
        "Camera refuses a scale nudge whose later endpoint exceeds the bound",
        rejected["pose"] == entry["pose"]
            && d.revision() == revision
            && framing(d) == *animated
            && d.app().error.is_some(),
        json!({"pose":entry["pose"],"revision":revision}),
        json!({"pose":rejected["pose"],"revision":d.revision(),"error":d.app().error}),
    )?;
    d.key(Key::Escape)?;
    idle(d)?;
    camera(d)?;
    d.key(Key::Plus)?;
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    idle(d)?;
    let adjusted = framing(d);
    let proportional = match (animated, &adjusted) {
        (
            Some(Framing {
                clock: old_clock,
                value:
                    FramingValue::Follow {
                        target: old_target,
                        scale: old_scale,
                        fallback: old_fallback,
                        scale_transition: old_transition,
                    },
            }),
            Some(Framing {
                clock,
                value:
                    FramingValue::Follow {
                        target,
                        scale,
                        fallback,
                        scale_transition,
                    },
            }),
        ) => {
            let factor = scale
                .checked_div(*old_scale)
                .map_err(|error| error.to_string())?;
            clock == old_clock
                && target == old_target
                && scale_transition == old_transition
                && scale.compare(*old_scale).is_gt()
                && fallback.center_x == old_fallback.center_x
                && fallback.center_y == old_fallback.center_y
                && fallback.scale
                    == old_fallback
                        .scale
                        .checked_mul(factor)
                        .map_err(|error| error.to_string())?
        }
        _ => false,
    };
    let shown = shown_pose(d);
    d.check(
        "Camera scale key changes both Follow bases proportionally and retains its animation",
        proportional
            && shown.is_some_and(|pose| {
                pose.center_x == before.center_x
                    && pose.center_y == before.center_y
                    && pose.scale.compare(before.scale).is_gt()
            }),
        json!({"proportional_bases":true,"target_clock_and_transition":"retained"}),
        json!({"framing":adjusted,"shown":shown}),
    )?;
    undo_to(d, animated)
}

fn switch_target(d: &mut Driver<'_>, animated: &Option<Framing>) -> Result<(), String> {
    let revision = d.revision();
    camera(d)?;
    d.key(Key::N)?;
    d.chord(&[Key::Num8, Key::H])?;
    d.key(Key::Enter)?;
    d.wait_for(
        "New target save retains animated Camera continuation",
        |app| {
            app.workspace
                .as_ref()
                .is_some_and(|workspace| workspace.document.targets().len() == 2)
                && app.camera.as_ref().is_some_and(|camera| {
                    let state = camera.harness_state();
                    state["saving"] == false && state["rebasing"] == false
                })
        },
    )?;
    d.changed(&revision)?;
    let target_revision = d.revision();
    d.chord(&[Key::F, Key::Num2])?;
    d.key(Key::Enter)?;
    d.changed(&target_revision)?;
    idle(d)?;
    let switched = framing(d);
    let mut expected = animated.clone().ok_or("Missing Follow recipe")?;
    if let FramingValue::Follow { target, .. } = &mut expected.value {
        *target = deadpan_core::TargetId::new("target-2").unwrap();
    }
    d.check(
        "Saving a target and switching in Camera preserves both bases, transition and clock",
        switched == Some(expected.clone()),
        json!(expected),
        json!(switched),
    )?;
    undo_to(d, animated)?;
    // Remove the temporary target too, so the rest of the zoom scenario keeps
    // its original one-target context and ordinary history checks.
    undo_to(d, animated)?;
    d.check(
        "Undo removes the temporary target while retaining the Follow animation",
        d.app().workspace.as_ref().unwrap().document.targets().len() == 1,
        json!(1),
        json!(d.app().workspace.as_ref().unwrap().document.targets().len()),
    )
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let fixed = framing(d);
    let duration = d.app().sequence_length();
    let original_cursor = d.app().source_cursor;
    let edit_cursor = d.app().sequence_cursor;
    d.chord(&[Key::Q, Key::X])?;
    let revision = d.revision();
    d.command("creep to=2.7 curve=linear")?;
    d.changed(&revision)?;
    idle(d)?;
    let animated = framing(d);
    d.check("Implicit creep animates scale while retaining the saved target",
        matches!(&animated, Some(Framing { value: FramingValue::Follow { target, scale,
            scale_transition: Some(FollowScaleTransition { factor, curve: ZoomCurve::Linear }), .. }, .. })
            if target.as_str() == "target-1" && *scale == ExactRatio::new(27, 20).unwrap()
                && *factor == ExactRatio::integer(2))
            && d.app().sequence_length() == duration && d.app().source_cursor == original_cursor
            && d.app().sequence_cursor == edit_cursor,
        json!({"target":"target-1","initial":"27/20","factor":2,"frames":duration}),
        json!({"framing":animated,"frames":d.app().sequence_length()}))?;
    d.key(Key::Q)?;
    d.wait_for("Follow creep macro saves", |app| {
        !app.service.is_busy() && !app.macros.is_pending() && !app.macros.recording()
    })?;
    let program = d
        .app()
        .copied
        .entries()
        .find_map(|(name, content)| match content {
            Content::Macro(program) if name == 'x' => Some(program.instructions().to_vec()),
            _ => None,
        });
    let expected = vec![I::Zoom {
        kind: ZoomKind::Creep {
            from: None,
            to: ExactRatio::new(27, 10).unwrap(),
            curve: ZoomCurve::Linear,
        },
        center: SemanticZoomCenter::Keep,
    }];
    d.check(
        "Whole-Source Follow creep records invocation-relative Zoom Keep intent",
        program == Some(expected.clone()),
        json!(expected),
        json!(program),
    )?;

    scale_adjustment(d, &animated)?;
    switch_target(d, &animated)?;

    // Enter with no adjustments must preserve the complete authored animation.
    let revision = d.revision();
    camera(d)?;
    d.key(Key::Enter)?;
    d.wait_for("Unchanged Camera closes", |app| {
        app.camera.is_none() && !app.service.is_busy()
    })?;
    idle(d)?;
    d.check(
        "Unchanged Camera Enter preserves Follow animation without a revision",
        d.revision() == revision && framing(d) == animated,
        json!({"revision":revision,"framing":animated}),
        json!({"revision":d.revision(),"framing":framing(d)}),
    )?;
    camera(d)?;
    d.key(Key::T)?;
    d.settled()?;
    d.check(
        "Camera release explains that Enter saves static framing and removes animation",
        d.app().message.as_deref().is_some_and(|message| {
            message.contains("Scale animation removed") && message.contains("static framing")
        }),
        json!("Scale animation removed; Enter saves this static framing"),
        json!(d.app().message),
    )?;
    d.key(Key::Escape)?;
    idle(d)?;
    d.check(
        "Escape after release retains the authored animated Follow",
        d.revision() == revision && framing(d) == animated,
        json!(animated),
        json!(framing(d)),
    )?;

    let installed = shown_pose(d).ok_or("Animated Follow has no installed picture")?;
    camera(d)?;
    d.key(Key::T)?;
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check("Explicit release commits exactly the installed pose as static framing",
        matches!(framing(d), Some(Framing { value: FramingValue::Static { pose }, .. }) if pose == installed),
        json!(installed), json!(framing(d)))?;
    undo_to(d, &animated)?;

    let revision = d.revision();
    camera(d)?;
    d.chord(&[Key::T, Key::Plus, Key::T, Key::Enter])?;
    d.changed(&revision)?;
    idle(d)?;
    let refollowed = framing(d);
    d.check(
        "Releasing, adjusting and re-following creates a deliberate fixed Follow",
        matches!(&refollowed, Some(Framing { value: FramingValue::Follow {
            target, scale, fallback, scale_transition: None }, .. })
            if target.as_str() == "target-1" && scale.compare(installed.scale).is_gt()
                && *scale == fallback.scale),
        json!({"target":"target-1","animation":"removed","scale":"adjusted"}),
        json!(refollowed),
    )?;
    undo_to(d, &animated)?;
    let revision = d.revision();
    camera(d)?;
    d.chord(&[Key::R, Key::Enter])?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Camera Reset removes the complete Follow animation",
        framing(d).is_none(),
        json!(null),
        json!(framing(d)),
    )?;
    undo_to(d, &animated)?;
    d.capture("Follow creep retains live targeting and explains explicit static release")?;

    undo_to(d, &fixed)?;
    let revision = d.revision();
    super::super::macros::execute(d, 'x', None)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Saved Follow creep re-resolves and restores the same complete recipe",
        framing(d) == animated,
        json!(animated),
        json!(framing(d)),
    )?;
    undo_to(d, &fixed)?;
    // Named Run intentionally clears dot authority. Establish it with a fresh
    // direct Apply, then use the ordinary Undo preservation proof.
    let revision = d.revision();
    d.command("creep to=2.7 curve=linear")?;
    d.changed(&revision)?;
    idle(d)?;
    undo_to(d, &fixed)?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Dot repeats Follow creep as semantic intent after Undo",
        framing(d) == animated,
        json!(animated),
        json!(framing(d)),
    )?;
    undo_to(d, &fixed)
}
