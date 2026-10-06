//! Golden picture and PCM hashes for representative structures.
//!
//! Each fixture is a real `.deadpan` package built through the public
//! `deadpan-cli` executable. Its committed revision is rendered through the
//! shared export picture path (`ProjectPictureSession` -> plan -> Metal
//! composition -> working-target readback -> limited Rec.709 I420) and the
//! limited audition bus (`OfflineAudioSession`, the same reader that export
//! and verification use). SHA-256 hashes of the exact I420 bytes of every
//! output frame and of the little-endian f32 PCM of every 8192-sample block
//! are compared with the committed files in `tests/golden_renders/`.
//!
//! The hashes are platform evidence, not a cross-platform contract: see
//! docs/GOLDEN_RENDERS.md. Regenerate them only deliberately, after
//! reviewing why they changed:
//!
//! ```sh
//! DEADPAN_BLESS_GOLDEN=1 cargo test --release --locked -p deadpan-cli --test golden_renders
//! ```
//!
//! Blessing never happens automatically; a missing or different golden fails.

#![cfg(target_os = "macos")]

#[allow(
    dead_code,
    reason = "the shared Section 8 recipe module; this target uses a subset"
)]
#[path = "preview_export/recipes.rs"]
mod recipes;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use deadpan_cli::audio::{MAX_OFFLINE_AUDIO_FRAMES, OfflineAudioSession};
use deadpan_cli::export_picture::{ExportPictureSession, OutputFrameOrdinal};
use deadpan_cli::picture::ProjectPictureSession;
use deadpan_core::{
    AudioTimingId, AudioTreatments, ClipGain, Command, ExactRatio, FrameDuration, FrameRange,
    Framing, FramingPose, GainDb, HoldAudio, HoldRecipe, HoldVideo, InstancePath, NodeId, NodeKind,
    OccurrenceEdit, OccurrenceIdentities, ProjectDocument, ProjectFrame, RepeatInstance,
    RevisionId, SplitIdentities, WrapAnchorPolicy,
};
use deadpan_render::PictureRenderer;
use deadpan_store::{AccessMode, ProjectStore};
use recipes::{Result, success};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const BLESS: &str = "DEADPAN_BLESS_GOLDEN";
/// Golden file grammar; bump when the hashed representation changes.
const GOLDEN_VERSION: u64 = 1;

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden_renders")
}

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The same qualified Metal device selection as the isolated render worker.
fn metal_renderer() -> Result<PictureRenderer> {
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
    if adapter.get_info().backend != wgpu::Backend::Metal {
        return Err("golden renders require the qualified Metal backend".into());
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Deadpan golden render pictures"),
        ..Default::default()
    }))?;
    Ok(PictureRenderer::new(&device, &queue))
}

/// Render one committed revision: per-frame I420 hashes and per-block PCM hashes.
fn render(name: &str, package: &Path, revision: &str) -> Result<Value> {
    let cancelled = AtomicBool::new(false);
    let deadline = Instant::now() + Duration::from_secs(900);
    let revision = RevisionId::new(revision)?;
    let pictures = ProjectPictureSession::open_revision(package, &revision, None, &cancelled)?;
    let mut session = ExportPictureSession::new(pictures, metal_renderer()?, &cancelled, deadline)?;
    let contract = session.contract().clone();
    let mut frames = Vec::new();
    for ordinal in 0..contract.frame_count() {
        let frame = session.prepare(OutputFrameOrdinal(ordinal), &cancelled, deadline)?;
        let pixels = frame.pixels().sdr().ok_or("golden fixtures are SDR")?;
        frames.push(json!(hex(pixels.bytes())));
    }
    drop(session);
    let mut audio = OfflineAudioSession::open_revision(
        package,
        &revision,
        contract.range(),
        &cancelled,
        deadline,
    )?;
    let samples = audio.sample_range();
    let mut whole = Sha256::new();
    let mut blocks = Vec::new();
    let mut at = samples.start.0;
    while at < samples.end.0 {
        let count = (samples.end.0 - at).min(i64::from(MAX_OFFLINE_AUDIO_FRAMES));
        let block = audio.read(
            deadpan_core::AudioSample(at),
            u32::try_from(count)?,
            &cancelled,
        )?;
        let bytes: Vec<u8> = block
            .samples
            .iter()
            .flatten()
            .flat_map(|sample| sample.to_le_bytes())
            .collect();
        whole.update(&bytes);
        blocks.push(json!(hex(&bytes)));
        at += count;
    }
    let [width, height] = contract.raster();
    Ok(json!({
        "version": GOLDEN_VERSION,
        "fixture": name,
        "frames": contract.frame_count(),
        "frame_rate": [contract.frame_rate().numerator(), contract.frame_rate().denominator()],
        "raster": [width, height],
        "picture": "sha256 of tight limited-range Rec.709 I420 (left-sited chroma) per output frame",
        "pictures": frames,
        "audio": {
            "pcm": "sha256 of interleaved stereo little-endian f32, limited audition bus, per 8192-sample block",
            "start": samples.start.0,
            "end": samples.end.0,
            "blocks": blocks,
            "total": whole.finalize().iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
        },
    }))
}

/// Compare with (or, only when explicitly requested, bless) the committed file.
fn compare(name: &str, actual: &Value) -> Result {
    let path = golden_dir().join(format!("{name}.json"));
    if std::env::var_os(BLESS).is_some() {
        if std::env::var_os("CI").is_some() {
            return Err(format!("{BLESS} must not be set in CI").into());
        }
        fs::create_dir_all(golden_dir())?;
        fs::write(
            &path,
            format!("{}\n", serde_json::to_string_pretty(actual)?),
        )?;
        eprintln!("{BLESS}: wrote {}", path.display());
        return Ok(());
    }
    let expected: Value = serde_json::from_slice(&fs::read(&path).map_err(|error| {
        format!(
            "{}: {error}; generate it deliberately with {BLESS}=1",
            path.display()
        )
    })?)?;
    let mut differences = Vec::new();
    for key in ["version", "frames", "frame_rate", "raster"] {
        if expected[key] != actual[key] {
            differences.push(format!("{key}: {} != {}", expected[key], actual[key]));
        }
    }
    let list = |value: &Value, key: &str| -> Vec<Value> {
        value
            .pointer(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let (want, got) = (list(&expected, "/pictures"), list(actual, "/pictures"));
    for (frame, (want, got)) in want.iter().zip(&got).enumerate() {
        if want != got {
            differences.push(format!("picture frame {frame}"));
        }
    }
    if want.len() != got.len() {
        differences.push(format!("picture count {} != {}", want.len(), got.len()));
    }
    for key in ["/audio/start", "/audio/end", "/audio/total"] {
        if expected.pointer(key) != actual.pointer(key) {
            differences.push(key.to_owned());
        }
    }
    let (want, got) = (
        list(&expected, "/audio/blocks"),
        list(actual, "/audio/blocks"),
    );
    for (block, (want, got)) in want.iter().zip(&got).enumerate() {
        if want != got {
            differences.push(format!(
                "PCM block {block} (interval offset {})",
                block * 8192
            ));
        }
    }
    if differences.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{name} differs from {}: {}. Review the cause; regenerate only deliberately with {BLESS}=1",
            path.display(),
            differences.join(", ")
        )
        .into())
    }
}

fn pictures(value: &Value) -> Vec<&str> {
    value["pictures"]
        .as_array()
        .map(|frames| frames.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// A minimal CLI-driven builder for structures the shared recipes lack.
struct Builder {
    name: &'static str,
    directory: PathBuf,
    package: PathBuf,
    step: u32,
    ids: u32,
}

impl Builder {
    fn create(root: &Path, name: &'static str) -> Result<Self> {
        let directory = root.join(name);
        fs::create_dir_all(&directory)?;
        let directory = directory.canonicalize()?;
        let media = directory.join("original.mp4");
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4"),
            &media,
        )?;
        let package = directory.join(format!("{name}.deadpan"));
        success(&[
            "project",
            "create-original",
            package.to_str().ok_or("UTF-8")?,
            media.to_str().ok_or("UTF-8")?,
        ])?;
        Ok(Self {
            name,
            directory,
            package,
            step: 0,
            ids: 0,
        })
    }

    /// Continue editing an existing package under a fresh identity prefix.
    fn reopen(package: &Path, name: &'static str) -> Result<Self> {
        Ok(Self {
            name,
            directory: package.parent().ok_or("package directory")?.to_owned(),
            package: package.to_owned(),
            step: 0,
            ids: 0,
        })
    }

    fn document(&self) -> Result<ProjectDocument> {
        Ok(ProjectStore::open(&self.package, AccessMode::ReadOnly)?.snapshot()?)
    }

    fn fresh(&mut self, count: usize) -> Result<Vec<NodeId>> {
        (0..count)
            .map(|_| {
                self.ids += 1;
                Ok(NodeId::new(format!("{}-n{}", self.name, self.ids))?)
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
            self.package.to_str().ok_or("UTF-8")?,
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
        self.apply(|builder, document, revision| {
            let range = FrameRange::new(ProjectFrame(start), ProjectFrame(end))?;
            let required = document
                .range_deletion(document.root(), range)?
                .required_ids;
            Ok(Command::DeleteRange {
                parent: document.root().clone(),
                range,
                identities: SplitIdentities {
                    nodes: builder.fresh(required)?,
                },
                timing: AudioTimingId {
                    allocation: revision.clone(),
                    ordinal: 0,
                },
            })
        })
    }

    /// Original frames [12, 42) as the 30-frame base edit, as the recipes do.
    fn shorten(&mut self) -> Result<ProjectDocument> {
        self.delete_range(42, 120)?;
        self.delete_range(0, 12)
    }

    fn split_root(&mut self, at: i64) -> Result<ProjectDocument> {
        self.apply(|builder, document, _| {
            let (child, start) = root_child_at(document, at)?;
            Ok(Command::Split {
                node: child,
                at: FrameDuration::new(at - start)?,
                identities: SplitIdentities {
                    nodes: builder.fresh(16)?,
                },
            })
        })
    }

    fn revision(&self) -> Result<String> {
        Ok(self.document()?.revision_id().to_string())
    }
}

fn root_child_at(document: &ProjectDocument, at: i64) -> Result<(NodeId, i64)> {
    let NodeKind::Sequence { children } = &document.nodes()[document.root()].kind else {
        return Err("root is not a Sequence".into());
    };
    let durations = document.durations()?;
    let mut start = 0;
    for child in children {
        let end = start + durations[child].frames();
        if (start..end).contains(&at) {
            return Ok((child.clone(), start));
        }
        start = end;
    }
    Err(format!("no root child contains Edit frame {at}").into())
}

/// The plain 30-frame base edit: Original 12..42 with the click at Edit 17.
fn base(root: &Path) -> Result<(PathBuf, String)> {
    let mut builder = Builder::create(root, "base")?;
    builder.shorten()?;
    Ok((builder.package.clone(), builder.revision()?))
}

/// Edit [12, 18) (Original 24..30) as an inner Repeat of 2 plays, wrapped by
/// an outer Repeat of 2 plays with a 4-frame silent Background gap:
/// 12 + 2·12 + 4 + 12 = 52 frames. In outer play 2 only, inner play 1 gets
/// a static 1.5x zoom and inner play 2 a -6 dB trim, each through
/// `EditOccurrence` (sparse overrides; the shared definition is unchanged).
fn nested_override(root: &Path) -> Result<(PathBuf, String)> {
    let mut builder = Builder::create(root, "nested-override")?;
    builder.shorten()?;
    builder.split_root(12)?;
    builder.split_root(18)?;
    let inner = NodeId::new("nested-inner")?;
    let outer = NodeId::new("nested-outer")?;
    let mut word = None;
    builder.apply(|_, document, _| {
        let (node, _) = root_child_at(document, 12)?;
        word = Some(node.clone());
        Ok(Command::WrapRepeat {
            node,
            id: inner.clone(),
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        })
    })?;
    builder.apply(|_, _, _| {
        Ok(Command::WrapRepeat {
            node: inner.clone(),
            id: outer.clone(),
            plays: 2,
            gap: Some(HoldRecipe {
                picture_context: None,
                duration: FrameDuration::new(4)?,
                video: HoldVideo::Background,
                audio: HoldAudio::Silence,
            }),
            anchor_policy: WrapAnchorPolicy::First,
        })
    })?;
    let word = word.ok_or("wrapped word")?;
    let play = |document: &ProjectDocument, repeat: &NodeId, index: u32| -> Result<_> {
        let NodeKind::Repeat { iterations, .. } = &document.nodes()[repeat].kind else {
            return Err("not a Repeat".into());
        };
        Ok(iterations.at(index).ok_or("play exists")?)
    };
    let half = ExactRatio::new(1, 2)?;
    let zoom = Framing::static_pose(FramingPose::new(half, half, ExactRatio::new(3, 2)?)?)?;
    builder.apply(|builder, document, _| {
        Ok(Command::EditOccurrence {
            instance: InstancePath {
                node: word.clone(),
                repeats: vec![
                    RepeatInstance {
                        node: outer.clone(),
                        iteration: play(document, &outer, 1)?,
                    },
                    RepeatInstance {
                        node: inner.clone(),
                        iteration: play(document, &inner, 0)?,
                    },
                ],
            },
            edit: OccurrenceEdit::SetFraming {
                framing: Some(zoom.clone()),
            },
            identities: OccurrenceIdentities {
                nodes: builder.fresh(16)?,
                marks: Vec::new(),
            },
        })
    })?;
    // The first edit isolated outer play 2; its inner Repeat is now a copy.
    builder.apply(|builder, document, _| {
        let overridden = document.overrides()[&outer]
            .get(&play(document, &outer, 1)?)
            .ok_or("outer play 2 is overridden")?
            .clone();
        let NodeKind::Repeat { child, .. } = &document.nodes()[&overridden].kind else {
            return Err("isolated outer play is a Repeat".into());
        };
        Ok(Command::EditOccurrence {
            instance: InstancePath {
                node: child.clone(),
                repeats: vec![
                    RepeatInstance {
                        node: outer.clone(),
                        iteration: play(document, &outer, 1)?,
                    },
                    RepeatInstance {
                        node: overridden.clone(),
                        iteration: play(document, &overridden, 1)?,
                    },
                ],
            },
            edit: OccurrenceEdit::SetAudioTreatments {
                treatments: AudioTreatments::from_clip_gain(ClipGain::new(
                    GainDb::new(-6_000)?,
                    false,
                    Vec::new(),
                    Vec::new(),
                )?),
            },
            identities: OccurrenceIdentities {
                nodes: builder.fresh(16)?,
                marks: Vec::new(),
            },
        })
    })?;
    Ok((builder.package.clone(), builder.revision()?))
}

fn recipe(
    root: &Path,
    build: fn(&Path) -> Result<recipes::Fixture>,
    name: &str,
) -> Result<(PathBuf, String)> {
    let fixture = build(&root.join(name))?;
    Ok((fixture.package, fixture.revision))
}

/// Every golden fixture renders to its committed hashes. Structural
/// self-consistency (identical plays render identical pictures) is checked
/// independently of the committed values.
#[test]
fn representative_structures_render_to_their_committed_goldens() -> Result {
    let scratch = tempfile::tempdir()?;
    let root = scratch.path();
    type Build = fn(&Path) -> Result<(PathBuf, String)>;
    let fixtures: [(&str, Build); 7] = [
        ("base", base),
        ("repeat-with-gap", |root| {
            recipe(root, recipes::repeat_with_gap, "repeat-with-gap")
        }),
        ("nested-override", nested_override),
        ("retime-half", |root| {
            recipe(root, recipes::retime_half, "retime-half")
        }),
        ("freeze-hold", |root| {
            recipe(root, recipes::freeze_hold, "freeze-hold")
        }),
        ("framing", |root| recipe(root, recipes::framing, "framing")),
        ("escalating-repeat", |root| {
            recipe(root, recipes::escalating_repeat, "escalating-repeat")
        }),
    ];
    let mut failures = Vec::new();
    for (name, build) in fixtures {
        let (package, revision) = build(root)?;
        let started = Instant::now();
        let actual = render(name, &package, &revision)?;
        eprintln!(
            "{name}: {} frames, {} PCM blocks in {:.1}s",
            actual["frames"],
            actual["audio"]["blocks"].as_array().map_or(0, Vec::len),
            started.elapsed().as_secs_f64()
        );
        let shown = pictures(&actual);
        match name {
            "repeat-with-gap" => {
                // Three identical plays of Edit 12..24, 18 frames apart.
                for frame in 12..24 {
                    assert_eq!(shown[frame], shown[frame + 18], "{name} {frame}");
                    assert_eq!(shown[frame], shown[frame + 36], "{name} {frame}");
                }
                assert_ne!(shown[12], shown[24], "{name}: the gap is Background");
            }
            "nested-override" => {
                // Outer play 1 [12, 24), gap [24, 28), outer play 2 [28, 40).
                for k in 0..6 {
                    assert_eq!(shown[12 + k], shown[18 + k], "{name} inner plays {k}");
                    // The gain-only occurrence keeps the shared picture.
                    assert_eq!(shown[12 + k], shown[34 + k], "{name} trim-only {k}");
                    // Only the overridden occurrence is zoomed.
                    assert_ne!(shown[12 + k], shown[28 + k], "{name} zoomed {k}");
                }
            }
            "escalating-repeat" => {
                for k in 12..24 {
                    assert_ne!(shown[k], shown[k + 12], "{name}: play 2 is zoomed");
                    assert_ne!(shown[k + 12], shown[k + 24], "{name}: play 3 is zoomed");
                }
            }
            _ => {}
        }
        if let Err(error) = compare(name, &actual) {
            failures.push(error.to_string());
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

/// Explode keeps every rendered picture and PCM sample: exploded variants
/// must hash exactly like the committed goldens of the Repeats they replace.
/// These comparisons never bless; the goldens belong to the originals.
#[test]
fn exploded_repeats_render_exactly_like_their_repeat_goldens() -> Result {
    let scratch = tempfile::tempdir()?;
    let root = scratch.path();
    let mut failures = Vec::new();
    let mut check = |golden: &str, package: &Path, revision: &str| -> Result {
        let actual = render(golden, package, revision)?;
        let expected: Value =
            serde_json::from_slice(&std::fs::read(golden_dir().join(format!("{golden}.json")))?)?;
        for key in ["frames", "pictures", "audio"] {
            if expected[key] != actual[key] {
                failures.push(format!("exploded {golden}: {key} differs"));
            }
        }
        Ok(())
    };
    let gapped = recipes::repeat_with_gap(&root.join("repeat-with-gap"))?;
    let revision = recipes::explode_fixture(&gapped, "repeat")?;
    check("repeat-with-gap", &gapped.package, &revision)?;
    let escalating = recipes::escalating_repeat(&root.join("escalating-repeat"))?;
    let revision = recipes::explode_fixture(&escalating, "escalator")?;
    check("escalating-repeat", &escalating.package, &revision)?;
    // Inner-first then outer explode of the nested overridden Repeats.
    let (package, _) = nested_override(root)?;
    let mut builder = Builder::reopen(&package, "nested-explode")?;
    for repeat in ["nested-inner", "nested-outer"] {
        builder.apply(|builder, document, revision| {
            let node = NodeId::new(repeat)?;
            let needs = document.explode_requirements(&node)?;
            Ok(Command::Explode {
                node,
                identities: OccurrenceIdentities {
                    nodes: builder.fresh(needs.nodes)?,
                    marks: (0..needs.marks)
                        .map(|n| deadpan_core::MarkId::new(format!("{revision}-m{n}")))
                        .collect::<std::result::Result<_, _>>()?,
                },
                timing: deadpan_core::AudioTimingId {
                    allocation: revision.clone(),
                    ordinal: 0,
                },
            })
        })?;
    }
    check("nested-override", &package, &builder.revision()?)?;
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}
