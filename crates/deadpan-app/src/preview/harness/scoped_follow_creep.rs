//! Live Follow creep on one explicitly selected nested Repeat-play owner.
//! Every authored change uses production Camera keys or the project writer.

use super::*;
use crate::preview::copied::Content;
use deadpan_core::{
    AudioSample, ExactRatio, FollowScaleTransition, Framing, FramingClock, FramingValue, NodeId,
    NodeKind, ProjectFrame, SemanticInstruction, SemanticZoomCenter, TargetId, TargetPoint,
    framing_zoom::{ZoomCurve, ZoomKind},
};
use egui::Key;

mod ranges;

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

/// Independent integer, half-even Q32 oracle at frame centers of a complete selected owner.
fn expected_scale(j: i128, frames: i128) -> ExactRatio {
    let q = i128::from(deadpan_core::FRAMING_NUMERIC_SCALE);
    let round = |n: i128, d: i128| {
        let a = n / d;
        let r = n % d;
        a + i128::from(r * 2 > d || (r * 2 == d && a % 2 != 0))
    };
    let first = round(11 * q, 10);
    let last = round(11 * q, 5);
    let progress = round((2 * j + 1) * q, 2 * frames);
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

fn equivalent(
    actual: &deadpan_core::ProjectDocument,
    expected: &deadpan_core::ProjectDocument,
) -> bool {
    let mut value = serde_json::to_value(actual).unwrap();
    value["revision_id"] = json!(expected.revision_id());
    value == serde_json::to_value(expected).unwrap()
}

fn leave(d: &mut Driver<'_>) -> Result<(), String> {
    for _ in 0..4 {
        if d.app().scoped.is_none() {
            return Ok(());
        }
        d.key(Key::Backspace)?;
        idle(d)?;
    }
    Err("Nested inspector did not return to the ordinary parent".into())
}

fn choose(
    d: &mut Driver<'_>,
    outer: u32,
    inner: u32,
) -> Result<crate::project::scoped::Target, String> {
    leave(d)?;
    d.key(Key::Enter)?;
    d.command(&format!("scope play {outer}"))?;
    idle(d)?;
    d.key(Key::Enter)?;
    d.command(&format!("scope play {inner}"))?;
    idle(d)?;
    d.key(Key::Home)?;
    idle(d)?;
    d.app()
        .scoped_target()?
        .ok_or("No explicit nested scoped owner".into())
}

fn local(d: &mut Driver<'_>, frame: u32) -> Result<(), String> {
    d.key(Key::Home)?;
    if frame > 0 {
        let mut keys = Vec::new();
        for digit in frame.to_string().chars() {
            keys.push(Key::from_name(&digit.to_string()).ok_or("Invalid local digit")?);
        }
        keys.push(Key::L);
        d.chord(&keys)?;
    }
    d.settled()
}

fn remember(
    aliases: &mut std::collections::BTreeMap<NodeId, NodeId>,
    before: &crate::project::Workspace,
    new: &NodeId,
    old: &NodeId,
) -> Result<(), String> {
    if new == old {
        return Ok(());
    }
    if before.document.nodes().contains_key(new) {
        return Err("Isolation reused an existing physical node as a different owner".into());
    }
    if aliases
        .insert(new.clone(), old.clone())
        .is_some_and(|previous| previous != *old)
    {
        return Err("A physical clone maps to two original nodes".into());
    }
    Ok(())
}

fn aliases(
    before: &crate::project::Workspace,
    after: &crate::project::Workspace,
) -> Result<std::collections::BTreeMap<NodeId, NodeId>, String> {
    let mut result = std::collections::BTreeMap::new();
    for frame in 0..before.plan.duration().frames() {
        let old = before
            .plan
            .picture(ProjectFrame(frame))
            .map_err(|error| error.to_string())?;
        let new = after
            .plan
            .picture(ProjectFrame(frame))
            .map_err(|error| error.to_string())?;
        if old.framing.len() != new.framing.len() {
            return Err("Scoped framing changed the picture layer count".into());
        }
        for (new, old) in new
            .framing
            .iter()
            .map(|layer| &layer.instance)
            .chain(std::iter::once(&new.instance))
            .zip(
                old.framing
                    .iter()
                    .map(|layer| &layer.instance)
                    .chain(std::iter::once(&old.instance)),
            )
        {
            remember(&mut result, before, &new.node, &old.node)?;
            if new.repeats.len() != old.repeats.len() {
                return Err("Scoped isolation changed occurrence depth".into());
            }
            for (new, old) in new.repeats.iter().zip(&old.repeats) {
                if new.iteration != old.iteration {
                    return Err("Scoped isolation changed a stable iteration identity".into());
                }
                remember(&mut result, before, &new.node, &old.node)?;
            }
        }
    }
    Ok(result)
}

fn normalized(value: Value, aliases: &std::collections::BTreeMap<NodeId, NodeId>) -> Value {
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
                .map(|item| normalized(item, aliases))
                .collect(),
        ),
        Value::Object(items) => Value::Object(
            items
                .into_iter()
                .map(|(key, value)| (replace(key), normalized(value, aliases)))
                .collect(),
        ),
        other => other,
    }
}

fn view(sample: &deadpan_plan::PictureSample) -> Value {
    json!({"instance":sample.instance,"gap_after":sample.gap_after,"local":sample.local_position,
        "picture":sample.picture,"context":sample.picture_context.as_deref(),"framing":sample.framing,"captions":sample.captions})
}

fn scale_at(position: ExactRatio, duration: ExactRatio) -> ExactRatio {
    let q = i128::from(deadpan_core::FRAMING_NUMERIC_SCALE);
    let round = |n: i128, d: i128| {
        let a = n / d;
        let r = n % d;
        a + i128::from(2 * r > d || (2 * r == d && a % 2 != 0))
    };
    let first = round(11 * q, 10);
    let last = round(11 * q, 5);
    let progress = round(
        position.numerator() * duration.denominator() * q,
        position.denominator() * duration.numerator(),
    );
    ratio(first + round((last - first) * progress, q), q)
}

/// Compare semantic views and full audio after separately validating physical clone aliases.
fn effect(
    d: &mut Driver<'_>,
    before: &crate::project::Workspace,
    captured: &crate::project::scoped::Target,
    label: &str,
) -> Result<NodeId, String> {
    let after = d
        .app()
        .workspace
        .clone()
        .ok_or("No scoped result workspace")?;
    let selected = d
        .app()
        .scoped_target()?
        .ok_or("Scoped receipt lost its concrete owner")?;
    let aliases = aliases(before, &after)?;
    let animated = framing(d, &selected.target.node);
    let stable_address = captured.target.repeats.len() == selected.target.repeats.len()
        && captured.root == selected.root
        && captured
            .target
            .repeats
            .iter()
            .zip(&selected.target.repeats)
            .all(|(old, new)| {
                aliases.get(&new.repeat).unwrap_or(&new.repeat) == &old.repeat
                    && old.branch == new.branch
            });
    let concrete = selected.presentation.as_ref().is_some_and(|instance| {
        selected
            .target
            .matches_instance(&after.document, instance)
            .unwrap_or(false)
    });
    let override_paths_exact = before.document.overrides().is_empty()
        && after.document.overrides().len() == selected.target.repeats.len()
        && selected
            .target
            .repeats
            .iter()
            .enumerate()
            .all(|(index, step)| {
                let deadpan_core::RepeatEditBranch::Play { iteration } = &step.branch else {
                    return false;
                };
                let next = selected
                    .target
                    .repeats
                    .get(index + 1)
                    .map_or(&selected.target.node, |next| &next.repeat);
                after
                    .document
                    .overrides()
                    .get(&step.repeat)
                    .is_some_and(|entries| {
                        entries.len() == 1 && entries.get(iteration) == Some(next)
                    })
            });
    let originals_unchanged = before
        .document
        .nodes()
        .iter()
        .all(|(id, node)| after.document.nodes().get(id) == Some(node));
    let mut clones_exact = true;
    for (id, node) in after
        .document
        .nodes()
        .iter()
        .filter(|(id, _)| !before.document.nodes().contains_key(*id))
    {
        let Some(original) = aliases.get(id) else {
            clones_exact = false;
            continue;
        };
        let mut expected = before.document.nodes()[original].clone();
        if id == &selected.target.node {
            expected.framing = animated.clone();
        }
        clones_exact &= normalized(json!(node), &aliases) == json!(expected);
    }
    let mut frames_exact = true;
    let mut altered_frames = 0;
    for frame in 0..before.plan.duration().frames() {
        let mut old = before
            .plan
            .picture(ProjectFrame(frame))
            .map_err(|error| error.to_string())?;
        let new = after
            .plan
            .picture(ProjectFrame(frame))
            .map_err(|error| error.to_string())?;
        for layer in &mut old.framing {
            if !layer.escalation && captured.presentation.as_ref() == Some(&layer.instance) {
                let pose = layer
                    .pose
                    .as_mut()
                    .ok_or("Selected fixed Follow has no resolved pose")?;
                pose.scale = scale_at(layer.local_position, layer.effective_duration);
                altered_frames += 1;
            }
        }
        frames_exact &= new.picture == old.picture
            && new.picture_context == old.picture_context
            && new.captions == old.captions
            && normalized(view(&new), &aliases) == view(&old);
    }
    let end = before
        .plan
        .audio_duration()
        .map_err(|error| error.to_string())?;
    let old_audio = before
        .plan
        .audio(AudioSample(0)..end, Default::default())
        .map_err(|error| error.to_string())?;
    let new_audio = after
        .plan
        .audio(AudioSample(0)..end, Default::default())
        .map_err(|error| error.to_string())?;
    let audio_exact = old_audio
        .spans
        .iter()
        .zip(&new_audio.spans)
        .all(|(old, new)| old.content == new.content)
        && normalized(json!(new_audio.spans), &aliases) == json!(old_audio.spans);
    d.check(label,
        matches!(&animated,Some(Framing {clock:FramingClock::OwnerOutput,value:FramingValue::Follow {
            target,scale,fallback,scale_transition:Some(FollowScaleTransition {factor,curve:ZoomCurve::Linear,range:None})}})
            if target.as_str()=="target-1"&&*scale==ratio(11,10)&&fallback.scale==ratio(11,10)&&*factor==ExactRatio::integer(2))
            && stable_address&&concrete&&override_paths_exact&&originals_unchanged&&clones_exact&&frames_exact&&audio_exact
            && selected.target.node!=captured.target.node
            && aliases.get(&selected.target.node)==Some(&captured.target.node)
            && before.document.targets()==after.document.targets()
            && before.document.gap_overrides()==after.document.gap_overrides()
            && after.plan.duration()==before.plan.duration()
            && after.plan.audio_duration().is_ok_and(|value|value==end)
            && altered_frames>0,
        json!({"only_selected_owner_scale_changes":true,"stable_iterations":true,"audio":"unchanged after explicit clone aliases"}),
        json!({"captured":captured.target,"selected":selected.target,"aliases":aliases,"altered_frames":altered_frames,
            "override_paths_exact":override_paths_exact,"originals_unchanged":originals_unchanged,"clones_exact":clones_exact,"frames_exact":frames_exact,"audio_exact":audio_exact,"framing":animated}))?;
    Ok(selected.target.node)
}

/// Reconcile former concrete-Play refusals without removing their preservation witness.
pub(super) fn prove_concrete_play(d: &mut Driver<'_>) -> Result<(), String> {
    let before = d
        .app()
        .workspace
        .clone()
        .ok_or("No concrete-play baseline")?;
    let captured = d.app().scoped_target()?.ok_or("No concrete-play capture")?;
    let source_cursor = d.app().source_cursor;
    let edit_cursor = d.app().sequence_cursor;
    edit(d, "creep from=1.1 to=2.2 curve=linear")?;
    effect(
        d,
        &before,
        &captured,
        "Concrete Play Keep creep isolates only its whole owner and preserves every other view and audio",
    )?;
    d.check(
        "Concrete Play creep preserves both independent cursors",
        d.app().source_cursor == source_cursor && d.app().sequence_cursor == edit_cursor,
        json!({"Original":source_cursor,"Edit":edit_cursor}),
        json!({"Original":d.app().source_cursor,"Edit":d.app().sequence_cursor}),
    )?;
    edit(d, "undo")?;
    d.check("One Undo removes concrete-Play framing isolation and restores the complete former-refusal fixture",
        d.app().workspace.as_ref().is_some_and(|workspace|equivalent(&workspace.document,&before.document))&&d.app().scoped.is_none(),
        json!("complete document restored"),d.snapshot())
}

fn nested_oracle(
    d: &mut Driver<'_>,
    baseline: &crate::project::Workspace,
    retime: &NodeId,
    start: i64,
) -> Result<(), String> {
    let current = d
        .app()
        .workspace
        .clone()
        .ok_or("No nested scoped workspace")?;
    let aliases = aliases(baseline, &current)?;
    let outer = baseline
        .document
        .children(baseline.document.root())
        .next()
        .ok_or("No outer Repeat")?
        .clone();
    let (inner, outer_iterations) = match &baseline.document.nodes()[&outer].kind {
        NodeKind::Repeat {
            child, iterations, ..
        } => (child.clone(), iterations.clone()),
        _ => return Err("No outer Repeat recipe".into()),
    };
    let inner_iterations = match &baseline.document.nodes()[&inner].kind {
        NodeKind::Repeat { iterations, .. } => iterations.clone(),
        _ => return Err("No inner Repeat recipe".into()),
    };
    let target = baseline
        .document
        .targets()
        .get(&TargetId::new("target-1").unwrap())
        .ok_or("No point target")?;
    let index = baseline
        .sources
        .get(&target.asset)
        .and_then(|source| source.video_index.as_ref())
        .ok_or("No Original index")?;
    let first = index.frames()[0].pts;
    let step = index.frames()[1].pts - first;
    let fixed_scale = scale_at(ExactRatio::ZERO, ExactRatio::ONE);
    let mut exact = true;
    let mut fractional = 0;
    let mut rows = Vec::new();
    for frame in 0..240_i64 {
        let outer_play = frame / 124;
        let outer_local = frame % 124;
        let outer_gap = outer_local >= 116;
        let inner_play = outer_local / 40;
        let inner_local = outer_local % 40;
        let gap = outer_gap || inner_local >= 36;
        let sample = current
            .plan
            .picture(ProjectFrame(frame))
            .map_err(|error| error.to_string())?;
        let outer_iteration = outer_iterations
            .at(u32::try_from(outer_play).unwrap())
            .ok_or("No outer iteration")?;
        let ordinal = if gap { 23 } else { (2 * inner_local + 1) / 3 };
        let expected_point = deadpan_core::SourcePoint {
            ticks: if gap {
                ExactRatio::integer(index.frames()[23].pts)
            } else {
                ratio(
                    3 * i128::from(first) + i128::from(2 * inner_local + 1) * i128::from(step),
                    3,
                )
            },
            time_base: index.time_base(),
        };
        let owner = sample.framing.iter().find(|layer| {
            !layer.escalation
                && aliases
                    .get(&layer.instance.node)
                    .unwrap_or(&layer.instance.node)
                    == retime
        });
        if gap {
            exact &= owner.is_none()
                && matches!(&sample.picture, deadpan_plan::Picture::Freeze { .. })
                && sample.picture_context.is_some();
            if outer_gap {
                exact &= sample.instance.node == outer
                    && sample.instance.repeats.is_empty()
                    && sample.gap_after == Some(outer_iteration);
            } else {
                let inner_iteration = inner_iterations
                    .at(u32::try_from(inner_play).unwrap())
                    .ok_or("No inner gap iteration")?;
                exact &= aliases
                    .get(&sample.instance.node)
                    .unwrap_or(&sample.instance.node)
                    == &inner
                    && sample.instance.repeats.len() == 1
                    && sample.instance.repeats[0].node == outer
                    && sample.instance.repeats[0].iteration == outer_iteration
                    && sample.gap_after == Some(inner_iteration);
            }
        } else {
            let layer = owner.ok_or("A live nested play lost its Retime Follow owner")?;
            let shown = layer.pose.ok_or("A live nested Follow has no pose")?;
            let selected = (start..start + 36).contains(&frame);
            let expected_scale = if selected {
                expected_scale(i128::from(frame - start), 36)
            } else {
                fixed_scale
            };
            let inner_iteration = inner_iterations
                .at(u32::try_from(inner_play).unwrap())
                .ok_or("No inner play iteration")?;
            exact &= matches!(&sample.picture, deadpan_plan::Picture::Source { .. })
                && shown.center_x == center(if ordinal < 12 { 58 } else { 68 })
                && shown.center_y == ratio(1, 2)
                && shown.scale == expected_scale
                && layer.local_position == ratio(i128::from(2 * inner_local + 1), 2)
                && layer.effective_duration == ExactRatio::integer(36)
                && layer.duration.frames() == 36
                && sample.gap_after.is_none()
                && sample.instance.repeats.len() == 2
                && sample.instance.repeats[0].node == outer
                && sample.instance.repeats[0].iteration == outer_iteration
                && aliases
                    .get(&sample.instance.repeats[1].node)
                    .unwrap_or(&sample.instance.repeats[1].node)
                    == &inner
                && sample.instance.repeats[1].iteration == inner_iteration;
            fractional += i64::from((2 * inner_local + 1) % 3 != 0);
        }
        exact &= sample.picture.follow_point() == Some((&target.asset, expected_point))
            && sample
                .picture
                .select_source_frame(index)
                .is_ok_and(|selected| {
                    selected == &index.frames()[usize::try_from(ordinal).unwrap()]
                });
        rows.push(json!({"Edit":frame,"gap":gap,"Original_ordinal":ordinal,"expected_point":expected_point,
            "provider":sample.instance,"gap_after":sample.gap_after,"owner_pose":owner.and_then(|layer|layer.pose)}));
    }
    let selected = d.app().scoped_target()?.ok_or("No final scoped address")?;
    let branches = &selected.target.repeats;
    let override_edges = branches.len() == 2
        && current
            .document
            .overrides()
            .get(&outer)
            .and_then(|entries| entries.get(&outer_iterations.at(1).unwrap()))
            == Some(&branches[1].repeat)
        && current
            .document
            .overrides()
            .get(&branches[1].repeat)
            .and_then(|entries| entries.get(&inner_iterations.at(1).unwrap()))
            == Some(&selected.target.node);
    d.check("All 240 frames retain exact providers and captured gaps while only the selected 36-frame owner creeps",
        exact&&fractional==144&&override_edges
            && current.plan.duration().frames()==240&&current.plan.picture(ProjectFrame(240)).is_err()
            && current.plan.audio_duration().is_ok_and(|end|end==AudioSample(384384)),
        json!({"selected":[start,start+36],"provider":"(2j+1)/3","correction_local":18,"frames":240,
            "fractional_play_coordinates":144,"audio_samples":384384,"override_edges":"outer Play2 -> cloned inner, inner Play2 -> cloned Retime"}),
        json!({"exact":exact,"fractional":fractional,"override_edges":override_edges,"selected":selected.target,"frames":rows}))
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
        "Scoped witness starts with 120 Original pictures at 30000/1001",
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
    d.check("The saved point changes at indexed Original12 without authoring framing",
        target.region == TargetPoint { center:[580000,500000] }.into()
            && target.corrections.len() == 1 && target.corrections[0].at == index.frames()[12].pts
            && target.corrections[0].region == TargetPoint {center:[680000,500000]}.into()
            && target.span.start().ticks == index.frames()[0].pts && target.samples.is_empty()
            && d.app().targets.job().is_none()
            && d.app().workspace.as_ref().unwrap().document.nodes().values().all(|node| node.framing.is_none()),
        json!({"early":[580000,500000],"later":[680000,500000],"correction_ordinal":12,"framing":null}), json!(target))?;

    at(d, 24)?;
    d.chord(&[Key::V, Key::Num9, Key::Num6, Key::L])?;
    edit(d, "delete")?;
    at(d, 0)?;
    edit(d, "wrap-retime 2/3 pitch=tape")?;
    let retime = d
        .app()
        .selected_beat
        .clone()
        .ok_or("No Retime template selected")?;
    edit(d, "zoom 1.1 target=target-1")?;
    let fixed = framing(d, &retime);
    edit(d, "repeat 3 gap=4f")?;
    let inner = d
        .app()
        .selected_beat
        .clone()
        .ok_or("No inner Repeat selected")?;
    edit(d, "wrap-repeat 2")?;
    let outer = d
        .app()
        .selected_beat
        .clone()
        .ok_or("No outer Repeat selected")?;
    edit(d, "repeat gap=8f")?;
    let baseline = d.app().workspace.clone().ok_or("No nested baseline")?;
    let (outer_iterations, outer_gap) = match &baseline.document.nodes()[&outer].kind {
        NodeKind::Repeat {
            child,
            iterations,
            gap: Some(gap),
            ..
        } if child == &inner => (iterations.clone(), gap.clone()),
        _ => {
            return Err(
                "The outer wrapper did not retain the inner Repeat and configured gap".into(),
            );
        }
    };
    let (inner_iterations, inner_gap) = match &baseline.document.nodes()[&inner].kind {
        NodeKind::Repeat {
            child,
            iterations,
            gap: Some(gap),
            ..
        } if child == &retime => (iterations.clone(), gap.clone()),
        _ => return Err("The inner Repeat did not retain the fixed-Follow Retime".into()),
    };
    d.check("Production commands create the nested 36-frame Retime, 116-frame inner Repeat and 240-frame outer Repeat",
        outer_iterations.len()==2&&inner_iterations.len()==3&&outer_gap.duration.frames()==8&&inner_gap.duration.frames()==4
            && outer_gap.audio==deadpan_core::HoldAudio::Silence&&inner_gap.audio==deadpan_core::HoldAudio::Silence
            && outer_gap.picture_context.is_some()&&inner_gap.picture_context.is_some()
            && matches!(&baseline.document.nodes()[&retime].kind,NodeKind::Retime {duration,mapping,pitch:deadpan_core::PitchPolicy::FollowSpeed,..}
                if duration.frames()==36&&mapping.start()==ProjectFrame(0)&&mapping.end()==ProjectFrame(24))
            && baseline.document.node_duration(&inner).is_ok_and(|duration|duration.frames()==116)
            && baseline.plan.duration().frames()==240&&baseline.plan.audio_duration().is_ok_and(|end|end==AudioSample(384384))
            && matches!(&fixed,Some(Framing {value:FramingValue::Follow {target,scale,scale_transition:None,..},..})
                if target.as_str()=="target-1"&&*scale==ratio(11,10)),
        json!({"Retime":36,"inner":116,"outer":240,"audio_samples":384384,"gaps":[4,8],"fixed_scale":"1.1"}),
        json!({"retime":retime,"inner":inner,"outer":outer,"fixed":fixed,"outer_gap":outer_gap,"inner_gap":inner_gap}))?;
    let index = baseline
        .sources
        .get(&target.asset)
        .and_then(|source| source.video_index.as_ref())
        .ok_or("No qualified source index")?;
    let step = index.frames()[1].pts - index.frames()[0].pts;
    d.check("The source-time oracle uses a verified uniform 30000/1001 Original index and 16:9 geometry",
        step>0&&index.frames().len()==120&&index.frames().iter().enumerate().all(|(j,frame)|frame.pts==index.frames()[0].pts+step*i64::try_from(j).unwrap())
            && ratio(i128::from(step)*i128::from(index.time_base().numerator()),i128::from(index.time_base().denominator()))==ratio(1001,30000)
            && target.display_aspect==Some(ratio(16,9))
            && u64::from(baseline.document.presentation_basis().width)*9==u64::from(baseline.document.presentation_basis().height)*16,
        json!({"frame_seconds":"1001/30000","aspect":"16/9"}),json!({"step":step,"time_base":index.time_base(),"aspect":target.display_aspect}))?;

    // The native inspector can express missing choices and both mixed paths.
    for (outer_choice, inner_choice) in [
        ("", ""),
        ("scope all", "scope all"),
        ("scope play 2", "scope all"),
        ("scope all", "scope play 2"),
    ] {
        leave(d)?;
        d.key(Key::Enter)?;
        if !outer_choice.is_empty() {
            d.command(outer_choice)?;
        }
        d.key(Key::Enter)?;
        if !inner_choice.is_empty() {
            d.command(inner_choice)?;
        }
        idle(d)?;
        let scoped = d
            .app()
            .scoped_target()?
            .ok_or("No Default or mixed scope")?;
        d.check(
            "The refused Default or mixed path still names the real fixed-Follow Retime",
            scoped.target.node == retime && scoped.target.repeats.len() == 2,
            json!({"target":retime,"choices":[outer_choice,inner_choice]}),
            json!(scoped.target),
        )?;
        refused(
            d,
            "Default, no-choice and mixed Repeat paths cannot authorize scoped Keep Follow creep",
        )?;
    }
    let captured = choose(d, 2, 2)?;
    let before_visual = baseline.document.clone();
    d.key(Key::V)?;
    idle(d)?;
    d.check("The explicit nested Play opens an empty Time selection without changing authored state or authority",
        matches!(d.app().edit_range.semantic()?,Some(deadpan_core::SemanticVisualSelection::Time {anchor,head,extending:true}) if anchor==head)
            && d.app().workspace.as_ref().is_some_and(|workspace|workspace.document==before_visual)
            && d.app().scoped_target()?.is_some_and(|target|target.target==captured.target),
        json!({"Visual":"empty Time","document":"unchanged","target":captured.target}),d.snapshot())?;
    d.key(Key::Escape)?;
    idle(d)?;
    d.check(
        "Escape clears the scoped Time selection without leaving the selected Play or editing",
        !d.app().edit_range.has_bounds()
            && d.app()
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.document == before_visual)
            && d.app()
                .scoped_target()?
                .is_some_and(|target| target.target == captured.target),
        json!({"Visual":"cleared","document":"unchanged","target":captured.target}),
        d.snapshot(),
    )?;

    // A genuine writer Undo invalidates an already captured command.
    let stale_capture = choose(d, 2, 2)?;
    let old_revision = d.revision();
    d.key(Key::Colon)?;
    d.events(
        "Stage scoped Keep creep before a genuine history change",
        vec![egui::Event::Text(
            "creep from=1.1 to=2.2 curve=linear".into(),
        )],
    )?;
    d.check(
        "The stale-capture witness stages the real scoped command before history changes",
        d.app().command_open && d.app().command == "creep from=1.1 to=2.2 curve=linear",
        json!({"command":"creep from=1.1 to=2.2 curve=linear","scope":stale_capture.target}),
        json!({"open":d.app().command_open,"command":d.app().command}),
    )?;
    d.app().service.submit(ProjectRequest::Undo {
        expected_revision: baseline.document.revision_id().clone(),
    })?;
    d.changed(&old_revision)?;
    idle(d)?;
    let undone = d
        .app()
        .workspace
        .clone()
        .ok_or("No stale-command Undo result")?;
    let undone_revision = d.revision();
    d.check("The captured field remains pending after the genuine writer Undo",
        d.app().command_open&&d.app().command=="creep from=1.1 to=2.2 curve=linear",
        json!({"captured_revision":old_revision,"current_revision":undone_revision,"scope":stale_capture.target}),
        json!({"open":d.app().command_open,"command":d.app().command,"revision":d.revision()}))?;
    d.key(Key::Enter)?;
    idle(d)?;
    d.check("A scoped command captured before genuine Undo refuses the stale revision without another write",
        d.revision()==undone_revision&&!d.app().command_open
            &&d.app().error.as_deref().is_some_and(|error|error.contains("changed")||error.contains("revision")||error.contains("stale"))
            && d.app().workspace.as_ref().is_some_and(|workspace|workspace.document==undone.document),
        json!({"revision":undone_revision,"write":false,"captured_scope":stale_capture.target,"captured_presentation":stale_capture.presentation}),
        json!({"revision":d.revision(),"error":d.app().error,"current_scope":d.app().scoped_target()?.map(|target|target.target)}))?;
    edit(d, "redo")?;
    d.check(
        "Redo restores the exact nested baseline after the stale-capture witness",
        d.app()
            .workspace
            .as_ref()
            .is_some_and(|workspace| equivalent(&workspace.document, &baseline.document)),
        json!("complete baseline restored"),
        d.snapshot(),
    )?;

    let captured = choose(d, 2, 2)?;
    d.check("Two explicit stable Play choices capture only the complete Retime at global164",
        captured.root==outer&&captured.target.node==retime&&captured.target.repeats.len()==2
            && captured.target.repeats[0].repeat==outer&&captured.target.repeats[1].repeat==inner
            && matches!(&captured.target.repeats[0].branch,deadpan_core::RepeatEditBranch::Play {iteration} if iteration==&outer_iterations.at(1).unwrap())
            && matches!(&captured.target.repeats[1].branch,deadpan_core::RepeatEditBranch::Play {iteration} if iteration==&inner_iterations.at(1).unwrap())
            && d.app().sequence_cursor==164,
        json!({"outer_play":2,"inner_play":2,"owner":retime,"Edit":164}),json!(captured.target))?;
    let source_cursor = d.app().source_cursor;
    d.chord(&[Key::Q, Key::X])?;
    edit(d, "creep from=1.1 to=2.2 curve=linear")?;
    d.key(Key::Q)?;
    d.wait_for("Nested scoped Follow creep recording saves", |app| {
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
    d.check(
        "Scoped recording saves invocation-relative Keep without concrete play addresses",
        program == Some(expected_program.clone())
            && d.app().source_cursor == source_cursor
            && d.app().sequence_cursor == 164,
        json!(expected_program),
        json!({"program":program,"Original":d.app().source_cursor,"Edit":d.app().sequence_cursor}),
    )?;
    effect(
        d,
        &baseline,
        &captured,
        "Nested explicit Play Keep creep isolates only the selected Retime and preserves all other views and audio",
    )?;
    nested_oracle(d, &baseline, &retime, 164)?;
    let animated = d
        .app()
        .workspace
        .clone()
        .ok_or("No animated nested snapshot")?;
    edit(d, "undo")?;
    d.check(
        "One Undo removes both isolation steps and restores the entire fixed nested document",
        d.app()
            .workspace
            .as_ref()
            .is_some_and(|workspace| equivalent(&workspace.document, &baseline.document))
            && d.app().scoped.is_none(),
        json!("complete baseline restored"),
        d.snapshot(),
    )?;
    edit(d, "redo")?;
    d.check(
        "Redo restores the exact cloned branch identities and animated recipe",
        d.app()
            .workspace
            .as_ref()
            .is_some_and(|workspace| equivalent(&workspace.document, &animated.document)),
        json!("exact animated document restored"),
        d.snapshot(),
    )?;
    choose(d, 2, 2)?;
    local(d, 0)?;
    let selected = d
        .app()
        .scoped_target()?
        .ok_or("No restored animated owner")?;
    let animated_framing = framing(d, &selected.target.node);
    let revision = d.revision();
    open_camera(d)?;
    d.check(
        "Camera recognizes the live target at the current explicit nested scoped owner",
        camera(d)["follow"] == "target-1" && camera(d)["phase"] == "Adjust",
        json!({"follow":"target-1","owner":selected.target}),
        camera(d),
    )?;
    d.capture("Camera retains the explicit nested Play Follow creep")?;
    d.key(Key::Enter)?;
    idle(d)?;
    d.check(
        "No-change scoped Camera Enter preserves the complete animated branch without history",
        d.revision() == revision
            && d.app().camera.is_none()
            && framing(d, &selected.target.node) == animated_framing,
        json!(animated_framing),
        json!(framing(d, &selected.target.node)),
    )?;
    open_camera(d)?;
    d.chord(&[Key::Plus, Key::Escape])?;
    idle(d)?;
    d.check(
        "Adjusted scoped Camera Escape restores the exact animated Follow",
        d.revision() == revision && framing(d, &selected.target.node) == animated_framing,
        json!(animated_framing),
        json!(framing(d, &selected.target.node)),
    )?;
    let installed = d
        .app()
        .presentation
        .picture()
        .and_then(|picture| {
            picture
                .framing
                .iter()
                .find(|layer| !layer.escalation && layer.instance.node == selected.target.node)
        })
        .and_then(|layer| layer.pose)
        .ok_or("No actual displayed scoped pose")?;
    open_camera(d)?;
    d.key(Key::T)?;
    d.settled()?;
    d.check(
        "Explicit scoped release explains that Enter saves static framing and removes animation",
        d.app().message.as_deref().is_some_and(|message| {
            message.contains("Scale animation removed") && message.contains("static framing")
        }),
        json!("Scale animation removed; Enter saves this static framing"),
        json!(d.app().message),
    )?;
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    idle(d)?;
    let released = d
        .app()
        .scoped_target()?
        .ok_or("Static release lost its concrete owner")?;
    d.check("Explicit scoped release saves exactly the installed pose on that branch",
        matches!(framing(d,&released.target.node),Some(Framing {value:FramingValue::Static {pose},..}) if pose==installed),
        json!(installed),json!(framing(d,&released.target.node)))?;
    edit(d, "undo")?;
    d.check(
        "Undo restores the exact animated branch after explicit Camera release",
        d.app()
            .workspace
            .as_ref()
            .is_some_and(|workspace| equivalent(&workspace.document, &animated.document)),
        json!("animated document restored"),
        d.snapshot(),
    )?;
    edit(d, "undo")?;
    d.check(
        "Undo returns to the fixed baseline for fresh invocation-time macro authority",
        d.app()
            .workspace
            .as_ref()
            .is_some_and(|workspace| equivalent(&workspace.document, &baseline.document)),
        json!("baseline restored"),
        d.snapshot(),
    )?;

    let fresh = choose(d, 1, 1)?;
    let revision = d.revision();
    super::macros::execute(d, 'x', None)?;
    d.changed(&revision)?;
    idle(d)?;
    effect(
        d,
        &baseline,
        &fresh,
        "Named Keep replay resolves a fresh concrete outer1 inner1 branch",
    )?;
    d.check(
        "Named replay uses the newly selected first owner",
        d.app().sequence_cursor == 0,
        json!(0),
        json!(d.app().sequence_cursor),
    )?;
    edit(d, "undo")?;
    let fresh = choose(d, 2, 2)?;
    edit(d, "creep from=1.1 to=2.2 curve=linear")?;
    effect(
        d,
        &baseline,
        &fresh,
        "Fresh direct scoped Apply establishes dot authority",
    )?;
    edit(d, "undo")?;
    let fresh = choose(d, 1, 3)?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    idle(d)?;
    effect(
        d,
        &baseline,
        &fresh,
        "Dot after Undo resolves a fresh concrete outer1 inner3 owner",
    )?;
    d.check(
        "Dot follows the new third owner rather than its former branch",
        d.app().sequence_cursor == 80,
        json!(80),
        json!(d.app().sequence_cursor),
    )?;
    edit(d, "undo")?;
    let final_capture = choose(d, 2, 2)?;
    edit(d, "creep from=1.1 to=2.2 curve=linear")?;
    effect(
        d,
        &baseline,
        &final_capture,
        "The final export has exactly one animated concrete branch",
    )?;
    nested_oracle(d, &baseline, &retime, 164)?;
    for offset in [0, 17, 18, 35] {
        local(d, offset)?;
        let expected = d
            .app()
            .workspace
            .as_ref()
            .ok_or("No final plan")?
            .plan
            .picture(ProjectFrame(
                i64::try_from(d.app().sequence_cursor).map_err(|error| error.to_string())?,
            ))
            .map_err(|error| error.to_string())?;
        let displayed = d
            .app()
            .presentation
            .picture()
            .ok_or("No final scoped stopped picture")?;
        d.check(
            "The scoped stopped picture presents the committed owner pose and exact provider point",
            displayed.framing == expected.framing
                && displayed
                    .follow_point
                    .as_ref()
                    .map(|(asset, point)| (asset, *point))
                    == expected.picture.follow_point()
                && d.app()
                    .presentation
                    .displayed_source_frame()
                    .is_some_and(|id| id.0 == u64::from((2 * offset + 1) / 3)),
            json!({"local":offset,"framing":expected.framing}),
            json!({"framing":displayed.framing,"source_point":displayed.follow_point}),
        )?;
        if offset == 17 {
            d.capture("Scoped Follow creep before the mapped correction")?;
        }
        if offset == 18 {
            d.capture("Scoped Follow creep after the mapped correction")?;
        }
    }
    local(d, 18)?;
    d.check(
        "Final export retains outer Play2 inner Play2 and the independent Original cursor",
        d.app().sequence_cursor == 182
            && d.app().source_cursor == source_cursor
            && d.app()
                .scoped_target()?
                .is_some_and(|target| target.target.repeats.len() == 2),
        json!({"Edit":182,"Original":source_cursor,"selected":[164,200]}),
        d.snapshot(),
    )?;
    d.capture("Scoped Follow creep committed on one nested concrete Retime")?;
    ranges::run(d, &baseline)
}
