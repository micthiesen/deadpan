use super::*;
use deadpan_core::{MarkId, NamedMarkTarget, NodeId, ProjectFrame, SemanticVisualSelection};

fn named(a: &str, b: &str) -> SemanticSelector {
    SemanticSelector::MarkRange {
        start: NamedMarkTarget {
            id: MarkId::new(a).unwrap(),
            occurrence: None,
        },
        end: NamedMarkTarget {
            id: MarkId::new(b).unwrap(),
            occurrence: None,
        },
    }
}
fn context(selection: Option<SemanticVisualSelection>) -> SemanticContext {
    SemanticContext {
        original: None,
        scoped: None,
        hold_duration: None,
        shared_hold_duration: None,
        mixed_hold_duration: None,
        selected_hold_duration: None,
        parent: NodeId::new("root").unwrap(),
        cursor: ProjectFrame(99),
        selected_child: None,
        visual_selection: selection,
    }
}

#[test]
fn named_mark_repeat_receipts_preserve_intent_and_allow_explicit_visual_override() {
    let selector = named("first", "last");
    let register = RegisterName::new('a').unwrap();
    let instructions = [
        SemanticInstruction::Cut {
            selector: selector.clone(),
            register,
        },
        SemanticInstruction::Repeat {
            selector: selector.clone(),
            plays: NonZeroU32::new(3).unwrap(),
            escalation: None,
        },
        SemanticInstruction::Group {
            selector: selector.clone(),
            label: "Marked".into(),
        },
        SemanticInstruction::Duplicate {
            selector: selector.clone(),
        },
    ];
    for instruction in instructions {
        let receipt = LastEdit::from_instruction(&instruction).unwrap();
        assert_eq!(receipt.instruction(&context(None), register), instruction);
        let wire = serde_json::to_value(receipt.instruction(&context(None), register)).unwrap();
        assert_eq!(
            wire["selector"],
            serde_json::json!({"type":"mark_range","start":{"id":"first"},"end":{"id":"last"}})
        );
        let plain = context(Some(SemanticVisualSelection::Time {
            anchor: ProjectFrame(3),
            head: ProjectFrame(7),
            extending: false,
        }));
        let overridden = receipt.instruction(&plain, register);
        assert_eq!(
            serde_json::to_value(overridden).unwrap()["selector"],
            serde_json::json!({"type":"visual_selection"})
        );
        let selected = context(Some(SemanticVisualSelection::MarkRange {
            start: NamedMarkTarget {
                id: MarkId::new("new-start").unwrap(),
                occurrence: None,
            },
            end: NamedMarkTarget {
                id: MarkId::new("new-end").unwrap(),
                occurrence: None,
            },
        }));
        let overridden = receipt.instruction(&selected, register);
        assert_eq!(
            serde_json::to_value(overridden).unwrap()["selector"],
            serde_json::to_value(named("new-start", "new-end")).unwrap()
        );
        assert_eq!(receipt.instruction(&context(None), register), instruction);
    }
}
