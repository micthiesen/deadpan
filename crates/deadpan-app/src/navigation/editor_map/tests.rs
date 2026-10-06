use super::*;

#[test]
fn distinct_operator_families_cannot_overlap_at_either_prefix_depth() {
    let families = ["repeat.operator", "cut.operator", "yank.operator"];
    for mode in ["logical", "physical"] {
        for first in families {
            for second in families {
                if first == second {
                    continue;
                }
                let bytes = serde_json::to_vec(&serde_json::json!({
                    "version": 1, "key_mode": mode, "bindings": [
                        {"action": first, "keys": [["e"]]},
                        {"action": second, "keys": [["e", "b"]]}
                    ]
                }))
                .unwrap();
                let error = Bindings::from_json(&bytes)
                    .err()
                    .expect("overlapping operator families must reject");
                assert!(error.contains("Operator prefixes"), "{error}");
                assert!(error.contains("overlap"), "{error}");
                assert!(error.contains("Repeat, Cut, and Yank"), "{error}");
            }
        }
    }
}

#[test]
fn operator_aliases_and_shared_nonoperator_ancestors_remain_configurable() {
    for entries in [
        serde_json::json!([
            {"action": "repeat.operator", "keys": [["o"], ["o", "z"]]}
        ]),
        serde_json::json!([
            {"action": "repeat.operator", "keys": [["o", "z"]]},
            {"action": "cut.operator", "keys": [["o", "f"]]},
            {"action": "hold", "keys": [["o", "n"]]}
        ]),
    ] {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "version": 1, "key_mode": "logical", "bindings": entries
        }))
        .unwrap();
        assert!(Bindings::from_json(&bytes).is_ok());
    }
}

#[test]
fn audit_enumerates_unannotated_intermediate_branches() {
    let a = Stroke::Key(Key::A, false);
    let b = Stroke::Key(Key::B, false);
    let c = Stroke::Key(Key::C, false);
    let trie = Trie::compile(
        vec![Binding {
            path: vec![a, b, c],
            label: "abc".into(),
            value: Rule {
                action: Action::First,
                count: CountPolicy::Ignore,
                short: "start",
                repeatable: false,
                interrupt: false,
            },
        }],
        Vec::new(),
    )
    .unwrap();
    assert_eq!(branch_paths(&trie), vec![vec![], vec![a], vec![a, b]]);
}

#[test]
fn rejected_count_teaching_does_not_advertise_an_available_edit() {
    for (digits, key, message) in [
        (
            "0",
            Key::R,
            "An edit count must be positive; no edit was made.",
        ),
        (
            "0",
            Key::D,
            "An operator count must be positive; no action was taken.",
        ),
    ] {
        let mut bindings = counted(digits);
        bindings.key(key, Modifiers::NONE, false, false);
        assert_eq!(bindings.pending_hint(), Some(message.to_owned()));
        assert_eq!(
            bindings.key(key, Modifiers::NONE, false, false),
            Some(Action::Invalid(message))
        );
    }
    let zero = counted("0");
    assert!(zero.pending_hint().unwrap().starts_with("Zero count:"));
    assert!(!zero.pending_hint().unwrap().contains("cut frames"));
}

fn enter(keys: &[Key]) -> Bindings {
    let mut bindings = Bindings::default();
    for key in keys {
        bindings.key(*key, Modifiers::NONE, false, false);
    }
    bindings
}

fn counted(digits: &str) -> Bindings {
    let mut bindings = Bindings::default();
    for digit in digits.bytes() {
        bindings.key(
            DIGITS[usize::from(digit - b'0')].0,
            Modifiers::NONE,
            false,
            false,
        );
    }
    bindings
}

#[test]
fn transport_and_group_interrupts_clear_every_non_mark_prefix_even_overflow() {
    let prefixes: &[&[Key]] = &[
        &[],
        &[Key::R],
        &[Key::D],
        &[Key::Comma],
        &[Key::G],
        &[Key::Num3],
        &[Key::Num3, Key::R],
        &[Key::Num9; 11],
    ];
    for prefix in prefixes {
        for (key, modifiers, expected) in [
            (Key::Space, Modifiers::NONE, Action::Playback),
            (Key::Space, Modifiers::SHIFT, Action::Audition),
            (Key::Enter, Modifiers::NONE, Action::EnterGroup),
            (Key::Backspace, Modifiers::NONE, Action::LeaveGroup),
            (Key::G, Modifiers::SHIFT, Action::Last),
            (
                Key::Colon,
                Modifiers::ALT | Modifiers::SHIFT,
                Action::Command,
            ),
        ] {
            let expected = if prefix == &[Key::D] && key == Key::G {
                Action::Operator {
                    cut: true,
                    selector: deadpan_core::SemanticSelector::Motion {
                        motion: deadpan_core::SemanticMotion::Scope { end: true },
                    },
                }
            } else if prefix.last() == Some(&Key::R) && key == Key::G {
                Action::Repeat {
                    selector: deadpan_core::SemanticSelector::Motion {
                        motion: deadpan_core::SemanticMotion::Scope { end: true },
                    },
                    plays: std::num::NonZeroU32::new(if prefix.len() == 1 { 2 } else { 3 })
                        .unwrap(),
                }
            } else {
                expected
            };
            let mut bindings = enter(prefix);
            assert_eq!(
                bindings.key(key, modifiers, false, false),
                Some(expected),
                "{prefix:?} then {key:?} {modifiers:?}"
            );
            assert!(bindings.pending().is_empty());
            assert_eq!(bindings.mark_prefix(), None);
            assert!(!bindings.reuse_pending());
        }
    }
}

#[test]
fn mark_names_outrank_transport_but_native_commands_and_focus_still_interrupt() {
    for prefix in [Key::M, Key::Quote] {
        for (key, modifiers) in [
            (Key::Space, Modifiers::NONE),
            (Key::Space, Modifiers::SHIFT),
            (Key::Enter, Modifiers::NONE),
            (Key::Backspace, Modifiers::NONE),
            (Key::Colon, Modifiers::NONE),
            (Key::Questionmark, Modifiers::NONE),
        ] {
            let mut bindings = enter(&[prefix]);
            assert!(matches!(
                bindings.key(key, modifiers, false, false),
                Some(Action::Invalid(_))
            ));
            assert!(bindings.pending().is_empty());
        }
        let expected = if prefix == Key::M {
            Action::SetMark('G')
        } else {
            Action::JumpMark('G')
        };
        assert_eq!(
            enter(&[prefix]).key(Key::G, Modifiers::SHIFT, false, false),
            Some(expected)
        );
        for (key, modifiers, text, expected) in [
            (Key::Escape, Modifiers::NONE, false, Action::Escape),
            (
                Key::Tab,
                Modifiers::SHIFT,
                false,
                Action::Pane { reverse: true },
            ),
            (Key::R, Modifiers::CTRL, false, Action::Redo),
            (Key::O, Modifiers::COMMAND, true, Action::Open),
        ] {
            let mut bindings = enter(&[prefix]);
            assert_eq!(bindings.key(key, modifiers, text, false), Some(expected));
            assert_eq!(bindings.mark_prefix(), None);
            assert!(bindings.pending().is_empty());
        }
        let mut bindings = enter(&[prefix]);
        assert!(matches!(
            bindings.key(Key::O, Modifiers::CTRL | Modifiers::COMMAND, false, false),
            Some(Action::Invalid(_))
        ));
        assert!(bindings.pending().is_empty());
        assert_eq!(
            enter(&[prefix]).key(Key::O, Modifiers::COMMAND, true, true),
            None,
            "IME owns even a native application shortcut"
        );
    }
}

#[test]
fn count_policy_distinguishes_zero_absence_one_and_overflow() {
    for key in [Key::H, Key::J] {
        let expected = if key == Key::H {
            Action::Step {
                forward: false,
                count: 1,
            }
        } else {
            Action::Beat {
                forward: true,
                count: 1,
            }
        };
        assert_eq!(
            counted("0").key(key, Modifiers::NONE, false, false),
            Some(expected)
        );
    }
    let mut zero_hold = counted("0");
    zero_hold.key(Key::Comma, Modifiers::NONE, false, false);
    assert!(!zero_hold.reuse_pending());
    assert_eq!(
        zero_hold.key(Key::H, Modifiers::NONE, false, false),
        Some(Action::Edit(BeatEdit::InsertHold(
            duration::DurationInput::half_seconds(0)
        )))
    );
    for key in [Key::X, Key::Plus, Key::Minus] {
        assert!(matches!(
            counted("0").key(key, Modifiers::NONE, false, false),
            Some(Action::Invalid(_))
        ));
    }
    for key in [Key::V, Key::P, Key::S] {
        assert!(matches!(
            counted("1").key(key, Modifiers::NONE, false, false),
            Some(Action::Invalid(_))
        ));
    }
    for (digits, expected) in [("", 2), ("1", 1), ("3", 3)] {
        let mut bindings = counted(digits);
        bindings.key(Key::R, Modifiers::NONE, false, false);
        assert_eq!(
            bindings.key(Key::R, Modifiers::NONE, false, false),
            Some(Action::Repeat {
                selector: deadpan_core::SemanticSelector::SelectedBeat,
                plays: std::num::NonZeroU32::new(expected).unwrap(),
            })
        );
    }
    for (key, expected) in [
        (Key::Home, Action::First),
        (Key::End, Action::Last),
        (Key::U, Action::Undo),
    ] {
        assert_eq!(
            counted("0").key(key, Modifiers::NONE, false, false),
            Some(expected)
        );
        let mut overflow = counted("4294967296");
        assert!(matches!(
            overflow.key(key, Modifiers::NONE, false, false),
            Some(Action::Invalid(_))
        ));
        assert!(overflow.pending().is_empty());
    }
    let mut overflow = counted("4294967296");
    assert_eq!(
        overflow.key(Key::Comma, Modifiers::NONE, false, false),
        Some(Action::OfferInsert)
    );
    assert!(!overflow.reuse_pending());
    assert!(matches!(
        overflow.key(Key::H, Modifiers::NONE, false, false),
        Some(Action::Invalid(_))
    ));
}

#[test]
fn armed_delete_keeps_its_operator_when_visual_context_changes() {
    for selection in [EditSelection::Empty, EditSelection::Range] {
        let mut bindings = enter(&[Key::Num1, Key::D]);
        assert_eq!(bindings.pending(), "1d");
        assert_eq!(
            bindings.key_with_selection(Key::D, Modifiers::NONE, false, false, selection),
            Some(Action::Operator {
                cut: true,
                selector: deadpan_core::SemanticSelector::SelectedBeat
            })
        );
        assert!(bindings.pending().is_empty());
        assert!(matches!(
            counted("1").key_with_selection(Key::D, Modifiers::NONE, false, false, selection),
            Some(Action::Invalid(_))
        ));
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
    }
}

#[test]
fn invalid_suffix_never_restarts_at_a_root_binding() {
    let mut bindings = enter(&[Key::G]);
    assert_eq!(bindings.key(Key::H, Modifiers::NONE, false, false), None);
    assert!(bindings.pending().is_empty());
    assert_eq!(
        bindings.key(Key::H, Modifiers::NONE, false, false),
        Some(Action::Step {
            forward: false,
            count: 1
        })
    );
    // `q` alone begins a macro recording prefix; as an invalid suffix it
    // must not restart there.
    let mut bindings = enter(&[Key::Comma]);
    assert!(matches!(
        bindings.key(Key::Q, Modifiers::NONE, false, false),
        Some(Action::Invalid(_))
    ));
    assert!(bindings.pending().is_empty());
    assert_eq!(
        bindings.key(Key::X, Modifiers::NONE, false, false),
        Some(Action::DeleteFrames(1))
    );
}

#[test]
fn held_motion_cannot_complete_a_pause_or_mark_and_does_not_consume_the_prefix() {
    assert!(Bindings::default().allows_key_repeat(Key::H, Modifiers::NONE));
    for prefix in [Key::Comma, Key::M, Key::Quote, Key::R, Key::D, Key::G] {
        let mut bindings = enter(&[prefix]);
        let pending = bindings.pending();
        let hint = bindings.pending_hint();
        for key in [
            Key::H,
            Key::L,
            Key::J,
            Key::K,
            Key::ArrowLeft,
            Key::R,
            Key::D,
            Key::G,
            Key::X,
        ] {
            assert!(
                !bindings.allows_key_repeat(key, Modifiers::NONE),
                "held {key:?} after {prefix:?}"
            );
            assert_eq!(bindings.pending(), pending);
            assert_eq!(bindings.pending_hint(), hint);
        }
        if prefix == Key::Comma {
            assert!(bindings.reuse_pending());
            assert_eq!(
                bindings.key(Key::H, Modifiers::NONE, false, false),
                Some(Action::Edit(BeatEdit::InsertHold(
                    duration::DurationInput::half_seconds(1)
                )))
            );
            assert!(bindings.pending().is_empty());
            assert!(bindings.allows_key_repeat(Key::H, Modifiers::NONE));
        }
    }
}

#[test]
fn native_cut_probe_preserves_pending_mark_authority() {
    for selection in [
        EditSelection::None,
        EditSelection::Empty,
        EditSelection::Range,
    ] {
        assert!(Bindings::default().native_control_owns_cut(Key::X, Modifiers::NONE, selection));
        assert_eq!(
            Bindings::default().native_control_owns_cut(Key::D, Modifiers::NONE, selection),
            selection != EditSelection::None
        );
        for prefix in [Key::M, Key::Quote] {
            let mut bindings = enter(&[prefix]);
            for key in [Key::X, Key::D] {
                assert!(!bindings.native_control_owns_cut(key, Modifiers::NONE, selection));
                assert!(!bindings.native_control_owns_cut(key, Modifiers::SHIFT, selection));
            }
            assert_eq!(bindings.pending(), if prefix == Key::M { "m" } else { "'" });
            assert!(bindings.mark_prefix().is_some());
            let expected = if prefix == Key::M {
                Action::SetMark('x')
            } else {
                Action::JumpMark('x')
            };
            assert_eq!(
                bindings.key_with_selection(Key::X, Modifiers::NONE, false, false, selection),
                Some(expected)
            );
        }
    }
}

#[test]
fn prefix_teaching_lists_reachable_declared_leaves_and_counted_leader_restricts_them() {
    for selection in [
        EditSelection::None,
        EditSelection::Empty,
        EditSelection::Range,
    ] {
        for key in [Key::G, Key::Comma] {
            if key == Key::D && selection != EditSelection::None {
                continue;
            }
            for count in [None, Some(3)] {
                if count.is_some() && key != Key::Comma {
                    continue;
                }
                let mut pending = counted(if count.is_some() { "3" } else { "" });
                pending.key_with_selection(key, Modifiers::NONE, false, false, selection);
                let hint = pending.pending_hint().expect("proper prefix has teaching");
                let advertised: Vec<_> = hint
                    .split(" · ")
                    .filter(|part| *part != "Esc cancels")
                    .map(|part| {
                        part.split_once(' ')
                            .expect("key and explanation")
                            .0
                            .to_owned()
                    })
                    .collect();
                let branch = map(selection)
                    .resolve(&[Stroke::Key(key, false)])
                    .expect("declared prefix");
                let mut reachable = Vec::new();
                for (stroke, child) in branch.children() {
                    let leaf = child.terminal().expect("shipped prefix has direct leaves");
                    let expected = leaf.value.resolve(count);
                    let mut candidate = pending.clone();
                    assert_eq!(candidate.audit_stroke(*stroke, selection), Some(expected));
                    if !matches!(expected, Action::Invalid(_)) {
                        reachable.push(stroke.label());
                    }
                }
                assert_eq!(
                    advertised, reachable,
                    "{key:?} count={count:?} selection={selection:?}"
                );
                assert!(hint.ends_with("Esc cancels"));
                assert_eq!(
                    pending.pending(),
                    if count.is_some() {
                        "3,".to_owned()
                    } else {
                        Stroke::Key(key, false).label()
                    }
                );
            }
        }
    }
}
