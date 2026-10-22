use super::*;
use crate::project::splice::{Destination, Operation, Proposal, ProposalId, Source};
use deadpan_core::{
    FrameDuration, FrameRange, SemanticObjectSelection, SemanticTextObject,
    SemanticVisualSelection, SourceQualificationId,
};

fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}

fn range(start: i64, end: i64) -> FrameRange {
    FrameRange::new(ProjectFrame(start), ProjectFrame(end)).unwrap()
}

fn source() -> Source {
    Source::Original {
        asset: AssetId::new("original").unwrap(),
        qualification: SourceQualificationId::new("a".repeat(64)).unwrap(),
        ordinals: 10..24,
    }
}

fn context() -> SemanticContext {
    SemanticContext {
        hold_duration: None,
        parent: node("root"),
        cursor: ProjectFrame(37),
        selected_child: Some(node("middle")),
        visual_selection: None,
        original: None,
        scoped: None,
    }
}

fn proposal(destination: Destination) -> Proposal {
    Proposal {
        id: ProposalId {
            session: 7,
            project: ProjectId::new("project").unwrap(),
            base_revision: RevisionId::new("base").unwrap(),
            draft: 1,
            change: 1,
        },
        operation: Operation::Copy,
        source: source(),
        scope: SequenceScope::default(),
        parent: node("root"),
        destination,
    }
}

fn eligible(
    context: &SemanticContext,
    proposal: &Proposal,
    captured: Option<FrameRange>,
) -> Result<SemanticInstruction, String> {
    splice_instruction(
        context,
        &[node("left"), node("middle"), node("right")],
        proposal,
        &source(),
        RegisterName::new('w').unwrap(),
        captured,
    )
}

#[test]
fn place_slice_recording_uses_literal_adjacent_slots_even_at_equal_time_boundaries() {
    let register = RegisterName::new('w').unwrap();
    // The retained cursor is intentionally unrelated to the selected child's
    // boundaries. Empty siblings may give all four slots the same timestamp.
    for (slot, before) in [(1, true), (2, false)] {
        assert_eq!(
            eligible(&context(), &proposal(Destination::Slot(slot)), None),
            Ok(SemanticInstruction::Paste { register, before })
        );
    }
    for slot in [0, 3, usize::MAX] {
        assert!(eligible(&context(), &proposal(Destination::Slot(slot)), None).is_err());
    }
    let mut absent = context();
    absent.selected_child = None;
    assert!(eligible(&absent, &proposal(Destination::Slot(1)), None).is_err());
    absent.selected_child = Some(node("not-a-child"));
    assert!(eligible(&absent, &proposal(Destination::Slot(1)), None).is_err());
    let empty = SemanticContext {
        selected_child: None,
        ..context()
    };
    assert_eq!(
        splice_instruction(
            &empty,
            &[],
            &proposal(Destination::Slot(0)),
            &source(),
            register,
            None
        ),
        Ok(SemanticInstruction::Paste {
            register,
            before: true
        })
    );
    assert!(
        splice_instruction(
            &empty,
            &[],
            &proposal(Destination::Slot(1)),
            &source(),
            register,
            None
        )
        .is_err()
    );
}

#[test]
fn place_slice_recording_refuses_move_refined_sources_and_interior_false_equivalences() {
    let base = proposal(Destination::Slot(1));
    let mut changed = base.clone();
    changed.operation = Operation::Move;
    assert!(
        eligible(&context(), &changed, None)
            .unwrap_err()
            .contains("Copy")
    );
    for (asset, qualification, ordinals) in [
        ("original", 'a', 11..24),
        ("original", 'a', 10..25),
        ("other", 'a', 10..24),
        ("original", 'b', 10..24),
    ] {
        changed = base.clone();
        changed.source = Source::Original {
            asset: AssetId::new(asset).unwrap(),
            qualification: SourceQualificationId::new(qualification.to_string().repeat(64))
                .unwrap(),
            ordinals,
        };
        assert!(
            eligible(&context(), &changed, None)
                .unwrap_err()
                .contains("exact source")
        );
    }
    changed = proposal(Destination::Interior {
        target: node("middle"),
        at: FrameDuration::ZERO,
    });
    assert!(eligible(&context(), &changed, None).is_err());
}

#[test]
fn place_slice_recording_requires_nonempty_time_but_preserves_zero_time_object_identity() {
    let selected = range(20, 30);
    let mut target = context();
    target.visual_selection = Some(SemanticVisualSelection::Time {
        anchor: ProjectFrame(30),
        head: ProjectFrame(20),
        extending: false,
    });
    let replace = proposal(Destination::Replace { range: selected });
    let instruction = SemanticInstruction::ReplaceSelection {
        register: RegisterName::new('w').unwrap(),
    };
    assert_eq!(
        eligible(&target, &replace, Some(selected)),
        Ok(instruction.clone())
    );
    assert!(eligible(&target, &replace, Some(range(19, 30))).is_err());
    assert!(eligible(&target, &replace, None).is_err());
    let empty = range(20, 20);
    assert!(
        eligible(
            &target,
            &proposal(Destination::Replace { range: empty }),
            Some(empty)
        )
        .is_err()
    );
    let object = SemanticObjectSelection {
        kind: SemanticTextObject::AroundBeat,
        group: node("middle"),
    };
    target.visual_selection = Some(SemanticVisualSelection::Object {
        selection: object.clone(),
        extending: false,
    });
    assert_eq!(
        eligible(
            &target,
            &proposal(Destination::Object {
                selection: object.clone()
            }),
            Some(selected)
        ),
        Ok(instruction)
    );
    assert!(eligible(&target, &replace, Some(selected)).is_err());
    assert!(
        eligible(&target, &proposal(Destination::Slot(1)), Some(selected))
            .unwrap_err()
            .contains("replaces that object")
    );
    let wrong = SemanticObjectSelection {
        kind: SemanticTextObject::InnerBeat,
        ..object.clone()
    };
    assert!(
        eligible(
            &target,
            &proposal(Destination::Object { selection: wrong }),
            Some(selected)
        )
        .is_err()
    );
    assert_eq!(
        eligible(
            &target,
            &proposal(Destination::Object { selection: object }),
            Some(empty)
        )
        .unwrap(),
        SemanticInstruction::ReplaceSelection {
            register: RegisterName::new('w').unwrap()
        }
    );
    assert!(eligible(&context(), &replace, Some(selected)).is_err());
}
