//! Whole ordinary Repeat Follow creep across plays, silent gaps and escalation.
//! Every authored change uses production Camera keys or the project writer.

use super::*;
use crate::preview::copied::Content;
use deadpan_core::{
    AudioSample, ExactRatio, FollowScaleTransition, Framing, FramingClock, FramingPose,
    FramingValue, HoldAudio, HoldVideo, NodeId, NodeKind, ProjectFrame, RepeatLayout,
    SemanticInstruction, SemanticZoomCenter, TargetId, TargetPoint,
    framing_zoom::{ZoomCurve, ZoomKind},
};
use egui::Key;

fn ratio(n: i128, d: i128) -> ExactRatio {
    ExactRatio::new(n, d).unwrap()
}

fn round_even(n: i128, d: i128) -> i128 {
    let a = n / d;
    let r = n % d;
    a + i128::from(r * 2 > d || (r * 2 == d && a % 2 != 0))
}

/// Source point carried through the centered inner escalation, then Q32.
fn expected_center(percent: i128, play: i128) -> ExactRatio {
    let q = i128::from(deadpan_core::FRAMING_NUMERIC_SCALE);
    ratio(round_even((400 + (percent - 50) * (8 + play)) * q, 800), q)
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
    d.wait_for("Repeat framing receipt finishes", |app| {
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

fn undo_to(d: &mut Driver<'_>, repeat: &NodeId, expected: &Option<Framing>) -> Result<(), String> {
    edit(d, "undo")?;
    d.check(
        "One Undo restores the exact Repeat Follow recipe",
        framing(d, repeat) == *expected,
        json!(expected),
        json!(framing(d, repeat)),
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
    d.check("Repeat witness starts with 120 Original pictures at 30000/1001",
        d.app().sequence_length()==120 && original.document.presentation_basis().frame_rate
            == deadpan_core::FrameRate::new(30000,1001).map_err(|error|error.to_string())?,
        json!({"frames":120,"rate":"30000/1001"}),json!({"frames":d.app().sequence_length(),"rate":original.document.presentation_basis().frame_rate}))?;
    open_camera(d)?;
    d.chord(&[Key::N, Key::N, Key::Num8, Key::L, Key::Enter])?;
    continued(d)?;
    d.key(Key::Escape)?;
    idle(d)?;
    at(d, 12)?;
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
    d.check(
        "The manually corrected point changes at indexed Original12 without framing or tracking",
        target.region
            == TargetPoint {
                center: [580000, 500000],
            }
            .into()
            && target.corrections.len() == 1
            && target.corrections[0].at == index.frames()[12].pts
            && target.corrections[0].region
                == TargetPoint {
                    center: [680000, 500000],
                }
                .into()
            && target.span.start().ticks == index.frames()[0].pts
            && target.display_aspect == Some(ratio(16, 9))
            && u64::from(original.document.presentation_basis().width) * 9
                == u64::from(original.document.presentation_basis().height) * 16
            && target.samples.is_empty()
            && d.app().targets.job().is_none()
            && d.app()
                .workspace
                .as_ref()
                .unwrap()
                .document
                .nodes()
                .values()
                .all(|node| node.framing.is_none()),
        json!({"early":[580000,500000],"later":[680000,500000],"correction_ordinal":12}),
        json!(target),
    )?;

    at(d, 24)?;
    d.chord(&[Key::V, Key::Num9, Key::Num6, Key::L])?;
    edit(d, "delete")?;
    at(d, 0)?;
    edit(d, "repeat 3 gap=4f zoom-step=0.125")?;
    let repeat = d.app().selected_beat.clone().ok_or("No Repeat selected")?;
    let repeated = d.app().workspace.clone().ok_or("No repeated workspace")?;
    let (child, iterations, gap, escalation) = match &repeated.document.nodes()[&repeat].kind {
        NodeKind::Repeat {
            child,
            iterations,
            gap: Some(gap),
            escalation: Some(escalation),
        } => (
            child.clone(),
            iterations.clone(),
            gap.clone(),
            escalation.clone(),
        ),
        _ => {
            return Err(
                "Repeat command did not create the requested owner, gaps and escalation".into(),
            );
        }
    };
    let lengths = repeated
        .document
        .durations()
        .map_err(|error| error.to_string())?;
    let layout = RepeatLayout::compile_with_gap_overrides(
        &iterations,
        &child,
        repeated.document.overrides().get(&repeat),
        gap.duration,
        repeated.document.gap_overrides().get(&repeat),
        &lengths,
    )
    .map_err(|error| error.to_string())?;
    let plays = (0..3)
        .map(|play| {
            layout
                .play(&iterations.at(play).unwrap())
                .ok_or("Missing Repeat play")
        })
        .collect::<Result<Vec<_>, _>>()?;
    let (freeze_asset, freeze) = match &gap.video {
        HoldVideo::Freeze { asset, timestamp } => (asset.clone(), *timestamp),
        _ => return Err("Default Repeat gap is not a Freeze".into()),
    };
    d.check("The selected whole Repeat has three 24-frame plays, two silent four-frame gaps and centered escalation",
        repeated.document.children(repeated.document.root()).any(|id|id==&repeat)
            && d.app().sequence_scope.groups().is_empty() && d.app().scoped.is_none()
            && iterations.len()==3 && lengths[&child].frames()==24 && layout.duration().frames()==80
            && plays.iter().enumerate().all(|(play,entry)|entry.start==28*i64::try_from(play).unwrap()
                && entry.duration.frames()==24 && entry.gap_after.frames()==if play<2 {4}else{0}
                && entry.gap_child.is_none())
            && gap.duration.frames()==4 && gap.audio==HoldAudio::Silence && gap.picture_context.is_some()
            && freeze_asset==target.asset && freeze.ticks==index.frames()[23].pts
            && escalation.gain_step.millidecibels()==0
            && escalation.zoom.as_ref().is_some_and(|zoom|zoom.step==ratio(1,8)
                && zoom.progression==deadpan_core::ZoomProgression::Add && zoom.target.is_none())
            && d.app().sequence_length()==80 && repeated.plan.picture(ProjectFrame(80)).is_err(),
        json!({"plays":[[0,24],[28,52],[56,80]],"gaps":[[24,28],[52,56]],"no_trailing_gap":true,"inner_scales":["1","9/8","5/4"],"gap_Original":23,"frames":80}),
        json!({"repeat":repeat,"plays":format!("{plays:?}"),"gap":gap,"escalation":escalation,"frames":d.app().sequence_length()}))?;

    open_camera(d)?;
    d.chord(&[Key::F, Key::Num1, Key::T])?;
    d.settled()?;
    let draft: FramingPose = serde_json::from_value(camera(d)["draft_pose"].clone())
        .map_err(|error| error.to_string())?;
    d.check(
        "Camera previews Follow on the actual whole Repeat owner",
        d.app().selected_beat.as_ref() == Some(&repeat)
            && d.app().scoped.is_none()
            && draft.center_x == expected_center(58, 0)
            && camera(d)["follow"] == "target-1",
        json!({"owner":repeat,"center":".58","follow":"target-1"}),
        camera(d),
    )?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    idle(d)?;
    let fixed = framing(d, &repeat);
    d.check("Fixed Follow is stored on the Repeat while its child remains unframed",
        matches!(&fixed,Some(Framing {value:FramingValue::Follow {target,scale_transition:None,..},..}) if target.as_str()=="target-1")
            && framing(d,&child).is_none(),json!(fixed),json!(framing(d,&repeat)))?;

    for width in [4, 80] {
        at(d, 0)?;
        d.key(Key::V)?;
        let mut keys = Vec::new();
        for digit in width.to_string().chars() {
            keys.push(Key::from_name(&digit.to_string()).ok_or("Invalid range digit")?);
        }
        keys.push(Key::L);
        d.chord(&keys)?;
        refused(
            d,
            "A Visual range, including the complete Repeat extent, refuses Keep Follow creep",
        )?;
        d.key(Key::Escape)?;
    }
    at(d, 0)?;
    edit(d, "wrap-repeat 2")?;
    let enclosing = d
        .app()
        .selected_beat
        .clone()
        .ok_or("No enclosing Repeat selected")?;
    let enclosing_second = match &d
        .app()
        .workspace
        .as_ref()
        .ok_or("No workspace")?
        .document
        .nodes()[&enclosing]
        .kind
    {
        NodeKind::Repeat { iterations, .. } => {
            iterations.at(1).ok_or("No enclosing second play")?
        }
        _ => return Err("The temporary wrapper is not a Repeat".into()),
    };
    d.key(Key::Enter)?;
    idle(d)?;
    for choice in ["scope all", "scope play 2"] {
        d.command(choice)?;
        idle(d)?;
        let scoped = d
            .app()
            .scoped_target()?
            .ok_or("Nested Repeat authority is absent")?;
        d.check("Nested inspection targets the inner fixed-Follow Repeat rather than a Source or the outer owner",
            scoped.root==enclosing && scoped.target.node==repeat && framing(d,&repeat)==fixed
                && scoped.target.repeats.len()==1 && scoped.target.repeats[0].repeat==enclosing
                && if choice=="scope all" {
                    matches!(&scoped.target.repeats[0].branch,deadpan_core::RepeatEditBranch::Default)
                } else {
                    matches!(&scoped.target.repeats[0].branch,deadpan_core::RepeatEditBranch::Play {iteration} if iteration==&enclosing_second)
                },
            json!({"inner_repeat":repeat,"enclosing":enclosing,"scope":choice}),json!({"target":scoped.target,"scope":choice}))?;
        if choice == "scope all" {
            refused(
                d,
                "An inner Repeat under enclosing Default authority refuses Keep creep",
            )?;
        } else {
            super::scoped_follow_creep::prove_concrete_play(d)?;
        }
    }
    d.key(Key::Backspace)?;
    idle(d)?;
    d.check(
        "Leaving the enclosing Repeat returns to ordinary owner selection",
        d.app().scoped.is_none(),
        json!({"scoped":false}),
        json!({"scoped":d.app().scoped.is_some()}),
    )?;
    edit(d, "undo")?;
    d.check(
        "Undoing the temporary outer Repeat restores the fixed original owner",
        d.app().selected_beat.as_ref() == Some(&repeat)
            && framing(d, &repeat) == fixed
            && d.app().sequence_length() == 80,
        json!({"owner":repeat,"frames":80}),
        json!({"owner":d.app().selected_beat,"frames":d.app().sequence_length()}),
    )?;

    at(d, 0)?;
    let baseline = d
        .app()
        .workspace
        .clone()
        .ok_or("No fixed Repeat workspace")?;
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
    let animated = framing(d, &repeat);
    d.key(Key::Q)?;
    d.wait_for("Repeat creep recording saves", |app| {
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
    let current = d
        .app()
        .workspace
        .clone()
        .ok_or("No animated Repeat workspace")?;
    let audio_after = current
        .plan
        .audio(AudioSample(0)..audio_end, Default::default())
        .map_err(|error| error.to_string())?;
    let other_nodes_unchanged = baseline
        .document
        .nodes()
        .iter()
        .filter(|(id, _)| *id != &repeat)
        .all(|(id, node)| current.document.nodes().get(id) == Some(node));
    let mut expected_repeat = baseline.document.nodes()[&repeat].clone();
    expected_repeat.framing = animated.clone();
    d.check("Keep creep changes only Repeat framing and preserves iterations, gaps, escalation, audio and cursors",
        matches!(&animated,Some(Framing {clock:FramingClock::OwnerOutput,value:FramingValue::Follow {
            target,scale,fallback,scale_transition:Some(FollowScaleTransition {factor,curve:ZoomCurve::Linear,range:None})}})
            if target.as_str()=="target-1"&&*scale==ratio(11,10)&&fallback.scale==ratio(11,10)&&*factor==ExactRatio::integer(2))
            && other_nodes_unchanged && current.document.nodes().get(&repeat)==Some(&expected_repeat)
            && baseline.document.nodes().len()==current.document.nodes().len()
            && baseline.document.targets()==current.document.targets()
            && baseline.document.overrides()==current.document.overrides() && baseline.document.gap_overrides()==current.document.gap_overrides()
            && audio_before.spans==audio_after.spans && audio_end==AudioSample(128128)
            && current.plan.audio_duration().is_ok_and(|end|end==audio_end)&&d.app().sequence_length()==80
            && d.app().source_cursor==original_cursor && d.app().sequence_cursor==edit_cursor && program==Some(expected_program.clone()),
        json!({"owner_frames":80,"from":"1.1","to":"2.2","factor":2,"audio_samples":128128,"macro":expected_program}),
        json!({"framing":animated,"other_nodes_unchanged":other_nodes_unchanged,"macro":program,"audio_samples":audio_end}))?;

    let mut exact = true;
    let mut observed = Vec::new();
    for j in 0..80_i64 {
        let play = j / 28;
        let within = j % 28;
        let in_gap = within >= 24;
        let ordinal = if in_gap { 23 } else { within };
        let sample = current
            .plan
            .picture(ProjectFrame(j))
            .map_err(|error| error.to_string())?;
        let before = baseline
            .plan
            .picture(ProjectFrame(j))
            .map_err(|error| error.to_string())?;
        let original_sample = original
            .plan
            .picture(ProjectFrame(ordinal))
            .map_err(|error| error.to_string())?;
        let layer = sample
            .framing
            .iter()
            .find(|layer| !layer.escalation && layer.instance.node == repeat)
            .ok_or("Repeat Follow layer is absent")?;
        let shown = layer.pose.ok_or("Repeat Follow pose is absent")?;
        let escalation_layer = sample
            .framing
            .iter()
            .find(|layer| layer.escalation && layer.instance.node == repeat);
        let escalation_exact = if play == 0 {
            escalation_layer.is_none()
        } else {
            escalation_layer.is_some_and(|layer| {
                layer.pose.is_some_and(|pose| {
                    pose.center_x == ratio(1, 2)
                        && pose.center_y == ratio(1, 2)
                        && pose.scale == ratio(8 + i128::from(play), 8)
                })
            })
        };
        let iteration = iterations
            .at(u32::try_from(play).unwrap())
            .ok_or("Missing stable iteration")?;
        let provider_exact = if in_gap {
            matches!(&sample.picture,deadpan_plan::Picture::Freeze {asset,point}
                if asset==&freeze_asset&&point.ticks==ExactRatio::integer(freeze.ticks)&&point.time_base==freeze.time_base)
                && sample.picture.follow_point().is_some_and(|(asset, point)| {
                    asset == &freeze_asset
                        && point.ticks == ExactRatio::integer(freeze.ticks)
                        && point.time_base == freeze.time_base
                })
                && sample.instance.node == repeat
                && sample.instance.repeats.is_empty()
                && sample.gap_after.as_ref() == Some(&iteration)
                && sample.picture_context.as_deref() == gap.picture_context.as_ref()
        } else {
            matches!(&sample.picture, deadpan_plan::Picture::Source { .. })
                && sample.picture.follow_point() == original_sample.picture.follow_point()
                && sample.gap_after.is_none()
                && sample
                    .instance
                    .repeats
                    .iter()
                    .any(|scope| scope.node == repeat && scope.iteration == iteration)
        };
        let child_layers = sample
            .framing
            .iter()
            .filter(|layer| layer.instance.node != repeat || layer.escalation)
            .collect::<Vec<_>>();
        let prior_child_layers = before
            .framing
            .iter()
            .filter(|layer| layer.instance.node != repeat || layer.escalation)
            .collect::<Vec<_>>();
        exact &= shown.center_x
            == expected_center(if ordinal < 12 { 58 } else { 68 }, i128::from(play))
            && shown.center_y == ratio(1, 2)
            && shown.scale == expected_scale(i128::from(j))
            && layer.local_position == ratio(i128::from(2 * j + 1), 2)
            && layer.effective_duration == ExactRatio::integer(80)
            && layer.duration.frames() == 80
            && escalation_exact
            && provider_exact
            && sample.instance == before.instance
            && sample.local_position == before.local_position
            && sample.picture == before.picture
            && sample.picture_context == before.picture_context
            && child_layers == prior_child_layers
            && sample
                .picture
                .select_source_frame(index)
                .is_ok_and(|found| found == &index.frames()[usize::try_from(ordinal).unwrap()]);
        observed.push(json!({"Edit":j,"play":play,"gap":in_gap,"Original":ordinal,"pose":shown,
            "expected_scale":expected_scale(i128::from(j)),"expected_center":expected_center(if ordinal<12 {58}else{68},i128::from(play)),
            "provider":sample.instance,"gap_after":sample.gap_after,"source_point":format!("{:?}",sample.picture.follow_point()),
            "inner_escalation":escalation_layer}));
    }
    d.check("Every Repeat frame uses the continuous owner scale clock, provider target time and separate inner escalation",
        exact,json!({"frames":80,"plays":[[0,24],[28,52],[56,80]],"gaps":[[24,28],[52,56]],"centers_by_play":[[".58",".68"],[".59",".7025"],[".60",".725"]],"Q32":"half-even"}),json!(observed))?;
    for frame in [
        0, 11, 12, 23, 24, 27, 28, 39, 40, 51, 52, 55, 56, 67, 68, 79,
    ] {
        at(d, frame)?;
        let ordinal = if frame % 28 >= 24 { 23 } else { frame % 28 };
        let expected = current
            .plan
            .picture(ProjectFrame(i64::from(frame)))
            .map_err(|error| error.to_string())?;
        let displayed = d
            .app()
            .presentation
            .picture()
            .ok_or("No stopped Repeat picture")?;
        d.check("Stopped Repeat picture presents committed scopes and exact Original identity at play and gap seams",
            displayed.framing==expected.framing
                && displayed.follow_point.as_ref().map(|(asset,point)|(asset,*point))==expected.picture.follow_point()
                && d.app().presentation.displayed_source_frame().is_some_and(|id|id.0==u64::from(ordinal)),
            json!({"Edit":frame,"Original":ordinal,"framing":expected.framing}),json!({"framing":displayed.framing}))?;
        if [24, 28, 56].contains(&frame) {
            d.capture("Repeat Follow creep at a play or silent gap seam")?;
        }
    }
    at(d, 0)?;
    undo_to(d, &repeat, &fixed)?;
    edit(d, "redo")?;
    d.check(
        "Redo restores the complete repeat Follow creep",
        framing(d, &repeat) == animated,
        json!(animated),
        json!(framing(d, &repeat)),
    )?;
    let revision = d.revision();
    open_camera(d)?;
    d.key(Key::Enter)?;
    idle(d)?;
    d.check(
        "Unchanged Camera Enter preserves repeat animation without history",
        d.app().camera.is_none() && d.revision() == revision && framing(d, &repeat) == animated,
        json!({"revision":revision,"framing":animated}),
        json!({"revision":d.revision(),"framing":framing(d,&repeat)}),
    )?;
    open_camera(d)?;
    d.chord(&[Key::Plus, Key::Escape])?;
    idle(d)?;
    d.check(
        "Camera cancel preserves the complete repeat Follow",
        d.revision() == revision && framing(d, &repeat) == animated,
        json!(animated),
        json!(framing(d, &repeat)),
    )?;
    let installed = current
        .plan
        .picture(ProjectFrame(0))
        .map_err(|error| error.to_string())?
        .framing
        .into_iter()
        .find(|layer| !layer.escalation && layer.instance.node == repeat)
        .and_then(|layer| layer.pose)
        .ok_or("No installed repeat pose")?;
    open_camera(d)?;
    d.key(Key::T)?;
    d.settled()?;
    d.check(
        "Explicit release explains the removal of repeat scale animation",
        d.app().message.as_deref().is_some_and(|message| {
            message.contains("Scale animation removed") && message.contains("static framing")
        }),
        json!("Scale animation removed; Enter saves this static framing"),
        json!(d.app().message),
    )?;
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check("Explicit repeat release commits exactly its displayed static pose",
        matches!(framing(d,&repeat),Some(Framing {value:FramingValue::Static {pose},..}) if pose==installed),json!(installed),json!(framing(d,&repeat)))?;
    undo_to(d, &repeat, &animated)?;

    undo_to(d, &repeat, &fixed)?;
    let revision = d.revision();
    super::macros::execute(d, 'x', None)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Named replay re-resolves Keep for the selected whole repeat",
        framing(d, &repeat) == animated,
        json!(animated),
        json!(framing(d, &repeat)),
    )?;
    undo_to(d, &repeat, &fixed)?;
    edit(d, "creep from=1.1 to=2.2 curve=linear")?;
    undo_to(d, &repeat, &fixed)?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Dot after Undo repeats whole-repeat semantic Keep",
        framing(d, &repeat) == animated,
        json!(animated),
        json!(framing(d, &repeat)),
    )?;

    at(d, 56)?;
    let final_workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No final Repeat workspace")?;
    let final_audio = final_workspace
        .plan
        .audio(AudioSample(0)..audio_end, Default::default())
        .map_err(|error| error.to_string())?;
    d.check("The final export fixture retains the live whole Repeat with exact children, gaps, escalation, targets and audio",
        final_workspace.document.nodes()==current.document.nodes() && final_workspace.document.targets()==current.document.targets()
            && final_workspace.document.overrides()==current.document.overrides()&&final_workspace.document.gap_overrides()==current.document.gap_overrides()
            && final_audio.spans==audio_before.spans&&d.app().sequence_length()==80&&final_workspace.plan.picture(ProjectFrame(80)).is_err()
            && d.app().selected_beat.as_ref()==Some(&repeat)&&d.app().scoped.is_none()&&d.app().sequence_scope.groups().is_empty(),
        json!({"frames":80,"audio_samples":128128,"owner":repeat,"live_follow":"target-1","default_gaps":2}),
        json!({"frames":d.app().sequence_length(),"owner":d.app().selected_beat,"framing":framing(d,&repeat)}))?;
    d.capture("Whole Repeat Follow creep committed across plays and silent gaps")
}
