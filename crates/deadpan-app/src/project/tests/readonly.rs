//! A readable newer package answers every pending channel without mutation.

use super::*;
use crate::project::{
    gain, generation, macros, marks, registers, semantic, slice, slip, splice, targets, trim,
};
use deadpan_core::{
    AudioTreatments, ClipGain, GainDb, SemanticInstruction, SemanticProgram, SliceCaptureSelection,
    SourceTrimControl,
};

fn package_bytes(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, path: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, files);
            } else if path.file_name().unwrap() != "project.sqlite-shm" {
                // SQLite read locks change SHM. DB, WAL, registers, managed
                // media and every other retained package byte must not change.
                files.insert(
                    path.strip_prefix(root).unwrap().into(),
                    std::fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(path, path, &mut files);
    files
}

fn assert_package_unchanged(path: &Path, before: &BTreeMap<PathBuf, Vec<u8>>) {
    let after = package_bytes(path);
    let differences: std::collections::BTreeSet<_> = before
        .keys()
        .chain(after.keys())
        .filter(|path| before.get(*path) != after.get(*path))
        .collect();
    let identity = |bytes: Option<&Vec<u8>>| {
        bytes.map_or_else(
            || "missing".to_owned(),
            |bytes| format!("{} bytes, BLAKE3 {}", bytes.len(), blake3::hash(bytes)),
        )
    };
    let details = differences
        .iter()
        .take(12)
        .map(|path| {
            format!(
                "{}: before [{}], after [{}]",
                path.display(),
                identity(before.get(*path)),
                identity(after.get(*path))
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        differences.is_empty(),
        "{} package paths changed (showing at most 12):\n{details}",
        differences.len()
    );
}

fn mark(workspace: &Workspace, ticket: u64, operation: marks::Operation) -> marks::Request {
    marks::Request {
        id: marks::Id {
            ticket,
            session: workspace.session,
            project: workspace.document.project_id().clone(),
            revision: workspace.document.revision_id().clone(),
        },
        operation,
    }
}

fn future_fixture(documents: &Path) -> (Harness, Arc<Workspace>, NodeId) {
    let harness = Harness::with_library(Some(
        ProjectLibrary::from_documents(documents.into()).unwrap(),
    ));
    command(
        &harness.service,
        ProjectRequest::CreateFromSource {
            path: fixture("cfr-bframes.mp4"),
        },
    );
    harness.finish(harness.job());
    harness.finish(harness.job());
    let initial = complete(&harness.service);
    let source = initial
        .sources
        .values()
        .find(|source| source.video_index.is_some())
        .unwrap();
    let pasted = command(
        &harness.service,
        ProjectRequest::PasteMoment(MomentPaste {
            expected_session: initial.session,
            expected_revision: initial.document.revision_id().clone(),
            asset: source.asset.clone(),
            qualification: source.receipt.id().clone(),
            ordinals: 10..24,
            scope: SequenceScope::default(),
            parent: initial.document.root().clone(),
            destination: splice::Destination::Slot(0),
        }),
    );
    assert!(pasted.error.is_none(), "{:?}", pasted.error);
    let selected = pasted.committed.unwrap().selected_node.unwrap();
    let workspace = pasted.workspace.unwrap();
    let saved = command(
        &harness.service,
        ProjectRequest::Marks(mark(
            &workspace,
            1,
            marks::Operation::Set {
                letter: 'a',
                location: marks::Location::Edit {
                    scope: SequenceScope::default(),
                    at: ProjectFrame(3),
                    selected: Some(selected.clone()),
                },
            },
        )),
    );
    assert!(saved.marks.reply.unwrap().result.is_ok());
    let workspace = saved.workspace.unwrap();
    let source = workspace
        .sources
        .values()
        .find(|source| source.video_index.is_some())
        .unwrap();
    let copied = command(
        &harness.service,
        ProjectRequest::CaptureOriginal(registers::OriginalRequest {
            id: copy_id(&workspace, 1),
            register: None,
            asset: source.asset.clone(),
            qualification: source.receipt.id().clone(),
            ordinals: 10..24,
        }),
    );
    assert!(copied.captured_original.unwrap().result.is_ok());
    let path = workspace.path.clone();
    let closed = command(&harness.service, ProjectRequest::Close);
    assert!(closed.workspace.is_none());
    if closed.backups.owned_workers_active_for_check {
        wait(&harness.service, |update| {
            update.workspace.is_none()
                && !harness.service.is_busy()
                && !update.backups.owned_workers_active_for_check
        });
    }
    {
        let connection = rusqlite::Connection::open(path.join("project.sqlite")).unwrap();
        connection.execute_batch("CREATE TABLE future_feature(value TEXT); INSERT INTO future_feature VALUES('retain this unknown metadata');").unwrap();
        connection
            .pragma_update(
                None,
                "user_version",
                deadpan_store::DATABASE_SCHEMA_VERSION + 1,
            )
            .unwrap();
        connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .unwrap();
    }
    let opened = command(&harness.service, ProjectRequest::Open(path));
    assert!(opened.error.is_none(), "{:?}", opened.error);
    let workspace = opened.workspace.unwrap();
    assert!(workspace.read_only.is_some());
    (harness, workspace, selected)
}

fn copy_id(workspace: &Workspace, request: u64) -> slice::CopyId {
    slice::CopyId {
        session: workspace.session,
        project: workspace.document.project_id().clone(),
        source_revision: workspace.document.revision_id().clone(),
        request,
        persisted_version: None,
    }
}

fn refused(
    service: &ProjectService,
    workspace: &Workspace,
    request: ProjectRequest,
) -> ProjectUpdate {
    let update = command(service, request);
    assert!(update.error.as_deref().is_some_and(|error| error.starts_with("Not saved:") && error.contains("newer Deadpan")), "{:?}", update.error);
    assert!(update.committed.is_none());
    assert_eq!(
        *update.workspace.as_ref().unwrap().document,
        *workspace.document
    );
    update
}

#[test]
fn readonly_pending_channels_preserve_captured_identities_and_all_package_bytes() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, workspace, selected) = future_fixture(&scratch.path().join("Documents"));
    let service = &harness.service;
    let before = package_bytes(&workspace.path);
    let bank = ProjectStore::open(&workspace.path, AccessMode::ReadOnly)
        .unwrap()
        .registers()
        .unwrap();
    let source = workspace
        .sources
        .values()
        .find(|source| source.video_index.is_some())
        .unwrap();

    for operation in [
        marks::Operation::Set {
            letter: 'b',
            location: marks::Location::Edit {
                scope: SequenceScope::default(),
                at: ProjectFrame(2),
                selected: Some(selected.clone()),
            },
        },
        marks::Operation::Delete { letter: 'a' },
    ] {
        let mut request = mark(&workspace, 31, operation);
        request.id.session += 17; // Failure must answer the capture, not today's workspace.
        let update = refused(service, &workspace, ProjectRequest::Marks(request.clone()));
        let reply = update.marks.reply.unwrap();
        assert_eq!(reply.id, request.id);
        assert_eq!(reply.result.unwrap_err(), update.error.unwrap());
    }
    let id = macros::Id {
        session: workspace.session + 17,
        project: ProjectId::new("captured-project").unwrap(),
        revision: RevisionId::new("captured-revision").unwrap(),
        bank_version: 7,
        request: 42,
    };
    let update = refused(
        service,
        &workspace,
        ProjectRequest::Macro(macros::Operation::Save {
            id: id.clone(),
            register: 'q',
            program: Arc::new(
                SemanticProgram::new(vec![SemanticInstruction::MoveFrames {
                    forward: true,
                    count: std::num::NonZeroU32::new(1).unwrap(),
                }])
                .unwrap(),
            ),
        }),
    );
    assert_eq!(update.macros.as_ref().unwrap().id, id);
    assert_eq!(
        update.macros.unwrap().result.unwrap_err(),
        update.error.unwrap()
    );

    let correction = CorrectionRequest {
        expected_session: workspace.session + 17,
        attempt: 44,
        key: deadpan_store::CorrectionsKey {
            content: "captured-original".into(),
            audio_stream: 1,
        },
        expected_version: 0,
        change: deadpan_store::CorrectionChange::Undo,
        transcript: None,
        activity: None,
    };
    let update = refused(
        service,
        &workspace,
        ProjectRequest::ChangeCorrections(correction.clone()),
    );
    assert_eq!(
        update.correction_save.unwrap(),
        TranscriptSave {
            session: correction.expected_session,
            attempt: correction.attempt,
            error: update.error
        }
    );

    let update = refused(
        service,
        &workspace,
        ProjectRequest::Target(targets::Operation::Track {
            ticket: 45,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            id: deadpan_core::TargetId::new("captured-target").unwrap(),
            mode: targets::TrackMode::Track {
                through_shots: true,
            },
        }),
    );
    assert_eq!(update.targets.unwrap().reply, Some((45, update.error)));
    for operation in [
        generation::GenerationOperation::Start {
            ticket: 46,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            hold: selected.clone(),
            authoring: None,
            variants: 1,
            options: None,
        },
        generation::GenerationOperation::Preview {
            ticket: 47,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            request: deadpan_jobs::RequestId::new("captured-request").unwrap(),
            attempt: deadpan_jobs::AttemptId::new("captured-attempt").unwrap(),
            draft: 5,
            presentation: None,
        },
        generation::GenerationOperation::Cancel {
            ticket: 48,
            session: workspace.session,
            job: 46,
        },
    ] {
        let ticket = match &operation {
            generation::GenerationOperation::Start { ticket, .. }
            | generation::GenerationOperation::Preview { ticket, .. }
            | generation::GenerationOperation::Cancel { ticket, .. } => *ticket,
            _ => unreachable!(),
        };
        let update = refused(service, &workspace, ProjectRequest::Generation(operation));
        let generation = update.generation.unwrap();
        assert_eq!(generation.reply, Some((ticket, update.error)));
        assert!(generation.job.is_none());
        assert!(generation.preview.is_none());
    }
    let copy = copy_id(&workspace, 49);
    let update = refused(
        service,
        &workspace,
        ProjectRequest::CaptureOriginal(registers::OriginalRequest {
            id: copy.clone(),
            register: Some('b'),
            asset: source.asset.clone(),
            qualification: source.receipt.id().clone(),
            ordinals: 1..4,
        }),
    );
    let reply = update.captured_original.unwrap();
    assert_eq!(reply.id, copy);
    assert_eq!(reply.result.unwrap_err(), update.error.unwrap());
    let capture = slice::CaptureRequest {
        id: copy,
        register: Some('b'),
        scope: SequenceScope::default(),
        parent: workspace.document.root().clone(),
        selection: SliceCaptureSelection::Child {
            node: selected.clone(),
        },
    };
    let update = refused(
        service,
        &workspace,
        ProjectRequest::CaptureEditSlice(capture.clone()),
    );
    assert_eq!(update.captured_slice.as_ref().unwrap().id, capture.id);
    assert_eq!(
        update.captured_slice.unwrap().result.unwrap_err(),
        update.error.unwrap()
    );
    for request in [
        ProjectRequest::CutEditSlice(capture.clone()),
        ProjectRequest::CutFrames {
            capture: capture.clone(),
            attempt: semantic::CutAttempt {
                operation: semantic::RepeatableCut::Frames(deadpan_core::FrameCut::new(1).unwrap()),
                repeat_version: None,
            },
        },
    ] {
        let update = refused(service, &workspace, request);
        assert_eq!(update.cut_slice.as_ref().unwrap().request, capture);
        assert_eq!(
            update.cut_slice.unwrap().result.unwrap_err(),
            update.error.unwrap()
        );
    }
    let id = splice::ProposalId {
        session: workspace.session + 17,
        project: workspace.document.project_id().clone(),
        base_revision: RevisionId::new("captured-revision").unwrap(),
        draft: 51,
        change: 2,
    };
    let update = refused(
        service,
        &workspace,
        ProjectRequest::PrepareSplice(splice::Proposal {
            id: id.clone(),
            operation: splice::Operation::Copy,
            source: splice::Source::Original {
                asset: source.asset.clone(),
                qualification: source.receipt.id().clone(),
                ordinals: 1..4,
            },
            scope: SequenceScope::default(),
            parent: workspace.document.root().clone(),
            destination: splice::Destination::Slot(0),
        }),
    );
    let reply = update.splice.unwrap();
    assert_eq!(reply.id, id);
    assert!(reply.source_view.is_none());
    assert_eq!(reply.result.err(), update.error);
    let update = refused(
        service,
        &workspace,
        ProjectRequest::CommitSplice(id.clone()),
    );
    assert_eq!(update.splice_commit.as_ref().unwrap().id, id);
    assert_eq!(
        update.splice_commit.unwrap().result.unwrap_err(),
        update.error.unwrap()
    );
    let slip = slip::ProposalId {
        session: id.session,
        project: id.project.clone(),
        base_revision: id.base_revision.clone(),
        draft: id.draft,
        change: id.change,
    };
    let update = refused(
        service,
        &workspace,
        ProjectRequest::CommitSlip(slip.clone()),
    );
    assert_eq!(update.slip_commit.as_ref().unwrap().id, slip);
    assert_eq!(
        update.slip_commit.unwrap().result.unwrap_err(),
        update.error.unwrap()
    );
    let trim = trim::ProposalId {
        session: id.session,
        project: id.project,
        base_revision: id.base_revision,
        draft: id.draft,
        change: id.change,
    };
    let update = refused(
        service,
        &workspace,
        ProjectRequest::CommitTrim(trim.clone()),
    );
    assert_eq!(update.trim_commit.as_ref().unwrap().id, trim);
    assert_eq!(
        update.trim_commit.unwrap().result.unwrap_err(),
        update.error.unwrap()
    );
    let update = refused(
        service,
        &workspace,
        ProjectRequest::CleanStorage {
            ticket: 52,
            expected_session: workspace.session + 17,
            previewed: Vec::new(),
        },
    );
    let reply = update.storage_cleanup.unwrap();
    assert_eq!((reply.ticket, reply.session), (52, workspace.session + 17));
    assert_eq!(reply.result.unwrap_err(), update.error.unwrap());
    let update = refused(
        service,
        &workspace,
        ProjectRequest::RelinkOriginal {
            ticket: 53,
            expected_session: workspace.session + 17,
            content: source.original.object().content().clone(),
            expected_version: source.original.version(),
            path: fixture("cfr-bframes.mp4"),
        },
    );
    let reply = update.relink.unwrap();
    assert_eq!((reply.ticket, reply.session), (53, workspace.session + 17));
    assert_eq!(reply.state, RelinkState::Failed(update.error.unwrap()));
    assert_eq!(
        ProjectStore::open(&workspace.path, AccessMode::ReadOnly)
            .unwrap()
            .registers()
            .unwrap(),
        bank
    );
    assert_package_unchanged(&workspace.path, &before);
}

#[test]
fn readonly_queries_and_private_previews_work_without_package_writes() {
    let scratch = tempfile::tempdir().unwrap();
    let (harness, workspace, selected) = future_fixture(&scratch.path().join("Documents"));
    let service = &harness.service;
    let before = package_bytes(&workspace.path);
    let source = workspace
        .sources
        .values()
        .find(|source| source.video_index.is_some())
        .unwrap();
    let request = mark(&workspace, 61, marks::Operation::Jump { letter: 'a' });
    let update = command(service, ProjectRequest::Marks(request.clone()));
    assert_eq!(update.marks.reply.as_ref().unwrap().id, request.id);
    assert!(matches!(
        update.marks.reply.unwrap().result,
        Ok(marks::Outcome::Jumped(_))
    ));
    let update = command(
        service,
        ProjectRequest::PrepareRoomTone {
            expected_session: workspace.session,
            expected_revision: workspace.document.revision_id().clone(),
            ticket: 62,
            selection: RoomToneSelection::Original {
                asset: source.asset.clone(),
                qualification: source.receipt.id().clone(),
                ordinals: 1..4,
            },
        },
    );
    assert!(update.error.is_none(), "{:?}", update.error);
    assert_eq!(update.room_tone.unwrap().ticket, 62);
    let treatments = AudioTreatments::from_clip_gain(
        ClipGain::new(GainDb::new(-3000).unwrap(), false, Vec::new(), Vec::new()).unwrap(),
    );
    let proposal = gain::Proposal {
        target: gain::Target {
            scoped: None,
            session: workspace.session,
            project: workspace.document.project_id().clone(),
            revision: workspace.document.revision_id().clone(),
            scope: SequenceScope::default(),
            node: selected.clone(),
            cursor: ProjectFrame(3),
            entry: workspace.document.nodes()[&selected]
                .audio_treatments
                .clone(),
        },
        draft: 63,
        change: 1,
        treatments,
    };
    let id = proposal.id();
    let update = command(service, ProjectRequest::PrepareGain(proposal));
    assert_eq!(update.gain.as_ref().unwrap().id, id);
    assert!(update.gain.unwrap().result.is_ok());
    let proposal = slip::Proposal {
        target: slip::Target::capture(
            &workspace,
            SequenceScope::default(),
            Some(&selected),
            ProjectFrame(3),
        )
        .unwrap(),
        draft: 64,
        change: 1,
        delta_frames: 2,
    };
    let id = proposal.id();
    let update = command(service, ProjectRequest::PrepareSlip(proposal));
    assert_eq!(update.slip.as_ref().unwrap().id, id);
    assert!(update.slip.unwrap().result.is_ok());
    command(service, ProjectRequest::AbandonSlip(id));
    let proposal = trim::Proposal {
        target: trim::Target::capture(
            &workspace,
            SequenceScope::default(),
            Some(&selected),
            ProjectFrame(3),
        )
        .unwrap(),
        draft: 65,
        change: 1,
        previous_change: None,
        events: vec![trim::Event::Nudge {
            control: SourceTrimControl::Out,
            frames: -1,
        }],
    };
    let id = proposal.id();
    let update = command(service, ProjectRequest::PrepareTrim(proposal));
    assert_eq!(update.trim.as_ref().unwrap().id, id);
    assert!(update.trim.unwrap().result.is_ok());
    command(service, ProjectRequest::AbandonTrim(id));
    // Cancellation reaches its normal handler even when there is no matching job.
    let update = command(
        service,
        ProjectRequest::Target(targets::Operation::Cancel {
            ticket: 66,
            session: workspace.session,
            job: 999,
        }),
    );
    assert!(update.error.is_none());
    assert_eq!(
        update.targets.unwrap().reply,
        Some((66, Some("No matching tracking job is running.".into())))
    );
    assert_package_unchanged(&workspace.path, &before);

    command(service, ProjectRequest::Close);
    let faces = ProjectService::start_with_tracking(
        Arc::new(|| {}),
        None,
        generation::Backend::Environment,
        targets::Backend::ScriptedFaces(Arc::new(targets::FaceScript::new([
            targets::FaceRun::Faces {
                faces: Vec::new(),
                delay: Duration::ZERO,
            },
        ]))),
    )
    .unwrap();
    let workspace = command(&faces, ProjectRequest::Open(workspace.path.clone()))
        .workspace
        .unwrap();
    let before = package_bytes(&workspace.path);
    let source = workspace
        .sources
        .values()
        .find(|source| source.video_index.is_some())
        .unwrap();
    let update = command(
        &faces,
        ProjectRequest::Target(targets::Operation::DetectFaces {
            ticket: 67,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            asset: source.asset.clone(),
            pts: source.video_index.as_ref().unwrap().frames()[0].pts,
        }),
    );
    assert!(update.error.is_none());
    assert_eq!(update.targets.as_ref().unwrap().reply, Some((67, None)));
    let finished = if update
        .targets
        .as_ref()
        .and_then(|targets| targets.faces.as_ref())
        .is_some_and(|faces| faces.outcome.is_some())
    {
        update
    } else {
        wait(&faces, |update| {
            update
                .targets
                .as_ref()
                .and_then(|targets| targets.faces.as_ref())
                .is_some_and(|faces| faces.ticket == 67 && faces.outcome.is_some())
        })
    };
    assert!(
        matches!(finished.targets.unwrap().faces.unwrap().outcome, Some(targets::FaceOutcome::Found(found)) if found.is_empty())
    );
    assert_package_unchanged(&workspace.path, &before);
}
