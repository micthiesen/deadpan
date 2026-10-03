use super::*;

fn press(bindings: &mut Bindings, key: Key) -> Option<Action> {
    bindings.key(key, Modifiers::NONE, false, false)
}

fn held(bindings: &mut Bindings, key: Key, selection: EditSelection) -> Option<Action> {
    bindings.route_event(
        key,
        Some(key),
        Modifiers::NONE,
        false,
        false,
        true,
        true,
        selection,
    )
}

#[test]
fn dot_routes_once_in_normal_and_visual_with_no_count_or_held_repeat() {
    assert_eq!(BindingId::RepeatLast.as_str(), "edit.repeat-last");
    for selection in [
        EditSelection::None,
        EditSelection::Empty,
        EditSelection::Range,
    ] {
        let mut bindings = Bindings::default();
        assert_eq!(bindings.key_label(BindingId::RepeatLast), ".");
        assert_eq!(
            bindings.key_with_selection(Key::Period, Modifiers::NONE, false, false, selection),
            Some(Action::RepeatLast)
        );
        assert!(bindings.pending().is_empty());
        assert!(!bindings.allows_key_repeat(Key::Period, Modifiers::NONE));
        assert_eq!(held(&mut bindings, Key::Period, selection), None);
        bindings.key_with_selection(Key::Comma, Modifiers::NONE, false, false, selection);
        assert_eq!(held(&mut bindings, Key::Period, selection), None);
        assert_eq!(bindings.pending(), ",");
        for digits in ["0", "1", "12", "4294967296"] {
            bindings.clear();
            for digit in digits.bytes() {
                bindings.key_with_selection(
                    DIGITS[usize::from(digit - b'0')].0,
                    Modifiers::NONE,
                    false,
                    false,
                    selection,
                );
            }
            assert!(matches!(
                bindings.key_with_selection(Key::Period, Modifiers::NONE, false, false, selection),
                Some(Action::Invalid(_))
            ));
            assert!(bindings.pending().is_empty());
        }
    }
    assert!(!allows_key_repeat(Key::Period, Modifiers::NONE));
}

#[test]
fn dot_preserves_native_input_and_cannot_complete_unrelated_prefixes() {
    for selection in [
        EditSelection::None,
        EditSelection::Empty,
        EditSelection::Range,
    ] {
        for (text, ime) in [(true, false), (false, true), (true, true)] {
            let mut bindings = Bindings::default();
            press(&mut bindings, Key::Num2);
            assert_eq!(
                bindings.key_with_selection(Key::Period, Modifiers::NONE, text, ime, selection),
                None
            );
            assert!(bindings.pending().is_empty());
        }
        for modifiers in [
            Modifiers::SHIFT,
            Modifiers::ALT,
            Modifiers::SHIFT | Modifiers::ALT,
            Modifiers::CTRL,
            Modifiers::COMMAND,
            Modifiers::MAC_CMD,
        ] {
            let mut bindings = Bindings::default();
            assert_eq!(
                bindings.key_with_selection(Key::Period, modifiers, false, false, selection),
                None
            );
            assert!(!bindings.native_control_owns_cut(Key::Period, modifiers, selection));
        }
        for (prefix, modifiers) in [
            (Key::D, Modifiers::NONE),
            (Key::R, Modifiers::NONE),
            (Key::Comma, Modifiers::NONE),
            (Key::G, Modifiers::NONE),
            (Key::M, Modifiers::NONE),
            (Key::Quote, Modifiers::NONE),
            (Key::Quote, Modifiers::SHIFT),
        ] {
            let mut bindings = Bindings::default();
            // CutBeat has a pending path only in Normal mode.
            bindings.key(prefix, modifiers, false, false);
            assert!(!bindings.native_control_owns_cut(Key::Period, Modifiers::NONE, selection));
            assert!(matches!(
                bindings.key_with_selection(Key::Period, Modifiers::NONE, false, false, selection),
                None | Some(Action::Invalid(_))
            ));
            assert!(bindings.pending().is_empty());
        }
    }
}

#[test]
fn native_control_guard_covers_dot_and_configured_aliases_without_taking_names() {
    for selection in [
        EditSelection::None,
        EditSelection::Empty,
        EditSelection::Range,
    ] {
        assert!(Bindings::default().native_control_owns_cut(
            Key::Period,
            Modifiers::NONE,
            selection
        ));
        let template = Bindings::from_json(
            br#"{"version":1,"key_mode":"logical","bindings":[{"action":"edit.repeat-last","keys":[["a"]]}]}"#,
        )
        .unwrap();
        assert!(template.native_control_owns_cut(Key::A, Modifiers::NONE, selection));
        assert!(!template.native_control_owns_cut(Key::Period, Modifiers::NONE, selection));
        for (prefix, modifiers, expected) in [
            (Key::M, Modifiers::NONE, Action::SetMark('a')),
            (Key::Quote, Modifiers::NONE, Action::JumpMark('a')),
            (Key::Quote, Modifiers::SHIFT, Action::SelectRegister('a')),
        ] {
            let mut bindings = template.clone();
            bindings.key_with_selection(prefix, modifiers, false, false, selection);
            let pending = bindings.pending();
            assert!(!bindings.native_control_owns_cut(Key::A, Modifiers::NONE, selection));
            assert!(!bindings.native_control_owns_cut(Key::A, Modifiers::SHIFT, selection));
            assert_eq!(bindings.pending(), pending);
            assert_eq!(
                bindings.key_with_selection(Key::A, Modifiers::NONE, false, false, selection),
                Some(expected)
            );
        }
    }
}

#[test]
fn configured_dot_path_retains_policy_and_teaches_its_supported_scope() {
    let mut bindings = Bindings::from_json(
        br#"{"version":1,"key_mode":"logical","bindings":[{"action":"edit.repeat-last","keys":[["a","."]]}]}"#,
    )
    .unwrap();
    assert_eq!(bindings.key_label(BindingId::RepeatLast), "a.");
    assert_eq!(press(&mut bindings, Key::Period), None);
    assert_eq!(press(&mut bindings, Key::A), None);
    let hint = bindings.pending_hint().unwrap();
    assert!(hint.contains(". repeat the last committed cut or Repeat"));
    assert!(hint.contains("current Visual range, beat or motion"));
    assert!(bindings.native_control_owns_cut(Key::Period, Modifiers::NONE, EditSelection::None));
    assert_eq!(held(&mut bindings, Key::Period, EditSelection::None), None);
    assert_eq!(bindings.pending(), "a");
    assert_eq!(press(&mut bindings, Key::Period), Some(Action::RepeatLast));
    assert_eq!(held(&mut bindings, Key::Period, EditSelection::None), None);
    press(&mut bindings, Key::Num2);
    press(&mut bindings, Key::A);
    assert!(bindings.pending_hint().unwrap().contains("without a count"));
    assert!(matches!(
        press(&mut bindings, Key::Period),
        Some(Action::Invalid(_))
    ));
    assert!(bindings.pending().is_empty());
    let report = shortcut_audit::audit_bindings(&bindings).unwrap();
    assert!(report.passed(), "{report:#?}");
}

#[test]
fn dot_uses_the_declared_key_mode_without_a_physical_fallback() {
    let mut logical = Bindings::default();
    assert_eq!(
        logical.route_event(
            Key::Period,
            Some(Key::A),
            Modifiers::NONE,
            false,
            false,
            false,
            true,
            EditSelection::None,
        ),
        Some(Action::RepeatLast)
    );
    let mut physical =
        Bindings::from_json(br#"{"version":1,"key_mode":"physical","bindings":[]}"#).unwrap();
    for position in [None, Some(Key::A)] {
        assert_eq!(
            physical.route_event(
                Key::Period,
                position,
                Modifiers::NONE,
                false,
                false,
                false,
                true,
                EditSelection::None,
            ),
            None
        );
    }
    assert_eq!(
        physical.route_event(
            Key::A,
            Some(Key::Period),
            Modifiers::NONE,
            false,
            false,
            false,
            true,
            EditSelection::None,
        ),
        Some(Action::RepeatLast)
    );
}
