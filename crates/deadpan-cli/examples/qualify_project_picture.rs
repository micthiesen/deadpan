//! Real Metal preparation from one committed project while its writer edits.
//! Usage: qualify_project_picture REPORT.json NEW_WORK_DIRECTORY [ACCEPTED.deadpan [WORKER_EXECUTABLE [--encoded MARKER_SOURCE]]]
//! The optional package must retain the adjacent generated-picture-fixture.json
//! exported by the Generated picture fixture. Omitting it reports that skip.
use std::{
    collections::BTreeMap,
    error::Error,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use deadpan_cli::{
    export_picture::{
        ExportPictureError, ExportPictureSession, ExportPictureSource, OutputFrameOrdinal,
    },
    picture::{PreparedPicture, ProjectPictureSession},
};
use deadpan_core::{
    AssetId, BeatNode, CapturedCanvas, CapturedFit, CapturedFraming, ColorPolicy, Command,
    CommandRequest, ExactRatio, FrameDuration, FrameRange, FrameRate, Framing, FramingPose,
    GeneratedArtifact, HoldAudio, HoldRecipe, HoldVideo, NodeId, PitchPolicy, PresentationBasis,
    ProjectDocument, ProjectFrame, ProjectId, RevisionId, SourceTimeBase, SourceTimestamp, Subtree,
};
use deadpan_media::{
    audio_session::{AudioSession, AudioSessionLimits},
    source_index::SourceContentIdentity,
    source_input::VerifiedSourceInput,
    source_qualification::DecodedSourceQualification,
    source_session::{SourceSession, SourceSessionLimits},
};
use deadpan_render::{
    FitMode, FrameMetadata, FramingLayer, PictureGeometry, PictureRenderer, Primaries, RenderError,
    Rgba8Frame, Rotation, SampleAspectRatio, SourceColor, Transfer,
};
use deadpan_store::{
    ProjectStore,
    original_media::{OriginalMediaLimits, OriginalOwnership},
    source_registration::{SourceInsertionPurpose, SourceInsertionRequest, SourceRegistration},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[path = "qualify_project_picture/reference.rs"]
mod reference;

#[path = "qualify_project_picture/worker.rs"]
mod worker;

#[path = "qualify_project_picture/encoded.rs"]
mod encoded;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn node(value: &str) -> NodeId {
    NodeId::new(value).expect("constant node ID")
}
fn revision(value: &str) -> RevisionId {
    RevisionId::new(value).expect("constant revision ID")
}
fn asset() -> AssetId {
    AssetId::new("original").expect("constant asset ID")
}

fn main() -> Result<()> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if !(2..=4).contains(&arguments.len()) && !(arguments.len() == 6 && arguments[4] == "--encoded")
    {
        return Err(
            "usage: qualify_project_picture REPORT.json NEW_WORK_DIRECTORY [ACCEPTED.deadpan [WORKER_EXECUTABLE [--encoded MARKER_SOURCE]]]"
                .into(),
        );
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&arguments[0])?;
    let seconds = if arguments.len() == 6 {
        600
    } else if arguments.len() == 4 {
        300
    } else if arguments.len() == 3 {
        180
    } else {
        60
    };
    let mut report = json!({"schema_version": 3, "status": "running", "checks": [], "frames": [],
        "scope": "production fixed-revision output-picture contract through actual Metal and owned SDR I420",
        "deadline": {"cooperative_seconds": seconds, "scope": "whole example, checked around bounded calls",
            "external_timeout_required": true, "native_calls_are_not_preempted": true},
        "limitations": ["synthetic SDR fixtures", "no encoded export or rendered audio", "no durable render jobs or native Render workflow",
            "no legacy Accepted/Still provider", "no HDR, physical display or performance qualification"]});
    if arguments.len() == 6 {
        report["scope"] = json!(
            "committed SDR pictures and canonical audio through actual Metal, isolated encoding, verification and explicit destination publication"
        );
        report["limitations"] = json!([
            "synthetic SDR fixtures; independent decoded-content qualification follows separately",
            "no durable render jobs, automatic platform policy or native Render workflow",
            "no legacy Accepted/Still provider, complete mastering/effects, HDR or release qualification",
            "no physical display or sustained performance qualification"
        ]);
    }
    let started = Instant::now();
    // Retain a valid running report even if an outer deadline terminates a
    // native call before ordinary Rust error handling can finish.
    write_report(&mut output, &report)?;
    let deadline = started + Duration::from_secs(seconds);
    let result = qualify(
        Path::new(&arguments[1]),
        arguments.get(2).map(Path::new),
        arguments.get(3).map(Path::new),
        arguments.get(5).map(Path::new),
        &mut report,
        deadline,
    )
    .and_then(|()| check_deadline(deadline));
    report["elapsed_seconds"] = json!(started.elapsed().as_secs_f64());
    match &result {
        Ok(()) => report["status"] = json!("passed"),
        Err(error) => {
            report["status"] = json!("failed");
            report["error"] = json!(error.to_string());
        }
    }
    write_report(&mut output, &report)?;
    result?;
    println!("Committed project Metal picture qualification passed");
    Ok(())
}

fn write_report(output: &mut File, report: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(report)?;
    bytes.push(b'\n');
    output.seek(SeekFrom::Start(0))?;
    output.write_all(&bytes)?;
    output.set_len(u64::try_from(bytes.len())?)?;
    output.sync_all()?;
    Ok(())
}

fn check_deadline(deadline: Instant) -> Result<()> {
    if Instant::now() >= deadline {
        return Err("project picture qualification exceeded its cooperative deadline".into());
    }
    Ok(())
}

fn check(report: &mut Value, label: &str, passed: bool, actual: Value) -> Result<()> {
    report["checks"]
        .as_array_mut()
        .ok_or("missing checks")?
        .push(json!({"label": label, "passed": passed, "actual": actual}));
    if !passed {
        return Err(format!("qualification failed: {label}").into());
    }
    Ok(())
}

fn edit(store: &mut ProjectStore, name: &str, command: Command) -> Result<()> {
    let document = store.snapshot()?;
    store.commit(&CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision(name),
        command,
    })?;
    Ok(())
}

fn append_hold(
    store: &mut ProjectStore,
    name: &str,
    index: usize,
    recipe: HoldRecipe,
) -> Result<()> {
    edit(
        store,
        name,
        Command::Insert {
            parent: node("root"),
            index,
            subtree: Subtree {
                root: node(name),
                nodes: BTreeMap::from([(node(name), BeatNode::hold(name, recipe))]),
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    )
}

fn make_project(path: &Path, report: &mut Value, odd_canvas: bool) -> Result<ProjectStore> {
    let document = if odd_canvas {
        ProjectDocument::new(
            ProjectId::new("project-picture-qualification-odd")?,
            revision("initial"),
            PresentationBasis {
                width: 319,
                height: 179,
                frame_rate: FrameRate::new(30000, 1001)?,
                color_policy: ColorPolicy::SdrRec709,
            },
            node("root"),
        )?
    } else {
        ProjectDocument::new_automatic(
            ProjectId::new("project-picture-qualification")?,
            revision("initial"),
            node("root"),
        )?
    };
    let mut store = ProjectStore::create(path, &document)?;
    let cancelled = AtomicBool::new(false);
    let limits = OriginalMediaLimits::default();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4")
        .canonicalize()?;
    let original = store
        .retain_original(&fixture, OriginalOwnership::Managed, limits, &cancelled)?
        .record;
    let mut snapshot = store.snapshot_original(original.object().content(), limits, &cancelled)?;
    let input = VerifiedSourceInput::copy_verified(
        &mut snapshot,
        SourceContentIdentity::new(original.sha256(), original.object().byte_length())?,
        2_000_000,
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
        .ok_or("fixture audio stream missing")?;
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
            new_revision: revision("registered"),
            original: original.object().content().clone(),
            new_asset_id: asset(),
            label: "Synthetic original".into(),
            insertion: Some(SourceInsertionRequest {
                parent: node("root"),
                index: 0,
                node: node("source"),
                label: "Full Original".into(),
                purpose: SourceInsertionPurpose::Primary,
            }),
        },
        &decoded,
        None,
        limits,
        &cancelled,
    )?;
    let index = video.index().index();
    let frozen = index.frames().get(41).ok_or("fixture frame 41 missing")?;
    let frozen_at = SourceTimestamp {
        ticks: frozen.pts,
        time_base: index.time_base(),
    };
    let basis = store.snapshot()?.presentation_basis().clone();
    report[if odd_canvas { "odd_source" } else { "source" }] = json!({"path": fixture, "sha256": original.sha256(),
        "bytes": original.object().byte_length(), "frames": index.frames().len(), "frozen_frame": 41,
        "frozen_pts": frozen_at, "basis": basis});
    check(
        report,
        "known NTSC source basis and retained explicit odd canvas",
        [basis.width, basis.height] == if odd_canvas { [319, 179] } else { [320, 180] }
            && basis.frame_rate == deadpan_core::FrameRate::new(30000, 1001)?
            && index.frames().len() == 120,
        json!(basis),
    )?;
    let pose = FramingPose::new(
        ExactRatio::new(3, 5)?,
        ExactRatio::new(2, 5)?,
        ExactRatio::new(7, 5)?,
    )?;
    edit(
        &mut store,
        "framed",
        Command::SetFraming {
            node: node("source"),
            framing: Some(Framing::static_pose(pose)?),
        },
    )?;
    edit(
        &mut store,
        "fast",
        Command::WrapRetime {
            node: node("source"),
            id: node("fast"),
            duration: FrameDuration::new(60)?,
            pitch: PitchPolicy::FollowSpeed,
        },
    )?;
    edit(
        &mut store,
        "repeat",
        Command::WrapRepeat {
            node: node("fast"),
            id: node("repeat"),
            plays: 2,
            gap: Some(HoldRecipe {
                picture_context: None,
                duration: FrameDuration::new(2)?,
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            }),
            anchor_policy: Default::default(),
        },
    )?;
    let context = CapturedFraming::new(vec![CapturedCanvas {
        width: 320,
        height: 180,
        fit: CapturedFit::Fit,
        layers: vec![Some(pose)],
    }])?;
    append_hold(
        &mut store,
        "freeze",
        1,
        HoldRecipe {
            picture_context: Some(context),
            duration: FrameDuration::new(3)?,
            video: HoldVideo::Freeze {
                asset: asset(),
                timestamp: frozen_at,
            },
            audio: HoldAudio::Silence,
        },
    )?;
    append_hold(
        &mut store,
        "background",
        2,
        HoldRecipe {
            picture_context: None,
            duration: FrameDuration::new(3)?,
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )?;
    Ok(store)
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    deadline: Instant,
}

impl Gpu {
    fn new(report: &mut Value, deadline: Instant) -> Result<Self> {
        check_deadline(deadline)?;
        if !cfg!(target_os = "macos") {
            return Err("qualification requires native macOS Metal".into());
        }
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::METAL,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            ..Default::default()
        }))?;
        let info = adapter.get_info();
        check(
            report,
            "real Metal adapter",
            info.backend == wgpu::Backend::Metal,
            json!({"name": info.name, "backend": format!("{:?}", info.backend)}),
        )?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("Deadpan committed project qualification"),
                ..Default::default()
            }))?;
        check_deadline(deadline)?;
        let renderer = PictureRenderer::new(&device, &queue);
        let target = renderer.create_target(320, 180)?;
        let mut other = PictureRenderer::new(&device, &queue);
        check(
            report,
            "background rejects a foreign renderer target",
            matches!(
                other.render_background(&target),
                Err(RenderError::ForeignTarget)
            ),
            json!({}),
        )?;
        Ok(Self {
            device,
            queue,
            deadline,
        })
    }

    fn session(&self, pictures: ProjectPictureSession) -> Result<ExportPictureSession> {
        Ok(ExportPictureSession::new(
            pictures,
            PictureRenderer::new(&self.device, &self.queue),
            &AtomicBool::new(false),
            self.deadline,
        )?)
    }

    fn frame(&self, session: &mut ExportPictureSession, number: i64) -> Result<(Vec<u8>, Value)> {
        let cancelled = AtomicBool::new(false);
        check_deadline(self.deadline)?;
        let ordinal = u64::try_from(
            number
                .checked_sub(session.contract().range().start().0)
                .ok_or("output ordinal subtraction overflow")?,
        )?;
        let prepared = session.prepare(OutputFrameOrdinal(ordinal), &cancelled, self.deadline)?;
        check_deadline(self.deadline)?;
        let contract = prepared.contract();
        let timing = prepared.timing();
        if contract.revision_id() != session.contract().revision_id()
            || timing.project_frame() != ProjectFrame(number)
            || timing.output_frame() != OutputFrameOrdinal(ordinal)
            || timing.pts()
                != i64::try_from(ordinal)?
                    .checked_mul(i64::from(contract.frame_rate().denominator()))
                    .ok_or("PTS overflow")?
            || timing.duration() != i64::from(contract.frame_rate().denominator())
            || contract.time_base().numerator() != 1
            || contract.time_base().denominator() != contract.frame_rate().numerator()
        {
            return Err("output frame lost immutable identity or exact rational timing".into());
        }
        let source = match prepared.source() {
            ExportPictureSource::Original {
                asset,
                qualification,
                id,
                pts,
            } => {
                json!({"type": "original", "asset": asset, "qualification": qualification, "frame": id.0, "pts": pts})
            }
            ExportPictureSource::Generated { artifact, id, pts } => {
                json!({"type": "generated", "artifact": artifact.as_ref(), "frame": id.0, "pts": pts})
            }
            ExportPictureSource::Background => Value::Null,
        };
        let metadata = json!({"project_id": contract.project_id(), "revision_id": contract.revision_id(),
            "project_frame": number, "canvas": contract.canvas(), "raster": contract.raster(),
            "frame_rate": contract.frame_rate(), "output_timing": timing, "time_base": contract.time_base(),
            "source": source, "framing_scopes": prepared.framing_scopes(), "gap_after": prepared.gap_after(),
            "picture_context": prepared.picture_context(), "byte_count": prepared.pixels().bytes().len()});
        Ok((prepared.pixels().bytes().to_vec(), metadata))
    }
}

fn render(
    gpu: &Gpu,
    session: &mut ExportPictureSession,
    frame: i64,
    label: &str,
    directory: &Path,
    report: &mut Value,
) -> Result<Vec<u8>> {
    let (bytes, mut metadata) = gpu.frame(session, frame)?;
    let path = directory.join(format!("{label}-{frame}.i420"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    metadata["path"] = json!(path);
    let sha256: String = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    metadata["sha256"] = json!(sha256);
    metadata["label"] = json!(label);
    report["frames"]
        .as_array_mut()
        .ok_or("missing frames")?
        .push(metadata);
    Ok(bytes)
}

fn qualify(
    directory: &Path,
    generated: Option<&Path>,
    executable: Option<&Path>,
    marker_source: Option<&Path>,
    report: &mut Value,
    deadline: Instant,
) -> Result<()> {
    check_deadline(deadline)?;
    fs::create_dir(directory)?;
    let directory = directory.canonicalize()?;
    let project = directory.join("picture.deadpan");
    let mut writer = make_project(&project, report, false)?;
    check_deadline(deadline)?;
    let committed = writer.snapshot()?;
    let cancelled = AtomicBool::new(false);
    let pictures =
        ProjectPictureSession::open_revision(&project, committed.revision_id(), None, &cancelled)?;
    check_deadline(deadline)?;
    report["project"] = json!({"path": project, "committed_revision": committed.revision_id(),
        "duration": pictures.plan().duration(), "range": pictures.range()});
    check(
        report,
        "exact project duration after retime, repeat and Holds",
        pictures.plan().duration().frames() == 128,
        json!({}),
    )?;
    let gpu = Gpu::new(report, deadline)?;
    let mut session = gpu.session(pictures)?;
    check(
        report,
        "full-range exact output contract",
        session.contract().frame_count() == 128
            && session.contract().terminal_pts() == 128128
            && session.contract().canvas() == [320, 180]
            && session.contract().raster() == [320, 180],
        json!(session.contract()),
    )?;
    let mut frames = BTreeMap::new();
    for (number, expected_source) in [
        (0, Some(1)),
        (20, Some(41)),
        (59, Some(119)),
        (60, None),
        (61, None),
        (62, Some(1)),
        (82, Some(41)),
        (121, Some(119)),
        (122, Some(41)),
        (123, Some(41)),
        (124, Some(41)),
        (125, None),
        (127, None),
    ] {
        let bytes = render(&gpu, &mut session, number, "committed", &directory, report)?;
        let last = report["frames"]
            .as_array()
            .ok_or("missing frames")?
            .last()
            .ok_or("missing frame")?;
        let observed = last["source"]["frame"].as_u64();
        let source_metadata = last["source"].clone();
        let gap_after = last["gap_after"].clone();
        check(
            report,
            "exact manually expected original frame selection",
            observed == expected_source
                && expected_source.is_none_or(|_| source_metadata["type"] == "original"),
            json!({"project_frame": number, "expected": expected_source, "observed": observed}),
        )?;
        if expected_source.is_none() {
            check(
                report,
                "authored background is opaque legal black in all planes",
                bytes.len() == 86400
                    && bytes[..57600].iter().all(|code| *code == 16)
                    && bytes[57600..].iter().all(|code| *code == 128),
                json!({"project_frame": number}),
            )?;
        }
        if number == 20 {
            let source_pts: SourceTimestamp =
                serde_json::from_value(source_metadata["pts"].clone())?;
            check(
                report,
                "retimed source PTS remains distinct from output PTS",
                i128::from(source_pts.ticks)
                    * i128::from(source_pts.time_base.numerator())
                    * 30_000
                    != 20_020 * i128::from(source_pts.time_base.denominator()),
                json!({"source": source_metadata, "output_pts": 20_020}),
            )?;
        }
        check(
            report,
            "Repeat gap retains its stable issuer only on gap frames",
            !gap_after.is_null() == matches!(number, 60 | 61),
            json!({"project_frame": number, "gap_after": gap_after}),
        )?;
        frames.insert(number, bytes);
    }
    check(
        report,
        "repeated retimed picture remains identical",
        frames[&20] == frames[&82],
        json!({}),
    )?;
    check(
        report,
        "captured Hold preserves framing of its explicitly selected source frame",
        [122, 123, 124]
            .iter()
            .all(|frame| frames[frame] == frames[&20]),
        json!({}),
    )?;
    check(
        report,
        "range end is excluded",
        matches!(
            session.prepare(OutputFrameOrdinal(128), &cancelled, deadline),
            Err(ExportPictureError::Range)
        ),
        json!({}),
    )?;
    check(
        report,
        "cancelled preparation publishes no frame",
        matches!(
            session.prepare(OutputFrameOrdinal(20), &AtomicBool::new(true), deadline),
            Err(ExportPictureError::Cancelled)
        ),
        json!({}),
    )?;
    check(
        report,
        "expired deadline publishes no frame",
        matches!(
            session.prepare(OutputFrameOrdinal(20), &cancelled, Instant::now()),
            Err(ExportPictureError::Deadline)
        ),
        json!({}),
    )?;
    let held = session.prepare(OutputFrameOrdinal(20), &cancelled, deadline)?;
    check(
        report,
        "one retained output result rejects another preparation",
        matches!(
            session.prepare(OutputFrameOrdinal(21), &cancelled, deadline),
            Err(ExportPictureError::OutstandingFrame)
        ),
        json!({"held_project_frame": held.timing().project_frame()}),
    )?;
    drop(held);
    let resumed = session.prepare(OutputFrameOrdinal(20), &cancelled, deadline)?;
    check(
        report,
        "dropping the result and prior failed requests permit exact recovery",
        resumed.pixels().bytes() == frames[&20],
        json!({}),
    )?;
    drop(resumed);

    let range = FrameRange::new(ProjectFrame(20), ProjectFrame(63))?;
    let mut ranged = gpu.session(ProjectPictureSession::open_revision(
        &project,
        committed.revision_id(),
        Some(range),
        &cancelled,
    )?)?;
    check(
        report,
        "nonzero capture retains origin-based absolute audio boundaries",
        ranged.contract().frame_count() == 43
            && ranged.contract().terminal_pts() == 43_043
            && ranged.contract().project_audio_start().0 == 32_032
            && ranged.contract().project_audio_end().0 == 100_901,
        json!(ranged.contract()),
    )?;
    let range_first = render(&gpu, &mut ranged, 20, "range-first", &directory, report)?;
    let range_last = render(&gpu, &mut ranged, 62, "range-last", &directory, report)?;
    check(
        report,
        "relative output clock preserves first and last selected pictures",
        range_first == frames[&20] && range_last == frames[&62],
        json!({}),
    )?;

    edit(
        &mut writer,
        "live-change",
        Command::SetFraming {
            node: node("source"),
            framing: Some(Framing::static_pose(FramingPose::new(
                ExactRatio::new(1, 2)?,
                ExactRatio::new(1, 2)?,
                ExactRatio::new(2, 1)?,
            )?)?),
        },
    )?;
    let mut changed = gpu.session(ProjectPictureSession::open_revision(
        &project,
        &revision("live-change"),
        None,
        &cancelled,
    )?)?;
    let changed_source = render(&gpu, &mut changed, 20, "changed", &directory, report)?;
    let retained_hold = render(&gpu, &mut changed, 122, "changed", &directory, report)?;
    check(
        report,
        "new committed Source pose changes its picture",
        changed_source != frames[&20],
        json!({}),
    )?;
    check(
        report,
        "captured Hold geometry survives changed source pose",
        retained_hold == frames[&20],
        json!({}),
    )?;
    let old = render(
        &gpu,
        &mut session,
        20,
        "after-live-change",
        &directory,
        report,
    )?;
    check(
        report,
        "in-progress session retains committed picture after live edit",
        old == frames[&20],
        json!({}),
    )?;
    writer.undo(&revision("live-change"), revision("live-undo"))?;
    let old = render(&gpu, &mut session, 20, "after-undo", &directory, report)?;
    check(
        report,
        "live undo cannot retarget captured revision",
        old == frames[&20],
        json!({}),
    )?;
    writer.redo(&revision("live-undo"), revision("live-redo"))?;
    let old = render(&gpu, &mut session, 20, "after-redo", &directory, report)?;
    check(
        report,
        "live redo cannot retarget captured revision",
        old == frames[&20],
        json!({}),
    )?;
    check(
        report,
        "read-only picture work preserves original snapshot and live history",
        serde_json::to_value(writer.snapshot_at(committed.revision_id())?)?
            == serde_json::to_value(&committed)?
            && writer.snapshot()?.revision_id() == &revision("live-redo"),
        json!({}),
    )?;
    drop(writer);
    let old = render(
        &gpu,
        &mut session,
        20,
        "after-writer-close",
        &directory,
        report,
    )?;
    check(
        report,
        "writer close preserves committed output pixels",
        old == frames[&20],
        json!({}),
    )?;
    qualify_odd(&gpu, &directory, report)?;
    if let Some(package) = generated {
        qualify_generated(&gpu, package, &directory, report)?;
    } else {
        report["generated"] = json!({"status": "skipped",
            "reason": "Pass the retained accepted.deadpan fixture as the third argument to exercise Generated output."});
    }
    if let Some(executable) = executable {
        worker::qualify(&gpu, executable, &directory, generated, report)?;
        if let Some(marker_source) = marker_source {
            encoded::qualify(
                &gpu,
                executable,
                &directory,
                generated,
                marker_source,
                report,
            )?;
        }
    } else {
        report["worker"] = json!({"status": "skipped",
            "reason": "Pass the freshly built deadpan-cli executable as the fourth argument to exercise process isolation."});
    }
    report["frame_count"] = json!(report["frames"].as_array().ok_or("missing frames")?.len());
    check_deadline(deadline)?;
    Ok(())
}

fn record_reference(
    actual: &[u8],
    expected: &[u8],
    raster: [u32; 2],
    path: &Path,
    label: &str,
    report: &mut Value,
) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(expected)?;
    file.sync_all()?;
    let (passed, mut comparison) = reference::compare(actual, expected, raster)?;
    comparison["reference_path"] = json!(path);
    let reference_sha256: String = Sha256::digest(expected)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    comparison["reference_sha256"] = json!(reference_sha256);
    comparison["source"] = json!(
        "independent f64 source transfer, linear interpolation, Rec.709 OETF/matrix and left-sited chroma filter; shared canvas geometry"
    );
    check(report, label, passed, comparison)
}

fn qualify_odd(gpu: &Gpu, directory: &Path, report: &mut Value) -> Result<()> {
    let path = directory.join("odd-canvas.deadpan");
    let store = make_project(&path, report, true)?;
    let committed = store.snapshot()?;
    let cancelled = AtomicBool::new(false);
    let mut pictures =
        ProjectPictureSession::open_revision(&path, committed.revision_id(), None, &cancelled)?;
    let decoded = pictures.prepare(ProjectFrame(20), &cancelled)?;
    let PreparedPicture::Frame { id, frame, .. } = decoded.picture else {
        return Err("odd fixture source frame missing".into());
    };
    let mut session = gpu.session(pictures)?;
    check(
        report,
        "odd committed canvas normalizes only its output raster",
        session.contract().canvas() == [319, 179]
            && session.contract().raster() == [318, 178]
            && session.contract().relative_aspect_error() == ExactRatio::new(70, 28391)?
            && id.0 == 41,
        json!(session.contract()),
    )?;
    let expected_layers = [
        FramingLayer::new(
            [ExactRatio::new(3, 5)?, ExactRatio::new(2, 5)?],
            ExactRatio::new(7, 5)?,
        )?,
        FramingLayer::identity(),
    ];
    let geometry = PictureGeometry::composed(
        frame.metadata(),
        None,
        [319, 179],
        [318, 178],
        FitMode::Fit,
        &expected_layers,
    )?;
    let expected = reference::i420(&frame, &geometry, [318, 178], gpu.deadline)?;
    let actual = render(gpu, &mut session, 20, "odd", directory, report)?;
    record_reference(
        &actual,
        &expected,
        [318, 178],
        &directory.join("odd-reference-20.i420"),
        "odd canvas keeps authored framing through even-raster Metal output",
        report,
    )?;
    check(
        report,
        "odd output preparation leaves committed canvas and history unchanged",
        store.snapshot()? == committed,
        json!({"revision": committed.revision_id()}),
    )?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GeneratedFixture {
    schema_version: u32,
    fixture: String,
    project_id: ProjectId,
    revision_id: RevisionId,
    artifact: GeneratedArtifact,
    picture_context: CapturedFraming,
    frames: Vec<ExpectedGeneratedFrame>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedGeneratedFrame {
    ordinal: u64,
    pts: i64,
    rgba: [u8; 32],
}

impl GeneratedFixture {
    fn read(package: &Path) -> Result<Self> {
        let path = package
            .parent()
            .ok_or("generated package has no parent")?
            .join("generated-picture-fixture.json");
        let mut bytes = Vec::new();
        File::open(path)?
            .take(128 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 128 * 1024 {
            return Err("generated fixture manifest exceeds 128 KiB".into());
        }
        let fixture: Self = serde_json::from_slice(&bytes)?;
        if fixture.schema_version != 1
            || fixture.fixture != "rgb25_24 sampled to 30 frames at 30000/1001"
            || fixture.project_id.as_str() != "project"
            || fixture.revision_id.as_str() != "ui-generated-ready"
            || fixture.frames.len() != 30
        {
            return Err("expected the explicitly retained canonical Generated fixture".into());
        }
        for (ordinal, expected) in fixture.frames.iter().enumerate() {
            if expected.ordinal != u64::try_from(ordinal)?
                || expected.pts != (i64::try_from(ordinal)? * 1_001_000 + 15_000) / 30_000
                || expected.rgba.chunks_exact(4).any(|pixel| pixel[3] != 255)
            {
                return Err("Generated manifest frame identity or opaque RGBA disagrees".into());
            }
        }
        Ok(fixture)
    }
}

fn qualify_generated(
    gpu: &Gpu,
    package: &Path,
    directory: &Path,
    report: &mut Value,
) -> Result<()> {
    check_deadline(gpu.deadline)?;
    let fixture = GeneratedFixture::read(package)?;
    let store = ProjectStore::open(package, deadpan_store::AccessMode::ReadOnly)?;
    let before = store.snapshot()?;
    let mut session = gpu.session(ProjectPictureSession::open_revision(
        package,
        &fixture.revision_id,
        None,
        &AtomicBool::new(false),
    )?)?;
    check(
        report,
        "retained accepted Generated fixture captures the complete output contract",
        session.contract().project_id() == &fixture.project_id
            && session.contract().revision_id() == &fixture.revision_id
            && session.contract().canvas() == [1920, 1080]
            && session.contract().raster() == [1920, 1080]
            && session.contract().frame_count() == 30
            && session.contract().terminal_pts() == 30_030,
        json!(session.contract()),
    )?;
    report["generated"] = json!({"status": "running", "path": package,
        "fixture": fixture.fixture, "frame_count": 30,
        "oracle": "retained expected RGBA, independently converted to all I420 plane codes",
        "tolerance_codes": 1});
    for expected in &fixture.frames {
        check_deadline(gpu.deadline)?;
        let number = i64::try_from(expected.ordinal)?;
        let actual = render(gpu, &mut session, number, "generated", directory, report)?;
        let metadata = report["frames"]
            .as_array()
            .ok_or("missing frames")?
            .last()
            .ok_or("missing generated frame")?
            .clone();
        let source = &metadata["source"];
        let expected_pts = SourceTimestamp {
            ticks: expected.pts,
            time_base: SourceTimeBase::new(1, 1000)?,
        };
        check(
            report,
            "Generated output preserves accepted identity, sampled ordinal, source PTS and captured framing",
            source["type"] == "generated"
                && source["frame"] == expected.ordinal
                && source["artifact"] == json!(fixture.artifact)
                && source["pts"] == json!(expected_pts)
                && metadata["picture_context"] == json!(fixture.picture_context)
                && metadata["framing_scopes"] == 2,
            json!({"ordinal": expected.ordinal, "source_pts": source["pts"], "output_timing": metadata["output_timing"]}),
        )?;
        // The expected bytes come from the retained fixture's independent
        // sampling oracle, never from a freshly decoded production frame.
        let frame = Rgba8Frame::new(
            FrameMetadata {
                width: 4,
                height: 2,
                row_stride_bytes: 16,
                sample_aspect_ratio: SampleAspectRatio::new(1, 1)?,
                rotation: Rotation::None,
                color: SourceColor {
                    transfer: Transfer::Srgb,
                    primaries: Primaries::Rec709,
                },
                pts: expected_pts,
            },
            expected.rgba.to_vec(),
        )?;
        let geometry = PictureGeometry::composed(
            frame.metadata(),
            Some(&fixture.picture_context),
            [1920, 1080],
            [1920, 1080],
            FitMode::Fit,
            &[FramingLayer::identity(), FramingLayer::identity()],
        )?;
        let reference = reference::i420(&frame, &geometry, [1920, 1080], gpu.deadline)?;
        record_reference(
            &actual,
            &reference,
            [1920, 1080],
            &directory.join(format!("generated-reference-{number}.i420")),
            &format!("Generated frame {number} matches independent complete I420 planes"),
            report,
        )?;
    }
    check(
        report,
        "Generated output leaves the accepted project and history unchanged",
        store.snapshot()? == before,
        json!({"revision": before.revision_id()}),
    )?;
    report["generated"]["status"] = json!("passed");
    Ok(())
}
