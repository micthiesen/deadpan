use super::*;

fn execute_prefix(bindings: &mut Bindings) -> Option<Action> {
    bindings.route_event_with_logical_text(
        Key::Num2,
        Some(Key::Num2),
        Modifiers::SHIFT,
        false,
        false,
        false,
        true,
        EditSelection::None,
        Some("@"),
    )
}

fn count(bindings: &mut Bindings, digits: &str) {
    for digit in digits.bytes() {
        bindings.key(
            DIGITS[usize::from(digit - b'0')].0,
            Modifiers::NONE,
            false,
            false,
        );
    }
}

#[test]
fn macro_names_are_named_case_insensitive_and_do_not_capture_marks() {
    for mode in ["logical", "physical"] {
        let bytes = format!(r#"{{"version":1,"key_mode":"{mode}","bindings":[]}}"#);
        let template = Bindings::from_json(bytes.as_bytes()).unwrap();
        for name in 'a'..='z' {
            let key = Key::from_name(&name.to_string()).unwrap();
            for modifiers in [Modifiers::NONE, Modifiers::SHIFT] {
                for record in [false, true] {
                    let mut bindings = template.clone();
                    if record {
                        assert_eq!(bindings.key(Key::Q, Modifiers::NONE, false, false), None);
                    } else {
                        assert_eq!(execute_prefix(&mut bindings), None);
                    }
                    assert!(bindings.macro_pending());
                    assert_eq!(bindings.mark_prefix(), None);
                    assert_eq!(
                        bindings.pending_next_keys().as_deref(),
                        Some("a–z / A–Z · Esc")
                    );
                    assert_eq!(
                        bindings.key(key, modifiers, false, false),
                        Some(if record {
                            Action::MacroRecord(name)
                        } else {
                            Action::MacroExecute {
                                register: name,
                                count: 1,
                            }
                        })
                    );
                    assert!(bindings.pending().is_empty());
                    assert!(!bindings.macro_pending());
                }
            }
        }
    }
    for record in [false, true] {
        let mut bindings = Bindings::default();
        if record {
            bindings.key(Key::Q, Modifiers::NONE, false, false);
        } else {
            execute_prefix(&mut bindings);
        }
        assert!(matches!(
            bindings.key(Key::Quote, Modifiers::SHIFT, false, false),
            Some(Action::Invalid(_))
        ));
        assert!(bindings.pending().is_empty());
    }
}

#[test]
fn logical_at_requires_its_text_companion_and_preserves_shifted_digit_counts() {
    let mut bindings = Bindings::default();
    assert_eq!(bindings.key_label(BindingId::MacroExecute), "@");
    assert_eq!(
        bindings.key(Key::Num2, Modifiers::SHIFT, false, false),
        None
    );
    assert_eq!(bindings.pending(), "2");
    assert!(!bindings.macro_pending());
    bindings.clear();
    // The text proves the symbol even on a layout with a different position.
    assert_eq!(
        bindings.route_event_with_logical_text(
            Key::G,
            Some(Key::G),
            Modifiers::ALT,
            false,
            false,
            false,
            true,
            EditSelection::None,
            Some("@"),
        ),
        None
    );
    assert_eq!(bindings.pending(), "@");
    assert!(bindings.macro_pending());
    assert_eq!(
        bindings.key(Key::A, Modifiers::NONE, false, false),
        Some(Action::MacroExecute {
            register: 'a',
            count: 1
        })
    );
    bindings.clear();
    assert_eq!(
        bindings.route_event_with_logical_text(
            Key::Num2,
            Some(Key::Num2),
            Modifiers::SHIFT,
            false,
            false,
            false,
            true,
            EditSelection::None,
            Some("not @"),
        ),
        None
    );
    assert_eq!(bindings.pending(), "2");
}

#[test]
fn macro_count_policy_rejects_zero_overflow_and_all_recording_counts() {
    for digits in ["0", "1", "3", "4294967295", "4294967296"] {
        let mut bindings = Bindings::default();
        count(&mut bindings, digits);
        assert!(matches!(
            bindings.key(Key::Q, Modifiers::NONE, false, false),
            Some(Action::Invalid(_))
        ));
        assert!(bindings.pending().is_empty());
        for recording in [false, true] {
            bindings.set_macro_recording(recording);
            count(&mut bindings, digits);
            let prefix = execute_prefix(&mut bindings);
            if digits == "0" || digits == "4294967296" {
                assert!(matches!(prefix, Some(Action::Invalid(_))));
                assert!(bindings.pending().is_empty());
            } else {
                assert_eq!(prefix, None);
                assert!(bindings.pending_hint().unwrap().contains(digits));
                assert_eq!(
                    bindings.key(Key::A, Modifiers::NONE, false, false),
                    Some(Action::MacroExecute {
                        register: 'a',
                        count: digits.parse().unwrap()
                    })
                );
            }
        }
        bindings.set_macro_recording(true);
        count(&mut bindings, digits);
        assert!(matches!(
            bindings.key(Key::Q, Modifiers::NONE, false, false),
            Some(Action::Invalid(_))
        ));
    }
    let mut bindings = Bindings::default();
    execute_prefix(&mut bindings);
    assert!(matches!(
        bindings.key(Key::Num3, Modifiers::NONE, false, false),
        Some(Action::Invalid(_))
    ));
    assert!(bindings.pending().is_empty());
}

#[test]
fn recording_mode_stops_on_the_complete_configured_prefix_and_clears_old_input() {
    let mut bindings = Bindings::default();
    bindings.key(Key::Q, Modifiers::NONE, false, false);
    assert!(bindings.macro_pending());
    bindings.set_macro_recording(true);
    assert!(bindings.pending().is_empty());
    assert_eq!(
        bindings.key(Key::Q, Modifiers::NONE, false, false),
        Some(Action::MacroStop)
    );
    assert!(!bindings.macro_pending());
    assert_eq!(bindings.key(Key::A, Modifiers::NONE, false, false), None);
    // No-op synchronization must preserve a user's pending path.
    execute_prefix(&mut bindings);
    bindings.set_macro_recording(true);
    assert_eq!(bindings.pending(), "@");
    bindings.set_macro_recording(false);
    assert!(bindings.pending().is_empty());

    let mut bindings = Bindings::from_json(br#"{"version":1,"key_mode":"logical","bindings":[{"action":"macro.record","keys":[["o","b"],["F2"]]},{"action":"macro.execute","keys":[["o","c"]]}]}"#).unwrap();
    assert_eq!(bindings.key_label(BindingId::MacroRecord), "ob");
    assert_eq!(bindings.key(Key::O, Modifiers::NONE, false, false), None);
    assert!(bindings.macro_pending());
    assert_eq!(bindings.key(Key::B, Modifiers::NONE, false, false), None);
    assert!(bindings.macro_pending());
    assert_eq!(
        bindings.key(Key::Z, Modifiers::SHIFT, false, false),
        Some(Action::MacroRecord('z'))
    );
    bindings.set_macro_recording(true);
    assert_eq!(bindings.key(Key::Q, Modifiers::NONE, false, false), None);
    assert_eq!(bindings.key(Key::O, Modifiers::NONE, false, false), None);
    assert!(
        bindings
            .pending_hint()
            .unwrap()
            .contains("b stops macro recording")
    );
    assert_eq!(
        bindings.key(Key::B, Modifiers::NONE, false, false),
        Some(Action::MacroStop)
    );
    assert_eq!(
        bindings.key(Key::F2, Modifiers::NONE, false, false),
        Some(Action::MacroStop)
    );
    bindings.set_macro_recording(false);
    count(&mut bindings, "3");
    bindings.key(Key::O, Modifiers::NONE, false, false);
    assert!(bindings.macro_pending());
    bindings.key(Key::C, Modifiers::NONE, false, false);
    assert_eq!(
        bindings.key(Key::A, Modifiers::NONE, false, false),
        Some(Action::MacroExecute {
            register: 'a',
            count: 3
        })
    );
}

#[test]
fn macros_preserve_native_text_composition_and_held_key_ownership() {
    for recording in [false, true] {
        for (text, ime, repeat) in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            for (key, modifiers, companion) in [
                (Key::Q, Modifiers::NONE, None),
                (Key::Num2, Modifiers::SHIFT, Some("@")),
            ] {
                let mut bindings = Bindings::default();
                bindings.set_macro_recording(recording);
                assert_eq!(
                    bindings.route_event_with_logical_text(
                        key,
                        Some(key),
                        modifiers,
                        text,
                        ime,
                        repeat,
                        true,
                        EditSelection::None,
                        companion
                    ),
                    None
                );
                assert!(bindings.pending().is_empty());
            }
        }
    }
    for prefix in [
        editor_map::Stroke::Key(Key::Q, false),
        editor_map::Stroke::At,
    ] {
        let mut bindings = Bindings::default();
        bindings.audit_stroke(prefix, EditSelection::None);
        let pending = bindings.pending();
        for key in [Key::A, Key::H, Key::Q] {
            assert_eq!(
                bindings.route_event(
                    key,
                    Some(key),
                    Modifiers::NONE,
                    false,
                    false,
                    true,
                    true,
                    EditSelection::None
                ),
                None
            );
            assert_eq!(bindings.pending(), pending);
        }
        assert_eq!(bindings.key(Key::A, Modifiers::NONE, true, false), None);
        assert!(bindings.pending().is_empty());
        bindings.audit_stroke(prefix, EditSelection::None);
        assert_eq!(bindings.key(Key::A, Modifiers::NONE, false, true), None);
        assert!(bindings.pending().is_empty());
    }
    let mut bindings = Bindings::default();
    bindings.key(Key::H, Modifiers::NONE, false, false);
    bindings.set_macro_recording(true);
    assert_eq!(
        bindings.route_event(
            Key::H,
            Some(Key::H),
            Modifiers::NONE,
            false,
            false,
            true,
            true,
            EditSelection::None
        ),
        None
    );
}

#[test]
fn macro_execution_is_destructive_for_native_controls_without_taking_names() {
    for selection in [
        EditSelection::None,
        EditSelection::Empty,
        EditSelection::Range,
    ] {
        let mut bindings = Bindings::default();
        execute_prefix(&mut bindings);
        assert!(bindings.native_control_owns_cut(Key::X, Modifiers::NONE, selection));
        assert_eq!(bindings.pending(), "@");
        for (prefix, modifiers) in [
            (Key::M, Modifiers::NONE),
            (Key::Quote, Modifiers::NONE),
            (Key::Quote, Modifiers::SHIFT),
            (Key::Q, Modifiers::NONE),
        ] {
            bindings.clear();
            bindings.key_with_selection(prefix, modifiers, false, false, selection);
            assert!(!bindings.native_control_owns_cut(Key::X, Modifiers::NONE, selection));
        }
    }
    let bindings = Bindings::from_json(br#"{"version":1,"key_mode":"logical","bindings":[{"action":"macro.execute","keys":[["o"]]},{"action":"cut.frames","keys":[["@"]]}]}"#).unwrap();
    assert!(bindings.native_control_owns_cut_event_with_logical_text(
        Key::Num2,
        Some(Key::Num2),
        Modifiers::SHIFT,
        EditSelection::None,
        Some("@"),
    ));
    assert!(!bindings.native_control_owns_cut_event(
        Key::Num2,
        Some(Key::Num2),
        Modifiers::SHIFT,
        EditSelection::None,
    ));
}

#[test]
fn native_interrupts_and_invalid_suffixes_clear_macro_prefixes() {
    for prefix in [
        editor_map::Stroke::Key(Key::Q, false),
        editor_map::Stroke::At,
    ] {
        for (key, modifiers, expected) in [
            (Key::Escape, Modifiers::NONE, Action::Escape),
            (Key::Tab, Modifiers::NONE, Action::Pane { reverse: false }),
            (Key::Tab, Modifiers::SHIFT, Action::Pane { reverse: true }),
            (Key::O, Modifiers::COMMAND, Action::Open),
        ] {
            let mut bindings = Bindings::default();
            bindings.audit_stroke(prefix, EditSelection::None);
            assert_eq!(bindings.key(key, modifiers, false, false), Some(expected));
            assert!(bindings.pending().is_empty());
        }
        for key in [
            Key::Space,
            Key::Enter,
            Key::Backspace,
            Key::Colon,
            Key::ArrowLeft,
        ] {
            let mut bindings = Bindings::default();
            bindings.audit_stroke(prefix, EditSelection::None);
            assert!(matches!(
                bindings.key(key, Modifiers::NONE, false, false),
                Some(Action::Invalid(_))
            ));
            assert!(bindings.pending().is_empty());
        }
    }
}

#[test]
fn macro_families_validate_expansion_and_logical_at_is_configurable() {
    for entries in [
        serde_json::json!([{"action":"macro.record","keys":[["a"]]},{"action":"trim","keys":[["a","t"]]}]),
        serde_json::json!([{"action":"macro.execute","keys":[vec!["a";16]]}]),
        serde_json::json!([{"action":"macro.record","keys":[["a"]]},{"action":"macro.execute","keys":[["a"]]}]),
    ] {
        let bytes = serde_json::to_vec(
            &serde_json::json!({"version":1,"key_mode":"logical","bindings":entries}),
        )
        .unwrap();
        assert!(Bindings::from_json(&bytes).is_err());
    }
    let mut bindings = Bindings::from_json(br#"{"version":1,"key_mode":"logical","bindings":[{"action":"macro.record","keys":[["@"]]},{"action":"macro.execute","keys":[["q"]]}]}"#).unwrap();
    execute_prefix(&mut bindings);
    assert_eq!(
        bindings.key(Key::A, Modifiers::NONE, false, false),
        Some(Action::MacroRecord('a'))
    );
    bindings.set_macro_recording(true);
    assert_eq!(execute_prefix(&mut bindings), Some(Action::MacroStop));
    let mut physical = Bindings::from_json(br#"{"version":1,"key_mode":"physical","bindings":[{"action":"macro.execute","keys":[["Shift+3"]]}]}"#).unwrap();
    assert_eq!(physical.key_label(BindingId::MacroExecute), "Shift+3");
    assert_eq!(
        physical.key(Key::Num3, Modifiers::SHIFT, false, false),
        None
    );
    assert!(physical.macro_pending());
    assert_eq!(
        physical.key(Key::A, Modifiers::NONE, false, false),
        Some(Action::MacroExecute {
            register: 'a',
            count: 1
        })
    );
    assert!(Bindings::from_json(br#"{"version":1,"key_mode":"physical","bindings":[{"action":"macro.execute","keys":[["@"]]}]}"#).is_err());
    assert!(Bindings::from_json(br#"{"version":1,"key_mode":"logical","bindings":[{"action":"macro.execute","keys":[["Shift+2"]]}]}"#).is_err());
}
