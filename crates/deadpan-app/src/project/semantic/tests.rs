use super::*;
use deadpan_core::{
    FrameRange, MarkId, NamedMarkTarget, NodeId, ProjectFrame, SemanticVisualSelection,
};

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

#[test]
fn transcript_search_dot_retains_query_and_accepts_an_explicit_new_visual_override() {
    let query = deadpan_core::SemanticTranscriptQuery::new("hello world").unwrap();
    let selector = SemanticSelector::TranscriptSearch {
        query: query.clone(),
    };
    let register = RegisterName::new('a').unwrap();
    let instruction = SemanticInstruction::Cut {
        selector: selector.clone(),
        register,
    };
    let receipt = LastEdit::from_instruction(&instruction).unwrap();
    assert_eq!(receipt.instruction(&context(None), register), instruction);
    let selection = context(Some(SemanticVisualSelection::TranscriptSearch {
        query,
        ranges: vec![FrameRange::new(ProjectFrame(1), ProjectFrame(4)).unwrap()],
    }));
    assert_eq!(receipt.instruction(&selection, register), instruction);
    let manual = context(Some(SemanticVisualSelection::Time {
        anchor: ProjectFrame(8),
        head: ProjectFrame(11),
        extending: false,
    }));
    assert!(matches!(
        receipt.instruction(&manual, register),
        SemanticInstruction::Cut {
            selector: SemanticSelector::VisualSelection,
            ..
        }
    ));
    let another = deadpan_core::SemanticTranscriptQuery::new("another phrase").unwrap();
    let new_search = context(Some(SemanticVisualSelection::TranscriptSearch {
        query: another.clone(),
        ranges: vec![FrameRange::new(ProjectFrame(12), ProjectFrame(15)).unwrap()],
    }));
    assert!(
        matches!(receipt.instruction(&new_search, register), SemanticInstruction::Cut { selector: SemanticSelector::TranscriptSearch { query }, .. } if query == another)
    );
    assert_eq!(
        serde_json::to_value(selector).unwrap(),
        serde_json::json!({"type":"transcript_search","query":"hello world"})
    );
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

#[test]
fn explicit_node_receipts_keep_identity_until_a_new_visual_selection_overrides_it() {
    let node = NodeId::new("exact-node").unwrap();
    let selector = SemanticSelector::Node { node: node.clone() };
    let register = RegisterName::unnamed();
    for instruction in [
        SemanticInstruction::Cut {
            selector: selector.clone(),
            register,
        },
        SemanticInstruction::Repeat {
            selector: selector.clone(),
            plays: NonZeroU32::new(2).unwrap(),
            escalation: None,
        },
        SemanticInstruction::Group {
            selector: selector.clone(),
            label: "Node group".into(),
        },
        SemanticInstruction::Duplicate {
            selector: selector.clone(),
        },
    ] {
        let receipt = LastEdit::from_instruction(&instruction).unwrap();
        assert_eq!(receipt.instruction(&context(None), register), instruction);
        assert_eq!(
            serde_json::to_value(&instruction).unwrap()["selector"],
            serde_json::json!({"type":"node","node":"exact-node"})
        );
        let selected = context(Some(SemanticVisualSelection::Node {
            node: NodeId::new("new-node").unwrap(),
        }));
        assert_eq!(
            serde_json::to_value(receipt.instruction(&selected, register)).unwrap()["selector"],
            serde_json::json!({"type":"node","node":"new-node"})
        );
        let time = context(Some(SemanticVisualSelection::Time {
            anchor: ProjectFrame(3),
            head: ProjectFrame(7),
            extending: false,
        }));
        assert_eq!(
            serde_json::to_value(receipt.instruction(&time, register)).unwrap()["selector"],
            serde_json::json!({"type":"visual_selection"})
        );
    }
}

#[test]
fn native_visual_range_set_cut_retains_new_selection_requirement() {
    let register = RegisterName::unnamed();
    let instruction = SemanticInstruction::CutSelection { register };
    let receipt = LastEdit::from_instruction(&instruction).unwrap();
    assert_eq!(
        receipt.operation,
        RepeatableEdit::Cut(RepeatableCut::Selector(SemanticSelector::VisualSelection))
    );
    let ranges = vec![
        FrameRange::new(ProjectFrame(2), ProjectFrame(6)).unwrap(),
        FrameRange::new(ProjectFrame(10), ProjectFrame(14)).unwrap(),
    ];
    let selected = context(Some(SemanticVisualSelection::Ranges { ranges }));
    assert!(matches!(
        receipt.instruction(&selected, register),
        SemanticInstruction::Cut {
            selector: SemanticSelector::VisualSelection,
            ..
        }
    ));
    assert!(matches!(
        receipt.instruction(&context(None), register),
        SemanticInstruction::Cut {
            selector: SemanticSelector::VisualSelection,
            ..
        }
    ));
}

#[test]
fn node_set_cut_receipt_keeps_the_requested_ids_and_allows_explicit_visual_override() {
    let nodes = vec![
        NodeId::new("later").unwrap(),
        NodeId::new("empty").unwrap(),
        NodeId::new("earlier").unwrap(),
    ];
    let register = RegisterName::unnamed();
    let instruction = SemanticInstruction::Cut {
        selector: SemanticSelector::Nodes {
            nodes: nodes.clone(),
        },
        register,
    };
    let receipt = LastEdit::from_instruction(&instruction).unwrap();
    assert_eq!(receipt.instruction(&context(None), register), instruction);
    assert_eq!(
        serde_json::to_value(&instruction).unwrap()["selector"],
        serde_json::json!({"type":"nodes","nodes":["later","empty","earlier"]})
    );
    let set = context(Some(SemanticVisualSelection::Nodes {
        nodes: vec![NodeId::new("new").unwrap()],
    }));
    assert_eq!(
        serde_json::to_value(receipt.instruction(&set, register)).unwrap()["selector"],
        serde_json::json!({"type":"nodes","nodes":["new"]})
    );
    let time = context(Some(SemanticVisualSelection::Time {
        anchor: ProjectFrame(2),
        head: ProjectFrame(4),
        extending: false,
    }));
    assert_eq!(
        serde_json::to_value(receipt.instruction(&time, register)).unwrap()["selector"],
        serde_json::json!({"type":"visual_selection"})
    );
}
