//! Real project preparation/admission with explicitly scripted detector output.
//! These tests do not claim Vision execution or UI cursor routing.
use super::*;
use crate::project::targets::{
    ProposalJob, ProposalOrigin, ProposalOutcome, ProposalRun, ProposalScript, ProposedTargets,
};
use crate::project::{registers, slice};
use deadpan_analysis::NormalizedRect;
use deadpan_cli::picture_proposals::{DetectedFace, DetectedSalientRegion, ProposalSelection};

fn face(x: f64) -> DetectedFace {
    DetectedFace {
        region: NormalizedRect::new(x, 0.2, 0.1, 0.2).unwrap(),
        confidence: 0.8,
    }
}
fn region(x: f64) -> DetectedSalientRegion {
    DetectedSalientRegion {
        region: NormalizedRect::new(x, 0.3, 0.1, 0.2).unwrap(),
    }
}
fn run(
    selection: ProposalSelection,
    faces: Option<Vec<DetectedFace>>,
    salient_regions: Option<Vec<DetectedSalientRegion>>,
) -> ProposalRun {
    ProposalRun::Proposals {
        selection,
        faces,
        salient_regions,
        delay: Duration::ZERO,
    }
}
fn scripted(runs: impl IntoIterator<Item = ProposalRun>) -> Fixture {
    project_with(Backend::ScriptedProposals(Arc::new(ProposalScript::new(
        runs,
    ))))
}
fn detect(
    fixture: &Fixture,
    workspace: &Workspace,
    ticket: u64,
    selection: ProposalSelection,
) -> ProjectUpdate {
    targets(
        &fixture.service,
        Operation::DetectPictureProposals {
            ticket,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            asset: fixture.asset.clone(),
            pts: fixture.pictures[2],
            selection,
        },
    )
}
fn done(
    service: &ProjectService,
    first: ProjectUpdate,
    ticket: u64,
) -> (ProposalJob, ProjectUpdate) {
    let matches = |update: &ProjectUpdate| {
        update
            .targets
            .as_ref()
            .and_then(|target| target.proposals.as_ref())
            .is_some_and(|job| job.ticket == ticket && job.outcome.is_some())
    };
    let update = if matches(&first) {
        first
    } else {
        wait(service, matches)
    };
    let job = update.targets.as_ref().unwrap().proposals.clone().unwrap();
    (job, update)
}
fn capture(fixture: &Fixture, workspace: &Workspace) -> ProjectUpdate {
    let copied = command(
        &fixture.service,
        ProjectRequest::CaptureOriginal(registers::OriginalRequest {
            id: slice::CopyId {
                session: workspace.session,
                project: workspace.document.project_id().clone(),
                source_revision: workspace.document.revision_id().clone(),
                request: 900,
                persisted_version: None,
            },
            register: Some('a'),
            asset: fixture.asset.clone(),
            qualification: workspace.sources[&fixture.asset].receipt.id().clone(),
            ordinals: 1..4,
        }),
    );
    assert!(copied.captured_original.as_ref().unwrap().result.is_ok());
    assert!(
        copied
            .registers
            .as_ref()
            .unwrap()
            .entries
            .contains_key(&'a')
    );
    copied
}
fn stored(
    workspace: &Workspace,
) -> (
    ProjectDocument,
    deadpan_store::registers::RegisterBank,
    Vec<String>,
) {
    let store = ProjectStore::open(&workspace.path, AccessMode::ReadOnly).unwrap();
    let head = store.snapshot().unwrap();
    let bank = store.registers().unwrap();
    let connection = rusqlite::Connection::open_with_flags(
        workspace.path.join("project.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut statement = connection
        .prepare(
            "SELECT json_array(id,parent_id,revision_id,request,edit) FROM history ORDER BY id",
        )
        .unwrap();
    let history = statement
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    (head, bank, history)
}
fn assert_found(
    job: &ProposalJob,
    faces: Option<Vec<DetectedFace>>,
    regions: Option<Vec<DetectedSalientRegion>>,
) {
    assert_eq!(
        job.outcome,
        Some(ProposalOutcome::Found(ProposedTargets {
            faces,
            salient_regions: regions,
            origin: ProposalOrigin::Scripted,
        }))
    );
}

#[test]
fn both_returns_separate_scripted_collections_without_target_register_history_or_cursor_effects() {
    let fixture = scripted([run(
        ProposalSelection::Both,
        Some(vec![face(0.1), face(0.5)]),
        Some(vec![region(0.2), region(0.6)]),
    )]);
    let saved = save(
        &fixture,
        &fixture.workspace,
        "kept-target",
        drawn(&fixture, 1),
    );
    let workspace = saved.workspace.unwrap();
    let copied = capture(&fixture, &workspace);
    let bank = copied.registers.as_ref().unwrap().clone();
    let before = stored(&workspace);
    let started = detect(&fixture, &workspace, 10, ProposalSelection::Both);
    assert_eq!(refusal(&started), None);
    let entry_commit = started.committed.clone();
    let (job, finished) = done(&fixture.service, started, 10);
    assert_eq!(job.selection, ProposalSelection::Both);
    assert_eq!(job.session, workspace.session);
    assert_eq!(&job.revision, workspace.document.revision_id());
    assert_eq!(job.asset, fixture.asset);
    assert_eq!(job.pts, fixture.pictures[2]);
    assert_found(
        &job,
        Some(vec![face(0.1), face(0.5)]),
        Some(vec![region(0.2), region(0.6)]),
    );
    assert_eq!(finished.committed, entry_commit);
    assert_eq!(finished.registers.as_ref().unwrap().version, bank.version);
    assert_eq!(
        format!("{:?}", finished.registers.as_ref().unwrap().entries),
        format!("{:?}", bank.entries)
    );
    assert_eq!(*finished.workspace.unwrap().document, *workspace.document);
    assert_eq!(stored(&workspace), before);
}

#[test]
fn salient_only_and_successful_empty_batches_do_not_invent_unrequested_faces() {
    let fixture = scripted([
        run(
            ProposalSelection::SalientRegions,
            None,
            Some(vec![region(0.2)]),
        ),
        run(ProposalSelection::SalientRegions, None, Some(vec![])),
        run(ProposalSelection::Both, Some(vec![]), Some(vec![])),
    ]);
    let before = stored(&fixture.workspace);
    for (ticket, selection, faces, regions) in [
        (
            1,
            ProposalSelection::SalientRegions,
            None,
            Some(vec![region(0.2)]),
        ),
        (2, ProposalSelection::SalientRegions, None, Some(vec![])),
        (3, ProposalSelection::Both, Some(vec![]), Some(vec![])),
    ] {
        let started = detect(&fixture, &fixture.workspace, ticket, selection);
        assert_eq!(refusal(&started), None);
        let (job, finished) = done(&fixture.service, started, ticket);
        assert_eq!(job.selection, selection);
        assert_found(&job, faces, regions);
        assert_eq!(
            *finished.workspace.unwrap().document,
            *fixture.workspace.document
        );
        assert_eq!(stored(&fixture.workspace), before);
    }
}

#[test]
fn mismatched_partial_unsorted_and_over_limit_batches_fail_atomically() {
    let fixture = scripted([
        run(ProposalSelection::Faces, Some(vec![face(0.1)]), None),
        run(ProposalSelection::Both, Some(vec![face(0.1)]), None),
        run(ProposalSelection::Both, None, Some(vec![region(0.2)])),
        run(
            ProposalSelection::Both,
            Some(vec![face(0.5), face(0.1)]),
            Some(vec![]),
        ),
        run(
            ProposalSelection::Both,
            Some(vec![]),
            Some(vec![region(0.6), region(0.2)]),
        ),
        run(
            ProposalSelection::Both,
            Some(vec![]),
            Some((0..65).map(|i| region(f64::from(i) * 0.001)).collect()),
        ),
        ProposalRun::Failed {
            reason: "Saliency revision2 refused; no partial batch".into(),
            delay: Duration::ZERO,
        },
    ]);
    let _copy = capture(&fixture, &fixture.workspace);
    let before = stored(&fixture.workspace);
    for ticket in 1..=7 {
        let started = detect(
            &fixture,
            &fixture.workspace,
            ticket,
            ProposalSelection::Both,
        );
        assert_eq!(refusal(&started), None);
        let (job, finished) = done(&fixture.service, started, ticket);
        assert_eq!(job.selection, ProposalSelection::Both);
        let Some(ProposalOutcome::Failed(reason)) = job.outcome else {
            panic!("partial batch escaped: {:?}", job.outcome)
        };
        assert!(!reason.is_empty());
        assert_eq!(stored(&fixture.workspace), before);
        assert_eq!(
            *finished.workspace.unwrap().document,
            *fixture.workspace.document
        );
    }
}

#[test]
fn faces_only_batch_cannot_fill_a_missing_saliency_collection() {
    let fixture = scripted([run(ProposalSelection::Faces, Some(vec![face(0.1)]), None)]);
    let before = stored(&fixture.workspace);
    let started = detect(&fixture, &fixture.workspace, 1, ProposalSelection::Both);
    let (job, _) = done(&fixture.service, started, 1);
    assert!(
        matches!(job.outcome,Some(ProposalOutcome::Failed(reason)) if reason.contains("selection"))
    );
    assert_eq!(stored(&fixture.workspace), before);
}

#[test]
fn a_delayed_batch_keeps_its_old_identity_and_cannot_overwrite_a_newer_save() {
    let fixture = scripted([ProposalRun::Proposals {
        selection: ProposalSelection::Both,
        faces: Some(vec![face(0.1)]),
        salient_regions: Some(vec![region(0.2)]),
        delay: Duration::from_secs(1),
    }]);
    let copied = capture(&fixture, &fixture.workspace);
    let bank = copied.registers.unwrap();
    let started = detect(&fixture, &fixture.workspace, 1, ProposalSelection::Both);
    assert_eq!(refusal(&started), None);
    assert!(
        started
            .targets
            .as_ref()
            .unwrap()
            .proposals
            .as_ref()
            .unwrap()
            .outcome
            .is_none()
    );
    let saved = targets(
        &fixture.service,
        Operation::Save {
            ticket: 2,
            session: fixture.workspace.session,
            revision: fixture.workspace.document.revision_id().clone(),
            id: TargetId::new("newer-target").unwrap(),
            target: Box::new(drawn(&fixture, 2)),
        },
    );
    assert_eq!(refusal(&saved), None);
    let workspace = saved.workspace.as_ref().unwrap().clone();
    let before = stored(&workspace);
    let notice = saved.message.clone();
    let committed = saved.committed.clone();
    let (job, finished) = done(&fixture.service, saved, 1);
    assert_eq!(&job.revision, fixture.workspace.document.revision_id());
    assert_ne!(&job.revision, workspace.document.revision_id());
    // Service results retain their captured old revision. Camera's separate
    // exact presentation/revision guard rejects this now-stale proposal.
    if let Some(ProposalOutcome::Found(found)) = &job.outcome {
        assert_eq!(found.origin, ProposalOrigin::Scripted);
    } else {
        assert!(
            matches!(&job.outcome,Some(ProposalOutcome::Failed(reason)) if reason.contains("changed"))
        );
    }
    assert_eq!(finished.message, notice);
    assert_eq!(finished.committed, committed);
    assert_eq!(finished.registers.unwrap().version, bank.version);
    assert_eq!(*finished.workspace.unwrap().document, *workspace.document);
    assert_eq!(stored(&workspace), before);
    assert!(
        refusal(&detect(
            &fixture,
            &fixture.workspace,
            3,
            ProposalSelection::Both
        ))
        .is_some()
    );
}

#[test]
fn closing_cancels_both_and_drains_without_saving_any_proposal() {
    let fixture = scripted([ProposalRun::Proposals {
        selection: ProposalSelection::Both,
        faces: Some(vec![face(0.1)]),
        salient_regions: Some(vec![region(0.2)]),
        delay: Duration::from_secs(30),
    }]);
    let before = stored(&fixture.workspace);
    let started = detect(&fixture, &fixture.workspace, 1, ProposalSelection::Both);
    assert_eq!(refusal(&started), None);
    assert!(
        refusal(&detect(
            &fixture,
            &fixture.workspace,
            2,
            ProposalSelection::SalientRegions
        ))
        .is_some()
    );
    let time = Instant::now();
    let closed = command(&fixture.service, ProjectRequest::Close);
    assert!(closed.workspace.is_none());
    assert!(time.elapsed() < Duration::from_secs(10));
    assert_eq!(stored(&fixture.workspace), before);
}

#[test]
fn cancel_matches_proposal_session_and_ticket_and_completed_cancel_is_idempotent() {
    let fixture = scripted([
        ProposalRun::Proposals {
            selection: ProposalSelection::Both,
            faces: Some(vec![face(0.1)]),
            salient_regions: Some(vec![region(0.2)]),
            delay: Duration::from_secs(30),
        },
        run(
            ProposalSelection::SalientRegions,
            None,
            Some(vec![region(0.6)]),
        ),
    ]);
    let copied = capture(&fixture, &fixture.workspace);
    let bank = copied.registers.unwrap();
    let before = stored(&fixture.workspace);
    let started = detect(&fixture, &fixture.workspace, 1, ProposalSelection::Both);
    assert_eq!(refusal(&started), None);
    assert!(
        started
            .targets
            .as_ref()
            .unwrap()
            .proposals
            .as_ref()
            .unwrap()
            .outcome
            .is_none()
    );
    let entry_commit = started.committed.clone();
    let cancel = |ticket, session, job| {
        targets(
            &fixture.service,
            Operation::Cancel {
                ticket,
                session,
                job,
            },
        )
    };

    for (ticket, session, job) in [
        (2, fixture.workspace.session, 99),
        (3, fixture.workspace.session + 1, 1),
    ] {
        let refused = cancel(ticket, session, job);
        assert!(refusal(&refused).is_some_and(|reason| reason.contains("No matching target job")));
        let pending = refused
            .targets
            .as_ref()
            .unwrap()
            .proposals
            .as_ref()
            .unwrap();
        assert_eq!(pending.ticket, 1);
        assert!(pending.outcome.is_none());
        assert_eq!(refused.committed, entry_commit);
        assert_eq!(stored(&fixture.workspace), before);
    }

    let cancelled = cancel(4, fixture.workspace.session, 1);
    assert_eq!(refusal(&cancelled), None);
    let (job, finished) = done(&fixture.service, cancelled, 1);
    assert_eq!(job.outcome, Some(ProposalOutcome::Cancelled));
    assert_eq!(finished.committed, entry_commit);
    assert_eq!(finished.registers.as_ref().unwrap().version, bank.version);
    assert_eq!(stored(&fixture.workspace), before);
    let repeated = cancel(5, fixture.workspace.session, 1);
    assert_eq!(refusal(&repeated), None);
    assert_eq!(
        repeated.targets.unwrap().proposals.unwrap().outcome,
        Some(ProposalOutcome::Cancelled)
    );
    assert_eq!(stored(&fixture.workspace), before);

    // Closing can race successful completion too. Acknowledge that exact
    // completed job without retracting its full result or authoring anything.
    let started = detect(
        &fixture,
        &fixture.workspace,
        6,
        ProposalSelection::SalientRegions,
    );
    assert_eq!(refusal(&started), None);
    let (job, _) = done(&fixture.service, started, 6);
    assert_found(&job, None, Some(vec![region(0.6)]));
    let completed = cancel(7, fixture.workspace.session, 6);
    assert_eq!(refusal(&completed), None);
    assert_eq!(
        completed
            .targets
            .as_ref()
            .unwrap()
            .proposals
            .as_ref()
            .unwrap()
            .outcome,
        job.outcome
    );
    assert_eq!(completed.committed, entry_commit);
    assert_eq!(stored(&fixture.workspace), before);

    let unrelated = cancel(8, fixture.workspace.session, 1);
    assert!(refusal(&unrelated).is_some());
    assert_eq!(unrelated.targets.unwrap().proposals.unwrap().ticket, 6);
    assert_eq!(stored(&fixture.workspace), before);
}
