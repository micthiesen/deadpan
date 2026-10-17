//! Keyboard scrolling uses the last measured viewport and the actual pointer
//! offset. It never installs an offset on an otherwise pointer-driven frame.
//! `/` moves the keyboard to the sheet's search field, which then owns every
//! event until Escape leaves it, except the palette's keys: Up/Down and
//! Ctrl-N/Ctrl-P move the highlighted result and Enter runs it. A palette key
//! ends this frame's routing; the field keeps what preceded it, and what
//! follows resumes in order on the next outer frame, after the key has acted
//! on the results drawn for the updated search.

use eframe::egui::{self, Event, Key, Modifiers};

use crate::navigation::palette::Choice;
use crate::navigation::panels::{HelpKey, PaletteKey, help_key, help_search_key};
use crate::navigation::registry::mode_label;

const LINE: f32 = 28.0;
pub(super) const SEARCH_ID: &str = "keys-reference-search";

/// The key line's keycaps and labels, naming the field's own keys while it
/// owns the keyboard. Typing has no key of its own, so it has no keycap.
fn hint_parts(searching: bool) -> Vec<(Vec<&'static str>, &'static str)> {
    if searching {
        vec![
            (Vec::new(), "Type to filter"),
            (mode_label("help.choose").split(" · ").collect(), "choose"),
            (vec![mode_label("help.run")], "run"),
            (vec![mode_label("help.close")], "leave search"),
        ]
    } else {
        vec![
            (vec![mode_label("help.search")], "search"),
            (mode_label("help.scroll").split(" · ").collect(), "scroll"),
            (vec![mode_label("help.close")], "close"),
        ]
    }
}

/// The key line as one spoken and hovered sentence.
fn hint(searching: bool) -> String {
    if searching {
        format!(
            "Type to filter     {} choose     {} run     {} leave search",
            mode_label("help.choose"),
            mode_label("help.run"),
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
    /// The search results' highlight, routed key, chosen action and status.
    pub(super) palette: Palette,
    /// Input after a palette key, resumed in order on the next outer frame.
    deferred: Vec<Event>,
    /// The field holds an IME composition: Enter and arrows are its own.
    composing: bool,
}

/// The Keys sheet's search results as an action palette.
#[derive(Default)]
pub(super) struct Palette {
    /// The highlighted result, the first whenever the search changes.
    highlight: usize,
    /// The palette key routed this frame, applied to the drawn results.
    key: Option<PaletteKey>,
    /// That key was an autorepeat: move silently, without announcements.
    held: bool,
    /// Scroll the highlighted row into view when it is next drawn.
    reveal: bool,
    /// The action Enter or a click chose; the editor runs it next frame.
    run: Option<Choice>,
    /// Why the last choice did not run, shown under the field.
    status: Option<String>,
}

impl Palette {
    /// Apply this frame's palette key to the results drawn for the current
    /// search, in their drawn order.
    pub(super) fn settle(&mut self, ids: &[Choice]) {
        let count = ids.len();
        self.highlight = self.highlight.min(count.saturating_sub(1));
        let Some(key) = self.key.take() else {
            return;
        };
        if count == 0 {
            self.status = Some(
                "No action matches, so there is nothing to choose or run. Change the search, or press Esc.".into(),
            );
            return;
        }
        match key {
            PaletteKey::Next => self.highlight = (self.highlight + 1) % count,
            PaletteKey::Previous => self.highlight = (self.highlight + count - 1) % count,
            PaletteKey::Run => self.run = Some(ids[self.highlight]),
        }
        self.reveal = key != PaletteKey::Run;
        if key != PaletteKey::Run {
            self.status = None;
        }
    }

    pub(super) fn highlighted(&self) -> usize {
        self.highlight
    }

    /// Whether the highlight moved by a held key this frame.
    pub(super) fn held(&self) -> bool {
        self.held
    }

    /// Scroll the highlighted row into view once.
    pub(super) fn take_reveal(&mut self) -> bool {
        std::mem::take(&mut self.reveal)
    }

    /// A click runs its row, as Enter runs the highlighted one.
    pub(super) fn choose(&mut self, index: usize, id: Choice) {
        self.highlight = index;
        self.run = Some(id);
    }

    /// Keep Help open and say why the choice did not run.
    pub(super) fn explain(&mut self, message: String) {
        self.status = Some(message);
    }

    pub(super) fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }
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

/// Whether the field is composing after this event.
fn composing_after(event: &Event, composing: bool) -> bool {
    match event {
        Event::Ime(egui::ImeEvent::Preedit { text, .. }) => !text.is_empty(),
        Event::Ime(_) | Event::WindowFocused(false) => false,
        _ => composing,
    }
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
        for event in &field {
            self.composing = composing_after(event, self.composing);
        }
        self.field_events.extend(field);
        *events = rest;
    }

    /// The action Enter or a click chose, once.
    pub(super) fn take_run(&mut self) -> Option<Choice> {
        self.palette.run.take()
    }

    /// Whether a chosen action waits for the next frame.
    pub(super) fn run_pending(&self) -> bool {
        self.palette.run.is_some()
    }

    /// Input that followed a palette key, for the next outer frame.
    pub(super) fn take_deferred(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.deferred)
    }

    /// Stop routing at a palette key: input before it stays with its owner,
    /// the key acts on the results drawn this frame, and the rest waits.
    fn palette_boundary(&mut self, events: &mut Vec<Event>, index: usize) {
        let mut rest = events.split_off(index + 1);
        if let Some(Event::Key {
            key,
            modifiers,
            repeat,
            ..
        }) = events.pop()
        {
            self.palette.key = help_search_key(key, modifiers);
            self.palette.held = repeat;
        }
        self.deferred.append(&mut rest);
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
            // A palette key acts only outside a composition, which owns its
            // own Enter and arrows; Escape keeps its existing precedence.
            let mut composing = self.composing;
            let palette = events.iter().position(|event| {
                composing = composing_after(event, composing);
                !composing
                    && matches!(event, Event::Key { key, modifiers, pressed: true, .. }
                        if help_search_key(*key, *modifiers).is_some())
            });
            if let Some(index) = palette
                && escape.is_none_or(|escape| index < escape)
            {
                self.palette_boundary(events, index);
                self.take_field_events(events);
                return false;
            }
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
                Some(HelpKey::Close | HelpKey::Search | HelpKey::Run)
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
        if let Some(index) = boundary
            && matches!(&events[index], Event::Key { key, modifiers, .. }
                if help_key(*key, *modifiers) == Some(HelpKey::Run))
        {
            // Enter runs the highlighted result of a kept filter.
            self.palette_boundary(events, index);
            events.retain(|event| !keyboard_event(event));
            return false;
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
            events.retain(|event| !keyboard_event(event));
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
        contents: impl FnOnce(&mut egui::Ui, &str, &mut Palette) -> R,
    ) -> Option<egui::InnerResponse<Option<R>>> {
        if !*open {
            // However Help closed, nothing typed for it survives.
            self.query.clear();
            self.focus_search = false;
            self.field_events.clear();
            self.field_focused = false;
            self.leave_search = false;
            self.composing = false;
            self.palette = Palette::default();
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
                // Keycaps teach the keys as they look on the keyboard; the
                // whole line stays one accessible, live sentence.
                let searching = self.field_focused || self.focus_search;
                let line = ui
                    .horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        for (keys, label) in hint_parts(searching) {
                            if keys.is_empty() {
                                ui.label(egui::RichText::new(label).weak().size(11.0));
                            } else {
                                super::style::keys_hint(ui, &keys, label);
                            }
                        }
                    })
                    .response;
                let line = super::accessibility::full_text(line, &hint(searching));
                super::accessibility::live(&line, false);
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
                if self.query != self.shown_query {
                    // A changed search starts at its first result.
                    self.palette.highlight = 0;
                    self.palette.status = None;
                }
                if let Some(status) = self.palette.status() {
                    // Why the chosen action did not run, announced once.
                    let status =
                        ui.label(egui::RichText::new(status).color(super::style::LAVENDER));
                    super::accessibility::live(&status, false);
                }
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
                let palette = &mut self.palette;
                let output = area.show(ui, |ui| contents(ui, &query, palette));
                if self.palette.key.take().is_some() {
                    // No results were drawn: the sheet is not filtered.
                    self.palette.status = Some(format!(
                        "Press {} and type to search; {} then runs the highlighted action.",
                        mode_label("help.search"),
                        mode_label("help.run"),
                    ));
                }
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
                    self.scroll.show(context, &mut self.help_open, |ui, _, _| {
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
                .show(ui.ctx(), &mut open, |ui, _, _| {
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
                scroll.show(context, open, |ui, query, _| {
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

    /// Draw the sheet with stand-in results, as the app draws its matches.
    fn palette_frame(
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
                scroll.show(context, open, |ui, query, palette| {
                    if !query.is_empty() {
                        let ids: Vec<Choice> = ["first", "second", "third"]
                            .into_iter()
                            .filter(|id| id.contains(query))
                            .map(Choice::from)
                            .collect();
                        palette.settle(&ids);
                        ui.label(format!("{ids:?}"));
                    }
                });
            },
        );
        output.textures_delta.clear();
        closed
    }

    fn open_search(context: &egui::Context, scroll: &mut HelpScroll, open: &mut bool) {
        palette_frame(context, scroll, open, vec![]);
        palette_frame(
            context,
            scroll,
            open,
            vec![
                key_event(Key::Slash, Modifiers::NONE),
                Event::Text("/".into()),
            ],
        );
        assert!(context.memory(|m| m.has_focus(egui::Id::new(SEARCH_ID))));
    }

    #[test]
    fn palette_keys_end_the_batch_after_the_field_reads_its_prefix() {
        let context = egui::Context::default();
        let mut scroll = HelpScroll::default();
        let mut open = true;
        open_search(&context, &mut scroll, &mut open);
        // Text, Down, Enter and more text arrive in one batch. The field
        // reads "ir" first; Down then moves within its results; Enter and
        // the rest wait, in order, for the next outer frame.
        let suffix = vec![
            key_event(Key::Enter, Modifiers::NONE),
            Event::Text("x".into()),
        ];
        let mut batch = vec![
            Event::Text("ir".into()),
            key_event(Key::ArrowDown, Modifiers::NONE),
        ];
        batch.extend(suffix.clone());
        assert!(!palette_frame(&context, &mut scroll, &mut open, batch));
        assert!(open);
        assert_eq!(scroll.query(), "ir");
        assert_eq!(
            scroll.palette.highlighted(),
            1,
            "first, third: Down reaches third"
        );
        assert!(scroll.take_run().is_none());
        assert_eq!(scroll.take_deferred(), suffix);
        // Resumed next frame: Enter runs the highlighted result before "x"
        // reaches anything, and "x" waits again.
        assert!(!palette_frame(&context, &mut scroll, &mut open, suffix));
        assert_eq!(scroll.take_run(), Some("third".into()));
        assert_eq!(scroll.take_deferred(), vec![Event::Text("x".into())]);
        assert_eq!(scroll.query(), "ir", "nothing after Enter was typed");
    }

    #[test]
    fn palette_highlight_wraps_resets_with_the_search_and_explains_no_match() {
        let context = egui::Context::default();
        let mut scroll = HelpScroll::default();
        let mut open = true;
        open_search(&context, &mut scroll, &mut open);
        palette_frame(
            &context,
            &mut scroll,
            &mut open,
            vec![Event::Text("i".into())],
        );
        // Up from the first result wraps to the last; Ctrl-N wraps back.
        palette_frame(
            &context,
            &mut scroll,
            &mut open,
            vec![key_event(Key::ArrowUp, Modifiers::NONE)],
        );
        assert_eq!(scroll.palette.highlighted(), 1, "first, third");
        palette_frame(
            &context,
            &mut scroll,
            &mut open,
            vec![key_event(Key::N, Modifiers::CTRL)],
        );
        assert_eq!(scroll.palette.highlighted(), 0);
        palette_frame(
            &context,
            &mut scroll,
            &mut open,
            vec![key_event(Key::P, Modifiers::CTRL)],
        );
        assert_eq!(scroll.palette.highlighted(), 1);
        // A changed search starts at its first result.
        palette_frame(
            &context,
            &mut scroll,
            &mut open,
            vec![Event::Text("r".into())],
        );
        assert_eq!(scroll.query(), "ir");
        assert_eq!(scroll.palette.highlighted(), 0);
        // With no match, Enter runs nothing and says so; Help stays open.
        palette_frame(
            &context,
            &mut scroll,
            &mut open,
            vec![Event::Text("zz".into())],
        );
        palette_frame(
            &context,
            &mut scroll,
            &mut open,
            vec![key_event(Key::Enter, Modifiers::NONE)],
        );
        assert!(open && scroll.take_run().is_none());
        assert!(
            scroll
                .palette
                .status()
                .is_some_and(|status| status.contains("No action matches"))
        );
        // Plain arrows with modifiers other than Control stay with the field.
        let mut events = vec![key_event(Key::ArrowDown, Modifiers::SHIFT)];
        assert!(!scroll.route_events(&mut events, true));
        assert!(scroll.palette.key.is_none());
    }

    #[test]
    fn enter_on_a_kept_filter_runs_and_escape_still_leaves_search_first() {
        let context = egui::Context::default();
        let mut scroll = HelpScroll::default();
        let mut open = true;
        open_search(&context, &mut scroll, &mut open);
        // Typing, Escape (leaves the field), then Enter in one batch: the
        // Escape keeps Help and its filter, and Help's own Enter runs.
        palette_frame(
            &context,
            &mut scroll,
            &mut open,
            vec![
                Event::Text("sec".into()),
                key_event(Key::Escape, Modifiers::NONE),
                key_event(Key::Enter, Modifiers::NONE),
                key_event(Key::Colon, Modifiers::SHIFT),
            ],
        );
        assert!(open, "the first Escape leaves search, not Help");
        assert!(!context.memory(|m| m.has_focus(egui::Id::new(SEARCH_ID))));
        assert_eq!(scroll.query(), "sec");
        assert_eq!(scroll.take_run(), Some("second".into()));
        assert_eq!(
            scroll.take_deferred(),
            vec![key_event(Key::Colon, Modifiers::SHIFT)]
        );
        // Without a filter, Enter says how to search instead of doing nothing.
        let mut scroll = HelpScroll::default();
        let mut open = true;
        palette_frame(&context, &mut scroll, &mut open, vec![]);
        palette_frame(
            &context,
            &mut scroll,
            &mut open,
            vec![key_event(Key::Enter, Modifiers::NONE)],
        );
        assert!(open && scroll.take_run().is_none());
        assert!(
            scroll
                .palette
                .status()
                .is_some_and(|status| status.contains("type to search"))
        );
        // Escape after an unfiltered Enter still closes Help with its suffix.
        let mut events = vec![
            key_event(Key::Escape, Modifiers::NONE),
            key_event(Key::Colon, Modifiers::SHIFT),
        ];
        assert!(scroll.route_events(&mut events, false));
        assert_eq!(events, vec![key_event(Key::Colon, Modifiers::SHIFT)]);
    }

    #[test]
    fn a_composition_keeps_its_own_enter_and_arrows() {
        let mut scroll = HelpScroll::default();
        let mut events = vec![
            Event::Ime(egui::ImeEvent::Preedit {
                text: "か".into(),
                active_range_chars: None,
            }),
            key_event(Key::ArrowDown, Modifiers::NONE),
            key_event(Key::Enter, Modifiers::NONE),
            Event::Ime(egui::ImeEvent::Commit("か".into())),
            key_event(Key::Enter, Modifiers::NONE),
            Event::Text("z".into()),
        ];
        assert!(!scroll.route_events(&mut events, true));
        // The composition's keys and commit reach the field; the Enter after
        // the commit runs, and what follows it waits.
        assert_eq!(scroll.field_events.len(), 4);
        assert_eq!(scroll.palette.key, Some(PaletteKey::Run));
        assert_eq!(scroll.take_deferred(), vec![Event::Text("z".into())]);
        assert!(!scroll.composing);
    }

    #[test]
    fn a_click_chooses_its_row_and_closing_help_forgets_the_palette() {
        let mut palette = Palette::default();
        palette.choose(2, "third".into());
        assert_eq!(palette.highlighted(), 2);
        assert_eq!(palette.run, Some("third".into()));
        let context = egui::Context::default();
        let mut scroll = HelpScroll {
            palette,
            ..HelpScroll::default()
        };
        scroll.palette.explain("why".into());
        let mut open = false;
        palette_frame(&context, &mut scroll, &mut open, vec![]);
        assert!(scroll.take_run().is_none() && scroll.palette.status().is_none());
    }
    #[test]
    fn deferred_choice_retains_exact_sibling_after_results_change() {
        use crate::navigation::palette::Form;
        let undo = Choice {
            spec_id: "undo",
            form: Form::Command(0),
        };
        let redo = Choice {
            spec_id: "undo",
            form: Form::Command(1),
        };
        let mut palette = Palette {
            key: Some(PaletteKey::Next),
            ..Palette::default()
        };
        palette.settle(&[undo, redo]);
        palette.key = Some(PaletteKey::Run);
        palette.settle(&[undo, redo]);
        // A later filter/result pass cannot reinterpret the already chosen row.
        palette.settle(&[undo]);
        assert_eq!(palette.run, Some(redo));
        palette.choose(0, undo);
        assert_eq!(palette.run, Some(undo));
    }
}
