//! Named marks retain unresolved intent through native selection, copy and repeat.
use super::*;
use deadpan_core::{
    MarkId, NamedMarkTarget, RegisterName, RegisterValue, SemanticInstruction, SemanticSelector,
};
use deadpan_store::registers::RegisterBank;

const SELECT: &str = "select marks start=native-mark-a end=native-mark-b";

fn idle(d: &mut Driver<'_>) -> Result<(), String> {
    d.wait_for("Named-mark service and bank receipts complete", |app| {
        !app.service.is_busy() && !app.macros.is_pending() && !app.copied.is_pending()
    })?;
    d.settled()
}
fn bank(d: &Driver<'_>) -> Result<RegisterBank, String> {
    ProjectStore::open(
        &d.app()
            .workspace
            .as_ref()
            .ok_or("Missing mark workspace")?
            .path,
        AccessMode::ReadOnly,
    )
    .and_then(|store| store.registers())
    .map_err(|error| error.to_string())
}
fn selector() -> SemanticSelector {
    SemanticSelector::MarkRange {
        start: NamedMarkTarget {
            id: MarkId::new("native-mark-a").unwrap(),
            occurrence: None,
        },
        end: NamedMarkTarget {
            id: MarkId::new("native-mark-b").unwrap(),
            occurrence: None,
        },
    }
}
fn mark(d: &mut Driver<'_>, at_frame: u64, letter: &str) -> Result<(), String> {
    at(d, at_frame)?;
    let before = d.revision();
    d.command(&format!("mark {letter}"))?;
    d.changed(&before)?;
    idle(d)
}
fn selected(d: &mut Driver<'_>, start: i64, end: i64) -> Result<(), String> {
    let before = document(d)?.clone();
    let registers = bank(d)?;
    d.command(SELECT)?;
    idle(d)?;
    d.check("Named selection keeps exact identities and half-open Edit geometry without a history or bank write",
        document(d)?==&before && bank(d)?==registers
        && d.app().selected_edit_range()==Some(FrameRange::new(ProjectFrame(start),ProjectFrame(end)).map_err(|e|e.to_string())?)
        && matches!(d.app().capture_visual_selection()?,Some(deadpan_core::SemanticVisualSelection::MarkRange{..}))
        && d.app().sequence_cursor==end as u64,
        json!({"marks":["native-mark-a","native-mark-b"],"range":[start,end],"history_write":false}),state(d))
}
pub(in crate::preview::harness) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 640.0));
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing named-mark viewport")?
        .inner_rect = Some(rect);
    d.step("Paint named-mark selectors at the minimum workspace", false)?;
    let baseline = document(d)?.clone();
    mark(d, 10, "a")?;
    let first_mark = document(d)?.clone();
    mark(d, 20, "b")?;
    let marked = document(d)?.clone();
    selected(d, 10, 20)?;
    d.capture("Named marks show both identities and exact Edit range before an operator")?;
    let before = d.revision();
    d.command("unmark a")?;
    d.changed(&before)?;
    idle(d)?;
    let missing = document(d)?.clone();
    let missing_bank = bank(d)?;
    let missing_selection = d.app().edit_range.semantic()?;
    d.chord(&[Key::Comma, Key::E])?;
    idle(d)?;
    d.check(
        "An unresolved named selection refuses escalating Repeat without falling back to a beat",
        document(d)? == &missing
            && bank(d)? == missing_bank
            && d.app().edit_range.semantic()? == missing_selection
            && d.app().error.is_some(),
        json!({"missing_mark":"native-mark-a","no_edit":true,"selection_retained":true}),
        state(d),
    )?;
    undo(d, &marked)?;
    idle(d)?;
    selected(d, 10, 20)?;
    d.command("record u")?;
    d.command(SELECT)?;
    d.key(Key::Y)?;
    idle(d)?;
    let before = d.revision();
    d.command("wrap-repeat 3")?;
    d.changed(&before)?;
    idle(d)?;
    d.key(Key::Q)?;
    idle(d)?;
    let saved = bank(d)?;
    let slot = RegisterName::new('u').unwrap();
    d.check("Recording saves mark identities in selection, Yank and Repeat rather than resolved timestamps",
        d.app().sequence_length()==140
        && matches!(saved.entries.get(&slot).map(AsRef::as_ref),Some(RegisterValue::Macro{program})
            if matches!(program.instructions(),[
                SemanticInstruction::SelectMarkRange{..},
                SemanticInstruction::Yank{selector:SemanticSelector::MarkRange{..},..},
                SemanticInstruction::Repeat{selector:SemanticSelector::MarkRange{..},plays,..}
            ] if plays.get()==3)),
        json!({"macro":"u","instructions":["select_mark_range","yank mark_range","repeat mark_range"],"frames":140}),state(d))?;
    undo(d, &marked)?;
    idle(d)?;
    mark(d, 15, "a")?;
    let rebound_a = document(d)?.clone();
    mark(d, 30, "b")?;
    let rebound = document(d)?.clone();
    at(d, 80)?;
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    idle(d)?;
    d.check("Dot re-resolves the saved named Repeat against rebound marks despite a different cursor",
        d.app().sequence_length()==150
        && d.app().semantic.snapshot().and_then(|snapshot|snapshot.edit.as_ref())
            .is_some_and(|last|matches!(&last.operation,RepeatableEdit::Repeat{selector:actual,plays,..} if *actual==selector() && plays.get()==3)),
        json!({"current_marks":[15,30],"cursor_at_invocation":80,"frames":150}),state(d))?;
    undo(d, &rebound)?;
    idle(d)?;
    at(d, 70)?;
    let before = d.revision();
    d.command("macro u")?;
    d.changed(&before)?;
    idle(d)?;
    d.check(
        "Named call resolves current mark range and persists its newly captured contents",
        d.app().sequence_length() == 150
            && copied(d, None)?.slice().range()
                == FrameRange::new(ProjectFrame(15), ProjectFrame(30))
                    .map_err(|e| e.to_string())?,
        json!({"macro":"u","range":[15,30],"frames":150}),
        state(d),
    )?;
    undo(d, &rebound)?;
    idle(d)?;
    selected(d, 15, 30)?;
    let before = d.revision();
    d.command("delete")?;
    d.changed(&before)?;
    idle(d)?;
    d.check("Direct command cut saves named-mark intent for dot and the exact historical slice",
        d.app().sequence_length()==105 && copied(d,None)?.slice().range()==FrameRange::new(ProjectFrame(15),ProjectFrame(30)).map_err(|e|e.to_string())?
        && d.app().semantic.snapshot().and_then(|snapshot|snapshot.edit.as_ref())
            .is_some_and(|last|matches!(&last.operation,RepeatableEdit::Cut(RepeatableCut::Selector(actual)) if *actual==selector())),
        json!({"cut":[15,30],"frames":105,"selector":"mark_range"}),state(d))?;
    undo(d, &rebound)?;
    idle(d)?;

    mark(d, 16, "a")?;
    let cut_rebound_a = document(d)?.clone();
    mark(d, 32, "b")?;
    let cut_rebound = document(d)?.clone();
    at(d, 80)?;
    let before = d.revision();
    d.key(Key::Period)?;
    d.changed(&before)?;
    idle(d)?;
    d.check("Dot follows the Cut receipt through rebound marks and publishes the current historical range",
        d.app().sequence_length() == 104
        && copied(d, None)?.slice().range() == FrameRange::new(ProjectFrame(16), ProjectFrame(32)).map_err(|error| error.to_string())?
        && d.app().sequence_cursor == 16,
        json!({"old_range":[15,30],"current_range":[16,32],"invocation_cursor":80,"frames":104}), state(d))?;
    undo(d, &cut_rebound)?;
    undo(d, &cut_rebound_a)?;
    undo(d, &rebound)?;
    idle(d)?;

    // Capture before command text, then change only the local cursor: Enter must not retarget.
    at(d, 70)?;
    let before = document(d)?.clone();
    let old_bank = bank(d)?;
    let old_selection = d.app().capture_visual_selection()?;
    d.key(Key::Colon)?;
    d.app_mut().command = SELECT.into();
    d.app_mut().sequence_cursor = 71;
    d.app_mut().request_picture(false);
    d.key(Key::Enter)?;
    idle(d)?;
    d.check("Delayed named selection refuses changed command context without touching history, bank or selection",
        document(d)?==&before && bank(d)?==old_bank && d.app().capture_visual_selection()?==old_selection
        && d.app().sequence_cursor==71 && d.app().error.as_deref().is_some_and(|error|error.contains("context changed")),
        json!({"captured_cursor":70,"retained_cursor":71,"no_write":true}),state(d))?;
    undo(d, &rebound_a)?;
    undo(d, &marked)?;
    undo(d, &first_mark)?;
    undo(d, &baseline)
}
