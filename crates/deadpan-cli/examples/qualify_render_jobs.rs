//! Native durable-render qualification over two separately prepared scratch projects.
//! Usage: qualify_render_jobs SOURCE_PACKAGE SOURCE_REVISION GENERATED_PACKAGE
//!        GENERATED_REVISION WORKER REPORT NEW_OUTPUT_DIRECTORY
//! The caller copies/migrates the fixtures first. This harness intentionally edits
//! the source fixture and allocates operational jobs in both supplied packages.

use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use deadpan_cli::{
    encoded_render::{
        EncodedProgress, EncodedWorkerLimits,
        jobs::{self, CaptureRenderIntent, RenderStageRequest},
        publication::{PublicationOutcome, publish},
        verification::VerificationLimits,
    },
    render_worker::RenderWorkerRuntime,
};
use deadpan_core::{FrameRange, ProjectFrame, RevisionId};
use deadpan_jobs::{
    AttemptId, CancellationToken, RequestId,
    render::{
        RenderAttemptState, RenderBFrames, RenderEncoder, RenderEngineeringPolicy, RenderSelection,
    },
};
use deadpan_store::{
    AccessMode, ProjectStore,
    render_jobs::{BeginRenderAttempt, RenderAttemptTransition},
    render_media::RenderMediaLimits,
};
use serde_json::{Value, json};

#[path = "qualify_render_jobs/evidence.rs"]
mod evidence;
use evidence::{authored, check, coverage, mutate, save};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
const MAX_PROGRESS: usize = 4096;

struct Case {
    name: &'static str,
    package: PathBuf,
    revision: RevisionId,
    range: FrameRange,
    generated: bool,
}

fn main() -> Result {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 7 {
        return Err("expected SOURCE_PACKAGE SOURCE_REVISION GENERATED_PACKAGE GENERATED_REVISION WORKER REPORT NEW_OUTPUT_DIRECTORY".into());
    }
    let cases = [
        Case {
            name: "structural",
            package: fs::canonicalize(&arguments[0])?,
            revision: RevisionId::new(arguments[1].to_str().ok_or("non-UTF8 source revision")?)?,
            range: FrameRange::new(ProjectFrame(20), ProjectFrame(128))?,
            generated: false,
        },
        Case {
            name: "generated",
            package: fs::canonicalize(&arguments[2])?,
            revision: RevisionId::new(arguments[3].to_str().ok_or("non-UTF8 generated revision")?)?,
            range: FrameRange::new(ProjectFrame(0), ProjectFrame(30))?,
            generated: true,
        },
    ];
    let scratch = fs::canonicalize("/tmp")?;
    if cases[0].package == cases[1].package
        || cases.iter().any(|case| !case.package.starts_with(&scratch))
    {
        return Err("qualification requires two distinct scratch packages under /tmp".into());
    }
    let runtime = RenderWorkerRuntime {
        executable: fs::canonicalize(&arguments[4])?,
        arguments: Vec::new(),
        environment: BTreeMap::new(),
    };
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&arguments[5])?;
    let directory = PathBuf::from(&arguments[6]);
    fs::create_dir(&directory)?;
    let directory = directory.canonicalize()?;
    let started = Instant::now();
    let deadline = started + Duration::from_secs(15 * 60);
    let mut report = json!({
        "schema_version": 1, "status": "running", "worker": runtime.executable,
        "scope": "two fresh native encodes, durable movie/manifest checkpoints, writer restart, fresh isolated verification and destination publication",
        "limitations": ["explicit engineering hardware/no-B-frame policy", "restart is an orderly writer close, not an injected process crash", "no durable publication journal or publication crash reconciliation", "independent content comparison remains a separate check", "no native Render UI, automatic encoder selection, full audio/effects, HDR or release qualification"],
        "cases": []
    });
    save(&mut output, &report)?;
    for case in cases {
        let mut result =
            json!({"name": case.name, "package": case.package, "status": "running", "checks": []});
        let outcome = run_case(&runtime, &case, &directory, deadline, &mut result);
        result["status"] = json!(if outcome.is_ok() { "passed" } else { "failed" });
        if let Err(error) = &outcome {
            result["error"] = json!(error.to_string());
        }
        report["cases"]
            .as_array_mut()
            .ok_or("missing cases")?
            .push(result);
        report["elapsed_seconds"] = json!(started.elapsed().as_secs_f64());
        report["status"] = json!(if outcome.is_ok() { "running" } else { "failed" });
        save(&mut output, &report)?;
        outcome?;
    }
    report["status"] = json!("passed");
    report["encoded_candidates"] = json!(2);
    report["fresh_verifications"] = json!(2);
    report["published_movies"] = json!(2);
    report["encoded_picture_frames"] = json!(138);
    save(&mut output, &report)
}

fn run_case(
    runtime: &RenderWorkerRuntime,
    case: &Case,
    directory: &Path,
    deadline: Instant,
    report: &mut Value,
) -> Result {
    let cancelled = AtomicBool::new(false);
    let started = Instant::now();
    let mut store = ProjectStore::open(&case.package, AccessMode::ReadWrite)?;
    let captured_document = store.snapshot_at(&case.revision)?;
    let original_head = store.snapshot()?;
    let before = authored(&case.package)?;
    let intent = jobs::capture_intent(
        CaptureRenderIntent {
            package: case.package.clone(),
            revision: case.revision.clone(),
            range: Some(case.range),
            job_id: RequestId::new(format!("durable-{}", case.name))?,
            policy: RenderEngineeringPolicy {
                schema_version: 1,
                selection: RenderSelection::ExplicitEngineering,
                encoder: RenderEncoder::Hardware,
                b_frames: RenderBFrames::None,
            }
            .into(),
        },
        &cancelled,
        deadline,
    )?;
    report["coverage"] = coverage(&captured_document, case.range)?;
    let expected_coverage = if case.generated {
        report["coverage"]["generated"] == 30 && report["coverage"]["original"] == 0
    } else {
        report["coverage"]["original"]
            .as_u64()
            .is_some_and(|count| count > 0)
            && report["coverage"]["background"]
                .as_u64()
                .is_some_and(|count| count > 0)
    };
    check(
        report,
        "representative picture coverage",
        expected_coverage,
        json!({"range": case.range}),
    )?;
    store.create_render_job(intent.clone(), &cancelled, deadline)?;
    let queued = store.begin_render_attempt(BeginRenderAttempt {
        job_id: intent.job_id.clone(),
        attempt_id: AttemptId::new(format!("{}-encode-1", case.name))?,
        cancellation_token: CancellationToken::new(format!("{}-encode-cancel-1", case.name))?,
        checkpoint_attempt_id: None,
    })?;
    let encoding =
        store.transition_render_attempt(&queued.identity(), RenderAttemptTransition::Encoding)?;
    check(
        report,
        "job allocation preserves authored rows and history",
        authored(&case.package)? == before,
        json!(before),
    )?;
    report["intent"] = json!(intent);
    report["encoding_attempt"] = json!(encoding);
    let handle = store.render_write_handle()?;
    let media_limits = RenderMediaLimits::new(
        512 * 1024 * 1024,
        256 * 1024,
        512 * 1024 * 1024 + 256 * 1024,
        1024 * 1024 * 1024,
        128,
    )?;
    let mut encode_limits = EncodedWorkerLimits::default();
    encode_limits.encode.maximum_output_bytes = 512 * 1024 * 1024;
    let mut progress = Vec::new();
    let mut mutation = None;
    let prepared = jobs::encode_and_retain(
        runtime,
        &RenderStageRequest {
            package: case.package.clone(),
            intent: intent.clone(),
            attempt: encoding.clone(),
        },
        &handle,
        (encode_limits, media_limits),
        &cancelled,
        deadline,
        |update| {
            if progress.len() == MAX_PROGRESS {
                cancelled.store(true, Ordering::Release);
                return;
            }
            progress.push(progress_value(update, started));
            if !case.generated
                && mutation.is_none()
                && update.completed_frames > 0
                && update.completed_frames < update.total_frames
            {
                let result = mutate(&mut store, update, "durable");
                if result.is_err() {
                    cancelled.store(true, Ordering::Release);
                }
                mutation = Some(result);
            }
        },
    );
    report["encoding_progress"] = json!(progress);
    let prepared = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            report["encoding_error"] = json!(error.to_string());
            if let Some(Err(mutation_error)) = &mutation {
                report["live_edit_error"] = json!(mutation_error.to_string());
            }
            return Err(error.into());
        }
    };
    if !case.generated {
        let mutation =
            mutation.ok_or("encoding produced no interior progress for the live edit")??;
        report["live_edit"] = mutation.clone();
        check(
            report,
            "live edit, undo, redo and final undo restore authored content",
            evidence::same_content(&original_head, &store.snapshot()?)?,
            mutation,
        )?;
    }
    report["encoded_candidates"] = json!(1);
    let live_document = store.snapshot()?;
    let after_edit = authored(&case.package)?;
    let retained =
        store.retain_render_checkpoint(&encoding.identity(), &prepared, &cancelled, deadline)?;
    let checkpoint = store.render_checkpoint(&intent.job_id, &encoding.attempt_id)?;
    report["retained_attempt"] = json!(retained);
    report["checkpoint"] = json!(checkpoint);
    check(
        report,
        "checkpoint preserves historical capture and current authored rows",
        store.snapshot_at(&case.revision)? == captured_document
            && authored(&case.package)? == after_edit,
        json!({"captured_revision": case.revision, "live_revision": live_document.revision_id(), "authoring": after_edit}),
    )?;
    drop(prepared);
    drop(store);
    let revoked = handle
        .check_live(&cancelled)
        .err()
        .ok_or("closed writer handle remained live")?;
    report["closed_writer_handle_error"] =
        json!({"code": revoked.code(), "message": revoked.to_string()});
    drop(handle);
    let readonly = ProjectStore::open(&case.package, AccessMode::ReadOnly)?;
    check(
        report,
        "read-only reopen does not recover an active attempt",
        readonly.render_attempt(&intent.job_id, &encoding.attempt_id)? == retained,
        json!({"state": retained.state}),
    )?;
    drop(readonly);
    let mut store = ProjectStore::open(&case.package, AccessMode::ReadWrite)?;
    let interrupted = store.render_attempt(&intent.job_id, &encoding.attempt_id)?;
    check(
        report,
        "writer reopen interrupts the attempt and preserves its checkpoint",
        interrupted.state == RenderAttemptState::Interrupted
            && store.render_checkpoint(&intent.job_id, &encoding.attempt_id)? == checkpoint
            && store.render_job(&intent.job_id)? == intent,
        json!(interrupted),
    )?;
    report["interrupted_attempt"] = json!(interrupted);
    let stale = store
        .transition_render_attempt(&retained.identity(), RenderAttemptTransition::Verifying)
        .err()
        .ok_or("pre-restart transition remained valid")?;
    report["stale_transition_error"] = json!(stale.to_string());
    let queued = store.begin_render_attempt(BeginRenderAttempt {
        job_id: intent.job_id.clone(),
        attempt_id: AttemptId::new(format!("{}-verify-2", case.name))?,
        cancellation_token: CancellationToken::new(format!("{}-verify-cancel-2", case.name))?,
        checkpoint_attempt_id: Some(encoding.attempt_id.clone()),
    })?;
    let verifying =
        store.transition_render_attempt(&queued.identity(), RenderAttemptTransition::Verifying)?;
    let reader = store.render_read_handle();
    let mut verification_progress = Vec::new();
    let verified = jobs::verify_checkpoint(
        runtime,
        &RenderStageRequest {
            package: case.package.clone(),
            intent: intent.clone(),
            attempt: verifying.clone(),
        },
        (&checkpoint, &reader),
        (VerificationLimits::default(), media_limits),
        &cancelled,
        deadline,
        |update| {
            if verification_progress.len() == MAX_PROGRESS {
                cancelled.store(true, Ordering::Release);
            } else {
                verification_progress.push(update);
            }
        },
    );
    report["verification_progress"] = json!(verification_progress);
    let verified = verified?;
    report["fresh_verifications"] = json!(1);
    report["manifest"] = json!(verified.candidate().manifest());
    report["contract"] = json!(verified.candidate().contract());
    report["verification"] = json!(verified.report());
    let observation = jobs::verification_observation(&verified, &cancelled, deadline)?;
    let terminal = store.record_render_verification(&verifying.identity(), observation)?;
    report["verified_attempt"] = json!(terminal);
    let attempts = store.render_attempts(&intent.job_id, 0, 3)?;
    check(
        report,
        "the job has exactly one encoding attempt and one verification retry",
        attempts.len() == 2
            && attempts.first() == Some(&interrupted)
            && attempts.get(1) == Some(&terminal),
        json!({"attempt_count": attempts.len()}),
    )?;
    check(
        report,
        "fresh verification uses the retained movie without another encode",
        terminal.state == RenderAttemptState::Verified
            && terminal.ordinal == 2
            && terminal.checkpoint_attempt_id.as_ref() == Some(&encoding.attempt_id)
            && verified.report().movie_sha256 == *checkpoint.media.movie_sha256()
            && verified.report().movie_bytes == checkpoint.media.movie().byte_length()
            && verified.report().video_frames == u64::try_from(case.range.duration().frames())?,
        json!({"encoding_attempt": encoding.attempt_id, "verification_attempt": terminal.attempt_id, "movie_sha256": verified.report().movie_sha256}),
    )?;
    let mut publication_progress = Vec::new();
    let publication = publish(
        verified,
        &case.package,
        &directory.join(format!("published-{}.mp4", case.name)),
        &cancelled,
        deadline,
        |stage| publication_progress.push(stage),
    );
    report["publication_progress"] = json!(publication_progress);
    match publication {
        Ok(PublicationOutcome::Published(receipt)) => {
            check(
                report,
                "published movie has the expected Generated coverage",
                receipt.contains_generated_pictures == case.generated,
                json!({"contains_generated_pictures": receipt.contains_generated_pictures}),
            )?;
            report["path"] = json!(receipt.movie);
            report["publication_status"] = json!("published");
            report["publication"] = json!(receipt);
        }
        Ok(PublicationOutcome::PublishedUnconfirmed {
            receipt,
            diagnostic,
        }) => {
            report["publication_status"] = json!("published_unconfirmed");
            report["publication"] = json!(receipt);
            report["publication_error"] = json!(diagnostic);
            return Err(diagnostic.into());
        }
        Err(failure) => {
            report["publication_status"] = json!("failed_before_movie_rename");
            report["publication_error"] = json!(failure.error);
            report["publication_retained"] = json!(failure.retained);
            return Err(failure.into());
        }
    }
    check(
        report,
        "recovery, verification and publication preserve exact authoring state",
        store.snapshot()? == live_document
            && store.snapshot_at(&case.revision)? == captured_document
            && authored(&case.package)? == after_edit,
        json!(after_edit),
    )?;
    store.validate()?;
    drop(store);
    let reopened = ProjectStore::open(&case.package, AccessMode::ReadWrite)?;
    check(
        report,
        "terminal observation survives reopen without changing history",
        reopened.render_attempt(&intent.job_id, &terminal.attempt_id)? == terminal
            && authored(&case.package)? == after_edit
            && reopened.snapshot_at(&case.revision)? == captured_document,
        json!({"state": terminal.state, "checkpoint": checkpoint.media}),
    )?;
    report["elapsed_seconds"] = json!(started.elapsed().as_secs_f64());
    Ok(())
}

fn progress_value(update: EncodedProgress, started: Instant) -> Value {
    json!({"completed_frames": update.completed_frames, "total_frames": update.total_frames,
        "completed_audio_samples": update.completed_audio_samples, "total_audio_samples": update.total_audio_samples,
        "elapsed_seconds": started.elapsed().as_secs_f64()})
}
