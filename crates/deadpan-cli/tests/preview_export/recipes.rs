//! Headless fixture projects for preview-versus-export verification.
//!
//! Each fixture is a real one-Original `.deadpan` package built only through
//! the `deadpan-cli` executable: `project create-original`, then ordinary
//! `command --json` requests (and `retain-original`/`register-source` for the
//! catalog sound). Commands are typed `deadpan_core::Command` values, so their
//! wire shape is the serde contract. The read-only store is used only to find
//! current node, asset and revision identities between commands.
//!
//! Every recipe first shortens the Original (`cfr-bframes.mp4`, 120 frames at
//! 30000/1001, burnt-in counter equal to the decoded ordinal) to Original
//! frames [12, 42): Edit frame `f` shows Original ordinal `12 + f`. That range
//! holds the click at Original sample 48000 (end of Original frame 29, Edit
//! frame 17 of the base). Expectations are derived from recipe semantics,
//! never from the compiled plan.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Output};
use std::sync::Arc;

use deadpan_core::{
    AssetId, AttentionTarget, AudioEdgePolicy, AudioSample, AudioTimingId, AudioTreatments,
    ClipGain, Command, Cutaway, CutawayFit, ExactRatio, ExactSourceSpan, FrameDuration, FrameRange,
    Framing, FramingClock, FramingCurve, FramingPose, FramingValue, GainDb, HoldAudio, HoldRecipe,
    HoldVideo, NodeId, NodeKind, PitchPolicy, ProjectDocument, ProjectFrame, RepeatEscalation,
    RetimePurpose, RevisionId, SoundEvent, SoundId, SoundOverflowPolicy, SourceAudio,
    SourceAudioMapping, SourcePoint, SourceSpan, SourceTimestamp, SplitIdentities, TargetId,
    TargetRegion, TargetSample, TrackState, WrapAnchorPolicy, ZoomProgression, ZoomStep,
};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::{Value, json};

pub type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

/// Original ordinals [BASE_START, BASE_END) remain after the base shortening.
pub const BASE_START: u64 = 12;
pub const BASE_END: u64 = 42;
const BASE_FRAMES: u64 = BASE_END - BASE_START;

/// Independently derived picture provenance of one output frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expected {
    /// The 0-based decoded frame of `cfr-bframes.mp4`, equal to its burnt-in counter.
    Original {
        source_ordinal: u64,
    },
    Background,
    Generated,
}

#[derive(Debug, Clone)]
pub struct Fixture {
    pub name: &'static str,
    pub package: PathBuf,
    /// Final committed revision.
    pub revision: String,
    /// Final Edit duration in project frames.
    pub frames: u64,
    /// Row names in docs/SECTION8_COVERAGE.md this fixture demonstrates.
    pub section8_rows: Vec<&'static str>,
    pub expectations: Vec<(u64, Expected)>,
    /// Recipe choices made while building, such as a refused pitch policy.
    pub notes: Vec<String>,
}

fn original(source_ordinal: u64) -> Expected {
    Expected::Original { source_ordinal }
}

/// Base Edit frame `f` shows Original ordinal `BASE_START + f`.
fn base(frame: u64) -> Expected {
    original(BASE_START + frame)
}

pub fn cli_path() -> &'static str {
    env!("CARGO_BIN_EXE_deadpan-cli")
}

pub fn cli(arguments: &[&str]) -> Result<Output> {
    Ok(Process::new(cli_path()).args(arguments).output()?)
}

pub fn success(arguments: &[&str]) -> Result<Value> {
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

fn source_media() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/fixtures/cfr-bframes.mp4")
}

fn sound_media() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../native/deadpan-source/tests/audio-fixtures/pcm-stereo-48000.wav")
}

fn node(value: &str) -> Result<NodeId> {
    Ok(NodeId::new(value)?)
}

fn ratio(numerator: i128, denominator: i128) -> Result<ExactRatio> {
    Ok(ExactRatio::new(numerator, denominator)?)
}

/// One package edited through the CLI, with deterministic fresh identities.
struct Project {
    name: &'static str,
    directory: PathBuf,
    package: PathBuf,
    step: u32,
    ids: u32,
    asset: AssetId,
}

impl Project {
    fn create(directory: &Path, name: &'static str) -> Result<Self> {
        fs::create_dir_all(directory)?;
        let directory = directory.canonicalize()?;
        let media = directory.join("original.mp4");
        fs::copy(source_media(), &media)?;
        let package = directory.join(format!("{name}.deadpan"));
        let created = success(&[
            "project",
            "create-original",
            package.to_str().ok_or("UTF-8 path")?,
            media.to_str().ok_or("UTF-8 path")?,
        ])?;
        let asset = AssetId::new(
            created["created"]["asset_id"]
                .as_str()
                .ok_or("create-original reports its asset")?,
        )?;
        Ok(Self {
            name,
            directory,
            package,
            step: 0,
            ids: 0,
            asset,
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

    fn next_revision(&mut self) -> Result<RevisionId> {
        self.step += 1;
        Ok(RevisionId::new(format!("{}-r{:02}", self.name, self.step))?)
    }

    fn request(
        &self,
        document: &ProjectDocument,
        revision: &RevisionId,
        command: &Command,
    ) -> Result<PathBuf> {
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
        Ok(request)
    }

    /// Commit one command built against the current document and the new
    /// revision (timing allocations must equal it).
    fn apply(
        &mut self,
        build: impl FnOnce(&mut Self, &ProjectDocument, &RevisionId) -> Result<Command>,
    ) -> Result<ProjectDocument> {
        let document = self.document()?;
        let revision = self.next_revision()?;
        let command = build(self, &document, &revision)?;
        let request = self.request(&document, &revision, &command)?;
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

    /// Dry-run a command; `Ok(Err(stderr))` reports a refusal.
    fn admits(&mut self, command: &Command) -> Result<std::result::Result<(), String>> {
        let document = self.document()?;
        let revision = RevisionId::new(format!("{}-dry-{}", self.name, self.step + 1))?;
        let request = self.request(&document, &revision, command)?;
        let output = cli(&[
            "command",
            self.path(),
            "--json",
            request.to_str().ok_or("UTF-8")?,
            "--dry-run",
        ])?;
        Ok(if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).into_owned())
        })
    }

    /// Delete Edit [start, end) from the root Sequence.
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

    /// Keep Original frames [BASE_START, BASE_END) as the 30-frame base edit.
    fn shorten(&mut self) -> Result<ProjectDocument> {
        self.delete_range(BASE_END as i64, 120)?;
        let document = self.delete_range(0, BASE_START as i64)?;
        let frames = document.duration()?.frames();
        if frames != BASE_FRAMES as i64 {
            return Err(format!("base edit is {frames} frames").into());
        }
        Ok(document)
    }

    /// Split the root child containing Edit boundary `at` (a strict interior).
    fn split_root(&mut self, at: i64) -> Result<ProjectDocument> {
        self.apply(|project, document, _| {
            let (child, start) = root_child_at(document, at)?;
            Ok(Command::Split {
                node: child,
                at: FrameDuration::new(at - start)?,
                // Unused identities are not persisted; supply a generous pool.
                identities: SplitIdentities {
                    nodes: project.fresh(16)?,
                },
            })
        })
    }

    fn finish(
        self,
        section8_rows: Vec<&'static str>,
        expectations: Vec<(u64, Expected)>,
        notes: Vec<String>,
    ) -> Result<Fixture> {
        let document = self.document()?;
        let frames = u64::try_from(document.duration()?.frames())?;
        if let Some((frame, _)) = expectations.iter().find(|(frame, _)| *frame >= frames) {
            return Err(format!(
                "{} expectation {frame} is outside {frames} frames",
                self.name
            )
            .into());
        }
        Ok(Fixture {
            name: self.name,
            package: self.package,
            revision: document.revision_id().to_string(),
            frames,
            section8_rows,
            expectations,
            notes,
        })
    }
}

fn root_children(document: &ProjectDocument) -> Result<Vec<NodeId>> {
    match &document.nodes()[document.root()].kind {
        NodeKind::Sequence { children } => Ok(children.clone()),
        _ => Err("root is not a Sequence".into()),
    }
}

/// The direct root child whose half-open Edit interval contains `at`, and its start.
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

/// The Source beneath neutral Partitions, and the Source-local frame shown at
/// local frame zero of `beat`.
fn source_host(document: &ProjectDocument, beat: &NodeId) -> Result<(NodeId, i64)> {
    let mut current = beat.clone();
    let mut offset = 0;
    loop {
        match &document.nodes()[&current].kind {
            NodeKind::Source { .. } => return Ok((current, offset)),
            NodeKind::Retime {
                child,
                mapping,
                purpose: RetimePurpose::Partition,
                ..
            } => {
                offset += mapping.start().0;
                current = child.clone();
            }
            other => return Err(format!("{current} is not a Source fragment: {other:?}").into()),
        }
    }
}

fn silent(duration: i64, video: HoldVideo) -> Result<HoldRecipe> {
    Ok(HoldRecipe {
        picture_context: None,
        duration: FrameDuration::new(duration)?,
        video,
        audio: HoldAudio::Silence,
    })
}

/// Picture time base and ticks per frame of the Original (CFR, start 0).
fn original_clock(
    document: &ProjectDocument,
    asset: &AssetId,
) -> Result<(deadpan_core::SourceTimeBase, i64)> {
    let record = &document.assets()[asset];
    let video = record.video.ok_or("Original has video")?;
    let frames = i64::from(record.frame_count.ok_or("Original has a frame count")?);
    if video.start().ticks != 0 || (video.end().ticks - video.start().ticks) % frames != 0 {
        return Err("Original picture clock is not CFR from zero".into());
    }
    Ok((video.start().time_base, video.end().ticks / frames))
}

/// Exact picture interval of Original ordinals [first, end).
fn ordinal_span(
    document: &ProjectDocument,
    asset: &AssetId,
    first: i64,
    end: i64,
) -> Result<ExactSourceSpan> {
    let (time_base, per_frame) = original_clock(document, asset)?;
    let point = |ordinal: i64| SourcePoint {
        ticks: ExactRatio::integer(ordinal * per_frame),
        time_base,
    };
    Ok(ExactSourceSpan::new(point(first), point(end))?)
}

const REPEAT_ROWS: [&str; 3] = ["Word / syllable stutter", "Micro-loop", "Repeat operator"];

/// Three total plays of Edit [12, 24) (Original 24..36, holding the click)
/// with a silent 6-frame Background gap: 12 + 3·12 + 2·6 + 6 = 66 frames.
pub fn repeat_with_gap(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "repeat-with-gap")?;
    project.shorten()?;
    project.split_root(12)?;
    project.split_root(24)?;
    project.apply(|_, document, _| {
        Ok(Command::WrapRepeat {
            node: root_child_at(document, 12)?.0,
            id: node("repeat")?,
            plays: 3,
            gap: Some(silent(6, HoldVideo::Background)?),
            anchor_policy: WrapAnchorPolicy::First,
        })
    })?;
    // Play p (0-based) starts at 12 + 18p; its frame k shows Original 24 + k.
    let mut expectations = vec![(0, base(0)), (11, base(11))];
    for play in 0..3u64 {
        let start = 12 + 18 * play;
        expectations.extend([
            (start, original(24)),
            (start + 6, original(30)),
            (start + 11, original(35)),
        ]);
        if play < 2 {
            expectations.extend([
                (start + 12, Expected::Background),
                (start + 17, Expected::Background),
            ]);
        }
    }
    expectations.extend([(60, original(36)), (65, original(41))]);
    project.finish(REPEAT_ROWS.to_vec(), expectations, Vec::new())
}

/// The native `,h` freeze: a 15-frame silent Hold at Edit 15 (Original 27)
/// freezing the preceding picture (Original 26), with its captured view.
pub fn freeze_hold(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "freeze-hold")?;
    project.shorten()?;
    let at = ProjectFrame(15);
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
                duration: FrameDuration::new(15)?,
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
    let expectations = vec![
        (0, base(0)),
        (14, original(26)),
        (15, original(26)),
        (22, original(26)),
        (29, original(26)),
        (30, original(27)),
        (32, original(29)),
        (44, original(41)),
    ];
    project.finish(
        vec![
            "Dead air",
            "Frozen stare",
            "Hold (insert time)",
            "`,h` freeze hold",
        ],
        expectations,
        Vec::new(),
    )
}

/// `:hold 12f video=black` at Edit 15: a silent Background Hold.
pub fn black_pause(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "black-pause")?;
    project.shorten()?;
    let at = ProjectFrame(15);
    project.apply(|project, document, revision| {
        let identities = match document.insert_time_target(at)?.split {
            Some(split) => project.fresh(split.required_ids)?,
            None => Vec::new(),
        };
        Ok(Command::InsertTime {
            at,
            hold: silent(12, HoldVideo::Background)?,
            id: node("black")?,
            identities: SplitIdentities { nodes: identities },
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })?;
    let expectations = vec![
        (0, base(0)),
        (14, original(26)),
        (15, Expected::Background),
        (20, Expected::Background),
        (26, Expected::Background),
        (27, original(27)),
        (29, original(29)),
        (41, original(41)),
    ];
    project.finish(
        vec![
            "Black-frame punctuation",
            "Sudden silence",
            "Hold (insert time)",
        ],
        expectations,
        Vec::new(),
    )
}

/// Edit [12, 24) (Original 24..36) at 50% speed: 24 output frames. Output
/// frame n of the Retime samples input center (n + 1/2)/2, so it shows input
/// frame floor(n/2). Preserve pitch when admitted, else tape (FollowSpeed).
pub fn retime_half(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "retime-half")?;
    project.shorten()?;
    project.split_root(12)?;
    let document = project.split_root(24)?;
    let target = root_child_at(&document, 12)?.0;
    let wrap = |pitch| Command::WrapRetime {
        node: target.clone(),
        id: NodeId::new("retime").expect("constant identity"),
        duration: FrameDuration::new(24).expect("positive duration"),
        pitch,
    };
    let mut notes = Vec::new();
    let pitch = match project.admits(&wrap(PitchPolicy::Preserve))? {
        Ok(()) => {
            notes.push("pitch=preserve admitted".to_owned());
            PitchPolicy::Preserve
        }
        Err(refusal) => {
            notes.push(format!(
                "pitch=preserve refused, using tape: {}",
                refusal.trim()
            ));
            PitchPolicy::FollowSpeed
        }
    };
    project.apply(|_, _, _| Ok(wrap(pitch)))?;
    let mut expectations = vec![(0, base(0)), (11, base(11))];
    for n in [0u64, 1, 2, 3, 11, 12, 13, 23] {
        expectations.push((12 + n, original(24 + n / 2)));
    }
    expectations.extend([(36, original(36)), (41, original(41))]);
    project.finish(
        vec!["Slow delivery", "Retime", "Stretch / pitch"],
        expectations,
        notes,
    )
}

/// Three beats over the base: Edit [0, 10) static 1.35x zoom, [10, 20) a
/// smoothstep creep from 1x to 1.35x, [20, 30) a 1.35x follow of a target
/// moving left to right over Original 32..42. Pictures are unchanged.
pub fn framing(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "framing")?;
    project.shorten()?;
    project.split_root(10)?;
    project.split_root(20)?;
    let zoom = ratio(27, 20)?;
    let half = ratio(1, 2)?;
    let zoomed = FramingPose::new(half, half, zoom)?;
    project.apply(|_, document, _| {
        Ok(Command::SetFraming {
            node: root_child_at(document, 0)?.0,
            framing: Some(Framing::static_pose(zoomed)?),
        })
    })?;
    project.apply(|_, document, _| {
        Ok(Command::SetFraming {
            node: root_child_at(document, 10)?.0,
            framing: Some(Framing::creep(
                FramingPose::identity(),
                zoomed,
                FramingCurve::Smoothstep,
            )?),
        })
    })?;
    let target = TargetId::new("subject")?;
    let asset = project.asset.clone();
    project.apply(|_, document, _| {
        let (time_base, per_frame) = original_clock(document, &asset)?;
        let stamp = |ordinal: i64| SourceTimestamp {
            ticks: ordinal * per_frame,
            time_base,
        };
        let region = |x: u32| TargetRegion {
            center: [x, 400_000],
            size: [200_000, 200_000],
        };
        Ok(Command::SetTarget {
            id: target.clone(),
            target: AttentionTarget {
                label: "Subject".into(),
                asset: asset.clone(),
                span: SourceSpan::new(stamp(32), stamp(42))?,
                region: region(300_000),
                samples: vec![
                    TargetSample {
                        at: 32 * per_frame,
                        region: region(300_000),
                        confidence: 1000,
                        state: TrackState::Tracked,
                    },
                    TargetSample {
                        at: 41 * per_frame,
                        region: region(700_000),
                        confidence: 1000,
                        state: TrackState::Tracked,
                    },
                ],
                corrections: Vec::new(),
                provenance: None,
            },
        })
    })?;
    project.apply(|_, document, _| {
        Ok(Command::SetFraming {
            node: root_child_at(document, 20)?.0,
            framing: Some(Framing {
                clock: FramingClock::OwnerOutput,
                value: FramingValue::Follow {
                    target: target.clone(),
                    scale: zoom,
                    fallback: zoomed,
                },
            }),
        })
    })?;
    let expectations = [0u64, 9, 10, 15, 19, 20, 25, 29]
        .into_iter()
        .map(|frame| (frame, base(frame)))
        .collect();
    project.finish(
        vec!["Smash zoom", "Slow creep", "Off-center stare", "Framing"],
        expectations,
        Vec::new(),
    )
}

/// `:cutaway` over Edit [10, 20) of the base showing Original 90..96 with the
/// default hold fit: Edit 10 + k shows Original 90 + min(k, 5).
pub fn cutaway(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "cutaway")?;
    project.shorten()?;
    let asset = project.asset.clone();
    project.apply(|_, document, _| {
        let (beat, start) = root_child_at(document, 10)?;
        let (host, offset) = source_host(document, &beat)?;
        let local = offset + 10 - start;
        Ok(Command::SetCutaways {
            node: host,
            cutaways: vec![Cutaway {
                range: FrameRange::new(ProjectFrame(local), ProjectFrame(local + 10))?,
                asset: asset.clone(),
                selection: ordinal_span(document, &asset, 90, 96)?,
                fit: CutawayFit::Hold,
            }],
        })
    })?;
    let expectations = vec![
        (0, base(0)),
        (9, base(9)),
        (10, original(90)),
        (12, original(92)),
        (15, original(95)),
        (16, original(95)),
        (19, original(95)),
        (20, base(20)),
        (29, base(29)),
    ];
    project.finish(
        vec!["Reaction cutaway", "Replace picture", "Cutaways"],
        expectations,
        Vec::new(),
    )
}

/// The 0.08 zoom step on the 2^-32 framing grid, rounded to nearest as the
/// native `:repeat zoom-step=0.08` resolves it.
fn grid_step() -> Result<ExactRatio> {
    let scale = i128::from(deadpan_core::FRAMING_NUMERIC_SCALE);
    // round(0.08 * 2^32) = round(343597383.68) = 343597384.
    let units = (8 * scale + 50) / 100;
    ratio(units, scale)
}

/// Edit [12, 24) wrapped in 3 plays without gap, escalating +3 dB and +0.08
/// centered scale per play: 12 + 36 + 6 = 54 frames.
pub fn escalating_repeat(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "escalating-repeat")?;
    project.shorten()?;
    project.split_root(12)?;
    project.split_root(24)?;
    project.apply(|_, document, _| {
        Ok(Command::WrapRepeat {
            node: root_child_at(document, 12)?.0,
            id: node("escalator")?,
            plays: 3,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        })
    })?;
    project.apply(|_, _, _| {
        Ok(Command::SetRepeatEscalation {
            node: node("escalator")?,
            escalation: Some(RepeatEscalation {
                gain_step: GainDb::new(3_000)?,
                zoom: Some(ZoomStep {
                    step: grid_step()?,
                    progression: ZoomProgression::Add,
                }),
            }),
        })
    })?;
    let mut expectations = vec![(0, base(0)), (11, base(11))];
    for play in 0..3u64 {
        let start = 12 + 12 * play;
        expectations.extend([
            (start, original(24)),
            (start + 6, original(30)),
            (start + 11, original(35)),
        ]);
    }
    expectations.extend([(48, original(36)), (53, original(41))]);
    project.finish(
        vec!["Escalation", "Escalating crop", "`,e` escalating repeat"],
        expectations,
        Vec::new(),
    )
}

/// A whole-beat +6 dB clip-gain trim on the base beat. Pictures are unchanged.
pub fn gain_trim(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "gain-trim")?;
    project.shorten()?;
    project.apply(|_, document, _| {
        Ok(Command::SetAudioTreatments {
            node: root_child_at(document, 0)?.0,
            treatments: AudioTreatments::from_clip_gain(ClipGain::new(
                GainDb::new(6_000)?,
                false,
                Vec::new(),
                Vec::new(),
            )?),
        })
    })?;
    let expectations = [0u64, 10, 17, 18, 29]
        .into_iter()
        .map(|frame| (frame, base(frame)))
        .collect();
    project.finish(
        vec!["Selective emphasis", "`:gain +6dB`"],
        expectations,
        Vec::new(),
    )
}

/// Edit frame at which the catalog sound starts (an exact 48 kHz sample:
/// 10 · 48000 · 1001 / 30000 = 16016).
pub const SOUND_FRAME: i64 = 10;

/// Rewrite a canonical 44-byte PCM WAVE header as WAVE_FORMAT_EXTENSIBLE with
/// an explicit front-left/front-right channel mask. A plain WAVE header leaves
/// the speaker layout unspecified, which audition and export both refuse
/// rather than guessing stereo from the channel count.
fn declared_stereo(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.len() < 44
        || &bytes[0..4] != b"RIFF"
        || &bytes[8..16] != b"WAVEfmt "
        || bytes[16..20] != 16_u32.to_le_bytes()
        || bytes[20..24] != [1, 0, 2, 0]
        || &bytes[36..40] != b"data"
    {
        return Err("expected a canonical 16-byte-fmt stereo PCM WAVE file".into());
    }
    let mut declared = Vec::with_capacity(bytes.len() + 24);
    declared.extend_from_slice(b"RIFF");
    declared.extend_from_slice(&u32::try_from(bytes.len() + 16)?.to_le_bytes());
    declared.extend_from_slice(b"WAVEfmt ");
    declared.extend_from_slice(&40_u32.to_le_bytes());
    declared.extend_from_slice(&0xfffe_u16.to_le_bytes());
    declared.extend_from_slice(&bytes[22..36]);
    declared.extend_from_slice(&22_u16.to_le_bytes());
    declared.extend_from_slice(&bytes[34..36]);
    declared.extend_from_slice(&3_u32.to_le_bytes());
    // KSDATAFORMAT_SUBTYPE_PCM in WAVE GUID byte order.
    declared.extend_from_slice(&[
        1, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xaa, 0, 0x38, 0x9b, 0x71,
    ]);
    declared.extend_from_slice(&bytes[36..]);
    Ok(declared)
}

/// Register `pcm-stereo-48000.wav` (with a declared stereo mask) as an audio-only catalog asset and place
/// it whole as a root sound starting at Edit frame 10. Pictures are unchanged.
pub fn sound_event(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "sound-event")?;
    project.shorten()?;
    let wav = project.directory.join("sound.wav");
    fs::write(&wav, declared_stereo(&fs::read(sound_media())?)?)?;
    let retained = success(&[
        "project",
        "retain-original",
        project.path(),
        wav.to_str().ok_or("UTF-8")?,
    ])?;
    let content = retained["retained_original"]["record"]["object"]["content"].clone();
    let document = project.document()?;
    let revision = project.next_revision()?;
    let registration = project.directory.join("register-sound.json");
    fs::write(
        &registration,
        serde_json::to_vec(&json!({
            "protocol": 1,
            "registration": {
                "expected_revision": document.revision_id(),
                "new_revision": revision,
                "original": content,
                "new_asset_id": "sound",
                "label": "Click sound",
                "insertion": null
            },
            "streams": {"type": "audio_only", "stream": 0}
        }))?,
    )?;
    success(&[
        "project",
        "register-source",
        project.path(),
        "--request-json",
        registration.to_str().ok_or("UTF-8")?,
    ])?;
    project.apply(|_, document, _| {
        let asset = document
            .assets()
            .iter()
            .find(|(_, record)| record.video.is_none() && record.audio.is_some())
            .map(|(id, record)| (id.clone(), record.audio.expect("audio")))
            .ok_or("the catalog sound is registered")?;
        let source = SourceAudio {
            asset: asset.0,
            span: asset.1,
        };
        let rate = document.presentation_basis().frame_rate;
        let numerator = SOUND_FRAME * 48_000 * i64::from(rate.denominator());
        let denominator = i64::from(rate.numerator());
        if numerator % denominator != 0 {
            return Err("sound frame is not on an exact sample".into());
        }
        Ok(Command::SetSound {
            id: SoundId::new("click")?,
            event: SoundEvent {
                owner: document.root().clone(),
                label: "Click".into(),
                mapping: SourceAudioMapping::natural_rate(source.span, rate)?,
                source,
                offset: AudioSample(numerator / denominator),
                gain_millidecibels: 0,
                start_edge: AudioEdgePolicy::Automatic,
                end_edge: AudioEdgePolicy::Automatic,
                overflow: SoundOverflowPolicy::Reject,
            },
        })
    })?;
    let expectations = [0u64, 9, 10, 15, 29]
        .into_iter()
        .map(|frame| (frame, base(frame)))
        .collect();
    project.finish(
        vec!["Bed drop", "Wrongly triumphant sting"],
        expectations,
        Vec::new(),
    )
}

/// Build every fixture, each in its own subdirectory of `dir`.
pub fn all(dir: &Path) -> Result<Vec<Fixture>> {
    type Builder = fn(&Path) -> Result<Fixture>;
    let builders: [(&str, Builder); 9] = [
        ("repeat-with-gap", repeat_with_gap),
        ("freeze-hold", freeze_hold),
        ("black-pause", black_pause),
        ("retime-half", retime_half),
        ("framing", framing),
        ("cutaway", cutaway),
        ("escalating-repeat", escalating_repeat),
        ("gain-trim", gain_trim),
        ("sound-event", sound_event),
    ];
    builders
        .into_iter()
        .map(|(name, build)| {
            build(&dir.join(name)).map_err(|error| format!("{name}: {error}").into())
        })
        .collect()
}

/// Commit a later edit to an already rendered fixture without changing its
/// duration: a static 1.35x zoom and a -12 dB trim on the root child at Edit
/// frame `at`. Verifying the earlier movie against the returned revision must
/// then fail, which proves the harness detects framing and level mismatches.
/// (An increase would be hidden for the near-full-scale click: the shared
/// limiter clamps it, so the attenuation is the meaningful level change.)
pub fn reframe_and_attenuate(fixture: &Fixture, at: i64) -> Result<String> {
    let directory = fixture
        .package
        .parent()
        .ok_or("fixture package has a parent")?
        .to_owned();
    let document = ProjectStore::open(&fixture.package, AccessMode::ReadOnly)?.snapshot()?;
    let asset = document
        .assets()
        .iter()
        .find(|(_, record)| record.video.is_some())
        .map(|(id, _)| id.clone())
        .ok_or("fixture has a picture asset")?;
    let mut project = Project {
        name: fixture.name,
        directory,
        package: fixture.package.clone(),
        step: 90,
        ids: 9000,
        asset,
    };
    let half = ratio(1, 2)?;
    let zoomed = FramingPose::new(half, half, ratio(27, 20)?)?;
    project.apply(|_, document, _| {
        Ok(Command::SetFraming {
            node: root_child_at(document, at)?.0,
            framing: Some(Framing::static_pose(zoomed)?),
        })
    })?;
    let saved = project.apply(|_, document, _| {
        Ok(Command::SetAudioTreatments {
            node: root_child_at(document, at)?.0,
            treatments: AudioTreatments::from_clip_gain(ClipGain::new(
                GainDb::new(-12_000)?,
                false,
                Vec::new(),
                Vec::new(),
            )?),
        })
    })?;
    Ok(saved.revision_id().to_string())
}
