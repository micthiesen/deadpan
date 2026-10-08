//! Durable provider-tail coverage using synthetic qualified bundle objects.
//! This proves transactions and history, not model inference or video decoding.
use super::*;
use deadpan_core::{
    AudioTimingId, BoundaryReplacement, BoundaryReplacementEdit, LeafEdit, ResolvedStep,
    ResolvedTransaction,
};
use deadpan_store::generation::GenerationContextResolver;
use deadpan_store::generation_intents::{IntentAuthorization, IntentCause};
use deadpan_store::generation_pictures::GenerationPictures;
use deadpan_store::generation_preparations::{PreparationState, StoredGenerationPreparation};

struct Measured;
impl GenerationContextResolver for Measured {
    fn observe(
        &self,
        _: &ProjectDocument,
        _: &ProjectDocument,
        _: &StoredGenerationRequest,
    ) -> ContextObservation {
        ContextObservation::Unresolved
    }
    fn observe_with_pictures(
        &self,
        origin: &ProjectDocument,
        after: &ProjectDocument,
        request: &StoredGenerationRequest,
        pictures: &dyn GenerationPictures,
    ) -> ContextObservation {
        let old = deadpan_store::generation_inputs::GenerationInputCapture::capture(
            origin,
            &request.origin_target,
            pictures,
        );
        let new = deadpan_store::generation_inputs::GenerationInputCapture::capture(
            after,
            &request.target,
            pictures,
        );
        if matches!((old, new), (Ok(old), Ok(new)) if old == new) {
            ContextObservation::Resolved(request.binding.context_sha256.clone())
        } else {
            ContextObservation::Unresolved
        }
    }
    fn preparation_is_relevant_with_pictures(
        &self,
        origin: &ProjectDocument,
        after: &ProjectDocument,
        preparation: &StoredGenerationPreparation,
        pictures: &dyn GenerationPictures,
    ) -> bool {
        matches!((deadpan_store::generation_inputs::GenerationInputCapture::capture(origin, &preparation.origin_target, pictures),
            deadpan_store::generation_inputs::GenerationInputCapture::capture(after, &preparation.target, pictures)),
            (Ok(old), Ok(new)) if old == new)
    }
}

fn accepted(package: &Path) -> Result<ProjectStore> {
    let mut store = ProjectStore::create(package, &document()?)?;
    let input = ready_for_acceptance(&mut store)?;
    store.accept_generation_bundle(
        &input,
        &unchanged_relevance(&store, &input.new_revision)?,
        media_limits(),
    )?;
    store.set_generation_context_resolver(std::sync::Arc::new(Measured));
    Ok(store)
}

fn request(store: &ProjectStore, revision: &str, command: Command) -> Result<CommandRequest> {
    let document = store.snapshot()?;
    Ok(CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(revision)?,
        command,
    })
}

fn black_neighbor() -> Result<Command> {
    let id = NodeId::new("neighbor")?;
    Ok(Command::Insert {
        parent: NodeId::new("root")?,
        index: 1,
        subtree: Subtree {
            root: id.clone(),
            nodes: BTreeMap::from([(
                id,
                BeatNode::hold(
                    "Black",
                    HoldRecipe {
                        duration: FrameDuration::new(5)?,
                        video: HoldVideo::Background,
                        audio: HoldAudio::Silence,
                        picture_context: None,
                    },
                ),
            )]),
            overrides: BTreeMap::new(),
            gap_overrides: BTreeMap::new(),
        },
    })
}

fn remove_neighbor(revision: &str) -> Result<Command> {
    Ok(Command::DeleteRipple {
        node: NodeId::new("neighbor")?,
        timing: AudioTimingId {
            allocation: RevisionId::new(revision)?,
            ordinal: 0,
        },
    })
}

fn hold_video(document: &ProjectDocument) -> &HoldVideo {
    let NodeKind::Hold { recipe } = &document.nodes()[&NodeId::new("hold").unwrap()].kind else {
        panic!()
    };
    &recipe.video
}

#[test]
fn boundary_edit_and_replacement_are_one_transaction_with_fresh_redo_and_cancellation() -> Result {
    let scratch = tempfile::tempdir()?;
    let package = scratch.path().join("boundary.deadpan");
    let mut store = accepted(&package)?;
    let before = store.snapshot()?;
    let edit = request(&store, "neighbor-added", black_neighbor()?)?;
    let preview = store.preview(&edit)?;
    assert_eq!(store.snapshot()?, before);
    assert!(store.generation_intents(None, 10)?.is_empty());
    let committed = store.commit(&edit)?;
    assert_eq!(committed.edit, preview);
    assert_eq!(committed.edit.duration_delta, 5);
    assert_eq!(hold_video(&store.snapshot()?), &HoldVideo::Background);
    let first = committed.generation_preparations[0].clone();
    let birth = store.generation_preparation(&first)?.unwrap();
    assert_eq!(birth.intent.cause, IntentCause::SourceBoundaryChanged);
    assert_eq!(birth.duration, FrameDuration::new(12)?);
    assert_eq!(birth.state, PreparationState::Queued);
    let database = Connection::open(package.join("project.sqlite"))?;
    let json: String = database.query_row(
        "SELECT request FROM history WHERE revision_id='neighbor-added'",
        [],
        |row| row.get(0),
    )?;
    assert!(matches!(
        serde_json::from_str::<CommandRequest>(&json)?.command,
        Command::WithBoundaryReplacements { .. }
    ));
    store.validate_full()?;
    store.undo(&edit.new_revision, RevisionId::new("undone")?)?;
    assert_eq!(hold_video(&store.snapshot()?), hold_video(&before));
    assert!(store.generation_intents(None, 10)?.is_empty());
    let redone = store.redo(&RevisionId::new("undone")?, RevisionId::new("redone")?)?;
    let second = redone.generation_preparations[0].clone();
    assert_ne!(first, second);
    assert!(
        matches!(store.generation_preparation(&second)?.unwrap().intent.authorization,
        IntentAuthorization::Redo { original_activation } if original_activation == first)
    );
    let renewed = store.commit(&request(
        &store,
        "neighbor-removed",
        remove_neighbor("neighbor-removed")?,
    )?)?;
    let third = renewed.generation_preparations[0].clone();
    assert!(
        matches!(store.generation_preparation(&third)?.unwrap().intent.authorization,
        IntentAuthorization::Renewal { predecessor } if predecessor == second)
    );
    assert!(store.cancel_generation_intent(&third, &store.head_revision()?)?);
    let no_resurrection =
        store.commit(&request(&store, "neighbor-restored", black_neighbor()?)?)?;
    assert!(no_resurrection.generation_preparations.is_empty());
    store.validate_full()?;
    drop(store);
    ProjectStore::open(&package, AccessMode::ReadOnly)?.validate_full()?;
    Ok(())
}

#[test]
fn compound_replacement_preserves_leaf_checkpoints_and_compares_only_final_boundaries() -> Result {
    for restores_boundary in [false, true] {
        let scratch = tempfile::tempdir()?;
        let package = scratch.path().join("compound-boundary.deadpan");
        let mut store = accepted(&package)?;
        let before = store.snapshot()?;
        let second = if restores_boundary {
            remove_neighbor("leaf-1")?
        } else {
            Command::SetHoldDuration {
                node: NodeId::new("hold")?,
                duration: FrameDuration::new(8)?,
            }
        };
        let command = request(
            &store,
            "compound-boundary",
            Command::Compound {
                transaction: ResolvedTransaction::new(
                    0,
                    BTreeMap::new(),
                    vec![
                        ResolvedStep::Edit {
                            edit: LeafEdit::new(RevisionId::new("leaf-0")?, black_neighbor()?)?,
                        },
                        ResolvedStep::Edit {
                            edit: LeafEdit::new(RevisionId::new("leaf-1")?, second)?,
                        },
                        ResolvedStep::Edit {
                            edit: LeafEdit::new(
                                RevisionId::new("leaf-2")?,
                                Command::EditScoped {
                                    target: deadpan_core::ScopedNodeTarget {
                                        node: NodeId::new("hold")?,
                                        repeats: Vec::new(),
                                    },
                                    edit: deadpan_core::ScopedNodeEdit::Rename {
                                        label: "Edited pause".into(),
                                    },
                                    identities: deadpan_core::OccurrenceIdentities {
                                        nodes: Vec::new(),
                                        marks: Vec::new(),
                                    },
                                },
                            )?,
                        },
                    ],
                )?,
            },
        )?;
        let preview = store.preview_compound(&command)?;
        let (context_preview, _) = store.preview_generation_contexts(&command)?;
        assert_eq!(Some(context_preview), preview.edit);
        let outcome = store.commit_compound(&command, None)?.committed.unwrap();
        assert_eq!(Some(outcome.edit), preview.edit);
        assert_eq!(
            outcome.generation_preparations.is_empty(),
            restores_boundary
        );
        assert_eq!(
            matches!(hold_video(&store.snapshot()?), HoldVideo::Generated { .. }),
            restores_boundary
        );
        let db = Connection::open(package.join("project.sqlite"))?;
        let reservations: i64 = db.query_row(
            "SELECT count(*) FROM transaction_steps WHERE owner_revision='compound-boundary'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(reservations, 3);
        store.validate_full()?;
        store.undo(&command.new_revision, RevisionId::new("undo-compound")?)?;
        assert_eq!(hold_video(&store.snapshot()?), hold_video(&before));
        store.redo(
            &RevisionId::new("undo-compound")?,
            RevisionId::new("redo-compound")?,
        )?;
        store.validate_full()?;
        drop(store);
        ProjectStore::open(&package, AccessMode::ReadOnly)?.validate_full()?;
    }
    Ok(())
}

#[test]
fn caller_cannot_add_an_unneeded_provider_tail_to_a_valid_shortening() -> Result {
    let scratch = tempfile::tempdir()?;
    let mut store = accepted(&scratch.path().join("forged-tail.deadpan"))?;
    let before = store.snapshot()?;
    let HoldVideo::Generated { accepted } = hold_video(&before) else {
        panic!()
    };
    let forged = request(
        &store,
        "forged",
        Command::WithBoundaryReplacements {
            edit: BoundaryReplacementEdit::new(
                Command::SetHoldDuration {
                    node: NodeId::new("hold")?,
                    duration: FrameDuration::new(8)?,
                },
                vec![BoundaryReplacement {
                    target: deadpan_core::ScopedNodeTarget {
                        node: NodeId::new("hold")?,
                        repeats: Vec::new(),
                    },
                    accepted: accepted.clone(),
                }],
            )?,
        },
    )?;
    deadpan_core::apply(&before, &forged)?;
    assert!(store.preview(&forged).is_err());
    assert!(store.commit(&forged).is_err());
    assert_eq!(store.snapshot()?, before);
    assert!(store.generation_intents(None, 10)?.is_empty());
    store.validate_full()?;
    Ok(())
}
