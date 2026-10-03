//! Semantic macros through native event routing, durable registers and Metal.

use super::*;
use crate::preview::copied::Content;
use deadpan_core::{ProjectDocument, RegisterName, RegisterValue, SemanticInstruction};
use deadpan_store::{AccessMode, ProjectStore, registers::RegisterBank};
use egui::{Event, Key, Modifiers};
use egui_kittest::kittest::Queryable as _;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    resize(d, 960.0, 640.0)?;
    let baseline = document(d)?.clone();
    d.check(
        "Macro replay starts from the full 120-frame Original",
        d.app().sequence_length() == 120 && !d.app().workspace.as_ref().unwrap().can_undo,
        json!({"frames":120,"undo":false}),
        state(d),
    )?;
    record_cuts(d, &baseline)?;
    counted_run(d, &baseline)?;
    delayed_navigation(d, &baseline)?;
    motion_and_call(d, &baseline)?;
    type_errors(d)?;
    native_ownership(d)?;
    captured_context(d, &baseline)?;
    cancelled_recording(d)?;
    reopen(d, &baseline)
}

fn record_cuts(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 20)?;
    let initial = d.revision();
    d.key(Key::Q)?;
    d.check(
        "q captures macro intent before the named register arrives",
        d.app().bindings.pending() == "q"
            && d.app().bindings.macro_pending()
            && d.app()
                .macro_prefix_target
                .as_ref()
                .is_some_and(Result::is_ok)
            && !d.app().macros.recording(),
        json!({"pending":"q","recording":false,"captured":true}),
        state(d),
    )?;
    painted(d, "a–z")?;
    d.key_modified(Key::A, Modifiers::SHIFT)?;
    d.check(
        "qA records normalized macro a without saving a revision",
        d.app().macros.recording_name() == Some('a')
            && d.app().macros.instruction_count() == 0
            && d.revision() == initial,
        json!({"recording":"a","instructions":0,"revision":initial}),
        state(d),
    )?;
    recording_paint(d, 'a', 0)?;
    d.capture("Recording a exposes its empty body and save/cancel keys at 960 by 640")?;
    d.events(
        "Record a relative two-frame forward motion",
        keys(&[Key::Num2, Key::L]),
    )?;
    d.settled()?;
    d.check(
        "Motion recording stores intent after moving the live cursor",
        d.app().sequence_cursor == 22 && d.app().macros.instruction_count() == 1,
        json!({"Edit":22,"instructions":1}),
        state(d),
    )?;
    let before = d.revision();
    d.events("Record a three-frame cut", keys(&[Key::Num3, Key::X]))?;
    d.changed(&before)?;
    idle(d)?;
    let after_first = document(d)?.clone();
    d.check(
        "The first recorded cut is performed normally and acknowledged before saving",
        d.app().sequence_length() == 117
            && d.app().sequence_cursor == 22
            && d.app().macros.instruction_count() == 2,
        json!({"frames":117,"Edit":22,"instructions":2}),
        state(d),
    )?;
    d.key(Key::H)?;
    let before = d.revision();
    d.key(Key::X)?;
    d.changed(&before)?;
    idle(d)?;
    recording_paint(d, 'a', 4)?;
    resize(d, 1280.0, 820.0)?;
    recording_paint(d, 'a', 4)?;
    d.capture("Recording a contains two frame motions and two completed cuts")?;
    let before_save = d.revision();
    let before_bank = bank(d)?;
    let before_history = history(d);
    d.key(Key::Q)?;
    wait_saved(d, 'a')?;
    let saved = bank(d)?;
    let name = register('a')?;
    let expected = json!({"instructions":[
        {"type":"move_frames","forward":true,"count":2},
        {"type":"cut_frames","operation":{"count":3},"register":"\""},
        {"type":"move_frames","forward":false,"count":1},
        {"type":"cut_frames","operation":{"count":1},"register":"\""}
    ]});
    d.check(
        "q saves the exact semantic program in SQLite and preserves the unnamed cut and history",
        matches!(saved.entries.get(&name).map(AsRef::as_ref), Some(RegisterValue::Macro { program }) if serde_json::to_value(program).is_ok_and(|value| value == expected))
            && saved.entries.get(&RegisterName::unnamed()) == before_bank.entries.get(&RegisterName::unnamed())
            && saved.version == before_bank.version + 1
            && d.revision() == before_save
            && history(d) == before_history
            && d.app().sequence_length() == 116
            && d.app().sequence_cursor == 21,
        json!({"a":expected,"unnamed_unchanged":true,"timeline_revision":before_save,"frames":116,"Edit":21}),
        json!({"state":state(d),"bank":saved}),
    )?;
    undo(d, &after_first)?;
    undo(d, baseline)?;
    d.check(
        "The recording's ordinary edits undo separately while its saved program remains",
        !d.app().workspace.as_ref().unwrap().can_undo && program(d, 'a').is_some(),
        json!({"frames":120,"undo":false,"macro_a":"retained"}),
        state(d),
    )
}

fn counted_run(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 40)?;
    resize(d, 960.0, 640.0)?;
    let before = d.revision();
    execute(d, 'A', Some(2))?;
    d.changed(&before)?;
    idle(d)?;
    let repeated = document(d)?.clone();
    d.check(
        "2@A resolves every cut against the preceding staged edit and displays the final source frame",
        d.app().sequence_length() == 112
            && d.app().sequence_cursor == 42
            && d.app().presentation.displayed_source_frame() == Some(SourceFrameId(50)),
        json!({"frames":112,"Edit":42,"Original_picture":50,"count":2}),
        state(d),
    )?;
    d.capture("Counted macro a cuts eight frames in one saved edit")?;
    undo(d, baseline)?;
    d.check(
        "One Undo restores all four cuts from the counted macro execution",
        !d.app().workspace.as_ref().unwrap().can_undo,
        json!({"frames":120,"undo":false}),
        state(d),
    )?;
    let before = d.revision();
    d.key_modified(Key::R, Modifiers::CTRL)?;
    d.changed(&before)?;
    d.check(
        "Redo restores the same compound macro edit with a fresh revision",
        same_document(document(d)?, &repeated)? && d.revision() != repeated.revision_id().as_str(),
        json!({"same_authored_edit":true,"fresh_revision":true}),
        state(d),
    )?;
    undo(d, baseline)
}

fn delayed_navigation(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 20)?;
    let revision = d.revision();
    let before_bank = bank(d)?;
    d.app_mut().feedback.hold_project_updates = true;
    execute(d, 'a', None)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    let update = loop {
        if let Some(update) = d.app().service.take_update()
            && update
                .saved_macro
                .as_ref()
                .is_some_and(|receipt| receipt.id.revision.as_str() == revision)
        {
            break update;
        }
        if Instant::now() >= deadline {
            return Err("The genuine deferred macro receipt did not arrive".into());
        }
        d.step(
            "Withhold only delivery while the real macro transaction finishes",
            false,
        )?;
        d.wake
            .wait_until((Instant::now() + Duration::from_millis(16)).min(deadline));
    };
    // Model two pointer navigation observations while the real service reply is
    // withheld. Returning to the entry cursor must not restore receipt ownership.
    d.app_mut().sequence_cursor = 80;
    d.app_mut().reconcile_macro_recording();
    d.app_mut().sequence_cursor = 20;
    d.app_mut().reconcile_macro_recording();
    d.app_mut().feedback.release_project_update = Some(update);
    d.app_mut().feedback.hold_project_updates = false;
    d.step(
        "Release the genuine macro commit after away-and-back navigation",
        false,
    )?;
    idle(d)?;
    let after_bank = bank(d)?;
    d.check(
        "A delayed macro installs its saved document and registers without reclaiming an abandoned cursor",
        d.revision() != revision && d.app().sequence_length() == 116
            && d.app().sequence_cursor == 20
            && after_bank.version > before_bank.version
            && after_bank.entries.get(&register('a')?) == before_bank.entries.get(&register('a')?)
            && after_bank.entries.get(&RegisterName::unnamed()) != before_bank.entries.get(&RegisterName::unnamed()),
        json!({"frames":116,"retained_Edit":20,"macro_result_cursor":21,"saved_bank_installed":true,"injection":"reply timing and pointer navigation observations only"}),
        state(d),
    )?;
    d.capture("Delayed macro commit preserves the independently retained cursor")?;
    undo(d, baseline)
}

fn motion_and_call(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    at(d, 10)?;
    let before = d.revision();
    let history_before = history(d);
    d.command("record b")?;
    d.events(
        "Record motion-only macro b",
        keys(&[Key::Num4, Key::L, Key::H]),
    )?;
    d.command("record-stop")?;
    wait_saved(d, 'b')?;
    d.check(
        "Command aliases save a motion-only macro without changing timeline history or Redo",
        d.revision() == before
            && history(d) == history_before
            && program(d, 'b').is_some_and(|program| program.instructions().len() == 2),
        json!({"revision":before,"history":history_before,"instructions":2}),
        state(d),
    )?;
    at(d, 20)?;
    let before_bank = bank(d)?;
    d.command("macro B 3")?;
    idle(d)?;
    d.check(
        "Three runs of a motion-only macro move the cursor without a revision or register write",
        d.app().sequence_cursor == 29
            && d.revision() == before
            && history(d) == history_before
            && bank(d)? == before_bank,
        json!({"Edit":29,"revision":before,"registers_and_history_unchanged":true}),
        state(d),
    )?;

    at(d, 30)?;
    d.command("record c")?;
    execute(d, 'b', None)?;
    idle(d)?;
    d.check(
        "A completed named call records one Call instruction rather than expanding its input",
        d.app().macros.instruction_count() == 1 && d.app().sequence_cursor == 33,
        json!({"instructions":1,"Edit":33}),
        state(d),
    )?;
    let before = d.revision();
    d.events(
        "Record one cut after the named call",
        keys(&[Key::Num2, Key::X]),
    )?;
    d.changed(&before)?;
    idle(d)?;
    d.command("record-stop")?;
    wait_saved(d, 'c')?;
    d.check(
        "Macro c retains the named call and one frame-cut instruction",
        program(d, 'c').is_some_and(|program| matches!(program.instructions(),
            [SemanticInstruction::Call { register, count }, SemanticInstruction::CutFrames { operation, .. }]
                if register.as_char() == 'b' && count.get() == 1 && operation.count() == 2)),
        json!({"instructions":["call b once","cut two frames"]}),
        state(d),
    )?;
    undo(d, baseline)?;
    at(d, 30)?;
    let before = d.revision();
    execute(d, 'c', Some(2))?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Counted nested calls use one shared staged cursor and one history transaction",
        d.app().sequence_length() == 116 && d.app().sequence_cursor == 36,
        json!({"frames":116,"Edit":36}),
        state(d),
    )?;
    undo(d, baseline)
}

fn type_errors(d: &mut Driver<'_>) -> Result<(), String> {
    at(d, 20)?;
    d.command("register d")?;
    d.key(Key::Y)?;
    d.wait_for("A real copied beat occupies named register d", |app| {
        !app.service.is_busy()
            && !app.copied.is_pending()
            && app
                .copied
                .entries()
                .any(|(name, value)| name == 'd' && matches!(value, Content::Edited(_)))
    })?;
    let expected = document(d)?.clone();
    let before_bank = bank(d)?;
    for name in ['d', 'z'] {
        execute(d, name, None)?;
        idle(d)?;
        d.check(
            "Executing a copied or empty named register rejects without changing the timeline or bank",
            document(d)? == &expected && bank(d)? == before_bank && d.app().error.is_some(),
            json!({"name":name,"unchanged":true,"explicit_error":true}),
            state(d),
        )?;
    }
    d.command("register a")?;
    d.key(Key::P)?;
    idle(d)?;
    d.check(
        "A typed macro cannot paste the unnamed copied beat as a fallback",
        document(d)? == &expected
            && bank(d)? == before_bank
            && d.app().error.as_deref() == Some(crate::preview::copied::MACRO_PASTE_ERROR),
        json!({"typed_macro_paste_refused":true,"unchanged":true}),
        state(d),
    )?;
    d.capture("Macro register type errors preserve the existing edit and copied contents")?;
    d.key(Key::Escape)
}

fn native_ownership(d: &mut Driver<'_>) -> Result<(), String> {
    at(d, 20)?;
    let expected = document(d)?.clone();
    let before_bank = bank(d)?;
    d.harness.get_by_label("Keys  ?").focus();
    d.step(
        "Focus the native Keys control before macro execution",
        false,
    )?;
    d.check(
        "A native control owns focus",
        native_control_focused(&d.harness.ctx),
        json!(true),
        state(d),
    )?;
    execute(d, 'a', None)?;
    d.check(
        "Native control focus prevents the destructive macro's final name from dispatching",
        !d.app().macros.is_pending() && document(d)? == &expected && bank(d)? == before_bank,
        json!({"unchanged":true,"pending":false}),
        state(d),
    )?;
    at(d, 20)?;
    d.key(Key::Colon)?;
    let mut input = at_events();
    input.extend(stroke(Key::A, Modifiers::NONE, Some("a")));
    d.events("Native command text owns the paired @ and its name", input)?;
    d.check(
        "The command field retains literal @a without running the macro",
        d.app().command_open
            && d.app().command == "@a"
            && document(d)? == &expected
            && bank(d)? == before_bank,
        json!({"command":"@a","unchanged":true}),
        state(d),
    )?;
    d.key(Key::Escape)?;
    for ime in [
        egui::ImeEvent::Preedit {
            text: "@a".into(),
            active_range_chars: Some(0..2),
        },
        egui::ImeEvent::Commit("@a".into()),
    ] {
        let mut input = vec![Event::Ime(ime)];
        input.extend(at_events());
        input.extend(keys(&[Key::A]));
        d.events("IME owns the entire macro input batch", input)?;
        d.check(
            "Composition cannot leak macro execution",
            document(d)? == &expected && !d.app().macros.is_pending() && bank(d)? == before_bank,
            json!("no macro execution or write"),
            state(d),
        )?;
    }
    d.key(Key::Escape)
}

fn captured_context(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    d.command("source")?;
    for prefix in [keys(&[Key::Q]), at_events()] {
        d.events(
            "Enter a macro family while Original has no eligible Edit target",
            prefix,
        )?;
        d.check(
            "The family prefix captures unavailable context before a name arrives",
            d.app()
                .macro_prefix_target
                .as_ref()
                .is_some_and(Result::is_err),
            json!("captured unavailable target"),
            state(d),
        )?;
        d.key(Key::A)?;
        d.check(
            "Completing an unavailable macro prefix cannot record or run against the retained Edit cursor",
            !d.app().macros.recording() && !d.app().macros.is_pending()
                && same_document(document(d)?, baseline)? && d.app().error.is_some(),
            json!("no recording or execution"),
            state(d),
        )?;
    }
    at(d, 20)?;
    for finished in [false, true] {
        d.key(Key::V)?;
        if finished {
            d.key(Key::V)?;
        }
        let selection = d.app().edit_range.clone();
        execute(d, 'a', None)?;
        d.check(
            "Active and retained empty Visual selections remain explicit macro refusals",
            !d.app().macros.is_pending()
                && d.app().edit_range == selection
                && same_document(document(d)?, baseline)?,
            json!({"finished":finished,"selection_retained":true,"unchanged":true}),
            state(d),
        )?;
        d.key(Key::Escape)?;
    }
    at(d, 20)?;
    let before_bank = bank(d)?;
    let workspace = d
        .app()
        .workspace
        .clone()
        .ok_or("Macro replay lost its workspace")?;
    let request = crate::project::slice::CaptureRequest {
        id: crate::project::slice::CopyId {
            session: workspace.session,
            project: workspace.document.project_id().clone(),
            source_revision: workspace.document.revision_id().clone(),
            request: d
                .app_mut()
                .next_serial()
                .ok_or("No independent copy request identity")?,
            persisted_version: None,
        },
        register: Some('z'),
        scope: d.app().sequence_scope.clone(),
        parent: d.app().sequence_scope.resolve(&workspace)?.owner.clone(),
        selection: deadpan_core::SliceCaptureSelection::Range {
            range: deadpan_core::FrameRange::new(ProjectFrame(20), ProjectFrame(21))
                .map_err(|error| error.to_string())?,
        },
    };
    d.key(Key::Q)?;
    d.app()
        .service
        .submit(ProjectRequest::CaptureEditSlice(request))?;
    d.wait_for(
        "An independent genuine copy changes the bank after macro prefix capture",
        |app| !app.service.is_busy() && app.copied.bank_version() != Some(before_bank.version),
    )?;
    d.key(Key::A)?;
    d.check(
        "A later register-bank update cannot retarget a previously captured recording prefix",
        !d.app().macros.recording()
            && !d.app().macros.is_pending()
            && program(d, 'a').is_some_and(|program| program.instructions().len() == 4)
            && same_document(document(d)?, baseline)?
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("context changed")),
        json!({"recording":false,"old_macro_unchanged":true,"stale_context_rejected":true}),
        state(d),
    )?;

    // The same entry rule applies to commands while a genuine Undo reply lands.
    d.key(Key::Escape)?;
    let before = d.revision();
    d.key(Key::X)?;
    d.changed(&before)?;
    let before_undo = d.revision();
    let expected_revision = document(d)?.revision_id().clone();
    d.key(Key::Colon)?;
    d.events(
        "Type a macro command before an independent history change",
        vec![Event::Text("macro a".into())],
    )?;
    d.app()
        .service
        .submit(ProjectRequest::Undo { expected_revision })?;
    d.changed(&before_undo)?;
    let restored = document(d)?.clone();
    d.key(Key::Enter)?;
    idle(d)?;
    d.check(
        "Command entry retains the old revision and rejects after a real Undo",
        document(d)? == &restored
            && same_document(document(d)?, baseline)?
            && !d.app().macros.is_pending()
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("context changed")),
        json!("no macro execution after captured revision changed"),
        state(d),
    )?;
    d.capture("Macro prefixes and command entry reject changed context")?;
    d.key(Key::Escape)
}

fn cancelled_recording(d: &mut Driver<'_>) -> Result<(), String> {
    at(d, 20)?;
    let before = bank(d)?;
    for command in [false, true] {
        d.command("record a")?;
        d.key(Key::L)?;
        if command {
            d.command("record-cancel")?;
        } else {
            d.key(Key::Escape)?;
        }
        d.check(
            "Escape and record-cancel abandon the draft without overwriting the saved macro",
            !d.app().macros.recording() && !d.app().macros.is_pending() && bank(d)? == before,
            json!({"command_cancel":command,"bank_unchanged":true}),
            state(d),
        )?;
    }
    Ok(())
}

fn reopen(d: &mut Driver<'_>, baseline: &ProjectDocument) -> Result<(), String> {
    let expected_bank = bank(d)?;
    let path = d.app().workspace.as_ref().unwrap().path.clone();
    let old_session = d.app().workspace.as_ref().unwrap().session;
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for("Close the real macro project", |app| {
        app.workspace.is_none() && !app.service.is_busy()
    })?;
    d.app().service.submit(ProjectRequest::Open(path))?;
    d.wait_for("Reopen durable macros in a fresh project session", |app| {
        !app.service.is_busy()
            && app
                .workspace
                .as_ref()
                .is_some_and(|workspace| workspace.session != old_session)
    })?;
    at(d, 10)?;
    d.check(
        "Reopening restores the complete typed bank and authored baseline without a live recording",
        bank(d)? == expected_bank
            && same_document(document(d)?, baseline)?
            && !d.app().macros.recording()
            && !d.app().macros.is_pending()
            && ['a', 'b', 'c']
                .into_iter()
                .all(|name| program(d, name).is_some()),
        json!({"bank_restored":true,"recording":false,"frames":120}),
        state(d),
    )?;
    let before = d.revision();
    d.command("macro a")?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "A reopened macro runs from the new current cursor",
        d.app().sequence_length() == 116 && d.app().sequence_cursor == 11,
        json!({"frames":116,"Edit":11}),
        state(d),
    )?;
    undo(d, baseline)?;
    d.command("registers")?;
    painted(d, "Macro · 4 instructions")?;
    d.capture("Reopened register inventory retains Macro types and instruction counts")?;
    d.key(Key::Escape)
}

fn idle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for(
        "Macro service action and recording acknowledgment complete",
        |app| !app.service.is_busy() && !app.macros.is_pending() && !app.copied.is_pending(),
    )?;
    d.settled()
}

fn wait_saved(d: &mut Driver<'_>, name: char) -> Result<(), String> {
    d.wait_for(
        "The named semantic macro is saved and reflected in the typed bank",
        |app| {
            !app.service.is_busy()
                && !app.macros.recording()
                && !app.macros.is_pending()
                && app
                    .copied
                    .entries()
                    .any(|(slot, value)| slot == name && matches!(value, Content::Macro(_)))
        },
    )?;
    d.settled()
}

fn at(d: &mut Driver<'_>, cursor: u64) -> Result<(), String> {
    d.command("sequence")?;
    d.key(Key::Escape)?;
    for _ in 0..6 {
        if d.app().pane == Pane::Sequence
            && d.harness
                .ctx
                .memory(|memory| memory.has_focus(pane_id(Pane::Sequence)))
        {
            break;
        }
        d.key(Key::Tab)?;
    }
    let mut input = keys(&[Key::G, Key::G]);
    if cursor > 0 {
        input.extend(count_keys(cursor)?);
        input.extend(keys(&[Key::L]));
    }
    d.events(
        "Navigate to an explicit Edit cursor before macro input",
        input,
    )?;
    d.settled()?;
    d.check(
        "Macro replay reaches the requested Edit cursor",
        d.app().sequence_cursor == cursor && d.app().pane == Pane::Sequence,
        json!(cursor),
        state(d),
    )
}

fn execute(d: &mut Driver<'_>, name: char, count: Option<u32>) -> Result<(), String> {
    let mut events = count.map_or(Ok(Vec::new()), |count| count_keys(u64::from(count)))?;
    events.extend(at_events());
    let key = Key::from_name(&name.to_string()).ok_or("Invalid macro replay register key")?;
    events.extend(stroke(
        key,
        if name.is_ascii_uppercase() {
            Modifiers::SHIFT
        } else {
            Modifiers::NONE
        },
        Some(&name.to_string()),
    ));
    d.events(
        "Execute the named macro through paired native @ text",
        events,
    )
}

fn at_events() -> Vec<Event> {
    stroke(Key::Num2, Modifiers::SHIFT, Some("@"))
}

fn stroke(key: Key, modifiers: Modifiers, text: Option<&str>) -> Vec<Event> {
    let mut events = vec![Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    }];
    if let Some(text) = text {
        events.push(Event::Text(text.into()));
    }
    events.push(Event::Key {
        key,
        physical_key: Some(key),
        pressed: false,
        repeat: false,
        modifiers,
    });
    events
}

fn keys(keys: &[Key]) -> Vec<Event> {
    keys.iter()
        .flat_map(|key| stroke(*key, Modifiers::NONE, None))
        .collect()
}

fn count_keys(count: u64) -> Result<Vec<Event>, String> {
    count
        .to_string()
        .chars()
        .map(|digit| {
            Key::from_name(&digit.to_string()).ok_or_else(|| "Invalid count digit".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|digits| keys(&digits))
}

fn document<'a>(d: &'a Driver<'_>) -> Result<&'a ProjectDocument, String> {
    d.app()
        .workspace
        .as_ref()
        .map(|workspace| workspace.document.as_ref())
        .ok_or_else(|| "Macro replay has no workspace".into())
}

fn history(d: &Driver<'_>) -> (bool, bool) {
    let workspace = d.app().workspace.as_ref().expect("macro fixture workspace");
    (workspace.can_undo, workspace.can_redo)
}

fn bank(d: &Driver<'_>) -> Result<RegisterBank, String> {
    ProjectStore::open(
        &d.app()
            .workspace
            .as_ref()
            .ok_or("Missing macro workspace")?
            .path,
        AccessMode::ReadOnly,
    )
    .and_then(|store| store.registers())
    .map_err(|error| error.to_string())
}

fn register(name: char) -> Result<RegisterName, String> {
    RegisterName::new(name).map_err(|error| error.to_string())
}

fn program<'a>(d: &'a Driver<'_>, name: char) -> Option<&'a deadpan_core::SemanticProgram> {
    d.app()
        .copied
        .entries()
        .find_map(|(slot, value)| match value {
            Content::Macro(program) if slot == name => Some(program.as_ref()),
            _ => None,
        })
}

fn same_document(actual: &ProjectDocument, expected: &ProjectDocument) -> Result<bool, String> {
    let mut actual = serde_json::to_value(actual).map_err(|error| error.to_string())?;
    actual["revision_id"] = json!(expected.revision_id());
    Ok(actual == serde_json::to_value(expected).map_err(|error| error.to_string())?)
}

fn undo(d: &mut Driver<'_>, expected: &ProjectDocument) -> Result<(), String> {
    let before = d.revision();
    d.key(Key::U)?;
    d.changed(&before)?;
    d.check(
        "One Undo restores the complete expected authored document",
        same_document(document(d)?, expected)?,
        json!(expected.revision_id()),
        state(d),
    )
}

fn resize(d: &mut Driver<'_>, width: f32, height: f32) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing macro viewport")?
        .inner_rect = Some(rect);
    d.step("Paint the resized macro workspace", false)?;
    d.settled()
}

fn recording_paint(d: &mut Driver<'_>, name: char, count: usize) -> Result<(), String> {
    painted(d, &format!("RECORDING @{name} · {count} instructions"))?;
    painted(d, "save macro")?;
    painted(d, "cancel recording")?;
    scenarios::footer_anchored(d, "Macro recording footer stays above the notice panel")
}

fn painted(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let paint = scenarios::text_paint_visibility(d, label);
    d.check(
        "Macro status and controls are fully painted inside their clips",
        !paint.is_empty() && paint.iter().all(|part| part["fully_visible"] == true),
        json!(label),
        json!(paint),
    )
}

fn state(d: &Driver<'_>) -> Value {
    let mut snapshot = d.snapshot();
    snapshot["macros"] = json!({"recording":d.app().macros.recording_name(),"instructions":d.app().macros.instruction_count(),"pending":d.app().macros.is_pending()});
    snapshot["registers"] = json!(
        d.app()
            .copied
            .entries()
            .map(|(name, content)| (name.to_string(), content.label()))
            .collect::<std::collections::BTreeMap<_, _>>()
    );
    snapshot
}
