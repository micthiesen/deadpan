//! Actual encoded child, retained direct inputs, and immutable history checks.
//! The Python runner independently decodes these files before qualification.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use deadpan_cli::audio::OfflineAudioSession;
use deadpan_cli::encoded_render::{
    EncodedProgress, EncodedRenderError, EncodedWorkerLimits, encode, protocol::EncoderChoice,
};
use deadpan_cli::picture::ProjectPictureSession;
use deadpan_cli::render_worker::{
    RenderPictureRequest, RenderWorkerRuntime, protocol::RenderIdentity,
};
use deadpan_core::{
    AudioSample, Command, ExactRatio, FrameRange, Framing, FramingPose, ProjectDocument,
    ProjectFrame, ProjectId, RevisionId,
};
use deadpan_encode::{BFramePolicy, EncoderMode};
use deadpan_jobs::{AttemptId, CancellationToken, RequestId};
use deadpan_media::{
    audio_session::{AudioSession, AudioSessionLimits},
    source_index::SourceContentIdentity,
    source_input::VerifiedSourceInput,
    source_qualification::DecodedSourceQualification,
    source_session::{SourceSession, SourceSessionLimits},
};
use deadpan_store::{
    AccessMode, ProjectStore,
    original_media::{OriginalMediaLimits, OriginalOwnership},
    source_registration::{SourceInsertionPurpose, SourceInsertionRequest, SourceRegistration},
};
use serde_json::{Value, json};

use super::{GeneratedFixture, Gpu, Result, asset, check, check_deadline, edit, node, revision};

struct Case<'a> {
    name: &'static str,
    package: &'a Path,
    revision: RevisionId,
    range: Option<FrameRange>,
    choice: EncoderChoice,
}

fn request(case: &Case<'_>, suffix: &str) -> Result<RenderPictureRequest> {
    let name = format!("encoded-{}-{suffix}", case.name);
    Ok(RenderPictureRequest {
        package: case.package.to_owned(),
        revision: case.revision.clone(),
        range: case.range,
        identity: RenderIdentity {
            request_id: RequestId::new(name.clone())?,
            attempt_id: AttemptId::new(format!("{name}-attempt"))?,
        },
        cancellation_token: CancellationToken::new(format!("{name}-cancel"))?,
    })
}

fn progress_value(update: EncodedProgress, started: Instant) -> Value {
    json!({"completed_frames": update.completed_frames, "total_frames": update.total_frames,
        "completed_audio_samples": update.completed_audio_samples, "total_audio_samples": update.total_audio_samples,
        "elapsed_seconds": started.elapsed().as_secs_f64()})
}

pub(super) fn qualify(
    gpu: &Gpu,
    executable: &Path,
    directory: &Path,
    generated: Option<&Path>,
    marker_source: &Path,
    report: &mut Value,
) -> Result<()> {
    let mut environment = BTreeMap::<OsString, OsString>::new();
    for name in [
        "DYLD_LIBRARY_PATH",
        "DYLD_FALLBACK_LIBRARY_PATH",
        "LD_LIBRARY_PATH",
    ] {
        if let Some(value) = std::env::var_os(name) {
            environment.insert(name.into(), value);
        }
    }
    let runtime = RenderWorkerRuntime {
        executable: executable.canonicalize()?,
        arguments: Vec::new(),
        environment,
    };
    report["encoded"] = json!({"status": "running", "checks": [], "cases": [],
        "scope": "real isolated committed picture and canonical PCM encoding; private candidate only",
        "limitations": ["independent decode follows in Python", "no production verifier or publication",
            "no durable render jobs, native Render, full audio effects, HDR or release qualification"]});
    let report = &mut report["encoded"];
    let hardware = EncoderChoice {
        mode: EncoderMode::Hardware,
        b_frames: BFramePolicy::None,
    };
    let software = EncoderChoice {
        mode: EncoderMode::Software,
        b_frames: BFramePolicy::TargetTwo,
    };
    let project = directory.join("picture.deadpan");
    let odd = directory.join("odd-canvas.deadpan");
    let marker = directory.join("marker.deadpan");
    make_marker_project(&marker, marker_source, report)?;
    let mut writer = ProjectStore::open(&project, AccessMode::ReadWrite)?;
    let before = writer.snapshot_at(&revision("background"))?;
    let mut mutation = None;
    run_case(
        gpu,
        &runtime,
        &Case {
            name: "structural",
            package: &project,
            revision: revision("background"),
            range: Some(FrameRange::new(ProjectFrame(20), ProjectFrame(128))?),
            choice: hardware,
        },
        directory,
        report,
        |update| {
            if mutation.is_none() {
                mutation = Some(mutate(&mut writer, update));
            }
        },
    )?;
    let mutation = mutation.ok_or("encoded worker emitted no progress")??;
    check(
        report,
        "encoded capture survives live edit, undo and redo",
        writer.snapshot_at(&revision("background"))? == before
            && writer.snapshot()?.revision_id() == &revision("encoded-live-redo")
            && mutation["completed_frames"]
                .as_u64()
                .zip(mutation["total_frames"].as_u64())
                .is_some_and(|(complete, total)| complete > 0 && complete < total),
        mutation,
    )?;
    drop(writer);
    for case in [
        Case {
            name: "nonzero",
            package: &project,
            revision: revision("background"),
            range: Some(FrameRange::new(ProjectFrame(1), ProjectFrame(2))?),
            choice: hardware,
        },
        Case {
            name: "software-two",
            package: &project,
            revision: revision("background"),
            range: Some(FrameRange::new(ProjectFrame(20), ProjectFrame(63))?),
            choice: software,
        },
        Case {
            name: "odd",
            package: &odd,
            revision: revision("background"),
            range: Some(FrameRange::new(ProjectFrame(20), ProjectFrame(21))?),
            choice: hardware,
        },
        Case {
            name: "marker",
            package: &marker,
            revision: revision("marker-registered"),
            range: None,
            choice: hardware,
        },
    ] {
        run_case(gpu, &runtime, &case, directory, report, |_| {})?;
    }
    if let Some(package) = generated {
        let fixture = GeneratedFixture::read(package)?;
        run_case(
            gpu,
            &runtime,
            &Case {
                name: "generated",
                package,
                revision: fixture.revision_id,
                range: None,
                choice: hardware,
            },
            directory,
            report,
            |_| {},
        )?;
    } else {
        return Err("encoded qualification requires the accepted Generated fixture".into());
    }
    let full = Case {
        name: "cancel",
        package: &project,
        revision: revision("background"),
        range: None,
        choice: hardware,
    };
    let cancelled = AtomicBool::new(false);
    let mut updates = Vec::new();
    let started = Instant::now();
    let result = encode(
        &runtime,
        request(&full, "cancel")?,
        hardware,
        EncodedWorkerLimits::default(),
        &cancelled,
        gpu.deadline,
        |update| {
            updates.push(progress_value(update, started));
            cancelled.store(true, Ordering::Release);
        },
    );
    let interrupted = matches!(result, Err(EncodedRenderError::Cancelled));
    let diagnostic = result.err().map(|error| error.to_string());
    check(
        report,
        "real encoded cancellation after progress admits no candidate",
        interrupted
            && updates.first().is_some_and(|update| {
                update["completed_frames"]
                    .as_u64()
                    .zip(update["total_frames"].as_u64())
                    .is_some_and(|(complete, total)| complete > 0 && complete < total)
            }),
        json!({"progress": updates, "error": diagnostic, "elapsed_seconds": started.elapsed().as_secs_f64()}),
    )?;
    let mut limits = EncodedWorkerLimits::default();
    limits.encode.maximum_output_bytes = 1024;
    let result = encode(
        &runtime,
        request(&full, "byte-limit")?,
        hardware,
        limits,
        &AtomicBool::new(false),
        gpu.deadline,
        |_| {},
    );
    let diagnostic = result
        .err()
        .ok_or("byte-limited encoded worker returned a candidate")?
        .to_string();
    check(
        report,
        "native byte exhaustion returns no encoded candidate",
        diagnostic.contains("output_too_large") || diagnostic.contains("packet_limit"),
        json!({"error": diagnostic}),
    )?;
    run_case(
        gpu,
        &runtime,
        &Case {
            name: "after-cancel",
            package: &project,
            revision: revision("background"),
            range: Some(FrameRange::new(ProjectFrame(20), ProjectFrame(21))?),
            choice: hardware,
        },
        directory,
        report,
        |_| {},
    )?;
    report["status"] = json!("passed candidate preparation; independent decode required");
    check_deadline(gpu.deadline)?;
    Ok(())
}

fn mutate(writer: &mut ProjectStore, update: EncodedProgress) -> Result<Value> {
    edit(
        writer,
        "encoded-live-change",
        Command::SetFraming {
            node: node("source"),
            framing: Some(Framing::static_pose(FramingPose::new(
                ExactRatio::new(1, 4)?,
                ExactRatio::new(3, 4)?,
                ExactRatio::new(5, 4)?,
            )?)?),
        },
    )?;
    writer.undo(
        &revision("encoded-live-change"),
        revision("encoded-live-undo"),
    )?;
    writer.redo(
        &revision("encoded-live-undo"),
        revision("encoded-live-redo"),
    )?;
    Ok(
        json!({"completed_frames": update.completed_frames, "total_frames": update.total_frames,
        "head": writer.snapshot()?.revision_id()}),
    )
}

fn run_case(
    gpu: &Gpu,
    runtime: &RenderWorkerRuntime,
    case: &Case<'_>,
    directory: &Path,
    report: &mut Value,
    mut on_progress: impl FnMut(EncodedProgress),
) -> Result<()> {
    check_deadline(gpu.deadline)?;
    let started = Instant::now();
    let cancelled = AtomicBool::new(false);
    let mut progress = Vec::new();
    let mut candidate = encode(
        runtime,
        request(case, "complete")?,
        case.choice,
        EncodedWorkerLimits::default(),
        &cancelled,
        gpu.deadline,
        |update| {
            progress.push(progress_value(update, started));
            on_progress(update);
        },
    )?;
    let path = directory.join(format!("encoded-{}.mp4", case.name));
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    let copied = candidate.copy_to(&mut output, &cancelled, gpu.deadline)?;
    output.sync_all()?;
    check(
        report,
        "retained encoded candidate has the admitted extent",
        copied == candidate.byte_length() && fs::metadata(&path)?.len() == copied,
        json!({"case": case.name, "bytes": copied}),
    )?;
    let contract = candidate.contract().clone();
    let mut pictures = gpu.session(ProjectPictureSession::open_revision(
        case.package,
        &case.revision,
        Some(contract.range()),
        &cancelled,
    )?)?;
    let mut audio = OfflineAudioSession::open_revision(
        case.package,
        &case.revision,
        contract.range(),
        &cancelled,
        gpu.deadline,
    )?;
    let picture_path = directory.join(format!("encoded-{}.reference.i420", case.name));
    let audio_path = directory.join(format!("encoded-{}.reference.f32", case.name));
    let mut picture_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&picture_path)?;
    let mut audio_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&audio_path)?;
    for ordinal in 0..contract.frame_count() {
        let project_frame = contract
            .range()
            .start()
            .0
            .checked_add(i64::try_from(ordinal)?)
            .ok_or("frame overflow")?;
        let (bytes, _) = gpu.frame(&mut pictures, project_frame)?;
        picture_file.write_all(&bytes)?;
    }
    let mut position = audio.sample_range().start.0;
    while position < audio.sample_range().end.0 {
        let count = u32::try_from((audio.sample_range().end.0 - position).min(8192))?;
        let block = audio.read(AudioSample(position), count, &cancelled)?;
        for sample in block.samples {
            for channel in sample {
                audio_file.write_all(&channel.to_le_bytes())?;
            }
        }
        position += i64::from(count);
    }
    picture_file.sync_all()?;
    audio_file.sync_all()?;
    check(
        report,
        "direct inputs retain the encoded contract and exact absolute audio interval",
        pictures.contract() == &contract
            && audio.sample_range()
                == (contract.project_audio_start()..contract.project_audio_end()),
        json!({"case": case.name, "contract": contract, "audio_samples": audio.sample_count()}),
    )?;
    report["cases"].as_array_mut().ok_or("missing encoded cases")?.push(json!({
        "name": case.name, "path": path, "picture_reference": picture_path, "audio_reference": audio_path,
        "contract": contract, "manifest": candidate.manifest(), "progress": progress,
        "elapsed_seconds": started.elapsed().as_secs_f64(), "independent_decode": "pending",
    }));
    Ok(())
}

fn make_marker_project(path: &Path, source: &Path, report: &mut Value) -> Result<()> {
    let document = ProjectDocument::new_automatic(
        ProjectId::new("encoded-marker-project")?,
        revision("initial"),
        node("root"),
    )?;
    let mut store = ProjectStore::create(path, &document)?;
    let cancelled = AtomicBool::new(false);
    let limits = OriginalMediaLimits::default();
    let source = source.canonicalize()?;
    let original = store
        .retain_original(&source, OriginalOwnership::Managed, limits, &cancelled)?
        .record;
    let mut snapshot = store.snapshot_original(original.object().content(), limits, &cancelled)?;
    let input = VerifiedSourceInput::copy_verified(
        &mut snapshot,
        SourceContentIdentity::new(original.sha256(), original.object().byte_length())?,
        16_000_000,
        Duration::from_secs(10),
        &cancelled,
    )?;
    let video = SourceSession::open_input(
        input.clone(),
        asset(),
        SourceSessionLimits::default(),
        &cancelled,
    )?;
    let stream = video
        .info()
        .audio_streams
        .first()
        .ok_or("marker source audio missing")?;
    let audio = AudioSession::open_input(
        input,
        stream.stream_index,
        AudioSessionLimits::default(),
        &cancelled,
    )?;
    let decoded = DecodedSourceQualification::from_sessions(Some(&video), Some(&audio))?;
    store.register_source(
        &SourceRegistration {
            expected_revision: revision("initial"),
            new_revision: revision("marker-registered"),
            original: original.object().content().clone(),
            new_asset_id: asset(),
            label: "Known marker Original".into(),
            insertion: Some(SourceInsertionRequest {
                parent: node("root"),
                index: 0,
                node: node("source"),
                label: "Known marker Original".into(),
                purpose: SourceInsertionPurpose::Primary,
            }),
        },
        &decoded,
        None,
        limits,
        &cancelled,
    )?;
    report["marker_source"] = json!({"path": source, "sha256": original.sha256(),
        "bytes": original.object().byte_length(), "video_frames": video.index().index().frames().len(),
        "basis": store.snapshot()?.presentation_basis()});
    Ok(())
}
