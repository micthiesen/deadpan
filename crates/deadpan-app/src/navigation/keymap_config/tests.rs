use super::*;
use crate::navigation::{Action, BeatEdit, Bindings, EditSelection, MarkPrefix};
use eframe::egui::Modifiers;

fn configured(entries: serde_json::Value) -> Bindings {
    let bytes = serde_json::to_vec(
        &serde_json::json!({"version":1,"key_mode":"logical","bindings":entries}),
    )
    .unwrap();
    Bindings::from_json(&bytes).unwrap()
}
fn press(bindings: &mut Bindings, key: Key) -> Option<Action> {
    bindings.key(key, Modifiers::NONE, false, false)
}
fn repeated(bindings: &mut Bindings, key: Key) -> Option<Action> {
    bindings.route_event(
        key,
        Some(key),
        Modifiers::NONE,
        false,
        false,
        true,
        true,
        EditSelection::None,
    )
}
fn release(bindings: &mut Bindings, key: Key) {
    bindings.route_event(
        key,
        Some(key),
        Modifiers::NONE,
        false,
        false,
        false,
        false,
        EditSelection::None,
    );
}
fn next(count: u32) -> Option<Action> {
    Some(Action::Step {
        forward: true,
        count,
    })
}

#[test]
fn aliases_replace_completely_and_swaps_compile_atomically() {
    let mut map = configured(serde_json::json!([
        {"action":"frame.next","keys":[["h"]]},
        {"action":"frame.previous","keys":[["l"]]}
    ]));
    assert_eq!(press(&mut map, Key::H), next(1));
    assert_eq!(
        press(&mut map, Key::L),
        Some(Action::Step {
            forward: false,
            count: 1
        })
    );
    assert_eq!(press(&mut map, Key::ArrowRight), None);
    assert_eq!(map.key_labels(BindingId::FrameNext), "h");
    assert_eq!(map.key_label(BindingId::First), "gg");
    assert_eq!(map.key_label(BindingId::Last), "G");
    assert_eq!(map.key_label(BindingId::Audition), "Shift+Space");
    assert_eq!(map.key_label(BindingId::Escape), "Esc");
}

#[test]
fn arbitrary_paths_can_include_former_outer_interrupts() {
    let mut map = configured(serde_json::json!([
        {"action":"frame.next","keys":[["a","Space","G",":"]]}
    ]));
    assert_eq!(press(&mut map, Key::A), None);
    assert!(map.pending_hint().unwrap().contains("Space …"));
    assert_eq!(press(&mut map, Key::Space), None);
    assert_eq!(map.key(Key::G, Modifiers::SHIFT, false, false), None);
    assert_eq!(press(&mut map, Key::Colon), next(1));
    assert!(map.pending().is_empty());
}

#[test]
fn multi_key_motion_latches_its_action_and_count_is_initial_only() {
    let mut map = configured(serde_json::json!([{"action":"frame.next","keys":[["a","h"]]}]));
    assert_eq!(
        press(&mut map, Key::H),
        Some(Action::Step {
            forward: false,
            count: 1
        })
    );
    press(&mut map, Key::A);
    assert_eq!(repeated(&mut map, Key::H), None);
    assert_eq!(map.pending(), "a");
    release(&mut map, Key::H);
    assert_eq!(press(&mut map, Key::H), next(1));
    assert_eq!(repeated(&mut map, Key::H), next(1));
    map.clear_pending();
    assert_eq!(repeated(&mut map, Key::H), next(1));
    release(&mut map, Key::H);
    assert_eq!(repeated(&mut map, Key::H), None);
    press(&mut map, Key::Num3);
    press(&mut map, Key::A);
    assert_eq!(press(&mut map, Key::H), next(3));
    assert_eq!(repeated(&mut map, Key::H), next(1));
    map.clear();
    assert_eq!(repeated(&mut map, Key::H), None);
}

#[test]
fn held_non_motion_never_reopens_an_action_or_consumes_a_prefix() {
    let mut map = configured(serde_json::json!([{"action":"hold","keys":[["e","b"]]}]));
    press(&mut map, Key::E);
    assert_eq!(repeated(&mut map, Key::B), None);
    assert_eq!(map.pending(), "e");
    assert!(matches!(
        press(&mut map, Key::B),
        Some(Action::Edit(BeatEdit::InsertHold(_)))
    ));
    assert_eq!(repeated(&mut map, Key::B), None);
    press(&mut map, Key::Comma);
    assert_eq!(repeated(&mut map, Key::V), None);
    assert_eq!(map.pending(), ",");
}

#[test]
fn mode_is_pinned_for_the_whole_pending_path() {
    let mut map = configured(serde_json::json!([
        {"action":"cut.beat","keys":[["a","b"]]},
        {"action":"cut.range","keys":[["a","c"]]}
    ]));
    press(&mut map, Key::A);
    assert_eq!(
        map.key_with_selection(Key::B, Modifiers::NONE, false, false, EditSelection::Range),
        Some(Action::Operator {
            cut: true,
            selector: deadpan_core::SemanticSelector::SelectedBeat
        })
    );
    map.key_with_selection(Key::A, Modifiers::NONE, false, false, EditSelection::Range);
    assert_eq!(press(&mut map, Key::C), Some(Action::DeleteSelection));
}

#[test]
fn capture_families_are_semantic_and_independent() {
    let mut map = configured(serde_json::json!([
        {"action":"trim","keys":[["e","t","v"],["F2"]]},
        {"action":"insert","keys":[["a","i"]]},
        {"action":"mark.set","keys":[["e","m"]]}
    ]));
    press(&mut map, Key::E);
    assert!(map.trim_pending());
    assert!(!map.reuse_pending());
    assert_eq!(map.mark_prefix(), None);
    press(&mut map, Key::T);
    assert!(map.trim_pending());
    assert_eq!(press(&mut map, Key::V), Some(Action::Trim));
    assert!(!map.trim_pending());
    assert_eq!(press(&mut map, Key::F2), Some(Action::Trim));
    press(&mut map, Key::E);
    press(&mut map, Key::M);
    assert!(!map.trim_pending());
    assert_eq!(map.mark_prefix(), Some(MarkPrefix::Set));
    assert_eq!(
        map.key(Key::ShiftLeft, Modifiers::SHIFT, false, false),
        None
    );
    assert_eq!(map.mark_prefix(), Some(MarkPrefix::Set));
    assert_eq!(
        map.key(Key::A, Modifiers::SHIFT, false, false),
        Some(Action::SetMark('A'))
    );
    press(&mut map, Key::A);
    assert!(map.reuse_pending());
    assert!(!map.trim_pending());
}

#[test]
fn expanded_mark_paths_reject_collisions_and_excess_length() {
    for entries in [
        serde_json::json!([{"action":"mark.set","keys":[["a"]]},{"action":"trim","keys":[["a","t"]]}]),
        serde_json::json!([{"action":"mark.set","keys":[vec!["a";16]]}]),
        serde_json::json!([{"action":"mark.set","keys":[["a"]]},{"action":"mark.jump","keys":[["a","b"]]}]),
    ] {
        let bytes = serde_json::to_vec(
            &serde_json::json!({"version":1,"key_mode":"logical","bindings":entries}),
        )
        .unwrap();
        assert!(Bindings::from_json(&bytes).is_err());
    }
}

#[test]
fn register_family_expands_custom_paths_and_keeps_mark_capture_separate() {
    let mut map = configured(serde_json::json!([
        {"action":"register.select","keys":[["F2","F3","F4","F5","F6","F7"]]},
        {"action":"mark.set","keys":[["e","m"]]},
        {"action":"hold","keys":[["e","b"]]}
    ]));
    let path = [Key::F2, Key::F3, Key::F4, Key::F5, Key::F6, Key::F7];
    for (index, key) in path.into_iter().enumerate() {
        assert_eq!(press(&mut map, key), None);
        assert_eq!(map.mark_prefix(), None);
        assert!(
            map.map.prefix_paths().contains(
                &path[..=index]
                    .iter()
                    .map(|key| Stroke::Key(*key, false))
                    .collect::<Vec<_>>()
            )
        );
    }
    assert_eq!(map.pending(), "F2 F3 F4 F5 F6 F7");
    assert!(map.pending_hint().unwrap().contains("unnamed register"));
    assert_eq!(
        map.key(Key::A, Modifiers::SHIFT, false, false),
        Some(Action::SelectRegister('a'))
    );
    for key in path {
        press(&mut map, key);
    }
    assert_eq!(
        map.key(Key::Quote, Modifiers::SHIFT, false, false),
        Some(Action::SelectRegister('"'))
    );
    assert_eq!(map.key(Key::Quote, Modifiers::SHIFT, false, false), None);
    assert!(
        map.pending().is_empty(),
        "replacement removes the shipped prefix"
    );
    press(&mut map, Key::E);
    press(&mut map, Key::M);
    assert_eq!(map.mark_prefix(), Some(MarkPrefix::Set));
    assert_eq!(press(&mut map, Key::A), Some(Action::SetMark('a')));
    press(&mut map, Key::E);
    assert!(matches!(
        press(&mut map, Key::B),
        Some(Action::Edit(BeatEdit::InsertHold(_)))
    ));
    let report = crate::navigation::shortcut_audit::audit_bindings(&map).unwrap();
    assert!(report.passed(), "{report:#?}");
}

#[test]
fn register_prefix_rejects_counts_collisions_and_overlong_expansion() {
    let template = configured(serde_json::json!([
        {"action":"register.select","keys":[["F2","F3"]]}
    ]));
    for digit in [Key::Num0, Key::Num1, Key::Num3] {
        let mut map = template.clone();
        press(&mut map, digit);
        press(&mut map, Key::F2);
        assert_eq!(
            map.pending_hint().as_deref(),
            Some("Select a register without a count; put the count after its name.")
        );
        assert!(matches!(press(&mut map, Key::F3), Some(Action::Invalid(_))));
        assert!(map.pending().is_empty());
    }
    for entries in [
        serde_json::json!([{"action":"register.select","keys":[vec!["F2";16]]}]),
        serde_json::json!([{"action":"register.select","keys":[["a"]]},{"action":"trim","keys":[["a","z"]]}]),
        serde_json::json!([{"action":"register.select","keys":[["a"]]},{"action":"trim","keys":[["a","\""]]}]),
        serde_json::json!([{"action":"register.select","keys":[["a"]]},{"action":"mark.set","keys":[["a","b"]]}]),
    ] {
        let bytes = serde_json::to_vec(
            &serde_json::json!({"version":1,"key_mode":"logical","bindings":entries}),
        )
        .unwrap();
        assert!(Bindings::from_json(&bytes).is_err());
    }
}

#[test]
fn double_quote_symbol_and_shift_quote_name_configure_the_same_path() {
    for mode in ["logical", "physical"] {
        for token in ["\"", "Shift+Quote"] {
            let bytes = serde_json::to_vec(&serde_json::json!({"version":1,"key_mode":mode,"bindings":[{"action":"register.select","keys":[[token]]}]})).unwrap();
            let mut map = Bindings::from_json(&bytes).unwrap();
            assert_eq!(map.key_label(BindingId::RegisterSelect), "\"");
            assert_eq!(map.key(Key::Quote, Modifiers::SHIFT, false, false), None);
            assert_eq!(press(&mut map, Key::X), Some(Action::SelectRegister('x')));
            assert_eq!(press(&mut map, Key::Quote), None);
            assert_eq!(map.mark_prefix(), Some(MarkPrefix::Jump));
        }
    }
}

#[test]
fn physical_mode_uses_positions_without_a_logical_fallback() {
    let mut map =
        Bindings::from_json(br#"{"version":1,"key_mode":"physical","bindings":[]}"#).unwrap();
    assert_eq!(map.key_mode_label(), "Physical positions");
    assert_eq!(
        map.route_event(
            Key::H,
            Some(Key::L),
            Modifiers::NONE,
            false,
            false,
            false,
            true,
            EditSelection::None
        ),
        next(1)
    );
    assert_eq!(
        map.route_event(
            Key::H,
            None,
            Modifiers::NONE,
            false,
            false,
            false,
            true,
            EditSelection::None
        ),
        None
    );
    assert_eq!(
        map.route_event(
            Key::Colon,
            Some(Key::Semicolon),
            Modifiers::SHIFT,
            false,
            false,
            false,
            true,
            EditSelection::None
        ),
        Some(Action::Command)
    );
    assert_eq!(
        map.route_event(
            Key::Questionmark,
            Some(Key::Slash),
            Modifiers::SHIFT,
            false,
            false,
            false,
            true,
            EditSelection::None
        ),
        Some(Action::Help)
    );
    assert_eq!(
        map.route_event(
            Key::Plus,
            Some(Key::Equals),
            Modifiers::SHIFT,
            false,
            false,
            false,
            true,
            EditSelection::None
        ),
        Some(Action::GainStep(3000))
    );
    assert!(map.native_control_owns_cut_event(
        Key::L,
        Some(Key::X),
        Modifiers::NONE,
        EditSelection::None
    ));
    assert!(!map.native_control_owns_cut_event(Key::X, None, Modifiers::NONE, EditSelection::None));
}

#[test]
fn all_reviewed_physical_reservations_outrank_different_logical_symbols() {
    let (_, reservations) = reservations::parse_fixture(reservations::FIXTURE).unwrap();
    for mode in ["logical", "physical"] {
        let bytes = format!(
            r#"{{"version":1,"key_mode":"{mode}","bindings":[{{"action":"trim","keys":[["e","a","v"]]}}]}}"#
        );
        let template = Bindings::from_json(bytes.as_bytes()).unwrap();
        for reservation in &reservations {
            for logical in [
                Key::Comma,
                Key::Period,
                Key::Colon,
                Key::N,
                Key::A,
                Key::Quote,
            ] {
                for prefix in [vec![], vec![Key::E], vec![Key::E, Key::A], vec![Key::M]] {
                    let mut map = template.clone();
                    for key in prefix {
                        press(&mut map, key);
                    }
                    assert_eq!(
                        map.route_event(
                            logical,
                            Some(reservation.key),
                            reservation.modifiers,
                            false,
                            false,
                            false,
                            true,
                            EditSelection::None
                        ),
                        None
                    );
                    assert!(map.pending().is_empty(), "{}", reservation.chord);
                }
            }
        }
    }
}

#[test]
fn complete_candidate_audit_enumerates_new_unannotated_branches() {
    let map = configured(serde_json::json!([{"action":"frame.next","keys":[["a","b","c","h"]]}]));
    let paths = map.map.prefix_paths();
    assert!(paths.contains(&vec![
        Stroke::Key(Key::A, false),
        Stroke::Key(Key::B, false),
        Stroke::Key(Key::C, false)
    ]));
    let report = crate::navigation::shortcut_audit::audit_bindings(&map).unwrap();
    assert!(report.passed(), "{report:#?}");
    assert!(
        report.routing_cases
            > crate::navigation::shortcut_audit::audit()
                .unwrap()
                .routing_cases
    );
}

#[test]
fn fixed_keys_counts_and_modified_native_chords_cannot_be_hidden_in_paths() {
    for keys in [
        serde_json::json!(["a", "Tab"]),
        serde_json::json!(["a", "Escape"]),
        serde_json::json!(["a", "3"]),
        serde_json::json!(["a", "Alt+h"]),
        serde_json::json!(["a", "Ctrl+o"]),
        serde_json::json!(["a", "Cmd+Enter"]),
        serde_json::json!(["a", "ShiftLeft"]),
    ] {
        let bytes = serde_json::to_vec(&serde_json::json!({"version":1,"key_mode":"logical","bindings":[{"action":"frame.next","keys":[keys]}]})).unwrap();
        assert!(Bindings::from_json(&bytes).is_err());
    }
    assert!(Bindings::from_json(br#"{"version":1,"key_mode":"logical","bindings":[{"action":"escape","keys":[["a"]]}]}"#).is_err());
}

#[test]
fn schema_rejects_unknown_duplicate_empty_or_malformed_entries() {
    for json in [
        r#"{"version":1,"version":1,"key_mode":"logical","bindings":[]}"#,
        r#"{"version":1,"key_mode":"logical","bindings":[],"extra":true}"#,
        r#"{"version":2,"key_mode":"logical","bindings":[]}"#,
        r#"{"version":1,"key_mode":"mixed","bindings":[]}"#,
        r#"{"version":1,"key_mode":"logical","bindings":[{"action":"wat","keys":[["a"]]}]}"#,
        r#"{"version":1,"key_mode":"logical","bindings":[{"action":"trim","keys":[]}]}"#,
        r#"{"version":1,"key_mode":"logical","bindings":[{"action":"trim","keys":[[]]}]}"#,
        r#"{"version":1,"key_mode":"logical","bindings":[{"action":"trim","keys":[["a"]]},{"action":"trim","keys":[["b"]]}]}"#,
        r#"{"version":1,"key_mode":"logical","bindings":[{"action":"trim","keys":[["a"],["a"]]}]}"#,
        r#"{"version":1,"key_mode":"logical","bindings":[{"action":"trim","action":"trim","keys":[["a"]]}]}"#,
    ] {
        assert!(Bindings::from_json(json.as_bytes()).is_err(), "{json}");
    }
}

#[test]
fn parse_errors_and_resource_limits_remain_bounded() {
    assert!(Bindings::from_json(&vec![b' '; MAX_FILE_BYTES + 1]).is_err());
    for entries in [
        serde_json::json!([{"action":"trim","keys":vec![vec!["a"];9]}]),
        serde_json::json!([{"action":"trim","keys":[vec!["a";17]]}]),
        serde_json::json!([{"action":"trim","keys":[["x".repeat(100_000)]]}]),
        serde_json::json!([{"action":"x".repeat(100_000),"keys":[["a"]]}]),
    ] {
        let bytes = serde_json::to_vec(
            &serde_json::json!({"version":1,"key_mode":"logical","bindings":entries}),
        )
        .unwrap();
        let error = Bindings::from_json(&bytes).err().unwrap();
        assert!(error.len() < 512);
    }
    let unknown = format!("{{\"{}\":1}}", "x".repeat(100_000));
    assert!(
        Bindings::from_json(unknown.as_bytes())
            .err()
            .unwrap()
            .chars()
            .count()
            < 256
    );
}

#[test]
fn custom_last_alias_inherits_primary_semantic_interrupt_policy() {
    let mut map = configured(serde_json::json!([{"action":"last","keys":[["e"]]}]));
    for _ in 0..11 {
        press(&mut map, Key::Num9);
    }
    press(&mut map, Key::Comma);
    assert_eq!(press(&mut map, Key::E), Some(Action::Last));
    assert!(map.pending().is_empty());
}

#[test]
fn native_focus_ime_and_escape_cancel_paths_and_repeat_authority() {
    let mut map = configured(serde_json::json!([{"action":"frame.next","keys":[["a","h"]]}]));
    for (text, ime) in [(true, false), (false, true)] {
        press(&mut map, Key::A);
        assert_eq!(
            map.route_event(
                Key::H,
                Some(Key::H),
                Modifiers::NONE,
                text,
                ime,
                false,
                true,
                EditSelection::None
            ),
            None
        );
        assert!(map.pending().is_empty());
        assert_eq!(repeated(&mut map, Key::H), None);
    }
    press(&mut map, Key::A);
    assert_eq!(press(&mut map, Key::Escape), Some(Action::Escape));
    assert_eq!(
        press(&mut map, Key::Tab),
        Some(Action::Pane { reverse: false })
    );
}

#[test]
fn logical_symbols_use_the_delivered_identity_across_shift_and_option() {
    let template = configured(serde_json::json!([
        {"action":"frame.next","keys":[["!"],["|"],["{"],["}"]]}
    ]));
    for key in [
        Key::Exclamationmark,
        Key::Pipe,
        Key::OpenCurlyBracket,
        Key::CloseCurlyBracket,
    ] {
        for modifiers in [
            Modifiers::NONE,
            Modifiers::SHIFT,
            Modifiers::ALT,
            Modifiers::ALT | Modifiers::SHIFT,
        ] {
            let mut map = template.clone();
            assert_eq!(
                map.route_event(
                    key,
                    Some(Key::F6),
                    modifiers,
                    false,
                    false,
                    false,
                    true,
                    EditSelection::None
                ),
                next(1)
            );
        }
    }
    for (key, expected) in [
        (Key::Comma, Some(Action::OfferInsert)),
        (Key::Quote, None),
        (Key::Colon, Some(Action::Command)),
        (Key::Slash, Some(Action::Search)),
        (Key::Questionmark, Some(Action::Help)),
    ] {
        for modifiers in [
            Modifiers::NONE,
            Modifiers::SHIFT,
            Modifiers::ALT,
            Modifiers::ALT | Modifiers::SHIFT,
        ] {
            let mut map = template.clone();
            assert_eq!(
                map.route_event(
                    key,
                    Some(Key::F6),
                    modifiers,
                    false,
                    false,
                    false,
                    true,
                    EditSelection::None
                ),
                expected
            );
        }
    }
    for modifiers in [Modifiers::NONE, Modifiers::SHIFT] {
        assert_eq!(
            template.clone().key(Key::Plus, modifiers, false, false),
            Some(Action::GainStep(3000))
        );
    }
    assert_eq!(
        template
            .clone()
            .key(Key::Plus, Modifiers::ALT, false, false),
        None
    );
    assert_eq!(
        template
            .clone()
            .key(Key::Minus, Modifiers::SHIFT, false, false),
        None
    );
}

#[test]
fn physical_numpad_add_is_named_separately_from_shift_equals() {
    let map = Bindings::from_json(br#"{"version":1,"key_mode":"physical","bindings":[{"action":"gain.up","keys":[["NumpadAdd"]]}]}"#).unwrap();
    assert_eq!(map.key_label(BindingId::GainUp), "NumpadAdd");
    for key in [
        "Colon",
        "Questionmark",
        "Pipe",
        "Exclamationmark",
        "OpenCurlyBracket",
        "CloseCurlyBracket",
        "BrowserBack",
    ] {
        let bytes = serde_json::to_vec(&serde_json::json!({"version":1,"key_mode":"physical","bindings":[{"action":"frame.next","keys":[[key]]}]})).unwrap();
        assert!(Bindings::from_json(&bytes).is_err(), "{key}");
    }
}

#[test]
fn compact_next_keys_share_count_and_nonterminal_filtering() {
    let mut map = configured(serde_json::json!([
        {"action":"hold","keys":[["e","a","h"]]},
        {"action":"trim","keys":[["e","t"]]}
    ]));
    press(&mut map, Key::Num3);
    press(&mut map, Key::E);
    assert_eq!(map.pending_next_keys().as_deref(), Some("a · Esc"));
    assert_eq!(map.pending_hint().as_deref(), Some("a … · Esc cancels"));
    map.clear();
    press(&mut map, Key::M);
    assert_eq!(map.pending_next_keys().as_deref(), Some("a–z / A–Z · Esc"));
}
