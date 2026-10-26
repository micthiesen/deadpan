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

fn arrival_text(d: &mut Driver<'_>, text: &str) -> Result<(), String> {
    d.key_modified(Key::A, egui::Modifiers::COMMAND)?;
    d.events(
        "Edit the focused Follow arrival scale",
        vec![egui::Event::Text(text.into())],
    )
}

fn disabled_apply(d: &mut Driver<'_>, reason: &str) -> Result<egui::Rect, String> {
    let buttons = d
        .harness
        .root()
        .children_recursive()
        .filter_map(|node| {
            let access = node.accesskit_node();
            (access.role() == egui::accesskit::Role::Button
                && access.label().as_deref() == Some("Apply  Enter"))
            .then(|| (node.rect(), access.is_disabled()))
        })
        .collect::<Vec<_>>();
    let [(rect, disabled)] = buttons.as_slice() else {
        return Err(format!(
            "Expected one Apply button for {reason}, found {buttons:?}"
        ));
    };
    let rect = *rect;
    let paint = super::super::scenarios::text_paint_visibility(d, "Apply  Enter");
    d.check(
        &format!("Apply is visibly disabled during {reason}"),
        *disabled
            && !paint.is_empty()
            && paint
                .iter()
                .all(|part| part["fully_visible"] == true && part["elided"] == false),
        json!({"disabled":true,"fully_visible":true}),
        json!({"buttons":buttons.iter().map(|(rect, disabled)|
            json!({"bounds":[rect.min.x,rect.min.y,rect.max.x,rect.max.y],"disabled":disabled}))
            .collect::<Vec<_>>(),"paint":paint}),
    )?;
    Ok(rect)
}

fn resize_camera(d: &mut Driver<'_>, width: f32, height: f32) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing Camera root viewport")?
        .inner_rect = Some(rect);
    d.step("Resize Camera with editable Follow arrival controls", true)?;
    d.settled()
}

fn retune_layouts(d: &mut Driver<'_>) -> Result<(), String> {
    for (width, height) in [(1280.0, 820.0), (960.0, 640.0)] {
        resize_camera(d, width, height)?;
        let labels = [
            "Arrival scale (%)",
            "Arrival curve",
            "Smoothstep",
            "Apply  Enter",
            "Cancel  Esc",
        ];
        let paint = labels
            .into_iter()
            .map(|label| {
                (
                    label,
                    super::super::scenarios::text_paint_visibility(d, label),
                )
            })
            .collect::<Vec<_>>();
        let apply = d.rect("Apply  Enter")?;
        let cancel = d.rect("Cancel  Esc")?;
        let viewport = d.harness.ctx.content_rect();
        // Retain the actual frame even if the visibility assertion fails.
        d.capture(&format!("Follow arrival controls at {width}x{height}"))?;
        d.check(&format!("Follow arrival fields and Apply/Cancel remain fully visible at {width}x{height}"),
            paint.iter().all(|(_, parts)| !parts.is_empty() && parts.iter().all(|part| part["fully_visible"] == true && part["elided"] == false))
                && viewport.contains_rect(apply) && viewport.contains_rect(cancel),
            json!({"fully_visible":labels,"buttons_inside_viewport":true}),
            json!({"paint":paint,"apply":format!("{apply:?}"),"cancel":format!("{cancel:?}"),"viewport":format!("{viewport:?}")}))?;
    }
    resize_camera(d, 1280.0, 820.0)
}

fn curve_focus(
    d: &mut Driver<'_>,
    label: &str,
    curve: ZoomCurve,
    action: &str,
) -> Result<(), String> {
    // Deferred arrows apply after the native widget paints. Observe the next
    // ordinary frame's updated label, with no intervening input.
    d.step("Paint the deferred Follow curve label", false)?;
    d.settled()?;
    let focused = d.harness.root().children_recursive().any(|node| {
        let access = node.accesskit_node();
        access.role() == egui::accesskit::Role::Button
            && access.label().as_deref() == Some(label)
            && access.is_focused()
            && !access.is_disabled()
    });
    let state = d
        .app()
        .camera
        .as_ref()
        .ok_or("Camera closed during curve adjustment")?
        .harness_state();
    d.check(
        &format!("{action} keeps the curve control focused and changes its actual Follow recipe"),
        focused
            && d.app().error.is_none()
            && state["framing"]["value"]["scale_transition"]["curve"] == json!(curve),
        json!({"focused":label,"curve":curve,"error":null}),
        json!({"focused":focused,"camera":state,"error":d.app().error}),
    )
}

fn retune_controls(d: &mut Driver<'_>, animated: &Option<Framing>) -> Result<(), String> {
    let revision = d.revision();
    let expected = deadpan_core::framing_zoom::retune_follow(
        animated.as_ref().ok_or("Missing animated Follow")?,
        None,
        ExactRatio::new(25, 8).unwrap(),
        ZoomCurve::Smoothstep,
    )?;
    camera(d)?;
    // Follow's disabled centers leave Scale, Arrival and Curve in the native
    // field order. Reach arrival by real Tab routing from the viewer.
    d.chord(&[Key::Tab, Key::Tab])?;
    arrival_text(d, "312.5")?;
    d.key(Key::Tab)?;
    curve_focus(d, "Linear", ZoomCurve::Linear, "Tab from the arrival field")?;
    d.key(Key::ArrowDown)?;
    curve_focus(
        d,
        "Smoothstep",
        ZoomCurve::Smoothstep,
        "Keyboard-only ArrowDown",
    )?;
    d.key(Key::Space)?;
    curve_focus(d, "Linear", ZoomCurve::Linear, "Keyboard-only Space")?;
    d.click("Linear")?;
    curve_focus(
        d,
        "Smoothstep",
        ZoomCurve::Smoothstep,
        "Pointer curve activation",
    )?;
    d.key(Key::ArrowDown)?;
    curve_focus(
        d,
        "Linear",
        ZoomCurve::Linear,
        "ArrowDown after pointer activation",
    )?;
    d.key(Key::Space)?;
    curve_focus(
        d,
        "Smoothstep",
        ZoomCurve::Smoothstep,
        "Space after pointer activation",
    )?;
    d.settled()?;
    let state = d
        .app()
        .camera
        .as_ref()
        .ok_or("Camera closed while editing fields")?
        .harness_state();
    d.check(
        "Native arrival text and keyboard curve edits preview without flattening Follow or saving",
        d.revision() == revision
            && framing(d) == *animated
            && state["framing"] == json!(expected)
            && state["fields_valid"] == true,
        json!({"revision":revision,"draft":expected}),
        state,
    )?;
    d.capture("Follow arrival scale and curve are independently editable")?;
    retune_layouts(d)?;
    d.key(Key::Escape)?;
    idle(d)?;
    d.check(
        "Escape restores the exact animated Follow after arrival and curve preview",
        d.app().camera.is_none() && d.revision() == revision && framing(d) == *animated,
        json!(animated),
        json!(framing(d)),
    )?;

    camera(d)?;
    d.chord(&[Key::Tab, Key::Tab])?;
    arrival_text(d, "NaN")?;
    let apply = disabled_apply(d, "invalid Follow arrival text")?;
    d.click_at("Disabled Apply with invalid arrival", apply.center())?;
    d.check(
        "Invalid arrival text blocks Apply without replacing the accepted preview",
        d.revision() == revision
            && framing(d) == *animated
            && d.app()
                .camera
                .as_ref()
                .is_some_and(|camera| camera.harness_state()["fields_valid"] == false),
        json!({"revision":revision,"valid":false}),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    idle(d)?;

    camera(d)?;
    d.chord(&[Key::Tab, Key::Tab])?;
    d.key_modified(Key::A, egui::Modifiers::COMMAND)?;
    d.events(
        "Compose an uncommitted Follow arrival",
        vec![
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "312.5".into(),
                active_range_chars: None,
            }),
            key_event(Key::Enter, egui::Modifiers::NONE, true),
            key_event(Key::Enter, egui::Modifiers::NONE, false),
        ],
    )?;
    let apply = disabled_apply(d, "Follow arrival IME composition")?;
    d.click_at("Disabled Apply during arrival composition", apply.center())?;
    d.check(
        "Arrival IME preedit and Enter cannot save or close Camera",
        d.revision() == revision
            && framing(d) == *animated
            && d.app()
                .camera
                .as_ref()
                .is_some_and(|camera| camera.harness_state()["fields_valid"] == false),
        json!({"revision":revision,"valid":false}),
        d.snapshot(),
    )?;
    d.events(
        "Cancel Follow arrival composition",
        vec![egui::Event::Ime(egui::ImeEvent::Preedit {
            text: String::new(),
            active_range_chars: None,
        })],
    )?;
    d.click("Cancel  Esc")?;
    idle(d)?;

    camera(d)?;
    d.chord(&[Key::Tab, Key::Tab])?;
    arrival_text(d, "312.5")?;
    d.click("Linear")?;
    let label = d
        .app()
        .presentation
        .displayed_label()
        .ok_or("Camera has no displayed picture")?;
    d.click(&label)?;
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "One viewer Enter saves the validated arrival and curve as one Follow edit",
        framing(d) == Some(expected.clone()) && d.app().camera.is_none(),
        json!(expected),
        json!(framing(d)),
    )?;
    undo_to(d, animated)
}

fn retune_command(d: &mut Driver<'_>, animated: &Option<Framing>) -> Result<(), String> {
    let expected = deadpan_core::framing_zoom::retune_follow(
        animated.as_ref().ok_or("Missing animated Follow")?,
        None,
        ExactRatio::new(25, 8).unwrap(),
        ZoomCurve::Smoothstep,
    )?;
    d.chord(&[Key::Q, Key::Z])?;
    let revision = d.revision();
    d.command("creep to=3.125 curve=smoothstep")?;
    d.changed(&revision)?;
    idle(d)?;
    d.key(Key::Q)?;
    d.wait_for("Follow retune macro saves", |app| {
        !app.service.is_busy() && !app.macros.recording() && !app.macros.is_pending()
    })?;
    let program = d
        .app()
        .copied
        .entries()
        .find_map(|(name, content)| match content {
            Content::Macro(program) if name == 'z' => Some(program.instructions().to_vec()),
            _ => None,
        });
    d.check(
        "Retuning existing Follow records Zoom Keep intent without concrete target or window",
        framing(d) == Some(expected.clone())
            && program
                == Some(vec![I::Zoom {
                    kind: ZoomKind::Creep {
                        from: None,
                        to: ExactRatio::new(25, 8).unwrap(),
                        curve: ZoomCurve::Smoothstep,
                    },
                    center: SemanticZoomCenter::Keep,
                }]),
        json!(expected),
        json!({"framing":framing(d),"program":program}),
    )?;
    undo_to(d, animated)?;
    let revision = d.revision();
    super::super::macros::execute(d, 'z', None)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Saved Follow retune re-resolves the invocation's existing animation",
        framing(d) == Some(expected.clone()),
        json!(expected),
        json!(framing(d)),
    )?;
    undo_to(d, animated)?;
    let revision = d.revision();
    d.command("creep to=3.125 curve=smoothstep")?;
    d.changed(&revision)?;
    idle(d)?;
    undo_to(d, animated)?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Dot retunes arrival and curve while preserving this invocation's Follow window",
        framing(d) == Some(expected.clone()),
        json!(expected),
        json!(framing(d)),
    )?;
    undo_to(d, animated)
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
                        ..
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
                        ..
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
            scale_transition: Some(FollowScaleTransition { factor, curve: ZoomCurve::Linear, range: None }), .. }, .. })
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
    retune_controls(d, &animated)?;
    retune_command(d, &animated)?;
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
            target, scale, fallback, scale_transition: None, .. }, .. })
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
    undo_to(d, &fixed)?;
    ranged(d, &fixed)
}

fn at(d: &mut Driver<'_>, frame: u64) -> Result<(), String> {
    d.key(Key::Escape)?;
    let mut keys = vec![Key::G, Key::G];
    if frame > 0 {
        for digit in frame.to_string().chars() {
            keys.push(Key::from_name(&digit.to_string()).ok_or("Invalid frame digit")?);
        }
        keys.push(Key::L);
    }
    d.chord(&keys)?;
    d.settled()
}

fn select(d: &mut Driver<'_>, anchor: u64, head: u64) -> Result<(), String> {
    at(d, anchor)?;
    d.key(Key::V)?;
    let mut keys = Vec::new();
    for digit in anchor.abs_diff(head).to_string().chars() {
        keys.push(Key::from_name(&digit.to_string()).ok_or("Invalid range digit")?);
    }
    keys.push(if head > anchor { Key::L } else { Key::H });
    keys.push(Key::V);
    d.chord(&keys)?;
    d.settled()
}

fn point_saved(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Point save retains Camera on the new head", |app| {
        app.camera.as_ref().is_some_and(|camera| {
            let state = camera.harness_state();
            state["saving"] == false
                && state["rebasing"] == false
                && state["phase"] == "Adjust"
                && app.workspace.as_ref().is_some_and(|workspace| {
                    state["revision"] == workspace.document.revision_id().as_str()
                })
        })
    })?;
    d.key(Key::Escape)?;
    idle(d)
}

fn window(framing: &Option<Framing>) -> Option<deadpan_core::FollowScaleRange> {
    match &framing.as_ref()?.value {
        FramingValue::Follow {
            scale_transition: Some(transition),
            ..
        } => transition.range,
        _ => None,
    }
}

fn ranged(d: &mut Driver<'_>, original: &Option<Framing>) -> Result<(), String> {
    let ratio = |n, den| ExactRatio::new(n, den).unwrap();
    let source_cursor = d.app().source_cursor;
    let duration = d.app().sequence_length();
    at(d, 10)?;
    camera(d)?;
    d.chord(&[Key::N, Key::N, Key::Enter])?;
    point_saved(d)?;
    at(d, 20)?;
    camera(d)?;
    d.chord(&[Key::F, Key::Num2, Key::C, Key::Num8, Key::L, Key::Enter])?;
    point_saved(d)?;
    let target = d
        .app()
        .workspace
        .as_ref()
        .unwrap()
        .document
        .targets()
        .get(&deadpan_core::TargetId::new("target-2").unwrap())
        .cloned()
        .ok_or("Missing ranged Follow point")?;
    d.check(
        "A native point correction supplies distinct live centers within the creep range",
        target.region.is_point()
            && target.samples.is_empty()
            && target.corrections.len() == 1
            && target.region.center_ratio() != target.corrections[0].region.center_ratio(),
        json!({"kind":"point","corrections":1,"tracked_samples":0}),
        json!(target),
    )?;
    let revision = d.revision();
    d.command("zoom 1.35 target=target-2")?;
    d.changed(&revision)?;
    idle(d)?;
    let fixed = framing(d);
    select(d, 15, 25)?;
    d.chord(&[Key::Q, Key::Y])?;
    let revision = d.revision();
    d.command("creep from=1.5 to=2.25 curve=linear")?;
    d.changed(&revision)?;
    idle(d)?;
    let animated = framing(d);
    let selected = d.app().selected_edit_range();
    let poses: Vec<_> = [0, 12, 18, 22, 30]
        .into_iter()
        .map(|frame| planned_pose(d, frame))
        .collect();
    d.check("Ranged Follow preserves its prefix and keeps a moving center through and after the scale ramp",
        window(&animated) == Some(deadpan_core::FollowScaleRange {
            start: ratio(15, i128::from(duration)), end: ratio(25, i128::from(duration)),
            from_factor: ratio(10, 9),
        }) && poses.iter().all(Option::is_some)
            && (scale_of(poses[0]) - 1.35).abs() < 1e-7
            && (scale_of(poses[1]) - 1.35).abs() < 1e-7
            && (scale_of(poses[2]) - 1.7625).abs() < 1e-7
            && (scale_of(poses[3]) - 2.0625).abs() < 1e-7
            && (scale_of(poses[4]) - 2.25).abs() < 1e-7
            && poses[2].unwrap().center_x != poses[3].unwrap().center_x
            && poses[3].unwrap().center_x == poses[4].unwrap().center_x
            && d.app().message.as_deref().is_some_and(|message| message.contains("end scale while continuing to follow"))
            && d.app().source_cursor == source_cursor && d.app().sequence_length() == duration,
        json!({"range":[15,25],"scales":[1.35,1.35,1.7625,2.0625,2.25],"live_center":"changes at correction","Original":source_cursor}),
        json!({"framing":animated,"poses":poses,"message":d.app().message}))?;
    d.key(Key::Q)?;
    d.wait_for("Ranged Follow macro saves", |app| {
        !app.service.is_busy() && !app.macros.recording() && !app.macros.is_pending()
    })?;
    let program = d
        .app()
        .copied
        .entries()
        .find_map(|(name, content)| match content {
            Content::Macro(program) if name == 'y' => Some(program.instructions().to_vec()),
            _ => None,
        });
    d.check(
        "Ranged Follow records only scale, curve and Keep intent",
        program
            == Some(vec![I::Zoom {
                kind: ZoomKind::Creep {
                    from: Some(ratio(3, 2)),
                    to: ratio(9, 4),
                    curve: ZoomCurve::Linear,
                },
                center: SemanticZoomCenter::Keep,
            }]),
        json!({"owner":null,"range":null,"center":"keep"}),
        json!(program),
    )?;
    d.capture("Ranged Follow keeps the target live and shows its saved scale window")?;
    let description = d
        .app()
        .inspector_description()
        .ok_or("Missing ranged Follow inspector")?;
    let inspector = d.rect("Selected beat inspector pane")?;
    let complete = |row: &serde_json::Value, text: &str| {
        row["text"].as_str() == Some(text)
            && row["elided"] == false
            && row["fully_visible"] == true
            && row["bounds"][0]
                .as_f64()
                .is_some_and(|x| x >= f64::from(inspector.left()))
            && row["bounds"][1]
                .as_f64()
                .is_some_and(|y| y >= f64::from(inspector.bottom()))
    };
    for label in ["Scale before", "Creep", "Window"] {
        let value = description
            .fields
            .iter()
            .find(|(name, _)| *name == label)
            .map(|(_, value)| value)
            .ok_or("Missing ranged Follow detail")?;
        let labels = super::super::scenarios::text_paint_visibility(d, label);
        let paint = super::super::scenarios::text_paint_visibility(d, value);
        let same_row = labels.iter().any(|key| {
            complete(key, label)
                && paint.iter().any(|row| {
                    complete(row, value)
                        && row["bounds"][0].as_f64() > key["bounds"][2].as_f64()
                        && row["bounds"][1]
                            .as_f64()
                            .zip(key["bounds"][1].as_f64())
                            .is_some_and(|(value_y, label_y)| (value_y - label_y).abs() <= 8.0)
                })
        });
        d.check(
            "The ranged Follow inspector paints the complete saved scale and window text",
            same_row,
            json!({"field":label,"value":value,"fully_visible":true,"elided":false,"same_inspector_row":true}),
            json!({"labels":labels,"values":paint}),
        )?;
    }
    let unchanged = d.app().workspace.as_ref().unwrap().document.clone();
    for command in [
        "zoom 2",
        "zoom off",
        "creep to=3",
        "creep to=3 target=current",
    ] {
        let revision = d.revision();
        d.command(command)?;
        idle(d)?;
        d.check("A ranged Follow animation refuses implicit replacement and target sampling without changing any authored field",
            d.revision() == revision && d.app().workspace.as_ref().unwrap().document == unchanged
                && d.app().selected_edit_range() == selected && d.app().error.is_some(),
            json!({"command":command,"authored":"unchanged","range":selected}), d.snapshot())?;
    }
    let history_cursor = d.app().sequence_cursor;
    undo_to(d, &fixed)?;
    let revision = d.revision();
    d.command("redo")?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Redo restores the complete ranged Follow without reviving an expired Visual selection",
        framing(d) == animated
            && d.app().selected_edit_range().is_none()
            && d.app().sequence_cursor == history_cursor
            && d.app().source_cursor == source_cursor,
        json!({"framing":animated,"range":null,"Edit_cursor":history_cursor,"Original_cursor":source_cursor}),
        json!({"framing":framing(d),"range":d.app().selected_edit_range(),"Edit_cursor":d.app().sequence_cursor,"Original_cursor":d.app().source_cursor}),
    )?;
    at(d, 22)?;
    let revision = d.revision();
    camera(d)?;
    d.key(Key::Enter)?;
    idle(d)?;
    d.check(
        "Unchanged Camera Enter preserves the complete ranged Follow without a new revision",
        d.revision() == revision && framing(d) == animated,
        json!(animated),
        json!(framing(d)),
    )?;
    scale_adjustment(d, &animated)?;
    retune_controls(d, &animated)?;
    retune_command(d, &animated)?;
    undo_to(d, &fixed)?;
    select(d, 35, 25)?;
    let revision = d.revision();
    super::super::macros::execute(d, 'y', None)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Named ranged Follow rebinds a new backward invocation range",
        window(&framing(d)).is_some_and(|window| {
            window.start == ratio(25, i128::from(duration))
                && window.end == ratio(35, i128::from(duration))
        }) && d
            .app()
            .selected_edit_range()
            .is_some_and(|range| range.start().0 == 25 && range.end().0 == 35),
        json!({"range":[25,35],"recorded_range":[15,25]}),
        json!(framing(d)),
    )?;
    undo_to(d, &fixed)?;
    select(d, 15, 25)?;
    let revision = d.revision();
    d.command("creep from=1.5 to=2.25 curve=linear")?;
    d.changed(&revision)?;
    idle(d)?;
    undo_to(d, &fixed)?;
    select(d, 30, 40)?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Dot rebinds ranged Follow to fresh bounds after Undo",
        window(&framing(d)).is_some_and(|window| {
            window.start == ratio(30, i128::from(duration))
                && window.end == ratio(40, i128::from(duration))
        }),
        json!({"range":[30,40]}),
        json!(framing(d)),
    )?;
    undo_to(d, &fixed)?;
    undo_to(d, original)?;
    // The correction and the independent target save precede framing.
    undo_to(d, original)?;
    undo_to(d, original)?;
    at(d, 10)
}
