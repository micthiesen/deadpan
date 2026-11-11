//! The native tracking lifecycle through the real `deadpan-track` Vision
//! worker: ranged tracking with a partial preview, cancellation that tears the
//! worker down and keeps completed ranges, resumption from them, a manual
//! correction that re-tracks its segment, the saved path against the
//! fixture's truth and Undo of both saves.

use std::time::Instant;

use super::*;
use crate::project::targets::{Coverage, Installed};

/// Pictures per range: the 28 pictures before the cut make twenty-eight ranges.
/// Narrow ranges keep the job running for many worker launches, so the cancel
/// below lands while it runs even when the machine is loaded; with wide ranges
/// the job could finish before the cancel arrived.
const WIDTH: usize = 1;
/// The fixture's cut (a stored shot boundary stops tracking there).
const CUT: usize = 30;
/// The tolerance the app's other real-worker test uses, in coded pixels.
const TOLERANCE: f64 = 8.0;

/// The clean aperture of `moving-square-aperture.mp4` in coded pixels.
const APERTURE: [f64; 4] = [20.25, 10.25, 279.5, 159.5];

/// The fixture moves a 36 px square to (40 + 5n, 40 + 1.5n) coded px until
/// the cut, passing behind a pillar in pictures 20 to 22. Returns the centre
/// normalized to the clean aperture. (`moving-square-cut.mkv` itself cannot
/// be an Original: its millisecond Matroska timestamps have no admissible
/// cadence.)
fn truth(picture: usize) -> (f64, f64) {
    let n = picture as f64;
    (
        ((40.0 + 5.0 * n).round() + 18.0 - APERTURE[0]) / APERTURE[2],
        ((40.0 + 1.5 * n).round() + 18.0 - APERTURE[1]) / APERTURE[3],
    )
}

fn square(picture: usize) -> TargetRegion {
    let (x, y) = truth(picture);
    TargetRegion {
        center: [(x * 1e6).round() as u32, (y * 1e6).round() as u32],
        size: [128_801, 225_705],
    }
}

/// Live `deadpan-track` processes this test process launched.
fn live_workers(worker: &std::path::Path) -> Vec<String> {
    let output = std::process::Command::new("/bin/ps")
        .args(["-A", "-o", "pid=,ppid=,command="])
        .output()
        .unwrap();
    assert!(output.status.success());
    let me = std::process::id().to_string();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| {
            let mut fields = line.split_whitespace();
            let (_pid, ppid) = (fields.next(), fields.next());
            ppid == Some(me.as_str())
                && fields
                    .next()
                    .is_some_and(|command| std::path::Path::new(command) == worker)
        })
        .map(str::to_owned)
        .collect()
}

fn job(update: &ProjectUpdate) -> Job {
    update.targets.as_ref().unwrap().job.clone().unwrap()
}

/// The largest error, in coded pixels, of the tracked samples before the cut,
/// and how many there are.
fn error_before_cut(fixture: &Fixture, target: &AttentionTarget, from: usize) -> (f64, usize) {
    let mut worst = 0.0_f64;
    let mut checked = 0;
    for (picture, pts) in fixture.pictures.iter().enumerate().take(CUT).skip(from) {
        let Some(sample) = target.samples.iter().find(|sample| sample.at == *pts) else {
            continue;
        };
        if sample.state != TrackState::Tracked {
            continue;
        }
        let (x, y) = truth(picture);
        let [cx, cy] = sample.region.center();
        let error = ((f64::from(cx) / 1e6 - x) * APERTURE[2])
            .hypot((f64::from(cy) / 1e6 - y) * APERTURE[3]);
        assert!(error < TOLERANCE, "picture {picture} is {error:.2} px off");
        worst = worst.max(error);
        checked += 1;
    }
    (worst, checked)
}

#[test]
fn the_real_worker_tracks_cancels_resumes_corrects_and_undoes_through_the_app_service() {
    let Some(worker) = installed_tracker() else {
        return;
    };
    let cache = tempfile::tempdir().unwrap();
    let fixture = project_from(
        Backend::Installed(Installed {
            executable: worker.clone(),
            range_pictures: Some(WIDTH),
            cache: Some(cache.path().join("ranges")),
        }),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../native/deadpan-track/tests/fixtures/moving-square-aperture.mp4")
            .canonicalize()
            .unwrap(),
    );
    assert_eq!(fixture.pictures.len(), 48);
    let workspace = store_cut(&fixture, CUT);
    let id = TargetId::new("square").unwrap();
    let mut drawn = drawn(&fixture, 2);
    drawn.region = square(2).into();
    let workspace = save(&fixture, &workspace, "square", drawn.clone())
        .workspace
        .unwrap();
    let total = (CUT - 2).div_ceil(WIDTH);
    let clock = Instant::now();

    // Track until the first range completes, then cancel while a worker runs.
    let started = targets(
        &fixture.service,
        track(
            &workspace,
            2,
            "square",
            TrackMode::Track {
                through_shots: false,
            },
        ),
    );
    assert_eq!(refusal(&started), None);
    let first = job(&job_until(&fixture.service, |job| {
        job.ticket == 2
            && job
                .coverage
                .as_ref()
                .is_some_and(|coverage| coverage.completed >= 1)
    }));
    let partial: Coverage = first.coverage.clone().unwrap();
    assert_eq!(partial.total, total);
    assert_eq!(partial.cached, 0);
    let preview = partial
        .partial
        .clone()
        .expect("a prefix from the selection");
    assert_ne!(
        preview.provenance.as_ref().unwrap().engine,
        SCRIPTED_ENGINE,
        "the preview comes from the real worker"
    );
    let first_range = clock.elapsed();
    // Cancel straight away, with a range worker normally running: polling for
    // one first (a `ps` per try) let the whole job finish under load.
    let running_worker = live_workers(&worker);
    let cancelled = targets(
        &fixture.service,
        Operation::Cancel {
            ticket: 102,
            session: workspace.session,
            job: 2,
        },
    );
    assert_eq!(refusal(&cancelled), None);
    let (outcome, update) = concluded(&fixture.service, 2);
    assert_eq!(outcome, Outcome::Cancelled);
    let kept = job(&update).coverage.unwrap();
    assert!(
        (1..total).contains(&kept.completed),
        "cancelled with {} of {total} ranges complete",
        kept.completed
    );
    assert!(kept.partial.is_none(), "the preview ends with its job");
    assert_eq!(
        live_workers(&worker),
        Vec::<String>::new(),
        "the cancelled worker was reaped"
    );
    let head = |workspace: &Workspace| {
        ProjectStore::open(&workspace.path, AccessMode::ReadOnly)
            .unwrap()
            .snapshot()
            .unwrap()
            .targets()[&id]
            .clone()
    };
    assert_eq!(head(&workspace), drawn, "a cancelled track saves nothing");

    // An explicit :track reuses the completed ranges and saves once.
    let resumed = Instant::now();
    let started = targets(
        &fixture.service,
        track(
            &workspace,
            3,
            "square",
            TrackMode::Track {
                through_shots: false,
            },
        ),
    );
    assert_eq!(refusal(&started), None);
    let (outcome, update) = concluded(&fixture.service, 3);
    assert!(matches!(outcome, Outcome::Saved { .. }), "{outcome:?}");
    let resumed = resumed.elapsed();
    let finished = job(&update).coverage.unwrap();
    assert_eq!(
        (finished.completed, finished.total, finished.cached),
        (total, total, kept.completed)
    );
    let tracked = update.workspace.unwrap();
    let path = tracked.document.targets()[&id].clone();
    assert_ne!(path.provenance.as_ref().unwrap().engine, SCRIPTED_ENGINE);
    let (tracked_error, tracked_samples) = error_before_cut(&fixture, &path, 3);
    assert!(
        tracked_samples >= 8,
        "only {tracked_samples} tracked samples"
    );
    assert_eq!(live_workers(&worker), Vec::<String>::new());

    // A manual correction (`c`) on the truth re-tracks its own segment.
    // A correction re-tracks its own segment across several ranges, including
    // the pillar seam. (The square is still partly behind the pillar until
    // shortly before the cut, so it cannot seed a fair re-acquisition there.)
    let at = 10;
    let correcting = Instant::now();
    let started = targets(
        &fixture.service,
        track(
            &tracked,
            4,
            "square",
            TrackMode::Correct {
                at: fixture.pictures[at],
                region: square(at),
            },
        ),
    );
    assert_eq!(refusal(&started), None);
    let (outcome, update) = concluded(&fixture.service, 4);
    assert!(matches!(outcome, Outcome::Saved { .. }), "{outcome:?}");
    let correcting = correcting.elapsed();
    let corrected_workspace = update.workspace.unwrap();
    let corrected = corrected_workspace.document.targets()[&id].clone();
    assert_eq!(corrected.corrections.len(), 1);
    assert_eq!(corrected.corrections[0].at, fixture.pictures[at]);
    let point = |target: &AttentionTarget, ordinal: usize| {
        target
            .region_at(deadpan_core::SourcePoint {
                ticks: deadpan_core::ExactRatio::integer(fixture.pictures[ordinal]),
                time_base: target.span.start().time_base,
            })
            .unwrap()
    };
    let (manual, source) = point(&corrected, at);
    assert_eq!(source, TargetSource::Manual);
    assert_eq!(manual.center(), square(at).center);
    // Before the correction the saved path is unchanged.
    for ordinal in 2..at {
        assert_eq!(point(&corrected, ordinal), point(&path, ordinal));
    }
    let (corrected_error, corrected_samples) = error_before_cut(&fixture, &corrected, at + 1);
    assert!(
        corrected_samples >= 4,
        "only {corrected_samples} re-tracked"
    );
    assert_eq!(live_workers(&worker), Vec::<String>::new());

    // Undo restores the tracked path, then the untracked rectangle.
    let undo = |workspace: &Workspace| {
        command(
            &fixture.service,
            ProjectRequest::Undo {
                expected_revision: workspace.document.revision_id().clone(),
            },
        )
        .workspace
        .unwrap()
    };
    let undone = undo(&corrected_workspace);
    assert_eq!(undone.document.targets()[&id], path);
    let undone = undo(&undone);
    assert_eq!(undone.document.targets()[&id], drawn);
    assert_eq!(head(&undone), drawn);
    eprintln!(
        "engine {} · first range {first_range:?} · cancelled with {}/{total} ranges (worker running at cancel: {}) · resumed in {resumed:?} · correction in {correcting:?} · max error {tracked_error:.2} px over {tracked_samples} samples, {corrected_error:.2} px over {corrected_samples} after the correction",
        path.provenance.as_ref().unwrap().engine,
        kept.completed,
        !running_worker.is_empty(),
    );
}
