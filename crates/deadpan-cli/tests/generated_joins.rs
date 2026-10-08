#![cfg(all(target_os = "macos", feature = "synthetic-worker"))]

//! Source joins and speech preservation of an accepted AI pause (spec §12.2,
//! §12.5 and the §2 "Live stare" scenario), on real media through
//! production code.
//!
//! The fixture is the `freeze_hold` recipe: the 30-frame base edit of
//! `cfr-bframes.mp4` (Original 12..42, burnt-in counter, a click at Original
//! sample 48,000) with a 15-frame silent Freeze Hold inserted at Edit
//! boundary f = 15, so L (Original 26) shows at f-1 and R (Original 27),
//! formerly at f, moves to f+N = 30. The pause is filled by two synthetic
//! Ready variants: real conditioning from the committed boundary pictures,
//! the durable attempt lifecycle, host qualification by
//! `deadpan-media-worker`, publication and `accept_generation_bundle`; only
//! the model is replaced. Variant 1 is accepted, undone (back to the Freeze
//! fallback), then variant 2 is accepted.
//!
//! Pictures are read through `ProjectPictureSession`, the committed reader
//! render and conditioning use. Audio is read through the CLI's
//! `ProjectAudioSession` (edge-faded bus) and `OfflineAudioSession` (the
//! limited bus shared by audition and export). The speech checks are
//! structural: exact bus equality and an exact sample shift of the decoded
//! programme. They are not a listening test, and the synthetic pictures say
//! nothing about real-model seam quality.

#[allow(dead_code)]
#[path = "preview_export/recipes.rs"]
mod recipes;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_cli::audio::{OfflineAudioSession, ProjectAudioSession};
use deadpan_cli::generation::acceptance;
use deadpan_cli::generation::attempt::{self, AllocateInput, synthetic};
use deadpan_cli::generation::conditioning;
use deadpan_cli::generation::joins::{self, JoinClass, RgbPicture};
use deadpan_cli::picture::{
    PreparedPicture, ProjectPictureSession, aspect_region, fill_canvas_aspect,
    open_candidate_master, source_to_render_frame,
};
use deadpan_core::{
    AudioSample, ExactRatio, FrameRange, GeneratedArtifact, HoldVideo, NodeId, NodeKind,
    ProjectDocument, ProjectFrame, RevisionId, SourceFrameId,
};
use deadpan_jobs::JobState;
use deadpan_render::Rgba8Frame;
use deadpan_store::generated_media::GeneratedReadHandle;
use deadpan_store::generation_attempts::BundleValidationReceipt;
use deadpan_store::{AccessMode, ProjectStore};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

/// The pause: Edit boundary f and its N inserted frames.
const F: i64 = 15;
const N: i64 = 15;
/// The synthetic worker paints a seed-coloured band over native rows
/// `[H*5/8, H*5/8 + H/8)`; picture comparisons with conditioning skip it.
const NATIVE_HEIGHT: u32 = 320;
const BAND: std::ops::Range<u32> = NATIVE_HEIGHT * 5 / 8..NATIVE_HEIGHT * 5 / 8 + NATIVE_HEIGHT / 8;

/// The synthetic worker's tools: an `ffmpeg` with `libx264rgb`
/// (`DEADPAN_BRIDGE_FFMPEG`, else Homebrew's), `deadpan-media-worker`
/// (`DEADPAN_MEDIA_WORKER`, else beside the tested `deadpan-cli`) and sibling
/// `deadpan-track`. A workspace build places the workers there. Without them
/// these tests skip, unless `DEADPAN_REQUIRE_SYNTHETIC_WORKER=1` (set by
/// `cargo xtask gate`) makes a missing tool a failure.
fn synthetic_tools() -> Option<synthetic::SyntheticWorker> {
    let ffmpeg = std::env::var_os("DEADPAN_BRIDGE_FFMPEG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/opt/homebrew/bin/ffmpeg"));
    let media_worker = std::env::var_os("DEADPAN_MEDIA_WORKER")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(recipes::cli_path()).with_file_name("deadpan-media-worker"));
    let landmark_worker = media_worker.with_file_name("deadpan-track");
    let missing: Vec<String> = [&ffmpeg, &media_worker, &landmark_worker]
        .into_iter()
        .filter(|tool| !tool.is_file())
        .map(|tool| tool.display().to_string())
        .collect();
    if missing.is_empty() {
        return Some(synthetic::SyntheticWorker {
            ffmpeg,
            media_worker,
            landmark_worker,
        });
    }
    let message = format!(
        "needs ffmpeg with libx264rgb and built deadpan-media-worker/deadpan-track; missing {missing:?}"
    );
    assert!(
        std::env::var_os("DEADPAN_REQUIRE_SYNTHETIC_WORKER").is_none_or(|value| value != "1"),
        "DEADPAN_REQUIRE_SYNTHETIC_WORKER=1: {message}"
    );
    eprintln!("skipped: {message}");
    None
}

fn run_variant(
    store: &mut ProjectStore,
    allocated: &attempt::Allocated,
    worker: &synthetic::SyntheticWorker,
) -> Result<BundleValidationReceipt> {
    let run = synthetic::run(
        allocated,
        worker,
        |_| {},
        |record| attempt::record(store, allocated, &record).map_err(|error| error.to_string()),
        &AtomicBool::new(false),
    );
    let finished = attempt::finish(store, allocated, run)?;
    assert_eq!(finished.state, JobState::Ready, "{:?}", finished.failure);
    Ok(finished.receipt.ok_or("Ready receipt")?)
}

/// Undo the head as the app does: reconciled against current requests.
fn undo(store: &mut ProjectStore, revision: &str) -> Result<RevisionId> {
    let head = store.head_revision()?;
    let next = RevisionId::new(revision)?;
    let before = store.snapshot()?;
    let after = store
        .preview_undo(&head, next.clone())?
        .edit
        .forward
        .apply(&before)?;
    let relevance = acceptance::relevance_plan(store, &before, &after)?;
    store.undo_reconciled(&head, next.clone(), &relevance)?;
    Ok(next)
}

/// The committed playback capability for the store's head, built as the
/// app's project service builds it: every qualified source with its receipt
/// and original record.
fn playback_snapshot(store: &ProjectStore) -> Result<Arc<deadpan_playback::Snapshot>> {
    let document = Arc::new(store.snapshot()?);
    let mut sources = BTreeMap::new();
    for (asset, record) in document.assets() {
        if record.source_qualification.is_none() {
            continue;
        }
        let receipt = Arc::new(store.registered_source(document.revision_id(), asset)?);
        let original = store
            .original_record(receipt.original().content())?
            .ok_or("original record")?;
        sources.insert(
            asset.clone(),
            deadpan_playback::SourceEntry { receipt, original },
        );
    }
    Ok(Arc::new(deadpan_playback::Snapshot::committed(
        1,
        document,
        sources,
        store.original_import_handle()?,
    )))
}

/// Accepting changes only the pause's visual provider: apart from the new
/// revision ID, the Hold's `video` and two added video-only assets (the
/// artifact's masters), the proposed document equals its base, including the
/// Hold's duration, audio policy and every other beat, sound and asset.
fn assert_picture_only(
    base: &ProjectDocument,
    proposed: &ProjectDocument,
    hold: &NodeId,
) -> Result {
    let HoldVideo::Generated { accepted } = (match &proposed.nodes()[hold].kind {
        NodeKind::Hold { recipe } => &recipe.video,
        _ => return Err("the pause is not a Hold".into()),
    }) else {
        return Err("the proposal does not show generated pictures".into());
    };
    let added: Vec<_> = proposed
        .assets()
        .keys()
        .filter(|asset| !base.assets().contains_key(*asset))
        .cloned()
        .collect();
    let mut expected = vec![
        accepted.artifact.native_asset.clone(),
        accepted.artifact.sampled_asset.clone(),
    ];
    expected.sort();
    assert_eq!(added, expected, "only the two masters are added");
    for asset in &added {
        let record = &proposed.assets()[asset];
        assert!(record.audio.is_none() && record.video.is_some());
    }
    let strip = |document: &ProjectDocument,
                 added: &[deadpan_core::AssetId]|
     -> Result<serde_json::Value> {
        let mut wire = serde_json::to_value(document)?;
        let fields = wire.as_object_mut().ok_or("document object")?;
        fields.remove("revision_id").ok_or("revision_id")?;
        let assets = fields
            .get_mut("assets")
            .and_then(serde_json::Value::as_object_mut)
            .ok_or("assets")?;
        for asset in added {
            assets.remove(asset.as_str());
        }
        wire["nodes"][hold.as_str()]["kind"]["recipe"]
            .as_object_mut()
            .ok_or("Hold recipe")?
            .remove("video")
            .ok_or("Hold video")?;
        Ok(wire)
    };
    assert_eq!(strip(base, &added)?, strip(proposed, &added)?);
    Ok(())
}

struct Scenario {
    _root: tempfile::TempDir,
    package: PathBuf,
    /// The base edit before the pause was inserted.
    base: RevisionId,
    /// Edit duration of the base revision, in project frames.
    base_frames: i64,
    /// The pause inserted as a Freeze of L.
    freeze: RevisionId,
    accepted_first: RevisionId,
    undone: RevisionId,
    accepted_second: RevisionId,
    first: BundleValidationReceipt,
    second: BundleValidationReceipt,
}

/// Which real Original the pause is inserted into.
#[derive(Clone, Copy)]
enum Media {
    /// `cfr-bframes.mp4` through the `freeze_hold` recipe: a burnt-in frame
    /// counter, and audio that is silent except for one click.
    Counter,
    /// A generated Original whose audio is continuous, speech-like and
    /// aperiodic, so both seams cut through sound.
    Speech,
}

/// A 60-frame 320x180 `testsrc2` Original at 30000/1001 with 48 kHz stereo
/// AAC: an amplitude-modulated chirp (syllable-rate envelope, rising pitch),
/// never silent and never periodic, with a different right channel.
fn speech_media(directory: &Path, ffmpeg: &Path) -> Result<PathBuf> {
    let output = directory.join("speech.mp4");
    let voice = |gain: f64, base: u32| {
        format!("{gain}*(0.55+0.45*sin(2*PI*3.7*t))*sin(2*PI*({base}*t+45*t*t))")
    };
    let audio = format!(
        "aevalsrc=exprs='{}|{}':s=48000:d=2.002:c=stereo",
        voice(0.5, 180),
        voice(-0.4, 233)
    );
    let status = std::process::Command::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args(["-f", "lavfi", "-i", "testsrc2=size=320x180:rate=30000/1001"])
        .args(["-f", "lavfi", "-i", &audio])
        .args(["-frames:v", "60"])
        .args([
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-profile:v",
            "high",
        ])
        .args([
            "-preset", "medium", "-crf", "20", "-threads", "1", "-bf", "2",
        ])
        .args([
            "-x264-params",
            "colorprim=bt709:transfer=bt709:colormatrix=bt709",
        ])
        .args(["-color_primaries", "bt709", "-color_trc", "bt709"])
        .args(["-colorspace", "bt709", "-color_range", "tv"])
        .args(["-chroma_sample_location", "left"])
        .args(["-video_track_timescale", "30000"])
        .args(["-c:a", "aac", "-b:a", "192k", "-t", "2.002"])
        .arg(&output)
        .status()?;
    if !status.success() {
        return Err(format!("ffmpeg could not generate the speech Original: {status}").into());
    }
    Ok(output)
}

/// Create a project from `media` and insert the native `,h` Freeze pause of
/// N frames at Edit boundary F, as `freeze_hold` does. Returns the package,
/// the base and the Freeze revisions.
fn insert_freeze(directory: &Path, media: &Path) -> Result<(PathBuf, RevisionId, RevisionId)> {
    let package = directory.join("speech.deadpan");
    recipes::success(&[
        "project",
        "create-original",
        package.to_str().ok_or("UTF-8")?,
        media.to_str().ok_or("UTF-8")?,
    ])?;
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let document = store.snapshot()?;
    let base = document.revision_id().clone();
    let freeze = RevisionId::new("speech-freeze")?;
    let at = ProjectFrame(F);
    let plan = deadpan_plan::RenderPlan::compile(&document)?;
    let provider = deadpan_cli::pause::pause_provider(&document, &plan, at, &mut |asset| {
        store
            .source_video_index(document.revision_id(), asset)
            .map(Arc::new)
            .map_err(|error| error.to_string())
    })?;
    if !matches!(provider.video, HoldVideo::Freeze { .. }) {
        return Err("the pause provider did not freeze the Original".into());
    }
    let identities = match document.insert_time_target(at)?.split {
        Some(split) => (0..split.required_ids)
            .map(|index| NodeId::new(format!("speech-split-{index}")))
            .collect::<std::result::Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };
    let command = deadpan_core::Command::InsertTime {
        at,
        hold: deadpan_core::HoldRecipe {
            picture_context: provider.picture_context,
            duration: deadpan_core::FrameDuration::new(N)?,
            video: provider.video,
            audio: deadpan_core::HoldAudio::Silence,
        },
        id: NodeId::new("freeze")?,
        identities: deadpan_core::SplitIdentities { nodes: identities },
        timing: deadpan_core::AudioTimingId {
            allocation: freeze.clone(),
            ordinal: 0,
        },
    };
    let request = directory.join("insert.json");
    std::fs::write(
        &request,
        serde_json::to_vec(&serde_json::json!({
            "protocol": 1,
            "project_id": document.project_id(),
            "expected_revision": base,
            "new_revision": freeze,
            "command": command,
        }))?,
    )?;
    drop(store);
    recipes::success(&[
        "command",
        package.to_str().ok_or("UTF-8")?,
        "--json",
        request.to_str().ok_or("UTF-8")?,
    ])?;
    Ok((package, base, freeze))
}

/// Build the scenario, or None when the synthetic tools are absent.
fn scenario(media: Media) -> Result<Option<Scenario>> {
    let Some(worker) = synthetic_tools() else {
        return Ok(None);
    };
    let root = tempfile::tempdir()?;
    let fixture = root.path().canonicalize()?.join("fixture");
    let (package, base, freeze) = match media {
        Media::Counter => {
            let fixture = recipes::freeze_hold(&fixture)?;
            // freeze_hold commits two shortening deletions, then the insertion.
            assert_eq!(fixture.revision, "freeze-hold-r03");
            (
                fixture.package.clone(),
                RevisionId::new("freeze-hold-r02")?,
                RevisionId::new(fixture.revision.clone())?,
            )
        }
        Media::Speech => {
            std::fs::create_dir_all(&fixture)?;
            let media = speech_media(&fixture, &worker.ffmpeg)?;
            insert_freeze(&fixture, &media)?
        }
    };
    let hold = NodeId::new("freeze")?;
    let base_frames = {
        let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
        let before = store.snapshot_at(&base)?;
        let after = store.snapshot_at(&freeze)?;
        let frames = before.duration()?.frames();
        assert!(frames > F + 1);
        assert_eq!(after.duration()?.frames(), frames + N);
        assert!(matches!(
            &after.nodes()[&hold].kind,
            NodeKind::Hold { recipe }
                if recipe.duration.frames() == N && matches!(recipe.video, HoldVideo::Freeze { .. })
        ));
        frames
    };

    let cancelled = AtomicBool::new(false);
    let inputs = conditioning::prepare(&package, &freeze, &hold, &cancelled)?;
    let mut store = ProjectStore::open(&package, AccessMode::ReadWrite)?;
    let first_attempt = attempt::allocate(
        &mut store,
        AllocateInput {
            hold: hold.clone(),
            expected_revision: freeze.clone(),
            seed: 11,
            inputs: inputs.into(),
        },
    )?;
    let first = run_variant(&mut store, &first_attempt, &worker)?;
    let second_attempt = attempt::allocate_variant(
        &mut store,
        first_attempt.request.clone(),
        first_attempt.inputs().clone(),
    )?;
    let second = run_variant(&mut store, &second_attempt, &worker)?;
    assert_ne!(first.sampled_object(), second.sampled_object());
    let request = first_attempt.request.request_id.clone();

    store.select_generation_bundle_variant(&first_attempt.identity)?;
    let accepted_first = RevisionId::new("accepted-first")?;
    acceptance::accept(&mut store, &request, accepted_first.clone())?;
    let undone = undo(&mut store, "undone-first")?;
    assert!(matches!(
        &store.snapshot()?.nodes()[&hold].kind,
        NodeKind::Hold { recipe } if matches!(recipe.video, HoldVideo::Freeze { .. })
    ));

    // The proposal the app auditions before accepting the second variant.
    store.select_generation_bundle_variant(&second_attempt.identity)?;
    let accepted_second = RevisionId::new("accepted-second")?;
    let proposal = acceptance::acceptance_for(&store, &request, accepted_second.clone())?;
    let before = store.snapshot()?;
    let proposed_second = store
        .preview_generation_acceptance(&proposal, attempt::object_limits())?
        .forward
        .apply(&before)?;
    let base_playback = playback_snapshot(&store)?;
    deadpan_playback::Snapshot::proposed_generated(
        &base_playback,
        Arc::new(proposed_second.clone()),
        1,
        1,
    )?;
    assert_picture_only(&before, &proposed_second, &hold)?;
    acceptance::accept(&mut store, &request, accepted_second.clone())?;
    drop(store);
    Ok(Some(Scenario {
        _root: root,
        package,
        base,
        base_frames,
        freeze,
        accepted_first,
        undone,
        accepted_second,
        first,
        second,
    }))
}

fn rgb(frame: &Rgba8Frame) -> RgbPicture {
    let metadata = frame.metadata();
    let (width, height) = (metadata.width, metadata.height);
    let stride = metadata.row_stride_bytes as usize;
    let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
    for row in frame.bytes().chunks_exact(stride).take(height as usize) {
        for pixel in row[..width as usize * 4].chunks_exact(4) {
            rgb.extend_from_slice(&pixel[..3]);
        }
    }
    RgbPicture { width, height, rgb }
}

/// Rows outside the synthetic band.
fn without_band(picture: &RgbPicture) -> Vec<u8> {
    let row = picture.width as usize * 3;
    picture
        .rgb
        .chunks_exact(row)
        .enumerate()
        .filter(|(y, _)| !BAND.contains(&(*y as u32)))
        .flat_map(|(_, bytes)| bytes.iter().copied())
        .collect()
}

/// One decoded master's frames, uncropped, and each frame as presentation
/// crops it.
struct Master {
    raw: Vec<Rgba8Frame>,
}

impl Master {
    fn open(
        handle: &GeneratedReadHandle,
        object: &deadpan_core::GeneratedObjectRef,
        frames: u32,
        size: (u32, u32),
    ) -> Result<Self> {
        let cancelled = AtomicBool::new(false);
        let mut source = open_candidate_master(handle, object, frames, size, &cancelled)?;
        let ids: Vec<SourceFrameId> = source
            .index()
            .index()
            .frames()
            .iter()
            .map(|frame| frame.identity)
            .collect();
        assert_eq!(ids.len(), frames as usize);
        let mut raw = Vec::new();
        for id in ids {
            let decoded = source.frame(id, Duration::from_secs(15), &cancelled)?;
            raw.push(source_to_render_frame(decoded, source.info())?);
        }
        Ok(Self { raw })
    }
}

/// The receipt's two masters, decoded from verified store snapshots.
fn masters(store: &ProjectStore, receipt: &BundleValidationReceipt) -> Result<(Master, Master)> {
    let handle = store.generated_read_handle();
    let video = receipt.native_video();
    let size = (video.width(), video.height());
    let native = Master::open(
        &handle,
        receipt.native_object(),
        u32::try_from(video.frames().frames())?,
        size,
    )?;
    let sampled = Master::open(
        &handle,
        receipt.sampled_object(),
        u32::try_from(receipt.sampled_video().frames().frames())?,
        size,
    )?;
    Ok((native, sampled))
}

/// What a committed revision presents at one frame.
enum Shown {
    Original { id: SourceFrameId, bytes: Vec<u8> },
    Generated { id: SourceFrameId, bytes: Vec<u8> },
}

fn shown(package: &Path, revision: &RevisionId, frames: &[i64]) -> Result<Vec<Shown>> {
    let cancelled = AtomicBool::new(false);
    let mut session = ProjectPictureSession::open_revision(package, revision, None, &cancelled)?;
    frames
        .iter()
        .map(|frame| {
            let prepared = session.prepare(ProjectFrame(*frame), &cancelled)?;
            Ok(match prepared.picture {
                PreparedPicture::Frame { id, frame, .. } => Shown::Original {
                    id,
                    bytes: frame.bytes().to_vec(),
                },
                PreparedPicture::Generated { id, frame, .. } => Shown::Generated {
                    id,
                    bytes: frame.bytes().to_vec(),
                },
                PreparedPicture::Background => return Err("unexpected background".into()),
            })
        })
        .collect()
}

/// The committed picture at one frame, uncomposed, as the reader decodes it.
fn boundary_frame(package: &Path, revision: &RevisionId, frame: i64) -> Result<Rgba8Frame> {
    let cancelled = AtomicBool::new(false);
    let mut session = ProjectPictureSession::open_revision(package, revision, None, &cancelled)?;
    match session.prepare(ProjectFrame(frame), &cancelled)?.picture {
        PreparedPicture::Frame { frame, .. } | PreparedPicture::Generated { frame, .. } => {
            Ok(frame)
        }
        PreparedPicture::Background => Err("unexpected background".into()),
    }
}

fn copy(frame: &Rgba8Frame) -> Rgba8Frame {
    Rgba8Frame::new(*frame.metadata(), frame.bytes().to_vec()).expect("valid frame")
}

fn original(shown: &Shown) -> (SourceFrameId, &[u8]) {
    match shown {
        Shown::Original { id, bytes } => (*id, bytes),
        Shown::Generated { .. } => panic!("expected an Original picture"),
    }
}

fn artifact(package: &Path, revision: &RevisionId) -> Result<GeneratedArtifact> {
    let document = ProjectStore::open(package, AccessMode::ReadOnly)?.snapshot_at(revision)?;
    match &document.nodes()[&NodeId::new("freeze")?].kind {
        NodeKind::Hold { recipe } => match &recipe.video {
            HoldVideo::Generated { accepted } => Ok(accepted.artifact.clone()),
            other => Err(format!("{revision} pause shows {other:?}").into()),
        },
        _ => Err("the pause is not a Hold".into()),
    }
}

/// Check one accepted revision's joins against its variant's masters.
fn check_accepted(
    scenario: &Scenario,
    revision: &RevisionId,
    receipt: &BundleValidationReceipt,
    left: (SourceFrameId, &[u8]),
    right: (SourceFrameId, &[u8]),
) -> Result {
    let store = ProjectStore::open(&scenario.package, AccessMode::ReadOnly)?;
    let canvas = {
        let basis = store.snapshot_at(revision)?.presentation_basis().clone();
        [basis.width, basis.height]
    };
    let artifact = artifact(&scenario.package, revision)?;
    assert_eq!(&artifact.sampled_object, receipt.sampled_object());
    assert_eq!(artifact.content_aspect, Some(canvas));

    // The sampling map retains exactly N interior positions (j+1)(M-1)/(N+1),
    // never a conditioning endpoint.
    let map = &artifact.sampling;
    let m = receipt.native_video().frames().frames();
    assert_eq!(map.output_frame_count().frames(), N);
    assert_eq!(map.native_frame_count().frames(), m);
    for j in 0..N {
        let position = map.native_position(j)?;
        assert_eq!(
            position,
            ExactRatio::new(i128::from((j + 1) * (m - 1)), i128::from(N + 1))?
        );
        // 0 < (j+1)(M-1)/(N+1) < M-1 because 1 <= j+1 <= N < N+1.
        assert!((1..=N).contains(&(j + 1)));
    }
    assert!(map.native_position(N).is_err());

    let (native, sampled) = masters(&store, receipt)?;
    // Sampled frame j is the half-up linear interpolation of the native
    // frames around its position.
    for (j, frame) in sampled.raw.iter().enumerate() {
        let numerator = (j as u64 + 1) * (m as u64 - 1);
        let denominator = N as u64 + 1;
        let (lower, remainder) = (numerator / denominator, numerator % denominator);
        let a = native.raw[lower as usize].bytes();
        let expected: Vec<u8> = if remainder == 0 {
            a.to_vec()
        } else {
            let b = native.raw[lower as usize + 1].bytes();
            a.iter()
                .zip(b)
                .map(|(a, b)| {
                    ((u64::from(*a) * (denominator - remainder)
                        + u64::from(*b) * remainder
                        + denominator / 2)
                        / denominator) as u8
                })
                .collect()
        };
        assert_eq!(frame.bytes(), expected.as_slice(), "sampled frame {j}");
    }

    // The conditioning endpoints are native frames 0 and M-1 (outside the
    // synthetic band), and neither appears among the presented pictures.
    let admission = receipt.admission().ok_or("admission evidence")?;
    let read_png = |reference| -> Result<RgbPicture> {
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(
            &mut store.snapshot_generated_object(reference, attempt::object_limits())?,
            &mut bytes,
        )?;
        Ok(joins::decode_png(&bytes, (768, NATIVE_HEIGHT))?)
    };
    let left_png = read_png(admission.inputs().left().expect("Bridge left input"))?;
    let right_png = read_png(admission.inputs().right().expect("Bridge right input"))?;
    assert_eq!(without_band(&rgb(&native.raw[0])), without_band(&left_png));
    assert_eq!(
        without_band(&rgb(native.raw.last().ok_or("native frames")?)),
        without_band(&right_png)
    );
    let first = rgb(&sampled.raw[0]);
    let last = rgb(sampled.raw.last().ok_or("sampled frames")?);
    for (frame, endpoint, name) in [
        (&first, &left_png, "first vs left"),
        (&first, &right_png, "first vs right"),
        (&last, &left_png, "last vs left"),
        (&last, &right_png, "last vs right"),
    ] {
        let a = without_band(frame);
        let b = without_band(endpoint);
        let differing = a.iter().zip(&b).filter(|(a, b)| a != b).count();
        assert!(
            differing > 0,
            "{name}: a generated frame copies a conditioning endpoint"
        );
    }

    // Advisory join measurement against the committed boundary pictures at
    // the request's origin revision and the presented (cropped) master.
    let cancelled = AtomicBool::new(false);
    let report = joins::measure_request_joins(
        &scenario.package,
        &store.generated_read_handle(),
        &scenario.freeze,
        &NodeId::new("freeze")?,
        receipt,
        &cancelled,
    )?;
    let region = joins::comparison_region(canvas, [768, NATIVE_HEIGHT]);
    assert_eq!(report.region, region);
    assert_eq!(region, aspect_region(canvas, [768, NATIVE_HEIGHT], [1, 1]));
    let before = boundary_frame(&scenario.package, &scenario.freeze, F - 1)?;
    let after = boundary_frame(&scenario.package, &scenario.freeze, F + N)?;
    let last_raw = sampled.raw.last().ok_or("sampled frames")?;
    assert_eq!(
        report,
        joins::measure_pictures(
            Some(&before),
            copy(&sampled.raw[0]),
            copy(last_raw),
            Some(&after),
            canvas
        )?,
        "the report measures these pictures"
    );
    for measure in [report.entry, report.exit] {
        let measure = measure.measure().expect("Bridge join is measured");
        assert!(measure.mean_abs_diff > 0.0 && measure.mean_abs_diff < 255.0);
        assert_eq!(measure.class, JoinClass::of(measure.mean_abs_diff));
    }
    eprintln!("{revision} joins: {}", serde_json::to_string(&report)?);
    // Known pairs on these real pictures: a master whose end frames are the
    // retained conditioning pictures (each boundary as conditioning placed
    // it in the native raster) joins Smoothly; their inverse is a Jump.
    let as_frame = |picture: &RgbPicture, invert: bool| -> Result<Rgba8Frame> {
        let rgba = picture
            .rgb
            .chunks_exact(3)
            .flat_map(|pixel| {
                let value = |code: u8| if invert { 255 - code } else { code };
                [value(pixel[0]), value(pixel[1]), value(pixel[2]), 255]
            })
            .collect();
        Ok(Rgba8Frame::new(*sampled.raw[0].metadata(), rgba)?)
    };
    let smooth = joins::measure_pictures(
        Some(&before),
        as_frame(&left_png, false)?,
        as_frame(&right_png, false)?,
        Some(&after),
        canvas,
    )?;
    eprintln!(
        "{revision} known smooth: {}",
        serde_json::to_string(&smooth)?
    );
    assert_eq!(
        smooth.entry.measure().unwrap().class,
        JoinClass::Smooth,
        "{smooth:?}"
    );
    assert_eq!(
        smooth.exit.measure().unwrap().class,
        JoinClass::Smooth,
        "{smooth:?}"
    );
    let jump = joins::measure_pictures(
        Some(&before),
        as_frame(&left_png, true)?,
        as_frame(&right_png, true)?,
        Some(&after),
        canvas,
    )?;
    assert_eq!(
        jump.entry.measure().unwrap().class,
        JoinClass::Jump,
        "{jump:?}"
    );
    assert_eq!(
        jump.exit.measure().unwrap().class,
        JoinClass::Jump,
        "{jump:?}"
    );

    // Hard cuts at exact frames through the committed picture reader.
    let mut frames = vec![F - 1];
    frames.extend(F..F + N);
    frames.push(F + N);
    let pictures = shown(&scenario.package, revision, &frames)?;
    assert_eq!(original(&pictures[0]), left, "f-1 is the unchanged L");
    assert_eq!(
        original(&pictures[frames.len() - 1]),
        right,
        "f+N is the unchanged R that was at f"
    );
    for (j, picture) in pictures[1..=N as usize].iter().enumerate() {
        let Shown::Generated { id, bytes } = picture else {
            panic!("frame {} is not generated", F + j as i64);
        };
        assert_eq!(id.0, j as u64, "Hold frame {j} shows sampled frame {j}");
        let expected = fill_canvas_aspect(copy(&sampled.raw[j]), canvas)?;
        assert_eq!(bytes.as_slice(), expected.bytes(), "Hold frame {j}");
    }
    Ok(())
}

#[test]
fn accepted_ai_pause_meets_the_original_with_exact_hard_cuts() -> Result {
    let Some(scenario) = scenario(Media::Counter)? else {
        return Ok(());
    };
    // L and R at the base revision, before the pause was inserted.
    let base = shown(&scenario.package, &scenario.base, &[F - 1, F])?;
    let (left, right) = (original(&base[0]), original(&base[1]));
    assert_ne!(left.1, right.1, "the boundary pictures differ");

    // The inserted Freeze: L everywhere in the pause.
    for revision in [&scenario.freeze, &scenario.undone] {
        let mut frames: Vec<i64> = (F - 1..=F + N).collect();
        frames.push(0);
        let pictures = shown(&scenario.package, revision, &frames)?;
        assert_eq!(original(&pictures[0]), left, "{revision} f-1");
        for picture in &pictures[1..=N as usize] {
            assert_eq!(original(picture), left, "{revision} freezes L");
        }
        assert_eq!(original(&pictures[N as usize + 1]), right, "{revision} f+N");
    }
    check_accepted(
        &scenario,
        &scenario.accepted_first,
        &scenario.first,
        left,
        right,
    )?;
    check_accepted(
        &scenario,
        &scenario.accepted_second,
        &scenario.second,
        left,
        right,
    )?;
    Ok(())
}

/// Stereo f32 PCM of one revision's whole Edit through the edge-faded bus.
fn edge_faded(package: &Path, revision: &RevisionId, end: i64) -> Result<Vec<[f32; 2]>> {
    let cancelled = AtomicBool::new(false);
    let mut session = ProjectAudioSession::open_revision(package, revision)?;
    let mut samples = Vec::new();
    let mut at = 0;
    while at < end {
        let count = (end - at).min(256);
        let block = session.read_edge_faded(AudioSample(at), u32::try_from(count)?, &cancelled)?;
        assert_eq!(&block.revision_id, revision);
        samples.extend(block.samples);
        at += count;
    }
    Ok(samples)
}

/// The limited bus that audition and export share, over the whole Edit.
fn limited(package: &Path, revision: &RevisionId, frames: i64) -> Result<Vec<[f32; 2]>> {
    let cancelled = AtomicBool::new(false);
    let mut session = OfflineAudioSession::open_revision(
        package,
        revision,
        FrameRange::new(ProjectFrame(0), ProjectFrame(frames))?,
        &cancelled,
        Instant::now() + Duration::from_secs(600),
    )?;
    let range = session.sample_range();
    let mut samples = Vec::new();
    let mut at = range.start.0;
    while at < range.end.0 {
        let count = (range.end.0 - at).min(8192);
        let block = session.read(AudioSample(at), u32::try_from(count)?, &cancelled)?;
        samples.extend(block.samples);
        at += count;
    }
    Ok(samples)
}

fn bits(samples: &[[f32; 2]]) -> Vec<[u32; 2]> {
    samples
        .iter()
        .map(|pair| [pair[0].to_bits(), pair[1].to_bits()])
        .collect()
}

/// `a[range]` equals `b` shifted by `shift` samples, bit for bit.
fn assert_shifted(
    a: &[[f32; 2]],
    b: &[[f32; 2]],
    range: std::ops::Range<i64>,
    shift: i64,
    what: &str,
) {
    let mismatch = range.clone().find(|at| {
        let x = a[*at as usize];
        let y = b[(at - shift) as usize];
        x[0].to_bits() != y[0].to_bits() || x[1].to_bits() != y[1].to_bits()
    });
    assert!(
        mismatch.is_none(),
        "{what}: sample {mismatch:?} differs over {range:?} (shift {shift})"
    );
}

/// Inserting the pause shifts later audio by exactly B(f+N) - B(f) samples,
/// and accepting, undoing or switching its pictures changes no sample.
/// Structural (bit equality and exact shift), not a listening judgement.
fn check_speech(media: Media) -> Result {
    let Some(scenario) = scenario(media)? else {
        return Ok(());
    };
    let package = &scenario.package;
    let store = ProjectStore::open(package, AccessMode::ReadOnly)?;
    let rate = store
        .snapshot_at(&scenario.freeze)?
        .presentation_basis()
        .frame_rate;
    drop(store);
    let boundary = |frame: i64| {
        rate.audio_boundary(ProjectFrame(frame))
            .map(|sample| sample.0)
    };
    let frames = scenario.base_frames;
    let (b_f, b_fn, b_base_end, b_end) = (
        boundary(F)?,
        boundary(F + N)?,
        boundary(frames)?,
        boundary(frames + N)?,
    );
    let shift = b_fn - b_f;
    assert_eq!(b_end - b_base_end, shift);

    let base = edge_faded(package, &scenario.base, b_base_end)?;
    let freeze = edge_faded(package, &scenario.freeze, b_end)?;
    let loud = |samples: &[[f32; 2]], range: std::ops::Range<i64>| {
        range
            .map(|at| {
                samples[at as usize][0]
                    .abs()
                    .max(samples[at as usize][1].abs())
            })
            .fold(0.0_f32, f32::max)
    };
    match media {
        // The click (Original sample 48,000, base Edit frame 17) lies after
        // the pause.
        Media::Counter => assert!(
            loud(&base, b_f..b_base_end) > 0.5,
            "the suffix holds the click"
        ),
        // Both seams cut through sound.
        Media::Speech => {
            assert!(loud(&base, b_f - 480..b_f) > 0.05, "sound before the cut");
            assert!(loud(&base, b_f..b_f + 480) > 0.05, "sound after the cut");
            assert!(
                loud(&base, b_base_end - 4800..b_base_end) > 0.05,
                "sound to the end"
            );
        }
    }

    // Picture providers never change audio: every accepted, undone and
    // switched revision is bit-identical to the Freeze revision.
    for revision in [
        &scenario.accepted_first,
        &scenario.undone,
        &scenario.accepted_second,
    ] {
        assert_eq!(
            bits(&edge_faded(package, revision, b_end)?),
            bits(&freeze),
            "{revision} edge-faded bus"
        );
    }

    // The Freeze revision against the original edit: unchanged before the
    // pause, silent inside it, and the suffix shifted by exactly
    // B(f+N) - B(f) samples, away from the seam fade windows (at most 96
    // samples each side of an edge, docs/AUDIO_EDGES.md). Inside those
    // windows a fade only attenuates.
    const FADE: i64 = 96;
    assert_shifted(&freeze, &base, 0..b_f - FADE, 0, "prefix");
    assert_eq!(
        loud(&freeze, b_f + FADE..b_fn - FADE),
        0.0,
        "the pause is silent"
    );
    assert_shifted(&freeze, &base, b_fn + FADE..b_end, shift, "suffix");
    for (at, from) in (b_f - FADE..b_f)
        .map(|at| (at, at))
        .chain((b_fn..b_fn + FADE).map(|at| (at, at - shift)))
    {
        for channel in 0..2 {
            assert!(
                freeze[at as usize][channel].abs() <= base[from as usize][channel].abs(),
                "seam sample {at} is louder than the original"
            );
        }
    }
    if let Media::Counter = media {
        let click = |samples: &[[f32; 2]]| {
            (0..samples.len())
                .max_by(|a, b| samples[*a][0].abs().total_cmp(&samples[*b][0].abs()))
                .map(|at| at as i64)
        };
        assert_eq!(
            click(&freeze),
            click(&base).map(|at| at + shift),
            "the click moves by exactly the pause"
        );
    }

    // The limited bus that audition and export share: identical across all
    // picture choices, and the suffix keeps its exact samples.
    let limited_freeze = limited(package, &scenario.freeze, frames + N)?;
    assert_eq!(limited_freeze.len() as i64, b_end);
    for revision in [
        &scenario.accepted_first,
        &scenario.undone,
        &scenario.accepted_second,
    ] {
        assert_eq!(
            bits(&limited(package, revision, frames + N)?),
            bits(&limited_freeze),
            "{revision} limited bus"
        );
    }
    let limited_base = limited(package, &scenario.base, frames)?;
    assert_shifted(
        &limited_freeze,
        &limited_base,
        0..b_f - FADE,
        0,
        "limited prefix",
    );
    assert_shifted(
        &limited_freeze,
        &limited_base,
        b_fn + FADE..b_end,
        shift,
        "limited suffix",
    );
    Ok(())
}

#[test]
fn accepting_rejecting_or_switching_ai_pictures_never_moves_the_click() -> Result {
    check_speech(Media::Counter)
}

#[test]
fn accepting_rejecting_or_switching_ai_pictures_never_moves_continuous_speech() -> Result {
    check_speech(Media::Speech)
}
