//! Actual private CLI worker against the same committed projects and Metal
//! renderer as the direct path. Numerical reference planes remain independent
//! of the worker; exact direct comparison additionally checks process transport.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use deadpan_cli::export_picture::OutputFrameOrdinal;
use deadpan_cli::picture::ProjectPictureSession;
use deadpan_cli::render_worker::{
    RenderPictureRequest, RenderProgress, RenderWorkerError, RenderWorkerLimits,
    RenderWorkerRuntime, prepare, protocol::RenderIdentity,
};
use deadpan_core::{
    Command, ExactRatio, FrameRange, Framing, FramingPose, ProjectFrame, RevisionId,
};
use deadpan_jobs::{AttemptId, CancellationToken, RequestId};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{
    GeneratedFixture, Gpu, Result, check, check_deadline, edit, node, reference, revision,
};

#[derive(Clone, Copy)]
enum Reference {
    None,
    Odd,
    Generated,
}

struct Case<'a> {
    label: &'static str,
    package: &'a Path,
    revision: RevisionId,
    range: Option<FrameRange>,
    reference: Reference,
}

fn request(case: &Case<'_>, suffix: &str) -> Result<RenderPictureRequest> {
    let name = format!("qualification-{}-{suffix}", case.label);
    Ok(RenderPictureRequest {
        package: case.package.to_path_buf(),
        revision: case.revision.clone(),
        range: case.range,
        identity: RenderIdentity {
            request_id: RequestId::new(name.clone())?,
            attempt_id: AttemptId::new(format!("{name}-attempt"))?,
        },
        cancellation_token: CancellationToken::new(format!("{name}-cancel"))?,
    })
}

fn sha(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut hex, "{byte:02x}").expect("writing to a String cannot fail");
    }
    hex
}

fn progress_value(progress: RenderProgress, started: Instant) -> Value {
    json!({"completed_frames": progress.completed_frames, "total_frames": progress.total_frames,
        "elapsed_seconds": started.elapsed().as_secs_f64()})
}

/// Call after the direct qualification has written its reference files and
/// released its writable store. `executable` is an explicit built CLI artifact.
pub(super) fn qualify(
    gpu: &Gpu,
    executable: &Path,
    directory: &Path,
    generated: Option<&Path>,
    report: &mut Value,
) -> Result<()> {
    check_deadline(gpu.deadline)?;
    let executable = executable.canonicalize()?;
    let mut environment = BTreeMap::<OsString, OsString>::new();
    // The actual executable may use the parent's explicitly selected FFmpeg
    // development libraries. Do not inherit unrelated user environment values.
    for name in [
        "DYLD_LIBRARY_PATH",
        "DYLD_FALLBACK_LIBRARY_PATH",
        "LD_LIBRARY_PATH",
    ] {
        if let Some(value) = std::env::var_os(name) {
            environment.insert(name.into(), value);
        }
    }
    let environment_keys: Vec<_> = environment
        .keys()
        .map(|key| key.to_string_lossy())
        .collect();
    report["worker"] = json!({"status": "running", "runtime": executable,
        "runtime_environment_keys": environment_keys, "checks": [], "cases": [],
        "scope": "actual private CLI worker, clean teardown, owned raw I420 admission and complete-frame comparison",
        "limitations": ["synthetic SDR Original and retained Generated fixtures", "no encoded media, audio or publication",
            "no OS sandbox or performance qualification", "this native run exercises cancellation after progress; it does not measure hard-timeout latency"]});
    let report = &mut report["worker"];
    let runtime = RenderWorkerRuntime {
        executable,
        arguments: Vec::new(),
        environment,
    };
    let original = directory.join("picture.deadpan");
    let odd = directory.join("odd-canvas.deadpan");
    let mut writer = ProjectStore::open(&original, AccessMode::ReadWrite)?;
    let before = writer.snapshot_at(&revision("background"))?;
    let starting_head = writer.snapshot()?.revision_id().clone();
    let mut mutation: Option<Result<Value>> = None;
    run_case(
        gpu,
        &runtime,
        Case {
            label: "original",
            package: &original,
            revision: revision("background"),
            range: Some(FrameRange::new(ProjectFrame(20), ProjectFrame(63))?),
            reference: Reference::None,
        },
        directory,
        report,
        |progress| {
            if mutation.is_none() {
                // This controlled fixture performs its tiny transactions in
                // the progress callback so the host cannot admit completion
                // before all three live history changes have been exercised.
                // It is not a performance or nonblocking callback measurement.
                mutation = Some(mutate(&mut writer, progress));
            }
        },
    )?;
    let mutation = mutation.ok_or("real worker emitted no progress before completion")??;
    check(
        report,
        "live history changes start after nonterminal worker progress",
        mutation["trigger_completed_frames"]
            .as_u64()
            .zip(mutation["trigger_total_frames"].as_u64())
            .is_some_and(|(completed, total)| completed > 0 && completed < total),
        mutation.clone(),
    )?;
    report["live_history"] = mutation;
    check(
        report,
        "actual worker retains the captured document across live edit, undo and redo",
        writer.snapshot_at(&revision("background"))? == before
            && writer.snapshot()?.revision_id() == &revision("worker-live-redo")
            && starting_head != revision("worker-live-redo"),
        json!({"captured_revision": "background", "starting_head": starting_head,
            "head_after": writer.snapshot()?.revision_id()}),
    )?;
    let mut current = gpu.session(ProjectPictureSession::open_revision(
        &original,
        &revision("worker-live-redo"),
        Some(FrameRange::new(ProjectFrame(20), ProjectFrame(21))?),
        &AtomicBool::new(false),
    )?)?;
    let (current_bytes, _) = gpu.frame(&mut current, 20)?;
    let captured_first =
        read_first_frame(&directory.join("worker-original.i420"), current_bytes.len())?;
    check(
        report,
        "live revision actually changes the source picture while captured worker bytes stay fixed",
        current_bytes != captured_first,
        json!({"project_frame": 20, "live_revision": "worker-live-redo"}),
    )?;
    drop(current);
    drop(writer);

    run_case(
        gpu,
        &runtime,
        Case {
            label: "captured-freeze-and-background",
            package: &original,
            revision: revision("background"),
            range: Some(FrameRange::new(ProjectFrame(121), ProjectFrame(128))?),
            reference: Reference::None,
        },
        directory,
        report,
        |_| {},
    )?;
    run_case(
        gpu,
        &runtime,
        Case {
            label: "odd",
            package: &odd,
            revision: revision("background"),
            range: Some(FrameRange::new(ProjectFrame(20), ProjectFrame(21))?),
            reference: Reference::Odd,
        },
        directory,
        report,
        |_| {},
    )?;
    if let Some(package) = generated {
        let fixture = GeneratedFixture::read(package)?;
        let store = ProjectStore::open(package, AccessMode::ReadOnly)?;
        let before = store.snapshot()?;
        run_case(
            gpu,
            &runtime,
            Case {
                label: "generated",
                package,
                revision: fixture.revision_id.clone(),
                range: None,
                reference: Reference::Generated,
            },
            directory,
            report,
            |_| {},
        )?;
        check(
            report,
            "actual Generated worker preserves the accepted snapshot and history",
            store.snapshot()? == before,
            json!({"revision": fixture.revision_id}),
        )?;
        cancellation(
            gpu,
            &runtime,
            Case {
                label: "generated-cancellation",
                package,
                revision: fixture.revision_id,
                range: None,
                reference: Reference::None,
            },
            report,
        )?;
    } else {
        report["generated"] =
            json!({"status": "skipped", "reason": "accepted Generated fixture was not supplied"});
        cancellation(
            gpu,
            &runtime,
            Case {
                label: "original-cancellation",
                package: &original,
                revision: revision("background"),
                range: None,
                reference: Reference::None,
            },
            report,
        )?;
    }
    run_case(
        gpu,
        &runtime,
        Case {
            label: "after-cancellation",
            package: &original,
            revision: revision("background"),
            range: Some(FrameRange::new(ProjectFrame(20), ProjectFrame(21))?),
            reference: Reference::None,
        },
        directory,
        report,
        |_| {},
    )?;
    report["status"] = json!("passed");
    check_deadline(gpu.deadline)?;
    Ok(())
}

fn mutate(writer: &mut ProjectStore, progress: RenderProgress) -> Result<Value> {
    let head = writer.snapshot()?.revision_id().clone();
    edit(
        writer,
        "worker-live-change",
        Command::SetFraming {
            node: node("source"),
            framing: Some(Framing::static_pose(FramingPose::new(
                ExactRatio::new(1, 3)?,
                ExactRatio::new(2, 3)?,
                ExactRatio::new(3, 2)?,
            )?)?),
        },
    )?;
    writer.undo(
        &revision("worker-live-change"),
        revision("worker-live-undo"),
    )?;
    writer.redo(&revision("worker-live-undo"), revision("worker-live-redo"))?;
    Ok(
        json!({"starting_head": head, "edit_revision": "worker-live-change", "undo_revision": "worker-live-undo",
        "redo_revision": "worker-live-redo", "trigger_completed_frames": progress.completed_frames,
        "trigger_total_frames": progress.total_frames,
        "ordering": "transactions ran in a real progress callback before host completion admission"}),
    )
}

fn read_first_frame(path: &Path, length: usize) -> Result<Vec<u8>> {
    use std::io::Read as _;
    let mut output = vec![0; length];
    fs::File::open(path)?.read_exact(&mut output)?;
    Ok(output)
}

fn run_case(
    gpu: &Gpu,
    runtime: &RenderWorkerRuntime,
    case: Case<'_>,
    directory: &Path,
    report: &mut Value,
    mut on_progress: impl FnMut(RenderProgress),
) -> Result<()> {
    check_deadline(gpu.deadline)?;
    let index = report["cases"]
        .as_array()
        .ok_or("missing worker cases")?
        .len();
    report["cases"]
        .as_array_mut()
        .ok_or("missing worker cases")?
        .push(json!({
        "label": case.label, "status": "running", "package": case.package,
        "captured_revision": case.revision, "range": case.range, "frames": [], "progress": []}));
    let started = Instant::now();
    let mut progress = Vec::new();
    let result = prepare(
        runtime,
        request(&case, "complete")?,
        RenderWorkerLimits::default(),
        &AtomicBool::new(false),
        gpu.deadline,
        |update| {
            progress.push(progress_value(update, started));
            on_progress(update);
        },
    );
    report["cases"][index]["progress"] = json!(progress);
    let mut prepared = result?;
    report["cases"][index]["contract"] = json!(prepared.contract());
    report["cases"][index]["manifest"] = json!(prepared.manifest());
    report["cases"][index]["document_sha256"] = json!(prepared.document_sha256());
    report["cases"][index]["prepare_seconds"] = json!(started.elapsed().as_secs_f64());
    let mut direct = gpu.session(ProjectPictureSession::open_revision(
        case.package,
        &case.revision,
        case.range,
        &AtomicBool::new(false),
    )?)?;
    check(
        report,
        "child and direct path capture the identical immutable output contract",
        prepared.contract() == direct.contract(),
        json!({"case": case.label, "contract": prepared.contract()}),
    )?;
    let output_path = directory.join(format!("worker-{}.i420", case.label));
    let direct_path = directory.join(format!("worker-direct-{}.i420", case.label));
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output_path)?;
    let mut direct_output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&direct_path)?;
    let mut child_hash = Sha256::new();
    let mut direct_hash = Sha256::new();
    let mut child = vec![0; usize::try_from(prepared.frame_bytes())?];
    let contract = prepared.contract().clone();
    for ordinal in 0..contract.frame_count() {
        check_deadline(gpu.deadline)?;
        let timing = prepared.read_frame(OutputFrameOrdinal(ordinal), &mut child)?;
        let (direct_bytes, direct_metadata) = gpu.frame(&mut direct, timing.project_frame().0)?;
        output.write_all(&child)?;
        direct_output.write_all(&direct_bytes)?;
        child_hash.update(&child);
        direct_hash.update(&direct_bytes);
        let child_digest = sha(&Sha256::digest(&child));
        let direct_digest = sha(&Sha256::digest(&direct_bytes));
        report["cases"][index]["frames"]
            .as_array_mut()
            .ok_or("missing worker frame list")?
            .push(json!({
            "output_timing": timing, "sha256": child_digest, "direct_sha256": direct_digest,
            "direct_metadata": direct_metadata, "byte_length": child.len()}));
        check(
            report,
            "every isolated frame and exact clock match the direct production path",
            child == direct_bytes && timing == contract.timing(OutputFrameOrdinal(ordinal))?,
            json!({"case": case.label, "ordinal": ordinal, "project_frame": timing.project_frame(),
                "pts": timing.pts(), "duration": timing.duration(), "sha256": child_digest}),
        )?;
        let reference_path = match case.reference {
            Reference::None => None,
            Reference::Odd => Some(directory.join("odd-reference-20.i420")),
            Reference::Generated => Some(directory.join(format!(
                "generated-reference-{}.i420",
                timing.project_frame().0
            ))),
        };
        if let Some(path) = reference_path {
            let expected = fs::read(&path)?;
            let (passed, mut comparison) =
                reference::compare(&child, &expected, contract.raster())?;
            comparison["case"] = json!(case.label);
            comparison["ordinal"] = json!(ordinal);
            comparison["reference_path"] = json!(path);
            comparison["reference_sha256"] = json!(sha(&Sha256::digest(&expected)));
            check(
                report,
                "isolated complete I420 planes match the independent numerical reference",
                passed,
                comparison,
            )?;
        }
    }
    output.sync_all()?;
    direct_output.sync_all()?;
    let child_hash = sha(&child_hash.finalize());
    let direct_hash = sha(&direct_hash.finalize());
    check(
        report,
        "owned worker bytes retain the admitted manifest hash after private workspace cleanup",
        child_hash == prepared.manifest().planes.sha256().as_str()
            && child_hash == direct_hash
            && fs::metadata(&output_path)?.len() == prepared.manifest().planes.byte_length(),
        json!({"case": case.label, "sha256": child_hash, "direct_sha256": direct_hash,
            "byte_length": prepared.manifest().planes.byte_length()}),
    )?;
    report["cases"][index]["path"] = json!(output_path);
    report["cases"][index]["direct_path"] = json!(direct_path);
    report["cases"][index]["sha256"] = json!(child_hash);
    report["cases"][index]["status"] = json!("passed");
    Ok(())
}

fn cancellation(
    gpu: &Gpu,
    runtime: &RenderWorkerRuntime,
    case: Case<'_>,
    report: &mut Value,
) -> Result<()> {
    let cancelled = AtomicBool::new(false);
    let started = Instant::now();
    let mut updates = Vec::new();
    let result = prepare(
        runtime,
        request(&case, "cancel")?,
        RenderWorkerLimits::default(),
        &cancelled,
        gpu.deadline,
        |update| {
            updates.push(progress_value(update, started));
            cancelled.store(true, Ordering::Release);
        },
    );
    let interrupted = matches!(result, Err(RenderWorkerError::Cancelled));
    let error = result.err().map(|error| error.to_string());
    report["cancellation"] = json!({"case": case.label, "progress": updates,
        "elapsed_seconds": started.elapsed().as_secs_f64(), "error": error,
        "ordering": "cancellation requested after nonterminal progress from the actual CLI worker; no artifact admitted"});
    let actual = report["cancellation"].clone();
    check(
        report,
        "real render cancellation after progress returns no prepared range",
        interrupted
            && updates.first().is_some_and(|update| {
                update["completed_frames"]
                    .as_u64()
                    .zip(update["total_frames"].as_u64())
                    .is_some_and(|(completed, total)| completed > 0 && completed < total)
            }),
        actual,
    )?;
    check_deadline(gpu.deadline)?;
    Ok(())
}
