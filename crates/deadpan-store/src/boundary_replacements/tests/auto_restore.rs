use super::*;
use crate::generation_intents::{InputUnavailableCause, IntentInputBinding};
use crate::generation_preparations::{PreparationState, StoredGenerationPreparation};
use crate::{AccessMode, CommitOutcome, ProjectStore};

// This fixture has no worker requests. Keep the host's additional preparation
// veto open while the store independently validates every captured input and
// durable intent. The established insertion fixtures use the same resolver.
struct Relevant;
impl crate::generation::GenerationContextResolver for Relevant {
    fn observe(
        &self,
        _: &ProjectDocument,
        _: &ProjectDocument,
        _: &crate::generation::StoredGenerationRequest,
    ) -> crate::generation::ContextObservation {
        crate::generation::ContextObservation::Unresolved
    }
    fn preparation_is_relevant(
        &self,
        _: &ProjectDocument,
        _: &ProjectDocument,
        _: &StoredGenerationPreparation,
    ) -> bool {
        true
    }
}

fn commit(store: &mut ProjectStore, name: &str, command: Command) -> CommitOutcome {
    let document = store.snapshot().unwrap();
    store
        .commit(&CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision(name),
            command,
        })
        .unwrap()
}

#[track_caller]
fn current(store: &ProjectStore) -> StoredGenerationPreparation {
    let rows = store.generation_preparations(None, 16).unwrap();
    assert_eq!(
        rows.len(),
        1,
        "one durable active preparation follows the exact pause"
    );
    rows.into_iter().next().unwrap()
}

fn missing(preparation: &StoredGenerationPreparation) {
    assert!(matches!(
        preparation.intent.input_binding,
        IntentInputBinding::Unavailable {
            cause: InputUnavailableCause::MissingContext,
            ..
        }
    ));
}

fn insert_neighbor(name: &str, index: usize, duration: i64) -> Command {
    Command::Insert {
        parent: id("root"),
        index,
        subtree: Subtree {
            root: id(name),
            overrides: BTreeMap::new(),
            gap_overrides: BTreeMap::new(),
            nodes: BTreeMap::from([(
                id(name),
                BeatNode::hold(
                    name,
                    HoldRecipe {
                        duration: frames(duration),
                        ..hold()
                    },
                ),
            )]),
        },
    }
}

#[test]
fn unresolved_auto_resolves_once_then_survives_short_context_history_and_reopen() {
    for (index, direction) in [
        (0, ExtensionDirection::FromLeft),
        (1, ExtensionDirection::FromRight),
    ] {
        let scratch = tempfile::tempdir().unwrap();
        let package = scratch.path().join("automatic.deadpan");
        let mut store = ProjectStore::create(&package, &fixture(&[], &[])).unwrap();
        store.set_generation_context_resolver(Arc::new(Relevant));
        let inserted = commit(
            &mut store,
            "insert-ai",
            Command::InsertAiTime {
                at: ProjectFrame(0),
                hold: HoldRecipe {
                    duration: frames(4),
                    ..hold()
                },
                id: id("ai"),
                identities: SplitIdentities { nodes: vec![] },
                timing: AudioTimingId {
                    allocation: revision("insert-ai"),
                    ordinal: 0,
                },
            },
        );
        assert_eq!(inserted.generation_preparations.len(), 1);
        let unresolved = current(&store);
        assert_eq!(unresolved.intent.capture, None);
        missing(&unresolved);
        let unchanged = commit(
            &mut store,
            "rename",
            Command::Rename {
                node: id("ai"),
                label: "My pause".into(),
            },
        );
        assert!(unchanged.generation_preparations.is_empty());
        assert_eq!(current(&store).id, unresolved.id);

        // An endpoint fixes Auto's direction even while the available context
        // is too short. This is a real durable renewal with no request/model.
        let resolved = commit(
            &mut store,
            "neighbor",
            insert_neighbor("neighbor", index, 1),
        );
        assert_eq!(resolved.generation_preparations.len(), 1);
        let short = current(&store);
        assert_ne!(short.id, unresolved.id);
        let captured = short.intent.capture.unwrap();
        assert!(
            matches!(captured, GenerationCaptureSpec::Extension { direction: actual, .. } if actual == direction)
        );
        missing(&short);
        store.validate_full().unwrap();

        let restored = commit(
            &mut store,
            "enough-context",
            Command::SetHoldDuration {
                node: id("neighbor"),
                duration: frames(12),
            },
        );
        assert_eq!(restored.generation_preparations.len(), 1);
        let ready_inputs = current(&store);
        assert_ne!(ready_inputs.id, short.id);
        assert_eq!(ready_inputs.intent.capture, Some(captured));
        assert!(matches!(
            ready_inputs.intent.input_binding,
            IntentInputBinding::Measured { .. }
        ));

        // Both endpoints now exist, but a previously resolved extension never
        // silently turns into a bridge.
        let opposite_index = if index == 0 { 2 } else { 0 };
        commit(
            &mut store,
            "opposite",
            insert_neighbor("opposite", opposite_index, 12),
        );
        let opposite = current(&store);
        assert_eq!(opposite.intent.capture, Some(captured));
        store.validate_full().unwrap();
        // Section 12.6: Undo cancels relevance. Restoring authored pictures
        // cannot resurrect either the latest or an older preparation activation.
        let undo = store
            .undo(&store.head_revision().unwrap(), revision("undo-opposite"))
            .unwrap();
        assert!(undo.generation_preparations.is_empty());
        assert!(store.generation_preparations(None, 16).unwrap().is_empty());
        assert_eq!(
            store
                .generation_preparation(&opposite.id)
                .unwrap()
                .unwrap()
                .state,
            PreparationState::Cancelled
        );
        assert_eq!(
            store
                .generation_preparation(&ready_inputs.id)
                .unwrap()
                .unwrap()
                .state,
            PreparationState::Cancelled
        );
        let undo = store
            .undo(&store.head_revision().unwrap(), revision("undo-context"))
            .unwrap();
        assert!(undo.generation_preparations.is_empty());
        assert!(store.generation_preparations(None, 16).unwrap().is_empty());
        store.validate_full().unwrap();
        let redo = store
            .redo(&store.head_revision().unwrap(), revision("redo-context"))
            .unwrap();
        assert_eq!(redo.generation_preparations.len(), 1);
        let redone = current(&store);
        assert_ne!(redone.id, ready_inputs.id);
        assert_ne!(redone.id, opposite.id);
        assert_eq!(redone.intent.capture, Some(captured));
        assert_eq!(
            redone.intent.authorization,
            crate::generation_intents::IntentAuthorization::Redo {
                original_activation: ready_inputs.id,
            }
        );
        assert!(matches!(
            redone.intent.input_binding,
            IntentInputBinding::Measured { .. }
        ));
        store.validate_full().unwrap();
        let snapshot = store.snapshot().unwrap();
        drop(store);
        let reopened = ProjectStore::open(&package, AccessMode::ReadOnly).unwrap();
        reopened.validate_full().unwrap();
        assert_eq!(reopened.snapshot().unwrap(), snapshot);
        assert_eq!(current(&reopened).intent.capture, Some(captured));
    }
}

#[test]
fn unresolved_auto_chooses_bridge_when_both_endpoints_arrive_together() {
    let document = fixture(&["left", "pending", "right"], &[]);
    let connection = connection();
    let result = derive_with_bindings(
        &connection,
        &ValidatedDocument::new(Arc::new(document)).unwrap(),
        &BTreeSet::new(),
        &BTreeMap::from([(
            id("pending"),
            IntentCaptureSettings::Unresolved {
                preference: deadpan_jobs::GenerationModePreference::Automatic,
                region: None,
            },
        )]),
        None,
    )
    .unwrap();
    assert_eq!(
        result.bindings[&target("pending")].capture_spec(),
        GenerationCaptureSpec::Bridge
    );
    assert!(result.unavailable.is_empty());
}

#[test]
fn unresolved_explicit_preference_is_not_automatic_resolution_authority() {
    let document = fixture(&["left", "pending", "right"], &[]);
    let connection = connection();
    let result = derive_with_bindings(
        &connection,
        &ValidatedDocument::new(Arc::new(document)).unwrap(),
        &BTreeSet::new(),
        &BTreeMap::from([(
            id("pending"),
            IntentCaptureSettings::Unresolved {
                preference: deadpan_jobs::GenerationModePreference::Bridge,
                region: None,
            },
        )]),
        None,
    );
    assert!(result.is_err());
}
