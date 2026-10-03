use super::*;
use deadpan_core::{SemanticMotion as Motion, SemanticSelector as Selector};
use std::num::NonZeroU32;

fn press(bindings: &mut Bindings, key: Key) -> Option<Action> {
    bindings.key(key, Modifiers::NONE, false, false)
}

fn digits(bindings: &mut Bindings, value: &str) {
    for digit in value.bytes() {
        assert_eq!(press(bindings, DIGITS[usize::from(digit - b'0')].0), None);
    }
}

fn repeat(selector: Selector, plays: u32) -> Action {
    Action::Repeat {
        selector,
        plays: NonZeroU32::new(plays).unwrap(),
    }
}

fn frames(forward: bool, count: u32) -> Selector {
    Selector::Motion {
        motion: Motion::Frames {
            forward,
            count: NonZeroU32::new(count).unwrap(),
        },
    }
}

fn configured(entries: serde_json::Value) -> Result<Bindings, String> {
    Bindings::from_json(
        &serde_json::to_vec(&serde_json::json!({
            "version":1, "key_mode":"logical", "bindings":entries
        }))
        .unwrap(),
    )
}

#[test]
fn leading_counts_are_total_plays_and_suffix_counts_are_motion_distance() {
    for (leading, suffix, plays, distance) in [
        ("", "", 2, 1),
        ("1", "", 1, 1),
        ("3", "", 3, 1),
        ("", "3", 2, 3),
        ("4294967295", "", u32::MAX, 1),
        ("", "4294967295", 2, u32::MAX),
    ] {
        for (key, forward, beats) in [
            (Key::H, false, false),
            (Key::L, true, false),
            (Key::K, false, true),
            (Key::J, true, true),
        ] {
            let mut bindings = Bindings::default();
            digits(&mut bindings, leading);
            press(&mut bindings, Key::R);
            assert_eq!(
                bindings.repeat_pending_scope(),
                Some(RepeatPendingScope::Mixed)
            );
            assert!(!bindings.operator_pending());
            digits(&mut bindings, suffix);
            let selector = if beats {
                Selector::Motion {
                    motion: Motion::Beats {
                        forward,
                        count: NonZeroU32::new(distance).unwrap(),
                    },
                }
            } else {
                frames(forward, distance)
            };
            assert_eq!(press(&mut bindings, key), Some(repeat(selector, plays)));
            assert!(!bindings.repeat_pending());
            assert!(bindings.pending().is_empty());
        }
    }
    for (leading, plays) in [("", 2), ("1", 1), ("3", 3)] {
        for (key, selector) in [
            (Key::R, Selector::SelectedBeat),
            (
                Key::Home,
                Selector::Motion {
                    motion: Motion::Scope { end: false },
                },
            ),
            (
                Key::End,
                Selector::Motion {
                    motion: Motion::Scope { end: true },
                },
            ),
        ] {
            let mut bindings = Bindings::default();
            digits(&mut bindings, leading);
            press(&mut bindings, Key::R);
            assert_eq!(press(&mut bindings, key), Some(repeat(selector, plays)));
        }
        for end in [false, true] {
            let mut bindings = Bindings::default();
            digits(&mut bindings, leading);
            press(&mut bindings, Key::R);
            if !end {
                press(&mut bindings, Key::G);
                assert_eq!(
                    bindings.repeat_pending_scope(),
                    Some(RepeatPendingScope::Motion)
                );
            }
            assert_eq!(
                bindings.key(
                    Key::G,
                    if end {
                        Modifiers::SHIFT
                    } else {
                        Modifiers::NONE
                    },
                    false,
                    false
                ),
                Some(repeat(
                    Selector::Motion {
                        motion: Motion::Scope { end }
                    },
                    plays
                ))
            );
        }
    }
}

#[test]
fn invalid_repeat_counts_stay_pending_until_the_terminal_is_consumed() {
    for (leading, suffix) in [
        ("0", ""),
        ("", "0"),
        ("4294967296", ""),
        ("", "4294967296"),
        ("3", "2"),
        ("3", "3"),
        ("1", "1"),
    ] {
        let mut bindings = Bindings::default();
        digits(&mut bindings, leading);
        press(&mut bindings, Key::R);
        digits(&mut bindings, suffix);
        assert!(bindings.repeat_pending());
        assert!(matches!(
            press(&mut bindings, Key::L),
            Some(Action::Invalid(_))
        ));
        assert!(!bindings.repeat_pending());
        assert!(bindings.pending().is_empty());
        assert_eq!(
            press(&mut bindings, Key::L),
            Some(Action::Step {
                forward: true,
                count: 1
            })
        );
    }
    for terminal in [Key::R, Key::Home, Key::End] {
        let mut bindings = Bindings::default();
        press(&mut bindings, Key::R);
        digits(&mut bindings, "1");
        assert!(matches!(
            press(&mut bindings, terminal),
            Some(Action::Invalid(_))
        ));
    }
    let mut bindings = Bindings::default();
    press(&mut bindings, Key::R);
    press(&mut bindings, Key::G);
    digits(&mut bindings, "2");
    assert!(bindings.repeat_pending());
    assert!(
        bindings
            .pending_hint()
            .unwrap()
            .contains("immediately after")
    );
    assert!(matches!(
        press(&mut bindings, Key::G),
        Some(Action::Invalid(_))
    ));
    assert!(bindings.pending().is_empty());
}

#[test]
fn visual_repeat_is_immediate_and_mode_changes_cannot_reinterpret_pending_input() {
    for selection in [EditSelection::Empty, EditSelection::Range] {
        for (leading, plays) in [("", 2), ("3", 3)] {
            let mut bindings = Bindings::default();
            digits(&mut bindings, leading);
            assert_eq!(
                bindings.key_with_selection(Key::R, Modifiers::NONE, false, false, selection),
                Some(repeat(Selector::VisualSelection, plays))
            );
            assert!(!bindings.repeat_pending());
            assert_eq!(
                bindings.key_label_in(BindingId::Repeat, RoutingDomain::Edit, selection),
                "r"
            );
        }
        for leading in ["0", "4294967296"] {
            let mut bindings = Bindings::default();
            digits(&mut bindings, leading);
            assert!(matches!(
                bindings.key_with_selection(Key::R, Modifiers::NONE, false, false, selection),
                Some(Action::Invalid(_))
            ));
            assert!(bindings.pending().is_empty());
        }
        let mut bindings = Bindings::default();
        press(&mut bindings, Key::R);
        assert_eq!(
            bindings.key_with_selection(Key::R, Modifiers::NONE, false, false, selection),
            Some(repeat(Selector::SelectedBeat, 2)),
            "the UI validates the captured original context"
        );
    }
    for domain in [RoutingDomain::Original, RoutingDomain::Sound] {
        let mut bindings = Bindings::default();
        press(&mut bindings, Key::R);
        bindings.set_routing_domain(domain);
        assert!(!bindings.repeat_pending());
        assert!(matches!(
            press(&mut bindings, Key::R),
            Some(Action::Invalid(_))
        ));
        press(&mut bindings, Key::R);
        assert!(!bindings.repeat_pending());
        assert_eq!(
            press(&mut bindings, Key::R),
            Some(Action::Edit(BeatEdit::WrapRepeat(2)))
        );
    }
}

#[test]
fn repeat_configuration_composes_motion_paths_and_reports_exact_pending_scope() {
    let mut bindings = configured(serde_json::json!([
        {"action":"repeat", "keys":[["f","r"]]},
        {"action":"repeat.operator", "keys":[["e","b"]]},
        {"action":"repeat.range", "keys":[["e","v"]]},
        {"action":"frame.next", "keys":[["a","l"]]}
    ]))
    .unwrap();
    press(&mut bindings, Key::E);
    assert_eq!(
        bindings.repeat_pending_scope(),
        Some(RepeatPendingScope::Motion)
    );
    press(&mut bindings, Key::B);
    digits(&mut bindings, "3");
    assert_eq!(bindings.pending(), "eb3");
    assert!(
        bindings
            .pending_next_keys()
            .unwrap()
            .split(" · ")
            .any(|key| key == "a")
    );
    press(&mut bindings, Key::A);
    assert_eq!(bindings.pending(), "eb3a");
    assert!(
        bindings
            .pending_hint()
            .unwrap()
            .contains("3 frame(s) forward, 2 total plays")
    );
    assert_eq!(
        press(&mut bindings, Key::L),
        Some(repeat(frames(true, 3), 2))
    );
    press(&mut bindings, Key::F);
    assert_eq!(
        bindings.repeat_pending_scope(),
        Some(RepeatPendingScope::SelectedBeat)
    );
    assert_eq!(
        press(&mut bindings, Key::R),
        Some(repeat(Selector::SelectedBeat, 2))
    );
    bindings.key_with_selection(Key::E, Modifiers::NONE, false, false, EditSelection::Range);
    assert_eq!(
        bindings.repeat_pending_scope(),
        Some(RepeatPendingScope::VisualSelection)
    );
    assert_eq!(
        bindings.key_with_selection(Key::V, Modifiers::NONE, false, false, EditSelection::None),
        Some(repeat(Selector::VisualSelection, 2))
    );
    assert_eq!(
        bindings.key_labels_in(BindingId::Repeat, RoutingDomain::Edit, EditSelection::Range),
        "ev"
    );
    for entries in [
        serde_json::json!([{"action":"repeat.operator", "keys":[["h"]]}]),
        serde_json::json!([{"action":"frame.next", "keys":[["r"]]}]),
        serde_json::json!([{"action":"repeat.range", "keys":[["v"]]}]),
    ] {
        assert!(configured(entries).is_err());
    }
}

#[test]
fn ancestor_repeat_aliases_keep_counts_at_the_completed_prefix() {
    for (path, leading, distance, expected_label, plays) in [
        (&[Key::E][..], "", "3", "e3", 2),
        (&[Key::E, Key::B][..], "", "3", "eb3", 2),
        (&[Key::E, Key::B][..], "3", "", "3eb", 3),
    ] {
        let mut bindings = configured(serde_json::json!([
            {"action":"repeat.operator", "keys":[["e"],["e","b"]]}
        ]))
        .unwrap();
        digits(&mut bindings, leading);
        for (index, key) in path.iter().enumerate() {
            assert_eq!(
                press(&mut bindings, *key),
                (index == 0).then_some(Action::OfferInsert)
            );
            assert!(bindings.repeat_pending());
            assert_eq!(
                bindings.pending(),
                format!("{leading}{}", if index == 0 { "e" } else { "eb" })
            );
        }
        digits(&mut bindings, distance);
        assert_eq!(bindings.pending(), expected_label);
        assert_eq!(
            press(&mut bindings, Key::L),
            Some(repeat(
                frames(true, if distance.is_empty() { 1 } else { 3 }),
                plays
            ))
        );
        assert!(bindings.pending().is_empty());
    }
}

#[test]
fn a_motion_count_cannot_cross_into_another_completed_operator_alias() {
    for first_count in ["3", "0", "4294967296"] {
        for second_count in ["", "4"] {
            let mut bindings = configured(serde_json::json!([
                {"action":"repeat.operator", "keys":[["e"],["e","b"]]}
            ]))
            .unwrap();
            assert_eq!(press(&mut bindings, Key::E), Some(Action::OfferInsert));
            digits(&mut bindings, first_count);
            assert_eq!(press(&mut bindings, Key::B), None);
            assert!(bindings.repeat_pending());
            digits(&mut bindings, second_count);
            assert!(bindings.repeat_pending());
            assert!(
                bindings
                    .pending_hint()
                    .unwrap()
                    .contains("complete operator prefix")
            );
            if first_count == "3" {
                assert_eq!(bindings.pending(), "e3b");
            }
            assert!(matches!(
                press(&mut bindings, Key::L),
                Some(Action::Invalid(_))
            ));
            assert!(bindings.pending().is_empty());
            assert_eq!(
                press(&mut bindings, Key::L),
                Some(Action::Step {
                    forward: true,
                    count: 1
                })
            );
            press(&mut bindings, Key::E);
            press(&mut bindings, Key::B);
            digits(&mut bindings, "2");
            assert_eq!(bindings.pending(), "eb2");
            assert_eq!(
                press(&mut bindings, Key::L),
                Some(repeat(frames(true, 2), 2))
            );
        }
    }
}

#[test]
fn repeat_teaching_distinguishes_plays_distance_and_unavailable_suffixes() {
    let mut bindings = Bindings::default();
    press(&mut bindings, Key::R);
    assert!(
        bindings
            .pending_hint()
            .unwrap()
            .contains("1–9 starts a motion count")
    );
    press(&mut bindings, Key::Escape);
    digits(&mut bindings, "3");
    press(&mut bindings, Key::R);
    let hint = bindings.pending_hint().unwrap();
    assert!(hint.contains("1 frame(s) backward, 3 total plays"));
    assert!(hint.contains("this whole beat, 3 total plays"));
    assert!(hint.contains("to the group end, 3 total plays"));
    assert!(!hint.contains("1–9"));
    press(&mut bindings, Key::Escape);
    press(&mut bindings, Key::R);
    digits(&mut bindings, "3");
    assert_eq!(bindings.pending(), "r3");
    let keys = bindings.pending_next_keys().unwrap();
    assert!(keys.split(" · ").any(|key| key == "l"));
    assert!(
        !keys
            .split(" · ")
            .any(|key| ["r", "g", "G", "Home", "End"].contains(&key))
    );
    assert!(
        bindings
            .pending_hint()
            .unwrap()
            .contains("3 frame(s) forward, 2 total plays")
    );
}

#[test]
fn native_input_and_held_keys_keep_ownership_during_repeat_paths() {
    for (text, ime) in [(true, false), (false, true), (true, true)] {
        let mut bindings = Bindings::default();
        press(&mut bindings, Key::R);
        assert_eq!(bindings.key(Key::L, Modifiers::NONE, text, ime), None);
        assert!(!bindings.repeat_pending());
    }
    let held = |bindings: &mut Bindings, key| {
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
    };
    let mut bindings = Bindings::default();
    assert_eq!(held(&mut bindings, Key::R), None);
    press(&mut bindings, Key::R);
    assert!(bindings.native_control_owns_cut(Key::L, Modifiers::NONE, EditSelection::None));
    assert_eq!(held(&mut bindings, Key::L), None);
    assert!(bindings.repeat_pending());
    assert_eq!(
        press(&mut bindings, Key::L),
        Some(repeat(frames(true, 1), 2))
    );
    assert_eq!(held(&mut bindings, Key::L), None);
    for interrupt in [Key::Escape, Key::Space, Key::Tab] {
        press(&mut bindings, Key::R);
        press(&mut bindings, interrupt);
        assert!(!bindings.repeat_pending());
    }
    press(&mut bindings, Key::R);
    assert!(matches!(
        press(&mut bindings, Key::X),
        Some(Action::Invalid(_))
    ));
    assert!(bindings.pending().is_empty());
    for (prefix, modifiers) in [(Key::M, Modifiers::NONE), (Key::Quote, Modifiers::SHIFT)] {
        bindings.key(prefix, modifiers, false, false);
        assert!(!bindings.native_control_owns_cut(Key::R, Modifiers::NONE, EditSelection::Range));
        press(&mut bindings, Key::Escape);
    }
    assert!(bindings.native_control_owns_cut(Key::R, Modifiers::NONE, EditSelection::Range));
}

#[test]
fn repeat_motion_composition_uses_the_selected_logical_or_physical_key_mode() {
    for mode in ["logical", "physical"] {
        let mut bindings = Bindings::from_json(
            &serde_json::to_vec(&serde_json::json!({
                "version":1, "key_mode":mode, "bindings":[]
            }))
            .unwrap(),
        )
        .unwrap();
        let (first, first_physical, last, last_physical) = if mode == "logical" {
            (Key::R, Key::F, Key::L, Key::B)
        } else {
            (Key::F, Key::R, Key::B, Key::L)
        };
        bindings.route_event(
            first,
            Some(first_physical),
            Modifiers::NONE,
            false,
            false,
            false,
            true,
            EditSelection::None,
        );
        assert!(bindings.repeat_pending());
        assert_eq!(
            bindings.route_event(
                last,
                Some(last_physical),
                Modifiers::NONE,
                false,
                false,
                false,
                true,
                EditSelection::None
            ),
            Some(repeat(frames(true, 1), 2))
        );
    }
}
