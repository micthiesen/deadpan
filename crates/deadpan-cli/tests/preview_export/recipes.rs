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
    HoldVideo, NodeId, NodeKind, PitchPolicy, ProjectDocument, ProjectFrame, RegisterName,
    RegisterValue, RepeatEscalation, RetimePurpose, RevisionId, SoundEvent, SoundId,
    SoundOverflowPolicy, SourceAudio, SourceAudioMapping, SourcePoint, SourceSpan, SourceTimestamp,
    SplitIdentities, TargetId, TargetRegion, TargetSample, TrackState, WrapAnchorPolicy,
    ZoomProgression, ZoomStep,
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
    /// Independently derived sound: `(start, loud)` for 256-sample windows of
    /// the limited audition bus on the absolute 48 kHz Edit grid. Loud windows
    /// hold the click or a placed sound (peak above 0.5); quiet ones are
    /// silent (peak below 0.01).
    pub audio: Vec<(i64, bool)>,
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

    /// Save `instructions` as macro `m` and run it once through the headless
    /// semantic path the native app shares: the same planner, pause-picture
    /// resolver and store admission, committed as one Compound.
    fn run_semantic(
        &mut self,
        instructions: Value,
        cursor: i64,
        selected_child: Option<&NodeId>,
    ) -> Result<ProjectDocument> {
        let bank = |project: &Self| -> Result<(ProjectDocument, u64)> {
            let inspected = success(&["macro", "inspect", project.path()])?;
            Ok((
                project.document()?,
                inspected["bank_version"]
                    .as_u64()
                    .ok_or("macro inspect reports its bank version")?,
            ))
        };
        let send = |project: &Self, name: &str, request: Value| -> Result {
            let path = project.directory.join(name);
            fs::write(&path, serde_json::to_vec(&request)?)?;
            success(&[
                "macro",
                project.path(),
                "--json",
                path.to_str().ok_or("UTF-8")?,
            ])?;
            Ok(())
        };
        let (document, version) = bank(self)?;
        send(
            self,
            "macro-save.json",
            json!({"protocol":1,"project_id":document.project_id(),
                "expected_revision":document.revision_id(),"expected_bank_version":version,
                "operation":{"type":"save","register":"m","program":{"instructions":instructions}}}),
        )?;
        let (document, version) = bank(self)?;
        let before = document.revision_id().clone();
        send(
            self,
            "macro-run.json",
            json!({"protocol":1,"project_id":document.project_id(),
                "expected_revision":document.revision_id(),"expected_bank_version":version,
                "operation":{"type":"run","register":"m","parent":document.root(),
                    "cursor":cursor,"selected_child":selected_child,"count":1}}),
        )?;
        let saved = self.document()?;
        if saved.revision_id() == &before {
            return Err("the semantic run committed no revision".into());
        }
        Ok(saved)
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
            audio: Vec::new(),
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

/// An off-center stare: the whole base beat framed by a static 1.5x pose
/// centered at (0.35, 0.40), as Camera and `:framing-save` presets author it.
/// Pictures keep their Original frames.
pub fn off_center(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "off-center")?;
    project.shorten()?;
    let pose = FramingPose::new(ratio(7, 20)?, ratio(2, 5)?, ratio(3, 2)?)?;
    project.apply(|_, document, _| {
        Ok(Command::SetFraming {
            node: root_child_at(document, 0)?.0,
            framing: Some(Framing::static_pose(pose)?),
        })
    })?;
    let expectations = [0u64, 10, 20, 29]
        .into_iter()
        .map(|frame| (frame, base(frame)))
        .collect();
    project.finish(vec!["Off-center stare"], expectations, Vec::new())
}

/// `,m` over Edit [25, 35) of the whole, unshortened Original: a mute range
/// in the beat's own frames that silences the click at sample 48,000 while
/// the click at 191,992 still sounds. Pictures and timing are unchanged.
pub fn mute_range(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "mute-range")?;
    project.apply(|_, document, _| {
        Ok(Command::SetAudioTreatments {
            node: root_child_at(document, 0)?.0,
            treatments: AudioTreatments::from_clip_gain(ClipGain::new(
                GainDb::UNITY,
                false,
                Vec::new(),
                vec![deadpan_core::GainRange::new(
                    ExactRatio::integer(25),
                    ExactRatio::integer(35),
                )?],
            )?),
        })
    })?;
    let expectations = [0u64, 25, 30, 35, 119]
        .into_iter()
        .map(|frame| (frame, original(frame)))
        .collect();
    let mut fixture = project.finish(
        vec!["Sudden silence", "`,m` mute"],
        expectations,
        Vec::new(),
    )?;
    fixture.audio = vec![(47_872, false), (191_872, true)];
    Ok(fixture)
}

/// Edit frame at which the catalog sound starts (an exact 48 kHz sample:
/// 10 · 48000 · 1001 / 30000 = 16016).
pub const SOUND_FRAME: i64 = 10;

/// Register the plain `pcm-stereo-48000.wav`, explicitly interpreted as stereo
/// L/R, as an audio-only catalog asset and place
/// it whole as a root sound starting at Edit frame 10. Pictures are unchanged.
pub fn sound_event(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "sound-event")?;
    project.shorten()?;
    place_click_sound(&mut project)?;
    let expectations = [0u64, 9, 10, 15, 29]
        .into_iter()
        .map(|frame| (frame, base(frame)))
        .collect();
    let mut fixture = project.finish(
        vec!["Bed drop", "Wrongly triumphant sting"],
        expectations,
        Vec::new(),
    )?;
    fixture.audio = vec![(15_744, false), (16_128, true), (21_504, true)];
    Ok(fixture)
}

/// `:sound-cut` with the Edit cursor at frame 12: the placed sound of
/// [`sound_event`] (about 5 frames from frame 10) ends exactly at that frame
/// boundary, sample 19,219.2, with a hard edge. Its local selection becomes
/// [0, 2) frames on the unchanged natural-rate mapping and onset. Pictures
/// are unchanged.
pub fn bed_drop(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "bed-drop")?;
    project.shorten()?;
    place_click_sound(&mut project)?;
    project.apply(|_, document, _| {
        // The same `SoundEvent::cut_at` the native `:sound-cut` commits.
        let id = SoundId::new("click")?;
        let event = document.sounds()[&id]
            .cut_at(ProjectFrame(12), document.presentation_basis().frame_rate)?;
        let SourceAudioMapping::SelectedPlacement { selection, .. } = event.mapping else {
            return Err("the cut sound has a selected placement".into());
        };
        if selection.end != ExactRatio::integer(12 - SOUND_FRAME)
            || event.end_edge != AudioEdgePolicy::Hard
        {
            return Err(format!("unexpected cut {:?}", event.mapping).into());
        }
        Ok(Command::SetSound { id, event })
    })?;
    let expectations = [0u64, 10, 12, 15, 29]
        .into_iter()
        .map(|frame| (frame, base(frame)))
        .collect();
    let mut fixture = project.finish(vec!["Bed drop", "`:sound-cut`"], expectations, Vec::new())?;
    // The sound plays from sample 16,016 and stops at 19,219.2 instead of
    // running on to about 24,000; the base click at 28,781 is untouched.
    fixture.audio = vec![
        (16_128, true),
        (18_944, true),
        (19_456, false),
        (21_504, false),
        (28_672, true),
    ];
    Ok(fixture)
}

/// Register `pcm-stereo-48000.wav` and place it whole as root sound `click`
/// at Edit frame [`SOUND_FRAME`]. The plain WAVE header declares no speaker
/// layout, so registration states the explicit stereo L/R interpretation that
/// audition and export then use; nothing is guessed from the channel count.
fn place_click_sound(project: &mut Project) -> Result {
    let wav = project.directory.join("sound.wav");
    fs::copy(sound_media(), &wav)?;
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
            "streams": {"type": "audio_only", "stream": 0, "interpretation": "stereo_left_right"}
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
    Ok(())
}

/// `:gag one-more-time plays=3 gap=12f shorten=6f` on Edit [12, 24)
/// (Original 24..36), through the headless semantic path: three plays with
/// silent freeze gaps of 12 and then 6 frames, each holding the play's last
/// picture (Original 35), grouped: 12 + 3·12 + 12 + 6 + 6 = 72 frames.
pub fn one_more_time(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "one-more-time")?;
    project.shorten()?;
    project.split_root(12)?;
    let document = project.split_root(24)?;
    let beat = root_child_at(&document, 12)?.0;
    let saved = project.run_semantic(
        json!([{"type":"gag","recipe":{"recipe":"one_more_time","version":1,"plays":3,
            "gap":{"unit":"frames","frames":12},"shorten":{"unit":"frames","frames":6}}}]),
        12,
        Some(&beat),
    )?;
    let (group, _) = root_child_at(&saved, 12)?;
    let label = &saved.nodes()[&group].label;
    if label != "One More Time · v1 · 3 plays, gap 12f shortening by 6f" {
        return Err(format!("unexpected gag label {label:?}").into());
    }
    let mut expectations = vec![(0, base(0)), (11, base(11))];
    for (start, gap) in [(12u64, Some(12u64)), (36, Some(6)), (54, None)] {
        expectations.extend([
            (start, original(24)),
            (start + 6, original(30)),
            (start + 11, original(35)),
        ]);
        if let Some(gap) = gap {
            expectations.extend([(start + 12, original(35)), (start + 11 + gap, original(35))]);
        }
    }
    expectations.extend([(66, original(36)), (71, original(41))]);
    let mut fixture = project.finish(vec!["One More Time"], expectations, Vec::new())?;
    // Each play holds the click 9,562 samples after its start (base click
    // 28,781 less B(12) = 19,219), at the play's own rounded start: B(12),
    // B(36) = 57,658 and B(54) = 86,486. The gaps between are silent.
    fixture.audio = vec![
        (28_672, true),
        (45_056, false),
        (67_072, true),
        (80_128, false),
        (96_000, true),
    ];
    Ok(fixture)
}

/// `:repeat 3 gap=200ms,120ms gain-step=3dB zoom-step=0.08` as the native
/// command records it (`SetRepeat` with plays, gaps and escalation) on Edit
/// [12, 24), through the headless semantic path. 200 ms and 120 ms round once
/// to 6 and 4 frames: 12 + 3·12 + 6 + 4 + 6 = 64 frames. Gaps hold Original 35.
pub fn repeat_gaps_steps(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "repeat-gaps-steps")?;
    project.shorten()?;
    project.split_root(12)?;
    let document = project.split_root(24)?;
    let beat = root_child_at(&document, 12)?.0;
    let escalation = serde_json::to_value(RepeatEscalation {
        gain_step: GainDb::new(3_000)?,
        zoom: Some(ZoomStep {
            step: grid_step()?,
            progression: ZoomProgression::Add,
        }),
    })?;
    let saved = project.run_semantic(
        json!([{"type":"set_repeat","plays":3,
            "gaps":[{"unit":"milliseconds","milliseconds":200},{"unit":"milliseconds","milliseconds":120}],
            "escalation":escalation}]),
        12,
        Some(&beat),
    )?;
    let (repeat, _) = root_child_at(&saved, 12)?;
    let NodeKind::Repeat {
        gap: Some(gap),
        escalation: Some(escalation),
        ..
    } = &saved.nodes()[&repeat].kind
    else {
        return Err("the beat was wrapped with gap and escalation".into());
    };
    if gap.duration.frames() != 6 || escalation.gain_step.millidecibels() != 3000 {
        return Err(format!("unexpected Repeat {gap:?} {escalation:?}").into());
    }
    let mut expectations = vec![(0, base(0)), (11, base(11))];
    for (start, gap) in [(12u64, Some(6u64)), (30, Some(4)), (46, None)] {
        expectations.extend([
            (start, original(24)),
            (start + 6, original(30)),
            (start + 11, original(35)),
        ]);
        if let Some(gap) = gap {
            expectations.extend([(start + 12, original(35)), (start + 11 + gap, original(35))]);
        }
    }
    expectations.extend([(58, original(36)), (63, original(41))]);
    let mut fixture = project.finish(
        vec![
            "`:repeat 3 gap=120ms gain-step=3dB zoom-step=0.08`",
            "Escalation",
        ],
        expectations,
        Vec::new(),
    )?;
    // Clicks 9,562 samples into plays at B(12), B(30) = 48,048 and
    // B(46) = 73,674; silent gaps [38,438, 48,048) and [67,267, 73,674).
    fixture.audio = vec![
        (28_672, true),
        (43_008, false),
        (57_600, true),
        (70_144, false),
        (83_200, true),
    ];
    Ok(fixture)
}

/// `:gag nothing-happens register=r tone=12f silence=12f` at Edit 15,
/// through the headless semantic path (planner, pause-picture resolver,
/// `copied_moment_audio` and store admission). Register `r` holds the
/// Original moment [28, 31) exactly as a native `"ry` copy stores it. Two
/// 12-frame freezes of Original 26 follow, the first looping room tone from
/// that moment's audio, the second true silence: 30 + 24 = 54 frames.
pub fn nothing_happens(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "nothing-happens")?;
    project.shorten()?;
    {
        let mut store = ProjectStore::open(&project.package, AccessMode::ReadWrite)?;
        let document = store.snapshot()?;
        let record = &document.assets()[&project.asset];
        store.save_register(
            document.project_id(),
            document.revision_id(),
            RegisterName::new('r')?,
            RegisterValue::Original {
                revision: document.revision_id().clone(),
                asset: project.asset.clone(),
                qualification: record
                    .source_qualification
                    .clone()
                    .ok_or("the Original is qualified")?,
                ordinals: 28..31,
            },
        )?;
    }
    let saved = project.run_semantic(
        json!([{"type":"gag","recipe":{"recipe":"nothing_happens","version":1,
            "tone":{"unit":"frames","frames":12},"silence":{"unit":"frames","frames":12},
            "register":"r"}}]),
        15,
        None,
    )?;
    let (group, _) = root_child_at(&saved, 15)?;
    let parts: Vec<String> = match &saved.nodes()[&group].kind {
        NodeKind::Sequence { children } => children
            .iter()
            .map(|child| format!("{:?}", saved.nodes()[child].kind))
            .collect(),
        _ => Vec::new(),
    };
    if parts.len() != 2 || !parts[0].contains("RoomTone") || !parts[1].contains("Silence") {
        return Err(format!("unexpected Nothing Happens parts {parts:?}").into());
    }
    let expectations = vec![
        (0, base(0)),
        (14, original(26)),
        (15, original(26)),
        (26, original(26)),
        (27, original(26)),
        (38, original(26)),
        (39, original(27)),
        (53, original(41)),
    ];
    let mut fixture = project.finish(
        vec!["Nothing Happens", "Room tone", "Frozen stare"],
        expectations,
        Vec::new(),
    )?;
    // Moment [28, 31) hears source samples [44,845, 49,649): In rounds up
    // from 44,844.8, Out down from 49,649.6. The 4,804-sample loop holds the
    // click at 48,000, 3,155 samples in, and repeats every 4,708 samples
    // (less its 96-sample crossfade) from the Hold origin B(15) = 24,024:
    // clicks at 27,179, 31,887, 36,595 and 41,303. The silence Hold,
    // [43,243.2, 62,462.4), is digital silence, and the Original's own click
    // moves 24 frames (38,438.4 samples) later, to 67,219.
    fixture.audio = vec![
        (23_552, false),
        (27_136, true),
        (31_744, true),
        (36_352, true),
        (41_216, true),
        (43_520, false),
        (52_480, false),
        (61_952, false),
        (67_072, true),
    ];
    Ok(fixture)
}

/// `:audio-lag +50ms` on the base beat over Edit [10, 20): its Source's
/// sound plays 2,400 samples after its picture (the click moves 1.5 frames
/// later, still inside the beat). Pictures are unchanged.
pub fn audio_lag(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "audio-lag")?;
    project.shorten()?;
    project.split_root(10)?;
    project.split_root(20)?;
    project.apply(|_, document, _| {
        let (beat, _) = root_child_at(document, 10)?;
        let (host, _) = source_host(document, &beat)?;
        let NodeKind::Source { source } = &document.nodes()[&host].kind else {
            return Err("the beat hosts no Source".into());
        };
        Ok(Command::SetSourceAudioMapping {
            node: host,
            mapping: source.audio_mapping,
            offset: AudioSample(2_400),
        })
    })?;
    let expectations = [0u64, 9, 10, 17, 19, 20, 29]
        .into_iter()
        .map(|frame| (frame, base(frame)))
        .collect();
    let mut fixture = project.finish(vec!["Audio lag"], expectations, Vec::new())?;
    // The click at Edit sample 28,781 of the base plays exactly 2,400 later.
    fixture.audio = vec![(28_672, false), (30_976, true)];
    Ok(fixture)
}

/// Build every fixture, each in its own subdirectory of `dir`.
pub fn all(dir: &Path) -> Result<Vec<Fixture>> {
    type Builder = fn(&Path) -> Result<Fixture>;
    let builders: [(&str, Builder); 16] = [
        ("repeat-with-gap", repeat_with_gap),
        ("one-more-time", one_more_time),
        ("repeat-gaps-steps", repeat_gaps_steps),
        ("nothing-happens", nothing_happens),
        ("audio-lag", audio_lag),
        ("bed-drop", bed_drop),
        ("mute-range", mute_range),
        ("off-center", off_center),
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
