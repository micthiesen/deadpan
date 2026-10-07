//! HDR preview-versus-export verification (DP-17).
//!
//! Real one-Original projects from the HDR AV fixtures
//! (`hdr-pq-av.mp4`, `hdr-hlg-av.mp4`: 320x180, 30 fps, 60 frames, a
//! frame-30 marker and a 1 kHz click at Original sample 48000) and the grainy
//! real-like `hdr-pq-grain-av.mp4` and `hdr-hlg-grain-av.mp4` are edited
//! through the headless `command` API, exported and compared with
//! `verify-export` in ten-bit PQ/HLG code values. See
//! docs/PREVIEW_EXPORT_VERIFICATION.md and
//! docs/qualification/hdr-preview-export-2026-10-05.md.
//!
//! Expectations (picture provenance, branch decision, tags) are derived from
//! the recipe and the fixture generator, never from the compiled plan.

#![cfg(target_os = "macos")]

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Output, Stdio};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_cli::export_verification::{ReferencePicture, reference_pictures};
use deadpan_cli::picture::{
    AssetColor, AssetTransfer, OutputColorReason, committed_color, decide_output_color,
};
use deadpan_core::{
    AudioTimingId, BeatNode, Caption, ColorPolicy, Command, CommandRequest, ContentLight,
    FrameDuration, FrameRange, HoldAudio, HoldRecipe, HoldVideo, MasteringDisplay, NodeId,
    NodeKind, ProjectDocument, ProjectFrame, RevisionId, SplitIdentities, Subtree,
    WrapAnchorPolicy,
};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::{Value, json};

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

const FIXTURES: &str = "../../native/deadpan-source/tests/fixtures";

/// Mastering volume written by `generate_hdr_fixtures.py` for the PQ fixture:
/// R (0.68, 0.32), G (0.265, 0.69), B (0.15, 0.06), D65, 1000/0.0001 cd/m².
const PQ_FIXTURE_MASTERING: MasteringDisplay = MasteringDisplay {
    primaries: [[34_000, 16_000], [13_250, 34_500], [7_500, 3_000]],
    white_point: [15_635, 16_450],
    max_luminance: 10_000_000,
    min_luminance: 1,
};

/// Recipe fixtures: name, Original and container transfer characteristic.
/// The grain fixtures are real-like footage with clumped film-grain noise
/// (see `generate_hdr_fixtures.py`), the hardest case for PSNR.
const HDR_FIXTURES: [(&str, &str, u16); 4] = [
    ("hdr-pq", "hdr-pq-av.mp4", 16),
    ("hdr-hlg", "hdr-hlg-av.mp4", 18),
    ("hdr-pq-grain", "hdr-pq-grain-av.mp4", 16),
    ("hdr-hlg-grain", "hdr-hlg-grain-av.mp4", 18),
];

fn cli_path() -> &'static str {
    env!("CARGO_BIN_EXE_deadpan-cli")
}

fn cli(arguments: &[&str]) -> Result<Output> {
    let mut command = Process::new(cli_path());
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    Ok(deadpan_native_process::spawn(&mut command)?.wait_with_output()?)
}

fn success(arguments: &[&str]) -> Result<Value> {
    let output = cli(arguments)?;
    if !output.status.success() {
        return Err(format!(
            "deadpan-cli {arguments:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn fixture_media(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURES)
        .join(name)
}

fn node(value: &str) -> Result<NodeId> {
    Ok(NodeId::new(value)?)
}

/// One single-Original package edited through the CLI.
struct Project {
    name: &'static str,
    directory: PathBuf,
    package: PathBuf,
    step: u32,
    ids: u32,
}

impl Project {
    fn create(directory: &Path, name: &'static str, media: &str) -> Result<Self> {
        fs::create_dir_all(directory)?;
        let directory = directory.canonicalize()?;
        let original = directory.join(media);
        fs::copy(fixture_media(media), &original)?;
        let package = directory.join(format!("{name}.deadpan"));
        success(&[
            "project",
            "create-original",
            package.to_str().ok_or("UTF-8 path")?,
            original.to_str().ok_or("UTF-8 path")?,
        ])?;
        Ok(Self {
            name,
            directory,
            package,
            step: 0,
            ids: 0,
        })
    }

    fn path(&self) -> &str {
        self.package.to_str().expect("UTF-8 package path")
    }

    fn document(&self) -> Result<ProjectDocument> {
        Ok(ProjectStore::open(&self.package, AccessMode::ReadOnly)?.snapshot()?)
    }

    fn fresh(&mut self, count: usize) -> Result<Vec<NodeId>> {
        (0..count)
            .map(|_| {
                self.ids += 1;
                node(&format!("{}-n{}", self.name, self.ids))
            })
            .collect()
    }

    fn apply(
        &mut self,
        build: impl FnOnce(&mut Self, &ProjectDocument, &RevisionId) -> Result<Command>,
    ) -> Result<ProjectDocument> {
        let document = self.document()?;
        self.step += 1;
        let revision = RevisionId::new(format!("{}-r{:02}", self.name, self.step))?;
        let command = build(self, &document, &revision)?;
        let request = self.directory.join(format!("request-{revision}.json"));
        fs::write(
            &request,
            serde_json::to_vec(&json!({
                "protocol": 1,
                "project_id": document.project_id(),
                "expected_revision": document.revision_id(),
                "new_revision": revision,
                "command": command,
            }))?,
        )?;
        success(&[
            "command",
            self.path(),
            "--json",
            request.to_str().ok_or("UTF-8")?,
        ])?;
        let saved = self.document()?;
        if saved.revision_id() != &revision {
            return Err(format!("{revision} was not committed").into());
        }
        Ok(saved)
    }

    fn delete_range(&mut self, start: i64, end: i64) -> Result<ProjectDocument> {
        self.apply(|project, document, revision| {
            let range = FrameRange::new(ProjectFrame(start), ProjectFrame(end))?;
            let required = document
                .range_deletion(document.root(), range)?
                .required_ids;
            Ok(Command::DeleteRange {
                parent: document.root().clone(),
                range,
                identities: SplitIdentities {
                    nodes: project.fresh(required)?,
                },
                timing: AudioTimingId {
                    allocation: revision.clone(),
                    ordinal: 0,
                },
            })
        })
    }

    fn split_root(&mut self, at: i64) -> Result<ProjectDocument> {
        self.apply(|project, document, _| {
            let (child, start) = root_child_at(document, at)?;
            Ok(Command::Split {
                node: child,
                at: FrameDuration::new(at - start)?,
                identities: SplitIdentities {
                    nodes: project.fresh(16)?,
                },
            })
        })
    }
}

fn root_children(document: &ProjectDocument) -> Result<Vec<NodeId>> {
    match &document.nodes()[document.root()].kind {
        NodeKind::Sequence { children } => Ok(children.clone()),
        _ => Err("root is not a Sequence".into()),
    }
}

fn root_child_at(document: &ProjectDocument, at: i64) -> Result<(NodeId, i64)> {
    let durations = document.durations()?;
    let mut start = 0;
    for child in root_children(document)? {
        let end = start + durations[&child].frames();
        if (start..end).contains(&at) {
            return Ok((child, start));
        }
        start = end;
    }
    Err(format!("no root child contains Edit frame {at}").into())
}

/// A built HDR recipe and its independently derived expectations.
struct Fixture {
    name: &'static str,
    package: PathBuf,
    revision: String,
    /// The revision before the caption, for the graphics-white check.
    uncaptioned: String,
    frames: u64,
    /// Output frame to Original decoded ordinal.
    expectations: Vec<(u64, u64)>,
}

/// Cut, repeat, freeze and caption the 60-frame HDR Original:
///
/// 1. Keep Original [10, 40): Edit `f` shows Original `10 + f`.
/// 2. Two total plays of Edit [15, 21) (Original 25..31, holding the frame-30
///    marker and the click at Original sample 48000): 36 frames.
/// 3. A 6-frame silent freeze inserted at Edit 6, holding the preceding
///    picture (Original 15): 42 frames.
/// 4. A white caption on Edit [0, 4), BT.2408 graphics white in HDR.
///
/// Final Edit 0..6 shows Original 10..16; 6..12 freezes 15; 12..21 shows
/// 16..25; plays at 21 and 27 show 25..31 (marker at 26 and 32); 33..42
/// shows 31..40. The click sounds at Edit 26 and 32.
fn hdr_recipe(directory: &Path, name: &'static str, media: &str) -> Result<Fixture> {
    let mut project = Project::create(directory, name, media)?;
    project.delete_range(40, 60)?;
    let base = project.delete_range(0, 10)?;
    assert_eq!(base.duration()?.frames(), 30, "{name} base");
    project.split_root(15)?;
    project.split_root(21)?;
    project.apply(|_, document, _| {
        Ok(Command::WrapRepeat {
            node: root_child_at(document, 15)?.0,
            id: node("repeat")?,
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        })
    })?;
    let at = ProjectFrame(6);
    project.apply(|project, document, revision| {
        let store = ProjectStore::open(&project.package, AccessMode::ReadOnly)?;
        let plan = deadpan_plan::RenderPlan::compile(document)?;
        let provider = deadpan_cli::pause::pause_provider(document, &plan, at, &mut |asset| {
            store
                .source_video_index(document.revision_id(), asset)
                .map(Arc::new)
                .map_err(|error| error.to_string())
        })?;
        if !matches!(provider.video, HoldVideo::Freeze { .. }) {
            return Err("the pause provider did not freeze the Original".into());
        }
        let target = document.insert_time_target(at)?;
        let identities = match target.split {
            Some(split) => project.fresh(split.required_ids)?,
            None => Vec::new(),
        };
        Ok(Command::InsertTime {
            at,
            hold: HoldRecipe {
                picture_context: provider.picture_context,
                duration: FrameDuration::new(6)?,
                video: provider.video,
                audio: HoldAudio::Silence,
            },
            id: node("freeze")?,
            identities: SplitIdentities { nodes: identities },
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })?;
    let uncaptioned = project.document()?.revision_id().to_string();
    let saved = project.apply(|_, document, _| {
        let (beat, _) = root_child_at(document, 0)?;
        let (host, offset) =
            deadpan_core::cutaway_host(document, &beat).ok_or("Edit 0 hosts no captions")?;
        Ok(Command::SetCaptions {
            node: host,
            captions: vec![Caption {
                range: FrameRange::new(ProjectFrame(offset), ProjectFrame(offset + 4))?,
                text: "HDR".into(),
                placement: Default::default(),
                reveal: None,
            }],
        })
    })?;
    let frames = u64::try_from(saved.duration()?.frames())?;
    assert_eq!(frames, 42, "{name} final duration");
    Ok(Fixture {
        name,
        package: project.package.clone(),
        revision: saved.revision_id().to_string(),
        uncaptioned,
        frames,
        expectations: vec![
            (0, 10),
            (5, 15),
            (6, 15),
            (11, 15),
            (12, 16),
            (20, 24),
            (21, 25),
            (26, 30),
            (27, 25),
            (32, 30),
            (33, 31),
            (41, 39),
        ],
    })
}

/// Run `verify-export`; returns the report and whether the command passed.
fn verify(package: &Path, movie: &Path, revision: &str, extra: &[&str]) -> Result<(Value, bool)> {
    let mut arguments = vec![
        "verify-export",
        package.to_str().ok_or("UTF-8")?,
        "--movie",
        movie.to_str().ok_or("UTF-8")?,
        "--revision",
        revision,
    ];
    arguments.extend_from_slice(extra);
    let output = cli(&arguments)?;
    let report: Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "verify-export {}: {error}: {}",
            movie.display(),
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    assert_eq!(report["passed"], output.status.success());
    Ok((report, output.status.success()))
}

/// Public headless Render of the fixture's committed revision.
fn public_render(package: &Path, revision: &str, output: &Path, name: &str) -> Result<PathBuf> {
    fs::create_dir_all(output)?;
    let result = cli(&[
        "render",
        package.to_str().ok_or("UTF-8")?,
        "--output",
        output.to_str().ok_or("UTF-8")?,
        "--name",
        name,
        "--expected",
        revision,
    ])?;
    let stdout = String::from_utf8(result.stdout)?;
    if !result.status.success() {
        return Err(format!(
            "render {name}: {}\n{}",
            String::from_utf8_lossy(&result.stderr),
            stdout.lines().last().unwrap_or_default()
        )
        .into());
    }
    let finished: Value = serde_json::from_str(stdout.lines().last().ok_or("render events")?)?;
    assert_eq!(finished["event"], "finished");
    assert_eq!(finished["status"]["outcome"], "published");
    assert_eq!(finished["status"]["captured_revision"], revision);
    Ok(PathBuf::from(
        finished["status"]["receipt"]["movie"]
            .as_str()
            .ok_or("published movie")?,
    ))
}

/// Encode the committed revision through the real encoded render worker
/// (the `deadpan-cli` executable), bypassing automatic encoder admission,
/// finished-file verification and publication. This isolates the emitted
/// HEVC Main10 movie for the preview-versus-export comparison.
fn worker_render(package: &Path, revision: &str, movie: &Path) -> Result {
    use deadpan_cli::encoded_render::{EncodedWorkerLimits, encode, protocol::EncoderChoice};
    use deadpan_cli::render_worker::{
        RenderPictureRequest, RenderWorkerRuntime, protocol::RenderIdentity,
    };
    let runtime = RenderWorkerRuntime {
        executable: fs::canonicalize(cli_path())?,
        arguments: Vec::new(),
        environment: BTreeMap::new(),
    };
    let request = RenderPictureRequest {
        package: package.to_owned(),
        revision: RevisionId::new(revision)?,
        range: None,
        identity: RenderIdentity {
            request_id: deadpan_jobs::RequestId::new(format!("hdr-{revision}"))?,
            attempt_id: deadpan_jobs::AttemptId::new(format!("hdr-attempt-{revision}"))?,
        },
        cancellation_token: deadpan_jobs::CancellationToken::new(format!("hdr-cancel-{revision}"))?,
    };
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(600);
    let mut candidate = encode(
        &runtime,
        request,
        EncoderChoice {
            mode: deadpan_encode::EncoderMode::Hardware,
            b_frames: deadpan_encode::BFramePolicy::TargetTwo,
        },
        EncodedWorkerLimits::default(),
        &cancelled,
        deadline,
        |_| {},
    )
    .map_err(|error| format!("encoded worker: {error:?}"))?;
    let mut file = fs::File::create(movie)?;
    let mut offset = 0;
    let mut buffer = vec![0; 64 * 1024];
    while offset < candidate.byte_length() {
        let read = candidate.read_at(offset, &mut buffer, &cancelled, deadline)?;
        if read == 0 {
            return Err("encoded candidate ended early".into());
        }
        file.write_all(&buffer[..read])?;
        offset += u64::try_from(read)?;
    }
    file.sync_all()?;
    Ok(())
}

fn keep_or(scratch: tempfile::TempDir) -> (PathBuf, Option<tempfile::TempDir>) {
    if std::env::var_os("DEADPAN_PREVIEW_EXPORT_KEEP").is_some() {
        (scratch.keep(), None)
    } else {
        (scratch.path().to_owned(), Some(scratch))
    }
}

/// Assertions shared by every HDR movie verification.
fn check_hdr_report(fixture: &Fixture, report: &Value, transfer: u16) -> Result {
    let pq = transfer == 16;
    assert!(
        report["passed"].as_bool().unwrap_or(false),
        "{}: {}",
        fixture.name,
        report["failures"]
    );
    assert_eq!(report["schema_version"], 3);
    assert_eq!(report["picture_bits"], 10);
    let decision = &report["output_color"];
    assert_eq!(
        decision["output"],
        if pq {
            "hdr_rec2020_pq"
        } else {
            "hdr_rec2020_hlg"
        }
    );
    assert_eq!(decision["reason"], "hdr_sources");
    assert_eq!(decision["tone_map_peak_nits"], 1000);
    let color = &report["movie"]["color"];
    assert_eq!(color["container"], json!([9, transfer, 9]), "{color}");
    assert_eq!(color["container_full_range"], false);
    assert_eq!(color["sample_entry"], "hvc1");
    assert_eq!(color["hevc_profile_idc"], 2);
    assert_eq!(color["decoder_profile"], 2);
    assert_eq!(color["chroma_location"], "Left");
    assert_eq!(color["problems"], json!([]));
    if pq {
        assert_eq!(
            decision["mastering"],
            serde_json::to_value(PQ_FIXTURE_MASTERING)?
        );
        assert_eq!(color["mastering"], decision["mastering"]);
        assert!(color["content_light"].is_object(), "{color}");
        assert_eq!(report["hdr_light"]["complete"], true);
        assert_eq!(
            report["hdr_light"]["matches"], true,
            "{}",
            report["hdr_light"]
        );
    } else {
        assert!(decision["mastering"].is_null());
        assert!(color.get("mastering").is_none() && color.get("content_light").is_none());
        assert!(report.get("hdr_light").is_none());
    }
    assert_eq!(report["movie"]["video_frames"], fixture.frames);
    assert_eq!(report["summary"]["pictures_checked"], fixture.frames);
    let pictures = report["pictures"].as_array().ok_or("pictures")?;
    for (frame, ordinal) in &fixture.expectations {
        let picture = pictures
            .iter()
            .find(|picture| picture["output_frame"] == *frame)
            .ok_or_else(|| format!("{} frame {frame} not compared", fixture.name))?;
        assert_eq!(picture["provenance"]["kind"], "original", "{picture}");
        assert_eq!(
            picture["provenance"]["source_frame"], *ordinal,
            "{} frame {frame}",
            fixture.name
        );
    }
    assert_eq!(report["summary"]["nonzero_offsets"], 0);
    assert!(
        report["summary"]["offset_verified_windows"].as_u64() > Some(0),
        "{}",
        report["summary"]
    );
    for window in report["audio"].as_array().ok_or("audio")? {
        if window["kind"] == "signal" {
            assert_eq!(window["offset_status"], "verified_zero", "{window}");
            assert_eq!(window["measured_offset_samples"], 0, "{window}");
        }
    }
    Ok(())
}

fn summary_row(fixture: &Fixture, path: &str, report: &Value) -> Value {
    let summary = &report["summary"];
    json!({
        "fixture": fixture.name,
        "path": path,
        "frames": fixture.frames,
        "passed": report["passed"],
        "output": report["output_color"]["output"],
        "min_luma_psnr_db": summary["min_luma_psnr_db"],
        "min_chroma_psnr_db": summary["min_chroma_psnr_db"],
        "max_thumbnail_mad": summary["max_thumbnail_mad"],
        "max_local_luma_error": summary["max_local_luma_error"],
        "min_neighbor_margin_db": summary["min_neighbor_margin_db"],
        "min_block_snr_db": summary["min_block_snr_db"],
        "max_block_level_db": summary["max_block_level_db"],
        "offset_verified_windows": summary["offset_verified_windows"],
        "nonzero_offsets": summary["nonzero_offsets"],
        "content_light": report["movie"]["color"]["content_light"],
        "hdr_light": report["hdr_light"],
        "movie_bytes": report["movie"]["bytes"],
        "failures": report["failures"],
    })
}

fn record(rows: &[Value]) -> Result {
    for row in rows {
        eprintln!("{row}");
    }
    if let Some(path) = std::env::var_os("DEADPAN_PREVIEW_EXPORT_HDR_RESULTS") {
        fs::write(path, serde_json::to_vec_pretty(rows)?)?;
    }
    Ok(())
}

/// Per-picture measurements of a negative verification, for the record.
fn negative_row(fixture: &Fixture, negative: &str, report: &Value) -> Value {
    let pictures: Vec<Value> = report["pictures"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|picture| {
            json!({
                "frame": picture["output_frame"],
                "luma_psnr_db": picture["planes"][0]["psnr_db"],
                "local_luma_error": picture["local_luma_error"],
                "thumbnail_mad": picture["thumbnail_mad"],
                "flags": picture["flags"],
            })
        })
        .collect();
    json!({"fixture": fixture.name, "negative": negative, "pictures": pictures})
}

/// Negative: the same movie against a later revision with a static 1.35x
/// zoom must fail on content, proving the ten-bit comparison discriminates.
fn reframe_is_reported(fixture: &Fixture, movie: &Path) -> Result<Value> {
    let mut project = Project {
        name: fixture.name,
        directory: fixture.package.parent().ok_or("parent")?.to_owned(),
        package: fixture.package.clone(),
        step: 90,
        ids: 9000,
    };
    let half = deadpan_core::ExactRatio::new(1, 2)?;
    let zoom = deadpan_core::FramingPose::new(half, half, deadpan_core::ExactRatio::new(27, 20)?)?;
    let changed = project.apply(|_, document, _| {
        Ok(Command::SetFraming {
            node: root_child_at(document, 36)?.0,
            framing: Some(deadpan_core::Framing::static_pose(zoom)?),
        })
    })?;
    let (stale, passed) = verify(
        &fixture.package,
        movie,
        changed.revision_id().as_str(),
        &["--frames", "2,24,37,40", "--no-audio"],
    )?;
    assert!(!passed, "{stale}");
    let flags = |frame: u64| -> Vec<Value> {
        stale["pictures"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|picture| picture["output_frame"] == frame)
            .and_then(|picture| picture["flags"].as_array().cloned())
            .unwrap_or_default()
    };
    assert!(flags(2).is_empty() && flags(24).is_empty(), "{stale}");
    for frame in [37, 40] {
        assert!(
            flags(frame).contains(&json!("gross_structural_mismatch")),
            "{stale}"
        );
    }
    Ok(negative_row(fixture, "reframe_1_35x", &stale))
}

/// The ten-bit comparison against movies from the real encoded render
/// worker (hardware HEVC Main10, B frames requested), PQ and HLG. This does
/// not depend on automatic HDR encoder admission or publication.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow in debug builds; run with --release (see docs/PREVIEW_EXPORT_VERIFICATION.md)"
)]
fn hdr_worker_exports_match_their_committed_previews() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let mut rows = Vec::new();
    let mut failed = Vec::new();
    for (name, media, transfer) in HDR_FIXTURES {
        // Each transfer is reported independently so one failing branch
        // does not hide the other's measurements.
        let mut run = || -> Result {
            let fixture = hdr_recipe(&root.join(name), name, media)?;
            let movie = root.join(format!("{name}-worker.mp4"));
            worker_render(&fixture.package, &fixture.revision, &movie)?;
            let (report, _) = verify(&fixture.package, &movie, &fixture.revision, &[])?;
            rows.push(summary_row(&fixture, "encoded_worker", &report));
            check_hdr_report(&fixture, &report, transfer)?;
            rows.push(missing_caption_is_reported(&fixture, &movie)?);
            rows.push(reframe_is_reported(&fixture, &movie)?);
            Ok(())
        };
        if let Err(error) = run() {
            eprintln!("{name}: {error}");
            failed.push(format!("{name}: {error}"));
        }
    }
    record(&rows)?;
    assert!(failed.is_empty(), "{failed:#?}");
    Ok(())
}

/// Negative: against the revision before the caption, exactly the four
/// captioned frames fail the local structure gate, whatever their
/// whole-picture PSNR (which a small caption barely moves).
fn missing_caption_is_reported(fixture: &Fixture, movie: &Path) -> Result<Value> {
    let (stale, passed) = verify(
        &fixture.package,
        movie,
        &fixture.uncaptioned,
        &["--frames", "0,1,2,3,4,20", "--no-audio"],
    )?;
    assert!(!passed, "{stale}");
    for picture in stale["pictures"].as_array().ok_or("pictures")? {
        let frame = picture["output_frame"].as_u64().ok_or("frame")?;
        let flags = picture["flags"].as_array().ok_or("flags")?;
        if frame < 4 {
            assert!(
                flags.contains(&json!("local_structure_mismatch")),
                "{picture}"
            );
        } else {
            assert!(flags.is_empty(), "{picture}");
        }
    }
    Ok(negative_row(fixture, "missing_caption", &stale))
}

/// Public headless Render of HDR projects: AutomaticHdrV1 encoder admission,
/// finished-file HEVC verification and publication, then `verify-export`.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow in debug builds; run with --release (see docs/PREVIEW_EXPORT_VERIFICATION.md)"
)]
fn hdr_public_render_matches_its_committed_preview() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let mut rows = Vec::new();
    for (name, media, transfer) in HDR_FIXTURES {
        let fixture = hdr_recipe(&root.join(name), name, media)?;
        let movie = public_render(
            &fixture.package,
            &fixture.revision,
            &root.join("exports"),
            &format!("{name}.mp4"),
        )?;
        let (report, _) = verify(&fixture.package, &movie, &fixture.revision, &[])?;
        rows.push(summary_row(&fixture, "public_render", &report));
        check_hdr_report(&fixture, &report, transfer)?;
    }
    record(&rows)
}

/// An SDR Original renders SDR H.264: no source becomes HDR.
#[test]
fn sdr_original_renders_sdr_h264() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let mut project = Project::create(&root.join("sdr"), "sdr-original", "cfr-bframes.mp4")?;
    project.delete_range(10, 120)?;
    let document = project.delete_range(0, 4)?;
    assert_eq!(
        document.presentation_basis().color_policy,
        ColorPolicy::SdrRec709
    );
    let revision = document.revision_id().to_string();
    let movie = public_render(
        &project.package,
        &revision,
        &root.join("exports"),
        "sdr.mp4",
    )?;
    let (report, passed) = verify(&project.package, &movie, &revision, &[])?;
    assert!(passed, "{}", report["failures"]);
    assert_eq!(report["picture_bits"], 8);
    assert_eq!(report["output_color"]["output"], "sdr_rec709");
    assert_eq!(report["output_color"]["reason"], "sdr_sources");
    assert_eq!(report["output_color"]["hdr_sources"], false);
    let color = &report["movie"]["color"];
    assert_eq!(color["container"], json!([1, 1, 1]));
    assert!(color.get("sample_entry").is_none() && color.get("content_light").is_none());
    assert!(report.get("hdr_light").is_none());
    Ok(())
}

fn references(package: &Path, revision: &str, ordinals: &[u64]) -> Result<Vec<ReferencePicture>> {
    Ok(reference_pictures(
        package,
        &RevisionId::new(revision)?,
        ordinals,
        &AtomicBool::new(false),
        Instant::now() + Duration::from_secs(300),
    )?)
}

/// Luma of the patch row at `x` (fixture rows 0..89 hold five 64-pixel
/// patches: Y = 64, P, 400, Q, 940).
fn patch_luma(picture: &ReferencePicture, x: u32) -> u16 {
    picture.planes.y[(40 * picture.planes.width + x) as usize]
}

/// Captions are BT.2408 graphics white: working 1.0 = 203 cd/m², PQ code 573
/// (64 + 876 × 0.58069) and HLG 75 % code 721. HDR highlights above it keep
/// their code values in the HDR branch.
#[test]
fn hdr_references_place_captions_at_graphics_white_and_keep_highlights() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    for (name, media, white, highlight) in [
        ("white-pq", "hdr-pq-av.mp4", 573_u16, 723_u16),
        ("white-hlg", "hdr-hlg-av.mp4", 721, 502),
    ] {
        let fixture = hdr_recipe(&root.join(name), name, media)?;
        let captioned = references(&fixture.package, &fixture.revision, &[1])?;
        let plain = references(&fixture.package, &fixture.uncaptioned, &[1])?;
        let (captioned, plain) = (&captioned[0], &plain[0]);
        assert_eq!(captioned.planes.bits, 10);
        assert_ne!(captioned.color_policy, ColorPolicy::SdrRec709);
        assert!(captioned.light.is_some());
        // The source patches survive the shared picture path within one code.
        for (x, expected) in [(32, 64_u16), (96, white), (224, highlight), (288, 940)] {
            let actual = patch_luma(plain, x);
            assert!(
                actual.abs_diff(expected) <= 1,
                "{name} patch at {x}: {actual}"
            );
        }
        // Changed pixels are the caption: its opaque fill interior is the
        // most frequent changed value and sits at graphics white, and no
        // pixel becomes brighter than that fill (the dark outline and
        // antialiased edges only darken the brighter moving gradient).
        let mut histogram = BTreeMap::<u16, usize>::new();
        let mut brightened = 0_u16;
        for (left, right) in captioned.planes.y.iter().zip(&plain.planes.y) {
            if left != right {
                *histogram.entry(*left).or_default() += 1;
            }
            if left > right {
                brightened = brightened.max(*left);
            }
        }
        let changed: usize = histogram.values().sum();
        let (mode, count) = histogram
            .iter()
            .max_by_key(|entry| *entry.1)
            .map(|(value, count)| (*value, *count))
            .ok_or("the caption changed no pixel")?;
        eprintln!(
            "{name}: {changed} caption pixels, fill mode Y {mode} ({count} pixels), brightest raised Y {brightened}"
        );
        assert!(
            mode.abs_diff(white) <= 1 && count >= 10,
            "{name}: fill {mode} x{count}"
        );
        assert!(
            brightened <= white + 1,
            "{name}: caption raises Y to {brightened}"
        );
    }
    Ok(())
}

/// A PQ primary plus a registered SDR video makes the whole revision SDR:
/// HDR pictures are tone-mapped into Rec.709 I420 that never exceeds SDR
/// reference white (limited Y 235), the 1004 cd/m² patch is compressed and
/// the 203.7 cd/m² patch lands near reference white.
#[test]
fn pq_project_with_sdr_video_falls_back_to_tone_mapped_sdr_pictures() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let directory = root.join("mixed").canonicalize().or_else(|_| {
        fs::create_dir_all(root.join("mixed"))?;
        root.join("mixed").canonicalize()
    })?;
    let package = directory.join("mixed.deadpan");
    let created = success(&["project", "create", package.to_str().ok_or("UTF-8")?])?;
    let root_node = created["root"].as_str().ok_or("root")?.to_owned();
    let mut revision = created["revision_id"]
        .as_str()
        .ok_or("revision")?
        .to_owned();
    for (index, (media, streams, insertion)) in [
        (
            "hdr-pq-av.mp4",
            json!({"type": "video_and_audio", "audio_stream": 1}),
            json!({"parent": root_node, "index": 0, "node": "pq-beat", "label": "PQ"}),
        ),
        (
            "cfr-bframes.mp4",
            json!({"type": "video_only"}),
            Value::Null,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let copy = directory.join(media);
        fs::copy(fixture_media(media), &copy)?;
        let retained = success(&[
            "project",
            "retain-original",
            package.to_str().ok_or("UTF-8")?,
            copy.to_str().ok_or("UTF-8")?,
        ])?;
        let next = format!("mixed-r{index:02}");
        let request = directory.join(format!("register-{index}.json"));
        fs::write(
            &request,
            serde_json::to_vec(&json!({
                "protocol": 1,
                "registration": {
                    "expected_revision": revision,
                    "new_revision": next,
                    "original": retained["retained_original"]["record"]["object"]["content"],
                    "new_asset_id": format!("asset-{index}"),
                    "label": media,
                    "insertion": insertion,
                },
                "streams": streams,
            }))?,
        )?;
        success(&[
            "project",
            "register-source",
            package.to_str().ok_or("UTF-8")?,
            "--request-json",
            request.to_str().ok_or("UTF-8")?,
        ])?;
        revision = next;
    }
    let store = ProjectStore::open(&package, AccessMode::ReadOnly)?;
    let document = store.snapshot()?;
    assert_eq!(
        document.presentation_basis().color_policy,
        ColorPolicy::HdrRec2020Pq
    );
    let decision = committed_color(&store, &document)?;
    assert_eq!(decision.output, ColorPolicy::SdrRec709);
    assert_eq!(decision.reason, OutputColorReason::SdrSourceMixed);
    assert!(decision.hdr_sources && decision.mastering.is_none());
    assert_eq!(decision.tone_map_peak_nits, 1000);
    drop(store);
    let pictures = references(&package, &revision, &[0, 30])?;
    for picture in &pictures {
        assert_eq!(picture.color_policy, ColorPolicy::SdrRec709);
        assert_eq!(picture.planes.bits, 8);
        assert!(picture.light.is_none());
        let brightest = picture.planes.y.iter().copied().max().unwrap_or(0);
        assert!(brightest <= 235, "tone-mapped SDR reaches Y {brightest}");
        // 203.7 cd/m² and 1004 cd/m² patches: both at or below white, the
        // highlight brighter than the reference-white patch.
        let (white, highlight) = (patch_luma(picture, 96), patch_luma(picture, 224));
        eprintln!("fallback SDR: reference patch Y {white}, 1004 cd/m² patch Y {highlight}");
        assert!(highlight > white && highlight <= 235, "{white} {highlight}");
        assert!(white >= 180, "203.7 cd/m² maps near SDR white, got {white}");
    }
    // Public Render of the SDR fallback (requires automatic SDR selection of
    // an HDR-basis document); reported, not asserted, until it lands.
    match public_render(&package, &revision, &root.join("exports"), "mixed.mp4") {
        Ok(movie) => {
            let (report, passed) = verify(&package, &movie, &revision, &[])?;
            eprintln!("fallback public render: {}", report["summary"]);
            assert!(passed, "{}", report["failures"]);
            assert_eq!(report["picture_bits"], 8);
            assert_eq!(report["movie"]["color"]["container"], json!([1, 1, 1]));
        }
        Err(error) => eprintln!("fallback public render unavailable: {error}"),
    }
    Ok(())
}

/// Branch truth table of `decide_output_color` (specification 22.5) on real
/// committed documents, plus in-memory variants for SDR-only providers.
#[test]
fn automatic_output_branch_truth_table() -> Result {
    let (root, _guard) = keep_or(tempfile::tempdir()?);
    let committed = |name: &'static str, media: &str| -> Result<(ProjectDocument, _)> {
        let project = Project::create(&root.join(name), name, media)?;
        let store = ProjectStore::open(&project.package, AccessMode::ReadOnly)?;
        let document = store.snapshot()?;
        let decision = committed_color(&store, &document)?;
        Ok((document, decision))
    };
    let (sdr, decision) = committed("table-sdr", "cfr-bframes.mp4")?;
    assert_eq!(
        sdr.presentation_basis().color_policy,
        ColorPolicy::SdrRec709
    );
    assert_eq!(
        (decision.output, decision.reason, decision.hdr_sources),
        (ColorPolicy::SdrRec709, OutputColorReason::SdrSources, false)
    );
    assert!(decision.mastering.is_none());
    let (pq, decision) = committed("table-pq", "hdr-pq-av.mp4")?;
    assert_eq!(
        pq.presentation_basis().color_policy,
        ColorPolicy::HdrRec2020Pq
    );
    assert_eq!(
        (decision.output, decision.reason),
        (ColorPolicy::HdrRec2020Pq, OutputColorReason::HdrSources)
    );
    assert_eq!(decision.mastering, Some(PQ_FIXTURE_MASTERING));
    // MaxCLL 1000 and mastering peak 1000: the tone-map source peak.
    assert_eq!(decision.tone_map_peak_nits, 1000);
    let (hlg, decision) = committed("table-hlg", "hdr-hlg-av.mp4")?;
    assert_eq!(
        hlg.presentation_basis().color_policy,
        ColorPolicy::HdrRec2020Hlg
    );
    assert_eq!(
        (decision.output, decision.reason, decision.mastering),
        (
            ColorPolicy::HdrRec2020Hlg,
            OutputColorReason::HdrSources,
            None
        )
    );
    assert_eq!(decision.tone_map_peak_nits, 1000);

    // Pure decisions with explicit per-asset color facts.
    let only = |document: &ProjectDocument, color: AssetColor| {
        decide_output_color(document, |_| Some(color))
    };
    let pq_color = AssetColor {
        transfer: AssetTransfer::Pq,
        mastering: Some(PQ_FIXTURE_MASTERING),
        content_light: Some(ContentLight {
            max_cll: 1000,
            max_fall: 400,
        }),
    };
    let hlg_color = AssetColor {
        transfer: AssetTransfer::Hlg,
        mastering: Some(PQ_FIXTURE_MASTERING),
        content_light: None,
    };
    let sdr_color = AssetColor {
        transfer: AssetTransfer::Sdr,
        mastering: None,
        content_light: None,
    };
    // Mastering is retained only for PQ output, even if an HLG source declared one.
    assert_eq!(only(&hlg, hlg_color).mastering, None);
    assert_eq!(only(&pq, pq_color).mastering, Some(PQ_FIXTURE_MASTERING));
    // A PQ basis whose only video is SDR: no HDR source, SDR output.
    let decision = only(&pq, sdr_color);
    assert_eq!(
        (decision.output, decision.reason, decision.hdr_sources),
        (
            ColorPolicy::SdrRec709,
            OutputColorReason::NoHdrSource,
            false
        )
    );
    // An HLG basis with a PQ source: transfer mismatch, SDR with tone mapping.
    let decision = only(&hlg, pq_color);
    assert_eq!(
        (decision.output, decision.reason, decision.mastering),
        (
            ColorPolicy::SdrRec709,
            OutputColorReason::TransferMismatch,
            None
        )
    );
    // An SDR basis never produces HDR, even with an HDR source.
    let decision = only(&sdr, pq_color);
    assert_eq!(
        (decision.output, decision.reason, decision.hdr_sources),
        (
            ColorPolicy::SdrRec709,
            OutputColorReason::HdrSourceInSdrBasis,
            true
        )
    );
    // PQ plus accepted (SDR-only) Hold footage: the whole revision is SDR.
    let asset = pq
        .assets()
        .keys()
        .next()
        .ok_or("PQ Original asset")?
        .clone();
    let accepted = insert_hold(
        &pq,
        HoldVideo::Accepted {
            asset,
            frames: FrameRange::new(ProjectFrame(0), ProjectFrame(6))?,
        },
    )?;
    let decision = only(&accepted, pq_color);
    assert_eq!(
        (decision.output, decision.reason, decision.mastering),
        (
            ColorPolicy::SdrRec709,
            OutputColorReason::GeneratedPictures,
            None
        )
    );
    assert_eq!(decision.tone_map_peak_nits, 1000);
    // Background holds are neutral: a black pause keeps HDR.
    let black = insert_hold(&pq, HoldVideo::Background)?;
    assert_eq!(only(&black, pq_color).output, ColorPolicy::HdrRec2020Pq);
    Ok(())
}

/// The document with a 6-frame silent Hold showing `video` inserted first.
fn insert_hold(document: &ProjectDocument, video: HoldVideo) -> Result<ProjectDocument> {
    let hold = node("table-hold")?;
    let transaction = deadpan_core::apply(
        document,
        &CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: RevisionId::new("table-hold-revision")?,
            command: Command::Insert {
                parent: document.root().clone(),
                index: 0,
                subtree: Subtree {
                    root: hold.clone(),
                    nodes: BTreeMap::from([(
                        hold,
                        BeatNode::hold(
                            "hold",
                            HoldRecipe {
                                picture_context: None,
                                duration: FrameDuration::new(6)?,
                                video,
                                audio: HoldAudio::Silence,
                            },
                        ),
                    )]),
                    overrides: BTreeMap::new(),
                    gap_overrides: BTreeMap::new(),
                },
            },
        },
    )?;
    Ok(transaction.forward.apply(document)?)
}
