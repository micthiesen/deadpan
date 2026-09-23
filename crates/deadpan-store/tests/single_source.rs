#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::{
    error::Error,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};

use deadpan_core::{
    AssetId, Command, CommandRequest, NodeId, NodeKind, ProjectDocument, ProjectId, RevisionId,
};
use deadpan_media::{
    audio_session::{AudioSession, AudioSessionLimits},
    source_index::SourceContentIdentity,
    source_input::VerifiedSourceInput,
    source_qualification::DecodedSourceQualification,
    source_session::{SourceSession, SourceSessionLimits},
};
use deadpan_store::{
    AccessMode, ProjectStore, StoreError,
    original_media::{OriginalMediaLimits, OriginalOwnership},
    single_source::{SingleSourceInitialization, SingleSourceState},
    source_registration::{PreparedSourceRegistration, SourceInsertionRequest, SourceRegistration},
};
use rusqlite::Connection;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).unwrap()
}
fn node(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn asset(value: &str) -> AssetId {
    AssetId::new(value).unwrap()
}
fn active() -> AtomicBool {
    AtomicBool::new(false)
}
fn limits() -> OriginalMediaLimits {
    OriginalMediaLimits::new(2_000_000, Duration::from_secs(10)).unwrap()
}
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures")
        .join(name)
        .canonicalize()
        .unwrap()
}
fn create(parent: &Path) -> Result<(PathBuf, ProjectStore)> {
    let path = parent.join("focused.deadpan");
    let doc = ProjectDocument::new_automatic(
        ProjectId::new("focused")?,
        revision("initial"),
        node("root"),
    )?;
    Ok((
        path.clone(),
        ProjectStore::create_single_source(&path, &doc)?,
    ))
}
fn prepare(
    store: &mut ProjectStore,
    name: &str,
    picture: bool,
) -> Result<PreparedSourceRegistration> {
    let record = store
        .retain_original(
            &fixture(name),
            OriginalOwnership::Managed,
            limits(),
            &active(),
        )?
        .record;
    let mut original =
        store
            .original_import_handle()?
            .snapshot_original(&record, limits(), &active())?;
    let input = VerifiedSourceInput::copy_verified(
        &mut original,
        SourceContentIdentity::new(record.sha256(), record.object().byte_length())?,
        2_000_000,
        Duration::from_secs(10),
        &active(),
    )?;
    let video = picture
        .then(|| {
            SourceSession::open_input(
                input.clone(),
                asset("decode"),
                SourceSessionLimits::default(),
                &active(),
            )
        })
        .transpose()?;
    let audio = AudioSession::open_input(input, 1, AudioSessionLimits::default(), &active())?;
    let decoded = DecodedSourceQualification::from_sessions(video.as_ref(), Some(&audio))?;
    Ok(PreparedSourceRegistration::from_decoded(
        original,
        &decoded,
        &active(),
    )?)
}
fn initialization() -> SingleSourceInitialization {
    SingleSourceInitialization {
        expected_revision: revision("initial"),
        new_revision: revision("baseline"),
        new_asset_id: asset("original"),
        node: node("full-original"),
        label: "The original".into(),
    }
}
fn register(
    store: &ProjectStore,
    source: &PreparedSourceRegistration,
    next: &str,
) -> SourceRegistration {
    SourceRegistration {
        expected_revision: store.snapshot().unwrap().revision_id().clone(),
        new_revision: revision(next),
        original: source.receipt().original().content().clone(),
        new_asset_id: asset(next),
        label: next.into(),
        insertion: None,
    }
}
fn command(store: &ProjectStore, next: &str, command: Command) -> CommandRequest {
    let current = store.snapshot().unwrap();
    CommandRequest {
        project_id: current.project_id().clone(),
        expected_revision: current.revision_id().clone(),
        new_revision: revision(next),
        command,
    }
}
fn counts(path: &Path) -> Result<(i64, i64, i64)> {
    Ok(Connection::open(path.join("project.sqlite"))?.query_row("SELECT (SELECT count(*) FROM revisions),(SELECT count(*) FROM history),(SELECT count(*) FROM source_qualifications)", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)))?)
}

#[test]
fn full_original_is_atomic_and_undo_floor_survives_branch_and_reopen() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = create(scratch.path())?;
    assert_eq!(
        store.single_source_state()?,
        Some(SingleSourceState::AwaitingSource {
            initial_revision: revision("initial")
        })
    );
    let source = prepare(&mut store, "offset-bframes.mp4", true)?;
    let expected = source
        .receipt()
        .snapshot()
        .derive_timing(
            source
                .receipt()
                .snapshot()
                .basis_candidate()?
                .unwrap()
                .basis
                .frame_rate,
        )?
        .source_node(asset("original"));
    let outcome = store.initialize_prepared_source(&initialization(), &source, &active())?;
    assert!(outcome.commit.is_some());
    assert_eq!(counts(&path)?, (2, 1, 1));
    let baseline = store.snapshot()?;
    assert_eq!(baseline.nodes().len(), 2);
    assert!(
        matches!(&baseline.nodes()[&node("full-original")].kind, NodeKind::Source { source } if *source == expected)
    );
    assert_eq!(store.history_availability()?, (false, false));
    assert!(matches!(
        store.preview_undo(baseline.revision_id(), revision("bad-preview")),
        Err(StoreError::NothingToUndo)
    ));
    assert!(matches!(
        store.undo(baseline.revision_id(), revision("bad-undo")),
        Err(StoreError::NothingToUndo)
    ));
    let delete = command(
        &store,
        "delete",
        Command::Delete {
            node: node("full-original"),
        },
    );
    store.commit(&delete)?;
    assert_eq!(store.snapshot()?.duration()?.frames(), 0);
    store.undo(&revision("delete"), revision("undo"))?;
    assert_eq!(store.history_availability()?, (false, true));
    assert!(matches!(
        store.preview_undo(&revision("undo"), revision("bad-preview2")),
        Err(StoreError::NothingToUndo)
    ));
    store.redo(&revision("undo"), revision("redo"))?;
    store.undo(&revision("redo"), revision("undo2"))?;
    store.commit(&command(
        &store,
        "branch",
        Command::Rename {
            node: node("full-original"),
            label: "Massaged".into(),
        },
    ))?;
    assert_eq!(store.history_availability()?, (true, false));
    store.undo(&revision("branch"), revision("undo-branch"))?;
    store.validate()?;
    let state = store.single_source_state()?;
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(store.single_source_state()?, state);
    assert_eq!(store.history_availability()?, (false, true));
    assert!(matches!(
        store.undo(&revision("undo-branch"), revision("forbidden")),
        Err(StoreError::NothingToUndo)
    ));
    assert_eq!(store.snapshot()?.nodes(), baseline.nodes());
    Ok(())
}

#[test]
fn second_picture_is_rejected_after_delete_but_original_reuse_and_sounds_work() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = create(scratch.path())?;
    let original = prepare(&mut store, "offset-bframes.mp4", true)?;
    store.initialize_prepared_source(&initialization(), &original, &active())?;
    store.commit(&command(
        &store,
        "delete",
        Command::Delete {
            node: node("full-original"),
        },
    ))?;
    let other = prepare(&mut store, "cfr-bframes.mp4", true)?;
    let input = register(&store, &other, "another-video");
    let before = counts(&path)?;
    assert!(matches!(
        store.preview_prepared_source_registration(&input, &other, &active()),
        Err(StoreError::SingleSource(_))
    ));
    assert!(matches!(
        store.register_prepared_source(&input, &other, None, &active()),
        Err(StoreError::SingleSource(_))
    ));
    assert_eq!(counts(&path)?, before);
    let mut forged = store.snapshot()?.assets()[&asset("original")].clone();
    forged.source_qualification = None;
    assert!(matches!(
        store.commit(&command(
            &store,
            "unqualified-video",
            Command::AddAsset {
                id: asset("bypass"),
                asset: forged
            }
        )),
        Err(StoreError::SingleSource(_))
    ));
    let mut reuse = register(&store, &original, "reuse");
    reuse.insertion = Some(SourceInsertionRequest {
        parent: node("root"),
        index: 0,
        node: node("again"),
        label: "Again".into(),
        purpose: Default::default(),
    });
    let result = store.register_prepared_source(&reuse, &original, None, &active())?;
    assert_eq!(result.asset_id, asset("original"));
    let sound = prepare(&mut store, "cfr-bframes.mp4", false)?;
    let result = store.register_prepared_source(
        &register(&store, &sound, "sound"),
        &sound,
        None,
        &active(),
    )?;
    let document = store.snapshot()?;
    assert!(document.assets()[&result.asset_id].video.is_none());
    assert!(document.assets()[&result.asset_id].audio.is_some());
    store.validate()?;
    drop(store);
    ProjectStore::open(&path, AccessMode::ReadOnly)?.validate()?;
    Ok(())
}

#[test]
fn initialization_rejects_generic_edits_audio_stale_cancelled_and_closed_preparation() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = create(scratch.path())?;
    let original = prepare(&mut store, "offset-bframes.mp4", true)?;
    let request = command(
        &store,
        "too-early",
        Command::Rename {
            node: node("root"),
            label: "No".into(),
        },
    );
    assert!(matches!(
        store.commit(&request),
        Err(StoreError::SingleSource(_))
    ));
    assert!(matches!(
        store.register_prepared_source(
            &register(&store, &original, "generic"),
            &original,
            None,
            &active()
        ),
        Err(StoreError::SingleSource(_))
    ));
    let sound = prepare(&mut store, "offset-bframes.mp4", false)?;
    assert!(matches!(
        store.initialize_prepared_source(&initialization(), &sound, &active()),
        Err(StoreError::SingleSource(_))
    ));
    let mut stale = initialization();
    stale.expected_revision = revision("wrong");
    assert!(matches!(
        store.initialize_prepared_source(&stale, &original, &active()),
        Err(StoreError::RevisionConflict { .. })
    ));
    assert!(
        store
            .initialize_prepared_source(&initialization(), &original, &AtomicBool::new(true))
            .is_err()
    );
    assert_eq!(counts(&path)?, (1, 0, 0));
    drop(store);
    let mut store = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert!(
        store
            .initialize_prepared_source(&initialization(), &original, &active())
            .is_err()
    );
    assert_eq!(counts(&path)?, (1, 0, 0));
    let fresh = prepare(&mut store, "offset-bframes.mp4", true)?;
    store.initialize_prepared_source(&initialization(), &fresh, &active())?;
    let before = counts(&path)?;
    let mut again = initialization();
    again.expected_revision = revision("baseline");
    again.new_revision = revision("replace");
    assert!(matches!(
        store.initialize_prepared_source(&again, &fresh, &active()),
        Err(StoreError::SingleSource(_))
    ));
    assert_eq!(counts(&path)?, before);
    Ok(())
}

#[test]
fn malformed_or_displaced_profile_is_rejected_on_reopen() -> Result {
    for sql in [
        "DELETE FROM single_source",
        "UPDATE single_source SET profile=json_set(profile,'$.unexpected',true)",
        "UPDATE single_source SET profile=json_set(profile,'$.asset','missing')",
        "UPDATE single_source SET profile=json_set(profile,'$.node','root')",
        "UPDATE single_source SET profile=json_set(profile,'$.baseline_revision','later'),baseline_history=2",
        "UPDATE single_source SET profile=json_object('state','awaiting_source','initial_revision','initial'),baseline_history=NULL",
        "UPDATE state SET workflow='generic'",
    ] {
        let scratch = tempfile::tempdir()?;
        let (path, mut store) = create(scratch.path())?;
        let original = prepare(&mut store, "offset-bframes.mp4", true)?;
        store.initialize_prepared_source(&initialization(), &original, &active())?;
        store.commit(&command(
            &store,
            "later",
            Command::Rename {
                node: node("full-original"),
                label: "Later".into(),
            },
        ))?;
        drop(store);
        Connection::open(path.join("project.sqlite"))?.execute_batch(sql)?;
        assert!(
            ProjectStore::open(&path, AccessMode::ReadOnly).is_err(),
            "{sql}"
        );
    }
    Ok(())
}

#[test]
fn generic_projects_keep_source_import_undo_and_creation_collisions_are_specific() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("generic.deadpan");
    let doc = ProjectDocument::new_automatic(
        ProjectId::new("generic")?,
        revision("initial"),
        node("root"),
    )?;
    let mut store = ProjectStore::create(&path, &doc)?;
    assert_eq!(store.single_source_state()?, None);
    assert!(
        matches!(ProjectStore::create_single_source(&path, &doc), Err(StoreError::PackageAlreadyExists(existing)) if existing == path)
    );
    let original = prepare(&mut store, "offset-bframes.mp4", true)?;
    let result = store.register_prepared_source(
        &register(&store, &original, "first"),
        &original,
        None,
        &active(),
    )?;
    store.undo(&result.commit.unwrap().revision_id, revision("back"))?;
    assert_eq!(store.snapshot()?.assets().len(), 0);
    assert_eq!(store.history_availability()?, (false, true));
    let other = prepare(&mut store, "cfr-bframes.mp4", true)?;
    store.register_prepared_source(
        &register(&store, &other, "replacement"),
        &other,
        None,
        &active(),
    )?;
    store.validate()?;
    Ok(())
}

#[test]
fn failing_baseline_publication_rolls_back_source_history_and_can_retry() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = create(scratch.path())?;
    let original = prepare(&mut store, "offset-bframes.mp4", true)?;
    let before = store.snapshot()?;
    let database = Connection::open(path.join("project.sqlite"))?;
    database.execute_batch("CREATE TRIGGER fail_baseline BEFORE UPDATE ON single_source BEGIN SELECT RAISE(ABORT,'baseline publication failure'); END;")?;
    assert!(matches!(
        store.initialize_prepared_source(&initialization(), &original, &active()),
        Err(StoreError::Database(_))
    ));
    assert_eq!(counts(&path)?, (1, 0, 0));
    assert_eq!(store.snapshot()?, before);
    assert!(matches!(
        store.single_source_state()?,
        Some(SingleSourceState::AwaitingSource { .. })
    ));
    database.execute_batch("DROP TRIGGER fail_baseline")?;
    store.initialize_prepared_source(&initialization(), &original, &active())?;
    assert_eq!(counts(&path)?, (2, 1, 1));
    assert_eq!(store.history_availability()?, (false, false));
    Ok(())
}

#[test]
fn changed_original_cannot_initialize_from_a_retained_prepared_snapshot() -> Result {
    let scratch = tempfile::tempdir()?;
    let (path, mut store) = create(scratch.path())?;
    let original = prepare(&mut store, "offset-bframes.mp4", true)?;
    let digest = original.receipt().original().content().digest();
    let object = path
        .join("Media/Originals")
        .join(format!("blake3-{digest}"));
    std::fs::remove_file(&object)?;
    assert!(
        store
            .initialize_prepared_source(&initialization(), &original, &active())
            .is_err()
    );
    assert_eq!(counts(&path)?, (1, 0, 0));
    assert!(matches!(
        store.single_source_state()?,
        Some(SingleSourceState::AwaitingSource { .. })
    ));
    Ok(())
}

#[test]
fn a_valid_generic_reimport_branch_cannot_be_forged_into_single_source_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("generic-branches.deadpan");
    let doc = ProjectDocument::new_automatic(
        ProjectId::new("generic")?,
        revision("initial"),
        node("root"),
    )?;
    let mut store = ProjectStore::create(&path, &doc)?;
    let original = prepare(&mut store, "offset-bframes.mp4", true)?;
    let mut first = register(&store, &original, "baseline");
    first.new_asset_id = asset("original");
    first.insertion = Some(SourceInsertionRequest {
        parent: node("root"),
        index: 0,
        node: node("full-original"),
        label: "Original".into(),
        purpose: Default::default(),
    });
    store.register_prepared_source(&first, &original, None, &active())?;
    store.undo(&revision("baseline"), revision("below-original"))?;
    let second = SourceRegistration {
        expected_revision: revision("below-original"),
        new_revision: revision("other-root"),
        ..first
    };
    store.register_prepared_source(&second, &original, None, &active())?;
    // This is a fully valid generic branch with the same original identity in
    // its current document, not a broken cursor or fabricated edit patch.
    store.validate()?;
    drop(store);
    let database = Connection::open(path.join("project.sqlite"))?;
    let forged = SingleSourceState::Ready {
        initial_revision: revision("initial"),
        asset: asset("original"),
        qualification: original.receipt().id().clone(),
        node: node("full-original"),
        baseline_revision: revision("baseline"),
    };
    database.execute("UPDATE state SET workflow='single_source_v1'", [])?;
    database.execute(
        "INSERT INTO single_source(singleton,profile,baseline_history) VALUES(1,?1,1)",
        [serde_json::to_string(&forged)?],
    )?;
    assert!(
        matches!(ProjectStore::open(&path, AccessMode::ReadOnly), Err(StoreError::SingleSource(message)) if message.contains("branch below"))
    );
    Ok(())
}
