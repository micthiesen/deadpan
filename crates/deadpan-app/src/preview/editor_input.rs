//! Ordered boundary between an editor binding and newly focused native text.

use eframe::egui::{Event, Key, Modifiers};

#[derive(Default)]
pub(super) struct TextEntryGate {
    held: Option<Key>,
}

impl TextEntryGate {
    pub fn clear(&mut self) {
        self.held = None;
    }

    pub fn begin(
        &mut self,
        key: Key,
        physical_key: Option<Key>,
        modifiers: Modifiers,
        suffix: &[Event],
    ) -> Vec<Event> {
        self.held = Some(physical_key.unwrap_or(key));
        self.filter(field_suffix(key, modifiers, suffix).to_vec())
    }

    /// The held opener still belongs to the editor until release. Preserve all
    /// unrelated text, clipboard, composition and native modifier input. The
    /// physical opener stays owned when a Shift change alters its logical key;
    /// egui may then report a fresh press rather than a repeat.
    pub fn filter(&mut self, events: Vec<Event>) -> Vec<Event> {
        let mut output = Vec::with_capacity(events.len());
        let mut events = events.into_iter().peekable();
        while let Some(event) = events.next() {
            if matches!(
                event,
                Event::Ime(_) | Event::WindowFocused(false) | Event::PointerButton { .. }
            ) {
                self.clear();
            }
            if let Event::Key {
                key,
                physical_key,
                modifiers,
                pressed,
                ..
            } = &event
                && let Some(identity) = self.held
            {
                if modifiers.ctrl || modifiers.command || modifiers.mac_cmd {
                    self.clear();
                } else if physical_key.unwrap_or(*key) == identity {
                    if *pressed {
                        if printable(*key) && matches!(events.peek(), Some(Event::Text(_))) {
                            events.next();
                        }
                        continue;
                    }
                    self.clear();
                }
            }
            output.push(event);
        }
        output
    }
}

/// Keep only events before the first field submit/cancel. The caller dispatches
/// the returned suffix after closing the field, in the next outer UI frame.
pub(super) fn take_field_tail(events: &mut Vec<Event>, boundary: &Event) -> Vec<Event> {
    let Some(index) = events.iter().position(|event| event == boundary) else {
        return Vec::new();
    };
    let tail = events.split_off(index + 1);
    events.pop();
    tail
}

/// egui-winit emits a printable press as Key then its companion Text. Once that
/// press opens a field, only the later suffix belongs to native text editing.
/// A release or any other intervening event breaks the pair. Physical bindings
/// still use the delivered key here, since it describes the produced text.
pub(super) fn field_suffix(key: Key, modifiers: Modifiers, suffix: &[Event]) -> &[Event] {
    if !modifiers.ctrl
        && !modifiers.command
        && !modifiers.mac_cmd
        && printable(key)
        && matches!(suffix.first(), Some(Event::Text(_)))
    {
        &suffix[1..]
    } else {
        suffix
    }
}

fn printable(key: Key) -> bool {
    // name() is one ASCII character only for letters and digits. Arrow glyphs
    // from symbol_or_name() would incorrectly count as printable text here.
    key.name().len() == 1
        || matches!(
            key,
            Key::Space
                | Key::Colon
                | Key::Comma
                | Key::Backslash
                | Key::Slash
                | Key::Pipe
                | Key::Questionmark
                | Key::Exclamationmark
                | Key::OpenBracket
                | Key::CloseBracket
                | Key::OpenCurlyBracket
                | Key::CloseCurlyBracket
                | Key::Backtick
                | Key::Minus
                | Key::Period
                | Key::Plus
                | Key::Equals
                | Key::Semicolon
                | Key::Quote
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: Key, pressed: bool) -> Event {
        Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn consumes_only_the_adjacent_companion_and_preserves_later_same_key_text() {
        let suffix = [
            Event::Text("q".into()),
            key(Key::Q, false),
            Event::Text("command / :".into()),
            key(Key::Q, true),
            Event::Text("q".into()),
            key(Key::Q, false),
            key(Key::Enter, true),
        ];
        assert_eq!(field_suffix(Key::Q, Modifiers::NONE, &suffix), &suffix[1..]);
    }

    #[test]
    fn layout_text_is_not_guessed_from_an_ascii_physical_position() {
        let suffix = [Event::Text("ä".into()), Event::Text(":".into())];
        assert_eq!(field_suffix(Key::Q, Modifiers::NONE, &suffix), &suffix[1..]);
    }

    #[test]
    fn nonprintable_entry_and_intervening_events_preserve_native_input() {
        let text = [Event::Text("command".into())];
        for opener in [Key::F2, Key::Enter, Key::ArrowLeft, Key::Home, Key::Tab] {
            assert_eq!(field_suffix(opener, Modifiers::NONE, &text), &text);
        }
        let released = [key(Key::Q, false), Event::Text("q".into())];
        assert_eq!(field_suffix(Key::Q, Modifiers::NONE, &released), &released);
        for suffix in [
            vec![Event::Paste("q".into())],
            vec![Event::Ime(eframe::egui::ImeEvent::Commit("q".into()))],
        ] {
            assert_eq!(field_suffix(Key::Q, Modifiers::NONE, &suffix), &suffix);
        }
    }

    #[test]
    fn native_command_chords_have_no_printable_companion() {
        let suffix = [Event::Text("subsequent text".into())];
        for modifiers in [Modifiers::CTRL, Modifiers::COMMAND, Modifiers::MAC_CMD] {
            assert_eq!(field_suffix(Key::Q, modifiers, &suffix), &suffix);
        }
    }

    #[test]
    fn held_opener_owns_only_its_repeats_until_an_ordered_release() {
        let mut gate = TextEntryGate::default();
        assert!(
            gate.begin(Key::C, Some(Key::C), Modifiers::NONE, &[])
                .is_empty()
        );
        let repeat = Event::Key {
            key: Key::C,
            physical_key: Some(Key::C),
            modifiers: Modifiers::NONE,
            pressed: true,
            repeat: true,
        };
        let events = vec![
            repeat.clone(),
            Event::Text("c".into()),
            Event::Text("/ :".into()),
            key(Key::C, false),
            repeat,
            Event::Text("c".into()),
        ];
        assert_eq!(gate.filter(events.clone()), events[2..]);
    }

    #[test]
    fn held_function_opener_never_owns_unpaired_text() {
        let mut gate = TextEntryGate::default();
        gate.begin(Key::F2, Some(Key::F2), Modifiers::NONE, &[]);
        let events = vec![
            Event::Key {
                key: Key::F2,
                physical_key: Some(Key::F2),
                modifiers: Modifiers::NONE,
                pressed: true,
                repeat: true,
            },
            Event::Text("command".into()),
        ];
        assert_eq!(gate.filter(events.clone()), events[1..]);
    }

    #[test]
    fn shifted_opener_keeps_its_physical_identity_until_release() {
        // egui derives repeat from logical keys. Releasing Shift can change
        // Colon to Semicolon and make the same held physical key look fresh.
        for repeat in [false, true] {
            let mut gate = TextEntryGate::default();
            gate.begin(Key::Colon, Some(Key::Semicolon), Modifiers::SHIFT, &[]);
            let events = vec![
                Event::Key {
                    key: Key::Semicolon,
                    physical_key: Some(Key::Semicolon),
                    modifiers: Modifiers::NONE,
                    pressed: true,
                    repeat,
                },
                Event::Text(";".into()),
                key(Key::Semicolon, false),
                key(Key::Semicolon, true),
                Event::Text(";".into()),
            ];
            assert_eq!(gate.filter(events.clone()), events[2..]);
        }
    }

    #[test]
    fn a_field_gets_only_input_before_its_first_submit_or_cancel() {
        for action in [Key::Enter, Key::Escape] {
            let boundary = key(action, true);
            let before = vec![Event::Text("source".into())];
            let after = vec![
                key(action, false),
                Event::Text("later native text".into()),
                boundary.clone(),
                Event::Paste("later clipboard".into()),
            ];
            let mut events = before.clone();
            events.push(boundary.clone());
            events.extend(after.clone());
            assert_eq!(take_field_tail(&mut events, &boundary), after);
            assert_eq!(events, before);
        }
    }

    #[test]
    fn composition_and_native_chords_revoke_the_gate_without_consuming_input() {
        for native in [
            Event::Ime(eframe::egui::ImeEvent::Preedit {
                text: "candidate".into(),
                active_range_chars: None,
            }),
            Event::Key {
                key: Key::C,
                physical_key: Some(Key::C),
                modifiers: Modifiers::COMMAND,
                pressed: true,
                repeat: true,
            },
        ] {
            let mut gate = TextEntryGate::default();
            gate.begin(Key::C, Some(Key::C), Modifiers::NONE, &[]);
            let events = vec![native, Event::Text("c".into())];
            assert_eq!(gate.filter(events.clone()), events);
            assert!(gate.held.is_none());
        }
    }
}
