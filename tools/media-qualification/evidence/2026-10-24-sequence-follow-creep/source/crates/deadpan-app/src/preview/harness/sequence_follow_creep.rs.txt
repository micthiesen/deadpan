//! Whole ordinary Sequence Follow creep across a noncontiguous Original cut.
//! Every authored change uses production Camera keys or the project writer.

use super::*;
use crate::preview::copied::Content;
use deadpan_core::{
    AudioSample, ExactRatio, FollowScaleTransition, Framing, FramingClock, FramingPose,
    FramingValue, NodeId, NodeKind, ProjectFrame, SemanticInstruction, SemanticZoomCenter,
    TargetId, TargetPoint,
    framing_zoom::{ZoomCurve, ZoomKind},
};
use egui::Key;

fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

fn center(x: i128) -> ExactRatio {
    FramingPose::new(ratio(x, 100), ratio(1, 2), ExactRatio::ONE)
        .unwrap()
        .quantized()
        .unwrap()
        .center_x
}

/// Independent integer, half-even Q32 oracle at frame centers of 80 frames.
fn expected_scale(j: i128) -> ExactRatio {
    let q = i128::from(deadpan_core::FRAMING_NUMERIC_SCALE);
    let round = |n: i128, d: i128| {
        let a = n / d;
        let r = n % d;
        a + i128::from(r * 2 > d || (r * 2 == d && a % 2 != 0))
    };
    let first = round(11 * q, 10);
    let last = round(11 * q, 5);
    let progress = round((2 * j + 1) * q, 160);
    ratio(first + round((last - first) * progress, q), q)
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

fn camera(d: &Driver<'_>) -> Value {
    d.app()
        .camera
        .as_ref()
        .map_or(Value::Null, |camera| camera.harness_state())
}

fn idle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Sequence framing receipt finishes", |app| {
        !app.service.is_busy() && !app.macros.is_pending() && app.camera_pending.is_none()
    })?;
    d.settled()
}

fn at(d: &mut Driver<'_>, frame: u32) -> Result<(), String> {
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

fn edit(d: &mut Driver<'_>, command: &str) -> Result<(), String> {
    let revision = d.revision();
    d.command(command)?;
    d.changed(&revision)?;
    idle(d)
}

fn open_camera(d: &mut Driver<'_>) -> Result<(), String> {
    d.chord(&[Key::Comma, Key::F])?;
    d.wait_for("Camera opens on the selected ordinary owner", |app| {
        app.camera.is_some()
    })?;
    d.settled()
}

fn continued(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Point save continues Camera on the saved head", |app| {
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

fn undo_to(d: &mut Driver<'_>, group: &NodeId, expected: &Option<Framing>) -> Result<(), String> {
    edit(d, "undo")?;
    d.check(
        "One Undo restores the exact Sequence Follow recipe",
        framing(d, group) == *expected,
        json!(expected),
        json!(framing(d, group)),
    )
}

fn refused(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let revision = d.revision();
    let document = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No workspace")?
        .document
        .clone();
    d.command("creep from=1.1 to=2.2 curve=linear")?;
    idle(d)?;
    d.check(
        label,
        d.revision() == revision
            && d.app()
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.document == document)
            && d.app().error.is_some(),
        json!({"revision":revision,"write":false}),
        json!({"revision":d.revision(),"error":d.app().error}),
    )
}

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Shot analysis stored before target authoring", |app| {
        app.workspace
            .as_ref()
            .is_some_and(|workspace| workspace.shot_analysis.is_some())
    })?;
    super::transcript::focus_your_edit(d)?;
    at(d, 0)?;
    let original = d.app().workspace.clone().ok_or("No Original workspace")?;
    d.check(
        "Sequence witness starts with 120 Original pictures at 30000/1001",
        d.app().sequence_length() == 120
            && original.document.presentation_basis().frame_rate
                == deadpan_core::FrameRate::new(30000, 1001).map_err(|error| error.to_string())?,
        json!({"frames":120,"rate":"30000/1001"}),
        json!({"frames":d.app().sequence_length(),"rate":original.document.presentation_basis().frame_rate}),
    )?;

    open_camera(d)?;
    d.chord(&[Key::N, Key::N, Key::Num8, Key::L, Key::Enter])?;
    continued(d)?;
    d.key(Key::Escape)?;
    idle(d)?;
    at(d, 80)?;
    open_camera(d)?;
    d.chord(&[
        Key::F,
        Key::Num1,
        Key::C,
        Key::Num1,
        Key::Num0,
        Key::L,
        Key::Enter,
    ])?;
    continued(d)?;
    d.key(Key::Escape)?;
    idle(d)?;
    let target = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No workspace")?
        .document
        .targets()
        .get(&TargetId::new("target-1").unwrap())
        .cloned()
        .ok_or("No saved point")?;
    let index = original
        .sources
        .get(&target.asset)
        .and_then(|source| source.video_index.as_ref())
        .ok_or("No qualified Original index")?;
    d.check("Manual correction gives the same point distinct centers on either side of the future cut",
        target.region == TargetPoint { center:[580000,500000] }.into()
            && target.corrections.len() == 1 && target.corrections[0].at == index.frames()[80].pts
            && target.corrections[0].region == TargetPoint {center:[680000,500000]}.into()
            && target.span.start().ticks == index.frames()[0].pts && target.samples.is_empty()
            && d.app().targets.job().is_none()
            && d.app().workspace.as_ref().unwrap().document.nodes().values().all(|node| node.framing.is_none()),
        json!({"early":[580000,500000],"later":[680000,500000],"correction_ordinal":80,"framing":null}), json!(target))?;

    // Remove [40,80), leaving [0,40) followed by [80,120).
    at(d, 40)?;
    d.chord(&[Key::V, Key::Num4, Key::Num0, Key::L])?;
    edit(d, "delete")?;
    at(d, 0)?;
    d.chord(&[Key::V, Key::Num8, Key::Num0, Key::L])?;
    edit(d, "group name=\"Sequence Follow cut\"")?;
    let group = d
        .app()
        .selected_beat
        .clone()
        .ok_or("Group is not selected")?;
    let grouped = d.app().workspace.clone().ok_or("No grouped workspace")?;
    let children = match &grouped.document.nodes()[&group].kind {
        NodeKind::Sequence { children } => children.clone(),
        _ => return Err("The native grouping command did not select a Sequence".into()),
    };
    d.check("The selected whole ordinary Sequence contains two noncontiguous 40-frame Original fragments",
        children.len() == 2 && grouped.document.children(grouped.document.root()).any(|id| id == &group)
            && children.iter().all(|id| matches!(grouped.document.nodes()[id].kind, NodeKind::Source {..} | NodeKind::Retime {..})
                && grouped.document.node_duration(id).is_ok_and(|duration| duration.frames() == 40))
            && d.app().sequence_scope.groups().is_empty() && !d.app().edit_range.has_bounds()
            && d.app().sequence_length() == 80,
        json!({"children":2,"child_frames":[40,40],"Original":[[0,40],[80,120]],"total":80}),
        json!({"group":group,"children":children,"frames":d.app().sequence_length(),"scope":d.app().sequence_scope.groups()}))?;

    at(d, 0)?;
    open_camera(d)?;
    d.chord(&[Key::F, Key::Num1, Key::T])?;
    d.settled()?;
    let draft: FramingPose = serde_json::from_value(camera(d)["draft_pose"].clone())
        .map_err(|error| error.to_string())?;
    d.check(
        "Camera previews Follow on the selected group at the early target center",
        d.app().selected_beat.as_ref() == Some(&group)
            && draft.center_x == center(58)
            && camera(d)["follow"] == "target-1",
        json!({"owner":group,"center":".58","target":"target-1"}),
        camera(d),
    )?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    idle(d)?;
    let fixed = framing(d, &group);
    d.check("Follow is authored on the Sequence rather than either Source child",
        matches!(&fixed, Some(Framing {value:FramingValue::Follow {target,scale_transition:None,..},..}) if target.as_str()=="target-1")
            && children.iter().all(|id| framing(d,id).is_none()), json!(fixed), json!(framing(d,&group)))?;

    d.chord(&[Key::V, Key::Num4, Key::L])?;
    refused(
        d,
        "A Visual subrange refuses whole-Sequence Follow creep without a write",
    )?;
    d.key(Key::Escape)?;
    at(d, 0)?;
    let baseline = d
        .app()
        .workspace
        .clone()
        .ok_or("No fixed Follow workspace")?;
    let original_cursor = d.app().source_cursor;
    let edit_cursor = d.app().sequence_cursor;
    let audio_end = baseline
        .plan
        .audio_duration()
        .map_err(|error| error.to_string())?;
    let audio_before = baseline
        .plan
        .audio(AudioSample(0)..audio_end, Default::default())
        .map_err(|error| error.to_string())?;
    d.chord(&[Key::Q, Key::X])?;
    edit(d, "creep from=1.1 to=2.2 curve=linear")?;
    let animated = framing(d, &group);
    d.key(Key::Q)?;
    d.wait_for("Sequence creep recording saves", |app| {
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
    let current = d.app().workspace.clone().ok_or("No animated workspace")?;
    let audio_after = current
        .plan
        .audio(AudioSample(0)..audio_end, Default::default())
        .map_err(|error| error.to_string())?;
    let other_nodes_unchanged = baseline
        .document
        .nodes()
        .iter()
        .filter(|(id, _)| *id != &group)
        .all(|(id, node)| current.document.nodes().get(id) == Some(node));
    let mut expected_group = baseline.document.nodes()[&group].clone();
    expected_group.framing = animated.clone();
    d.check("One Keep creep changes only the group framing and records invocation-relative intent",
        matches!(&animated, Some(Framing {clock:FramingClock::OwnerOutput,value:FramingValue::Follow {
            target,scale,fallback,scale_transition:Some(FollowScaleTransition {factor,curve:ZoomCurve::Linear})}})
            if target.as_str()=="target-1" && *scale==ratio(11,10) && fallback.scale==ratio(11,10) && *factor==ExactRatio::integer(2))
            && other_nodes_unchanged && current.document.nodes().get(&group)==Some(&expected_group)
            && baseline.document.nodes().len()==current.document.nodes().len()
            && baseline.document.targets()==current.document.targets() && audio_before.spans==audio_after.spans
            && audio_end==AudioSample(128128)
            && current.plan.audio_duration().is_ok_and(|end| end==audio_end) && d.app().sequence_length()==80
            && d.app().source_cursor==original_cursor && d.app().sequence_cursor==edit_cursor
            && program==Some(expected_program.clone()),
        json!({"owner_frames":80,"from":"1.1","to":"2.2","factor":2,"audio":"unchanged","macro":expected_program}),
        json!({"framing":animated,"other_nodes_unchanged":other_nodes_unchanged,"macro":program,"audio_samples":audio_end}))?;

    let mut observed = Vec::new();
    let mut exact = true;
    for j in 0..80_i64 {
        let sample = current
            .plan
            .picture(ProjectFrame(j))
            .map_err(|error| error.to_string())?;
        let before = baseline
            .plan
            .picture(ProjectFrame(j))
            .map_err(|error| error.to_string())?;
        let ordinal = if j < 40 { j } else { j + 40 };
        let original_sample = original
            .plan
            .picture(ProjectFrame(ordinal))
            .map_err(|error| error.to_string())?;
        let layer = sample
            .framing
            .iter()
            .find(|layer| !layer.escalation && layer.instance.node == group)
            .ok_or("Sequence Follow layer is absent")?;
        let shown = layer.pose.ok_or("Sequence Follow pose is absent")?;
        let child_layers = sample
            .framing
            .iter()
            .filter(|layer| layer.instance.node != group)
            .collect::<Vec<_>>();
        let prior_child_layers = before
            .framing
            .iter()
            .filter(|layer| layer.instance.node != group)
            .collect::<Vec<_>>();
        exact &= shown.center_x == center(if j < 40 { 58 } else { 68 })
            && shown.center_y == ratio(1, 2)
            && shown.scale == expected_scale(i128::from(j))
            && layer.local_position == ratio(i128::from(2 * j + 1), 2)
            && layer.effective_duration == ExactRatio::integer(80)
            && layer.duration.frames() == 80
            && sample.instance == before.instance
            && sample.gap_after == before.gap_after
            && sample.local_position == before.local_position
            && matches!(sample.picture, deadpan_plan::Picture::Source { .. })
            && sample.picture == before.picture
            && sample.picture.follow_point() == original_sample.picture.follow_point()
            && sample
                .picture
                .select_source_frame(index)
                .is_ok_and(|found| found == &index.frames()[usize::try_from(ordinal).unwrap()])
            && sample.picture_context == before.picture_context
            && child_layers == prior_child_layers;
        observed.push(json!({"edit":j,"Original":ordinal,"provider":sample.instance.node,"pose":shown,
            "expected_scale":expected_scale(i128::from(j)),"source_point":format!("{:?}",sample.picture.follow_point())}));
    }
    d.check("Every group frame uses the continuous 80-frame scale clock and its child's exact Original target time",
        exact, json!({"frames":80,"seam_Edit":[39,40],"seam_Original":[39,80],"centers":[".58",".68"],"sampling":"frame centers, Q32 half-even"}), json!(observed))?;

    for frame in [0, 39, 40, 79] {
        at(d, frame)?;
        let expected = current
            .plan
            .picture(ProjectFrame(i64::from(frame)))
            .map_err(|error| error.to_string())?;
        let displayed = d
            .app()
            .presentation
            .picture()
            .ok_or("No stopped Sequence picture")?;
        d.check(
            "Stopped picture presents the committed group pose across the cut",
            displayed.framing == expected.framing
                && d.app()
                    .presentation
                    .displayed_source_frame()
                    .is_some_and(|id| {
                        id.0 == u64::from(if frame < 40 { frame } else { frame + 40 })
                    }),
            json!({"Edit":frame,"framing":expected.framing}),
            json!({"framing":displayed.framing}),
        )?;
        if frame == 39 || frame == 40 {
            d.capture("Sequence Follow creep at the noncontiguous cut")?;
        }
    }
    at(d, 0)?;
    undo_to(d, &group, &fixed)?;
    edit(d, "redo")?;
    d.check(
        "Redo restores the complete group Follow creep",
        framing(d, &group) == animated,
        json!(animated),
        json!(framing(d, &group)),
    )?;
    let revision = d.revision();
    open_camera(d)?;
    d.key(Key::Enter)?;
    idle(d)?;
    d.check(
        "Unchanged Camera Enter preserves group animation without history",
        d.app().camera.is_none() && d.revision() == revision && framing(d, &group) == animated,
        json!({"revision":revision,"framing":animated}),
        json!({"revision":d.revision(),"framing":framing(d,&group)}),
    )?;
    open_camera(d)?;
    d.chord(&[Key::Plus, Key::Escape])?;
    idle(d)?;
    d.check(
        "Camera cancel preserves the complete group Follow",
        d.revision() == revision && framing(d, &group) == animated,
        json!(animated),
        json!(framing(d, &group)),
    )?;
    let installed = current
        .plan
        .picture(ProjectFrame(0))
        .map_err(|error| error.to_string())?
        .framing
        .into_iter()
        .find(|layer| !layer.escalation && layer.instance.node == group)
        .and_then(|layer| layer.pose)
        .ok_or("No installed group pose")?;
    open_camera(d)?;
    d.key(Key::T)?;
    d.settled()?;
    d.check(
        "Explicit release explains the removal of group scale animation",
        d.app().message.as_deref().is_some_and(|message| {
            message.contains("Scale animation removed") && message.contains("static framing")
        }),
        json!("Scale animation removed; Enter saves this static framing"),
        json!(d.app().message),
    )?;
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check("Explicit group release commits exactly its displayed static pose",
        matches!(framing(d,&group),Some(Framing {value:FramingValue::Static {pose},..}) if pose==installed),json!(installed),json!(framing(d,&group)))?;
    undo_to(d, &group, &animated)?;

    undo_to(d, &group, &fixed)?;
    let revision = d.revision();
    super::macros::execute(d, 'x', None)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Named replay re-resolves Keep for the selected whole group",
        framing(d, &group) == animated,
        json!(animated),
        json!(framing(d, &group)),
    )?;
    undo_to(d, &group, &fixed)?;
    edit(d, "creep from=1.1 to=2.2 curve=linear")?;
    undo_to(d, &group, &fixed)?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Dot after Undo repeats whole-group semantic Keep",
        framing(d, &group) == animated,
        json!(animated),
        json!(framing(d, &group)),
    )?;

    // Outer composite owners retain their explicit admission boundary.
    for command in ["wrap-repeat 2", "wrap-retime 2 pitch=tape"] {
        edit(d, command)?;
        edit(d, "zoom 2 target=target-1")?;
        refused(
            d,
            "Repeat and Retime owners refuse Keep Follow creep without a write",
        )?;
        edit(d, "undo")?;
        edit(d, "undo")?;
        d.check("Undoing the temporary wrapper restores the selected live group",
            d.app().selected_beat.as_ref()==Some(&group) && framing(d,&group)==animated && d.app().sequence_length()==80,
            json!({"selected":group,"frames":80,"framing":animated}),json!({"selected":d.app().selected_beat,"frames":d.app().sequence_length(),"framing":framing(d,&group)}))?;
    }
    at(d, 40)?;
    let final_workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No final Sequence workspace")?;
    let final_audio = final_workspace
        .plan
        .audio(AudioSample(0)..audio_end, Default::default())
        .map_err(|error| error.to_string())?;
    d.check("The final export fixture retains the exact live group, children, target and audio after history replay",
        final_workspace.document.nodes()==current.document.nodes()
            && final_workspace.document.targets()==current.document.targets()
            && final_audio.spans==audio_before.spans && d.app().sequence_length()==80
            && d.app().selected_beat.as_ref()==Some(&group) && d.app().sequence_scope.groups().is_empty(),
        json!({"frames":80,"audio_samples":128128,"owner":group,"live_follow":"target-1"}),
        json!({"frames":d.app().sequence_length(),"owner":d.app().selected_beat,"framing":framing(d,&group)}))?;
    d.capture("Whole Sequence Follow creep committed across the Original cut")
}
