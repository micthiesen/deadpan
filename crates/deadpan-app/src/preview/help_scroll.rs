//! Keyboard scrolling uses the last measured viewport and the actual pointer
//! offset. It never installs an offset on an otherwise pointer-driven frame.
//! `/` moves the keyboard to the sheet's search field, which then owns every
//! event until egui's own Enter or Escape leaves it.

use eframe::egui::{self, Event, Key, Modifiers};

use crate::navigation::panels::{HelpKey, help_key};
use crate::navigation::registry::mode_label;

const LINE: f32 = 28.0;
pub(super) const SEARCH_ID: &str = "keys-reference-search";

/// The key line, which names the field's own keys while it owns the keyboard.
fn hint(searching: bool) -> String {
    if searching {
        format!(
            "Type to filter     Enter keep filter     {} leave search",
            mode_label("help.close")
        )
    } else {
        format!(
            "{} search     {} scroll     {} close",
            mode_label("help.search"),
            mode_label("help.scroll"),
            mode_label("help.close")
        )
    }
}

#[derive(Default)]
pub(super) struct HelpScroll {
    offset: f32,
    viewport: f32,
    content: f32,
    requested: Option<f32>,
    /// The search text, kept until Help closes.
    query: String,
    /// `/` was pressed: focus the field before it is drawn this frame.
    focus_search: bool,
    /// Keyboard input owned by the field, withheld from every widget drawn
    /// before it and restored only while it is drawn.
    field_events: Vec<Event>,
    /// The field held focus when last drawn. egui drops a plain Escape's focus
    /// before routing, so this, not the live focus, says who owns Escape.
    field_focused: bool,
    /// An Escape left the field: release focus when it is drawn.
    leave_search: bool,
    /// The query the results were last drawn for; a new one starts at the top.
    shown_query: String,
}

fn plain_escape(event: &Event) -> bool {
    matches!(
        event,
        Event::Key {
            key: Key::Escape,
            modifiers: Modifiers::NONE,
            pressed: true,
            ..
        }
    )
}

fn keyboard_event(event: &Event) -> bool {
    matches!(
        event,
        Event::Key { .. }
            | Event::Text(_)
            | Event::Ime(_)
            | Event::Paste(_)
            | Event::Copy
            | Event::Cut
    )
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
    #[cfg(feature = "ui-harness")]
    pub(super) fn diagnostic_snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "offset": self.offset, "viewport": self.viewport,
            "content": self.content, "maximum": self.maximum(),
        })
    }

    fn maximum(&self) -> f32 {
        (self.content - self.viewport).max(0.0)
    }

    fn key(&mut self, key: Key, modifiers: Modifiers) {
        let offset = self.requested.unwrap_or(self.offset);
        let next = match help_key(key, modifiers) {
            Some(HelpKey::LineDown) => offset + LINE,
            Some(HelpKey::LineUp) => offset - LINE,
            Some(HelpKey::PageDown) => offset + self.viewport * 0.9,
            Some(HelpKey::PageUp) => offset - self.viewport * 0.9,
            Some(HelpKey::Top) => 0.0,
            Some(HelpKey::Bottom) => self.maximum(),
            _ => return,
        };
        self.requested = Some(next.clamp(0.0, self.maximum()));
    }

    fn take_field_events(&mut self, events: &mut Vec<Event>) {
        let (field, rest): (Vec<_>, Vec<_>) =
            std::mem::take(events).into_iter().partition(keyboard_event);
        self.field_events.extend(field);
        *events = rest;
    }

    /// The current search text.
    #[cfg(test)]
    pub(super) fn query(&self) -> &str {
        &self.query
    }

    /// Return whether Escape closed help. Events before that boundary belong to
    /// help, including repeated key presses; events after it retain their order
    /// for the editor/command router. Pointer events reach help while it stays
    /// open; closing help discards its entire prefix, including owned clicks.
    /// While the search field has focus it owns every event, so its own Enter
    /// and Escape leave the field rather than Help. A `/` press, by key or by
    /// its typed character on a layout that shifts it, moves the keyboard to
    /// the field and leaves the ordered suffix for it.
    pub(super) fn route_events(&mut self, events: &mut Vec<Event>, search_focused: bool) -> bool {
        let escape = events.iter().position(plain_escape);
        if search_focused || (self.field_focused && escape.is_some()) {
            let Some(index) = escape else {
                self.take_field_events(events);
                return false;
            };
            // Escape leaves the field, never Help; the field keeps what was
            // typed before it, and Help routes what follows.
            let mut rest = events.split_off(index + 1);
            events.pop();
            self.take_field_events(events);
            self.field_focused = false;
            self.leave_search = true;
            let closed = self.route_events(&mut rest, false);
            events.append(&mut rest);
            return closed;
        }
        let boundary = events.iter().enumerate().position(|(index, event)| {
            let Event::Key {
                key,
                modifiers,
                pressed: true,
                ..
            } = event
            else {
                return false;
            };
            matches!(
                help_key(*key, *modifiers),
                Some(HelpKey::Close | HelpKey::Search)
            ) || (!modifiers.command
                && !modifiers.ctrl
                && !modifiers.mac_cmd
                && matches!(events.get(index + 1), Some(Event::Text(text)) if text == "/"))
        });
        for event in events.iter().take(boundary.unwrap_or(events.len())) {
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
        let close = boundary.is_some_and(|index| {
            matches!(
                events[index],
                Event::Key {
                    key: Key::Escape,
                    modifiers: Modifiers::NONE,
                    ..
                }
            )
        });
        if let Some(index) = boundary
            && !close
        {
            let opener = match &events[index] {
                Event::Key { key, .. } => *key,
                _ => unreachable!("the boundary is a key press"),
            };
            events.drain(..=index);
            // The opener's own character is not search text, and a held
            // opener's repeats (with their characters) never echo into it.
            loop {
                match events.first() {
                    Some(Event::Text(text)) if text == "/" => {
                        events.remove(0);
                    }
                    Some(Event::Key {
                        key, repeat: true, ..
                    }) if *key == opener => {
                        events.remove(0);
                    }
                    Some(Event::Key {
                        key,
                        pressed: false,
                        ..
                    }) if *key == opener => {
                        events.remove(0);
                    }
                    _ => break,
                }
            }
            self.focus_search = true;
            // The suffix belongs to the field, split at its own Escape like
            // any focused batch: what follows Escape is Help's again.
            self.field_focused = true;
            self.route_events(events, true)
        } else if let Some(index) = boundary {
            events.drain(..=index);
            self.query.clear();
            self.focus_search = false;
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
        search_focused: bool,
    ) -> RoutedInput {
        if *open {
            let mut remaining: Vec<_> = events.by_ref().collect();
            *open = !self.route_events(&mut remaining, search_focused);
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
        contents: impl FnOnce(&mut egui::Ui, &str) -> R,
    ) -> Option<egui::InnerResponse<Option<R>>> {
        if !*open {
            // However Help closed, nothing typed for it survives.
            self.query.clear();
            self.focus_search = false;
            self.field_events.clear();
            self.field_focused = false;
            self.leave_search = false;
        }
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
                // Spoken when Help opens: the keyboard stays with Help until
                // Escape, while focus remains on the pane it returns to.
                let hint = ui.weak(hint(self.field_focused || self.focus_search));
                super::accessibility::live(&hint, false);
                let id = egui::Id::new(SEARCH_ID);
                // Text typed before an Escape in the same batch still lands:
                // focus for the draw, then release.
                if std::mem::take(&mut self.focus_search)
                    || (self.leave_search && !self.field_events.is_empty())
                {
                    // Before drawing, so the field reads the rest of this batch.
                    ui.memory_mut(|memory| memory.request_focus(id));
                }
                ui.horizontal(|ui| {
                    // Its accessible name is the visible label.
                    let label = ui
                        .label(egui::RichText::new("Search actions").color(super::style::LAVENDER));
                    let owned = std::mem::take(&mut self.field_events);
                    let start = ui.input(|input| input.events.len());
                    ui.ctx().input_mut(|input| input.events.extend(owned));
                    let field = ui
                        .add(
                            egui::TextEdit::singleline(&mut self.query)
                                .id(id)
                                .hint_text("Search by name, key (dd, ,h) or command (:hold)")
                                .desired_width(f32::INFINITY),
                        )
                        .labelled_by(label.id);
                    ui.ctx().input_mut(|input| input.events.truncate(start));
                    if std::mem::take(&mut self.leave_search) {
                        field.surrender_focus();
                    }
                    self.field_focused = field.has_focus();
                });
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
                let query = self.query.clone();
                if query != self.shown_query {
                    area = area.vertical_scroll_offset(0.0);
                    self.shown_query.clone_from(&query);
                }
                let output = area.show(ui, |ui| contents(ui, &query));
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
        assert!(!scroll.route_events(&mut events, false));
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
            assert!(!scroll.route_events(&mut events, false));
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
        assert!(HelpScroll::default().route_events(&mut events, false));
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
                    event(Key::Enter, false, Modifiers::NONE),
                    event(Key::Backspace, false, Modifiers::NONE),
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
                match scroll.next_event(&mut open, &mut events, false) {
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
            assert!(HelpScroll::default().route_events(&mut events, false));
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
        assert!(!HelpScroll::default().route_events(&mut events, false));
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
                            match self
                                .scroll
                                .next_event(&mut self.help_open, &mut events, false)
                            {
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
                    self.scroll.show(context, &mut self.help_open, |ui, _| {
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
                    event(Key::Backspace, false, Modifiers::NONE),
                    event(Key::Enter, false, Modifiers::NONE),
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
                .show(ui.ctx(), &mut open, |ui, _| {
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

    fn key_event(key: Key, modifiers: Modifiers) -> Event {
        event(key, false, modifiers)
    }

    /// One frame in the application's order: route, then draw the sheet.
    fn search_frame(
        context: &egui::Context,
        scroll: &mut HelpScroll,
        open: &mut bool,
        events: Vec<Event>,
    ) -> bool {
        let mut closed = false;
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 720.0),
                )),
                events,
                ..Default::default()
            },
            |_| {
                let focused = context.memory(|m| m.has_focus(egui::Id::new(SEARCH_ID)));
                closed = context.input_mut(|input| scroll.route_events(&mut input.events, focused));
                *open &= !closed;
                scroll.show(context, open, |ui, query| {
                    ui.label(format!("query {query}"));
                });
            },
        );
        output.textures_delta.clear();
        closed
    }

    #[test]
    fn slash_searches_and_escape_leaves_the_field_before_closing_help() {
        let context = egui::Context::default();
        let mut scroll = HelpScroll::default();
        let mut open = true;
        search_frame(&context, &mut scroll, &mut open, vec![]);
        // The opener and the text typed after it arrive in one batch.
        search_frame(
            &context,
            &mut scroll,
            &mut open,
            vec![
                key_event(Key::J, Modifiers::NONE),
                key_event(Key::Slash, Modifiers::NONE),
                Event::Text("/".into()),
                Event::Text("dd".into()),
            ],
        );
        assert_eq!(scroll.query(), "dd");
        assert!(context.memory(|m| m.has_focus(egui::Id::new(SEARCH_ID))));
        // Scroll keys are text while the field owns the keyboard.
        search_frame(
            &context,
            &mut scroll,
            &mut open,
            vec![key_event(Key::K, Modifiers::NONE), Event::Text("k".into())],
        );
        assert_eq!(scroll.query(), "ddk");
        assert!(!search_frame(
            &context,
            &mut scroll,
            &mut open,
            vec![key_event(Key::Escape, Modifiers::NONE)]
        ));
        assert!(open, "the first Escape leaves the field");
        assert!(!context.memory(|m| m.has_focus(egui::Id::new(SEARCH_ID))));
        assert_eq!(scroll.query(), "ddk", "the filter stays");
        assert!(search_frame(
            &context,
            &mut scroll,
            &mut open,
            vec![key_event(Key::Escape, Modifiers::NONE)]
        ));
        assert!(!open);
        assert_eq!(scroll.query(), "", "closing clears the search");
    }

    #[test]
    fn a_shifted_slash_layout_reaches_search_by_its_typed_character() {
        // QWERTZ types `/` with Shift+7.
        let mut scroll = HelpScroll::default();
        let mut events = vec![
            key_event(Key::Num7, Modifiers::SHIFT),
            Event::Text("/".into()),
            Event::Text("hold".into()),
        ];
        assert!(!scroll.route_events(&mut events, false));
        assert!(scroll.focus_search);
        assert!(
            events.is_empty(),
            "no widget drawn before the field sees its text"
        );
        assert_eq!(scroll.field_events, vec![Event::Text("hold".into())]);
        // Command chords never open search.
        let mut scroll = HelpScroll::default();
        let mut events = vec![key_event(Key::Slash, Modifiers::COMMAND)];
        assert!(!scroll.route_events(&mut events, false));
        assert!(!scroll.focus_search);
    }

    #[test]
    fn escape_after_slash_in_one_batch_leaves_search_and_routes_the_rest_to_help() {
        let context = egui::Context::default();
        let mut scroll = HelpScroll::default();
        let mut open = true;
        search_frame(&context, &mut scroll, &mut open, vec![]);
        let closed = search_frame(
            &context,
            &mut scroll,
            &mut open,
            vec![
                key_event(Key::Slash, Modifiers::NONE),
                Event::Text("/".into()),
                Event::Text("a".into()),
                key_event(Key::Escape, Modifiers::NONE),
                key_event(Key::Colon, Modifiers::SHIFT),
                Event::Text(":".into()),
            ],
        );
        assert!(!closed && open, "the Escape leaves the field, not Help");
        assert_eq!(scroll.query(), "a", "text before the Escape is kept");
        assert!(!context.memory(|m| m.has_focus(egui::Id::new(SEARCH_ID))));
        // A later Escape in the same batch closes Help and keeps its suffix.
        let mut scroll = HelpScroll::default();
        let mut events = vec![
            key_event(Key::Slash, Modifiers::NONE),
            Event::Text("/".into()),
            key_event(Key::Escape, Modifiers::NONE),
            key_event(Key::Escape, Modifiers::NONE),
            key_event(Key::Colon, Modifiers::SHIFT),
        ];
        assert!(scroll.route_events(&mut events, false));
        assert_eq!(events, vec![key_event(Key::Colon, Modifiers::SHIFT)]);
    }

    #[test]
    fn a_held_slash_never_echoes_into_the_search_field() {
        let mut scroll = HelpScroll::default();
        let mut events = vec![
            key_event(Key::Slash, Modifiers::NONE),
            Event::Text("/".into()),
            event(Key::Slash, true, Modifiers::NONE),
            Event::Text("/".into()),
            event(Key::Slash, true, Modifiers::NONE),
            Event::Text("/".into()),
            Event::Text("x".into()),
        ];
        assert!(!scroll.route_events(&mut events, false));
        assert_eq!(scroll.field_events, vec![Event::Text("x".into())]);
    }
}
