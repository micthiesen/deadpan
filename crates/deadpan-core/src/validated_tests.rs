//! Validation reused through `ValidatedDocument` must agree with complete
//! validation, accepting and rejecting exactly the same states.
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::*;

fn id(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn hold(frames: i64) -> HoldRecipe {
    HoldRecipe {
        picture_context: None,
        duration: FrameDuration::new(frames).unwrap(),
        video: HoldVideo::Background,
        audio: HoldAudio::Silence,
    }
}

fn request(document: &ProjectDocument, revision: &str, command: Command) -> CommandRequest {
    CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(revision).unwrap(),
        command,
    }
}

fn pause(document: &ProjectDocument, revision: &str, at: i64) -> CommandRequest {
    let target = document.insert_time_target(ProjectFrame(at)).unwrap();
    let needed = target.split.map_or(0, |split| split.required_ids);
    request(
        document,
        revision,
        Command::InsertTime {
            at: ProjectFrame(at),
            hold: hold(2),
            id: id(&format!("{revision}-pause")),
            identities: SplitIdentities {
                nodes: (0..needed)
                    .map(|index| id(&format!("{revision}-split-{index}")))
                    .collect(),
            },
            timing: AudioTimingId {
                allocation: RevisionId::new(revision).unwrap(),
                ordinal: 0,
            },
        },
    )
}

fn complete(document: &ProjectDocument) -> Result<ValidatedDocument, DocumentError> {
    ValidatedDocument::new(Arc::new(document.clone()))
}

/// A project whose Holds all carry retained clocks.
fn bound() -> ValidatedDocument {
    let mut document = ProjectDocument::new(
        ProjectId::new("validated").unwrap(),
        RevisionId::new("r0").unwrap(),
        PresentationBasis {
            width: 16,
            height: 16,
            frame_rate: FrameRate::new(24, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        id("root"),
    )
    .unwrap();
    let mut nodes = BTreeMap::new();
    let mut children = Vec::new();
    for index in 0..12 {
        let node = id(&format!("b{index:02}"));
        nodes.insert(node.clone(), BeatNode::hold("Beat", hold(4)));
        children.push(node);
    }
    nodes.insert(id("beats"), BeatNode::sequence("Beats", children));
    let insert = request(
        &document,
        "r1",
        Command::Insert {
            parent: id("root"),
            index: 0,
            subtree: Subtree {
                root: id("beats"),
                nodes,
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        },
    );
    document = apply(&document, &insert)
        .unwrap()
        .forward
        .apply(&document)
        .unwrap();
    let mut validated = complete(&document).unwrap();
    for (index, at) in [(2, 6), (3, 19), (4, 31)] {
        let next = pause(&validated, &format!("r{index}"), at);
        validated = apply_validated(&validated, &next).unwrap().1;
    }
    assert!(validated.audio_bindings().bindings().len() >= 12);
    validated
}

#[test]
fn reused_proofs_equal_complete_validation_through_edits() {
    let mut validated = bound();
    let commands = [
        Command::WrapRepeat {
            node: id("b03"),
            id: id("wrapped"),
            plays: 3,
            gap: Some(hold(1)),
            anchor_policy: Default::default(),
        },
        Command::SetHoldDuration {
            node: id("b07"),
            duration: FrameDuration::new(6).unwrap(),
        },
        Command::DeleteRipple {
            node: id("b09"),
            timing: AudioTimingId {
                allocation: RevisionId::new("r7").unwrap(),
                ordinal: 0,
            },
        },
    ];
    for (index, command) in commands.into_iter().enumerate() {
        let revision = format!("r{}", index + 5);
        let next = request(&validated, &revision, command);
        crate::audio_binding::REUSED_OWNERS.with(|count| count.set(0));
        let (edit, reused) = apply_validated(&validated, &next).unwrap();
        // Most owners keep their bindings and tables, so their checks are reused.
        assert!(crate::audio_binding::REUSED_OWNERS.with(std::cell::Cell::get) >= 6);
        let full = complete(&reused).unwrap();
        assert_eq!(reused.durations(), full.durations());
        assert_eq!(reused.binding_proof(), full.binding_proof());
        assert_eq!(edit, apply(&validated, &next).unwrap());
        validated = reused;
        let next = pause(&validated, &format!("{revision}-pause"), 3);
        let (_, reused) = apply_validated(&validated, &next).unwrap();
        assert_eq!(
            reused.binding_proof(),
            complete(&reused).unwrap().binding_proof()
        );
        validated = reused;
    }
}

#[test]
fn reused_proofs_still_reject_every_changed_owner_and_table() {
    let validated = bound();
    let owner = validated
        .audio_bindings()
        .bindings()
        .keys()
        .next()
        .unwrap()
        .clone();
    let reject = |document: ProjectDocument| {
        let patch =
            AudioBindingPatch::between(validated.audio_bindings(), document.audio_bindings());
        assert!(patch.is_some());
        assert!(complete(&document).is_err());
        assert!(document.validate_after(&validated, patch.as_ref()).is_err());
    };
    // A changed binding naming an absent physical alias.
    let mut changed = ProjectDocument::clone(&validated);
    changed
        .audio_bindings
        .bindings
        .get_mut(&owner)
        .unwrap()
        .lattice
        .reference
        .physical = id("absent");
    reject(changed);
    // A removed table still named by unchanged owners.
    let mut removed = ProjectDocument::clone(&validated);
    let timing = removed.audio_bindings.bindings[&owner]
        .lattice
        .reference
        .timing
        .clone();
    removed.audio_bindings.timings.remove(&timing);
    reject(removed);
    // An unchanged binding whose owner became a Sequence.
    let mut retyped = ProjectDocument::clone(&validated);
    retyped.nodes.get_mut(&owner).unwrap().kind = NodeKind::Sequence {
        children: Vec::new(),
    };
    let patch = AudioBindingPatch::between(validated.audio_bindings(), retyped.audio_bindings());
    assert!(patch.is_none());
    assert!(complete(&retyped).is_err());
    assert!(retyped.validate_after(&validated, None).is_err());
}

#[test]
fn scoped_binding_validation_rechecks_owners_of_a_replaced_timing_table() {
    let validated = bound();
    let owner = validated
        .audio_bindings()
        .bindings()
        .keys()
        .next()
        .unwrap()
        .clone();
    let timing = validated.audio_bindings().bindings()[&owner]
        .lattice
        .reference
        .timing
        .clone();
    // Same timing identity, different tables: one from a structure whose
    // durations differ, one from a structure without the owners' aliases.
    let mut longer = ProjectDocument::clone(&validated);
    let NodeKind::Hold { recipe } = &mut longer.nodes.get_mut(&owner).unwrap().kind else {
        panic!("bound owner is a Hold")
    };
    recipe.duration = FrameDuration::new(9).unwrap();
    let mut unrelated = ProjectDocument::clone(&validated);
    unrelated.audio_bindings = AudioBindingState::default();
    for node in unrelated.nodes.values_mut() {
        if let NodeKind::Sequence { children } = &mut node.kind {
            children.retain(|child| child != &owner);
        }
    }
    unrelated.nodes.remove(&owner);
    unrelated.audio_lineage.remove(&owner);
    for (index, mut source) in [longer, unrelated].into_iter().enumerate() {
        source.audio_bindings = AudioBindingState::default();
        let table = FrozenAudioLayout::capture(&source).unwrap();
        let mut state = validated.audio_bindings().clone();
        state.timings.insert(timing.clone(), table);
        // Reuse through the scoped head must not skip any owner naming the
        // replaced table: the outcome equals unscoped validation.
        let scoped = validated.scope(|| validated.validate_bindings_in_scope(&state));
        let unscoped = state.validate_for(&validated);
        let reference = crate::with_reference_command_work(|| {
            validated.scope(|| validated.validate_bindings_in_scope(&state))
        });
        assert_eq!(scoped, unscoped);
        assert_eq!(scoped, reference);
        // A table without the owners' aliases must be refused.
        assert!(index == 0 || scoped.is_err());
    }
}
