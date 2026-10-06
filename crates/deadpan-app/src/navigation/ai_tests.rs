use super::*;

#[test]
fn comma_a_generates_ai_pictures_once_in_normal_edit_only() {
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
        Some(Action::Ai(AiAction::Generate { variants: 1 }))
    );
    assert!(bindings.pending().is_empty());
    assert!(!bindings.allows_key_repeat(Key::A, Modifiers::NONE));

    // A count refuses instead of generating several times.
    for key in [Key::Num3, Key::Comma] {
        bindings.key(key, Modifiers::NONE, false, false);
    }
    assert_eq!(bindings.pending(), "3,");
    assert!(matches!(
        bindings.key(Key::A, Modifiers::NONE, false, false),
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
        bindings.key_with_selection(Key::A, Modifiers::NONE, false, false, EditSelection::Range),
        Some(Action::Ai(AiAction::Generate { variants: 1 }))
    );
    for domain in [RoutingDomain::Original, RoutingDomain::Sound] {
        let mut bindings = Bindings::default();
        bindings.set_routing_domain(domain);
        bindings.key(Key::Comma, Modifiers::NONE, false, false);
        assert_ne!(
            bindings.key(Key::A, Modifiers::NONE, false, false),
            Some(Action::Ai(AiAction::Generate { variants: 1 })),
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
