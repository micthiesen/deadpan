//! Existing scoped Holds keep identity while duration intent follows invocation.
use super::*;
use crate::preview::copied::Content;
use crate::project::scoped::Target;
use deadpan_core::{HoldRecipe, HoldVideo, PauseLength, SemanticInstruction as I};
use std::num::NonZeroU32;

mod refusals;
const CHANGE: &str = "Change duration…  Enter";

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let baseline = document(d)?.clone();
    let original = d.app().source_cursor;
    let root = d
        .app()
        .selected_beat
        .clone()
        .ok_or("No Repeat for Hold duration")?;
    resize(d, 960.0, 640.0)?;
    pause(d, 3, 5, false)?;
    let inserted = document(d)?.clone();
    let selected = target(d)?;
    let start = d.app().sequence_cursor;
    let recipe = hold(d)?.clone();
    d.chord(&[Key::Num2, Key::L])?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.check(
        "Scoped Hold Enter teaches its exact current duration without opening another group",
        d.app().command_open
            && d.app().command == "hold-duration 5f"
            && target(d)?.target == selected.target
            && d.revision() == revision,
        json!({"command":"hold-duration 5f","selected":"same Hold","history":"unchanged"}),
        d.snapshot(),
    )?;
    submit_field(d, "hold-duration 7f")?;
    d.changed(&revision)?;
    settled(d)?;
    same_hold(d, &selected, &recipe, 7, start)?;
    d.check(
        "The scoped duration footer teaches the available dot action",
        d.app().repeat_hint().as_deref() == Some("repeat: pause length set to 7f"),
        json!("repeat: pause length set to 7f"),
        json!(d.app().repeat_hint()),
    )?;
    d.check(
        "Lengthening changes the selected Hold rather than inserting another pause",
        document(d)?.nodes().len() == inserted.nodes().len()
            && lengths(d, &root)? == [120, 120, 127]
            && unchanged(d, &inserted, &root, 2)?,
        json!({"play_frames":[120,120,127],"node_count":"unchanged","other_plays":"unchanged"}),
        d.snapshot(),
    )?;
    painted(d, CHANGE)?;
    scenarios::footer_anchored(d, "Scoped Hold duration controls keep the compact footer")?;
    d.capture("A lengthened scoped freeze retains its Hold and exact start")?;
    let lengthened = document(d)?.clone();
    d.chord(&[Key::Num2, Key::L])?;
    let revision = d.revision();
    d.click(CHANGE)?;
    submit_field(d, "hold-duration 3f")?;
    d.changed(&revision)?;
    settled(d)?;
    same_hold(d, &selected, &recipe, 3, start)?;
    let shortened = document(d)?.clone();
    d.key(Key::L)?;
    let capture = target(d)?;
    let revision = d.revision();
    let bank = d.app().copied.bank_version();
    d.command("hold-duration 3f")?;
    settled(d)?;
    d.check(
        "An unchanged duration preserves history, the interior cursor and exact scoped target",
        d.revision() == revision
            && target(d)? == capture
            && equivalent(document(d)?, &shortened)
            && d.app().copied.bank_version() == bank,
        json!({"duration":3,"Edit":start+1,"history":"unchanged","bank":"unchanged"}),
        d.snapshot(),
    )?;
    edit(d, "zoom 2 target=center")?;
    let zoomed = document(d)?.clone();
    d.check(
        "Duration continuation applies Zoom to that same Hold",
        target(d)?.target == selected.target
            && document(d)?.nodes()[&selected.target.node]
                .framing
                .is_some(),
        json!({"target":"same Hold","Zoom":"authored"}),
        d.snapshot(),
    )?;
    edit(d, "hold 2f")?;
    d.check(
        "The resized and framed Hold accepts a subsequent pause at the retained interior boundary",
        hold(d)?.duration.frames() == 2
            && target(d)?.target.node != selected.target.node
            && lengths(d, &root)? == [120, 120, 125],
        json!({"new_pause_frames":2,"play_frames":[120,120,125]}),
        d.snapshot(),
    )?;
    undo(d, &zoomed)?;
    undo(d, &shortened)?;
    undo(d, &lengthened)?;
    undo(d, &inserted)?;
    undo(d, &baseline)?;

    recording(d, &baseline, &root)?;
    delayed(d, &baseline, &root, true)?;
    delayed(d, &baseline, &root, false)?;
    refusals::run(d)?;
    d.check(
        "Hold duration verification restores the full document and independent Original clock",
        equivalent(document(d)?, &baseline) && d.app().source_cursor == original,
        json!({"document":"restored","Original":original}),
        d.snapshot(),
    )?;
    resize(d, 1280.0, 820.0)
}

fn recording(d: &mut Driver<'_>, baseline: &ProjectDocument, root: &NodeId) -> Result<(), String> {
    pause(d, 3, 4, false)?;
    let inserted = document(d)?.clone();
    d.chord(&[Key::Q, Key::U])?;
    edit(d, "hold-duration 9f")?;
    d.check(
        "Duration recording acknowledges one completed setter on the selected Hold",
        d.app().macros.recording_name() == Some('u')
            && d.app().macros.instruction_count() == 1
            && hold(d)?.duration.frames() == 9,
        json!({"recording":"u","instructions":1,"Hold_frames":9}),
        d.snapshot(),
    )?;
    d.key(Key::Q)?;
    settled(d)?;
    let intent = I::SetHoldDuration {
        length: PauseLength::Frames {
            frames: NonZeroU32::new(9).unwrap(),
        },
    };
    d.check("The saved Hold-duration macro contains length intent without owner, provider or play identities",
        saved(d,'u')==Some(vec![intent.clone()]) && !d.app().macros.recording(),json!([intent]),d.snapshot())?;
    undo(d, &inserted)?;
    undo(d, baseline)?;
    pause(d, 1, 5, true)?;
    let first = document(d)?.clone();
    pause(d, 3, 4, false)?;
    let both = document(d)?.clone();
    choose_hold(d, 1)?;
    d.chord(&[Key::Num2, Key::L])?;
    let prior = target(d)?;
    let recipe = hold(d)?.clone();
    let start = d.app().sequence_cursor - 2;
    let revision = d.revision();
    super::super::macros::execute(d, 'u', None)?;
    d.changed(&revision)?;
    settled(d)?;
    same_hold(d, &prior, &recipe, 9, start)?;
    d.check(
        "A duration macro replayed on another play keeps that Hold's black provider",
        hold(d)?.video == HoldVideo::Background && lengths(d, root)? == [129, 120, 124],
        json!({"provider":"black","play_frames":[129,120,124]}),
        d.snapshot(),
    )?;
    let replayed = document(d)?.clone();
    edit(d, "hold-duration 6f")?;
    let direct = document(d)?.clone();
    choose_hold(d, 3)?;
    d.key(Key::L)?;
    let prior = target(d)?;
    let recipe = hold(d)?.clone();
    let start = d.app().sequence_cursor - 1;
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    settled(d)?;
    same_hold(d, &prior, &recipe, 6, start)?;
    d.check("Dot sets the newly selected existing Hold and preserves the first play's separate duration",
        lengths(d,root)?==[126,120,126] && matches!(hold(d)?.video,HoldVideo::Freeze{..}),
        json!({"play_frames":[126,120,126],"third_provider":"freeze"}),d.snapshot())?;
    scenarios::footer_anchored(d, "Repeated Hold duration keeps the compact footer")?;
    d.capture("Saved duration and dot preserve different Hold providers in separate plays")?;
    undo(d, &direct)?;
    undo(d, &replayed)?;
    undo(d, &both)?;
    undo(d, &first)?;
    undo(d, baseline)
}

fn delayed(
    d: &mut Driver<'_>,
    baseline: &ProjectDocument,
    root: &NodeId,
    record: bool,
) -> Result<(), String> {
    pause(d, 3, 5, true)?;
    let inserted = document(d)?.clone();
    let bank = d.app().copied.bank_version();
    if record {
        d.chord(&[Key::Q, Key::V])?;
    }
    let revision = d.revision();
    d.app_mut().feedback.hold_project_updates = true;
    let result = (|| {
        d.command("hold-duration 9f")?;
        d.wait_for(
            "Scoped duration is durable before navigation changes",
            |app| !app.service.is_busy(),
        )?;
        if record {
            d.key(Key::Enter)?;
            d.check(
                "Pending recorded duration blocks parameter Enter before opening command mode",
                !d.app().command_open
                    && d.app().macros.recording_name() == Some('v')
                    && d.app().macros.instruction_count() == 0
                    && d.app().error.as_deref()
                        == Some("Wait for the pending macro action to finish."),
                json!({"command_open":false,"recording":"v","instructions":0}),
                d.snapshot(),
            )?;
            d.click(CHANGE)?;
            d.check("Pending recorded duration gives its pointer parameter button the same refusal as Enter",
                !d.app().command_open && d.app().macros.recording_name()==Some('v')
                    && d.app().macros.instruction_count()==0
                    && d.app().error.as_deref()==Some("Wait for the pending macro action to finish."),
                json!({"command_open":false,"recording":"v","instructions":0}),d.snapshot())?;
            let previous = format!("Previous  {}", d.app().editor_key(EditorKey::PlayPrevious));
            d.click(&previous)?;
        } else {
            d.chord(&[Key::OpenBracket, Key::R])?;
        }
        let cursor = d.app().sequence_cursor;
        d.app_mut().feedback.hold_project_updates = false;
        d.changed(&revision)?;
        settled(d)?;
        d.check("A saved Hold duration cannot reclaim newer keyboard or pointer play navigation",
            d.app().scoped.is_none() && d.app().sequence_cursor==cursor && cursor==120
                && lengths(d,root)?==[120,120,129] && !d.app().macros.recording()
                && saved(d,'v').is_none() && d.app().copied.bank_version()==bank,
            json!({"navigation":if record{"pointer"}else{"keyboard"},"Edit":120,"saved_Hold_frames":9,"inspector":"closed","recording":false}),d.snapshot())?;
        if record {
            scenarios::footer_anchored(d, "Delayed duration recording keeps the compact footer")?;
            d.capture("A resized Hold stays saved while newer navigation abandons recording")?;
        }
        undo(d, &inserted)?;
        undo(d, baseline)
    })();
    d.app_mut().feedback.hold_project_updates = false;
    result
}

fn choose(d: &mut Driver<'_>, play: u32) -> Result<(), String> {
    if d.app().scoped.is_some() {
        d.key(Key::Escape)?;
    }
    d.key(Key::Enter)?;
    d.command(&format!("scope play {play}"))?;
    settled(d)
}
fn choose_hold(d: &mut Driver<'_>, play: u32) -> Result<(), String> {
    choose(d, play)?;
    d.key(Key::Enter)?;
    d.key(Key::J)?;
    settled(d)?;
    hold(d).map(|_| ())
}
fn pause(d: &mut Driver<'_>, play: u32, frames: i64, black: bool) -> Result<(), String> {
    choose(d, play)?;
    d.key(Key::L)?;
    edit(
        d,
        &format!("hold {frames}f{}", if black { " video=black" } else { "" }),
    )
}
fn submit_field(d: &mut Driver<'_>, text: &str) -> Result<(), String> {
    d.key_modified(Key::A, egui::Modifiers::COMMAND)?;
    d.events(
        "Replace the scoped Hold-duration parameter",
        vec![
            egui::Event::Text(text.into()),
            super::super::key_event(Key::Enter, egui::Modifiers::NONE, true),
            super::super::key_event(Key::Enter, egui::Modifiers::NONE, false),
        ],
    )
}
fn edit(d: &mut Driver<'_>, text: &str) -> Result<(), String> {
    let revision = d.revision();
    d.command(text)?;
    d.changed(&revision)?;
    settled(d)
}
fn settled(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Hold-duration receipt is admitted", |app| {
        !app.service.is_busy() && !app.macros.is_pending()
    })?;
    d.settled()
}
fn target(d: &Driver<'_>) -> Result<Target, String> {
    d.app()
        .scoped_target()?
        .ok_or("No scoped Hold target".into())
}
fn hold<'a>(d: &'a Driver<'_>) -> Result<&'a HoldRecipe, String> {
    let target = target(d)?;
    match &document(d)?.nodes()[&target.target.node].kind {
        NodeKind::Hold { recipe } => Ok(recipe),
        _ => Err("Selected scoped node is not a Hold".into()),
    }
}
fn same_hold(
    d: &mut Driver<'_>,
    prior: &Target,
    recipe: &HoldRecipe,
    frames: i64,
    start: u64,
) -> Result<(), String> {
    let now = target(d)?;
    let workspace = d.app().workspace.as_ref().ok_or("No duration workspace")?;
    let shown = workspace
        .plan
        .picture(ProjectFrame(
            i64::try_from(start).map_err(|e| e.to_string())?,
        ))
        .map_err(|e| e.to_string())?;
    d.check(
        "A duration commit retains the actual Hold, its provider and its exact start occurrence",
        now.target == prior.target
            && now.presentation.as_ref() == Some(&shown.instance)
            && d.app().sequence_cursor == start
            && hold(d)?.duration.frames() == frames
            && hold(d)?.video == recipe.video
            && hold(d)?.audio == recipe.audio,
        json!({"target":"same Hold","frames":frames,"Edit":start,"provider":"preserved"}),
        d.snapshot(),
    )
}
fn saved(d: &Driver<'_>, name: char) -> Option<Vec<I>> {
    d.app()
        .copied
        .entries()
        .find_map(|(register, value)| match value {
            Content::Macro(program) if register == name => Some(program.instructions().to_vec()),
            _ => None,
        })
}
fn lengths(d: &Driver<'_>, root: &NodeId) -> Result<[i64; 3], String> {
    let document = document(d)?;
    let NodeKind::Repeat {
        child, iterations, ..
    } = &document.nodes()[root].kind
    else {
        return Err("Missing duration Repeat".into());
    };
    let durations = document.durations().map_err(|e| e.to_string())?;
    let mut result = [0; 3];
    for (index, length) in result.iter_mut().enumerate() {
        let iteration = iterations
            .at(u32::try_from(index).map_err(|e| e.to_string())?)
            .ok_or("Missing play")?;
        let child = document
            .overrides()
            .get(root)
            .and_then(|plays| plays.get(&iteration))
            .unwrap_or(child);
        *length = durations[child].frames();
    }
    Ok(result)
}
fn unchanged(
    d: &Driver<'_>,
    before: &ProjectDocument,
    root: &NodeId,
    edited: u32,
) -> Result<bool, String> {
    let after = document(d)?;
    let NodeKind::Repeat {
        child, iterations, ..
    } = &before.nodes()[root].kind
    else {
        return Err("Missing original Repeat".into());
    };
    for index in 0..3 {
        if index == edited {
            continue;
        }
        let iteration = iterations.at(index).ok_or("Missing unchanged play")?;
        let original = before
            .overrides()
            .get(root)
            .and_then(|plays| plays.get(&iteration))
            .unwrap_or(child);
        let mut pending = vec![original.clone()];
        while let Some(node) = pending.pop() {
            if before.nodes().get(&node) != after.nodes().get(&node)
                || before.overrides().get(&node) != after.overrides().get(&node)
                || before.gap_overrides().get(&node) != after.gap_overrides().get(&node)
            {
                return Ok(false);
            }
            pending.extend(before.children(&node).cloned());
        }
    }
    Ok(true)
}
fn equivalent(actual: &ProjectDocument, expected: &ProjectDocument) -> bool {
    let mut actual = serde_json::to_value(actual).unwrap();
    actual["revision_id"] = json!(expected.revision_id());
    actual == serde_json::to_value(expected).unwrap()
}
fn undo(d: &mut Driver<'_>, expected: &ProjectDocument) -> Result<(), String> {
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "One Undo restores the complete prior Hold-duration state",
        equivalent(document(d)?, expected) && d.app().scoped.is_none(),
        json!("restored"),
        d.snapshot(),
    )
}
