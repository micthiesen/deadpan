use super::*;
use egui_kittest::kittest::Queryable as _;

pub(super) fn run(d: &mut Driver<'_>) -> Result<(), String> {
    let baseline = document(d)?.clone();
    for finished in [false, true] {
        select(d, 20, 30, finished)?;
        let selection = d.app().edit_range.clone();
        let first_step = d.report.steps.len();
        for modifiers in [
            egui::Modifiers::SHIFT,
            egui::Modifiers::ALT,
            egui::Modifiers::CTRL,
            egui::Modifiers::COMMAND,
        ] {
            d.key_modified(Key::D, modifiers)?;
        }
        for digit in [Key::Num0, Key::Num1, Key::Num2] {
            d.chord(&[digit, Key::D])?;
            d.check(
                "A count cannot multiply or retarget an explicit range deletion",
                *document(d)? == baseline
                    && d.app().edit_range == selection
                    && d.app().bindings.pending().is_empty(),
                json!({"finished":finished,"count_key":format!("{digit:?}"),"unchanged":true}),
                d.snapshot(),
            )?;
        }
        d.events(
            "Hold modified d without releasing it",
            vec![key_event(Key::D, egui::Modifiers::ALT, true)],
        )?;
        d.events(
            "Held d key repeat is ignored",
            vec![egui::Event::Key {
                key: Key::D,
                physical_key: None,
                pressed: true,
                repeat: true,
                modifiers: egui::Modifiers::NONE,
            }],
        )?;
        d.events(
            "Release held d",
            vec![key_event(Key::D, egui::Modifiers::NONE, false)],
        )?;
        d.check(
            "Modified and repeated d never change an active or finished selection",
            *document(d)? == baseline
                && d.app().edit_range == selection
                && d.report.steps[first_step..].iter().all(|step| {
                    step.semantic["stages"].as_array().is_none_or(|stages| {
                        stages
                            .iter()
                            .all(|stage| stage["stage"] != "command_admitted")
                    })
                }),
            json!({"finished":finished,"unchanged":true}),
            d.snapshot(),
        )?;
    }
    d.events(
        "IME preedit owns d in its native batch",
        vec![
            egui::Event::Ime(egui::ImeEvent::Preedit {
                text: "d".into(),
                active_range_chars: Some(0..1),
            }),
            key_event(Key::D, egui::Modifiers::NONE, true),
            key_event(Key::D, egui::Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "IME composition cannot delete selected time",
        *document(d)? == baseline && d.app().ime_composing,
        json!("composition retained"),
        d.snapshot(),
    )?;
    d.events(
        "IME commit owns the remaining batch",
        vec![
            egui::Event::Ime(egui::ImeEvent::Commit("d".into())),
            key_event(Key::D, egui::Modifiers::NONE, true),
            key_event(Key::D, egui::Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "IME commit cannot leak a deletion",
        *document(d)? == baseline && !d.app().ime_composing,
        json!("unchanged"),
        d.snapshot(),
    )?;

    d.key(Key::Colon)?;
    d.events(
        "Native command text owns d",
        vec![
            egui::Event::Text("d".into()),
            key_event(Key::D, egui::Modifiers::NONE, true),
            key_event(Key::D, egui::Modifiers::NONE, false),
        ],
    )?;
    d.check(
        "Text editing keeps the selected interval and authored revision",
        *document(d)? == baseline
            && d.app().command_open
            && d.app().selected_edit_range() == Some(range(20, 30)),
        json!("native text only"),
        d.snapshot(),
    )?;
    d.key(Key::Escape)?;
    d.harness.get_by_label("Keys  ?").focus();
    d.step(
        "Focus a native button through its accessibility action",
        false,
    )?;
    d.check(
        "The native Keys button owns focus",
        native_control_focused(&d.harness.ctx),
        json!(true),
        d.snapshot(),
    )?;
    d.key(Key::D)?;
    d.check(
        "A focused native control prevents the Visual delete shortcut",
        *document(d)? == baseline && d.app().selected_edit_range() == Some(range(20, 30)),
        json!("selection unchanged"),
        d.snapshot(),
    )?;
    d.command("sequence")?;

    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 640.0));
    let input = d.harness.input_mut();
    input.screen_rect = Some(rect);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .ok_or("Missing viewport")?
        .inner_rect = Some(rect);
    d.step(
        "Paint selected range and deletion hints at minimum size",
        true,
    )?;
    for label in ["Edit [20..30)", "cut range"] {
        let paint = scenarios::text_paint_visibility(d, label);
        d.check(
            "Minimum workspace paints the exact range and deletion key hint",
            !paint.is_empty() && paint.iter().all(|part| part["fully_visible"] == true),
            json!(label),
            json!(paint),
        )?;
    }
    d.capture("Visual range deletion at 960 by 640")?;

    for finished in [false, true] {
        select(d, 20, 30, finished)?;
        let revision = d.revision();
        d.events(
            "Two d presses in one native batch",
            [Key::D, Key::D]
                .into_iter()
                .flat_map(|key| {
                    [
                        key_event(key, egui::Modifiers::NONE, true),
                        key_event(key, egui::Modifiers::NONE, false),
                    ]
                })
                .collect(),
        )?;
        d.changed(&revision)?;
        d.check(
            "A native batch cannot apply the captured range twice",
            d.app().sequence_length() == 110 && d.app().sequence_cursor == 20,
            json!({"finished":finished,"frames":110}),
            d.snapshot(),
        )?;
        undo(d, &baseline)?;
        d.check(
            "One Undo reaches the baseline after duplicate d input",
            !d.app().workspace.as_ref().unwrap().can_undo,
            json!(false),
            d.snapshot(),
        )?;
    }
    d.key(Key::Escape)?;
    d.chord(&[Key::G, Key::G])?;
    let revision = d.revision();
    d.events(
        "Select and delete with duplicate count digits in one native batch",
        [Key::V, Key::Num1, Key::Num1, Key::L, Key::D]
            .into_iter()
            .flat_map(|key| {
                [
                    key_event(key, egui::Modifiers::NONE, true),
                    key_event(key, egui::Modifiers::NONE, false),
                ]
            })
            .collect(),
    )?;
    d.changed(&revision)?;
    d.check(
        "Selection-aware routing observes the complete same-frame motion count",
        d.app().sequence_length() == 109 && d.app().sequence_cursor == 0,
        json!({"frames":109,"cursor":0}),
        d.snapshot(),
    )?;
    picture(d, 11)?;
    undo(d, &baseline)
}
