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
    project_from(backend, fixture("cfr-bframes.mp4"))
}

fn project_from(backend: Backend, original: std::path::PathBuf) -> Fixture {
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
            ownership: OriginalOwnership::Managed,
            path: original,
        })
        .unwrap();
    let initialized = wait(&service, |update| {
        update.import.as_ref().is_some_and(|status| {
            matches!(status.stage, ImportStage::Complete | ImportStage::Failed)
        }) && !service.is_busy()
    });
    let workspace = initialized.workspace.unwrap();
    let Some(SingleSourceState::Ready { asset, .. }) = &workspace.single_source else {
        panic!("original not initialized: {:?}", initialized.import)
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
        }
        .into(),
        samples: Vec::new(),
        corrections: Vec::new(),
        provenance: None,
        display_aspect: None,
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
                interpretation: None,
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
fn point_targets_save_and_correct_without_launching_rectangle_tracking() {
    // No scripted runs: any accidental launch cannot yield a successful result.
    let fixture = project(vec![]);
    let mut point = drawn(&fixture, 2);
    point.region = deadpan_core::TargetPoint {
        center: [400_000, 500_000],
    }
    .into();
    let saved = save(&fixture, &fixture.workspace, "point", point.clone());
    let workspace = saved.workspace.unwrap();
    let before = workspace.document.clone();
    for mode in [
        TrackMode::Track {
            through_shots: true,
        },
        TrackMode::Correct {
            at: fixture.pictures[6],
            region: TargetRegion {
                center: [300_000, 400_000],
                size: [200_000, 200_000],
            },
        },
    ] {
        let refused = targets(&fixture.service, track(&workspace, 2, "point", mode));
        assert!(refusal(&refused).unwrap().contains("no tracking rectangle"));
        assert!(refused.targets.as_ref().unwrap().job.is_none());
        assert_eq!(
            refused.workspace.unwrap().document.as_ref(),
            before.as_ref()
        );
    }
    let corrected = point
        .with_point_correction(
            fixture.pictures[6],
            deadpan_core::TargetPoint {
                center: [600_000, 300_000],
            },
        )
        .unwrap();
    let saved = save(&fixture, &workspace, "point", corrected.clone());
    let corrected_workspace = saved.workspace.unwrap();
    assert_eq!(
        corrected_workspace.document.targets()[&TargetId::new("point").unwrap()],
        corrected
    );
    assert!(saved.targets.unwrap().job.is_none());
    let undone = command(
        &fixture.service,
        ProjectRequest::Undo {
            expected_revision: corrected_workspace.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(
        undone.document.targets()[&TargetId::new("point").unwrap()],
        point
    );
    let redone = command(
        &fixture.service,
        ProjectRequest::Redo {
            expected_revision: undone.document.revision_id().clone(),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(
        redone.document.targets()[&TargetId::new("point").unwrap()],
        corrected
    );
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
    // The qualified 320x180 square-pixel Original's exact upright aspect.
    assert_eq!(
        target.display_aspect,
        Some(deadpan_core::ExactRatio::new(16, 9).unwrap())
    );
    // The scripted subject moves right; the saved path follows it.
    let at = |index: usize| deadpan_core::SourcePoint {
        ticks: deadpan_core::ExactRatio::integer(fixture.pictures[index]),
        time_base: target.span.start().time_base,
    };
    let (early, _) = target.region_at(at(3)).unwrap();
    let (later, source) = target.region_at(at(cut - 1)).unwrap();
    assert!(later.center()[0] > early.center()[0]);
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
    assert_eq!(after.center(), region.center);
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
mod picture_proposals;
mod sleep;

#[test]
fn a_target_in_use_refuses_deletion_and_an_unused_one_deletes_in_one_undo() {
    let fixture = project(vec![moving()]);
    let saved = save(&fixture, &fixture.workspace, "target-1", drawn(&fixture, 2));
    let workspace = saved.workspace.unwrap();
    let id = TargetId::new("target-1").unwrap();
    let NodeKind::Sequence { children } =
        &workspace.document.nodes()[workspace.document.root()].kind
    else {
        panic!()
    };
    let beat = children[0].clone();
    let follow = deadpan_core::Framing {
        clock: deadpan_core::FramingClock::OwnerOutput,
        value: deadpan_core::FramingValue::Follow {
            centering: Default::default(),
            target: id.clone(),
            scale: deadpan_core::ExactRatio::integer(2),
            fallback: deadpan_core::FramingPose::identity(),
            scale_transition: None,
        },
    };
    let followed = edited(
        &fixture.service,
        &workspace,
        ProjectEdit::SetFraming {
            node: beat.clone(),
            framing: Some(follow),
        },
    )
    .workspace
    .unwrap();
    assert_eq!(
        crate::project::targets::users(&followed.document, &id).len(),
        1
    );
    let delete = |workspace: &Workspace, ticket| Operation::Delete {
        ticket,
        session: workspace.session,
        revision: workspace.document.revision_id().clone(),
        id: id.clone(),
    };
    let refused = targets(&fixture.service, delete(&followed, 5));
    let reason = refusal(&refused).expect("a used target refuses deletion");
    assert!(reason.contains("used by"), "{reason}");
    assert!(
        refused
            .workspace
            .unwrap()
            .document
            .targets()
            .contains_key(&id)
    );
    // Without the follow, deletion is one undoable edit.
    let released = edited(
        &fixture.service,
        &followed,
        ProjectEdit::SetFraming {
            node: beat,
            framing: None,
        },
    )
    .workspace
    .unwrap();
    let deleted = targets(&fixture.service, delete(&released, 6));
    assert_eq!(refusal(&deleted), None);
    let after = deleted.workspace.unwrap();
    assert!(!after.document.targets().contains_key(&id));
    let undone = command(
        &fixture.service,
        ProjectRequest::Undo {
            expected_revision: after.document.revision_id().clone(),
        },
    );
    assert!(
        undone
            .workspace
            .unwrap()
            .document
            .targets()
            .contains_key(&id)
    );
}

/// `deadpan-track` from the workspace build (target/<profile>/), beside the
/// directory holding this test binary. Skips when absent unless
/// `DEADPAN_REQUIRE_SYNTHETIC_WORKER=1` (set by `cargo xtask gate`).
fn installed_tracker() -> Option<std::path::PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let worker = executable.parent()?.parent()?.join("deadpan-track");
    if worker.is_file() {
        return Some(worker);
    }
    assert!(
        std::env::var_os("DEADPAN_REQUIRE_SYNTHETIC_WORKER").is_none_or(|value| value != "1"),
        "the real tracking worker is missing: {}",
        worker.display()
    );
    eprintln!("skipping: build deadpan-track to run {}", worker.display());
    None
}

/// The app's real-worker path: the project service launches the installed
/// Vision worker on the verified Original and saves its path, which follows
/// the moving square (40 + 5n, 40 + 1.5n coded px, 36 px wide) inside the
/// fixture's exact clean aperture [20.25, 10.25, 279.5, 159.5].
#[test]
fn the_installed_tracker_follows_the_square_through_the_app_service() {
    let Some(worker) = installed_tracker() else {
        return;
    };
    let fixture = project_from(
        Backend::Installed(worker),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-track/tests/fixtures/moving-square-aperture.mp4")
            .canonicalize()
            .unwrap(),
    );
    let truth = |picture: usize| {
        let (x, y) = (40.0 + 5.0 * picture as f64, 40.0 + 1.5 * picture as f64);
        (
            (x.round() + 18.0 - 20.25) / 279.5,
            (y.round() + 18.0 - 10.25) / 159.5,
        )
    };
    let (x, y) = truth(2);
    let mut target = drawn(&fixture, 2);
    target.region = TargetRegion {
        center: [(x * 1e6).round() as u32, (y * 1e6).round() as u32],
        size: [128_801, 225_705],
    }
    .into();
    let saved = save(&fixture, &fixture.workspace, "square", target);
    let workspace = saved.workspace.unwrap();
    let started = targets(
        &fixture.service,
        track(
            &workspace,
            2,
            "square",
            TrackMode::Track {
                through_shots: true,
            },
        ),
    );
    assert_eq!(refusal(&started), None);
    let (outcome, update) = concluded(&fixture.service, 2);
    let Outcome::Saved { samples, .. } = outcome else {
        panic!("expected a saved path from the real worker, got {outcome:?}")
    };
    assert!(samples > 0);
    let tracked = update.workspace.unwrap();
    let target = &tracked.document.targets()[&TargetId::new("square").unwrap()];
    let provenance = target.provenance.clone().unwrap();
    assert_ne!(provenance.engine, SCRIPTED_ENGINE);
    // Before the cut at picture 30, tracked samples stay on the square.
    let mut checked = 0;
    for (picture, pts) in fixture.pictures.iter().enumerate().take(30).skip(3) {
        let Some(sample) = target.samples.iter().find(|sample| sample.at == *pts) else {
            continue;
        };
        if sample.state != TrackState::Tracked {
            continue;
        }
        let (x, y) = truth(picture);
        let [cx, cy] = sample.region.center();
        let error = ((f64::from(cx) / 1e6 - x) * 279.5).hypot((f64::from(cy) / 1e6 - y) * 159.5);
        assert!(error < 8.0, "picture {picture} is {error:.1} px off");
        checked += 1;
    }
    eprintln!(
        "engine {} · {checked} tracked samples checked of {}",
        provenance.engine,
        target.samples.len()
    );
    assert!(
        checked >= 8,
        "only {checked} tracked samples before the cut"
    );
}
