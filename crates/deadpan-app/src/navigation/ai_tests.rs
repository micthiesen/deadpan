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
        Some(Action::Ai(AiAction::Generate))
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
        Some(Action::Ai(AiAction::Generate))
    );
    for domain in [RoutingDomain::Original, RoutingDomain::Sound] {
        let mut bindings = Bindings::default();
        bindings.set_routing_domain(domain);
        bindings.key(Key::Comma, Modifiers::NONE, false, false);
        assert_ne!(
            bindings.key(Key::A, Modifiers::NONE, false, false),
            Some(Action::Ai(AiAction::Generate)),
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
fn ai_commands_parse_without_arguments() {
    use command::{Entry, parse};
    for (input, action) in [
        (":generate", AiAction::Generate),
        ("generate-ai", AiAction::Generate),
        ("cancel-ai", AiAction::Cancel),
        ("PREVIEW-AI", AiAction::Preview),
        ("accept-ai", AiAction::Accept),
        ("discard-ai", AiAction::Discard),
    ] {
        assert_eq!(
            parse(input),
            Ok(Entry::Action(Action::Ai(action))),
            "{input}"
        );
    }
    for input in ["generate now", "accept-ai 1", "cancel-ai x"] {
        assert!(parse(input).is_err(), "{input}");
    }
}

#[test]
fn comma_m_mutes_and_comma_r_picks_a_cutaway_in_your_edit_only() {
    assert_eq!(BindingId::Mute.as_str(), "gain.mute");
    assert_eq!(BindingId::CutawayPicker.as_str(), "cutaway.pick");
    for (key, action, id, label) in [
        (Key::M, Action::Mute, BindingId::Mute, ",m"),
        (
            Key::R,
            Action::CutawayPicker,
            BindingId::CutawayPicker,
            ",r",
        ),
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
