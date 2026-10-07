use super::*;

#[test]
fn comma_a_inserts_counted_half_seconds_in_normal_and_visual_edit_only() {
    assert_eq!(BindingId::GenerateAi.as_str(), "ai.generate");
    let mut bindings = Bindings::default();
    assert_eq!(bindings.key_label(BindingId::GenerateAi), ",a");
    assert_eq!(
        bindings.key(Key::Comma, Modifiers::NONE, false, false),
        Some(Action::OfferInsert)
    );
    assert_eq!(bindings.pending(), ",");
    assert_eq!(
        bindings.key(Key::A, Modifiers::NONE, false, false),
        Some(Action::Edit(BeatEdit::InsertAiHold(
            duration::DurationInput::half_seconds(1)
        )))
    );
    assert!(bindings.pending().is_empty());
    assert!(!bindings.allows_key_repeat(Key::A, Modifiers::NONE));

    // A count changes the inserted duration, as with ,h.
    for key in [Key::Num3, Key::Comma] {
        bindings.key(key, Modifiers::NONE, false, false);
    }
    assert_eq!(bindings.pending(), "3,");
    assert_eq!(
        bindings.key(Key::A, Modifiers::NONE, false, false),
        Some(Action::Edit(BeatEdit::InsertAiHold(
            duration::DurationInput::half_seconds(3)
        )))
    );

    // A Visual range keeps the same pause-at-cursor semantics as ,h.
    bindings.clear();
    bindings.key_with_selection(
        Key::Comma,
        Modifiers::NONE,
        false,
        false,
        EditSelection::Range,
    );
    assert_eq!(
        bindings.key_with_selection(Key::A, Modifiers::NONE, false, false, EditSelection::Range),
        Some(Action::Edit(BeatEdit::InsertAiHold(
            duration::DurationInput::half_seconds(1)
        )))
    );
    for domain in [RoutingDomain::Original, RoutingDomain::Sound] {
        let mut bindings = Bindings::default();
        bindings.set_routing_domain(domain);
        bindings.key(Key::Comma, Modifiers::NONE, false, false);
        assert_ne!(
            bindings.key(Key::A, Modifiers::NONE, false, false),
            Some(Action::Edit(BeatEdit::InsertAiHold(
                duration::DurationInput::half_seconds(1)
            ))),
            "{domain:?}"
        );
    }

    // Native text and composition keep their input.
    let mut bindings = Bindings::default();
    bindings.key(Key::Comma, Modifiers::NONE, false, false);
    assert_eq!(bindings.key(Key::A, Modifiers::NONE, true, false), None);
    let mut bindings = Bindings::default();
    bindings.key(Key::Comma, Modifiers::NONE, false, false);
    assert_eq!(bindings.key(Key::A, Modifiers::NONE, false, true), None);
}

#[test]
fn ai_hold_duration_commands_keep_exact_units_and_do_not_change_existing_hold_provider() {
    use command::{Entry, parse};
    use duration::DurationInput;
    let rate = deadpan_core::FrameRate::new(30_000, 1_001).unwrap();
    for (input, duration, frames) in [
        (":ai-hold 1.5s", DurationInput::half_seconds(3), 45),
        ("AI-HOLD 500ms", DurationInput::half_seconds(1), 15),
        ("ai-hold 00:01.500", DurationInput::half_seconds(3), 45),
        (
            "ai-hold 12f",
            DurationInput::Frames(deadpan_core::FrameDuration::new(12).unwrap()),
            12,
        ),
        (
            "ai-hold 0f",
            DurationInput::Frames(deadpan_core::FrameDuration::ZERO),
            0,
        ),
    ] {
        assert_eq!(
            parse(input),
            Ok(Entry::Action(Action::Edit(BeatEdit::InsertAiHold(
                duration
            )))),
            "{input}"
        );
        assert_eq!(duration.resolve(rate).unwrap().frames(), frames, "{input}");
    }
    assert_eq!(
        parse("hold-provider ai"),
        Ok(Entry::Action(Action::Ai(AiAction::Generate {
            variants: 1
        })))
    );
    for input in [
        "ai-hold",
        "ai-hold 1",
        "ai-hold -1f",
        "ai-hold 0.5s 2",
        "ai-hold 12f video=black",
        "ai-hold 12f motion=still",
    ] {
        assert!(parse(input).is_err(), "{input}");
    }
}

#[test]
fn hold_ai_grammar_preserves_exact_duration_parameter_order_and_zero() {
    use command::{Entry, parse};
    use duration::DurationInput;
    let rate = deadpan_core::FrameRate::new(30_000, 1_001).unwrap();
    for (input, duration, frames) in [
        (":hold 1.5s video=ai audio=silence", "1.5s", 45),
        ("hold 1.5s audio=silence video=ai", "1.5s", 45),
        ("hold 500ms video=ai", "500ms", 15),
        ("hold 12f video=ai audio=silence", "12f", 12),
        ("hold 0f video=ai audio=silence", "0f", 0),
        ("hold 0s audio=silence video=ai", "0s", 0),
    ] {
        let duration = DurationInput::parse(duration).unwrap();
        assert_eq!(
            parse(input),
            Ok(Entry::Action(Action::Edit(BeatEdit::InsertAiHold(
                duration
            )))),
            "{input}"
        );
        assert_eq!(duration.resolve(rate).unwrap().frames(), frames, "{input}");
    }
}

#[test]
fn hold_ai_grammar_keeps_duplicate_audio_and_duration_errors() {
    use command::parse;
    for input in [
        "hold 1.5s video=ai video=ai",
        "hold 1.5s video=ai video=freeze",
        "hold 1.5s video=black video=ai",
        "hold 1.5s video=ai audio=silence audio=silence",
        "hold 1.5s audio=silence video=ai audio=room-tone",
    ] {
        assert_eq!(
            parse(input),
            Err("Each parameter can be given once.".into()),
            "{input}"
        );
    }
    for input in [
        "hold 1.5s video=ai audio=room-tone",
        "hold 1.5s audio=keep video=ai",
    ] {
        assert_eq!(
            parse(input),
            Err("A new pause is silent (audio=silence); choose room tone afterwards with :room-tone.".into()),
            "{input}"
        );
    }
    for input in [
        "hold video=ai audio=silence",
        "hold 1.5 video=ai audio=silence",
        "hold -1f video=ai audio=silence",
        "hold 1f 2f video=ai audio=silence",
        "hold 1f video=ai motion=still",
        "hold 1f video=unknown audio=silence",
    ] {
        assert!(parse(input).is_err(), "{input}");
    }
}

#[test]
fn ai_commands_parse_with_their_exact_arguments() {
    use command::{Entry, parse};
    for (input, action) in [
        (":generate", AiAction::Generate { variants: 1 }),
        ("generate-ai", AiAction::Generate { variants: 1 }),
        ("generate 3", AiAction::Generate { variants: 3 }),
        ("generate-ai 4", AiAction::Generate { variants: 4 }),
        ("cancel-ai", AiAction::Cancel),
        ("PREVIEW-AI", AiAction::Preview),
        ("audition-ai", AiAction::Audition),
        ("accept-ai", AiAction::Accept),
        ("hold-provider ai", AiAction::Generate { variants: 1 }),
        ("HOLD-PROVIDER AI", AiAction::Generate { variants: 1 }),
        ("hold-provider fallback", AiAction::Revert),
        ("revert-ai", AiAction::Revert),
        ("discard-ai", AiAction::Discard),
        ("next-ai", AiAction::Choose(VariantChoice::Next)),
        ("prev-ai", AiAction::Choose(VariantChoice::Previous)),
        ("previous-ai", AiAction::Choose(VariantChoice::Previous)),
        ("pick-ai 2", AiAction::Choose(VariantChoice::Number(2))),
        ("compare-ai", AiAction::Compare(CompareChoice::Toggle)),
        (
            "compare-ai before",
            AiAction::Compare(CompareChoice::Before),
        ),
        (
            "compare-ai BEFORE",
            AiAction::Compare(CompareChoice::Before),
        ),
        ("compare-ai 3", AiAction::Compare(CompareChoice::Variant(3))),
        ("keep-ai", AiAction::Keep),
    ] {
        assert_eq!(
            parse(input),
            Ok(Entry::Action(Action::Ai(action))),
            "{input}"
        );
    }
    for input in [
        "generate now",
        "generate 0",
        "generate 5",
        "generate 2 3",
        "accept-ai 1",
        "hold-provider",
        "hold-provider ai 2",
        "hold-provider freeze",
        "hold-provider fallback now",
        "revert-ai now",
        "cancel-ai x",
        "next-ai 2",
        "pick-ai",
        "pick-ai 0",
        "audition-ai now",
        "compare-ai 0",
        "compare-ai after",
        "compare-ai 1 2",
        "keep-ai 2",
    ] {
        assert!(parse(input).is_err(), "{input}");
    }
}

#[test]
fn comma_x_compares_and_comma_n_chooses_the_next_ai_variant_in_normal_edit_only() {
    assert_eq!(BindingId::CompareAi.as_str(), "ai.compare");
    assert_eq!(BindingId::NextAi.as_str(), "ai.next");
    for (key, action, id, label) in [
        (
            Key::X,
            AiAction::Compare(CompareChoice::Toggle),
            BindingId::CompareAi,
            ",x",
        ),
        (
            Key::N,
            AiAction::Choose(VariantChoice::Next),
            BindingId::NextAi,
            ",n",
        ),
    ] {
        let mut bindings = Bindings::default();
        assert_eq!(bindings.key_label(id), label);
        assert_eq!(
            bindings.key(Key::Comma, Modifiers::NONE, false, false),
            Some(Action::OfferInsert)
        );
        assert!(
            bindings.ai_pending(),
            "{label} captures at its comma ancestor"
        );
        assert_eq!(
            bindings.key(key, Modifiers::NONE, false, false),
            Some(Action::Ai(action))
        );
        assert!(!bindings.allows_key_repeat(key, Modifiers::NONE));
        // A count refuses instead of acting.
        for prefix in [Key::Num2, Key::Comma] {
            bindings.key(prefix, Modifiers::NONE, false, false);
        }
        assert!(matches!(
            bindings.key(key, Modifiers::NONE, false, false),
            Some(Action::Invalid(_))
        ));
        // Visual Edit, Original and Sound routing never reach it.
        bindings.clear();
        bindings.key_with_selection(
            Key::Comma,
            Modifiers::NONE,
            false,
            false,
            EditSelection::Range,
        );
        assert_ne!(
            bindings.key_with_selection(key, Modifiers::NONE, false, false, EditSelection::Range),
            Some(Action::Ai(action))
        );
        for domain in [RoutingDomain::Original, RoutingDomain::Sound] {
            let mut bindings = Bindings::default();
            bindings.set_routing_domain(domain);
            bindings.key(Key::Comma, Modifiers::NONE, false, false);
            assert_ne!(
                bindings.key(key, Modifiers::NONE, false, false),
                Some(Action::Ai(action)),
                "{domain:?}"
            );
        }
        // Native text and composition keep their input.
        let mut bindings = Bindings::default();
        bindings.key(Key::Comma, Modifiers::NONE, false, false);
        assert_eq!(bindings.key(key, Modifiers::NONE, true, false), None);
        let mut bindings = Bindings::default();
        bindings.key(Key::Comma, Modifiers::NONE, false, false);
        assert_eq!(bindings.key(key, Modifiers::NONE, false, true), None);
    }
    // `n` alone keeps its search meaning outside the comma family.
    let mut bindings = Bindings::default();
    assert_ne!(
        bindings.key(Key::N, Modifiers::NONE, false, false),
        Some(Action::Ai(AiAction::Choose(VariantChoice::Next)))
    );
}

#[test]
fn comma_m_mutes_comma_r_picks_a_cutaway_and_comma_t_adds_a_tail_in_your_edit_only() {
    assert_eq!(BindingId::Mute.as_str(), "gain.mute");
    assert_eq!(BindingId::CutawayPicker.as_str(), "cutaway.pick");
    assert_eq!(BindingId::Tail.as_str(), "tail");
    for (key, action, id, label) in [
        (Key::M, Action::Mute, BindingId::Mute, ",m"),
        (
            Key::R,
            Action::CutawayPicker,
            BindingId::CutawayPicker,
            ",r",
        ),
        (Key::T, Action::TailPicker, BindingId::Tail, ",t"),
    ] {
        let mut bindings = Bindings::default();
        assert_eq!(bindings.key_label(id), label);
        bindings.key(Key::Comma, Modifiers::NONE, false, false);
        assert_eq!(
            bindings.key(key, Modifiers::NONE, false, false),
            Some(action)
        );
        assert!(!bindings.allows_key_repeat(key, Modifiers::NONE));
        // A count refuses instead of acting.
        for prefix in [Key::Num2, Key::Comma] {
            bindings.key(prefix, Modifiers::NONE, false, false);
        }
        assert!(matches!(
            bindings.key(key, Modifiers::NONE, false, false),
            Some(Action::Invalid(_))
        ));
        // Both act on a Visual range as well.
        bindings.clear();
        bindings.key_with_selection(
            Key::Comma,
            Modifiers::NONE,
            false,
            false,
            EditSelection::Range,
        );
        assert_eq!(
            bindings.key_with_selection(key, Modifiers::NONE, false, false, EditSelection::Range),
            Some(action)
        );
        for domain in [RoutingDomain::Original, RoutingDomain::Sound] {
            let mut bindings = Bindings::default();
            bindings.set_routing_domain(domain);
            bindings.key(Key::Comma, Modifiers::NONE, false, false);
            assert_ne!(
                bindings.key(key, Modifiers::NONE, false, false),
                Some(action),
                "{domain:?}"
            );
        }
        // Native text and composition keep their input.
        let mut bindings = Bindings::default();
        bindings.key(Key::Comma, Modifiers::NONE, false, false);
        assert_eq!(bindings.key(key, Modifiers::NONE, true, false), None);
    }
    // The mark prefix `m` is unchanged outside the comma family.
    let mut bindings = Bindings::default();
    bindings.key(Key::M, Modifiers::NONE, false, false);
    assert_eq!(
        bindings.key(Key::A, Modifiers::NONE, false, false),
        Some(Action::SetMark('a'))
    );
}
