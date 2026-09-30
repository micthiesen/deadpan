#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use deadpan_cli::host::Endpoint;
use deadpan_cli::live_project::{
    HistoryDirection, LiveError, Operation, Reply, Request, ShortOperation, dispatch_short,
    execute_short,
};
use deadpan_cli::render::RenderContext;
use deadpan_core::{
    BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate, HoldAudio,
    HoldRecipe, HoldVideo, NodeId, PresentationBasis, ProjectDocument, ProjectId, RevisionId,
    Subtree,
};
use deadpan_store::{AccessMode, ProjectStore};
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

fn document() -> Result<ProjectDocument> {
    Ok(ProjectDocument::new(
        ProjectId::new("live-project")?,
        RevisionId::new("initial")?,
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(30_000, 1_001)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root")?,
    )?)
}

fn insert(document: &ProjectDocument) -> Result<CommandRequest> {
    let node = NodeId::new("pause")?;
    Ok(CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new("inserted")?,
        command: Command::Insert {
            parent: document.root().clone(),
            index: 0,
            subtree: Subtree {
                overrides: Default::default(),
                gap_overrides: Default::default(),
                root: node.clone(),
                nodes: BTreeMap::from([(
                    node,
                    BeatNode::hold(
                        "Pause",
                        HoldRecipe {
                            picture_context: None,
                            duration: FrameDuration::new(45)?,
                            video: HoldVideo::Background,
                            audio: HoldAudio::Silence,
                        },
                    ),
                )]),
            },
        },
    })
}

fn edit(request: &CommandRequest, dry_run: bool) -> ShortOperation {
    ShortOperation::Edit {
        request: Box::new(request.clone()),
        dry_run,
    }
}

fn journal_state(path: &Path) -> Result<(i64, i64, String, Option<i64>)> {
    let connection = Connection::open_with_flags(
        path.join("project.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    Ok(connection.query_row(
        "SELECT (SELECT COUNT(*) FROM revisions), (SELECT COUNT(*) FROM history), head_revision, cursor FROM state WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?)
}

#[test]
fn request_schema_and_every_command_envelope_reject_unknown_fields() -> Result {
    let initial = document()?;
    let valid = serde_json::to_value(Request::new(Operation::Execute {
        project_id: initial.project_id().clone(),
        command: Box::new(edit(&insert(&initial)?, false)),
    }))?;
    let decoded = Request::from_value(valid.clone())?;
    assert_eq!(serde_json::to_value(decoded)?, valid);
    for pointer in [
        "",
        "/operation",
        "/operation/command",
        "/operation/command/request",
        "/operation/command/request/command",
    ] {
        let mut invalid = valid.clone();
        invalid
            .pointer_mut(pointer)
            .ok_or("missing fixture object")?["unexpected"] = json!(true);
        assert_eq!(
            Request::from_value(invalid).unwrap_err().code,
            "HostProtocolInvalid",
            "{pointer}"
        );
    }
    for invalid in [
        json!({"schema_version":1,"operation":{"operation":"inspect","unexpected":true}}),
        json!({"schema_version":1,"operation":{"operation":"execute","project_id":"live-project","command":{"command":"migrate","unexpected":true}}}),
        json!({"schema_version":1,"operation":{"operation":"execute","project_id":"live-project","command":{"command":"history","direction":"undo","expected_revision":"inserted","new_revision":"undone","dry_run":false,"unexpected":true}}}),
        json!({"schema_version":1,"operation":{"operation":"not_a_command"}}),
        json!({"schema_version":1,"operation":{"operation":"execute","project_id":"live-project","command":{"command":"history","direction":"backwards","expected_revision":"inserted","new_revision":"undone","dry_run":false}}}),
        json!({"schema_version":1,"operation":{"operation":"execute","project_id":"","command":{"command":"migrate"}}}),
        json!({"operation":{"operation":"inspect"}}),
    ] {
        assert_eq!(
            Request::from_value(invalid).unwrap_err().code,
            "HostProtocolInvalid"
        );
    }
    for version in [0, 2, u32::MAX] {
        assert_eq!(
            Request::from_value(
                json!({"schema_version":version,"operation":{"operation":"inspect"}})
            )
            .unwrap_err()
            .code,
            "HostProtocolUnsupported"
        );
    }
    for field in ["dry_run", "request"] {
        let mut invalid = valid.clone();
        invalid["operation"]["command"]
            .as_object_mut()
            .ok_or("missing command")?
            .remove(field);
        assert_eq!(
            Request::from_value(invalid).unwrap_err().code,
            "HostProtocolInvalid"
        );
    }
    Ok(())
}

#[test]
fn fieldless_variants_keep_their_wire_shape_and_reject_every_extra_field() -> Result {
    let inspect = json!({"operation":"inspect"});
    let migrate = json!({"command":"migrate"});
    let released = json!({"reply":"released"});
    assert!(matches!(
        serde_json::from_value::<Operation>(inspect.clone())?,
        Operation::Inspect
    ));
    assert!(matches!(
        serde_json::from_value::<ShortOperation>(migrate.clone())?,
        ShortOperation::Migrate
    ));
    assert!(matches!(
        serde_json::from_value::<Reply>(released.clone())?,
        Reply::Released
    ));
    assert_eq!(serde_json::to_value(Operation::Inspect)?, inspect);
    assert_eq!(serde_json::to_value(ShortOperation::Migrate)?, migrate);
    assert_eq!(serde_json::to_value(Reply::Released)?, released);

    for extra in [json!(true), json!({"nested":[]}), Value::Null] {
        let mut invalid_inspect = inspect.clone();
        invalid_inspect["unexpected"] = extra.clone();
        let mut invalid_migrate = migrate.clone();
        invalid_migrate["unexpected"] = extra.clone();
        let mut invalid_released = released.clone();
        invalid_released["unexpected"] = extra;
        assert!(serde_json::from_value::<Operation>(invalid_inspect.clone()).is_err());
        assert!(serde_json::from_value::<ShortOperation>(invalid_migrate.clone()).is_err());
        assert!(serde_json::from_value::<Reply>(invalid_released).is_err());
        for operation in [
            invalid_inspect,
            json!({"operation":"execute","project_id":"live-project","command":invalid_migrate}),
        ] {
            assert_eq!(
                Request::from_value(json!({"schema_version":1,"operation":operation}))
                    .unwrap_err()
                    .code,
                "HostProtocolInvalid"
            );
        }
    }
    Ok(())
}

#[test]
fn live_render_envelope_preserves_public_render_schema_and_size_checks() -> Result {
    let mut wire = json!({
        "schema_version":1,
        "operation":{"operation":"render","request":{
            "schema_version":1,"request_id":"render-request",
            "context":{"project_id":"live-project","revision_id":"initial"},
            "operation":{"operation":"start","destination":"/tmp/deadpan-live-test.mp4"}
        }}
    });
    Request::from_value(wire.clone())?;
    wire["operation"]["request"]["schema_version"] = json!(2);
    assert_eq!(
        Request::from_value(wire.clone()).unwrap_err().code,
        "RenderProtocolUnsupported"
    );
    wire["operation"]["request"]["schema_version"] = json!(1);
    wire["operation"]["request"]["operation"]["destination"] =
        json!("x".repeat(deadpan_cli::render::MAX_REQUEST_BYTES + 1));
    assert_eq!(
        Request::from_value(wire).unwrap_err().code,
        "RenderInvalidRequest"
    );
    Ok(())
}

#[test]
fn shared_executor_matches_direct_store_preview_commit_undo_and_redo() -> Result {
    let scratch = tempfile::tempdir()?;
    let initial = document()?;
    let actual_path = scratch.path().join("actual.deadpan");
    let mut actual = ProjectStore::create(&actual_path, &initial)?;
    let mut direct = ProjectStore::create(&scratch.path().join("direct.deadpan"), &initial)?;
    let request = insert(&initial)?;
    let before = journal_state(&actual_path)?;

    let (preview, receipt) =
        execute_short(&mut actual, initial.project_id(), &edit(&request, true))?;
    assert_eq!(
        preview,
        json!({"protocol":1,"committed":false,"edit":direct.preview(&request)?})
    );
    assert!(receipt.is_none());
    assert_eq!(actual.snapshot()?, initial);
    assert_eq!(journal_state(&actual_path)?, before);

    let expected = direct.commit(&request)?;
    let (committed, receipt) =
        execute_short(&mut actual, initial.project_id(), &edit(&request, false))?;
    assert_eq!(
        committed,
        json!({"protocol":1,"committed":true,"outcome":expected})
    );
    assert_eq!(receipt.as_ref(), Some(&request.new_revision));
    assert_eq!(actual.snapshot()?, direct.snapshot()?);

    for (direction, next) in [
        (HistoryDirection::Undo, "undone"),
        (HistoryDirection::Redo, "redone"),
    ] {
        let current = actual.snapshot()?;
        let before = journal_state(&actual_path)?;
        let next = RevisionId::new(next)?;
        let operation = |dry_run| ShortOperation::History {
            direction,
            expected_revision: current.revision_id().clone(),
            new_revision: next.clone(),
            dry_run,
        };
        let expected = match direction {
            HistoryDirection::Undo => direct.preview_undo(current.revision_id(), next.clone())?,
            HistoryDirection::Redo => direct.preview_redo(current.revision_id(), next.clone())?,
        };
        let (preview, receipt) =
            execute_short(&mut actual, initial.project_id(), &operation(true))?;
        assert_eq!(
            preview,
            json!({"protocol":1,"committed":false,"outcome":expected})
        );
        assert!(receipt.is_none());
        assert_eq!(actual.snapshot()?, current);
        assert_eq!(journal_state(&actual_path)?, before);
        let expected = match direction {
            HistoryDirection::Undo => direct.undo(current.revision_id(), next.clone())?,
            HistoryDirection::Redo => direct.redo(current.revision_id(), next.clone())?,
        };
        let (committed, receipt) =
            execute_short(&mut actual, initial.project_id(), &operation(false))?;
        assert_eq!(
            committed,
            json!({"protocol":1,"committed":true,"outcome":expected})
        );
        assert_eq!(receipt, Some(next));
        assert_eq!(actual.snapshot()?, direct.snapshot()?);
    }
    assert_eq!(actual.snapshot()?.duration()?.frames(), 45);
    Ok(())
}

#[test]
fn invalid_identity_revision_and_target_preserve_the_document_and_journal() -> Result {
    let scratch = tempfile::tempdir()?;
    let path = scratch.path().join("invalid.deadpan");
    let initial = document()?;
    let mut store = ProjectStore::create(&path, &initial)?;
    let request = insert(&initial)?;
    let other_project = ProjectId::new("different-project")?;
    let mut inner_mismatch = request.clone();
    inner_mismatch.project_id = other_project.clone();
    let mut stale = request.clone();
    stale.expected_revision = RevisionId::new("stale")?;
    let mut invalid_target = request.clone();
    if let Command::Insert { parent, .. } = &mut invalid_target.command {
        *parent = NodeId::new("missing-parent")?;
    }
    let before = journal_state(&path)?;
    for dry_run in [false, true] {
        for (project, operation, code) in [
            (
                &other_project,
                edit(&request, dry_run),
                "HostProjectChanged",
            ),
            (
                initial.project_id(),
                edit(&inner_mismatch, dry_run),
                "HostProjectChanged",
            ),
            (
                initial.project_id(),
                edit(&stale, dry_run),
                "RevisionConflict",
            ),
        ] {
            let error = execute_short(&mut store, project, &operation).unwrap_err();
            assert_eq!(error.code, code);
            assert_eq!(
                error.current_revision.as_ref(),
                (code == "RevisionConflict").then(|| initial.revision_id())
            );
            assert!(error.committed_revision.is_none());
            assert_eq!(store.snapshot()?, initial);
            assert_eq!(journal_state(&path)?, before);
        }
        assert!(
            execute_short(
                &mut store,
                initial.project_id(),
                &edit(&invalid_target, dry_run)
            )
            .is_err()
        );
        assert_eq!(store.snapshot()?, initial);
        assert_eq!(journal_state(&path)?, before);
    }
    // Rejected requests did not reserve their next revision or alter undo state.
    execute_short(&mut store, initial.project_id(), &edit(&request, false))?;
    let current = store.snapshot()?;
    let after = journal_state(&path)?;
    for direction in [HistoryDirection::Undo, HistoryDirection::Redo] {
        let error = execute_short(
            &mut store,
            initial.project_id(),
            &ShortOperation::History {
                direction,
                expected_revision: initial.revision_id().clone(),
                new_revision: RevisionId::new("invalid-history")?,
                dry_run: false,
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "RevisionConflict");
        assert_eq!(error.current_revision.as_ref(), Some(current.revision_id()));
        assert_eq!(store.snapshot()?, current);
        assert_eq!(journal_state(&path)?, after);
    }
    Ok(())
}

#[test]
fn closed_dispatch_matches_shared_execution_and_preview_keeps_writer_lock() -> Result {
    let scratch = tempfile::tempdir()?;
    let initial = document()?;
    let path = scratch.path().join("dispatch.deadpan");
    drop(ProjectStore::create(&path, &initial)?);
    let mut direct = ProjectStore::create(&scratch.path().join("direct.deadpan"), &initial)?;
    let request = insert(&initial)?;
    for dry_run in [true, false] {
        let expected =
            execute_short(&mut direct, initial.project_id(), &edit(&request, dry_run))?.0;
        assert_eq!(
            dispatch_short(&path, None, edit(&request, dry_run))?,
            expected
        );
        assert_eq!(
            ProjectStore::open(&path, AccessMode::ReadOnly)?.snapshot()?,
            direct.snapshot()?
        );
    }
    for (direction, revision) in [
        (HistoryDirection::Undo, "undo"),
        (HistoryDirection::Redo, "redo"),
    ] {
        let current = direct.snapshot()?;
        for dry_run in [true, false] {
            let operation = ShortOperation::History {
                direction,
                expected_revision: current.revision_id().clone(),
                new_revision: RevisionId::new(revision)?,
                dry_run,
            };
            let expected = execute_short(&mut direct, initial.project_id(), &operation)?.0;
            assert_eq!(
                dispatch_short(&path, Some(initial.project_id().clone()), operation)?,
                expected
            );
            assert_eq!(
                ProjectStore::open(&path, AccessMode::ReadOnly)?.snapshot()?,
                direct.snapshot()?
            );
        }
    }
    // Read-only previews can coexist with a writer that has not advertised a host.
    let writer = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let snapshot = writer.snapshot()?;
    let before = journal_state(&path)?;
    let operation = ShortOperation::History {
        direction: HistoryDirection::Undo,
        expected_revision: snapshot.revision_id().clone(),
        new_revision: RevisionId::new("preview-only")?,
        dry_run: true,
    };
    assert_eq!(dispatch_short(&path, None, operation)?["committed"], false);
    assert_eq!(writer.snapshot()?, snapshot);
    assert_eq!(journal_state(&path)?, before);
    let mut request = request;
    request.expected_revision = snapshot.revision_id().clone();
    request.new_revision = RevisionId::new("must-not-fallback")?;
    assert_eq!(
        dispatch_short(&path, None, edit(&request, false))
            .unwrap_err()
            .code,
        "HostOwnerUnavailable"
    );
    assert_eq!(journal_state(&path)?, before);
    Ok(())
}

#[derive(Default)]
struct Received {
    inspect: usize,
    execute: usize,
}

/// A service-thread fixture: real authenticated transport and the shared
/// executor, with no native UI, media worker, or separate writer connection.
fn remote_dispatch(
    store: &mut ProjectStore,
    endpoint: &mut Endpoint,
    package: &Path,
    project: Option<ProjectId>,
    operation: ShortOperation,
    refresh_error: Option<&str>,
) -> Result<(std::result::Result<Value, LiveError>, Received)> {
    thread::scope(|scope| {
        let client = scope.spawn(move || dispatch_short(package, project, operation));
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut received = Received::default();
        while !client.is_finished() {
            if Instant::now() >= deadline {
                return Err("live-project fixture exceeded its deadline".into());
            }
            for incoming in endpoint.poll() {
                store.check_writer_owner(endpoint.owner_handle())?;
                let reply = match Request::from_value(incoming.payload) {
                    Ok(Request {
                        operation: Operation::Inspect,
                        ..
                    }) => {
                        received.inspect += 1;
                        Reply::Context {
                            context: RenderContext::from_document(&store.snapshot()?),
                            preview_active: false,
                        }
                    }
                    Ok(Request {
                        operation:
                            Operation::Execute {
                                project_id,
                                command,
                            },
                        ..
                    }) => {
                        received.execute += 1;
                        match execute_short(store, &project_id, &command) {
                            Ok((output, committed_revision)) => Reply::Completed {
                                output,
                                committed_revision,
                                refresh_error: refresh_error.map(str::to_owned),
                            },
                            Err(error) => Reply::Failed { error },
                        }
                    }
                    Ok(_) => Reply::Failed {
                        error: LiveError::new(
                            "UnexpectedTestOperation",
                            "Fixture accepts short commands only",
                        ),
                    },
                    Err(error) => Reply::Failed { error },
                };
                endpoint.respond(incoming.ticket, serde_json::to_value(reply)?)?;
            }
            thread::park_timeout(Duration::from_millis(1));
        }
        Ok((
            client.join().map_err(|_| "live-project client panicked")?,
            received,
        ))
    })
}

#[test]
fn remote_dispatch_uses_the_same_commit_history_and_error_receipts() -> Result {
    let scratch = tempfile::tempdir()?;
    let initial = document()?;
    let path = scratch.path().join("remote.deadpan");
    let mut owner = ProjectStore::create(&path, &initial)?;
    let mut direct = ProjectStore::create(&scratch.path().join("direct.deadpan"), &initial)?;
    let mut endpoint = Endpoint::bind(&mut owner)?;
    let request = insert(&initial)?;
    let expected = execute_short(&mut direct, initial.project_id(), &edit(&request, false))?.0;
    let (actual, received) = remote_dispatch(
        &mut owner,
        &mut endpoint,
        &path,
        Some(initial.project_id().clone()),
        edit(&request, false),
        None,
    )?;
    assert_eq!(actual?, expected);
    assert_eq!((received.inspect, received.execute), (0, 1));
    assert_eq!(owner.snapshot()?, direct.snapshot()?);
    let before = journal_state(&path)?;
    let (actual, received) = remote_dispatch(
        &mut owner,
        &mut endpoint,
        &path,
        Some(initial.project_id().clone()),
        edit(&request, false),
        None,
    )?;
    let error = actual.unwrap_err();
    assert_eq!(error.code, "RevisionConflict");
    assert_eq!(error.current_revision, Some(request.new_revision.clone()));
    assert_eq!(received.execute, 1);
    assert_eq!(journal_state(&path)?, before);

    for (direction, revision) in [
        (HistoryDirection::Undo, "remote-undo"),
        (HistoryDirection::Redo, "remote-redo"),
    ] {
        let operation = ShortOperation::History {
            direction,
            expected_revision: direct.snapshot()?.revision_id().clone(),
            new_revision: RevisionId::new(revision)?,
            dry_run: false,
        };
        let expected = execute_short(&mut direct, initial.project_id(), &operation)?.0;
        let (actual, received) =
            remote_dispatch(&mut owner, &mut endpoint, &path, None, operation, None)?;
        assert_eq!(actual?, expected);
        assert_eq!((received.inspect, received.execute), (1, 1));
        assert_eq!(owner.snapshot()?, direct.snapshot()?);
    }
    drop(endpoint);
    drop(owner);
    let reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    assert_eq!(reopened.snapshot()?, direct.snapshot()?);
    Ok(())
}

#[test]
fn a_remote_commit_survives_refresh_failure_with_its_exact_revision_receipt() -> Result {
    let scratch = tempfile::tempdir()?;
    let initial = document()?;
    let path = scratch.path().join("refresh.deadpan");
    let mut owner = ProjectStore::create(&path, &initial)?;
    let mut endpoint = Endpoint::bind(&mut owner)?;
    let request = insert(&initial)?;
    let (actual, received) = remote_dispatch(
        &mut owner,
        &mut endpoint,
        &path,
        Some(initial.project_id().clone()),
        edit(&request, false),
        Some("workspace refresh failed"),
    )?;
    let actual = actual?;
    assert_eq!(actual["committed"], true);
    assert_eq!(actual["committed_revision"], json!(request.new_revision));
    assert_eq!(
        actual["outcome"]["revision_id"],
        actual["committed_revision"]
    );
    assert_eq!(actual["host_refresh_error"], "workspace refresh failed");
    assert_eq!(received.execute, 1);
    assert_eq!(owner.snapshot()?.revision_id(), &request.new_revision);
    assert_eq!(journal_state(&path)?.0, 2);
    Ok(())
}

#[test]
fn migration_of_a_live_current_schema_preserves_the_writer_and_history() -> Result {
    let scratch = tempfile::tempdir()?;
    let initial = document()?;
    let path = scratch.path().join("migration.deadpan");
    let mut owner = ProjectStore::create(&path, &initial)?;
    let mut endpoint = Endpoint::bind(&mut owner)?;
    let before = journal_state(&path)?;
    let (actual, received) = remote_dispatch(
        &mut owner,
        &mut endpoint,
        &path,
        None,
        ShortOperation::Migrate,
        None,
    )?;
    let schema = deadpan_store::DATABASE_SCHEMA_VERSION;
    assert_eq!(
        actual?,
        json!({"protocol":1,"migration":{"from_schema":schema,"to_schema":schema,"backup":null}})
    );
    assert_eq!((received.inspect, received.execute), (1, 1));
    assert_eq!(journal_state(&path)?, before);
    assert_eq!(owner.snapshot()?, initial);
    owner.check_writer_owner(endpoint.owner_handle())?;
    assert!(matches!(
        ProjectStore::open(&path, AccessMode::ReadWrite),
        Err(deadpan_store::StoreError::AlreadyOpen)
    ));
    Ok(())
}
