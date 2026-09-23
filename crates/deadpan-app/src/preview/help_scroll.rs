//! Keyboard scrolling uses the last measured viewport and the actual pointer
//! offset. It never installs an offset on an otherwise pointer-driven frame.

use eframe::egui::{self, Event, Key, Modifiers};

const LINE: f32 = 28.0;
const HINT: &str = "j/k · Up/Down scroll     PgUp/PgDn page     Home/End ends     Esc close";

#[derive(Default)]
pub(super) struct HelpScroll {
    offset: f32,
    viewport: f32,
    content: f32,
    requested: Option<f32>,
}

pub(super) enum RoutedInput {
    Help(Vec<Event>),
    Editor(Event),
    Done,
}

/// Native dialogs and egui menus own input before help or editor bindings.
/// Use persistent popup memory: `Context::any_popup_open` only reports popups
/// already drawn in this pass, while this router runs before File and ComboBox.
pub(super) fn defer_popup_input(
    context: &egui::Context,
    dialog_open: bool,
    composing: &mut bool,
    bindings: &mut crate::navigation::Bindings,
) -> bool {
    if dialog_open || egui::Popup::is_any_open(context) || context.any_popup_open() {
        context.input(|input| observe_composition(&input.events, composing));
        bindings.clear();
        true
    } else {
        false
    }
}

/// Observe native lifecycle state without consuming another popup's input.
/// The return value tells the editor to abandon any pending operator on blur.
pub(super) fn observe_composition(events: &[Event], composing: &mut bool) -> bool {
    let mut lost_focus = false;
    for event in events {
        match event {
            Event::Ime(egui::ImeEvent::Preedit { text, .. }) => *composing = !text.is_empty(),
            Event::Ime(egui::ImeEvent::Commit(_)) => *composing = false,
            Event::WindowFocused(false) => {
                *composing = false;
                lost_focus = true;
            }
            _ => {}
        }
    }
    lost_focus
}

impl HelpScroll {
    fn maximum(&self) -> f32 {
        (self.content - self.viewport).max(0.0)
    }

    fn key(&mut self, key: Key, modifiers: Modifiers) {
        if modifiers != Modifiers::NONE {
            return;
        }
        let offset = self.requested.unwrap_or(self.offset);
        let next = match key {
            Key::J | Key::ArrowDown => offset + LINE,
            Key::K | Key::ArrowUp => offset - LINE,
            Key::PageDown => offset + self.viewport * 0.9,
            Key::PageUp => offset - self.viewport * 0.9,
            Key::Home => 0.0,
            Key::End => self.maximum(),
            _ => return,
        };
        self.requested = Some(next.clamp(0.0, self.maximum()));
    }

    /// Return whether Escape closed help. Events before that boundary belong to
    /// help, including repeated key presses; events after it retain their order
    /// for the editor/command router. Pointer events reach help while it stays
    /// open; closing help discards its entire prefix, including owned clicks.
    pub(super) fn route_events(&mut self, events: &mut Vec<Event>) -> bool {
        let close = events.iter().position(|event| {
            matches!(
                event,
                Event::Key {
                    key: Key::Escape,
                    modifiers: Modifiers::NONE,
                    pressed: true,
                    ..
                }
            )
        });
        for event in events.iter().take(close.unwrap_or(events.len())) {
            if let Event::Key {
                key,
                modifiers,
                pressed: true,
                ..
            } = event
            {
                self.key(*key, *modifiers);
            }
        }
        if let Some(index) = close {
            events.drain(..=index);
            true
        } else {
            events.retain(|event| {
                !matches!(
                    event,
                    Event::Key { .. }
                        | Event::Text(_)
                        | Event::Ime(_)
                        | Event::Paste(_)
                        | Event::Copy
                        | Event::Cut
                )
            });
            false
        }
    }

    /// Recheck ownership before every event, since a preceding editor action
    /// can open help and a later Escape can return to command entry in one batch.
    pub(super) fn next_event(
        &mut self,
        open: &mut bool,
        events: &mut std::vec::IntoIter<Event>,
    ) -> RoutedInput {
        if *open {
            let mut remaining: Vec<_> = events.by_ref().collect();
            *open = !self.route_events(&mut remaining);
            *events = remaining.clone().into_iter();
            RoutedInput::Help(remaining)
        } else if let Some(event) = events.next() {
            RoutedInput::Editor(event)
        } else {
            RoutedInput::Done
        }
    }

    fn measured(&mut self, offset: f32, viewport: f32, content: f32) {
        self.viewport = viewport.max(0.0);
        self.content = content.max(0.0);
        self.offset = offset.clamp(0.0, self.maximum());
    }

    pub(super) fn show<R>(
        &mut self,
        context: &egui::Context,
        open: &mut bool,
        contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> Option<egui::InnerResponse<Option<R>>> {
        let available = (context.input(|input| input.content_rect().height()) - 80.0).max(140.0);
        egui::Window::new("Keys · reshape one Original")
            .open(open)
            .collapsible(false)
            .resizable(true)
            .default_width(680.0)
            .default_height(560.0_f32.min(available))
            .min_height(140.0)
            .max_height(available)
            .show(context, |ui| {
                ui.weak(HINT);
                ui.separator();
                let mut area = egui::ScrollArea::vertical()
                    .id_salt("keys-reference-scroll")
                    .auto_shrink([false, false])
                    .min_scrolled_height(0.0)
                    .max_height(ui.available_height())
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible);
                if let Some(offset) = self.requested.take() {
                    area = area.vertical_scroll_offset(offset);
                }
                let output = area.show(ui, contents);
                self.measured(
                    output.state.offset.y,
                    output.inner_rect.height(),
                    output.content_size.y,
                );
                output.inner
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(key: Key, repeat: bool, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat,
            modifiers,
        }
    }

    #[test]
    fn lines_pages_and_ends_clamp_to_measured_geometry_and_use_pointer_offset() {
        let mut scroll = HelpScroll::default();
        scroll.measured(70.0, 200.0, 950.0);
        scroll.key(Key::PageDown, Modifiers::NONE);
        assert_eq!(scroll.requested.take(), Some(250.0));
        // A pointer move changes the next keyboard origin, not just egui state.
        scroll.measured(413.0, 200.0, 950.0);
        scroll.key(Key::J, Modifiers::NONE);
        assert_eq!(scroll.requested.take(), Some(441.0));
        scroll.key(Key::PageUp, Modifiers::NONE);
        assert_eq!(scroll.requested.take(), Some(233.0));
        scroll.key(Key::End, Modifiers::NONE);
        scroll.key(Key::ArrowDown, Modifiers::NONE);
        assert_eq!(scroll.requested.take(), Some(750.0));
        scroll.key(Key::Home, Modifiers::NONE);
        scroll.key(Key::K, Modifiers::NONE);
        assert_eq!(scroll.requested.take(), Some(0.0));
        scroll.measured(750.0, 1000.0, 950.0);
        assert_eq!(scroll.offset, 0.0);
        scroll.key(Key::End, Modifiers::NONE);
        assert_eq!(scroll.requested.take(), Some(0.0));
    }

    #[test]
    fn held_keys_repeat_and_native_modifiers_do_not_scroll_or_escape() {
        let mut scroll = HelpScroll::default();
        scroll.measured(0.0, 200.0, 950.0);
        let mut events = vec![event(Key::J, false, Modifiers::NONE)];
        events.extend((0..8).map(|_| event(Key::J, true, Modifiers::NONE)));
        assert!(!scroll.route_events(&mut events));
        assert!(events.is_empty());
        assert_eq!(scroll.requested.take(), Some(LINE * 9.0));
        for modifiers in [
            Modifiers::CTRL,
            Modifiers::COMMAND,
            Modifiers::MAC_CMD,
            Modifiers::ALT,
            Modifiers::SHIFT,
        ] {
            let mut events = vec![
                event(Key::End, false, modifiers),
                event(Key::Escape, false, modifiers),
            ];
            assert!(!scroll.route_events(&mut events));
            assert!(events.is_empty());
            assert_eq!(scroll.requested, None);
        }
    }

    #[test]
    fn escape_preserves_the_ordered_command_suffix_without_replaying_help_input() {
        use crate::navigation::{Action, Bindings};
        let pointer = Event::PointerMoved(egui::pos2(80.0, 90.0));
        let suffix = vec![
            event(Key::Colon, false, Modifiers::SHIFT),
            Event::Text(":".into()),
            Event::Text("source".into()),
            event(Key::Enter, false, Modifiers::NONE),
            event(Key::S, false, Modifiers::NONE),
        ];
        let mut events = vec![
            event(Key::S, false, Modifiers::NONE),
            Event::Text("s".into()),
            pointer.clone(),
            event(Key::Escape, false, Modifiers::NONE),
        ];
        events.extend(suffix.clone());
        assert!(HelpScroll::default().route_events(&mut events));
        assert_eq!(events, suffix);
        let mut bindings = Bindings::default();
        let mut command_open = false;
        let mut actions = Vec::new();
        for event in events {
            if let Event::Key {
                key,
                modifiers,
                pressed: true,
                ..
            } = event
                && let Some(action) = bindings.key(key, modifiers, command_open, false)
            {
                command_open |= action == Action::Command;
                actions.push(action);
            }
        }
        assert!(command_open);
        assert_eq!(actions, vec![Action::Command]);
    }

    #[test]
    fn opening_closing_and_reopening_help_changes_ownership_within_one_batch() {
        use crate::navigation::{Action, Bindings};
        for (mut open, input, expected) in [
            (
                false,
                vec![
                    event(Key::Questionmark, false, Modifiers::NONE),
                    event(Key::S, false, Modifiers::NONE),
                ],
                vec![Action::Help],
            ),
            (
                true,
                vec![
                    event(Key::Escape, false, Modifiers::NONE),
                    event(Key::Questionmark, false, Modifiers::NONE),
                    event(Key::D, false, Modifiers::NONE),
                    event(Key::D, false, Modifiers::NONE),
                ],
                vec![Action::Help],
            ),
            (
                true,
                vec![
                    event(Key::Escape, false, Modifiers::NONE),
                    event(Key::Questionmark, false, Modifiers::NONE),
                    event(Key::Escape, false, Modifiers::NONE),
                    event(Key::Colon, false, Modifiers::SHIFT),
                    event(Key::S, false, Modifiers::NONE),
                ],
                vec![Action::Help, Action::Command],
            ),
        ] {
            let mut scroll = HelpScroll::default();
            let mut events = input.into_iter();
            let mut bindings = Bindings::default();
            let mut actions = Vec::new();
            let mut command = false;
            loop {
                match scroll.next_event(&mut open, &mut events) {
                    RoutedInput::Editor(Event::Key {
                        key,
                        modifiers,
                        pressed: true,
                        ..
                    }) => {
                        if let Some(action) = bindings.key(key, modifiers, command, false) {
                            open |= action == Action::Help;
                            command |= action == Action::Command;
                            actions.push(action);
                        }
                    }
                    RoutedInput::Help(_) if open => break,
                    RoutedInput::Done => break,
                    _ => {}
                }
            }
            assert_eq!(actions, expected);
        }
    }

    #[test]
    fn closing_help_discards_its_pointer_and_ime_prefix_before_editor_gating() {
        use crate::navigation::{Action, Bindings};
        for prefix in [
            Event::PointerButton {
                pos: egui::pos2(100.0, 100.0),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::NONE,
            },
            Event::Ime(egui::ImeEvent::Commit("old composition".into())),
        ] {
            let mut events = vec![
                prefix,
                event(Key::Escape, false, Modifiers::NONE),
                event(Key::Colon, false, Modifiers::SHIFT),
                Event::Text("source".into()),
            ];
            assert!(HelpScroll::default().route_events(&mut events));
            assert!(!super::super::pointer_focus_transition(&events));
            let ime_event = events.iter().any(|event| matches!(event, Event::Ime(_)));
            assert!(!ime_event);
            assert_eq!(
                Bindings::default().key(Key::Colon, Modifiers::SHIFT, false, ime_event),
                Some(Action::Command)
            );
            assert_eq!(events.len(), 2);
        }
        // A pointer event is still usable by the ScrollArea while help stays open.
        let pointer = Event::PointerMoved(egui::pos2(100.0, 100.0));
        let mut events = vec![event(Key::J, false, Modifiers::NONE), pointer.clone()];
        assert!(!HelpScroll::default().route_events(&mut events));
        assert_eq!(events, vec![pointer]);
    }

    #[test]
    fn popup_owned_commit_and_focus_loss_clear_composition_without_consuming_events() {
        for event in [
            Event::Ime(egui::ImeEvent::Commit("text".into())),
            Event::WindowFocused(false),
        ] {
            let events = vec![event];
            let retained = events.clone();
            let mut composing = true;
            let lost_focus = observe_composition(&events, &mut composing);
            assert!(!composing);
            assert_eq!(lost_focus, matches!(events[0], Event::WindowFocused(false)));
            assert_eq!(events, retained);
        }
    }

    #[derive(Default)]
    struct MenuReplay {
        scroll: HelpScroll,
        help_open: bool,
        composing: bool,
        bindings: crate::navigation::Bindings,
        actions: Vec<crate::navigation::Action>,
    }

    impl MenuReplay {
        fn frame(
            &mut self,
            context: &egui::Context,
            events: Vec<Event>,
        ) -> (egui::Rect, egui::Id, bool) {
            let mut menu = (egui::Rect::NOTHING, egui::Id::NULL);
            let mut deferred = None;
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 720.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    // Match the application's order: global keyboard routing
                    // runs before the header menu and the nonmodal Keys window.
                    let blocked =
                        defer_popup_input(context, false, &mut self.composing, &mut self.bindings);
                    deferred.get_or_insert(blocked);
                    if blocked {
                        assert!(!context.any_popup_open(), "menu has not drawn this pass");
                    } else {
                        let mut events = context.input(|input| input.events.clone()).into_iter();
                        loop {
                            match self.scroll.next_event(&mut self.help_open, &mut events) {
                                RoutedInput::Help(remaining) => {
                                    context.input_mut(|input| input.events = remaining);
                                    if self.help_open {
                                        break;
                                    }
                                }
                                RoutedInput::Editor(Event::Key {
                                    key,
                                    modifiers,
                                    pressed: true,
                                    ..
                                }) => {
                                    if let Some(action) =
                                        self.bindings.key(key, modifiers, false, false)
                                    {
                                        self.actions.push(action);
                                        context.input_mut(|input| {
                                            input.consume_key(modifiers, key);
                                        });
                                    }
                                }
                                RoutedInput::Done => break,
                                _ => {}
                            }
                        }
                    }
                    // Keep the test's menu button outside the floating window.
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        let response = ui.menu_button("File", |ui| ui.button("Open project"));
                        menu = (
                            response.response.rect,
                            egui::Popup::default_response_id(&response.response),
                        );
                    });
                    self.scroll.show(context, &mut self.help_open, |ui| {
                        for i in 0..100 {
                            ui.label(format!("Reference row {i}"));
                        }
                    });
                },
            );
            output.textures_delta.clear();
            (menu.0, menu.1, deferred.unwrap())
        }

        fn open_menu(&mut self, context: &egui::Context) -> egui::Id {
            self.frame(context, vec![]);
            let (rect, id, _) = self.frame(context, vec![]);
            for pressed in [true, false] {
                self.frame(
                    context,
                    vec![
                        Event::PointerMoved(rect.center()),
                        Event::PointerButton {
                            pos: rect.center(),
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        },
                    ],
                );
            }
            assert!(
                egui::Popup::is_id_open(context, id),
                "real File menu opened"
            );
            id
        }
    }

    #[test]
    fn headless_file_menu_owns_escape_and_edit_keys_before_help_and_editor() {
        use crate::navigation::Action;
        for help_open in [true, false] {
            let context = egui::Context::default();
            let mut replay = MenuReplay {
                help_open,
                ..Default::default()
            };
            let popup = replay.open_menu(&context);
            replay.composing = true;
            replay.bindings.key(Key::D, Modifiers::NONE, false, false);
            let (_, _, deferred) = replay.frame(
                &context,
                vec![
                    Event::Ime(egui::ImeEvent::Commit("old composition".into())),
                    event(Key::ArrowDown, false, Modifiers::NONE),
                    event(Key::S, false, Modifiers::NONE),
                    event(Key::D, false, Modifiers::NONE),
                    event(Key::D, false, Modifiers::NONE),
                ],
            );
            assert!(deferred, "open menu owns keyboard before its widgets draw");
            assert_eq!(replay.help_open, help_open);
            assert!(replay.actions.is_empty());
            assert!(!replay.composing);
            assert_eq!(replay.bindings.pending(), "");
            assert_eq!(replay.scroll.offset, 0.0);
            assert!(egui::Popup::is_id_open(&context, popup));

            // The whole batch belongs to the menu at entry. Its Escape must
            // reach Popup::show; suffix letters never become editor shortcuts.
            let (_, _, deferred) = replay.frame(
                &context,
                vec![
                    event(Key::Escape, false, Modifiers::NONE),
                    event(Key::Colon, false, Modifiers::SHIFT),
                    event(Key::S, false, Modifiers::NONE),
                ],
            );
            assert!(deferred);
            assert!(!egui::Popup::is_id_open(&context, popup));
            assert_eq!(replay.help_open, help_open);
            assert!(replay.actions.is_empty());

            // Once the menu has closed, help's ordinary same-batch Escape to
            // command transition remains available without an extra frame.
            replay.frame(
                &context,
                vec![
                    event(Key::Escape, false, Modifiers::NONE),
                    event(Key::Colon, false, Modifiers::SHIFT),
                ],
            );
            assert!(!replay.help_open);
            if help_open {
                assert_eq!(replay.actions, vec![Action::Command]);
            } else {
                assert_eq!(replay.actions, vec![Action::Escape, Action::Command]);
            }
        }
    }

    fn frame(
        context: &egui::Context,
        scroll: &mut HelpScroll,
        events: Vec<Event>,
    ) -> (egui::Rect, egui::Rect) {
        let mut last = egui::Rect::NOTHING;
        let mut window = egui::Rect::NOTHING;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 720.0),
            )),
            events,
            ..Default::default()
        };
        let mut output = context.run_ui(input, |ui| {
            let mut open = true;
            let shown = scroll
                .show(ui.ctx(), &mut open, |ui| {
                    for i in 0..100 {
                        last = ui.label(format!("Reference row {i}")).rect;
                    }
                })
                .unwrap();
            window = shown.response.rect;
        });
        output.textures_delta.clear();
        (window, last)
    }

    #[test]
    fn headless_window_replay_reaches_the_final_row_and_keeps_pointer_and_keyboard_in_sync() {
        let context = egui::Context::default();
        let mut scroll = HelpScroll::default();
        frame(&context, &mut scroll, vec![]);
        let (window, _) = frame(&context, &mut scroll, vec![]);
        assert!(
            scroll.viewport >= 350.0,
            "first page height {}",
            scroll.viewport
        );
        assert!(
            scroll.viewport < window.height() - 20.0,
            "hint and title remain outside the list"
        );
        assert!(scroll.content > scroll.viewport * 2.0);
        scroll.key(Key::End, Modifiers::NONE);
        let (window, last) = frame(&context, &mut scroll, vec![]);
        assert!((scroll.offset - scroll.maximum()).abs() < 1.0);
        assert!(
            window.contains_rect(last),
            "last row {last:?}, window {window:?}"
        );
        // Replay a real egui wheel event over the list, then start a keyboard
        // move from the offset reported by that very ScrollArea.
        let pointer = window.center();
        frame(
            &context,
            &mut scroll,
            vec![
                Event::PointerMoved(pointer),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, 120.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        for _ in 0..20 {
            frame(&context, &mut scroll, vec![]);
        }
        assert!(scroll.offset < scroll.maximum());
        let pointer_offset = scroll.offset;
        scroll.key(Key::ArrowUp, Modifiers::NONE);
        assert_eq!(scroll.requested, Some((pointer_offset - LINE).max(0.0)));
        frame(&context, &mut scroll, vec![]);
        assert!((scroll.offset - (pointer_offset - LINE).max(0.0)).abs() < 1.0);
        scroll.key(Key::Home, Modifiers::NONE);
        frame(&context, &mut scroll, vec![]);
        assert_eq!(scroll.offset, 0.0);
    }
}
