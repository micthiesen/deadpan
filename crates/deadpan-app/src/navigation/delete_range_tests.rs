use super::*;

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
