//! Target saves and tracking through the native project service. The
//! scripted backend keeps range resolution, the verified Original copy, the
//! tracking policy, compaction and the revision-guarded save real; it replaces
//! only the Vision worker's observations.

use deadpan_analysis::{SIGNATURE_VERSION, ShotAnalysis};
use deadpan_core::{
    AttentionTarget, SourceSpan, SourceTimestamp, TargetId, TargetRegion, TargetSource, TrackState,
};

use super::*;
use crate::project::targets::{
    Backend, Job, Operation, Outcome, SCRIPTED_ENGINE, Script, ScriptEnding, ScriptQueue, TrackMode,
};

struct Fixture {
    _scratch: tempfile::TempDir,
    service: ProjectService,
    workspace: Arc<Workspace>,
    asset: AssetId,
    /// Every indexed picture PTS of the Original.
    pictures: Vec<i64>,
}

fn script(ending: ScriptEnding, steps: u8, interval: u64) -> Script {
    Script {
        unavailable: None,
        steps,
        step_interval: Duration::from_millis(interval),
        ending,
    }
}

fn moving() -> Script {
    script(ScriptEnding::Moving { step: 0.002 }, 2, 1)
}

fn project(runs: Vec<Script>) -> Fixture {
    project_with(Backend::Scripted(Arc::new(ScriptQueue::new(runs))))
}

fn project_with(backend: Backend) -> Fixture {
    let scratch = tempfile::tempdir().unwrap();
    let service = ProjectService::start_with_tracking(
        Arc::new(|| {}),
        Some(ProjectLibrary::from_documents(scratch.path().join("Documents")).unwrap()),
        crate::project::generation::Backend::Environment,
        backend,
    )
    .unwrap();
    service
        .submit(ProjectRequest::CreateFromSource {
            path: fixture("cfr-bframes.mp4"),
        })
        .unwrap();
    let initialized = wait(&service, |update| {
        update.import.as_ref().is_some_and(|status| {
            matches!(status.stage, ImportStage::Complete | ImportStage::Failed)
        }) && !service.is_busy()
    });
    let workspace = initialized.workspace.unwrap();
    let Some(SingleSourceState::Ready { asset, .. }) = &workspace.single_source else {
        panic!("original not initialized")
    };
    let asset = asset.clone();
    let pictures = workspace.sources[&asset]
        .video_index
        .as_ref()
        .unwrap()
        .frames()
        .iter()
        .map(|frame| frame.pts)
        .collect();
    Fixture {
        _scratch: scratch,
        service,
        workspace,
        asset,
        pictures,
    }
}

/// A drawn target from picture `first` to the end of the video.
fn drawn(fixture: &Fixture, first: usize) -> AttentionTarget {
    let video = fixture.workspace.document.assets()[&fixture.asset]
        .video
        .unwrap();
    let time_base = video.start().time_base;
    AttentionTarget {
        label: "Target 1".into(),
        asset: fixture.asset.clone(),
        span: SourceSpan::new(
            SourceTimestamp {
                ticks: fixture.pictures[first],
                time_base,
            },
            video.end(),
        )
        .unwrap(),
        region: TargetRegion {
            center: [400_000, 500_000],
            size: [200_000, 200_000],
        },
        samples: Vec::new(),
        corrections: Vec::new(),
        provenance: None,
    }
}

fn targets(service: &ProjectService, operation: Operation) -> ProjectUpdate {
    command(service, ProjectRequest::Target(operation))
}

fn refusal(update: &ProjectUpdate) -> Option<String> {
    update.targets.as_ref()?.reply.as_ref()?.1.clone()
}

fn job_until(service: &ProjectService, predicate: impl Fn(&Job) -> bool) -> ProjectUpdate {
    wait(service, |update| {
        update
            .targets
            .as_ref()
            .and_then(|targets| targets.job.as_ref())
            .is_some_and(&predicate)
            && !service.is_busy()
    })
}

fn concluded(service: &ProjectService, ticket: u64) -> (Outcome, ProjectUpdate) {
    let update = job_until(service, |job| job.ticket == ticket && !job.running());
    let outcome = update
        .targets
        .as_ref()
        .and_then(|targets| targets.job.as_ref())
        .and_then(|job| job.outcome.clone())
        .unwrap();
    (outcome, update)
}

fn save(
    fixture: &Fixture,
    workspace: &Workspace,
    id: &str,
    target: AttentionTarget,
) -> ProjectUpdate {
    let update = targets(
        &fixture.service,
        Operation::Save {
            ticket: 1,
            session: workspace.session,
            revision: workspace.document.revision_id().clone(),
            id: TargetId::new(id).unwrap(),
            target: Box::new(target),
        },
    );
    assert_eq!(refusal(&update), None);
    update
}

fn track(workspace: &Workspace, ticket: u64, id: &str, mode: TrackMode) -> Operation {
    Operation::Track {
        ticket,
        session: workspace.session,
        revision: workspace.document.revision_id().clone(),
        id: TargetId::new(id).unwrap(),
        mode,
    }
}

fn store_cut(fixture: &Fixture, cut: usize) -> Arc<Workspace> {
    let content = fixture.workspace.sources[&fixture.asset]
        .receipt
        .original()
        .content()
        .to_string();
    let analysis = ShotAnalysis::from_changes(
        (0..fixture.pictures.len())
            .map(|picture| match picture {
                0 => [0, 0, 0],
                _ if picture == cut => [90, 200, 90],
                _ if picture == cut + 1 => [2, 1, 90],
                _ => [2, 1, 2],
            })
            .collect(),
    )
    .unwrap();
    assert_eq!(analysis.boundaries(), [cut]);
    let _ = fixture.service.take_update();
    fixture
        .service
        .submit(ProjectRequest::SaveShotAnalysis {
            expected_session: fixture.workspace.session,
            attempt: 1,
            key: deadpan_store::ShotAnalysisKey {
                content,
                video_stream: 0,
                signature_version: SIGNATURE_VERSION.into(),
            },
            analysis: Arc::new(analysis),
        })
        .unwrap();
    let update = wait(&fixture.service, |update| {
        update
            .shot_save
            .as_ref()
            .is_some_and(|save| save.attempt == 1)
            && !fixture.service.is_busy()
    });
    assert_eq!(update.shot_save.unwrap().error, None);
    update.workspace.unwrap()
}

#[test]
fn a_drawn_target_is_one_undoable_edit_and_tracking_needs_shots_or_an_explicit_crossing() {
    let fixture = project(vec![moving()]);
    let before = fixture.workspace.clone();
    let update = save(&fixture, &before, "target-1", drawn(&fixture, 2));
    let saved = update.targets.as_ref().unwrap().saved.clone().unwrap();
    let workspace = update.workspace.unwrap();
    assert_eq!(&saved.base, before.document.revision_id());
    assert_eq!(&saved.revision, workspace.document.revision_id());
    assert_eq!(
        workspace.document.targets()[&TargetId::new("target-1").unwrap()],
        drawn(&fixture, 2)
    );
    // Cursor and selection stay put: the save has no committed selection.
    assert!(update.committed.is_none());

    // Without a stored shot analysis, tracking refuses rather than crossing cuts.
    let refused = targets(
        &fixture.service,
        track(
            &workspace,
            2,
            "target-1",
            TrackMode::Track {
                through_shots: false,
            },
        ),
    );
    assert_eq!(refusal(&refused), None);
    let (outcome, update) = concluded(&fixture.service, 2);
    let Outcome::Unavailable(reason) = outcome else {
        panic!("expected unavailable, got {outcome:?}")
    };
    assert!(reason.contains("shot analysis"), "{reason}");
    assert_eq!(
        update.workspace.unwrap().document.revision_id(),
        workspace.document.revision_id()
    );

    // A stale save refuses without writing.
    let stale = targets(
        &fixture.service,
        Operation::Save {
            ticket: 3,
            session: before.session,
            revision: before.document.revision_id().clone(),
            id: TargetId::new("target-2").unwrap(),
            target: Box::new(drawn(&fixture, 3)),
        },
    );
    assert!(refusal(&stale).is_some());

    let undone = command(
        &fixture.service,
        ProjectRequest::Undo {
            expected_revision: workspace.document.revision_id().clone(),
        },
    );
    assert!(undone.workspace.unwrap().document.targets().is_empty());
}

#[test]
fn tracking_stops_at_the_stored_cut_saves_against_its_entry_head_and_corrects_one_range() {
    let fixture = project(vec![moving()]);
    let cut = fixture.pictures.len() / 2;
    let shots = store_cut(&fixture, cut);
    let update = save(&fixture, &shots, "target-1", drawn(&fixture, 2));
    let workspace = update.workspace.unwrap();
    let started = targets(
        &fixture.service,
        track(
            &workspace,
            2,
            "target-1",
            TrackMode::Track {
                through_shots: false,
            },
        ),
    );
    assert_eq!(refusal(&started), None);
    let (outcome, update) = concluded(&fixture.service, 2);
    let Outcome::Saved {
        revision, samples, ..
    } = outcome
    else {
        panic!("expected a saved path, got {outcome:?}")
    };
    let tracked = update.workspace.unwrap();
    assert_eq!(tracked.document.revision_id(), &revision);
    let saved = update.targets.unwrap().saved.unwrap();
    assert_eq!(&saved.base, workspace.document.revision_id());
    let id = TargetId::new("target-1").unwrap();
    let target = tracked.document.targets()[&id].clone();
    assert_eq!(target.samples.len(), samples);
    assert!(samples > 0);
    assert_eq!(target.label, "Target 1");
    assert_eq!(target.span.start().ticks, fixture.pictures[2]);
    assert_eq!(
        target.span.end().ticks,
        fixture.pictures[cut],
        "stops at the cut"
    );
    let provenance = target.provenance.clone().unwrap();
    assert_eq!(provenance.engine, SCRIPTED_ENGINE);
    assert_eq!(provenance.stop, deadpan_core::TargetStop::ShotBoundary);
    // The scripted subject moves right; the saved path follows it.
    let at = |index: usize| deadpan_core::SourcePoint {
        ticks: deadpan_core::ExactRatio::integer(fixture.pictures[index]),
        time_base: target.span.start().time_base,
    };
    let (early, _) = target.region_at(at(3)).unwrap();
    let (later, source) = target.region_at(at(cut - 1)).unwrap();
    assert!(later.center[0] > early.center[0]);
    assert_eq!(source, TargetSource::Tracked(TrackState::Tracked));

    // A correction re-tracks only from its picture; earlier positions stay.
    let correction = cut - 4;
    let region = TargetRegion {
        center: [300_000, 300_000],
        size: [150_000, 150_000],
    };
    let started = targets(
        &fixture.service,
        track(
            &tracked,
            4,
            "target-1",
            TrackMode::Correct {
                at: fixture.pictures[correction],
                region,
            },
        ),
    );
    assert_eq!(refusal(&started), None);
    let (outcome, update) = concluded(&fixture.service, 4);
    assert!(matches!(outcome, Outcome::Saved { .. }), "{outcome:?}");
    let corrected = update.workspace.unwrap().document.targets()[&id].clone();
    assert_eq!(corrected.corrections.len(), 1);
    assert_eq!(corrected.corrections[0].at, fixture.pictures[correction]);
    assert_eq!(corrected.span, target.span);
    for index in 2..correction {
        assert_eq!(corrected.region_at(at(index)), target.region_at(at(index)));
    }
    let (after, _) = corrected.region_at(at(correction)).unwrap();
    assert_eq!(after.center, region.center);
}

#[test]
fn an_edit_made_while_tracking_refuses_the_save_and_a_second_start_is_refused() {
    let fixture = project(vec![script(ScriptEnding::Moving { step: 0.002 }, 20, 25)]);
    let update = save(&fixture, &fixture.workspace, "target-1", drawn(&fixture, 2));
    let workspace = update.workspace.unwrap();
    let started = targets(
        &fixture.service,
        track(
            &workspace,
            2,
            "target-1",
            TrackMode::Track {
                through_shots: true,
            },
        ),
    );
    assert_eq!(refusal(&started), None);
    let again = targets(
        &fixture.service,
        track(
            &workspace,
            3,
            "target-1",
            TrackMode::Track {
                through_shots: true,
            },
        ),
    );
    assert!(refusal(&again).is_some_and(|reason| reason.contains("already tracking")));
    // Another ordinary edit lands while tracking runs.
    let edited = save(&fixture, &workspace, "target-2", drawn(&fixture, 5));
    let current = edited.workspace.unwrap();
    let (outcome, update) = concluded(&fixture.service, 2);
    let Outcome::Failed(reason) = outcome else {
        panic!("expected a refused save, got {outcome:?}")
    };
    // Depending on timing the job sees the edit before or after tracking.
    assert!(reason.contains("project changed"), "{reason}");
    let after = update.workspace.unwrap();
    assert_eq!(after.document.revision_id(), current.document.revision_id());
    assert!(
        after.document.targets()[&TargetId::new("target-1").unwrap()]
            .samples
            .is_empty()
    );
}

#[test]
fn cancellation_and_closing_drain_the_job_without_saving() {
    let fixture = project(vec![script(ScriptEnding::WaitForCancel, 2, 1)]);
    let update = save(&fixture, &fixture.workspace, "target-1", drawn(&fixture, 2));
    let workspace = update.workspace.unwrap();
    targets(
        &fixture.service,
        track(
            &workspace,
            2,
            "target-1",
            TrackMode::Track {
                through_shots: true,
            },
        ),
    );
    let running = job_until(&fixture.service, |job| {
        matches!(job.phase, crate::project::targets::Phase::Tracking(_))
    });
    let job = running.targets.unwrap().job.unwrap().ticket;
    let cancelled = targets(
        &fixture.service,
        Operation::Cancel {
            ticket: 3,
            session: workspace.session,
            job,
        },
    );
    assert_eq!(refusal(&cancelled), None);
    let (outcome, update) = concluded(&fixture.service, 2);
    assert_eq!(outcome, Outcome::Cancelled);
    assert_eq!(
        update.workspace.unwrap().document.revision_id(),
        workspace.document.revision_id()
    );

    // Closing while a job runs cancels it and waits for it to drain.
    targets(
        &fixture.service,
        track(
            &workspace,
            4,
            "target-1",
            TrackMode::Track {
                through_shots: true,
            },
        ),
    );
    job_until(&fixture.service, |job| job.ticket == 4 && job.running());
    fixture.service.submit(ProjectRequest::Close).unwrap();
    let closed = wait(&fixture.service, |update| {
        update.workspace.is_none() && !fixture.service.is_busy()
    });
    assert!(closed.error.is_none(), "{:?}", closed.error);
}

#[test]
fn a_worker_failure_reports_its_reason_and_saves_nothing() {
    let fixture = project(vec![script(
        ScriptEnding::Fail("Scripted tracker failure".into()),
        1,
        1,
    )]);
    let update = save(&fixture, &fixture.workspace, "target-1", drawn(&fixture, 2));
    let workspace = update.workspace.unwrap();
    targets(
        &fixture.service,
        track(
            &workspace,
            2,
            "target-1",
            TrackMode::Track {
                through_shots: true,
            },
        ),
    );
    let (outcome, update) = concluded(&fixture.service, 2);
    assert_eq!(outcome, Outcome::Failed("Scripted tracker failure".into()));
    assert_eq!(
        update.workspace.unwrap().document.revision_id(),
        workspace.document.revision_id()
    );
}

mod faces;
