//! Root Visual coordinates through nested concrete Plays and a fractional Retime.
//! The oracle uses the retained fixed plan and fixture arithmetic, never the
//! production owner-map or normalized-range helper.
use super::*;
use deadpan_core::{FollowScaleRange, SemanticVisualSelection};

const CREEP: &str = "creep from=1.5 to=2.2 curve=linear";

pub(super) fn select(
    d: &mut Driver<'_>,
    outer: u32,
    inner: u32,
    anchor: u32,
    head: u32,
) -> Result<crate::project::scoped::Target, String> {
    if d.app().edit_range.has_bounds() {
        d.key(Key::Escape)?;
        idle(d)?;
    }
    leave(d)?;
    let earlier_placement = d.app().capture_placement_target()?;
    let target = choose(d, outer, inner)?;
    local(d, anchor)?;
    placement_controls(d, &earlier_placement, false)?;
    d.key(Key::V)?;
    let distance = anchor.abs_diff(head);
    if distance > 0 {
        let mut keys = Vec::new();
        for digit in distance.to_string().chars() {
            keys.push(Key::from_name(&digit.to_string()).ok_or("Invalid range digit")?);
        }
        keys.push(if head > anchor { Key::L } else { Key::H });
        d.chord(&keys)?;
    }
    d.key(Key::V)?;
    idle(d)?;
    // Outer body: 3 * 36 + 2 * 4 = 116. Outer gap: 8.
    // Inner period: 36 + 4 = 40. These are independent fixture facts.
    let start = (outer - 1) * 124 + (inner - 1) * 40;
    d.check("Native v/move/v retains the exact root Time endpoints inside both explicit Plays",
        matches!(d.app().edit_range.semantic()?,Some(SemanticVisualSelection::Time {anchor:a,head:h,extending:false})
            if a==ProjectFrame(i64::from(start+anchor))&&h==ProjectFrame(i64::from(start+head)))
            && d.app().sequence_cursor==u64::from(start+head)
            && d.app().scoped_target()?.is_some_and(|current|current.target==target.target),
        json!({"root_anchor":start+anchor,"root_head":start+head,"extending":false,"scope":target.target}),
        d.snapshot())?;
    placement_controls(d, &earlier_placement, true)?;
    Ok(target)
}

fn placement_controls(
    d: &mut Driver<'_>,
    earlier_placement: &crate::preview::moment::PlacementTarget,
    ranged: bool,
) -> Result<(), String> {
    let expect_buttons = d
        .app()
        .copied
        .selected_content()
        .is_some_and(|content| !matches!(content, crate::preview::copied::Content::Macro(_)));
    let buttons = d
        .harness
        .root()
        .children_recursive()
        .filter_map(|node| {
            let access = node.accesskit_node();
            let label = access.label()?;
            (label.starts_with("Place slice")
                || label.starts_with("Replace range")
                || label.starts_with("Paste after")
                || label.starts_with("Paste before"))
            .then(|| (label.to_string(), access.is_disabled()))
        })
        .collect::<Vec<_>>();
    let camera_disabled = d.harness.root().children_recursive().any(|node| {
        let access = node.accesskit_node();
        access
            .label()
            .is_some_and(|label| label.starts_with("Camera…"))
            && access.is_disabled()
    });
    d.check("Scoped contents refuse fresh and pre-scope placement captures and disable direct slice buttons, with or without a Time range",
        d.app().capture_placement_target().is_err()
            && d.app().check_placement_target(earlier_placement).is_err()
            && camera_disabled == ranged
            && (!expect_buttons || (buttons.len()==3 && buttons.iter().all(|(_,disabled)|*disabled))),
        json!({"fresh_capture":"refused","pre_scope_capture":"refused","slice_buttons_disabled":expect_buttons,"ranged":ranged,"camera_disabled":ranged}),
        json!({"buttons":buttons,"camera_disabled":camera_disabled,"selection":d.app().edit_range.semantic()?}))
}

fn round(n: i128, d: i128) -> i128 {
    let whole = n / d;
    let remainder = n % d;
    whole + i128::from(2 * remainder > d || (2 * remainder == d && whole % 2 != 0))
}

fn quantized(n: i128, d: i128) -> ExactRatio {
    let q = i128::from(deadpan_core::FRAMING_NUMERIC_SCALE);
    ratio(round(n * q, d), q)
}

/// This fixture's Retime is 36 output frames. Frame centers are j + 1/2.
/// Prefix keeps 1.1, start steps to 1.5, window ends at 2.2, then holds.
fn expected(j: i64, start: i64, end: i64) -> ExactRatio {
    if j < start {
        return quantized(11, 10);
    }
    if j >= end {
        return quantized(11, 5);
    }
    let q = i128::from(deadpan_core::FRAMING_NUMERIC_SCALE);
    let from = round(3 * q, 2);
    let to = round(11 * q, 5);
    let progress = round(
        i128::from(2 * (j - start) + 1) * q,
        i128::from(2 * (end - start)),
    );
    ratio(from + round((to - from) * progress, q), q)
}

fn effect(
    d: &mut Driver<'_>,
    baseline: &crate::project::Workspace,
    captured: &crate::project::scoped::Target,
    root_start: i64,
    start: i64,
    end: i64,
) -> Result<(), String> {
    let after = d.app().workspace.clone().ok_or("No ranged workspace")?;
    let selected = d
        .app()
        .scoped_target()?
        .ok_or("Ranged receipt lost its Play")?;
    let aliases = aliases(baseline, &after)?;
    let authored = framing(d, &selected.target.node);
    let mut compared = 0;
    let mut changed = 0;
    let mut frames_exact = true;
    for frame in 0..240 {
        let mut old = baseline
            .plan
            .picture(ProjectFrame(frame))
            .map_err(|e| e.to_string())?;
        let new = after
            .plan
            .picture(ProjectFrame(frame))
            .map_err(|e| e.to_string())?;
        for layer in &mut old.framing {
            if !layer.escalation && captured.presentation.as_ref() == Some(&layer.instance) {
                let j = frame - root_start;
                // A source-Retime owner only exists in its own 36 live pictures.
                frames_exact &= (0..36).contains(&j)
                    && layer.local_position == ratio(i128::from(2 * j + 1), 2)
                    && layer.effective_duration == ExactRatio::integer(36);
                let pose = layer
                    .pose
                    .as_mut()
                    .ok_or("Fixed baseline Follow has no pose")?;
                let scale = expected(j, start, end);
                changed += usize::from(pose.scale != scale);
                pose.scale = scale;
                compared += 1;
            }
        }
        frames_exact &= normalized(view(&new), &aliases) == view(&old);
    }
    let audio_end = baseline.plan.audio_duration().map_err(|e| e.to_string())?;
    let old_audio = baseline
        .plan
        .audio(AudioSample(0)..audio_end, Default::default())
        .map_err(|e| e.to_string())?;
    let new_audio = after
        .plan
        .audio(AudioSample(0)..audio_end, Default::default())
        .map_err(|e| e.to_string())?;
    let audio_exact = normalized(json!(new_audio.spans), &aliases) == json!(old_audio.spans);
    let stable_address = captured.root == selected.root
        && captured.target.repeats.len() == selected.target.repeats.len()
        && captured
            .target
            .repeats
            .iter()
            .zip(&selected.target.repeats)
            .all(|(old, new)| {
                old.branch == new.branch
                    && aliases.get(&new.repeat).unwrap_or(&new.repeat) == &old.repeat
            });
    let isolation = after.document.overrides().len() == 2
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
                    .map_or(&selected.target.node, |step| &step.repeat);
                after
                    .document
                    .overrides()
                    .get(&step.repeat)
                    .is_some_and(|entries| {
                        entries.len() == 1 && entries.get(iteration) == Some(next)
                    })
            });
    let originals = baseline
        .document
        .nodes()
        .iter()
        .all(|(id, node)| after.document.nodes().get(id) == Some(node));
    let clones = after
        .document
        .nodes()
        .iter()
        .filter(|(id, _)| !baseline.document.nodes().contains_key(*id))
        .all(|(id, node)| {
            let Some(original) = aliases.get(id) else {
                return false;
            };
            let mut expected = baseline.document.nodes()[original].clone();
            if id == &selected.target.node {
                expected.framing = authored.clone();
            }
            normalized(json!(node), &aliases) == json!(expected)
        });
    let recipe = matches!(&authored,Some(Framing {clock:FramingClock::OwnerOutput,value:FramingValue::Follow {
        target,scale,fallback,centering:deadpan_core::FollowCentering::Unconstrained,scale_transition:Some(FollowScaleTransition {factor,curve:ZoomCurve::Linear,
            range:Some(FollowScaleRange {start:a,end:b,from_factor})})}})
        if target.as_str()=="target-1"&&*scale==ratio(11,10)&&fallback.scale==ratio(11,10)
            && *factor==ExactRatio::integer(2)&&*a==ratio(i128::from(start),36)
            && *b==ratio(i128::from(end),36)&&*from_factor==ratio(15,11));
    d.check("Ranged Keep Follow changes only one nested occurrence's scale; all 240 canonical providers, layers, captions and complete audio remain exact",
        recipe&&frames_exact&&audio_exact&&stable_address&&isolation&&originals&&clones
            && compared==36&&changed==usize::try_from(36-start).unwrap()
            && after.document.targets()==baseline.document.targets()
            && after.document.gap_overrides()==baseline.document.gap_overrides()
            && after.plan.duration()==baseline.plan.duration()
            && after.plan.audio_duration().is_ok_and(|value|value==audio_end),
        json!({"baseline_revision":baseline.document.revision_id(),"baseline_document":baseline.document,
            "root_owner":[root_start,root_start+36],"root_window":[root_start+start,root_start+end],
            "normalized_window":[ratio(i128::from(start),36),ratio(i128::from(end),36)],
            "prefix":"1.1 live Follow","from":"1.5","terminal":"2.2 live Follow","pictures":240,"selected_owner_pictures":36,"audio_samples":audio_end}),
        json!({"recipe":authored,"recipe_exact":recipe,"frames_exact":frames_exact,"audio_exact":audio_exact,
            "stable_address":stable_address,"isolation":isolation,"originals":originals,"clones":clones,
            "compared":compared,"changed":changed,"aliases":aliases,"selected":selected.target}))
}

fn undo(d: &mut Driver<'_>, baseline: &crate::project::Workspace) -> Result<(), String> {
    edit(d, "undo")?;
    d.check("One Undo restores the full fixed Follow baseline and both nested isolation edges disappear",
        d.app().workspace.as_ref().is_some_and(|workspace|equivalent(&workspace.document,&baseline.document))
            &&d.app().scoped.is_none(),json!(baseline.document),d.snapshot())
}

fn refuse(d: &mut Driver<'_>, command: &str) -> Result<(), String> {
    let revision = d.revision();
    let document = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No range refusal baseline")?
        .document
        .clone();
    let selection = d.app().edit_range.semantic()?;
    let original = d.app().source_cursor;
    let cursor = d.app().sequence_cursor;
    let target = d.app().scoped_target()?;
    d.command(command)?;
    idle(d)?;
    d.check(
        &format!(
            "Scoped Time range refuses unsupported {command} without history or selection changes"
        ),
        d.revision() == revision
            && d.app()
                .workspace
                .as_ref()
                .is_some_and(|w| w.document == document)
            && d.app().edit_range.semantic()? == selection
            && d.app().source_cursor == original
            && d.app().sequence_cursor == cursor
            && d.app().scoped_target()? == target
            && d.app().error.is_some(),
        json!({"command":command,"write":false,"selection":selection,"cursor":cursor}),
        d.snapshot(),
    )
}

fn saved(d: &Driver<'_>, name: char) -> Option<Vec<SemanticInstruction>> {
    d.app()
        .copied
        .entries()
        .find_map(|(register, content)| match content {
            Content::Macro(program) if register == name => Some(program.instructions().to_vec()),
            _ => None,
        })
}

fn inspector(d: &mut Driver<'_>) -> Result<(), String> {
    let heading = d.rect("Nested beat inspector pane")?;
    let viewport = d.harness.ctx.content_rect();
    let target_label = d
        .app()
        .workspace
        .as_ref()
        .and_then(|workspace| {
            workspace
                .document
                .targets()
                .get(&TargetId::new("target-1").unwrap())
        })
        .ok_or("No saved Follow target for inspector check")?
        .label
        .clone();
    let complete = |part: &Value, text: &str| {
        part["text"].as_str() == Some(text)
            && part["elided"] == false
            && part["fully_visible"] == true
            && part["bounds"][0]
                .as_f64()
                .is_some_and(|x| x >= f64::from(heading.left()))
            && part["bounds"][1]
                .as_f64()
                .is_some_and(|y| y >= f64::from(heading.bottom()))
            && part["bounds"][2]
                .as_f64()
                .is_some_and(|x| x <= f64::from(viewport.right()))
    };
    for (label, value) in [
        ("Follows", target_label.as_str()),
        ("Scale before", "1.100×"),
        ("Creep", "1.500× → 2.200× (linear)"),
        ("Window", "16.667%–66.667% of framing duration"),
        ("Framing clock", "Owner output"),
    ] {
        let labels = scenarios::text_paint_visibility(d, label);
        let values = scenarios::text_paint_visibility(d, value);
        let visible = labels.iter().any(|key| {
            complete(key, label)
                && values.iter().any(|part| {
                    complete(part, value)
                        && part["bounds"][1]
                            .as_f64()
                            .zip(key["bounds"][3].as_f64())
                            .is_some_and(|(value_y, key_bottom)| {
                                value_y >= key_bottom - 2.0 && value_y - key_bottom <= 16.0
                            })
                })
        });
        d.check("The nested inspector paints each complete saved range field below its own label",
            visible,json!({"label":label,"value":value,"pane":"Nested beat inspector pane","elided":false,"fully_visible":true}),
            json!({"labels":labels,"values":values}))?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum DelayedChange {
    Frame,
    Range,
    Play,
}

fn late(d: &mut Driver<'_>, change: DelayedChange) -> Result<(), String> {
    let revision = d.revision();
    let document = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No late range baseline")?
        .document
        .clone();
    d.key(Key::Colon)?;
    d.events(
        "Stage ranged creep against its stopped picture, root Time selection and concrete Play",
        vec![egui::Event::Text(CREEP.into())],
    )?;
    // Inject the asynchronous state change after real production command entry.
    // These are state-machine witnesses; v/move/v above exercises native routing.
    match change {
        DelayedChange::Frame => {
            d.app_mut().sequence_cursor += 1;
            d.app_mut().request_picture(false);
        }
        DelayedChange::Range => d.app_mut().edit_range.clear(),
        DelayedChange::Play => {
            let workspace = d.app().workspace.clone().ok_or("No late scope workspace")?;
            d.app_mut()
                .scoped
                .as_mut()
                .ok_or("No late scope")?
                .switch_play(&workspace, 1)?;
            d.app_mut().revoke_scoped_continuation();
            d.app_mut().edit_range.clear();
        }
    }
    let selection = d.app().edit_range.semantic()?;
    let cursor = d.app().sequence_cursor;
    d.key(Key::Enter)?;
    idle(d)?;
    d.check(
        match change {
            DelayedChange::Frame => {
                "A moved stopped-frame capture refuses delayed ranged framing without history"
            }
            DelayedChange::Range => {
                "A changed Time selection refuses delayed ranged framing without history"
            }
            DelayedChange::Play => {
                "A changed concrete Play refuses delayed ranged framing without history"
            }
        },
        d.revision() == revision
            && d.app()
                .workspace
                .as_ref()
                .is_some_and(|w| w.document == document)
            && d.app().edit_range.semantic()? == selection
            && d.app().sequence_cursor == cursor
            && d.app().error.is_some(),
        json!({"write":false,"selection":selection,"cursor":cursor}),
        d.snapshot(),
    )
}

pub(super) fn run(d: &mut Driver<'_>, baseline: &crate::project::Workspace) -> Result<(), String> {
    undo(d, baseline)?;
    let original = d.app().source_cursor;
    let captured = select(d, 2, 2, 6, 24)?;
    for command in ["gain -3dB", "hold 6f", "retime 2 pitch=tape", "splice"] {
        refuse(d, command)?;
    }
    // A macro may consume an already finished range, but cannot record range
    // construction as portable scoped instructions.
    d.chord(&[Key::Q, Key::Z])?;
    let recorded_document = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No recording baseline")?
        .document
        .clone();
    refuse(d, "creep from=1.5 to=2.2 target=current")?;
    d.check(
        "Scoped sampled-target recording refuses without a document change or recorded instruction",
        d.app().macros.recording_name() == Some('z')
            && d.app().macros.instruction_count() == 0
            && d.app()
                .workspace
                .as_ref()
                .is_some_and(|w| w.document == recorded_document)
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("cannot sample")),
        json!({"recording":"z","instructions":0,"document":"unchanged"}),
        d.snapshot(),
    )?;
    let selection = d.app().edit_range.semantic()?;
    let revision = d.revision();
    d.key(Key::V)?;
    idle(d)?;
    d.check("Scoped recording refuses v range construction while retaining the preselected finished range",
        d.revision()==revision&&d.app().edit_range.semantic()?==selection
            &&d.app().macros.recording_name()==Some('z')&&d.app().macros.instruction_count()==0,
        json!({"recording":"z","instructions":0,"selection":selection}),d.snapshot())?;
    edit(d, CREEP)?;
    effect(d, baseline, &captured, 164, 6, 24)?;
    inspector(d)?;
    d.key(Key::Q)?;
    idle(d)?;
    let intent = vec![SemanticInstruction::Zoom {
        kind: ZoomKind::Creep {
            from: Some(ratio(3, 2)),
            to: ratio(11, 5),
            curve: ZoomCurve::Linear,
        },
        center: SemanticZoomCenter::Keep,
    }];
    d.check("The saved ranged framing macro contains portable Keep intent without recorded endpoints or occurrence IDs",
        saved(d,'z')==Some(intent.clone())&&!d.app().macros.recording(),json!(intent),json!(saved(d,'z')))?;
    let authored = d
        .app()
        .workspace
        .clone()
        .ok_or("No ranged authored document")?;
    undo(d, baseline)?;
    edit(d, "redo")?;
    d.check(
        "One Redo restores the exact ranged recipe and physical cloned identities",
        d.app()
            .workspace
            .as_ref()
            .is_some_and(|w| equivalent(&w.document, &authored.document)),
        json!(authored.document),
        d.snapshot(),
    )?;
    undo(d, baseline)?;

    let captured = select(d, 1, 1, 30, 12)?;
    let revision = d.revision();
    super::super::macros::execute(d, 'z', None)?;
    d.changed(&revision)?;
    idle(d)?;
    effect(d, baseline, &captured, 0, 12, 30)?;
    d.capture("Named Keep macro uses a fresh backward Time range on another nested Play")?;
    undo(d, baseline)?;

    // Establish direct intent before dot, then invoke on fresh geometry.
    let captured = select(d, 2, 2, 6, 24)?;
    edit(d, CREEP)?;
    effect(d, baseline, &captured, 164, 6, 24)?;
    undo(d, baseline)?;
    let captured = select(d, 1, 3, 9, 27)?;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    idle(d)?;
    effect(d, baseline, &captured, 80, 9, 27)?;
    d.capture("Dot resolves the current nested Play and its fresh Time window")?;
    undo(d, baseline)?;

    select(d, 2, 2, 6, 24)?;
    late(d, DelayedChange::Frame)?;
    select(d, 2, 2, 6, 24)?;
    late(d, DelayedChange::Range)?;
    select(d, 2, 2, 6, 24)?;
    late(d, DelayedChange::Play)?;
    let captured = select(d, 2, 2, 24, 6)?;
    edit(d, CREEP)?;
    effect(d, baseline, &captured, 164, 6, 24)?;
    // Leave the selected range finished while inspecting frame 18, where the
    // point correction changes the live target independently of scale.
    local(d, 18)?;
    d.check("Ranged Follow retains the ordinary Original cursor and the selected nested owner's root Edit clock",
        d.app().source_cursor==original&&d.app().sequence_cursor==182,
        json!({"Original":original,"Edit":182}),d.snapshot())?;
    inspector(d)?;
    d.capture(
        "Nested explicit Play range keeps live Follow before, within and after its saved window",
    )
}
