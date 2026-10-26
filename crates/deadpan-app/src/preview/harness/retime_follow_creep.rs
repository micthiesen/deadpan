//! Whole ordinary Retime Follow creep across a noncontiguous Original cut.
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

fn center(percent: i128) -> ExactRatio {
    let q = i128::from(deadpan_core::FRAMING_NUMERIC_SCALE);
    let n = percent * q;
    let a = n / 100;
    let r = n % 100;
    ratio(
        a + i128::from(r * 2 > 100 || (r * 2 == 100 && a % 2 != 0)),
        q,
    )
}

/// Independent integer, half-even Q32 oracle at frame centers of 120 frames.
fn expected_scale(j: i128) -> ExactRatio {
    let q = i128::from(deadpan_core::FRAMING_NUMERIC_SCALE);
    let round = |n: i128, d: i128| {
        let a = n / d;
        let r = n % d;
        a + i128::from(r * 2 > d || (r * 2 == d && a % 2 != 0))
    };
    let first = round(11 * q, 10);
    let last = round(11 * q, 5);
    let progress = round((2 * j + 1) * q, 240);
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
    d.wait_for("Retime framing receipt finishes", |app| {
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

fn undo_to(d: &mut Driver<'_>, retime: &NodeId, expected: &Option<Framing>) -> Result<(), String> {
    edit(d, "undo")?;
    d.check(
        "One Undo restores the exact Retime Follow recipe",
        framing(d, retime) == *expected,
        json!(expected),
        json!(framing(d, retime)),
    )
}

fn refused(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let revision = d.revision();
    let selection = d.app().edit_range.semantic()?;
    let original_cursor = d.app().source_cursor;
    let edit_cursor = d.app().sequence_cursor;
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
            && d.app().edit_range.semantic()? == selection
            && d.app().source_cursor == original_cursor
            && d.app().sequence_cursor == edit_cursor
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
        "Retime witness starts with 120 Original pictures at 30000/1001",
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

    edit(d, "wrap-retime 2/3 pitch=tape")?;
    let retime = d
        .app()
        .selected_beat
        .clone()
        .ok_or("No actual Retime owner selected")?;
    let wrapped = d.app().workspace.clone().ok_or("No Retime workspace")?;
    d.check("The actual whole Retime maps the 80-frame group into 120 tape-speed output frames",
        wrapped.document.children(wrapped.document.root()).any(|id|id==&retime)
            && d.app().sequence_scope.groups().is_empty()&&d.app().scoped.is_none()
            && matches!(&wrapped.document.nodes()[&retime].kind,NodeKind::Retime {child,duration,mapping,pitch:deadpan_core::PitchPolicy::FollowSpeed,..}
                if child==&group&&duration.frames()==120&&mapping.start()==ProjectFrame(0)&&mapping.end()==ProjectFrame(80))
            && d.app().sequence_length()==120&&wrapped.plan.picture(ProjectFrame(120)).is_err(),
        json!({"child":group,"input":[0,80],"output_frames":120,"speed":"2/3","pitch":"tape"}),
        json!({"owner":retime,"kind":wrapped.document.nodes()[&retime].kind,"frames":d.app().sequence_length()}))?;
    let first_pts = index.frames()[0].pts;
    let tick_step = index.frames()[1].pts - first_pts;
    d.check("The qualified counter index has the exact uniform source clock used by the independent oracle",
        tick_step>0&&index.frames().len()==120
            && index.frames().iter().enumerate().all(|(ordinal,frame)|frame.pts==first_pts+tick_step*i64::try_from(ordinal).unwrap())
            && ratio(i128::from(tick_step)*i128::from(index.time_base().numerator()),i128::from(index.time_base().denominator()))==ratio(1001,30000)
            && target.display_aspect==Some(ratio(16,9))
            && u64::from(original.document.presentation_basis().width)*9==u64::from(original.document.presentation_basis().height)*16,
        json!({"frames":120,"frame_seconds":"1001/30000","display_aspect":"16/9"}),
        json!({"first_pts":first_pts,"tick_step":tick_step,"time_base":index.time_base(),"aspect":target.display_aspect}))?;

    at(d, 0)?;
    open_camera(d)?;
    d.chord(&[Key::F, Key::Num1, Key::T])?;
    d.settled()?;
    let draft: FramingPose = serde_json::from_value(camera(d)["draft_pose"].clone())
        .map_err(|error| error.to_string())?;
    d.check(
        "Camera previews Follow on the selected retime at the early target center",
        d.app().selected_beat.as_ref() == Some(&retime)
            && draft.center_x == center(58)
            && camera(d)["follow"] == "target-1",
        json!({"owner":retime,"center":".58","target":"target-1"}),
        camera(d),
    )?;
    d.capture("Camera follows the actual whole Retime owner")?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    idle(d)?;
    let fixed = framing(d, &retime);
    d.check("Follow is authored on the Retime rather than either Source child",
        matches!(&fixed, Some(Framing {value:FramingValue::Follow {target,scale_transition:None,..},..}) if target.as_str()=="target-1")
            && framing(d,&group).is_none()
            && children.iter().all(|id| framing(d,id).is_none()), json!(fixed), json!(framing(d,&retime)))?;

    for (start, end) in [(0, 120), (10, 18), (18, 10)] {
        at(d, start)?;
        d.key(Key::V)?;
        d.chord(&[if start == 0 { Key::Num1 } else { Key::Num8 }])?;
        if start == 0 {
            d.chord(&[Key::Num2, Key::Num0])?;
        }
        d.key(if end > start { Key::L } else { Key::H })?;
        d.settled()?;
        let selection = d.app().edit_range.semantic()?;
        d.check(
            "The refusal captures the intended full, interior or reverse Retime Visual range",
            matches!(&selection,Some(deadpan_core::SemanticVisualSelection::Time {anchor,head,..})
                if anchor.0==i64::from(start)&&head.0==i64::from(end)),
            json!({"anchor":start,"head":end}),
            json!(selection),
        )?;
        refused(
            d,
            "A full, interior or reverse Visual range refuses whole-Retime Keep creep",
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
        d.check("Nested inspection targets the inner fixed-Follow Retime rather than a Source or the outer owner",
            scoped.root==enclosing && scoped.target.node==retime && framing(d,&retime)==fixed
                && scoped.target.repeats.len()==1 && scoped.target.repeats[0].repeat==enclosing
                && if choice=="scope all" {
                    matches!(&scoped.target.repeats[0].branch,deadpan_core::RepeatEditBranch::Default)
                } else {
                    matches!(&scoped.target.repeats[0].branch,deadpan_core::RepeatEditBranch::Play {iteration} if iteration==&enclosing_second)
                },
            json!({"inner_retime":retime,"enclosing":enclosing,"scope":choice}),json!({"target":scoped.target,"scope":choice}))?;
        if choice == "scope all" {
            refused(
                d,
                "A Retime under enclosing Default authority refuses Keep creep",
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
        d.app().selected_beat.as_ref() == Some(&retime)
            && framing(d, &retime) == fixed
            && d.app().sequence_length() == 120,
        json!({"owner":retime,"frames":120}),
        json!({"owner":d.app().selected_beat,"frames":d.app().sequence_length()}),
    )?;

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
    let animated = framing(d, &retime);
    d.key(Key::Q)?;
    d.wait_for("Retime creep recording saves", |app| {
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
        .filter(|(id, _)| *id != &retime)
        .all(|(id, node)| current.document.nodes().get(id) == Some(node));
    let mut expected_retime = baseline.document.nodes()[&retime].clone();
    expected_retime.framing = animated.clone();
    d.check("One Keep creep changes only the retime framing and records invocation-relative intent",
        matches!(&animated, Some(Framing {clock:FramingClock::OwnerOutput,value:FramingValue::Follow {
            target,scale,fallback,centering:deadpan_core::FollowCentering::Unconstrained,scale_transition:Some(FollowScaleTransition {factor,curve:ZoomCurve::Linear,range:None})}})
            if target.as_str()=="target-1" && *scale==ratio(11,10) && fallback.scale==ratio(11,10) && *factor==ExactRatio::integer(2))
            && other_nodes_unchanged && current.document.nodes().get(&retime)==Some(&expected_retime)
            && baseline.document.nodes().len()==current.document.nodes().len()
            && baseline.document.targets()==current.document.targets() && audio_before.spans==audio_after.spans
            && audio_end==AudioSample(192192)
            && current.plan.audio_duration().is_ok_and(|end| end==audio_end) && d.app().sequence_length()==120
            && d.app().source_cursor==original_cursor && d.app().sequence_cursor==edit_cursor
            && program==Some(expected_program.clone()),
        json!({"owner_frames":120,"from":"1.1","to":"2.2","factor":2,"audio":"unchanged","macro":expected_program}),
        json!({"framing":animated,"other_nodes_unchanged":other_nodes_unchanged,"macro":program,"audio_samples":audio_end}))?;

    let mut observed = Vec::new();
    let mut exact = true;
    let mut fractional_points = 0;
    for j in 0..120_i64 {
        let sample = current
            .plan
            .picture(ProjectFrame(j))
            .map_err(|error| error.to_string())?;
        let before = baseline
            .plan
            .picture(ProjectFrame(j))
            .map_err(|error| error.to_string())?;
        let mapped_numerator = 2 * j + 1 + if j >= 60 { 120 } else { 0 };
        let ordinal = mapped_numerator / 3;
        let expected_point = deadpan_core::SourcePoint {
            ticks: ratio(
                3 * i128::from(first_pts) + i128::from(mapped_numerator) * i128::from(tick_step),
                3,
            ),
            time_base: index.time_base(),
        };
        fractional_points += i64::from(mapped_numerator % 3 != 0);
        let layer = sample
            .framing
            .iter()
            .find(|layer| !layer.escalation && layer.instance.node == retime)
            .ok_or("Retime Follow layer is absent")?;
        let shown = layer.pose.ok_or("Retime Follow pose is absent")?;
        let child_layers = sample
            .framing
            .iter()
            .filter(|layer| layer.instance.node != retime)
            .collect::<Vec<_>>();
        let prior_child_layers = before
            .framing
            .iter()
            .filter(|layer| layer.instance.node != retime)
            .collect::<Vec<_>>();
        exact &= shown.center_x == center(if j < 60 { 58 } else { 68 })
            && shown.center_y == ratio(1, 2)
            && shown.scale == expected_scale(i128::from(j))
            && layer.local_position == ratio(i128::from(2 * j + 1), 2)
            && layer.effective_duration == ExactRatio::integer(120)
            && layer.duration.frames() == 120
            && matches!(&sample.picture, deadpan_plan::Picture::Source { .. })
            && sample.picture.follow_point() == Some((&target.asset, expected_point))
            && (mapped_numerator % 3 == 0
                || expected_point.ticks
                    != ExactRatio::integer(index.frames()[usize::try_from(ordinal).unwrap()].pts))
            && sample
                .picture
                .select_source_frame(index)
                .is_ok_and(|found| found == &index.frames()[usize::try_from(ordinal).unwrap()])
            && sample.instance == before.instance
            && sample.gap_after == before.gap_after
            && sample.local_position == before.local_position
            && sample.picture == before.picture
            && sample.picture_context == before.picture_context
            && sample.captions == before.captions
            && child_layers == prior_child_layers;
        observed.push(json!({"Edit":j,"mapped_group_frame":ratio(i128::from(2*j+1),3),
            "Original_coordinate":ratio(i128::from(mapped_numerator),3),"Original_ordinal":ordinal,
            "expected_source_point":expected_point,"actual_source_point":format!("{:?}",sample.picture.follow_point()),
            "owner_position":layer.local_position,"pose":shown,"expected_scale":expected_scale(i128::from(j)),"provider":sample.instance}));
    }
    d.check("Every Retime frame distinguishes fractional provider time from decoded ordinal and uses the full owner clock",
        exact&&fractional_points==80,
        json!({"frames":120,"fractional_provider_coordinates":80,"owner_clock":120,"mapped_cut_Edit":[59,60],"mapped_cut_Original":["119/3","241/3"],"centers":[".58",".68"],"Q32":"half-even"}),
        json!({"fractional_provider_coordinates":fractional_points,"frames":observed}))?;
    for frame in [0, 1, 2, 58, 59, 60, 61, 118, 119] {
        at(d, frame)?;
        let ordinal = (2 * frame + 1) / 3 + if frame >= 60 { 40 } else { 0 };
        let expected = current
            .plan
            .picture(ProjectFrame(i64::from(frame)))
            .map_err(|error| error.to_string())?;
        let displayed = d
            .app()
            .presentation
            .picture()
            .ok_or("No stopped Retime picture")?;
        d.check("Stopped Retime picture presents the exact mapped point, decoded ordinal and committed framing",
            displayed.framing==expected.framing
                && displayed.follow_point.as_ref().map(|(asset,point)|(asset,*point))==expected.picture.follow_point()
                && d.app().presentation.displayed_source_frame().is_some_and(|id|id.0==u64::from(ordinal)),
            json!({"Edit":frame,"Original_ordinal":ordinal,"framing":expected.framing,"source_point":format!("{:?}",expected.picture.follow_point())}),
            json!({"framing":displayed.framing,"source_point":displayed.follow_point}))?;
        if frame == 59 {
            d.capture("Whole Retime Follow creep before the mapped cut")?;
        }
        if frame == 60 {
            d.capture("Whole Retime Follow creep after the mapped cut")?;
        }
    }
    at(d, 0)?;
    undo_to(d, &retime, &fixed)?;
    edit(d, "redo")?;
    d.check(
        "Redo restores the complete retime Follow creep",
        framing(d, &retime) == animated,
        json!(animated),
        json!(framing(d, &retime)),
    )?;
    let revision = d.revision();
    open_camera(d)?;
    d.key(Key::Enter)?;
    idle(d)?;
    d.check(
        "Unchanged Camera Enter preserves retime animation without history",
        d.app().camera.is_none() && d.revision() == revision && framing(d, &retime) == animated,
        json!({"revision":revision,"framing":animated}),
        json!({"revision":d.revision(),"framing":framing(d,&retime)}),
    )?;
    open_camera(d)?;
    d.chord(&[Key::Plus, Key::Escape])?;
    idle(d)?;
    d.check(
        "Camera cancel preserves the complete retime Follow",
        d.revision() == revision && framing(d, &retime) == animated,
        json!(animated),
        json!(framing(d, &retime)),
    )?;
    let installed = d
        .app()
        .presentation
        .picture()
        .ok_or("No displayed Retime picture before explicit release")?
        .framing
        .iter()
        .find(|layer| !layer.escalation && layer.instance.node == retime)
        .and_then(|layer| layer.pose)
        .ok_or("No installed retime pose")?;
    open_camera(d)?;
    d.key(Key::T)?;
    d.settled()?;
    d.check(
        "Explicit release explains the removal of retime scale animation",
        d.app().message.as_deref().is_some_and(|message| {
            message.contains("Scale animation removed") && message.contains("static framing")
        }),
        json!("Scale animation removed; Enter saves this static framing"),
        json!(d.app().message),
    )?;
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check("Explicit retime release commits exactly its displayed static pose",
        matches!(framing(d,&retime),Some(Framing {value:FramingValue::Static {pose},..}) if pose==installed),json!(installed),json!(framing(d,&retime)))?;
    undo_to(d, &retime, &animated)?;

    undo_to(d, &retime, &fixed)?;
    let revision = d.revision();
    super::macros::execute(d, 'x', None)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Named replay re-resolves Keep for the selected whole retime",
        framing(d, &retime) == animated,
        json!(animated),
        json!(framing(d, &retime)),
    )?;
    undo_to(d, &retime, &fixed)?;
    edit(d, "creep from=1.1 to=2.2 curve=linear")?;
    undo_to(d, &retime, &fixed)?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    idle(d)?;
    d.check(
        "Dot after Undo repeats whole-retime semantic Keep",
        framing(d, &retime) == animated,
        json!(animated),
        json!(framing(d, &retime)),
    )?;

    at(d, 60)?;
    let final_workspace = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No final Retime workspace")?;
    let final_audio = final_workspace
        .plan
        .audio(AudioSample(0)..audio_end, Default::default())
        .map_err(|error| error.to_string())?;
    d.check("The final export fixture retains the live Retime, exact child maps and complete audio after history",
        final_workspace.document.nodes()==current.document.nodes()&&final_workspace.document.targets()==current.document.targets()
            && final_workspace.document.overrides()==current.document.overrides()&&final_workspace.document.gap_overrides()==current.document.gap_overrides()
            && final_audio.spans==audio_before.spans&&d.app().sequence_length()==120&&final_workspace.plan.picture(ProjectFrame(120)).is_err()
            && d.app().selected_beat.as_ref()==Some(&retime)&&d.app().scoped.is_none()&&d.app().sequence_scope.groups().is_empty(),
        json!({"frames":120,"audio_samples":192192,"owner":retime,"follow":"target-1","speed":"2/3","pitch":"tape"}),
        json!({"frames":d.app().sequence_length(),"owner":d.app().selected_beat,"framing":framing(d,&retime)}))?;
    d.capture("Whole Retime Follow creep committed with fractional provider time")
}
