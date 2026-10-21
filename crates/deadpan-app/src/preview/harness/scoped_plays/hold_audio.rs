//! Qualified Original samples and audio intent through real nested-play controls.
use super::*;
use crate::preview::copied::Content;
use deadpan_core::{
    BeatNode, Command, CommandRequest, FrameDuration, HoldAudio, HoldRecipe, HoldVideo,
    IterationOrder, RegisterName, RevisionId, SemanticInstruction as I, SourceAudio, SourceSpan,
    SourceTimeBase, SourceTimestamp, Subtree,
};
use egui::Modifiers;
use std::{collections::BTreeMap, num::NonZeroU32, path::Path};

const ROOM: &str = "Room tone…  :room-tone";
const SILENCE: &str = "Use silence  :hold-silence";
const APPLY: &str = "Apply room tone  Enter";
const CANCEL: &str = "Cancel  Esc";
const IN: &str = "Room tone In sample";
const OUT: &str = "Room tone Out sample";
const PENDING: &str = "Wait for the pending macro action to finish.";

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let previous = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No audio workspace")?
        .path
        .clone();
    let original = d.app().source_cursor;
    let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
    let path = scratch.path().join("scoped-hold-audio.deadpan");
    fixture(&previous, &path)?;
    let expected = path.canonicalize().map_err(|e| e.to_string())?;
    d.app().service.submit(ProjectRequest::Open(path))?;
    let result = (|| {
        d.wait_for(
            "Open the private qualified nested Hold audio project",
            |app| {
                app.workspace.as_ref().is_some_and(|w| w.path == expected) && !app.service.is_busy()
            },
        )?;
        resize(d, 960.0, 640.0)?;
        copy_original(d)?;
        let copied = copied_samples(d)?;
        let baseline = document(d)?.clone();
        let source_cursor = d.app().source_cursor;
        cancellation(d, &baseline, &copied)?;
        let chosen = room_recording(d, &baseline, &copied)?;
        calls_and_dot(d, &baseline, &chosen)?;
        silence_recording(d, &baseline)?;
        direct_scopes(d, &baseline, &copied)?;
        delayed(d, &baseline, &chosen, true)?;
        delayed(d, &baseline, &chosen, false)?;
        d.check(
            "Nested audio verification restores the complete fixture and independent Original clock",
            equivalent(document(d)?, &baseline) && d.app().source_cursor == source_cursor,
            json!({"document":"restored","Original":source_cursor}), d.snapshot(),
        )
    })();
    d.app_mut().feedback.hold_project_updates = false;
    let cleanup = (|| {
        d.app().service.submit(ProjectRequest::Close)?;
        d.wait_for("Close the private Hold audio writer", |app| {
            app.workspace.is_none() && !app.service.is_busy()
        })?;
        d.app()
            .service
            .submit(ProjectRequest::Open(previous.clone()))?;
        d.wait_for("Restore the main scoped-play workspace", |app| {
            app.workspace.as_ref().is_some_and(|w| w.path == previous) && !app.service.is_busy()
        })?;
        d.command("source")?;
        d.chord(&[Key::G, Key::G])?;
        move_frames(d, original)?;
        d.command("sequence")?;
        d.chord(&[Key::G, Key::G])?;
        settled(d)?;
        resize(d, 1280.0, 820.0)?;
        restore_inspector_scroll(d)
    })();
    if cleanup.is_err() {
        let _ = scratch.keep();
    }
    match (result, cleanup) {
        (Err(error), Err(cleanup)) => Err(format!("{error}; Hold audio cleanup: {cleanup}")),
        (Err(error), _) | (_, Err(error)) => Err(error),
        _ => Ok(()),
    }
}

fn restore_inspector_scroll(d: &mut Driver<'_>) -> Result<(), String> {
    let before = (
        d.revision(),
        d.app().source_cursor,
        d.app().sequence_cursor,
        d.app().selected_beat.clone(),
    );
    // The two inspectors retain separate scroll IDs across project opens.
    // Restore both through real wheel input before another scenario uses them.
    scroll_inspector_top(d, "Selected beat inspector pane")?;
    d.key(Key::Enter)?;
    settled(d)?;
    scroll_inspector_top(d, "Nested beat inspector pane")?;
    resize(d, 960.0, 640.0)?;
    painted(d, "Insert pause")?;
    d.key(Key::Escape)?;
    settled(d)?;
    resize(d, 1280.0, 820.0)?;
    d.check(
        "Audio replay restores inspector scroll without changing the main document or either clock",
        before == (d.revision(), d.app().source_cursor, d.app().sequence_cursor, d.app().selected_beat.clone())
            && d.app().scoped.is_none(),
        json!({"history":"unchanged","cursors":"unchanged","selection":"unchanged","scoped":false,"Insert pause":"fully painted at minimum size"}),
        d.snapshot(),
    )
}

fn scroll_inspector_top(d: &mut Driver<'_>, heading: &str) -> Result<(), String> {
    let point = d.rect(heading)?.center() + egui::vec2(0.0, 120.0);
    d.events(
        "Restore inspector controls to their first row with the wheel",
        vec![
            egui::Event::PointerMoved(point),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, 10_000.0),
                phase: egui::TouchPhase::Move,
                modifiers: Modifiers::NONE,
            },
        ],
    )?;
    for _ in 0..8 {
        d.step("Inspector baseline scroll settles", false)?;
    }
    Ok(())
}

fn cancellation(
    d: &mut Driver<'_>,
    baseline: &ProjectDocument,
    source: &SourceAudio,
) -> Result<(), String> {
    choose(d, Some(3), Some(2))?;
    let cursor = d.app().sequence_cursor;
    let target = d.app().scoped_target()?;
    let bank = d.app().copied.bank_version();
    d.command("record a")?;
    open(d, false)?;
    prepare_edited(d, source)?;
    d.key(Key::Escape)?;
    open(d, true)?;
    d.click(CANCEL)?;
    d.check(
        "Keyboard and pointer cancellation append no scoped audio instruction or authored edit",
        d.app().room_tone.is_none()
            && d.app().macros.recording_name() == Some('a')
            && d.app().macros.instruction_count() == 0
            && d.app().copied.bank_version() == bank
            && equivalent(document(d)?, baseline)
            && d.app().sequence_cursor == cursor
            && d.app().scoped_target()? == target,
        json!({"recording":"a","instructions":0,"history":"unchanged","target":"unchanged"}),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    settled(d)
}

fn room_recording(
    d: &mut Driver<'_>,
    baseline: &ProjectDocument,
    source: &SourceAudio,
) -> Result<SourceAudio, String> {
    choose(d, Some(3), Some(2))?;
    let cursor = d.app().sequence_cursor;
    let original = d.app().source_cursor;
    d.command("record b")?;
    open(d, false)?;
    let chosen = prepare_edited(d, source)?;
    d.check(
        "Preparing exact Original samples during nested recording changes no authored state",
        equivalent(document(d)?, baseline)
            && d.app().macros.instruction_count() == 0
            && prepared(d)?.source == chosen,
        json!({"source":chosen,"instructions":0,"history":"unchanged"}),
        d.snapshot(),
    )?;
    scenarios::footer_anchored(d, "Prepared nested room tone keeps the compact footer")?;
    d.capture("Exact Original sample endpoints prepare before nested-play recording applies")?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    settled(d)?;
    changed(
        d,
        baseline,
        &[(3, 2)],
        &HoldAudio::RoomTone {
            source: chosen.clone(),
        },
        cursor,
        original,
    )?;
    d.check(
        "Only acknowledged nested RoomTone Apply becomes a recorded instruction",
        d.app().room_tone.is_none() && d.app().macros.instruction_count() == 1,
        json!({"sheet":"closed","instructions":1}),
        d.snapshot(),
    )?;
    d.command("record-stop")?;
    settled(d)?;
    d.check(
        "Saved RoomTone intent retains exact Original sample endpoints without a Hold or play identity",
        saved(d, 'b') == Some(vec![I::SetHoldAudio { audio: HoldAudio::RoomTone { source: chosen.clone() } }]),
        json!({"source":chosen}), json!(saved(d, 'b')),
    )?;
    undo(d, baseline)?;
    Ok(chosen)
}

fn calls_and_dot(
    d: &mut Driver<'_>,
    baseline: &ProjectDocument,
    chosen: &SourceAudio,
) -> Result<(), String> {
    choose(d, Some(1), Some(1))?;
    let (cursor, original) = (d.app().sequence_cursor, d.app().source_cursor);
    d.command("record c")?;
    execute(d, 'b')?;
    changed(
        d,
        baseline,
        &[(1, 1)],
        &HoldAudio::RoomTone {
            source: chosen.clone(),
        },
        cursor,
        original,
    )?;
    d.command("record-stop")?;
    settled(d)?;
    d.check(
        "A named caller records Call intent rather than expanded concrete audio targets",
        saved(d, 'c')
            == Some(vec![I::Call {
                register: RegisterName::new('b').map_err(|e| e.to_string())?,
                count: NonZeroU32::new(1).unwrap(),
            }]),
        json!({"call":"b","count":1}),
        json!(saved(d, 'c')),
    )?;
    undo(d, baseline)?;
    choose(d, Some(2), Some(1))?;
    let (cursor, original) = (d.app().sequence_cursor, d.app().source_cursor);
    execute(d, 'c')?;
    changed(
        d,
        baseline,
        &[(2, 1)],
        &HoldAudio::RoomTone {
            source: chosen.clone(),
        },
        cursor,
        original,
    )?;
    undo(d, baseline)?;

    choose(d, Some(1), Some(1))?;
    open(d, true)?;
    set_range(d, chosen.span.start().ticks, chosen.span.end().ticks)?;
    let revision = d.revision();
    d.click(APPLY)?;
    d.changed(&revision)?;
    settled(d)?;
    let first = document(d)?.clone();
    d.check(
        "Direct scoped RoomTone Apply teaches its available dot action",
        d.app().repeat_hint().as_deref() == Some("repeat: room tone applied"),
        json!("repeat: room tone applied"),
        json!(d.app().repeat_hint()),
    )?;
    choose(d, Some(2), Some(2))?;
    let (cursor, original) = (d.app().sequence_cursor, d.app().source_cursor);
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    settled(d)?;
    changed(
        d,
        &first,
        &[(2, 2)],
        &HoldAudio::RoomTone {
            source: chosen.clone(),
        },
        cursor,
        original,
    )?;
    painted(d, "This play 2/2")?;
    scenarios::footer_anchored(d, "Nested RoomTone dot preserves the compact footer")?;
    d.capture("RoomTone dot applies the same exact samples to the newly chosen nested play")?;
    undo(d, &first)?;
    undo(d, baseline)
}

fn silence_recording(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    choose(d, Some(3), Some(2))?;
    execute(d, 'b')?;
    let one = document(d)?.clone();
    choose(d, Some(1), Some(1))?;
    execute(d, 'b')?;
    let both = document(d)?.clone();
    choose(d, Some(3), Some(2))?;
    let (cursor, original) = (d.app().sequence_cursor, d.app().source_cursor);
    d.command("record d")?;
    let revision = d.revision();
    click_inspector(d, SILENCE)?;
    d.changed(&revision)?;
    settled(d)?;
    changed(d, &both, &[(3, 2)], &HoldAudio::Silence, cursor, original)?;
    d.command("record-stop")?;
    settled(d)?;
    d.check(
        "The scoped silence button records exactly one Silence instruction",
        saved(d, 'd')
            == Some(vec![I::SetHoldAudio {
                audio: HoldAudio::Silence,
            }]),
        json!(["set_hold_audio silence"]),
        json!(saved(d, 'd')),
    )?;
    undo(d, &both)?;
    choose(d, Some(1), Some(1))?;
    let (cursor, original) = (d.app().sequence_cursor, d.app().source_cursor);
    execute(d, 'd')?;
    changed(d, &both, &[(1, 1)], &HoldAudio::Silence, cursor, original)?;
    undo(d, &both)?;
    choose(d, Some(3), Some(2))?;
    let revision = d.revision();
    d.command("hold-silence")?;
    d.changed(&revision)?;
    settled(d)?;
    let first = document(d)?.clone();
    d.check(
        "The scoped silence command teaches its available dot action",
        d.app().repeat_hint().as_deref() == Some("repeat: pause sound set to silence"),
        json!("repeat: pause sound set to silence"),
        json!(d.app().repeat_hint()),
    )?;
    choose(d, Some(1), Some(1))?;
    let (cursor, original) = (d.app().sequence_cursor, d.app().source_cursor);
    let revision = d.revision();
    d.key(Key::Period)?;
    d.changed(&revision)?;
    settled(d)?;
    changed(d, &first, &[(1, 1)], &HoldAudio::Silence, cursor, original)?;
    undo(d, &first)?;
    undo(d, &both)?;
    undo(d, &one)?;
    undo(d, baseline)
}

fn direct_scopes(
    d: &mut Driver<'_>,
    baseline: &ProjectDocument,
    copied: &SourceAudio,
) -> Result<(), String> {
    for (outer, inner, changes, label) in [
        (
            None,
            None,
            vec![(1, 1), (1, 2), (2, 1), (2, 2), (3, 1), (3, 2)],
            "All plays",
        ),
        (Some(3), None, vec![(3, 1), (3, 2)], "Mixed Play and All"),
    ] {
        choose(d, outer, inner)?;
        let (cursor, original) = (d.app().sequence_cursor, d.app().source_cursor);
        let bank = d.app().copied.bank_version();
        open(d, false)?;
        let revision = d.revision();
        d.click(APPLY)?;
        d.changed(&revision)?;
        settled(d)?;
        changed(
            d,
            baseline,
            &changes,
            &HoldAudio::RoomTone {
                source: copied.clone(),
            },
            cursor,
            original,
        )?;
        d.check(
            &format!("{label} nonrecording audio retains its direct path and bank"),
            d.app().copied.bank_version() == bank && !d.app().macros.is_pending(),
            json!({"bank":"unchanged","semantic_pending":false}),
            d.snapshot(),
        )?;
        let toned = document(d)?.clone();
        let revision = d.revision();
        d.command("hold-silence")?;
        d.changed(&revision)?;
        settled(d)?;
        changed(d, &toned, &changes, &HoldAudio::Silence, cursor, original)?;
        undo(d, &toned)?;
        undo(d, baseline)?;
    }
    ordinary(d)?;
    d.chord(&[Key::V, Key::L])?;
    let visual = d.app().capture_visual_selection()?;
    d.check(
        "The ordinary audio witness has a real nonempty Visual selection",
        visual.is_some(),
        json!("Visual active"),
        d.snapshot(),
    )?;
    let cursor = d.app().sequence_cursor;
    let bank = d.app().copied.bank_version();
    open(d, true)?;
    let revision = d.revision();
    d.key(Key::Enter)?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "Ordinary nonrecording RoomTone remains available with a Visual selection",
        document(d)?.nodes()[&id("audio-ordinary")?].kind
            == NodeKind::Hold {
                recipe: HoldRecipe {
                    audio: HoldAudio::RoomTone {
                        source: copied.clone(),
                    },
                    ..hold_at(baseline, 1, 1)?.clone()
                },
            }
            && d.app().sequence_cursor == cursor
            && d.app().copied.bank_version() == bank,
        json!({"audio":"RoomTone","Visual":"available on entry","bank":"unchanged"}),
        d.snapshot(),
    )?;
    let toned = document(d)?.clone();
    let revision = d.revision();
    d.command("hold-silence")?;
    d.changed(&revision)?;
    settled(d)?;
    d.check(
        "Ordinary nonrecording Silence retains its existing availability",
        matches!(&document(d)?.nodes()[&id("audio-ordinary")?].kind,
            NodeKind::Hold { recipe } if recipe.audio==HoldAudio::Silence)
            && d.app().sequence_cursor == cursor
            && d.app().copied.bank_version() == bank,
        json!({"audio":"Silence","Edit":cursor,"bank":"unchanged"}),
        d.snapshot(),
    )?;
    undo(d, &toned)?;
    undo(d, baseline)
}

fn delayed(
    d: &mut Driver<'_>,
    baseline: &ProjectDocument,
    chosen: &SourceAudio,
    record: bool,
) -> Result<(), String> {
    choose(d, Some(3), Some(2))?;
    if record {
        d.command("record e")?;
    }
    open(d, false)?;
    set_range(d, chosen.span.start().ticks, chosen.span.end().ticks)?;
    let bank = d.app().copied.bank_version();
    let previous_macro = saved(d, 'e');
    let revision = d.revision();
    d.app_mut().feedback.hold_project_updates = true;
    let result = (|| {
        d.click(APPLY)?;
        d.wait_for(
            "Scoped audio is durable while the real completion is withheld",
            |app| !app.service.is_busy(),
        )?;
        if record {
            d.key(Key::Colon)?;
            pending_refused(d, "Pending recorded audio refuses keyboard command entry")?;
            click_inspector(d, ROOM)?;
            pending_refused(d, "Pending recorded audio refuses pointer RoomTone entry")?;
            click_inspector(d, SILENCE)?;
            pending_refused(
                d,
                "Pending recorded audio refuses the pointer Silence command",
            )?;
            let previous = format!("Previous  {}", d.app().editor_key(EditorKey::PlayPrevious));
            d.click(&previous)?;
        } else {
            d.chord(&[Key::OpenBracket, Key::R])?;
        }
        let cursor = d.app().sequence_cursor;
        d.app_mut().feedback.hold_project_updates = false;
        d.changed(&revision)?;
        settled(d)?;
        d.check(
            "Delayed scoped audio cannot reclaim newer navigation or append abandoned recording",
            d.app().scoped.is_none() && d.app().sequence_cursor==cursor
                && !d.app().macros.recording() && saved(d,'e')==previous_macro
                && d.app().copied.bank_version()==bank
                && hold_at(document(d)?,3,2)?.audio == (HoldAudio::RoomTone { source: chosen.clone() })
                && document(d)?.duration().map_err(|e|e.to_string())? == baseline.duration().map_err(|e|e.to_string())?,
            json!({"navigation":if record{"pointer"}else{"keyboard"},"Edit":cursor,"recording":false,"saved_audio":"RoomTone","inspector":"closed"}),d.snapshot(),
        )?;
        undo(d, baseline)
    })();
    d.app_mut().feedback.hold_project_updates = false;
    result
}

fn pending_refused(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    d.check(
        label,
        !d.app().command_open
            && d.app().room_tone.is_none()
            && d.app().macros.recording_name() == Some('e')
            && d.app().macros.instruction_count() == 0
            && d.app().error.as_deref() == Some(PENDING),
        json!({"command":false,"sheet":false,"instructions":0,"error":PENDING}),
        d.snapshot(),
    )
}

fn choose(d: &mut Driver<'_>, outer: Option<u32>, inner: Option<u32>) -> Result<(), String> {
    if d.app().scoped.is_some() {
        d.key(Key::Escape)?;
    }
    d.command("sequence")?;
    super::super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::Enter])?;
    d.command(&outer.map_or_else(|| "scope all".into(), |p| format!("scope play {p}")))?;
    d.key(Key::Enter)?;
    d.command(&inner.map_or_else(|| "scope all".into(), |p| format!("scope play {p}")))?;
    d.chord(&[Key::Num2, Key::L])?;
    settled(d)?;
    d.app().scoped_target()?.ok_or("No nested Hold target")?;
    Ok(())
}
fn ordinary(d: &mut Driver<'_>) -> Result<(), String> {
    if d.app().scoped.is_some() {
        d.key(Key::Escape)?;
    }
    d.command("sequence")?;
    super::super::transcript::focus_your_edit(d)?;
    d.chord(&[Key::G, Key::G, Key::J])?;
    settled(d)?;
    if d.app().selected_beat.as_ref() != Some(&id("audio-ordinary")?) {
        return Err("Ordinary audio fixture lost its actual row".into());
    }
    Ok(())
}
fn open(d: &mut Driver<'_>, pointer: bool) -> Result<(), String> {
    if pointer {
        click_inspector(d, ROOM)?;
    } else {
        d.command("room-tone")?;
    }
    wait_prepared(d)
}

fn click_inspector(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    if d.app().scoped.is_none() {
        scenarios::reveal_inspector_button(d, label)?;
        return d.click(label);
    }
    // The shared ordinary-inspector helper uses its own heading. Nested
    // contents expose a distinct focus target, but use the same real wheel
    // input and complete paint-clip check before clicking an action.
    let visible = |d: &Driver<'_>| {
        let paint = scenarios::text_paint_visibility(d, label);
        d.rect(label).is_ok()
            && !paint.is_empty()
            && paint.iter().all(|part| part["fully_visible"] == true)
    };
    if visible(d) {
        return d.click(label);
    }
    let before = (
        d.revision(),
        d.app().source_cursor,
        d.app().sequence_cursor,
        d.app().scoped_target()?,
    );
    let point = d.rect("Nested beat inspector pane")?.center() + egui::vec2(0.0, 120.0);
    for attempt in 0..6 {
        let bounds = d
            .harness
            .root()
            .children_recursive()
            .find_map(|node| {
                let access = node.accesskit_node();
                (access.label().as_deref() == Some(label)
                    && !access.is_disabled()
                    && !access.is_hidden()
                    && node.rect().is_positive())
                .then(|| node.rect())
            })
            .ok_or_else(|| format!("Missing enabled nested inspector action {label:?}"))?;
        d.events(
            &format!(
                "Wheel nested inspector to reveal {label}, attempt {}",
                attempt + 1
            ),
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(
                        0.0,
                        if bounds.center().y < point.y {
                            240.0
                        } else {
                            -240.0
                        },
                    ),
                    phase: egui::TouchPhase::Move,
                    modifiers: Modifiers::NONE,
                },
            ],
        )?;
        for _ in 0..8 {
            d.step("Nested inspector scroll settles", false)?;
        }
        if visible(d) {
            d.check(
                "Nested inspector wheel reveals the complete action without changing its captured target",
                before == (d.revision(), d.app().source_cursor, d.app().sequence_cursor, d.app().scoped_target()?),
                json!({"label":label,"history":"unchanged","cursors":"unchanged","target":"unchanged"}),
                json!({"paint":scenarios::text_paint_visibility(d,label),"wheel_attempts":attempt+1}),
            )?;
            return d.click(label);
        }
    }
    Err(format!(
        "Nested inspector action {label:?} remained outside its paint clip after six wheel attempts"
    ))
}
fn wait_prepared(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("The exact scoped RoomTone source is prepared", |app| {
        !app.service.is_busy()
            && app
                .room_tone
                .as_ref()
                .is_some_and(|draft| draft.prepared.is_some())
    })
}
fn prepared<'a>(d: &'a Driver<'_>) -> Result<&'a crate::project::PreparedRoomTone, String> {
    d.app()
        .room_tone
        .as_ref()
        .and_then(|draft| draft.prepared.as_ref())
        .ok_or("No prepared RoomTone source".into())
}
fn set_range(d: &mut Driver<'_>, start: i64, end: i64) -> Result<(), String> {
    for (label, value) in [(IN, start), (OUT, end)] {
        let rects = d
            .harness
            .root()
            .children_recursive()
            .filter(|node| {
                let access = node.accesskit_node();
                access.role() == egui::accesskit::Role::TextInput
                    && access.label().as_deref() == Some(label)
                    && !access.is_disabled()
                    && !access.is_hidden()
            })
            .map(|node| node.rect())
            .collect::<Vec<_>>();
        let [rect] = rects.as_slice() else {
            return Err(format!("Expected one RoomTone input {label:?}"));
        };
        d.click_at(label, rect.center())?;
        d.key_modified(Key::A, Modifiers::COMMAND)?;
        d.events(
            "Edit exact RoomTone sample endpoint",
            vec![egui::Event::Text(value.to_string())],
        )?;
    }
    d.click("Prepare range")?;
    wait_prepared(d)?;
    d.check(
        "Prepared RoomTone keeps the edited integer sample endpoints",
        prepared(d)?.source.span.start().ticks == start
            && prepared(d)?.source.span.end().ticks == end
            && prepared(d)?.audition.duration_samples().0 == end - start,
        json!({"samples":[start,end]}),
        json!(prepared(d)?.source),
    )
}
fn prepare_edited(d: &mut Driver<'_>, source: &SourceAudio) -> Result<SourceAudio, String> {
    let start = source.span.start().ticks + 1;
    let end = source.span.end().ticks - 1;
    set_range(d, start, end)?;
    let chosen = SourceAudio {
        asset: source.asset.clone(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: start,
                ..source.span.start()
            },
            SourceTimestamp {
                ticks: end,
                ..source.span.end()
            },
        )
        .map_err(|e| e.to_string())?,
    };
    if prepared(d)?.source != chosen {
        return Err("Prepared audio differs from the independently edited Original span".into());
    }
    Ok(chosen)
}
fn copy_original(d: &mut Driver<'_>) -> Result<(), String> {
    d.command("source")?;
    d.chord(&[
        Key::G,
        Key::G,
        Key::Num1,
        Key::Num0,
        Key::L,
        Key::V,
        Key::Num1,
        Key::Num0,
        Key::L,
        Key::Y,
    ])?;
    d.wait_for(
        "Save the real Original range before audio recording",
        |app| !app.service.is_busy() && !app.copied.is_pending(),
    )?;
    d.check(
        "The audio fixture copies Original pictures [10,20) through v/y",
        d.app()
            .copied
            .original()
            .is_some_and(|copy| copy.ordinals == (10..20)),
        json!([10, 20]),
        d.snapshot(),
    )?;
    d.command("sequence")?;
    settled(d)
}
fn copied_samples(d: &Driver<'_>) -> Result<SourceAudio, String> {
    let workspace = d.app().workspace.as_ref().ok_or("No qualified workspace")?;
    let asset = d
        .app()
        .selected_source
        .as_ref()
        .ok_or("No Original selected")?;
    let receipt = workspace
        .sources
        .get(asset)
        .ok_or("Missing Original qualification")?
        .receipt
        .snapshot();
    let video = receipt
        .video()
        .ok_or("No Original picture index")?
        .index()
        .index();
    let rate = receipt
        .audio()
        .ok_or("No Original audio")?
        .stream()
        .sample_rate;
    let sample = |ordinal, ceil: bool| -> Result<i64, String> {
        let ticks = video
            .interval(SourceFrameId(ordinal))
            .map_err(|e| e.to_string())?
            .0;
        let base = video.time_base();
        let numerator = i128::from(ticks) * i128::from(base.numerator()) * i128::from(rate);
        let denominator = i128::from(base.denominator());
        let floor = numerator.div_euclid(denominator);
        i64::try_from(floor + i128::from(ceil && numerator.rem_euclid(denominator) != 0))
            .map_err(|e| e.to_string())
    };
    let clock = SourceTimeBase::new(1, rate).map_err(|e| e.to_string())?;
    Ok(SourceAudio {
        asset: asset.clone(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: sample(10, true)?,
                time_base: clock,
            },
            SourceTimestamp {
                ticks: sample(20, false)?,
                time_base: clock,
            },
        )
        .map_err(|e| e.to_string())?,
    })
}
fn changed(
    d: &mut Driver<'_>,
    before: &ProjectDocument,
    changes: &[(u32, u32)],
    audio: &HoldAudio,
    cursor: u64,
    original: u64,
) -> Result<(), String> {
    let after = document(d)?;
    let mut unchanged = true;
    for outer in 1..=3 {
        for inner in 1..=2 {
            let old = hold_at(before, outer, inner)?;
            let mut expected = old.clone();
            if changes.contains(&(outer, inner)) {
                expected.audio = audio.clone();
            }
            unchanged &= hold_at(after, outer, inner)? == &expected;
        }
    }
    let target = d
        .app()
        .scoped_target()?
        .ok_or("Audio completion lost its nested inspector")?;
    let picture = d
        .app()
        .workspace
        .as_ref()
        .ok_or("No audio plan")?
        .plan
        .picture(ProjectFrame(
            i64::try_from(cursor).map_err(|e| e.to_string())?,
        ))
        .map_err(|e| e.to_string())?;
    d.check("Audio intent preserves exact nested Hold, timing, provider, cursors and unselected plays",
        unchanged&&after.duration().map_err(|e|e.to_string())?==before.duration().map_err(|e|e.to_string())?
            && d.app().sequence_cursor==cursor&&d.app().source_cursor==original
            &&target.target.repeats.len()==2
            &&(!target.has_single_play_authority() || target.presentation.as_ref()==Some(&picture.instance))
            &&target.target.node==picture.instance.node,
        json!({"changes":changes,"Edit":cursor,"Original":original,"duration":"unchanged","other_plays":"unchanged"}),d.snapshot())
}
fn branch<'a>(doc: &'a ProjectDocument, owner: &NodeId, play: u32) -> Result<&'a NodeId, String> {
    let NodeKind::Repeat {
        child, iterations, ..
    } = &doc.nodes().get(owner).ok_or("No fixture Repeat")?.kind
    else {
        return Err("Fixture path is not a Repeat".into());
    };
    let iteration = iterations.at(play - 1).ok_or("No fixture play")?;
    Ok(doc
        .overrides()
        .get(owner)
        .and_then(|o| o.get(&iteration))
        .unwrap_or(child))
}
fn hold_at(doc: &ProjectDocument, outer: u32, inner: u32) -> Result<&HoldRecipe, String> {
    let owner = branch(doc, &id("audio-outer")?, outer)?;
    let leaf = branch(doc, owner, inner)?;
    match &doc.nodes()[leaf].kind {
        NodeKind::Hold { recipe } => Ok(recipe),
        _ => Err("Nested fixture target is not a Hold".into()),
    }
}
fn execute(d: &mut Driver<'_>, name: char) -> Result<(), String> {
    let revision = d.revision();
    super::super::macros::execute(d, name, None)?;
    d.changed(&revision)?;
    settled(d)
}
fn saved(d: &Driver<'_>, name: char) -> Option<Vec<I>> {
    d.app()
        .copied
        .entries()
        .find_map(|(slot, value)| match value {
            Content::Macro(p) if slot == name => Some(p.instructions().to_vec()),
            _ => None,
        })
}
fn settled(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Scoped audio receipt is acknowledged", |app| {
        !app.service.is_busy() && !app.macros.is_pending()
    })?;
    d.settled()
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
        "One Undo restores the complete pre-audio document",
        equivalent(document(d)?, expected) && d.app().scoped.is_none(),
        json!("restored"),
        d.snapshot(),
    )
}
fn move_frames(d: &mut Driver<'_>, frames: u64) -> Result<(), String> {
    if frames == 0 {
        return Ok(());
    }
    let mut keys = frames
        .to_string()
        .chars()
        .map(|c| Key::from_name(&c.to_string()).ok_or("Invalid frame digit".to_owned()))
        .collect::<Result<Vec<_>, _>>()?;
    keys.push(Key::L);
    d.chord(&keys)
}
fn id(value: &str) -> Result<NodeId, String> {
    NodeId::new(value).map_err(|e| e.to_string())
}
fn rev(value: &str) -> Result<RevisionId, String> {
    RevisionId::new(value).map_err(|e| e.to_string())
}

fn fixture(source: &Path, path: &Path) -> Result<(), String> {
    use deadpan_store::{AccessMode, ProjectStore};
    deadpan_store::portable::copy_portable(
        source,
        path,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .map_err(|e| e.to_string())?;
    let mut store = ProjectStore::open(path, AccessMode::ReadWrite).map_err(|e| e.to_string())?;
    let before = store.snapshot().map_err(|e| e.to_string())?;
    let repeat = |name: &str, child: &str, plays: u32| -> Result<BeatNode, String> {
        let mut beat = BeatNode::sequence(name, vec![]);
        beat.kind = NodeKind::Repeat {
            child: id(child)?,
            iterations: IterationOrder::new(rev(name)?, plays).map_err(|e| e.to_string())?,
            gap: None,
            escalation: None,
        };
        Ok(beat)
    };
    let hold = || -> Result<BeatNode, String> {
        Ok(BeatNode::hold(
            "Audio pause",
            HoldRecipe {
                duration: FrameDuration::new(11).map_err(|e| e.to_string())?,
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
                picture_context: None,
            },
        ))
    };
    store
        .commit(&CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: rev("audio-fixture-insert")?,
            command: Command::Insert {
                parent: before.root().clone(),
                index: 0,
                subtree: Subtree {
                    root: id("audio-group")?,
                    nodes: BTreeMap::from([
                        (
                            id("audio-group")?,
                            BeatNode::sequence(
                                "Audio fixture",
                                vec![id("audio-outer")?, id("audio-ordinary")?],
                            ),
                        ),
                        (id("audio-outer")?, repeat("audio-outer", "audio-inner", 3)?),
                        (id("audio-inner")?, repeat("audio-inner", "audio-hold", 2)?),
                        (id("audio-hold")?, hold()?),
                        (id("audio-ordinary")?, hold()?),
                    ]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        })
        .map_err(|e| e.to_string())?;
    let inserted = store.snapshot().map_err(|e| e.to_string())?;
    store
        .commit(&CommandRequest {
            project_id: inserted.project_id().clone(),
            expected_revision: inserted.revision_id().clone(),
            new_revision: rev("audio-fixture-ungroup")?,
            command: Command::Ungroup {
                node: id("audio-group")?,
            },
        })
        .map_err(|e| e.to_string())?;
    store.validate().map_err(|e| e.to_string())
}
