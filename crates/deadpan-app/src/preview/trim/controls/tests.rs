//! Production keyboard/focus helpers under egui, without an app, service or GPU.

use super::*;
use egui::{Context, Event as UiEvent, Id, Key, Modifiers, RawInput};

fn key(key: Key, modifiers: Modifiers) -> UiEvent {
    UiEvent::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

fn plain(key_code: Key) -> UiEvent {
    key(key_code, Modifiers::NONE)
}

fn text(value: &str) -> UiEvent {
    UiEvent::Text(value.into())
}

#[test]
fn focus_shortcut_keeps_later_native_typing_and_field_enter_cannot_apply() {
    let mut events = vec![plain(Key::E), text("e"), text("+5f"), plain(Key::Enter)];
    let batch = route_events(&mut events, false, true);
    assert_eq!(batch.keys, vec![TrimKey::FocusAmount]);
    assert!(batch.accept_amount);
    assert!(events.is_empty());
    assert_eq!(batch.amount_events, Some(vec![text("+5f")]));
}

#[test]
fn background_text_before_focus_cannot_become_part_of_the_amount() {
    let mut events = vec![
        plain(Key::H),
        text("h"),
        plain(Key::E),
        text("e"),
        text("+5f"),
        plain(Key::Enter),
    ];
    let batch = route_events(&mut events, false, true);
    assert_eq!(batch.keys, vec![TrimKey::Nudge(-1), TrimKey::FocusAmount]);
    assert!(batch.accept_amount);
    assert!(events.is_empty());
    assert_eq!(batch.amount_events, Some(vec![text("+5f")]));
}

#[test]
fn unbound_background_text_is_not_replayed_into_a_later_focused_field() {
    let mut events = vec![
        plain(Key::Num7),
        text("7"),
        plain(Key::E),
        text("e"),
        text("+5f"),
    ];
    let batch = route_events(&mut events, false, true);
    assert_eq!(batch.keys, vec![TrimKey::FocusAmount]);
    assert!(!batch.accept_amount);
    assert_eq!(events, vec![plain(Key::Num7)]);
    assert_eq!(batch.amount_events, Some(vec![text("+5f")]));
}

#[test]
fn mixed_tab_and_nudge_batch_keeps_each_control_transition_in_order() {
    let mut events = vec![
        plain(Key::Tab),
        plain(Key::L),
        text("l"),
        plain(Key::Tab),
        key(Key::H, Modifiers::SHIFT),
        text("H"),
        key(Key::Tab, Modifiers::SHIFT),
        plain(Key::H),
        text("h"),
    ];
    let batch = route_events(&mut events, false, true);
    assert_eq!(
        batch.keys,
        vec![
            TrimKey::Cycle { reverse: false },
            TrimKey::Nudge(1),
            TrimKey::Cycle { reverse: false },
            TrimKey::Nudge(-10),
            TrimKey::Cycle { reverse: true },
            TrimKey::Nudge(-1),
        ]
    );
    assert!(!batch.accept_amount);
    assert!(batch.amount_events.is_none());
    let mut control = SourceTrimControl::In;
    let mut nudges = Vec::new();
    for action in batch.keys {
        match action {
            TrimKey::Cycle { reverse } => {
                control = navigation::trim::cycle_control(control, reverse);
            }
            TrimKey::Nudge(frames) => nudges.push((control, frames)),
            _ => panic!("unexpected action"),
        }
    }
    assert_eq!(
        nudges,
        vec![
            (SourceTrimControl::Out, 1),
            (SourceTrimControl::Slip, -10),
            (SourceTrimControl::Out, -1),
        ]
    );
}

#[test]
fn native_field_keeps_text_editing_and_modified_shortcuts() {
    let original = vec![
        key(Key::A, Modifiers::COMMAND),
        text("-7f"),
        plain(Key::ArrowLeft),
        plain(Key::Tab),
    ];
    let mut events = original.clone();
    let batch = route_events(&mut events, true, false);
    assert!(batch.keys.is_empty());
    assert!(!batch.accept_amount);
    assert!(batch.amount_events.is_none());
    assert_eq!(events, original);
}

#[test]
fn global_modifier_chords_before_focus_remain_unclaimed_outside_the_field_suffix() {
    let prefix = vec![
        key(Key::A, Modifiers::COMMAND),
        key(Key::ArrowLeft, Modifiers::ALT),
        key(Key::A, Modifiers::CTRL),
    ];
    let mut events = prefix.clone();
    events.extend([plain(Key::E), text("e"), text("+5f")]);
    let batch = route_events(&mut events, false, true);
    assert_eq!(batch.keys, vec![TrimKey::FocusAmount]);
    assert_eq!(events, prefix);
    assert_eq!(batch.amount_events, Some(vec![text("+5f")]));
}

#[derive(Default)]
struct FocusHarness {
    context: Context,
    amount: String,
}

#[derive(Default)]
struct FrameResult {
    keys: Vec<TrimKey>,
    accepted_amount: bool,
    next_button: Option<Id>,
    remaining_events: Vec<UiEvent>,
    passes: usize,
}

impl FocusHarness {
    /// Production helpers own routing, event isolation and heading locking. The
    /// small widget tree reproduces their real ordering around native TextEdit.
    fn frame(&mut self, mut events: Vec<UiEvent>, focus_heading: bool) -> FrameResult {
        // These are separate taps. egui derives repeat from its held-key set,
        // so release the keys after this batch before the next frame's input.
        let releases = events
            .iter()
            .filter_map(|event| {
                let UiEvent::Key { pressed: true, .. } = event else {
                    return None;
                };
                let mut release = event.clone();
                if let UiEvent::Key { pressed, .. } = &mut release {
                    *pressed = false;
                }
                Some(release)
            })
            .collect::<Vec<_>>();
        events.extend(releases);
        let context = self.context.clone();
        let mut result = FrameResult::default();
        let output = context.run_ui(
            RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 360.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                result.passes += 1;
                let first_pass = context.current_pass_index() == 0;
                let mut batch = if first_pass {
                    let field = context.memory(|memory| memory.has_focus(Id::new(AMOUNT)));
                    let background = context
                        .memory(|memory| memory.focused().is_none_or(|id| id == Id::new(FOCUS)));
                    context.input_mut(|input| route_events(&mut input.events, field, background))
                } else {
                    KeyBatch::default()
                };
                for action in &batch.keys {
                    if *action == TrimKey::FocusAmount {
                        context_focus(&context, AMOUNT);
                    }
                }
                result.keys.extend(batch.keys);

                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(200.0, 28.0), egui::Sense::hover());
                let heading = ui.interact(rect, Id::new(FOCUS), egui::Sense::click());
                if first_pass && focus_heading {
                    heading.request_focus();
                }
                lock_heading(ui);
                with_amount_events(ui, batch.amount_events.take(), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.amount).id(Id::new(AMOUNT)))
                });
                if batch.accept_amount {
                    result.accepted_amount = true;
                    assert!(navigation::trim::parse_frames(&self.amount).is_ok());
                    context_focus(&context, FOCUS);
                }
                result.next_button = Some(ui.button("Next native control").id);
                if first_pass {
                    result.remaining_events = context.input(|input| input.events.clone());
                }
            },
        );
        output.drop_without_applying_deltas();
        result
    }

    fn focused(&self) -> Option<Id> {
        self.context.memory(|memory| memory.focused())
    }
}

#[test]
fn native_text_edit_receives_typing_after_e_but_never_the_shortcut_text() {
    let mut harness = FocusHarness::default();
    harness.frame(vec![], true);
    let result = harness.frame(
        vec![plain(Key::E), text("e"), text("+5f"), plain(Key::Enter)],
        false,
    );
    assert_eq!(harness.amount, "+5f");
    assert_eq!(result.keys, vec![TrimKey::FocusAmount]);
    assert!(result.accepted_amount);
    assert_eq!(
        result.passes, 2,
        "focus return exercises the discarded pass"
    );
    assert_eq!(harness.focused(), Some(Id::new(FOCUS)));
}

#[test]
fn background_backspace_cannot_edit_the_amount_before_its_focus_transition() {
    let mut harness = FocusHarness {
        amount: "+12f".into(),
        ..Default::default()
    };
    harness.frame(vec![], true);
    let result = harness.frame(vec![plain(Key::Backspace), plain(Key::E), text("e")], false);
    assert_eq!(result.keys, vec![TrimKey::FocusAmount]);
    assert_eq!(harness.focused(), Some(Id::new(AMOUNT)));
    assert_eq!(harness.amount, "+12f");
    assert!(result.remaining_events.contains(&plain(Key::Backspace)));
}

#[test]
fn earlier_command_a_stays_unclaimed_without_selecting_the_new_amount() {
    let mut harness = FocusHarness {
        amount: "+12f".into(),
        ..Default::default()
    };
    harness.frame(vec![], true);
    let select_all = key(Key::A, Modifiers::COMMAND);
    let result = harness.frame(
        vec![select_all.clone(), plain(Key::E), text("e"), text("5")],
        false,
    );
    assert_eq!(harness.amount, "+12f5");
    assert!(result.remaining_events.contains(&select_all));
    assert_eq!(result.keys, vec![TrimKey::FocusAmount]);
}

#[test]
fn command_a_after_focus_edits_natively_while_the_earlier_global_chord_survives() {
    let mut harness = FocusHarness {
        amount: "+12f".into(),
        ..Default::default()
    };
    harness.frame(vec![], true);
    let select_all = key(Key::A, Modifiers::COMMAND);
    let result = harness.frame(
        vec![
            select_all.clone(),
            plain(Key::E),
            text("e"),
            select_all.clone(),
            text("-3f"),
            plain(Key::Enter),
        ],
        false,
    );
    assert_eq!(harness.amount, "-3f");
    assert!(result.accepted_amount);
    assert_eq!(result.passes, 2);
    assert!(result.remaining_events.contains(&select_all));
    assert_eq!(harness.focused(), Some(Id::new(FOCUS)));
}

#[test]
fn suffix_consumption_is_preserved_when_restoring_unclaimed_global_input() {
    let context = Context::default();
    let prefix = key(Key::A, Modifiers::COMMAND);
    let output = context.run_ui(
        RawInput {
            events: vec![prefix.clone()],
            ..Default::default()
        },
        |ui| {
            with_amount_events(ui, Some(vec![plain(Key::Backspace), text("5")]), |ui| {
                assert!(!ui.input(|input| input.events.contains(&prefix)));
                assert!(ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Backspace)));
            });
            assert_eq!(
                ui.input(|input| input.events.clone()),
                vec![prefix.clone(), text("5")]
            );
        },
    );
    output.drop_without_applying_deltas();
}

#[test]
fn heading_tab_lock_survives_immediate_and_repeated_forward_and_reverse_tabs() {
    let mut harness = FocusHarness::default();
    harness.frame(vec![], true);
    // No warmup frame: newly gained focus must be safe for the next input.
    for reverse in [false, false, true, false, true] {
        let modifiers = if reverse {
            Modifiers::SHIFT
        } else {
            Modifiers::NONE
        };
        let result = harness.frame(vec![key(Key::Tab, modifiers)], false);
        assert_eq!(result.keys, vec![TrimKey::Cycle { reverse }]);
        assert_eq!(harness.focused(), Some(Id::new(FOCUS)));
    }
}

#[test]
fn returning_from_native_amount_then_immediate_tab_keeps_heading_ownership() {
    let mut harness = FocusHarness::default();
    harness.frame(vec![], true);
    let accepted = harness.frame(
        vec![plain(Key::E), text("e"), text("-3f"), plain(Key::Enter)],
        false,
    );
    assert!(accepted.accepted_amount);
    assert_eq!(harness.focused(), Some(Id::new(FOCUS)));
    let next = harness.frame(vec![plain(Key::Tab)], false);
    assert_eq!(next.keys, vec![TrimKey::Cycle { reverse: false }]);
    assert_eq!(harness.focused(), Some(Id::new(FOCUS)));
    assert_eq!(harness.amount, "-3f");
}

#[test]
fn tab_in_native_amount_traverses_widgets_without_changing_trim_control() {
    let mut harness = FocusHarness::default();
    harness.frame(vec![], true);
    harness.frame(vec![plain(Key::E), text("e"), text("+2f")], false);
    assert_eq!(harness.focused(), Some(Id::new(AMOUNT)));
    let next = harness.frame(vec![plain(Key::Tab)], false);
    assert!(next.keys.is_empty());
    assert!(!next.accepted_amount);
    assert_eq!(harness.focused(), next.next_button);
    assert_eq!(harness.amount, "+2f");
}
