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

fn frames(cut: bool, forward: bool, count: u32) -> Action {
    Action::Operator {
        cut,
        selector: Selector::Motion {
            motion: Motion::Frames {
                forward,
                count: NonZeroU32::new(count).unwrap(),
            },
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
fn operators_share_frame_beat_and_scope_motion_semantics() {
    for (prefix, cut) in [(Key::D, true), (Key::Y, false)] {
        for (key, forward) in [(Key::H, false), (Key::L, true)] {
            for (count, distance) in [("", 1), ("1", 1), ("12", 12), ("4294967295", u32::MAX)] {
                for after in [false, true] {
                    let mut bindings = Bindings::default();
                    if !after {
                        digits(&mut bindings, count);
                    }
                    press(&mut bindings, prefix);
                    assert!(bindings.operator_pending());
                    if after {
                        digits(&mut bindings, count);
                    }
                    assert_eq!(
                        press(&mut bindings, key),
                        Some(frames(cut, forward, distance))
                    );
                    assert!(!bindings.operator_pending());
                    assert!(bindings.pending().is_empty());
                }
            }
        }
        for (key, forward) in [(Key::J, true), (Key::K, false)] {
            let mut bindings = Bindings::default();
            press(&mut bindings, prefix);
            digits(&mut bindings, "3");
            assert_eq!(
                press(&mut bindings, key),
                Some(Action::Operator {
                    cut,
                    selector: Selector::Motion {
                        motion: Motion::Beats {
                            forward,
                            count: NonZeroU32::new(3).unwrap()
                        }
                    }
                })
            );
        }
        for end in [false, true] {
            let mut bindings = Bindings::default();
            press(&mut bindings, prefix);
            if !end {
                press(&mut bindings, Key::G);
                assert!(bindings.operator_pending());
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
                Some(Action::Operator {
                    cut,
                    selector: Selector::Motion {
                        motion: Motion::Scope { end }
                    }
                })
            );
        }
        let mut bindings = Bindings::default();
        press(&mut bindings, prefix);
        assert_eq!(
            press(&mut bindings, prefix),
            Some(Action::Operator {
                cut,
                selector: Selector::SelectedBeat
            })
        );
    }
}

#[test]
fn rejected_counts_consume_the_complete_operator_without_running_a_root_motion() {
    for prefix in [Key::D, Key::Y] {
        for value in ["0", "4294967296", "999999999999999999"] {
            for after in [false, true] {
                let mut bindings = Bindings::default();
                if !after {
                    digits(&mut bindings, value);
                }
                press(&mut bindings, prefix);
                if after {
                    digits(&mut bindings, value);
                }
                assert!(bindings.operator_pending());
                assert!(matches!(
                    press(&mut bindings, Key::L),
                    Some(Action::Invalid(_))
                ));
                assert!(bindings.pending().is_empty());
            }
        }
        for outer in ["1", "3"] {
            let mut bindings = Bindings::default();
            digits(&mut bindings, outer);
            press(&mut bindings, prefix);
            digits(&mut bindings, outer);
            assert!(bindings.operator_pending());
            assert!(bindings.pending_hint().unwrap().contains("not both"));
            assert!(matches!(
                press(&mut bindings, Key::L),
                Some(Action::Invalid(_))
            ));
            assert!(bindings.pending().is_empty());
        }
        let mut bindings = Bindings::default();
        press(&mut bindings, prefix);
        press(&mut bindings, Key::G);
        digits(&mut bindings, "2");
        assert!(bindings.operator_pending());
        assert!(matches!(
            press(&mut bindings, Key::G),
            Some(Action::Invalid(_))
        ));
        for after in [false, true] {
            for (count, terminal) in [("2", prefix), ("1", Key::Home), ("3", Key::End)] {
                let mut bindings = Bindings::default();
                if !after {
                    digits(&mut bindings, count);
                }
                press(&mut bindings, prefix);
                if after {
                    digits(&mut bindings, count);
                }
                assert!(matches!(
                    press(&mut bindings, terminal),
                    Some(Action::Invalid(_))
                ));
                assert!(bindings.pending().is_empty());
            }
        }
    }
}

#[test]
fn original_visual_and_sound_preserve_their_immediate_copy_and_delete_routes() {
    for domain in [RoutingDomain::Original, RoutingDomain::Sound] {
        for selection in [
            EditSelection::None,
            EditSelection::Empty,
            EditSelection::Range,
        ] {
            let mut bindings = Bindings::default();
            bindings.set_routing_domain(domain);
            assert_eq!(
                bindings.key_with_selection(Key::Y, Modifiers::NONE, false, false, selection),
                Some(Action::CopyMoment)
            );
            assert!(!bindings.operator_pending());
            press(&mut bindings, Key::D);
            assert_eq!(
                press(&mut bindings, Key::D),
                Some(Action::Edit(BeatEdit::Delete))
            );
            assert_eq!(
                bindings.key_label_in(BindingId::Copy, domain, selection),
                "y"
            );
        }
    }
    for selection in [EditSelection::Empty, EditSelection::Range] {
        let mut bindings = Bindings::default();
        assert_eq!(
            bindings.key_with_selection(Key::Y, Modifiers::NONE, false, false, selection),
            Some(Action::CopyMoment)
        );
        assert_eq!(
            bindings.key_with_selection(Key::D, Modifiers::NONE, false, false, selection),
            Some(Action::DeleteSelection)
        );
        assert_eq!(
            bindings.key_label_in(BindingId::Copy, RoutingDomain::Edit, selection),
            "y"
        );
    }
    let mut bindings = Bindings::default();
    assert_eq!(
        bindings.key_labels_in(BindingId::Copy, RoutingDomain::Edit, EditSelection::None),
        "yy"
    );
    press(&mut bindings, Key::Y);
    bindings.set_routing_domain(RoutingDomain::Original);
    assert!(!bindings.operator_pending());
    assert!(matches!(
        press(&mut bindings, Key::Y),
        Some(Action::Invalid(_))
    ));
    assert_eq!(press(&mut bindings, Key::Y), Some(Action::CopyMoment));
}

#[test]
fn configured_operator_ancestors_compose_motion_paths_and_teach_them() {
    let mut bindings = configured(serde_json::json!([
        {"action":"cut.operator", "keys":[["e","b"]]},
        {"action":"yank.operator", "keys":[["e","c"]]},
        {"action":"frame.next", "keys":[["a","l"]]}
    ]))
    .unwrap();
    press(&mut bindings, Key::E);
    assert!(bindings.operator_pending());
    assert!(bindings.pending_next_keys().unwrap().contains('b'));
    press(&mut bindings, Key::B);
    digits(&mut bindings, "3");
    assert!(bindings.pending_next_keys().unwrap().contains('a'));
    press(&mut bindings, Key::A);
    assert!(bindings.operator_pending());
    assert_eq!(press(&mut bindings, Key::L), Some(frames(true, true, 3)));
    press(&mut bindings, Key::E);
    press(&mut bindings, Key::C);
    press(&mut bindings, Key::A);
    assert_eq!(press(&mut bindings, Key::L), Some(frames(false, true, 1)));
    assert!(configured(serde_json::json!([{"action":"cut.operator", "keys":[["h"]]}])).is_err());
    assert!(configured(serde_json::json!([{"action":"frame.next", "keys":[["d"]]}])).is_err());
    let mut counted = Bindings::default();
    digits(&mut counted, "2");
    press(&mut counted, Key::D);
    assert!(
        counted
            .pending_hint()
            .unwrap()
            .contains("cuts forward in frames")
    );
    assert!(
        !counted
            .pending_next_keys()
            .unwrap()
            .split(" · ")
            .any(|key| key == "d" || key == "G")
    );
}

#[test]
fn pending_operators_preserve_native_input_and_never_latch_held_motion() {
    for prefix in [Key::D, Key::Y] {
        for (text, ime) in [(true, false), (false, true), (true, true)] {
            let mut bindings = Bindings::default();
            press(&mut bindings, prefix);
            assert_eq!(bindings.key(Key::L, Modifiers::NONE, text, ime), None);
            assert!(!bindings.operator_pending());
        }
        let mut bindings = Bindings::default();
        press(&mut bindings, prefix);
        let repeated = |bindings: &mut Bindings| {
            bindings.route_event(
                Key::L,
                Some(Key::L),
                Modifiers::NONE,
                false,
                false,
                true,
                true,
                EditSelection::None,
            )
        };
        assert_eq!(repeated(&mut bindings), None);
        assert!(bindings.operator_pending());
        assert_eq!(
            press(&mut bindings, Key::L),
            Some(frames(prefix == Key::D, true, 1))
        );
        assert_eq!(repeated(&mut bindings), None);
        press(&mut bindings, prefix);
        assert_eq!(press(&mut bindings, Key::Space), Some(Action::Playback));
        assert!(!bindings.operator_pending());
        press(&mut bindings, prefix);
        assert!(matches!(
            press(&mut bindings, Key::X),
            Some(Action::Invalid(_))
        ));
        assert!(!bindings.operator_pending());
    }
    let mut bindings = Bindings::default();
    press(&mut bindings, Key::D);
    assert!(bindings.native_control_owns_cut(Key::L, Modifiers::NONE, EditSelection::None));
    press(&mut bindings, Key::Escape);
    press(&mut bindings, Key::Y);
    assert!(!bindings.native_control_owns_cut(Key::L, Modifiers::NONE, EditSelection::None));
}
