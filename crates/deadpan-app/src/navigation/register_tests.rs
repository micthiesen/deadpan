use super::*;

const LETTERS: [Key; 26] = [
    Key::A,
    Key::B,
    Key::C,
    Key::D,
    Key::E,
    Key::F,
    Key::G,
    Key::H,
    Key::I,
    Key::J,
    Key::K,
    Key::L,
    Key::M,
    Key::N,
    Key::O,
    Key::P,
    Key::Q,
    Key::R,
    Key::S,
    Key::T,
    Key::U,
    Key::V,
    Key::W,
    Key::X,
    Key::Y,
    Key::Z,
];

fn prefix(bindings: &mut Bindings) {
    assert_eq!(
        bindings.key(Key::Quote, Modifiers::SHIFT, false, false),
        None
    );
    assert_eq!(bindings.mark_prefix(), None);
}

fn repeat(bindings: &mut Bindings, key: Key, modifiers: Modifiers) -> Option<Action> {
    bindings.route_event(
        key,
        Some(key),
        modifiers,
        false,
        false,
        true,
        true,
        EditSelection::None,
    )
}

#[test]
fn register_names_outrank_editor_actions_without_capturing_a_mark() {
    for mode in ["logical", "physical"] {
        let bytes = format!(r#"{{"version":1,"key_mode":"{mode}","bindings":[]}}"#);
        let template = Bindings::from_json(bytes.as_bytes()).unwrap();
        for (index, key) in LETTERS.into_iter().enumerate() {
            for modifiers in [Modifiers::NONE, Modifiers::SHIFT] {
                for selection in [
                    EditSelection::None,
                    EditSelection::Empty,
                    EditSelection::Range,
                ] {
                    let mut bindings = template.clone();
                    assert_eq!(bindings.key_label(BindingId::RegisterSelect), "\"");
                    bindings.key_with_selection(
                        Key::Quote,
                        Modifiers::SHIFT,
                        false,
                        false,
                        selection,
                    );
                    assert_eq!(bindings.pending(), "\"");
                    assert_eq!(bindings.mark_prefix(), None);
                    assert!(!bindings.native_control_owns_cut(key, modifiers, selection));
                    assert_eq!(
                        bindings.key_with_selection(key, modifiers, false, false, selection),
                        Some(Action::SelectRegister(char::from(
                            b'a' + u8::try_from(index).unwrap()
                        )))
                    );
                    assert!(bindings.pending().is_empty());
                }
            }
        }
    }
    let mut bindings = Bindings::default();
    prefix(&mut bindings);
    assert_eq!(
        bindings.key(Key::Quote, Modifiers::SHIFT, false, false),
        Some(Action::SelectRegister('"'))
    );
    assert!(bindings.pending().is_empty());
}

#[test]
fn register_prefix_has_exact_teaching_and_never_times_out() {
    let mut bindings = Bindings::default();
    prefix(&mut bindings);
    for _ in 0..300 {
        assert_eq!(bindings.pending(), "\"");
        assert_eq!(
            bindings.pending_next_keys().as_deref(),
            Some("a–z / A–Z · \" · Esc")
        );
        assert_eq!(
            bindings.pending_hint().as_deref(),
            Some("a–z / A–Z selects a register · \" selects the unnamed register · Esc cancels")
        );
        assert_eq!(bindings.mark_prefix(), None);
    }
    assert_eq!(
        bindings.key(Key::A, Modifiers::NONE, false, false),
        Some(Action::SelectRegister('a'))
    );
}

#[test]
fn register_counts_are_rejected_before_and_available_after_selection() {
    for digits in ["0", "1", "3", "4294967295", "4294967296"] {
        let mut bindings = Bindings::default();
        for digit in digits.bytes() {
            bindings.key(
                DIGITS[usize::from(digit - b'0')].0,
                Modifiers::NONE,
                false,
                false,
            );
        }
        assert_eq!(
            bindings.key(Key::Quote, Modifiers::SHIFT, false, false),
            Some(Action::Invalid(
                "Select a register without a count; put the count after its name."
            ))
        );
        assert!(bindings.pending().is_empty());
        assert_eq!(bindings.mark_prefix(), None);
    }
    let mut bindings = Bindings::default();
    prefix(&mut bindings);
    assert_eq!(
        bindings.key(Key::A, Modifiers::NONE, false, false),
        Some(Action::SelectRegister('a'))
    );
    assert_eq!(bindings.key(Key::Num1, Modifiers::NONE, false, false), None);
    assert_eq!(bindings.key(Key::Num2, Modifiers::NONE, false, false), None);
    assert_eq!(bindings.pending(), "12");
    assert_eq!(
        bindings.key(Key::X, Modifiers::NONE, false, false),
        Some(Action::DeleteFrames(12))
    );
}

#[test]
fn invalid_register_suffix_never_executes_as_a_root_command() {
    for (key, modifiers) in [
        (Key::Quote, Modifiers::NONE),
        (Key::Num3, Modifiers::NONE),
        (Key::Space, Modifiers::NONE),
        (Key::Space, Modifiers::SHIFT),
        (Key::Enter, Modifiers::NONE),
        (Key::Backspace, Modifiers::NONE),
        (Key::Colon, Modifiers::NONE),
        (Key::Questionmark, Modifiers::SHIFT),
        (Key::Comma, Modifiers::NONE),
        (Key::ArrowLeft, Modifiers::NONE),
    ] {
        let mut bindings = Bindings::default();
        prefix(&mut bindings);
        assert!(
            matches!(
                bindings.key(key, modifiers, false, false),
                Some(Action::Invalid(_))
            ),
            "{key:?} {modifiers:?}"
        );
        assert!(bindings.pending().is_empty());
    }
    for first in [Key::G, Key::R, Key::D, Key::Comma] {
        let mut bindings = Bindings::default();
        bindings.key(first, Modifiers::NONE, false, false);
        assert!(matches!(
            bindings.key(Key::Quote, Modifiers::SHIFT, false, false),
            Some(Action::Invalid(_))
        ));
        assert!(bindings.pending().is_empty());
    }
}

#[test]
fn native_input_cancels_register_prefix_without_dispatching_a_name() {
    for (text, ime) in [(true, false), (false, true), (true, true)] {
        let mut bindings = Bindings::default();
        assert_eq!(bindings.key(Key::Quote, Modifiers::SHIFT, text, ime), None);
        assert!(bindings.pending().is_empty());
        prefix(&mut bindings);
        assert_eq!(bindings.key(Key::A, Modifiers::NONE, text, ime), None);
        assert!(bindings.pending().is_empty());
    }
    for modifiers in [
        Modifiers::ALT,
        Modifiers::CTRL,
        Modifiers::MAC_CMD,
        Modifiers::COMMAND,
        Modifiers::ALT | Modifiers::SHIFT,
    ] {
        let mut bindings = Bindings::default();
        prefix(&mut bindings);
        assert_eq!(bindings.key(Key::A, modifiers, false, false), None);
        assert!(bindings.pending().is_empty());
    }
    for (key, modifiers, expected) in [
        (Key::Escape, Modifiers::NONE, Action::Escape),
        (Key::Tab, Modifiers::NONE, Action::Pane { reverse: false }),
        (Key::Tab, Modifiers::SHIFT, Action::Pane { reverse: true }),
        (Key::O, Modifiers::COMMAND, Action::Open),
    ] {
        let mut bindings = Bindings::default();
        prefix(&mut bindings);
        assert_eq!(bindings.key(key, modifiers, false, false), Some(expected));
        assert!(bindings.pending().is_empty());
    }
}

#[test]
fn held_input_cannot_enter_complete_or_consume_register_selection() {
    let mut bindings = Bindings::default();
    assert_eq!(repeat(&mut bindings, Key::Quote, Modifiers::SHIFT), None);
    assert!(bindings.pending().is_empty());
    bindings.key(Key::H, Modifiers::NONE, false, false);
    prefix(&mut bindings);
    for (key, modifiers) in [
        (Key::Quote, Modifiers::SHIFT),
        (Key::H, Modifiers::NONE),
        (Key::A, Modifiers::SHIFT),
    ] {
        assert_eq!(repeat(&mut bindings, key, modifiers), None);
        assert!(!bindings.allows_key_repeat(key, modifiers));
        assert_eq!(bindings.pending(), "\"");
    }
    assert_eq!(
        bindings.key(Key::H, Modifiers::NONE, false, false),
        Some(Action::SelectRegister('h'))
    );
    assert_eq!(repeat(&mut bindings, Key::H, Modifiers::NONE), None);
    assert!(bindings.pending().is_empty());
}
