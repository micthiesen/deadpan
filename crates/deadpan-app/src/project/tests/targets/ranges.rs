//! Incremental tracking through the native service: bounded ranges nearest
//! the visible cursor, partial coverage that previews but never saves, and
//! resumption from the private range cache after cancellation or sleep.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::project::targets::{Coverage, Phase};

const WIDTH: usize = 4;

fn ranged(ending: ScriptEnding, before: Option<usize>, counter: &Arc<AtomicUsize>) -> Script {
    Script {
        unavailable: None,
        steps: 1,
        step_interval: Duration::from_millis(1),
        ending,
        range_pictures: Some(WIDTH),
        ranges_before_ending: before,
        tracked_ranges: counter.clone(),
    }
}

fn cached(runs: Vec<Script>, cache: &std::path::Path) -> Fixture {
    project_with(Backend::Scripted(Arc::new(
        ScriptQueue::new(runs).with_cache(cache.to_path_buf()),
    )))
}

fn start(fixture: &Fixture, workspace: &Workspace, ticket: u64) {
    let started = targets(
        &fixture.service,
        track(
            workspace,
            ticket,
            "target-1",
            TrackMode::Track {
                through_shots: true,
            },
        ),
    );
    assert_eq!(refusal(&started), None);
}

fn coverage(fixture: &Fixture, ticket: u64, completed: usize) -> Coverage {
    running(fixture, ticket, completed).coverage.unwrap()
}

fn running(fixture: &Fixture, ticket: u64, completed: usize) -> Job {
    job_until(&fixture.service, |job| {
        job.ticket == ticket
            && job
                .coverage
                .as_ref()
                .is_some_and(|coverage| coverage.completed >= completed)
    })
    .targets
    .unwrap()
    .job
    .unwrap()
}

fn cancel(fixture: &Fixture, workspace: &Workspace, job: u64) {
    let cancelled = targets(
        &fixture.service,
        Operation::Cancel {
            ticket: job + 100,
            session: workspace.session,
            job,
        },
    );
    assert_eq!(refusal(&cancelled), None);
}

fn head_target(workspace: &Workspace) -> AttentionTarget {
    let store = ProjectStore::open(&workspace.path, AccessMode::ReadOnly).unwrap();
    store.snapshot().unwrap().targets()[&TargetId::new("target-1").unwrap()].clone()
}

#[test]
fn partial_ranges_preview_without_saving_and_a_cancelled_run_resumes_from_them() {
    let cache = tempfile::tempdir().unwrap();
    let counter = Arc::new(AtomicUsize::new(0));
    let fixture = cached(
        vec![
            ranged(ScriptEnding::WaitForCancel, Some(2), &counter),
            ranged(ScriptEnding::Moving { step: 0.002 }, None, &counter),
        ],
        &cache.path().join("ranges"),
    );
    let count = fixture.pictures.len();
    assert!(count >= 2 + 3 * WIDTH, "the fixture needs several ranges");
    let total = (count - 2).div_ceil(WIDTH);
    let workspace = save(&fixture, &fixture.workspace, "target-1", drawn(&fixture, 2))
        .workspace
        .unwrap();
    start(&fixture, &workspace, 2);
    let job = running(&fixture, 2, 2);
    let partial = job.coverage.clone().unwrap();
    assert_eq!(
        (partial.completed, partial.total, partial.cached),
        (2, total, 0)
    );
    assert_eq!(
        partial.spans,
        [(fixture.pictures[2], fixture.pictures[2 + 2 * WIDTH])]
    );
    // The completed prefix is usable as a target, ending where it ends.
    let preview = partial.partial.expect("a prefix from the selection");
    assert_eq!(preview.span.end().ticks, fixture.pictures[2 + 2 * WIDTH]);
    // Linear scripted motion compacts to the run's endpoints.
    assert!(!preview.samples.is_empty());
    assert_eq!(preview.provenance.as_ref().unwrap().engine, SCRIPTED_ENGINE);
    assert!(matches!(job.phase, Phase::Tracking(percent) if percent < 100));
    assert!(job.partial(&TargetId::new("target-1").unwrap()).is_some());
    // Nothing is saved while ranges remain.
    assert!(head_target(&workspace).samples.is_empty());

    cancel(&fixture, &workspace, 2);
    let (outcome, update) = concluded(&fixture.service, 2);
    assert_eq!(outcome, Outcome::Cancelled);
    let job = update.targets.unwrap().job.unwrap();
    assert!(
        job.coverage.unwrap().partial.is_none(),
        "an unsaved preview ends with its job"
    );
    assert!(head_target(&workspace).samples.is_empty());
    assert_eq!(counter.load(Ordering::SeqCst), 2);

    // An explicit :track reuses both completed ranges and tracks the rest.
    start(&fixture, &workspace, 3);
    let (outcome, update) = concluded(&fixture.service, 3);
    assert!(matches!(outcome, Outcome::Saved { .. }), "{outcome:?}");
    assert_eq!(counter.load(Ordering::SeqCst), total);
    let job = update.targets.as_ref().unwrap().job.clone().unwrap();
    let final_coverage = job.coverage.unwrap();
    assert_eq!(
        (final_coverage.completed, final_coverage.cached),
        (total, 2)
    );
    let saved = update.workspace.unwrap();
    let target = &saved.document.targets()[&TargetId::new("target-1").unwrap()];
    assert_eq!(target.span.end(), drawn(&fixture, 2).span.end());
    // Each range continued the same motion from the last reported region:
    // the last picture is where one uninterrupted run would put it.
    let last = *fixture.pictures.last().unwrap();
    let (region, _) = target
        .region_at(deadpan_core::SourcePoint {
            ticks: deadpan_core::ExactRatio::integer(last),
            time_base: target.span.start().time_base,
        })
        .unwrap();
    let expected = 400_000.0 + 2_000.0 * (count - 3) as f64;
    let center = f64::from(region.as_region().unwrap().center[0]);
    assert!(
        (center - expected).abs() <= 1_000.0,
        "{center} vs {expected}"
    );
}

#[test]
fn a_correction_seeds_its_own_segment_and_the_cursor_neighbourhood_runs_first() {
    let cache = tempfile::tempdir().unwrap();
    let counter = Arc::new(AtomicUsize::new(0));
    let fixture = cached(
        vec![
            ranged(ScriptEnding::WaitForCancel, Some(1), &counter),
            ranged(ScriptEnding::Moving { step: 0.002 }, None, &counter),
        ],
        &cache.path().join("ranges"),
    );
    let count = fixture.pictures.len();
    let seed = 2 + 3 * WIDTH;
    assert!(count > seed + WIDTH);
    let mut target = drawn(&fixture, 2);
    let correction = TargetRegion {
        center: [600_000, 500_000],
        size: [200_000, 200_000],
    };
    target.corrections.push(deadpan_core::TargetCorrection {
        at: fixture.pictures[seed],
        region: correction.into(),
    });
    let workspace = save(&fixture, &fixture.workspace, "target-1", target)
        .workspace
        .unwrap();
    // The person is looking just after the correction.
    fixture
        .service
        .set_tracking_cursor(fixture.pictures[seed + 1]);
    start(&fixture, &workspace, 2);
    let first = coverage(&fixture, 2, 1);
    assert_eq!(
        first.spans,
        [(fixture.pictures[seed], fixture.pictures[seed + WIDTH])],
        "the cursor's range is tracked first, from the correction"
    );
    assert!(
        first.partial.is_none(),
        "nothing contiguous from the selection yet"
    );
    cancel(&fixture, &workspace, 2);
    assert_eq!(concluded(&fixture.service, 2).0, Outcome::Cancelled);

    start(&fixture, &workspace, 3);
    let (outcome, update) = concluded(&fixture.service, 3);
    assert!(matches!(outcome, Outcome::Saved { .. }), "{outcome:?}");
    let ranges = (seed - 2).div_ceil(WIDTH) + (count - seed).div_ceil(WIDTH);
    assert_eq!(counter.load(Ordering::SeqCst), ranges);
    let saved = update.workspace.unwrap();
    let target = &saved.document.targets()[&TargetId::new("target-1").unwrap()];
    // The correction stays a keyframe; tracking before it never crossed it.
    assert_eq!(target.corrections.len(), 1);
    assert_eq!(target.corrections[0].at, fixture.pictures[seed]);
    let at = |ordinal: usize| {
        target
            .region_at(deadpan_core::SourcePoint {
                ticks: deadpan_core::ExactRatio::integer(fixture.pictures[ordinal]),
                time_base: target.span.start().time_base,
            })
            .unwrap()
    };
    assert_eq!(at(seed).1, TargetSource::Manual);
    assert!(f64::from(at(seed + 2).0.as_region().unwrap().center[0]) > 600_000.0);
    assert!(f64::from(at(seed - 1).0.as_region().unwrap().center[0]) < 500_000.0);
}

#[test]
fn sleep_keeps_completed_ranges_for_the_next_explicit_track() {
    let cache = tempfile::tempdir().unwrap();
    let counter = Arc::new(AtomicUsize::new(0));
    let fixture = cached(
        vec![
            ranged(ScriptEnding::WaitForCancel, Some(1), &counter),
            ranged(ScriptEnding::Moving { step: 0.002 }, None, &counter),
        ],
        &cache.path().join("ranges"),
    );
    let total = (fixture.pictures.len() - 2).div_ceil(WIDTH);
    let workspace = save(&fixture, &fixture.workspace, "target-1", drawn(&fixture, 2))
        .workspace
        .unwrap();
    start(&fixture, &workspace, 2);
    coverage(&fixture, 2, 1);
    let lifecycle = fixture.service.jobs().lifecycle();
    lifecycle.will_sleep();
    lifecycle.did_wake();
    assert_eq!(concluded(&fixture.service, 2).0, Outcome::Interrupted);
    assert!(head_target(&workspace).samples.is_empty());
    start(&fixture, &workspace, 3);
    let (outcome, update) = concluded(&fixture.service, 3);
    assert!(matches!(outcome, Outcome::Saved { .. }), "{outcome:?}");
    assert_eq!(counter.load(Ordering::SeqCst), total);
    let coverage = update.targets.unwrap().job.unwrap().coverage.unwrap();
    assert_eq!(coverage.cached, 1);
}
