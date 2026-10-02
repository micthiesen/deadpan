use super::*;

const LETTERS: &[(Key, char)] = &[
    (Key::A, 'a'),
    (Key::B, 'b'),
    (Key::C, 'c'),
    (Key::D, 'd'),
    (Key::E, 'e'),
    (Key::F, 'f'),
    (Key::G, 'g'),
    (Key::H, 'h'),
    (Key::I, 'i'),
    (Key::J, 'j'),
    (Key::K, 'k'),
    (Key::L, 'l'),
    (Key::M, 'm'),
    (Key::N, 'n'),
    (Key::O, 'o'),
    (Key::P, 'p'),
    (Key::Q, 'q'),
    (Key::R, 'r'),
    (Key::S, 's'),
    (Key::T, 't'),
    (Key::U, 'u'),
    (Key::V, 'v'),
    (Key::W, 'w'),
    (Key::X, 'x'),
    (Key::Y, 'y'),
    (Key::Z, 'z'),
];

#[test]
fn all_mark_letters_take_precedence_over_motions_operators_and_visual_cut() {
    for (prefix, kind, label) in [
        (Key::M, MarkPrefix::Set, "m"),
        (Key::Quote, MarkPrefix::Jump, "'"),
    ] {
        for &(key, letter) in LETTERS {
            for modifiers in [Modifiers::NONE, Modifiers::SHIFT] {
                for selection in [
                    EditSelection::None,
                    EditSelection::Empty,
                    EditSelection::Range,
                ] {
                    let mut bindings = Bindings::default();
                    assert_eq!(
                        bindings.key_with_selection(
                            prefix,
                            Modifiers::NONE,
                            false,
                            false,
                            selection
                        ),
                        None
                    );
                    assert_eq!(bindings.mark_prefix(), Some(kind));
                    assert_eq!(bindings.pending(), label);
                    assert!(bindings.pending_hint().unwrap().contains("a–z / A–Z"));
                    let name = if modifiers.shift {
                        letter.to_ascii_uppercase()
                    } else {
                        letter
                    };
                    let expected = match kind {
                        MarkPrefix::Set => Action::SetMark(name),
                        MarkPrefix::Jump => Action::JumpMark(name),
                    };
                    assert_eq!(
                        bindings.key_with_selection(key, modifiers, false, false, selection),
                        Some(expected),
                        "{prefix:?} {key:?} {modifiers:?} {selection:?}"
                    );
                    assert_eq!(bindings.mark_prefix(), None);
                    assert!(bindings.pending().is_empty());
                }
            }
        }
    }
}

#[test]
fn batched_and_slow_mark_prefixes_have_the_same_meaning() {
    for (prefix, expected) in [
        (Key::M, Action::SetMark('g')),
        (Key::Quote, Action::JumpMark('g')),
    ] {
        let mut batch = Bindings::default();
        let output: Vec<_> = [prefix, Key::G]
            .into_iter()
            .filter_map(|key| batch.key(key, Modifiers::NONE, false, false))
            .collect();
        assert_eq!(output, vec![expected]);
        let mut slow = Bindings::default();
        slow.key(prefix, Modifiers::NONE, false, false);
        // Reading the visible pending state on arbitrarily many idle frames
        // cannot age out a prefix. The router has no clock or timeout input.
        for _ in 0..300 {
            assert!(!slow.pending().is_empty());
            assert!(slow.pending_hint().is_some());
            assert!(slow.mark_prefix().is_some());
        }
        assert_eq!(
            slow.key(Key::G, Modifiers::NONE, false, false),
            Some(expected)
        );
    }
}

#[test]
fn mark_prefixes_respect_apostrophe_and_exact_letter_modifiers() {
    let mut bindings = Bindings::default();
    assert_eq!(
        bindings.key(Key::Quote, Modifiers::NONE, false, false),
        None
    );
    assert_eq!(bindings.mark_prefix(), Some(MarkPrefix::Jump));
    assert_eq!(
        bindings.key(Key::A, Modifiers::NONE, false, false),
        Some(Action::JumpMark('a'))
    );
    for modifiers in [Modifiers::ALT, Modifiers::ALT | Modifiers::SHIFT] {
        let mut bindings = Bindings::default();
        assert_eq!(bindings.key(Key::Quote, modifiers, false, false), None);
        assert_eq!(bindings.mark_prefix(), None);
        assert!(bindings.pending().is_empty());
    }
    for key in [Key::Semicolon, Key::Backtick, Key::Equals] {
        let mut bindings = Bindings::default();
        bindings.key(key, Modifiers::NONE, false, false);
        assert_eq!(bindings.mark_prefix(), None);
    }
    for modifiers in [
        Modifiers::ALT,
        Modifiers::CTRL,
        Modifiers::COMMAND,
        Modifiers::MAC_CMD,
        Modifiers::ALT | Modifiers::SHIFT,
        Modifiers::CTRL | Modifiers::SHIFT,
    ] {
        for prefix in [Key::M, Key::Quote] {
            let mut bindings = Bindings::default();
            bindings.key(prefix, Modifiers::NONE, false, false);
            assert_eq!(bindings.key(Key::A, modifiers, false, false), None);
            assert!(bindings.pending().is_empty());
            assert_eq!(bindings.mark_prefix(), None);
        }
    }
    for modifiers in [
        Modifiers::SHIFT,
        Modifiers::ALT,
        Modifiers::CTRL,
        Modifiers::COMMAND,
    ] {
        let mut bindings = Bindings::default();
        assert_eq!(bindings.key(Key::M, modifiers, false, false), None);
        assert!(bindings.pending().is_empty());
    }
}

#[test]
fn marks_reject_counts_overflow_conflicting_prefixes_and_invalid_names() {
    for prefix in [Key::M, Key::Quote] {
        for earlier in [
            vec![Key::Num0],
            vec![Key::Num1],
            vec![Key::Num3],
            vec![Key::Num9; 11],
            vec![Key::G],
            vec![Key::R],
            vec![Key::D],
            vec![Key::Comma],
            vec![Key::Num3, Key::R],
        ] {
            let mut bindings = Bindings::default();
            for key in earlier {
                bindings.key(key, Modifiers::NONE, false, false);
            }
            assert!(matches!(
                bindings.key(prefix, Modifiers::NONE, false, false),
                Some(Action::Invalid(_))
            ));
            assert!(bindings.pending().is_empty());
            assert_eq!(bindings.mark_prefix(), None);
        }
        for invalid in [
            Key::Num0,
            Key::Num3,
            Key::Quote,
            Key::Comma,
            Key::Colon,
            Key::Questionmark,
            Key::Plus,
            Key::Space,
            Key::Enter,
            Key::Backspace,
            Key::ArrowLeft,
            Key::F1,
        ] {
            let mut bindings = Bindings::default();
            bindings.key(prefix, Modifiers::NONE, false, false);
            assert!(
                matches!(
                    bindings.key(invalid, Modifiers::NONE, false, false),
                    Some(Action::Invalid(_))
                ),
                "{prefix:?} {invalid:?}"
            );
            assert!(bindings.pending().is_empty());
        }
    }
}

#[test]
fn text_ime_escape_pane_and_context_reset_cancel_pending_marks() {
    for prefix in [Key::M, Key::Quote] {
        for (text, ime) in [(true, false), (false, true), (true, true)] {
            let mut bindings = Bindings::default();
            assert_eq!(bindings.key(prefix, Modifiers::NONE, text, ime), None);
            assert!(bindings.pending().is_empty());
            bindings.key(prefix, Modifiers::NONE, false, false);
            assert_eq!(bindings.key(Key::D, Modifiers::NONE, text, ime), None);
            assert_eq!(bindings.mark_prefix(), None);
        }
        for (key, modifiers, expected) in [
            (Key::Escape, Modifiers::NONE, Action::Escape),
            (Key::Tab, Modifiers::NONE, Action::Pane { reverse: false }),
            (Key::Tab, Modifiers::SHIFT, Action::Pane { reverse: true }),
        ] {
            let mut bindings = Bindings::default();
            bindings.key(prefix, Modifiers::NONE, false, false);
            assert_eq!(bindings.key(key, modifiers, false, false), Some(expected));
            assert_eq!(bindings.mark_prefix(), None);
            assert!(bindings.pending().is_empty());
        }
        let mut bindings = Bindings::default();
        bindings.key(prefix, Modifiers::NONE, false, false);
        bindings.clear();
        assert_eq!(bindings.mark_prefix(), None);
        assert_eq!(bindings.key(Key::G, Modifiers::NONE, false, false), None);
        assert_eq!(bindings.pending(), "g");
    }
}

#[test]
fn held_keys_cannot_start_marks_complete_letter_names_or_repeat_history() {
    let mut bindings = Bindings::default();
    for prefix in [Key::M, Key::Quote] {
        assert!(!bindings.allows_key_repeat(prefix, Modifiers::NONE));
        bindings.key(prefix, Modifiers::NONE, false, false);
        for &(key, _) in LETTERS {
            assert!(!bindings.allows_key_repeat(key, Modifiers::NONE));
            assert!(!bindings.allows_key_repeat(key, Modifiers::SHIFT));
        }
        assert!(bindings.mark_prefix().is_some());
        bindings.clear();
    }
    for key in [Key::O, Key::I] {
        assert!(!bindings.allows_key_repeat(key, Modifiers::CTRL));
        assert!(!bindings.allows_key_repeat(key, Modifiers::CTRL | Modifiers::COMMAND));
    }
    for key in [Key::H, Key::J, Key::K, Key::L, Key::ArrowLeft] {
        assert!(bindings.allows_key_repeat(key, Modifiers::NONE));
    }
}

#[test]
fn jump_history_uses_exact_control_before_platform_command_shortcuts() {
    for (key, forward) in [(Key::O, false), (Key::I, true)] {
        for modifiers in [Modifiers::CTRL, Modifiers::CTRL | Modifiers::COMMAND] {
            assert_eq!(
                Bindings::default().key(key, modifiers, false, false),
                Some(Action::JumpHistory { forward })
            );
            for (text, ime) in [(true, false), (false, true), (true, true)] {
                let mut bindings = Bindings::default();
                bindings.key(Key::M, Modifiers::NONE, false, false);
                assert_eq!(bindings.key(key, modifiers, text, ime), None);
                assert!(bindings.pending().is_empty());
            }
            for prefix in [
                vec![Key::Num0],
                vec![Key::Num1],
                vec![Key::Num9; 11],
                vec![Key::G],
                vec![Key::R],
                vec![Key::D],
                vec![Key::Comma],
                vec![Key::M],
                vec![Key::Quote],
            ] {
                let mut bindings = Bindings::default();
                for part in prefix {
                    bindings.key(part, Modifiers::NONE, false, false);
                }
                assert!(matches!(
                    bindings.key(key, modifiers, false, false),
                    Some(Action::Invalid(_))
                ));
                assert!(bindings.pending().is_empty());
            }
        }
        for modifiers in [
            Modifiers::CTRL | Modifiers::SHIFT,
            Modifiers::CTRL | Modifiers::ALT,
            Modifiers::CTRL | Modifiers::MAC_CMD,
            Modifiers::CTRL | Modifiers::MAC_CMD | Modifiers::COMMAND,
            Modifiers::CTRL | Modifiers::COMMAND | Modifiers::ALT,
            Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
        ] {
            let mut bindings = Bindings::default();
            bindings.key(Key::Quote, Modifiers::NONE, false, false);
            assert_eq!(bindings.key(key, modifiers, false, false), None);
            assert!(bindings.pending().is_empty());
        }
        assert_eq!(
            Bindings::default().key(key, Modifiers::MAC_CMD | Modifiers::COMMAND, false, false),
            Some(if forward {
                Action::Import
            } else {
                Action::Open
            })
        );
    }
}

#[test]
fn native_mark_commands_preserve_case_and_require_exact_names_or_no_arguments() {
    use command::{Entry, parse};
    for &(verb, make_action) in &[
        ("mark", Action::SetMark as fn(char) -> Action),
        ("jump", Action::JumpMark),
        ("unmark", Action::DeleteMark),
    ] {
        for letter in ('a'..='z').chain('A'..='Z') {
            assert_eq!(
                parse(&format!(":{verb} {letter}")),
                Ok(Entry::Action(make_action(letter)))
            );
            assert_eq!(
                parse(&format!("{} {letter}", verb.to_ascii_uppercase())),
                Ok(Entry::Action(make_action(letter)))
            );
        }
        for suffix in [
            "",
            "1",
            "'",
            "é",
            "あ",
            "aA",
            "a extra",
            "a b c",
            "a\u{0301}",
            "🙂",
        ] {
            assert!(
                parse(&format!("{verb} {suffix}")).is_err(),
                "{verb} {suffix}"
            );
        }
    }
    for (verb, action) in [
        ("marks", Action::Marks),
        ("jump-back", Action::JumpHistory { forward: false }),
        ("jump-forward", Action::JumpHistory { forward: true }),
    ] {
        assert_eq!(parse(verb), Ok(Entry::Action(action)));
        assert!(parse(&format!("{verb} a")).is_err());
        assert!(parse(&format!("{verb} 2 extra")).is_err());
    }
}
