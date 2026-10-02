//! Cursor cuts use the production router, exact historical capture and real pictures.

use super::*;
use crate::preview::copied::Content;
use crate::project::ProjectUpdate;
use crate::project::slice::Captured;
use deadpan_core::{Command, CommandRequest, RevisionId};
use deadpan_store::{AccessMode, ProjectStore};
use egui::{Event, Modifiers};
use egui_kittest::kittest::Queryable as _;
use std::sync::Arc;

const BEATS: &str = "Current group beat outline pane";

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 640.0));
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing frame-cut viewport")?
        .inner_rect = Some(rect);
    d.step("Paint frame-cut controls at the minimum viewport", false)?;
    interior(d)?;
    refusals(d)?;
    captured(d)?;
    clipped_group(d)?;
    composite_endpoint(d)?;
    focused_mark_name(d)
}

fn interior(d: &mut Driver<'_>) -> Result<(), String> {
    let baseline = document(d)?.clone();
    for route in ["x", "12x", "delete-frames 12f"] {
        at(d, 20)?;
        let revision = d.revision();
        let count = if route == "x" { 1 } else { 12 };
        match route {
            "x" => d.events(
                "Press x and keep it held through the saved cut",
                vec![key_event(Key::X, Modifiers::NONE, true)],
            )?,
            "12x" => d.events(
                "Count and cut in one native batch",
                keys(&[Key::Num1, Key::Num2, Key::X]),
            )?,
            _ => {
                d.key(Key::Colon)?;
                d.events("Type captured frame cut", vec![Event::Text(route.into())])?;
                paint(d, "Cut captured Edit [20..32) · 12 f")?;
                d.key(Key::Enter)?;
            }
        }
        d.changed(&revision)?;
        if route == "x" {
            let saved = document(d)?.clone();
            d.events(
                "Repeat held x after the first cut has saved and the writer is idle",
                vec![Event::Key {
                    key: Key::X,
                    physical_key: None,
                    pressed: true,
                    repeat: true,
                    modifiers: Modifiers::NONE,
                }],
            )?;
            d.events(
                "Release held x",
                vec![key_event(Key::X, Modifiers::NONE, false)],
            )?;
            d.settled()?;
            d.check(
                "A held x never cuts again after the writer becomes idle",
                *document(d)? == saved,
                json!("one saved cut"),
                d.snapshot(),
            )?;
        }
        let copied = edited(d)?;
        let saved_message = format!(
            "Cut saved and copied: Edit [20..{}) · {count} f.",
            20 + count
        );
        d.check(
            "Single, counted and command frame cuts save the exact cursor interval once",
            d.app().sequence_cursor == 20
                && d.app().sequence_length() == 120 - count as u64
                && copied.slice().range() == range(20, 20 + count)
                && copied.slice().revision_id().as_str() == revision
                && copied.slice().duration().frames() == count
                && !d.app().copied.is_pending()
                && d.app()
                    .message
                    .as_deref()
                    .is_some_and(|text| text.starts_with(&saved_message)),
            json!({"route":route,"range":[20,20+count],"source_revision":revision}),
            d.snapshot(),
        )?;
        picture(d, (20 + count) as u64)?;
        if route == "12x" {
            d.key(Key::H)?;
            picture(d, 19)?;
            d.key(Key::L)?;
            picture(d, 32)?;
            d.capture("Counted frame cut retains the exact right join and historical copy")?;
        }
        undo(d, &baseline)?;
        d.check(
            "One Undo restores the protected baseline after each frame-cut route",
            !d.app().workspace.as_ref().unwrap().can_undo && same_copy(d, &copied),
            json!({"route":route,"one_undo":true,"historical_copy_retained":true}),
            d.snapshot(),
        )?;
    }
    at(d, 20)?;
    paint(d, "cut frame")
}

fn refusals(d: &mut Driver<'_>) -> Result<(), String> {
    let baseline = document(d)?.clone();
    let accepted = edited(d)?;
    for finished in [false, true] {
        for end in [20, 30] {
            select(d, 20, end, finished)?;
            let selection = d.app().edit_range.clone();
            for command in [false, true] {
                if command {
                    d.command("delete-frames 12f")?;
                } else {
                    d.key(Key::X)?;
                }
                d.settled()?;
                d.check(
                    "Frame cuts refuse active and finished Visual selections, including empty ones",
                    *document(d)? == baseline
                        && d.app().edit_range == selection
                        && same_copy(d, &accepted)
                        && !d.app().copied.is_pending()
                        && d.app().error.as_deref().is_some_and(|error| error.contains("Use d")),
                    json!({"finished":finished,"range":[20,end],"command":command,"unchanged":true}),
                    d.snapshot(),
                )?;
            }
        }
    }

    // Hold an actual yank reply; a refused newer x must revoke its register intent.
    select(d, 40, 50, false)?;
    let selection = d.app().edit_range.clone();
    d.app_mut().feedback.hold_project_updates = true;
    d.key(Key::Y)?;
    let held = take_yank_update(d)?;
    let capture = held
        .captured_slice
        .as_ref()
        .ok_or("Missing held yank reply")?;
    let saved_copy = capture.result.as_ref().map_err(Clone::clone)?.clone();
    d.check(
        "The held real yank succeeded for the exact source revision and selected interval",
        capture.id.source_revision == *baseline.revision_id()
            && capture.result.as_ref().is_ok_and(|copied| {
                copied.id() == &capture.id
                    && copied.slice().revision_id() == baseline.revision_id()
                    && copied.slice().range() == range(40, 50)
                    && copied.slice().duration().frames() == 10
            }),
        json!({"source_revision":baseline.revision_id(),"range":[40,50],"frames":10,"successful":true}),
        json!({"id":format!("{:?}", capture.id),"result":capture.result.as_ref().map(|copied| json!({"source_revision":copied.slice().revision_id(),"range":copied.slice().range(),"frames":copied.slice().duration().frames()}))}),
    )?;
    d.check(
        "The withheld yank owns a pending register intent",
        d.app().copied.is_pending(),
        json!(true),
        d.snapshot(),
    )?;
    d.key(Key::X)?;
    d.check(
        "Refused Visual x supersedes confirmation while the accepted write remains unsettled",
        d.app().copied.is_pending() && same_copy(d, &accepted),
        json!("visible old bank retained until durable write acknowledgment"),
        d.snapshot(),
    )?;
    d.app_mut().feedback.release_project_update = Some(held);
    d.app_mut().feedback.hold_project_updates = false;
    d.step("Deliver the real superseded yank reply", false)?;
    d.settled()?;
    d.check(
        "The saved yank installs its authoritative bank without consuming selection after refused x",
        *document(d)? == baseline && d.app().edit_range == selection && same_copy(d, &saved_copy)
            && !d.app().copied.is_pending(),
        json!("unchanged document and selection; exact saved copy installed"),
        d.snapshot(),
    )?;
    let accepted = saved_copy;

    at(d, 20)?;
    for modifiers in [
        Modifiers::SHIFT,
        Modifiers::ALT,
        Modifiers::CTRL,
        Modifiers::COMMAND,
    ] {
        d.key_modified(Key::X, modifiers)?;
    }
    d.harness.get_by_label("Keys  ?").focus();
    d.step("Focus native Keys button before x", false)?;
    d.check(
        "Native button owns frame-cut input focus",
        native_control_focused(&d.harness.ctx),
        json!(true),
        d.snapshot(),
    )?;
    d.key(Key::X)?;
    unchanged(
        d,
        &baseline,
        &accepted,
        "Modified x and focused native controls cannot cut picture time",
    )?;
    at(d, 20)?;
    for (ime, composing) in [
        (
            egui::ImeEvent::Preedit {
                text: "x".into(),
                active_range_chars: Some(0..1),
            },
            true,
        ),
        (egui::ImeEvent::Commit("x".into()), false),
    ] {
        d.events(
            "IME owns x for its entire native batch",
            [vec![Event::Ime(ime)], keys(&[Key::X])].concat(),
        )?;
        d.check(
            "Frame-cut input follows IME composition ownership",
            d.app().ime_composing == composing,
            json!(composing),
            d.snapshot(),
        )?;
        unchanged(
            d,
            &baseline,
            &accepted,
            "IME preedit and commit cannot leak a frame cut",
        )?;
    }
    d.key(Key::Colon)?;
    d.events(
        "Native command text owns x",
        [vec![Event::Text("x".into())], keys(&[Key::X])].concat(),
    )?;
    d.check(
        "Typed x remains command text",
        d.app().command_open && d.app().command == "x",
        json!("x"),
        d.snapshot(),
    )?;
    unchanged(
        d,
        &baseline,
        &accepted,
        "Typing x does not cut through the command field",
    )?;
    d.key(Key::Escape)?;

    for count in ["0", "4294967296"] {
        for digit in count.bytes() {
            d.key(digit_key(digit))?;
        }
        d.key(Key::X)?;
        unchanged(
            d,
            &baseline,
            &accepted,
            "Zero and overflowing keyboard counts cannot cut",
        )?;
        d.command(&format!("delete-frames {count}f"))?;
        unchanged(
            d,
            &baseline,
            &accepted,
            "Zero and overflowing command counts cannot cut",
        )?;
    }
    d.key_modified(Key::G, Modifiers::SHIFT)?;
    d.key(Key::X)?;
    d.command("delete-frames 12f")?;
    d.check(
        "The terminal cursor refuses a frame cut without falling back to its selected beat",
        d.app().sequence_cursor == 120
            && d.app()
                .error
                .as_deref()
                .is_some_and(|error| error.contains("group's end")),
        json!({"cursor":120,"terminal_refused":true}),
        d.snapshot(),
    )?;
    unchanged(
        d,
        &baseline,
        &accepted,
        "Terminal cuts preserve the document and accepted copy",
    )?;

    at(d, 20)?;
    d.command("source")?;
    d.key(Key::X)?;
    d.command("delete-frames 12f")?;
    unchanged(
        d,
        &baseline,
        &accepted,
        "Original cannot cut at the retained Edit cursor",
    )?;
    d.command("sequence")?;
    d.click("Original and sounds pane")?;
    d.key(Key::X)?;
    d.command("delete-frames 12f")?;
    d.check(
        "Catalog focus retains the Sequence view",
        d.app().pane == Pane::Sources && d.app().view == View::Sequence,
        json!("Sequence view; Sources focus"),
        d.snapshot(),
    )?;
    unchanged(
        d,
        &baseline,
        &accepted,
        "Catalog focus cannot cut through a retained Sequence view",
    )?;
    d.command("sounds")?;
    d.key(Key::X)?;
    d.command("delete-frames 12f")?;
    unchanged(
        d,
        &baseline,
        &accepted,
        "Placed sounds focus cannot cut picture time",
    )?;

    at(d, 20)?;
    d.events(
        "Open help before x in one native batch",
        keys(&[Key::Questionmark, Key::X]),
    )?;
    d.check(
        "Help owns the remaining batch",
        d.app().help_open,
        json!(true),
        d.snapshot(),
    )?;
    unchanged(
        d,
        &baseline,
        &accepted,
        "Same-batch help prevents the trailing x cut",
    )?;
    d.key(Key::Escape)?;
    d.click(BEATS)?;
    d.app_mut().dialogs = Dialogs::scripted(vec![(DialogKind::OpenProject, None)]);
    d.events(
        "Open project picker before x in one native batch",
        vec![
            key_event(Key::O, Modifiers::COMMAND, true),
            key_event(Key::O, Modifiers::COMMAND, false),
            key_event(Key::X, Modifiers::NONE, true),
            key_event(Key::X, Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "Project picker owns the same-batch suffix",
        d.app().dialogs.is_open(),
        json!(true),
        d.snapshot(),
    )?;
    unchanged(
        d,
        &baseline,
        &accepted,
        "A pending project picker prevents same-batch x",
    )?;
    d.step("Consume scripted picker cancellation", false)?;
    at(d, 20)
}

fn captured(d: &mut Driver<'_>) -> Result<(), String> {
    let accepted = edited(d)?;
    let baseline = document(d)?.clone();
    let revision = d.revision();
    d.key(Key::S)?;
    d.changed(&revision)?;
    d.key(Key::Colon)?;
    d.events(
        "Type frame cut against its captured revision",
        vec![Event::Text("delete-frames 12f".into())],
    )?;
    let revision = document(d)?.revision_id().clone();
    d.app().service.submit(ProjectRequest::Undo {
        expected_revision: revision.clone(),
    })?;
    d.wait_for(
        "Concurrent real Undo invalidates the captured frame cut",
        |app| {
            !app.service.is_busy()
                && app
                    .workspace
                    .as_ref()
                    .is_some_and(|workspace| workspace.document.revision_id() != &revision)
        },
    )?;
    let saved = document(d)?.clone();
    d.key(Key::Enter)?;
    unchanged(
        d,
        &saved,
        &accepted,
        "A captured frame cut refuses a fresh revision instead of retargeting",
    )?;
    d.check(
        "Stale frame-cut entry reports its captured-target failure",
        d.app()
            .error
            .as_deref()
            .is_some_and(|error| error.contains("captured deletion target changed")),
        json!("captured deletion target changed"),
        d.snapshot(),
    )?;

    at(d, 20)?;
    let revision = d.revision();
    d.app_mut().feedback.hold_project_updates = true;
    d.key(Key::S)?;
    d.wait_for(
        "Real split saves before its held frame-cut context reply",
        |app| !app.service.is_busy(),
    )?;
    d.command("source")?;
    d.key(Key::Colon)?;
    d.events(
        "Type frame cut while Original supplies no eligible target",
        vec![Event::Text("delete-frames 12f".into())],
    )?;
    paint(
        d,
        "Return to Your edit and focus Beats before cutting frames.",
    )?;
    d.app_mut().feedback.hold_project_updates = false;
    d.changed(&revision)?;
    let saved = document(d)?.clone();
    d.check(
        "Late split returns Your edit while frame-cut command remains open",
        d.app().view == View::Sequence && d.app().command_open,
        json!("Sequence with open command"),
        d.snapshot(),
    )?;
    d.key(Key::Enter)?;
    unchanged(
        d,
        &saved,
        &accepted,
        "Late split cannot supply a missing command-entry frame target",
    )?;
    d.check(
        "Captured Original absence survives a late Edit completion",
        d.app()
            .error
            .as_deref()
            .is_some_and(|error| error.contains("Return to Your edit")),
        json!("Original absence retained"),
        d.snapshot(),
    )?;
    undo(d, &baseline)
}

fn clipped_group(d: &mut Driver<'_>) -> Result<(), String> {
    let baseline = document(d)?.clone();
    for cursor in [20, 80] {
        at(d, cursor)?;
        let revision = d.revision();
        d.key(Key::S)?;
        d.changed(&revision)?;
    }
    let split = document(d)?.clone();
    let path = d.app().workspace.as_ref().unwrap().path.clone();
    let group = NodeId::new("frame-cut-middle").map_err(|error| error.to_string())?;
    d.app().service.submit(ProjectRequest::Close)?;
    d.wait_for(
        "Release writer for ordinary frame-cut group fixture",
        |app| app.workspace.is_none() && !app.service.is_busy(),
    )?;
    let mut store =
        ProjectStore::open(&path, AccessMode::ReadWrite).map_err(|error| error.to_string())?;
    let doc = store.snapshot().map_err(|error| error.to_string())?;
    store
        .commit(&CommandRequest {
            project_id: doc.project_id().clone(),
            expected_revision: doc.revision_id().clone(),
            new_revision: RevisionId::new("frame-cut-middle-fixture")
                .map_err(|error| error.to_string())?,
            command: Command::Group {
                parent: doc.root().clone(),
                start: 1,
                end: 2,
                id: group.clone(),
                label: "Frame-cut middle group".into(),
            },
        })
        .map_err(|error| error.to_string())?;
    drop(store);
    d.app().service.submit(ProjectRequest::Open(path))?;
    d.wait_for("Open ordinary frame-cut group fixture", |app| {
        app.workspace.is_some() && !app.service.is_busy()
    })?;
    at(d, 20)?;
    d.key(Key::Enter)?;
    motion(d, 55, true)?;
    let grouped = document(d)?.clone();
    let scope = d.app().sequence_scope.clone();
    d.check(
        "Counted frame cut begins five frames before the displayed group's end",
        scope.groups() == [group]
            && d.app().scope_start == 20
            && d.app().scope_end == 80
            && d.app().sequence_cursor == 75,
        json!({"group":[20,80],"cursor":75}),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.chord(&[Key::Num1, Key::Num2, Key::X])?;
    d.changed(&revision)?;
    let copied = edited(d)?;
    d.check(
        "12x clips to the displayed group and reports the actual five saved frames",
        d.app().sequence_scope == scope
            && d.app().scope_start == 20
            && d.app().scope_end == 75
            && d.app().sequence_length() == 115
            && d.app().sequence_cursor == 75
            && copied.slice().range() == range(75, 80)
            && copied.slice().duration().frames() == 5
            && copied.slice().revision_id().as_str() == revision
            && d.app()
                .message
                .as_deref()
                .is_some_and(|text| text.starts_with("Cut saved and copied: Edit [75..80) · 5 f.")),
        json!({"cut":[75,80],"frames":115,"scope_end":75,"cursor":75}),
        d.snapshot(),
    )?;
    d.key(Key::Backspace)?;
    at(d, 75)?;
    picture(d, 80)?;
    undo(d, &grouped)?;
    undo(d, &split)?;
    let revision = d.revision();
    d.key(Key::U)?;
    d.changed(&revision)?;
    undo(d, &baseline)
}

fn composite_endpoint(d: &mut Driver<'_>) -> Result<(), String> {
    let baseline = document(d)?.clone();
    at(d, 20)?;
    let revision = d.revision();
    d.key(Key::S)?;
    d.changed(&revision)?;
    let split = document(d)?.clone();
    for command in ["wrap-repeat 2", "wrap-retime 0.5 pitch=preserve"] {
        at(d, 20)?;
        let revision = d.revision();
        d.command(command)?;
        d.changed(&revision)?;
        let saved = document(d)?.clone();
        let accepted = edited(d)?;
        at(d, 15)?;
        d.chord(&[Key::Num1, Key::Num2, Key::X])?;
        d.wait_for(
            "Composite endpoint frame cut produces a typed refusal",
            |app| !app.service.is_busy() && !app.copied.is_pending() && app.error.is_some(),
        )?;
        unchanged(
            d,
            &saved,
            &accepted,
            "A frame cut crossing into Repeat or Retime refuses the whole requested range",
        )?;
        d.check(
            "Unsupported interior does not truncate the cut to an eligible seam",
            d.app().sequence_cursor == 15
                && d.app()
                    .error
                    .as_deref()
                    .is_some_and(|error| error.contains("Range endpoints")),
            json!({"wrapper":command,"requested":[15,27],"no_cut_at_seam":20}),
            d.snapshot(),
        )?;
        undo(d, &split)?;
    }
    undo(d, &baseline)
}

fn focused_mark_name(d: &mut Driver<'_>) -> Result<(), String> {
    let baseline = document(d)?.clone();
    let accepted = edited(d)?;
    at(d, 20)?;
    let original_cursor = d.app().source_cursor;
    d.harness.get_by_label("Keys  ?").focus();
    d.step("Focus native Keys button before setting mark x", false)?;
    d.check(
        "Native button owns focus before the mx mark prefix",
        native_control_focused(&d.harness.ctx),
        json!(true),
        d.snapshot(),
    )?;
    let revision = d.revision();
    d.key(Key::M)?;
    d.check(
        "Focused native control preserves the pending m prefix",
        d.app().bindings.pending() == "m" && native_control_focused(&d.harness.ctx),
        json!("m pending with native control focus"),
        d.snapshot(),
    )?;
    d.key(Key::X)?;
    d.check(
        "Focused native control routes x as the pending mark name",
        d.app().marks.is_pending() || d.revision() != revision,
        json!("mark save submitted"),
        d.snapshot(),
    )?;
    d.changed(&revision)?;
    let marked = document(d)?.clone();
    let id = crate::project::marks::mark_id('x')?;
    let mark = marked
        .marks()
        .get(&id)
        .ok_or("Focused mx did not save native-mark-x")?;
    let mut without_mark = serde_json::to_value(&marked).map_err(|error| error.to_string())?;
    without_mark["revision_id"] = json!(baseline.revision_id());
    without_mark["marks"] = json!(baseline.marks());
    d.check(
        "mx from native button focus saves only the exact Edit mark without cutting",
        mark.label == "x"
            && matches!(&mark.boundary.coordinate, deadpan_core::Anchor::Occurrence { position, .. }
                if *position == deadpan_core::ExactRatio::integer(20))
            && marked.marks().len() == baseline.marks().len() + 1
            && without_mark == serde_json::to_value(&baseline).map_err(|error| error.to_string())?
            && d.app().sequence_cursor == 20
            && d.app().source_cursor == original_cursor
            && same_copy(d, &accepted)
            && !d.app().copied.is_pending(),
        json!({"mark":"native-mark-x","Edit":20,"Original":original_cursor,"only_mark_changed":true}),
        d.snapshot(),
    )?;

    at(d, 30)?;
    d.harness.get_by_label("Keys  ?").focus();
    d.step("Focus native Keys button before jumping to mark x", false)?;
    d.key(Key::Quote)?;
    d.check(
        "Focused native control preserves the pending mark-jump prefix",
        d.app().bindings.mark_prefix().is_some() && native_control_focused(&d.harness.ctx),
        json!("mark jump pending with native control focus"),
        d.snapshot(),
    )?;
    d.key(Key::X)?;
    d.check(
        "Focused native control routes x as the pending jump name",
        d.app().marks.is_pending() || d.app().sequence_cursor == 20,
        json!("mark jump submitted"),
        d.snapshot(),
    )?;
    d.wait_for("Real mark x resolves back to Edit 20", |app| {
        app.sequence_cursor == 20 && !app.marks.is_pending() && !app.service.is_busy()
    })?;
    d.settled()?;
    d.check(
        "Quoted x from native button focus jumps without an edit or register change",
        *document(d)? == marked
            && same_copy(d, &accepted)
            && d.app().source_cursor == original_cursor,
        json!({"Edit":20,"same_revision":marked.revision_id(),"same_copy":true}),
        d.snapshot(),
    )?;
    undo(d, &baseline)?;
    d.check(
        "One Undo removes the focused-button mark and preserves the historical cut copy",
        !d.app().workspace.as_ref().unwrap().can_undo && same_copy(d, &accepted),
        json!("protected baseline; accepted copy retained"),
        d.snapshot(),
    )
}

fn at(d: &mut Driver<'_>, cursor: u64) -> Result<(), String> {
    d.command("sequence")?;
    d.click(BEATS)?;
    d.key(Key::Escape)?;
    d.chord(&[Key::G, Key::G])?;
    motion(d, cursor - d.app().scope_start, true)?;
    d.settled()
}

fn keys(keys: &[Key]) -> Vec<Event> {
    keys.iter()
        .flat_map(|key| {
            [
                key_event(*key, Modifiers::NONE, true),
                key_event(*key, Modifiers::NONE, false),
            ]
        })
        .collect()
}

fn digit_key(digit: u8) -> Key {
    [
        Key::Num0,
        Key::Num1,
        Key::Num2,
        Key::Num3,
        Key::Num4,
        Key::Num5,
        Key::Num6,
        Key::Num7,
        Key::Num8,
        Key::Num9,
    ][usize::from(digit - b'0')]
}

fn take_yank_update(d: &mut Driver<'_>) -> Result<ProjectUpdate, String> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(update) = d.app().service.take_update()
            && update.captured_slice.is_some()
        {
            return Ok(update);
        }
        if Instant::now() >= deadline {
            return Err("The genuine frame-cut yank reply did not arrive".into());
        }
        d.step("Retain the real yank reply before refused x", false)?;
        d.wake
            .wait_until((Instant::now() + Duration::from_millis(16)).min(deadline));
    }
}

fn edited(d: &Driver<'_>) -> Result<Arc<Captured>, String> {
    match d.app().copied.content() {
        Some(Content::Edited(copied)) => Ok(copied.clone()),
        _ => Err("Missing historical frame-cut register".into()),
    }
}

fn same_copy(d: &Driver<'_>, accepted: &Arc<Captured>) -> bool {
    matches!(d.app().copied.content(), Some(Content::Edited(current)) if Arc::ptr_eq(current, accepted))
}

fn unchanged(
    d: &mut Driver<'_>,
    expected: &ProjectDocument,
    accepted: &Arc<Captured>,
    label: &str,
) -> Result<(), String> {
    d.settled()?;
    d.check(
        label,
        *document(d)? == *expected && same_copy(d, accepted) && !d.app().copied.is_pending(),
        json!("unchanged authored snapshot and accepted register"),
        d.snapshot(),
    )
}

fn paint(d: &mut Driver<'_>, label: &str) -> Result<(), String> {
    let paint = scenarios::text_paint_visibility(d, label);
    d.check(
        "Frame-cut guidance is fully painted at the minimum viewport",
        !paint.is_empty() && paint.iter().all(|part| part["fully_visible"] == true),
        json!({"label":label,"viewport":[960,640]}),
        json!(paint),
    )
}
