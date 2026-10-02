use super::*;

#[test]
fn frame_cut_counts_are_exact_positive_and_never_repeat_on_a_held_key() {
    for (digits, count) in [("", 1), ("1", 1), ("12", 12), ("4294967295", u32::MAX)] {
        let mut bindings = Bindings::default();
        for digit in digits.bytes() {
            let key = DIGITS[usize::from(digit - b'0')].0;
            assert_eq!(bindings.key(key, Modifiers::NONE, false, false), None);
        }
        assert_eq!(
            bindings.key(Key::X, Modifiers::NONE, false, false),
            Some(Action::DeleteFrames(count))
        );
        assert!(bindings.pending().is_empty());
    }
    for digits in ["0", "00", "4294967296", "999999999999999999"] {
        let mut bindings = Bindings::default();
        for digit in digits.bytes() {
            bindings.key(
                DIGITS[usize::from(digit - b'0')].0,
                Modifiers::NONE,
                false,
                false,
            );
        }
        assert!(matches!(
            bindings.key(Key::X, Modifiers::NONE, false, false),
            Some(Action::Invalid(_))
        ));
        assert!(bindings.pending().is_empty());
    }
    assert!(!allows_key_repeat(Key::X, Modifiers::NONE));
}

#[test]
fn frame_cut_intent_survives_visual_state_but_not_prefixes_or_native_text_chords() {
    for selection in [EditSelection::Empty, EditSelection::Range] {
        let mut bindings = Bindings::default();
        assert_eq!(
            bindings.key_with_selection(Key::X, Modifiers::NONE, false, false, selection),
            Some(Action::DeleteFrames(1))
        );
        assert!(bindings.pending().is_empty());
    }
    for prefix in [Key::D, Key::R, Key::Comma, Key::G] {
        let mut bindings = Bindings::default();
        bindings.key(prefix, Modifiers::NONE, false, false);
        assert!(!matches!(
            bindings.key(Key::X, Modifiers::NONE, false, false),
            Some(Action::DeleteFrames(_))
        ));
        assert!(bindings.pending().is_empty());
    }
    for (text, ime) in [(true, false), (false, true), (true, true)] {
        let mut bindings = Bindings::default();
        bindings.key(Key::Num2, Modifiers::NONE, false, false);
        assert_eq!(bindings.key(Key::X, Modifiers::NONE, text, ime), None);
        assert!(bindings.pending().is_empty());
    }
    for modifiers in [
        Modifiers::ALT,
        Modifiers::SHIFT,
        Modifiers::CTRL,
        Modifiers::COMMAND,
        Modifiers::MAC_CMD,
    ] {
        let mut bindings = Bindings::default();
        bindings.key(Key::Num2, Modifiers::NONE, false, false);
        assert_eq!(bindings.key(Key::X, modifiers, false, false), None);
        assert!(bindings.pending().is_empty());
    }
}

#[test]
fn visual_delete_preserves_native_guards_and_rejects_counts() {
    for selection in [EditSelection::Empty, EditSelection::Range] {
        assert_eq!(
            Bindings::default().key_with_selection(
                Key::D,
                Modifiers::NONE,
                false,
                false,
                selection
            ),
            Some(Action::DeleteSelection)
        );
        for (text, ime) in [(true, false), (false, true), (true, true)] {
            assert_eq!(
                Bindings::default().key_with_selection(
                    Key::D,
                    Modifiers::NONE,
                    text,
                    ime,
                    selection
                ),
                None
            );
        }
        for modifiers in [
            Modifiers::ALT,
            Modifiers::SHIFT,
            Modifiers::CTRL,
            Modifiers::COMMAND,
            Modifiers::MAC_CMD,
        ] {
            assert_eq!(
                Bindings::default().key_with_selection(Key::D, modifiers, false, false, selection),
                None
            );
            assert!(!allows_key_repeat(Key::D, modifiers));
        }
        for count in [Key::Num0, Key::Num1, Key::Num2] {
            let mut bindings = Bindings::default();
            bindings.key_with_selection(count, Modifiers::NONE, false, false, selection);
            assert!(matches!(
                bindings.key_with_selection(Key::D, Modifiers::NONE, false, false, selection),
                Some(Action::Invalid(_))
            ));
            assert!(bindings.pending().is_empty());
        }
        assert!(!allows_key_repeat(Key::D, Modifiers::NONE));
    }
}

#[test]
fn whole_beat_prefix_remains_distinct_from_visual_delete() {
    let mut bindings = Bindings::default();
    assert_eq!(
        bindings.key(Key::D, Modifiers::NONE, false, false),
        Some(Action::OfferInsert)
    );
    assert_eq!(bindings.pending(), "d");
    assert_eq!(
        bindings.key(Key::D, Modifiers::NONE, false, false),
        Some(Action::Edit(BeatEdit::Delete))
    );
    assert!(bindings.pending().is_empty());
    // A previously armed operator can resolve at most one action. The app's
    // deletion capture still gives its explicit Visual range precedence.
    bindings.key(Key::D, Modifiers::NONE, false, false);
    assert_eq!(
        bindings.key_with_selection(Key::D, Modifiers::NONE, false, false, EditSelection::Range),
        Some(Action::Edit(BeatEdit::Delete))
    );
    assert!(bindings.pending().is_empty());
}
