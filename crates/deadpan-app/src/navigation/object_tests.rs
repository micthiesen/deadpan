use super::*;
use deadpan_core::{SemanticSelector as Selector, SemanticTextObject as Object};

fn key(bindings: &mut Bindings, key: Key, selection: EditSelection) -> Option<Action> {
    bindings.key_with_selection(key, Modifiers::NONE, false, false, selection)
}

#[test]
fn group_objects_compose_with_operators_and_visual_selection() {
    for (prefix, object) in [(Key::I, Object::InnerGroup), (Key::A, Object::AroundGroup)] {
        for operator in [Key::D, Key::Y, Key::R] {
            let mut bindings = Bindings::default();
            assert_eq!(
                key(&mut bindings, operator, EditSelection::None),
                Some(Action::OfferInsert)
            );
            assert_eq!(key(&mut bindings, prefix, EditSelection::None), None);
            assert!(bindings.pending_hint().unwrap().contains("group"));
            let selector = Selector::TextObject { object };
            assert_eq!(
                key(&mut bindings, Key::G, EditSelection::None),
                Some(if operator == Key::R {
                    Action::Repeat {
                        selector,
                        plays: std::num::NonZeroU32::new(2).unwrap(),
                    }
                } else {
                    Action::Operator {
                        cut: operator == Key::D,
                        selector,
                    }
                })
            );
        }
        for selection in [
            EditSelection::Empty,
            EditSelection::Range,
            EditSelection::Object,
        ] {
            let mut bindings = Bindings::default();
            assert_eq!(key(&mut bindings, prefix, selection), None);
            assert_eq!(
                key(&mut bindings, Key::G, selection),
                Some(Action::SelectObject(object))
            );
        }
    }
}

#[test]
fn object_count_means_total_repeat_plays_but_never_multiple_groups() {
    for operator in [Key::D, Key::Y, Key::R] {
        let mut bindings = Bindings::default();
        for stroke in [Key::Num3, operator, Key::I] {
            key(&mut bindings, stroke, EditSelection::None);
        }
        let action = key(&mut bindings, Key::G, EditSelection::None);
        if operator == Key::R {
            assert_eq!(
                action,
                Some(Action::Repeat {
                    selector: Selector::TextObject {
                        object: Object::InnerGroup
                    },
                    plays: std::num::NonZeroU32::new(3).unwrap(),
                })
            );
        } else {
            assert!(matches!(action, Some(Action::Invalid(_))));
        }
    }
    let mut bindings = Bindings::default();
    for stroke in [Key::R, Key::Num3, Key::I] {
        key(&mut bindings, stroke, EditSelection::None);
    }
    assert!(matches!(
        key(&mut bindings, Key::G, EditSelection::None),
        Some(Action::Invalid(_))
    ));
}

#[test]
fn remapped_objects_are_shared_by_visual_and_operator_grammar() {
    let mut bindings = Bindings::from_json(br#"{"version":1,"key_mode":"logical","bindings":[{"action":"object.inner_group","keys":[["F7","F8"]]}]}"#).unwrap();
    assert_eq!(bindings.key_label(BindingId::InnerGroup), "F7 F8");
    for selection in [EditSelection::None, EditSelection::Object] {
        if selection == EditSelection::None {
            key(&mut bindings, Key::Y, selection);
        }
        assert_eq!(key(&mut bindings, Key::F7, selection), None);
        assert_eq!(
            key(&mut bindings, Key::F8, selection),
            Some(if selection == EditSelection::None {
                Action::Operator {
                    cut: false,
                    selector: Selector::TextObject {
                        object: Object::InnerGroup,
                    },
                }
            } else {
                Action::SelectObject(Object::InnerGroup)
            })
        );
    }
}

#[test]
fn native_text_and_composition_cancel_pending_object_paths() {
    for (text, ime) in [(true, false), (false, true), (true, true)] {
        for selection in [EditSelection::None, EditSelection::Object] {
            let mut bindings = Bindings::default();
            if selection == EditSelection::None {
                key(&mut bindings, Key::D, selection);
            }
            key(&mut bindings, Key::I, selection);
            assert_eq!(
                bindings.key_with_selection(Key::G, Modifiers::NONE, text, ime, selection),
                None
            );
            assert!(bindings.pending().is_empty());
        }
    }
}
