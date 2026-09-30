//! Real Metal preparation from one committed project while its writer edits.
//! Usage: qualify_project_picture REPORT.json NEW_WORK_DIRECTORY
use std::{
    collections::BTreeMap,
    error::Error,
    fs::{self, File, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use deadpan_cli::picture::{PreparedPicture, ProjectPictureSession};
use deadpan_core::{
    AssetId, BeatNode, CapturedCanvas, CapturedFit, CapturedFraming, Command, CommandRequest,
    ExactRatio, FrameDuration, Framing, FramingPose, HoldAudio, HoldRecipe, HoldVideo, NodeId,
    PitchPolicy, ProjectDocument, ProjectFrame, ProjectId, RevisionId, SourceTimestamp, Subtree,
};
use deadpan_media::{
    audio_session::{AudioSession, AudioSessionLimits},
    source_index::SourceContentIdentity,
    source_input::VerifiedSourceInput,
    source_qualification::DecodedSourceQualification,
    source_session::{SourceSession, SourceSessionLimits},
};
use deadpan_render::{FitMode, PictureRenderer, Rec709Yuv420Frame, RenderError, RenderTarget};
use deadpan_store::{
    ProjectStore,
    original_media::{OriginalMediaLimits, OriginalOwnership},
    source_registration::{SourceInsertionPurpose, SourceInsertionRequest, SourceRegistration},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

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
    if arguments.len() != 2 {
        return Err("usage: qualify_project_picture REPORT.json NEW_WORK_DIRECTORY".into());
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&arguments[0])?;
    let mut report = json!({"schema_version": 1, "status": "running", "checks": [], "frames": [],
        "scope": "committed project picture preparation through actual Metal and owned SDR I420",
        "deadline": {"cooperative_seconds": 60, "scope": "whole example, checked around bounded calls",
            "external_timeout_required": true, "native_calls_are_not_preempted": true},
        "limitations": ["synthetic SDR fixture", "no encoded export or audio", "no final-render process isolation",
            "no Accepted/Still provider", "no HDR, physical display or performance qualification"]});
    let started = Instant::now();
    // Retain a valid running report even if an outer deadline terminates a
    // native call before ordinary Rust error handling can finish.
    write_report(&mut output, &report)?;
    let deadline = started + Duration::from_secs(60);
    let result = qualify(Path::new(&arguments[1]), &mut report, deadline)
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

fn make_project(path: &Path, report: &mut Value) -> Result<ProjectStore> {
    let document = ProjectDocument::new_automatic(
        ProjectId::new("project-picture-qualification")?,
        revision("initial"),
        node("root"),
    )?;
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
    report["source"] = json!({"path": fixture, "sha256": original.sha256(),
        "bytes": original.object().byte_length(), "frames": index.frames().len(), "frozen_frame": 41,
        "frozen_pts": frozen_at, "basis": basis});
    check(
        report,
        "known automatic NTSC source basis",
        basis.width == 320
            && basis.height == 180
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
        width: basis.width,
        height: basis.height,
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
    renderer: PictureRenderer,
    target: RenderTarget,
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
            renderer,
            target,
            deadline,
        })
    }

    fn frame(
        &mut self,
        session: &mut ProjectPictureSession,
        number: i64,
    ) -> Result<(Vec<u8>, Value)> {
        let cancelled = AtomicBool::new(false);
        check_deadline(self.deadline)?;
        let prepared = session.prepare(ProjectFrame(number), &cancelled)?;
        check_deadline(self.deadline)?;
        if prepared.revision_id != *session.revision()
            || prepared.project_frame != ProjectFrame(number)
            || prepared.canvas != [320, 180]
        {
            return Err("prepared frame lost immutable identity or canvas".into());
        }
        while !self.renderer.is_idle()? {
            if Instant::now() >= self.deadline {
                return Err("GPU idle deadline".into());
            }
            std::thread::yield_now();
        }
        let layers = prepared.render_layers()?;
        let source = match &prepared.picture {
            PreparedPicture::Frame {
                asset,
                qualification,
                id,
                frame,
            } => {
                self.renderer.render_composed(
                    frame,
                    &self.target,
                    prepared.picture_context.as_deref(),
                    prepared.canvas,
                    FitMode::Fit,
                    &layers,
                )?;
                json!({"asset": asset, "qualification": qualification, "frame": id.0, "pts": frame.metadata().pts})
            }
            PreparedPicture::Generated { .. } => {
                return Err("fixture must retain Original media".into());
            }
            PreparedPicture::Background => {
                self.renderer.render_background(&self.target)?;
                Value::Null
            }
        };
        let mut pending = loop {
            match self
                .renderer
                .begin_working_readback(&self.target, &cancelled, self.deadline)
            {
                Ok(pending) => break pending,
                Err(RenderError::ReadbackBusy) if Instant::now() < self.deadline => {
                    std::thread::yield_now()
                }
                Err(error) => return Err(error.into()),
            }
        };
        let working = loop {
            if Instant::now() >= self.deadline {
                return Err("GPU readback deadline".into());
            }
            if let Some(frame) = pending.poll(&cancelled)? {
                break frame;
            }
            std::thread::yield_now();
        };
        let planes = Rec709Yuv420Frame::from_working(&working)?;
        check_deadline(self.deadline)?;
        let metadata = json!({"project_id": prepared.project_id, "revision_id": prepared.revision_id,
            "project_frame": number, "canvas": prepared.canvas, "frame_rate": prepared.frame_rate,
            "source": source, "framing_scopes": prepared.framing.len(), "gap_after": prepared.gap_after,
            "picture_context": prepared.picture_context.as_deref(), "byte_count": planes.bytes().len()});
        Ok((planes.bytes().to_vec(), metadata))
    }
}

fn render(
    gpu: &mut Gpu,
    session: &mut ProjectPictureSession,
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

fn qualify(directory: &Path, report: &mut Value, deadline: Instant) -> Result<()> {
    check_deadline(deadline)?;
    fs::create_dir(directory)?;
    let directory = directory.canonicalize()?;
    let project = directory.join("picture.deadpan");
    let mut writer = make_project(&project, report)?;
    check_deadline(deadline)?;
    let committed = writer.snapshot()?;
    let cancelled = AtomicBool::new(false);
    let mut session =
        ProjectPictureSession::open_revision(&project, committed.revision_id(), None, &cancelled)?;
    check_deadline(deadline)?;
    report["project"] = json!({"path": project, "committed_revision": committed.revision_id(),
        "duration": session.plan().duration(), "range": session.range()});
    check(
        report,
        "exact project duration after retime, repeat and Holds",
        session.plan().duration().frames() == 128,
        json!({}),
    )?;
    let mut gpu = Gpu::new(report, deadline)?;
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
        let bytes = render(
            &mut gpu,
            &mut session,
            number,
            "committed",
            &directory,
            report,
        )?;
        let last = report["frames"]
            .as_array()
            .ok_or("missing frames")?
            .last()
            .ok_or("missing frame")?;
        let observed = last["source"]["frame"].as_u64();
        check(
            report,
            "exact manually expected original frame selection",
            observed == expected_source,
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
        session.prepare(ProjectFrame(128), &cancelled).is_err(),
        json!({}),
    )?;
    check(
        report,
        "cancelled preparation publishes no frame",
        session
            .prepare(ProjectFrame(20), &AtomicBool::new(true))
            .is_err(),
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
    let mut changed =
        ProjectPictureSession::open_revision(&project, &revision("live-change"), None, &cancelled)?;
    let changed_source = render(&mut gpu, &mut changed, 20, "changed", &directory, report)?;
    let retained_hold = render(&mut gpu, &mut changed, 122, "changed", &directory, report)?;
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
        &mut gpu,
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
    let old = render(&mut gpu, &mut session, 20, "after-undo", &directory, report)?;
    check(
        report,
        "live undo cannot retarget captured revision",
        old == frames[&20],
        json!({}),
    )?;
    writer.redo(&revision("live-undo"), revision("live-redo"))?;
    let old = render(&mut gpu, &mut session, 20, "after-redo", &directory, report)?;
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
    report["frame_count"] = json!(report["frames"].as_array().ok_or("missing frames")?.len());
    check_deadline(deadline)?;
    Ok(())
}
