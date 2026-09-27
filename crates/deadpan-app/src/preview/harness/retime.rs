//! Production command and inspector input for structural speed editing.

use super::*;
use deadpan_core::{ExactRatio, NodeKind, PitchPolicy, RetimePurpose};
use egui::{Key, Modifiers};

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let initial = d.app().workspace.as_ref().ok_or("No project")?.clone();
    let original = d.app().selected_beat.clone().ok_or("No selected beat")?;
    let revision = d.revision();
    let initial_length = d.app().sequence_length();
    let expected = crate::project::retime::resolve(
        &initial,
        &original,
        ExactRatio::new(3, 4).map_err(|error| error.to_string())?,
        false,
    )?;
    d.click("Change speed…  ·  :retime")?;
    d.capture("Speed entry previews exact duration and pitch before applying")?;
    let hint = expected.describe(PitchPolicy::Preserve);
    let hint_paint = scenarios::text_paint_visibility(d, &hint);
    d.check(
        "Inspector speed entry teaches command and quantized duration without a revision",
        d.app().command_open
            && d.app().command == "retime 0.75 pitch=preserve"
            && d.app().pane == Pane::Inspector
            && !hint_paint.is_empty()
            && hint_paint
                .iter()
                .all(|paint| paint["fully_visible"] == true)
            && d.revision() == revision,
        json!({"command":"retime 0.75 pitch=preserve", "preview":hint, "revision":revision}),
        json!({"state":d.snapshot(),"hint_paint":hint_paint}),
    )?;
    d.key(Key::Escape)?;
    d.check(
        "Escape cancels speed entry without changing the selected source",
        !d.app().command_open
            && d.revision() == revision
            && d.app().selected_beat.as_ref() == Some(&original),
        json!("unchanged Source and history"),
        d.snapshot(),
    )?;

    d.command("retime 0.75 pitch=preserve")?;
    d.changed(&revision)?;
    let retimed = d.app().selected_beat.clone().ok_or("Retime not selected")?;
    d.capture("Retime remains an editable speed stage over the Original")?;
    d.check("One speed command wraps the unchanged Original and selects its saved Retime",
        retimed != original && d.app().sequence_length() == expected.after.frames() as u64
            && d.app().workspace.as_ref().is_some_and(|workspace| {
                workspace.document.nodes()[&original] == initial.document.nodes()[&original]
                    && matches!(&workspace.document.nodes()[&retimed].kind, NodeKind::Retime { child, pitch: PitchPolicy::Preserve, purpose: RetimePurpose::Edit, .. } if child == &original)
            }), json!({"frames":expected.after.frames(), "pitch":"preserve", "input":original.as_str()}), d.snapshot())?;

    let before_update = d.revision();
    d.click("Change speed…  ·  Enter")?;
    d.key_modified(Key::A, Modifiers::COMMAND)?;
    d.events(
        "Replace speed and pitch through native-style command text editing",
        vec![egui::Event::Text("retime 0.5 pitch=tape".into())],
    )?;
    d.capture("Tape-speed preview retains the original input extent")?;
    d.key(Key::Enter)?;
    d.changed(&before_update)?;
    d.check("Pointer entry updates the same Retime from its retained input, without compounding its prior speed",
        d.app().selected_beat.as_ref() == Some(&retimed) && d.app().sequence_length() == initial_length * 2
            && d.app().workspace.as_ref().is_some_and(|workspace| matches!(&workspace.document.nodes()[&retimed].kind, NodeKind::Retime { pitch: PitchPolicy::FollowSpeed, .. })),
        json!({"frames":initial_length * 2, "pitch":"tape", "selected":retimed.as_str()}), d.snapshot())?;

    let before_nested = d.revision();
    d.command("wrap-retime 2 pitch=preserve")?;
    d.changed(&before_nested)?;
    let outer = d
        .app()
        .selected_beat
        .clone()
        .ok_or("Outer Retime not selected")?;
    d.check("Explicit wrap-retime creates a second editable stage", outer != retimed && d.app().sequence_length() == initial_length
        && d.app().workspace.as_ref().is_some_and(|workspace| matches!(&workspace.document.nodes()[&outer].kind, NodeKind::Retime { child, pitch: PitchPolicy::Preserve, .. } if child == &retimed)),
        json!({"frames":initial_length, "nested":true}), d.snapshot())?;
    for _ in 0..3 {
        let before = d.revision();
        d.key(Key::U)?;
        d.changed(&before)?;
    }
    d.check(
        "Three undos restore the protected full Original baseline",
        d.app().sequence_length() == initial_length
            && d.app().workspace.as_ref().is_some_and(|workspace| {
                workspace.document.nodes() == initial.document.nodes() && !workspace.can_undo
            }),
        json!("exact Original structure, protected baseline"),
        d.snapshot(),
    )?;
    d.command("source")?;
    let before_source = d.revision();
    d.command("retime 2 pitch=tape")?;
    d.capture("Original context keeps speed changes non-destructive")?;
    d.check(
        "A speed command in Original never modifies the project",
        d.revision() == before_source && d.app().sequence_length() == initial_length,
        json!(before_source),
        d.snapshot(),
    )
}
