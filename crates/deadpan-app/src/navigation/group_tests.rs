use super::*;

fn key(bindings: &mut Bindings, key: Key, selection: EditSelection) -> Option<Action> {
    bindings.key_with_selection(key, Modifiers::NONE, false, false, selection)
}

#[test]
fn group_leader_teaches_named_entry_and_refuses_counts_or_held_activation() {
    for selection in [
        EditSelection::None,
        EditSelection::Empty,
        EditSelection::Range,
    ] {
        let mut bindings = Bindings::default();
        assert_eq!(bindings.key_label(BindingId::Group), ",g");
        assert!(bindings.key_label(BindingId::Ungroup).is_empty());
        key(&mut bindings, Key::Comma, selection);
        assert_eq!(bindings.pending(), ",");
        assert!(bindings.pending_hint().unwrap().contains("g name a group"));
        assert!(bindings.native_control_owns_cut(Key::G, Modifiers::NONE, selection));
        assert_eq!(
            bindings.route_event(
                Key::G,
                Some(Key::G),
                Modifiers::NONE,
                false,
                false,
                true,
                true,
                selection
            ),
            None
        );
        assert_eq!(bindings.pending(), ",");
        assert_eq!(key(&mut bindings, Key::G, selection), Some(Action::Group));
        assert!(bindings.pending().is_empty());
        for digits in ["0", "1", "12", "4294967296"] {
            for digit in digits.bytes() {
                key(
                    &mut bindings,
                    DIGITS[usize::from(digit - b'0')].0,
                    selection,
                );
            }
            key(&mut bindings, Key::Comma, selection);
            assert!(matches!(
                key(&mut bindings, Key::G, selection),
                Some(Action::Invalid(_))
            ));
            assert!(bindings.pending().is_empty());
        }
    }
}

#[test]
fn named_entry_keys_keep_native_text_ime_marks_and_reserved_modifiers() {
    for (text, ime) in [(true, false), (false, true), (true, true)] {
        let mut bindings = Bindings::default();
        key(&mut bindings, Key::Comma, EditSelection::Range);
        assert_eq!(
            bindings.key_with_selection(Key::G, Modifiers::NONE, text, ime, EditSelection::Range),
            None
        );
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
        key(&mut bindings, Key::Comma, EditSelection::None);
        assert_eq!(bindings.key(Key::G, modifiers, false, false), None);
        assert!(!bindings.native_control_owns_cut(Key::G, modifiers, EditSelection::None));
    }
    let mut bindings = Bindings::default();
    key(&mut bindings, Key::M, EditSelection::None);
    assert!(!bindings.native_control_owns_cut(Key::G, Modifiers::NONE, EditSelection::None));
    assert_eq!(
        key(&mut bindings, Key::G, EditSelection::None),
        Some(Action::SetMark('g'))
    );
}

#[test]
fn group_and_ungroup_remaps_share_count_and_native_control_policies() {
    for mode in ["logical", "physical"] {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "version":1, "key_mode":mode, "bindings":[
                {"action":"group.create", "keys":[["o","g"]]},
                {"action":"group.ungroup", "keys":[["o","u"]]}
            ]
        }))
        .unwrap();
        let template = Bindings::from_json(&bytes).unwrap();
        assert!(
            template
                .map
                .prefix_paths()
                .contains(&vec![editor_map::Stroke::Key(Key::O, false)])
        );
        for (terminal, action) in [(Key::G, Action::Group), (Key::U, Action::Ungroup)] {
            let mut bindings = template.clone();
            key(&mut bindings, Key::O, EditSelection::None);
            assert!(
                bindings
                    .pending_hint()
                    .unwrap()
                    .contains("ungroup a neutral Sequence")
            );
            assert!(bindings.native_control_owns_cut(
                terminal,
                Modifiers::NONE,
                EditSelection::None
            ));
            assert_eq!(
                key(&mut bindings, terminal, EditSelection::None),
                Some(action)
            );
            key(&mut bindings, Key::Num3, EditSelection::None);
            key(&mut bindings, Key::O, EditSelection::None);
            assert!(matches!(
                key(&mut bindings, terminal, EditSelection::None),
                Some(Action::Invalid(_))
            ));
        }
    }
}
