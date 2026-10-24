//! Whole-Hold Follow creep through actual native pause capture and commands.

use deadpan_core::{
    ExactRatio, FollowScaleTransition, Framing, FramingClock, FramingPose, FramingValue,
    HoldRecipe, HoldVideo, NodeId, NodeKind, ProjectFrame, SemanticInstruction, SemanticZoomCenter,
    TargetId, TargetPoint,
    framing_zoom::{ZoomCurve, ZoomKind},
};
use egui::Key;

use super::*;
use crate::preview::copied::Content;

fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

fn grid_center(x: ExactRatio) -> ExactRatio {
    FramingPose::new(x, ratio(1, 2), ExactRatio::ONE)
        .unwrap()
        .quantized()
        .unwrap()
        .center_x
}

fn camera(d: &Driver<'_>) -> Value {
    d.app()
        .camera
        .as_ref()
        .map_or(Value::Null, |camera| camera.harness_state())
}

fn framing(d: &Driver<'_>, id: &NodeId) -> Option<Framing> {
    d.app()
        .workspace
        .as_ref()?
        .document
        .nodes()
        .get(id)?
        .framing
        .clone()
}

fn recipe(d: &Driver<'_>, id: &NodeId) -> Option<HoldRecipe> {
    match &d.app().workspace.as_ref()?.document.nodes().get(id)?.kind {
        NodeKind::Hold { recipe } => Some(recipe.clone()),
        _ => None,
    }
}

fn target(d: &Driver<'_>, name: &str) -> Option<deadpan_core::AttentionTarget> {
    d.app()
        .workspace
        .as_ref()?
        .document
        .targets()
        .get(&TargetId::new(name).ok()?)
        .cloned()
}

fn pose(d: &Driver<'_>, id: &NodeId, frame: i64) -> Option<FramingPose> {
    d.app()
        .workspace
        .as_ref()?
        .plan
        .picture(ProjectFrame(frame))
        .ok()?
        .framing
        .iter()
        .find(|layer| !layer.escalation && &layer.instance.node == id)?
        .pose
}

fn idle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Hold framing command finishes", |app| {
        !app.service.is_busy() && !app.macros.is_pending() && app.camera_pending.is_none()
    })?;
    d.settled()
}

fn open_camera(d: &mut Driver<'_>) -> Result<(), String> {
    d.chord(&[Key::Comma, Key::F])?;
    d.wait_for("Camera opens on the exact Hold or Source", |app| {
        app.camera.is_some()
    })?;
    d.settled()
}

fn continued(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Camera continues on its point save", |app| {
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
    d.settled()
}

fn undo_to(d: &mut Driver<'_>, id: &NodeId, expected: &Option<Framing>) -> Result<(), String> {
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Undo restores the complete Hold framing recipe",
        framing(d, id) == *expected,
        json!(expected),
        json!(framing(d, id)),
    )
}

/// Independent integer Q32 oracle for linear 1.1 -> 2.2 at (j+1/2)/12.
fn expected_scale(j: i128) -> ExactRatio {
    let q = i128::from(deadpan_core::FRAMING_NUMERIC_SCALE);
    let round = |n: i128, d: i128| {
        let a = n / d;
        let r = n % d;
        a + i128::from(r * 2 > d || (r * 2 == d && a % 2 != 0))
    };
    let first = round(11 * q, 10);
    let last = round(11 * q, 5);
    let progress = round((2 * j + 1) * q, 24);
    ratio(first + round((last - first) * progress, q), q)
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Shot analysis stored before target creation", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.shot_analysis.is_some())
    })?;
    super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Num1, Key::Num0, Key::L])?;
    d.settled()?;
    let source = d
        .app()
        .selected_beat
        .clone()
        .ok_or("No initial Source selected")?;
    open_camera(d)?;
    d.chord(&[Key::N, Key::N, Key::Num8, Key::L, Key::Enter])?;
    continued(d)?;
    d.key(Key::Escape)?;
    idle(d)?;
    d.check(
        "The first point is saved independently of Source framing",
        target(d, "target-1").is_some_and(|target| {
            target.region
                == TargetPoint {
                    center: [580000, 500000],
                }
                .into()
        }) && framing(d, &source).is_none(),
        json!({"center":[580000,500000],"framing":null}),
        json!(target(d, "target-1")),
    )?;
    let revision = d.revision();
    d.command("zoom 2 target=target-1")?;
    d.changed(&revision)?;
    idle(d)?;
    d.chord(&[Key::G, Key::G, Key::Num4, Key::Num0, Key::L])?;
    d.settled()?;
    let original = d.app().workspace.clone().ok_or("No Original workspace")?;
    let revision = d.revision();
    d.command("hold 12f")?;
    d.changed(&revision)?;
    idle(d)?;
    let hold = d
        .app()
        .beat_rows
        .iter()
        .find(|row| {
            row.start == 40
                && d.app().workspace.as_ref().is_some_and(|workspace| {
                    matches!(
                        workspace
                            .document
                            .nodes()
                            .get(&row.id)
                            .map(|node| &node.kind),
                        Some(NodeKind::Hold { .. })
                    )
                })
        })
        .map(|row| row.id.clone())
        .ok_or("No ordinary Hold begins at Edit 40")?;
    if d.app().selected_beat.as_ref() != Some(&hold) {
        d.key(Key::J)?;
        d.settled()?;
    }
    let held = recipe(d, &hold).ok_or("The inserted Hold has no recipe")?;
    let (asset, freeze) = match &held.video {
        HoldVideo::Freeze { asset, timestamp } => (asset.clone(), *timestamp),
        _ => return Err("The native pause did not retain a Freeze provider".into()),
    };
    let expected_pts = d.app().workspace.as_ref().and_then(|workspace| {
        Some(
            workspace
                .sources
                .get(&asset)?
                .video_index
                .as_ref()?
                .frames()
                .get(39)?
                .pts,
        )
    });
    d.check("Native insertion captures a cropped 12-frame Hold at the preceding Original picture",
        d.app().selected_beat.as_ref() == Some(&hold)
            && held.duration.frames() == 12 && held.picture_context.is_some()
            && expected_pts == Some(freeze.ticks) && d.app().sequence_length() == 132,
        json!({"start":40,"frames":12,"captured":true,"source_ordinal":39,"total":132}),
        json!({"selected":d.app().selected_beat,"hold":hold,"recipe":held,"pts":freeze.ticks,"total":d.app().sequence_length()}))?;

    // The new point differs from the captured crop center. Only actual
    // projection can produce .7; the neutral fallback remains .5.
    open_camera(d)?;
    d.chord(&[Key::N, Key::N, Key::Num1, Key::Num0, Key::L, Key::Enter])?;
    continued(d)?;
    d.check(
        "Point creation on the captured Hold uses upright source coordinates",
        target(d, "target-2").is_some_and(|target| {
            target.region
                == TargetPoint {
                    center: [680000, 500000],
                }
                .into()
                && target.span.start().ticks == freeze.ticks
        }),
        json!({"center":[680000,500000],"pts":freeze.ticks}),
        json!(target(d, "target-2")),
    )?;
    d.key(Key::T)?;
    d.settled()?;
    let live: FramingPose = serde_json::from_value(camera(d)["draft_pose"].clone())
        .map_err(|error| error.to_string())?;
    d.check(
        "Camera follows through the captured crop instead of using the fallback center",
        live.center_x == grid_center(ratio(7, 10))
            && live.center_y == ratio(1, 2)
            && camera(d)["follow"] == "target-2",
        json!({"live_center":["0.7","0.5"],"fallback_center":["0.5","0.5"]}),
        json!(camera(d)),
    )?;
    d.check(
        "Nearby secondary point markers do not overpaint the active target label",
        scenarios::text_paint_visibility(d, "Target 1 · drawn · point").is_empty()
            && !scenarios::text_paint_visibility(d, "Target 2 · drawn · following · point").is_empty(),
        json!({"secondary":"marker only","active":"full label"}),
        json!({"secondary_labels":scenarios::text_paint_visibility(d,"Target 1 · drawn · point"),
            "active_labels":scenarios::text_paint_visibility(d,"Target 2 · drawn · following · point")}),
    )?;
    d.key(Key::F)?;
    d.step("Compact saved-target picker", true)?;
    let picker = camera(d);
    let widgets = d.widgets().to_string();
    d.check(
        "Multiple saved targets use compact overlay digits and retain full inspector names",
        picker["picker"][0]["number"] == 1
            && picker["picker"][0]["label"] == "Target 1"
            && picker["picker"][1]["number"] == 2
            && picker["picker"][1]["label"] == "Target 2"
            && scenarios::text_paint_visibility(d, "1 Target 1 · drawn · point").is_empty()
            && widgets.contains("Target 1")
            && widgets.contains("Target 2"),
        json!({"digits":[1,2],"names":["Target 1","Target 2"]}),
        picker,
    )?;
    d.key(Key::F)?;
    d.settled()?;
    d.capture("Hold Camera resolves the target through its retained crop")?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    idle(d)?;
    let fixed = framing(d, &hold);
    d.check("The saved Hold Follow retains a distinct neutral fallback and the captured recipe",
        matches!(&fixed, Some(Framing {value:FramingValue::Follow {target, fallback, scale_transition:None, ..}, ..})
            if target.as_str()=="target-2" && fallback.center_x==ratio(1,2) && fallback.center_y==ratio(1,2))
            && pose(d,&hold,40).is_some_and(|pose| pose.center_x==grid_center(ratio(7,10)))
            && recipe(d,&hold)==Some(held.clone()),
        json!({"live_center":"0.7","fallback":"0.5","captured_recipe":"unchanged"}), json!(fixed))?;
    let others = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No workspace")?
        .document
        .nodes()
        .iter()
        .filter(|(id, _)| *id != &hold)
        .map(|(id, node)| (id.clone(), node.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let original_cursor = d.app().source_cursor;
    let edit_cursor = d.app().sequence_cursor;
    let before_creep = d.app().workspace.clone().ok_or("No Hold workspace")?;
    d.chord(&[Key::Q, Key::X])?;
    let revision = d.revision();
    d.command("creep from=1.1 to=2.2 curve=linear")?;
    d.changed(&revision)?;
    idle(d)?;
    let animated = framing(d, &hold);
    d.key(Key::Q)?;
    d.wait_for("Whole-Hold creep macro saves", |app| {
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
    let expected_program = vec![SemanticInstruction::Zoom {
        kind: ZoomKind::Creep {
            from: Some(ratio(11, 10)),
            to: ratio(11, 5),
            curve: ZoomCurve::Linear,
        },
        center: SemanticZoomCenter::Keep,
    }];
    let unchanged_others = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No workspace")?
        .document
        .nodes()
        .iter()
        .filter(|(id, _)| *id != &hold)
        .map(|(id, node)| (id.clone(), node.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    d.check("Whole-Hold creep records Keep intent and preserves timing, media and every other node",
        matches!(&animated,Some(Framing {clock:FramingClock::OwnerOutput,value:FramingValue::Follow {
            target,scale,fallback,scale_transition:Some(FollowScaleTransition {factor,curve:ZoomCurve::Linear})}})
            if target.as_str()=="target-2" && *scale==ratio(11,10) && fallback.scale==ratio(11,10) && *factor==ExactRatio::integer(2))
            && recipe(d,&hold)==Some(held.clone()) && others==unchanged_others
            && d.app().sequence_length()==132 && d.app().source_cursor==original_cursor && d.app().sequence_cursor==edit_cursor
            && program==Some(expected_program.clone()),
        json!({"initial":"1.1","terminal":"2.2","factor":2,"owner_frames":12,"total":132,"macro":expected_program}),
        json!({"framing":animated,"recipe":recipe(d,&hold),"macro":program,"total":d.app().sequence_length()}))?;
    let mut boundaries = Vec::new();
    let mut boundaries_exact = true;
    for (frame, ordinal, is_hold) in [
        (39, 39, false),
        (40, 39, true),
        (51, 39, true),
        (52, 40, false),
    ] {
        let workspace = d.app().workspace.as_ref().ok_or("No workspace")?;
        let sample = workspace
            .plan
            .picture(ProjectFrame(frame))
            .map_err(|error| error.to_string())?;
        let before = before_creep
            .plan
            .picture(ProjectFrame(frame))
            .map_err(|error| error.to_string())?;
        let original_sample = original
            .plan
            .picture(ProjectFrame(ordinal))
            .map_err(|error| error.to_string())?;
        let index = workspace
            .sources
            .get(&asset)
            .and_then(|source| source.video_index.as_ref())
            .ok_or("No qualified Original index")?;
        let shown_frame = sample
            .picture
            .select_source_frame(index)
            .map_err(|error| error.to_string())?;
        let has_hold_layer = sample
            .framing
            .iter()
            .any(|layer| !layer.escalation && layer.instance.node == hold);
        let provider_and_point = if is_hold {
            matches!(&sample.picture, deadpan_plan::Picture::Freeze { asset: found, point }
                if found == &asset && point.ticks == ExactRatio::integer(freeze.ticks)
                    && point.time_base == freeze.time_base)
                && sample.picture.follow_point().is_some_and(|(found, point)| {
                    found == &asset
                        && point.ticks == ExactRatio::integer(freeze.ticks)
                        && point.time_base == freeze.time_base
                })
                && sample.instance.node == hold
                && sample.picture_context.as_deref() == held.picture_context.as_ref()
        } else {
            matches!(&sample.picture, deadpan_plan::Picture::Source { .. })
                && sample.picture.follow_point() == original_sample.picture.follow_point()
                && sample.instance.node != hold
                && sample.instance == before.instance
                && sample.gap_after == before.gap_after
                && sample.local_position == before.local_position
                && sample.picture == before.picture
                && sample.picture_context == before.picture_context
                && sample.framing == before.framing
                && sample.captions == before.captions
        };
        boundaries_exact &= provider_and_point
            && has_hold_layer == is_hold
            && index
                .frames()
                .get(usize::try_from(ordinal).map_err(|error| error.to_string())?)
                == Some(shown_frame);
        boundaries.push(json!({"edit_frame":frame,"original_ordinal":ordinal,"hold_layer":has_hold_layer,
            "picture":sample.picture,"follow_point":format!("{:?}",sample.picture.follow_point()),
            "framing":sample.framing,"context":sample.picture_context.as_deref(),"provider_and_point":provider_and_point}));
    }
    d.check(
        "Hold boundaries are half-open and creep preserves both adjacent Source plans",
        boundaries_exact,
        json!({"edit_frames":[39,40,51,52],"original_ordinals":[39,39,39,40],"hold_layers":[false,true,true,false],"adjacent_source_plans":"unchanged"}),
        json!(boundaries),
    )?;
    let mut observed = Vec::new();
    let mut all_exact = true;
    for j in 0..12 {
        let sample = d
            .app()
            .workspace
            .as_ref()
            .ok_or("No workspace")?
            .plan
            .picture(ProjectFrame(40 + j))
            .map_err(|error| error.to_string())?;
        let layer = sample
            .framing
            .iter()
            .find(|layer| !layer.escalation && layer.instance.node == hold)
            .ok_or("Hold framing layer missing")?;
        let shown = layer.pose.ok_or("Hold Follow has no pose")?;
        let source = sample.picture.follow_point();
        all_exact &= shown.center_x == grid_center(ratio(7, 10))
            && shown.center_y == ratio(1, 2)
            && shown.scale == expected_scale(i128::from(j))
            && layer.local_position == ratio(i128::from(2 * j + 1), 2)
            && layer.effective_duration == ExactRatio::integer(12)
            && source.is_some_and(|(found, point)| {
                found == &asset
                    && point.ticks == ExactRatio::integer(freeze.ticks)
                    && point.time_base == freeze.time_base
            })
            && sample.picture_context.as_deref() == held.picture_context.as_ref();
        observed.push(json!({"local":j,"pose":shown,"expected_scale":expected_scale(i128::from(j)),"source":format!("{source:?}")}));
    }
    d.check(
        "Every Hold picture follows the projected point on the exact 12-frame owner clock",
        all_exact,
        json!({"center_x":"0.7","source_ordinal":39,"frames":12,"sampling":"frame centers"}),
        json!(observed),
    )?;
    d.capture("Whole-Hold creep keeps the target live through captured geometry")?;

    undo_to(d, &hold, &fixed)?;
    let revision = d.revision();
    d.command("redo")?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Redo restores the complete captured-Hold Follow creep",
        framing(d, &hold) == animated,
        json!(animated),
        json!(framing(d, &hold)),
    )?;
    open_camera(d)?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.wait_for("Unchanged Hold Camera closes", |app| {
        app.camera.is_none() && !app.service.is_busy()
    })?;
    d.check(
        "Unchanged Camera Enter preserves Hold animation without history",
        d.revision() == revision && framing(d, &hold) == animated,
        json!(animated),
        json!(framing(d, &hold)),
    )?;
    open_camera(d)?;
    d.key(Key::Plus)?;
    d.key(Key::Escape)?;
    idle(d)?;
    d.check(
        "Camera cancel restores the exact Hold Follow and retained capture",
        d.revision() == revision
            && framing(d, &hold) == animated
            && recipe(d, &hold) == Some(held.clone()),
        json!(animated),
        json!(framing(d, &hold)),
    )?;

    undo_to(d, &hold, &fixed)?;
    let revision = d.revision();
    super::macros::execute(d, 'x', None)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "The saved macro re-resolves whole-Hold Follow creep",
        framing(d, &hold) == animated,
        json!(animated),
        json!(framing(d, &hold)),
    )?;
    undo_to(d, &hold, &fixed)?;
    let revision = d.revision();
    d.command("creep from=1.1 to=2.2 curve=linear")?;
    d.changed(&revision)?;
    idle(d)?;
    undo_to(d, &hold, &fixed)?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Dot after Undo retains whole-Hold Keep intent",
        framing(d, &hold) == animated,
        json!(animated),
        json!(framing(d, &hold)),
    )?;

    // A later outer scale cannot bring a target back through an earlier clip.
    let saved_target = target(d, "target-2").ok_or("Missing Hold point")?;
    open_camera(d)?;
    d.key(Key::C)?;
    d.chord(&[Key::Num3, Key::Num0, Key::L, Key::Enter])?;
    continued(d)?;
    let clipped_target = target(d, "target-2").ok_or("Missing corrected Hold point")?;
    let clipped_pose: FramingPose = serde_json::from_value(camera(d)["draft_pose"].clone())
        .map_err(|error| error.to_string())?;
    let mut clipped_frames = Vec::new();
    let mut all_clipped_exact = true;
    for j in 0..12 {
        let sample = d
            .app()
            .workspace
            .as_ref()
            .ok_or("No workspace")?
            .plan
            .picture(ProjectFrame(40 + j))
            .map_err(|error| error.to_string())?;
        let layer = sample
            .framing
            .iter()
            .find(|layer| !layer.escalation && layer.instance.node == hold)
            .ok_or("Clipped Hold framing layer missing")?;
        let shown = layer
            .pose
            .ok_or("Clipped Hold Follow has no fallback pose")?;
        let source = sample.picture.follow_point();
        all_clipped_exact &= shown.center_x == ratio(1, 2)
            && shown.center_y == ratio(1, 2)
            && shown.scale == expected_scale(i128::from(j))
            && layer.local_position == ratio(i128::from(2 * j + 1), 2)
            && layer.effective_duration == ExactRatio::integer(12)
            && matches!(&sample.picture, deadpan_plan::Picture::Freeze { asset: found, point }
                if found == &asset && point.ticks == ExactRatio::integer(freeze.ticks)
                    && point.time_base == freeze.time_base)
            && source.is_some_and(|(found, point)| {
                found == &asset
                    && point.ticks == ExactRatio::integer(freeze.ticks)
                    && point.time_base == freeze.time_base
            })
            && sample.picture_context.as_deref() == held.picture_context.as_ref();
        clipped_frames.push(
            json!({"local":j,"pose":shown,"expected_scale":expected_scale(i128::from(j)),
            "source":format!("{source:?}"),"context":sample.picture_context.as_deref()}),
        );
    }
    d.check("A corrected point clipped by the retained source crop uses the fallback in Camera and plan",
        clipped_target.region==TargetPoint {center:[980000,500000]}.into()
            && clipped_pose.center_x==ratio(1,2) && clipped_pose.center_y==ratio(1,2)
            && all_clipped_exact
            && framing(d,&hold)==animated && recipe(d,&hold)==Some(held.clone()),
        json!({"source_center":"0.98","fallback_center":["0.5","0.5"],"animation":"retained","frames":12}),json!({"camera":camera(d),"frames":clipped_frames,"target":clipped_target}))?;
    d.capture("Captured clipping keeps the target on its explicit fallback")?;
    d.key(Key::Escape)?;
    idle(d)?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Undo restores the actual projected Hold target without changing its captured recipe",
        target(d, "target-2") == Some(saved_target)
            && framing(d, &hold) == animated
            && recipe(d, &hold) == Some(held)
            && pose(d, &hold, 40).is_some_and(|pose| pose.center_x == grid_center(ratio(7, 10)))
            && d.app().sequence_length() == 132,
        json!({"center":"0.7","target":"restored","total":132}),
        json!({"pose":pose(d,&hold,40),"framing":framing(d,&hold)}),
    )?;
    d.capture("Resolved whole-Hold Follow creep committed")
}
