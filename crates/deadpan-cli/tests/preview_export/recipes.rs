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

#[cfg(feature = "synthetic-worker")]
#[path = "generated.rs"]
pub mod generated;

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
    /// Frame `sampled_frame` of an accepted Generated Hold's sampled master
    /// (one picture per Hold frame, so Hold-local frame `k` shows frame `k`).
    Generated {
        sampled_frame: u64,
    },
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
    /// Independently derived caption text shown at an output frame.
    pub captions: Vec<(u64, Vec<&'static str>)>,
    /// Independently derived waveform properties of limited-bus windows.
    pub signals: Vec<Signal>,
}

/// One left-channel window of the limited audition bus and what it must show.
#[derive(Debug, Clone)]
pub struct Signal {
    pub start: i64,
    pub count: i64,
    /// Zero crossings per second, within a relative tolerance.
    pub crossings_per_second: Option<(f64, f64)>,
    /// Peak over RMS strictly inside this range (a sine is 1.414).
    pub crest: Option<(f64, f64)>,
    /// Absolute peak strictly inside this range.
    pub peak: Option<(f64, f64)>,
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
        Self::create_from(directory, name, &source_media())
    }

    /// A one-Original project whose Original is a copy of `source`.
    fn create_from(directory: &Path, name: &'static str, source: &Path) -> Result<Self> {
        fs::create_dir_all(directory)?;
        let directory = directory.canonicalize()?;
        let media = directory.join("original.mp4");
        fs::copy(source, &media)?;
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
            captions: Vec::new(),
            signals: Vec::new(),
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

/// A micro-loop with explicit seams: Edit [0, 6) as three total plays whose
/// fragment seams `:edge auto plays` marks for the shared short fade, then a
/// 16-frame freeze pause at Edit 24 over which `:cutaway fit=bounce` plays
/// Original 90..96 forward and back. Both edge and pause run through the
/// headless semantic path. 18 + 24 + 16 = 58 frames.
pub fn micro_loop(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "micro-loop")?;
    project.shorten()?;
    project.split_root(6)?;
    project.apply(|_, document, _| {
        Ok(Command::WrapRepeat {
            node: root_child_at(document, 0)?.0,
            id: node("loop")?,
            plays: 3,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        })
    })?;
    // The repeated fragment's seams add no fade until marked; `:edge auto
    // plays` marks them for the shared short fade.
    let saved = project.run_semantic(
        json!([{"type":"set_audio_edges","side":"plays","policy":"automatic"}]),
        0,
        Some(&node("loop")?),
    )?;
    let NodeKind::Repeat { child, .. } = &saved.nodes()[&node("loop")?].kind else {
        return Err("the loop is a Repeat".into());
    };
    let marks = saved.nodes()[child].audio_editorial_edges;
    if !(marks.start && marks.end) {
        return Err(format!("play seams are not marked: {marks:?}").into());
    }
    let saved = project.run_semantic(
        json!([{"type":"insert_pause","length":{"unit":"frames","frames":16}}]),
        24,
        None,
    )?;
    let (pause, start) = root_child_at(&saved, 24)?;
    if start != 24 || !matches!(saved.nodes()[&pause].kind, NodeKind::Hold { .. }) {
        return Err("the pause is the root child at Edit 24".into());
    }
    let asset = project.asset.clone();
    project.apply(|_, document, _| {
        Ok(Command::SetCutaways {
            node: pause.clone(),
            cutaways: vec![Cutaway {
                range: FrameRange::new(ProjectFrame(0), ProjectFrame(16))?,
                asset: asset.clone(),
                selection: ordinal_span(document, &asset, 90, 96)?,
                fit: CutawayFit::Bounce,
                removed: false,
            }],
        })
    })?;
    let mut expectations = Vec::new();
    for play in 0..3u64 {
        expectations.extend([(play * 6, original(12)), (play * 6 + 5, original(17))]);
    }
    expectations.extend([(18, base(6)), (23, base(11))]);
    // Bounce: forward 90..95, back 95..90, then forward again.
    for (frame, ordinal) in [
        (0, 90),
        (5, 95),
        (6, 95),
        (7, 94),
        (11, 90),
        (12, 90),
        (13, 91),
        (15, 93),
    ] {
        expectations.push((24 + frame, original(ordinal)));
    }
    expectations.extend([(40, base(12)), (57, base(29))]);
    project.finish(vec!["Micro-loop", "Cutaways"], expectations, Vec::new())
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
                removed: false,
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

/// Explode `repeat` in place through the public command path.
fn explode(project: &mut Project, repeat: &str) -> Result<ProjectDocument> {
    project.apply(|project, document, revision| {
        let needs = document.explode_requirements(&node(repeat)?)?;
        Ok(Command::Explode {
            node: node(repeat)?,
            identities: deadpan_core::OccurrenceIdentities {
                nodes: project.fresh(needs.nodes)?,
                marks: (0..needs.marks)
                    .map(|n| deadpan_core::MarkId::new(format!("{revision}-mark-{n}")))
                    .collect::<std::result::Result<_, _>>()?,
            },
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })
}

/// [`repeat_with_gap`] with a quieter second play (an override), then
/// `:explode`: an ordinary Sequence of three independent plays and two gap
/// Holds whose pictures and timing are the Repeat's.
pub fn exploded_repeat(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "exploded-repeat")?;
    project.shorten()?;
    project.split_root(12)?;
    project.split_root(24)?;
    let document = project.apply(|_, document, _| {
        Ok(Command::WrapRepeat {
            node: root_child_at(document, 12)?.0,
            id: node("repeat")?,
            plays: 3,
            gap: Some(silent(6, HoldVideo::Background)?),
            anchor_policy: WrapAnchorPolicy::First,
        })
    })?;
    let NodeKind::Repeat {
        child, iterations, ..
    } = &document.nodes()[&node("repeat")?].kind
    else {
        return Err("repeat".into());
    };
    let (child, second) = (child.clone(), iterations.at(1).ok_or("second play")?);
    project.apply(|project, _, _| {
        Ok(Command::EditOccurrence {
            instance: deadpan_core::InstancePath {
                node: child,
                repeats: vec![deadpan_core::RepeatInstance {
                    node: node("repeat")?,
                    iteration: second,
                }],
            },
            edit: deadpan_core::OccurrenceEdit::SetAudioTreatments {
                treatments: AudioTreatments::from_clip_gain(ClipGain::new(
                    GainDb::new(-6_000)?,
                    false,
                    Vec::new(),
                    Vec::new(),
                )?),
            },
            identities: deadpan_core::OccurrenceIdentities {
                nodes: project.fresh(4)?,
                marks: Vec::new(),
            },
        })
    })?;
    let exploded = explode(&mut project, "repeat")?;
    if !matches!(
        exploded.nodes()[&node("repeat")?].kind,
        NodeKind::Sequence { ref children } if children.len() == 5
    ) {
        return Err("explode makes three plays and two gaps".into());
    }
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

/// [`escalating_repeat`] exploded: per-play scale and gain become explicit
/// groups around plays two and three, with the same pictures and timing.
pub fn exploded_escalation(dir: &Path) -> Result<Fixture> {
    let mut fixture = escalating_repeat(dir)?;
    fixture.revision = explode_fixture(&fixture, "escalator")?;
    fixture.name = "exploded-escalation";
    Ok(fixture)
}

/// Commit `:explode` of `repeat` to an already built fixture and return the
/// new revision. Rendering it must equal rendering the fixture before.
pub fn explode_fixture(fixture: &Fixture, repeat: &str) -> Result<String> {
    let document = ProjectStore::open(&fixture.package, AccessMode::ReadOnly)?.snapshot()?;
    let asset = document
        .assets()
        .iter()
        .find(|(_, record)| record.video.is_some())
        .map(|(id, _)| id.clone())
        .ok_or("fixture has a picture asset")?;
    let mut project = Project {
        name: fixture.name,
        directory: fixture
            .package
            .parent()
            .ok_or("fixture package has a parent")?
            .to_owned(),
        package: fixture.package.clone(),
        step: 80,
        ids: 8000,
        asset,
    };
    Ok(explode(&mut project, repeat)?.revision_id().to_string())
}

/// `:duplicate` of Edit [12, 24): the copy enters at Edit 24 inside the base
/// beat, so Original 24..36 plays twice: 12 + 12 + 12 + 6 = 42 frames.
pub fn duplicated_range(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "duplicated-range")?;
    project.shorten()?;
    project.apply(|project, document, revision| {
        let selection = deadpan_core::SliceCaptureSelection::Range {
            range: FrameRange::new(ProjectFrame(12), ProjectFrame(24))?,
        };
        let timing = AudioTimingId {
            allocation: revision.clone(),
            ordinal: 0,
        };
        let needs = document.duplicate_requirements(document.root(), &selection, &timing)?;
        Ok(Command::Duplicate {
            parent: document.root().clone(),
            selection,
            identities: deadpan_core::SlicePasteIdentities {
                authored: deadpan_core::OccurrenceIdentities {
                    nodes: project.fresh(needs.slice.nodes)?,
                    marks: (0..needs.slice.marks)
                        .map(|n| deadpan_core::MarkId::new(format!("{revision}-mark-{n}")))
                        .collect::<std::result::Result<_, _>>()?,
                },
                aliases: project.fresh(needs.slice.aliases)?,
            },
            split_identities: SplitIdentities {
                nodes: project.fresh(needs.split_nodes)?,
            },
            timing,
        })
    })?;
    let expectations = vec![
        (0, base(0)),
        (11, base(11)),
        (12, original(24)),
        (23, original(35)),
        (24, original(24)),
        (30, original(30)),
        (35, original(35)),
        (36, original(36)),
        (41, original(41)),
    ];
    project.finish(vec!["Callback"], expectations, Vec::new())
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

/// `:sting`: Deadpan's own synthesized triumphant sting
/// (`deadpan_audio::triumphant_sting_wav`) registered like any user sound and
/// placed at Edit frame 10 of the whole, unshortened Original. The chord
/// window's peak is predicted from the synthesized samples themselves; before
/// and after the sting the bus is silent. Pictures are unchanged.
pub fn triumphant_sting(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "triumphant-sting")?;
    let wav = project.directory.join(deadpan_audio::STING_FILE_NAME);
    fs::write(&wav, deadpan_audio::triumphant_sting_wav())?;
    place_catalog_sound(&mut project, &wav, deadpan_audio::STING_LABEL)?;
    let expectations = [0u64, 10, 40, 119]
        .into_iter()
        .map(|frame| (frame, original(frame)))
        .collect();
    let mut fixture = project.finish(vec!["Wrongly triumphant sting"], expectations, Vec::new())?;
    // Edit frame 10 is sample 16,016; the chord sounds 0.9 s later.
    let onset = 16_016_i64;
    let chord = onset + 43_200;
    let samples = deadpan_audio::triumphant_sting();
    let window = usize::try_from(chord - onset)?..usize::try_from(chord - onset + 2_048)?;
    let peak = samples[window]
        .iter()
        .fold(0.0_f64, |peak, frame| peak.max(frame[0].abs()));
    fixture.signals = vec![Signal {
        start: chord,
        count: 2_048,
        crossings_per_second: None,
        crest: None,
        peak: Some((peak * 0.97, peak * 1.03)),
    }];
    fixture.audio = vec![(8_000, false), (onset + 86_400 + 2_000, false)];
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
    place_catalog_sound(project, &wav, "Click sound")
}

/// Register the stereo WAV at `wav` as the catalog sound and place it whole
/// as a root sound starting at Edit frame [`SOUND_FRAME`].
fn place_catalog_sound(project: &mut Project, wav: &Path, label: &str) -> Result {
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
                "label": label,
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

/// `:gag one-more-time plays=3 gap=12f shorten=6f` on Edit [12, 24), then
/// `:gag-set plays=2 gap=8f shorten=2f` on the inserted group through the
/// headless semantic path (`SetGag`): the Repeat drops to two plays with one
/// 8-frame freeze gap and the label is repinned, so the export shows exactly
/// what inserting the new parameters would.
pub fn gag_set(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "gag-set")?;
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
    let saved = project.run_semantic(
        json!([{"type":"set_gag","recipe":{"recipe":"one_more_time","version":1,"plays":2,
            "gap":{"unit":"frames","frames":8},"shorten":{"unit":"frames","frames":2}}}]),
        12,
        Some(&group),
    )?;
    let label = &saved.nodes()[&group].label;
    if label != "One More Time · v1 · 2 plays, gap 8f shortening by 2f" {
        return Err(format!("unexpected gag label {label:?}").into());
    }
    let expectations = vec![
        (0, base(0)),
        (11, base(11)),
        (12, original(24)),
        (18, original(30)),
        (23, original(35)),
        (24, original(35)),
        (31, original(35)),
        (32, original(24)),
        (43, original(35)),
        (44, original(36)),
        (49, original(41)),
    ];
    project.finish(
        vec!["Exposed parameter editing after insertion", "One More Time"],
        expectations,
        Vec::new(),
    )
}

/// `:gag one-more-time plays=3 gap=12f shorten=6f vary=25% seed=7` on Edit
/// [12, 24): seeded variation resolves each gap once from the pinned seed
/// (`GagVariation::vary`), so the stored gap Holds, pictures and clicks follow
/// those exact lengths in preview and export alike.
pub fn one_more_time_varied(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "one-more-time-varied")?;
    project.shorten()?;
    project.split_root(12)?;
    let document = project.split_root(24)?;
    let beat = root_child_at(&document, 12)?.0;
    let saved = project.run_semantic(
        json!([{"type":"gag","recipe":{"recipe":"one_more_time","version":1,"plays":3,
            "gap":{"unit":"frames","frames":12},"shorten":{"unit":"frames","frames":6},
            "variation":{"percent":25,"seed":7}}}]),
        12,
        Some(&beat),
    )?;
    let (group, _) = root_child_at(&saved, 12)?;
    let label = &saved.nodes()[&group].label;
    if !label.ends_with("varied ±25% (seed 7)") {
        return Err(format!("unexpected gag label {label:?}").into());
    }
    let variation = deadpan_core::GagVariation {
        percent: 25,
        seed: 7,
    };
    let gaps = [
        u64::from(variation.vary(12, 0)),
        u64::from(variation.vary(6, 1)),
    ];
    if gaps == [12, 6] {
        return Err("seed 7 left both gaps unchanged".into());
    }
    let mut expectations = vec![(0, base(0)), (11, base(11))];
    let mut start = 12u64;
    let mut starts = Vec::new();
    for gap in [Some(gaps[0]), Some(gaps[1]), None] {
        starts.push(start);
        expectations.extend([
            (start, original(24)),
            (start + 6, original(30)),
            (start + 11, original(35)),
        ]);
        if let Some(gap) = gap {
            expectations.extend([(start + 12, original(35)), (start + 11 + gap, original(35))]);
        }
        start += 12 + gap.unwrap_or(0);
    }
    expectations.extend([(start, original(36)), (start + 5, original(41))]);
    let mut fixture = project.finish(
        vec!["Seeded variation", "Saved gags"],
        expectations,
        vec![format!("seeded gaps {gaps:?}")],
    )?;
    // B(f) = round(f · 1601.6) on the 48 kHz grid; each play's click is
    // 9,562 samples after its start; each gap's middle is silent.
    let boundary = |frame: u64| -> i64 { ((frame * 16_016 + 5) / 10) as i64 };
    fixture.audio = starts
        .iter()
        .map(|start| (boundary(*start) + 9_562 - 100, true))
        .chain(
            starts
                .iter()
                .zip(gaps)
                .map(|(start, gap)| (boundary(start + 12 + gap / 2) - 128, false)),
        )
        .collect();
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

/// `:reverse 8f` at Edit 20 (Original 32), through the headless semantic
/// path: an 8-frame pause plays Edit [12, 20) (Original 24..32) backwards,
/// picture and sound, then the base continues forward: 30 + 8 = 38 frames.
pub fn reverse_hiccup(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "reverse-hiccup")?;
    project.shorten()?;
    let saved = project.run_semantic(
        json!([{"type":"insert_reverse","length":{"unit":"frames","frames":8}}]),
        20,
        None,
    )?;
    let (pause, _) = root_child_at(&saved, 20)?;
    if !matches!(
        &saved.nodes()[&pause].kind,
        NodeKind::Hold { recipe } if matches!(
            (&recipe.video, &recipe.audio),
            (HoldVideo::Reverse { .. }, HoldAudio::Reverse { .. })
        )
    ) {
        return Err(format!("unexpected reverse pause {:?}", saved.nodes()[&pause].kind).into());
    }
    let expectations = vec![
        (0, base(0)),
        (19, original(31)),
        (20, original(31)),
        (21, original(30)),
        (24, original(27)),
        (27, original(24)),
        (28, original(32)),
        (37, original(41)),
    ];
    let mut fixture = project.finish(
        vec!["Reverse hiccup", "Stretch / pitch"],
        expectations,
        Vec::new(),
    )?;
    // The pause hears source samples [38,439, 51,251) backwards: In rounds up
    // from the exact source point 38,438.2 of B(12) = 19,219, Out down from
    // 51,251.2 of B(20) = 32,032. Output sample n plays source 51,250 - n,
    // so the click at 48,000 lands 3,250 samples in, at 35,282. The forward
    // click (28,781) still plays before the pause; none follows it.
    fixture.audio = vec![
        (28_672, true),
        (32_256, false),
        (35_072, true),
        (38_400, false),
        (50_000, false),
    ];
    Ok(fixture)
}

/// `:ping-pong 12f` at Edit 24 (Original 36): Edit [12, 24) (Original
/// 24..36) bounces back without showing Original 35 twice, an 11-frame pause
/// of Original 34 down to 24, then the base resumes at Original 36:
/// 30 + 11 = 41 frames.
pub fn ping_pong(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "ping-pong")?;
    project.shorten()?;
    project.run_semantic(
        json!([{"type":"insert_reverse","length":{"unit":"frames","frames":12},"bounce":true}]),
        24,
        None,
    )?;
    let expectations = vec![
        (0, base(0)),
        (23, original(35)),
        (24, original(34)),
        (25, original(33)),
        (30, original(28)),
        (34, original(24)),
        (35, original(36)),
        (40, original(41)),
    ];
    let mut fixture = project.finish(vec!["Ping-pong hold"], expectations, Vec::new())?;
    // Sound under Edit [12, 23), source [38,439, 56,056), reversed from the
    // pause start B(24) = 38,438: the click at 48,000 plays 8,055 samples in,
    // at 46,493.
    fixture.audio = vec![
        (28_672, true),
        (40_000, false),
        (46_336, true),
        (52_000, false),
        (60_000, false),
    ];
    Ok(fixture)
}

/// `,h` then `:tail 20f` on the new pause, through the headless semantic
/// path: a 30-frame freeze of Original 29 at Edit 18, just after the click,
/// whose reverb of the two seconds heard before it (the processed edit, read
/// live at render time) rings for 20 frames and fades to digital silence;
/// the base resumes at Original 30: 60 frames.
pub fn hanging_tail(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "hanging-tail")?;
    project.shorten()?;
    let saved = project.run_semantic(
        json!([
            {"type":"insert_pause","length":{"unit":"frames","frames":30}},
            {"type":"tail","length":{"unit":"frames","frames":20}}
        ]),
        18,
        None,
    )?;
    let (pause, _) = root_child_at(&saved, 18)?;
    match &saved.nodes()[&pause].kind {
        NodeKind::Hold { recipe }
            if matches!(
                recipe.audio,
                HoldAudio::Tail { maximum, effect: deadpan_core::TailEffect::Reverb, .. }
                    if maximum.frames() == 20
            ) && recipe.duration.frames() == 30 => {}
        other => return Err(format!("unexpected tail pause {other:?}").into()),
    }
    let expectations = vec![
        (0, base(0)),
        (17, original(29)),
        (18, original(29)),
        (47, original(29)),
        (48, original(30)),
        (59, original(41)),
    ];
    let mut fixture = project.finish(
        vec![
            "Hanging tail",
            "Tails",
            "`,t` reverb tail",
            "`:tail 400ms effect=reverb`",
        ],
        expectations,
        Vec::new(),
    )?;
    // The click (28,781) plays before the pause starts at B(18) = 28,829.
    // Reverb reaches the pause after its shortest comb delay (about 1,167
    // samples), and its 20-frame ring (32,032 samples) has faded to exact
    // zero by 60,861; the rest of the pause is digital silence.
    fixture.audio = vec![
        (28_672, true),
        (28_864, false),
        (61_184, false),
        (70_000, false),
    ];
    Ok(fixture)
}

/// `:tail 1s effect=delay` at Edit 18 with no pause selected: a new 30-frame
/// freeze whose 300 ms echo of the click (28,781) repeats inside the pause
/// at 43,181 and, at half level, 57,581: 60 frames.
pub fn tail_echo(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "tail-echo")?;
    project.shorten()?;
    project.run_semantic(
        json!([{"type":"tail","length":{"unit":"frames","frames":30},"effect":"delay"}]),
        18,
        None,
    )?;
    let expectations = vec![(17, original(29)), (18, original(29)), (48, original(30))];
    let mut fixture = project.finish(vec!["Tails"], expectations, Vec::new())?;
    fixture.audio = vec![
        (28_672, true),
        (30_000, false),
        (43_008, true),
        (50_000, false),
        (64_000, false),
    ];
    Ok(fixture)
}

/// `:hold 12f video=black` at Edit 15 captioned `:caption Hello? at=center`,
/// then `:caption Are we done? at=top delay=4f` on the base beat before it,
/// both through the headless semantic path: captions over Original and black
/// pictures, with every picture and sound unchanged: 30 + 12 = 42 frames.
pub fn delayed_caption(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "delayed-caption")?;
    project.shorten()?;
    project.run_semantic(
        json!([
            {"type":"insert_pause","length":{"unit":"frames","frames":12},"black":true},
            {"type":"set_caption","text":"Hello?","placement":"center"}
        ]),
        15,
        None,
    )?;
    let document = project.document()?;
    let (first, _) = root_child_at(&document, 0)?;
    let saved = project.run_semantic(
        json!([{"type":"set_caption","text":"Are we done?","placement":"top",
            "delay":{"unit":"frames","frames":4}}]),
        0,
        Some(&first),
    )?;
    if saved.duration()?.frames() != 42 {
        return Err("captions must not change timing".into());
    }
    let expectations = vec![
        (0, base(0)),
        (4, base(4)),
        (14, base(14)),
        (15, Expected::Background),
        (26, Expected::Background),
        (27, base(15)),
        (41, base(29)),
    ];
    let mut fixture = project.finish(
        vec!["Delayed caption"],
        expectations,
        vec!["captions drawn by the shared GPU pass over Original and black pictures".into()],
    )?;
    fixture.captions = vec![
        (3, vec![]),
        (4, vec!["Are we done?"]),
        (14, vec!["Are we done?"]),
        (15, vec!["Hello?"]),
        (26, vec!["Hello?"]),
        (27, vec![]),
    ];
    // The click (base 28,781) moves 12 frames later, to 48,000; the black
    // pause is silent.
    fixture.audio = vec![(24_576, false), (47_872, true), (52_000, false)];
    Ok(fixture)
}

/// `:gag are-we-done register=r pause=12f` at Edit 15 through the headless
/// semantic path: a 12-frame pause whose sound is the reverb tail of what
/// precedes it while the picture cuts to the reaction in register `r`
/// (Original moment [28, 31), holding 30), grouped: 30 + 12 = 42 frames.
pub fn are_we_done(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "are-we-done")?;
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
        json!([{"type":"gag","recipe":{"recipe":"are_we_done","version":1,
            "pause":{"unit":"frames","frames":12},"register":"r"}}]),
        15,
        None,
    )?;
    let (group, _) = root_child_at(&saved, 15)?;
    let label = &saved.nodes()[&group].label;
    if label != "Are We Done? · v1 · pause 12f with a reverb tail, reaction from register r" {
        return Err(format!("unexpected gag label {label:?}").into());
    }
    let expectations = vec![
        (0, base(0)),
        (14, original(26)),
        (15, original(28)),
        (16, original(29)),
        (17, original(30)),
        (26, original(30)),
        (27, original(27)),
        (41, original(41)),
    ];
    let mut fixture = project.finish(
        vec!["Are We Done?", "Reaction cutaway", "Hanging tail"],
        expectations,
        Vec::new(),
    )?;
    // The two seconds before the pause hold no click, so its tail is quiet;
    // the click moves 12 frames later, to 48,000.
    fixture.audio = vec![(30_000, false), (47_872, true)];
    Ok(fixture)
}

/// Visual Edit [10, 15) then `:lift` through the headless semantic path: the
/// range is cut into register `l` and its five frames come back as a silent
/// black pause, so the base keeps its 30 frames and every later picture and
/// sound keeps its time.
pub fn lift(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "lift")?;
    project.shorten()?;
    let saved = project.run_semantic(
        json!([
            {"type":"begin_selection"},
            {"type":"move_frames","forward":true,"count":5},
            {"type":"lift","register":"l"}
        ]),
        10,
        None,
    )?;
    if saved.duration()?.frames() != 30 {
        return Err("a lift keeps the total time".into());
    }
    let expectations = vec![
        (0, base(0)),
        (9, base(9)),
        (10, Expected::Background),
        (14, Expected::Background),
        (15, base(15)),
        (29, base(29)),
    ];
    let mut fixture = project.finish(vec!["Lift"], expectations, Vec::new())?;
    // The lifted pause [B(10), B(15)) = [16,016, 24,024) is silent; the click
    // at 28,781 keeps its time.
    fixture.audio = vec![(16_384, false), (23_552, false), (28_672, true)];
    Ok(fixture)
}

/// Visual Edit [15, 20) then `:bleep level=-3dB` through the headless
/// semantic path: the range's pictures keep playing (Original 27..31) while
/// its sound, which holds the click, becomes a 1 kHz tone; 30 frames.
pub fn bleep(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "bleep")?;
    project.shorten()?;
    let saved = project.run_semantic(
        json!([
            {"type":"begin_selection"},
            {"type":"move_frames","forward":true,"count":5},
            {"type":"bleep","register":"b","level":-3000}
        ]),
        15,
        None,
    )?;
    let (pause, _) = root_child_at(&saved, 15)?;
    if !matches!(
        &saved.nodes()[&pause].kind,
        NodeKind::Hold { recipe } if matches!(
            (&recipe.video, &recipe.audio),
            (HoldVideo::Play { .. }, HoldAudio::Tone { frequency_hz: 1_000, .. })
        )
    ) {
        return Err(format!("unexpected bleep pause {:?}", saved.nodes()[&pause].kind).into());
    }
    let expectations = [0u64, 14, 15, 17, 19, 20, 29]
        .into_iter()
        .map(|frame| (frame, base(frame)))
        .collect();
    let mut fixture = project.finish(vec!["Bleep"], expectations, Vec::new())?;
    // The tone (-3 dBFS) fills [B(15), B(20)) = [24,024, 32,032) where the
    // click was; the base is quiet on both sides.
    fixture.audio = vec![
        (20_000, false),
        (24_576, true),
        (28_672, true),
        (31_488, true),
        (33_024, false),
    ];
    Ok(fixture)
}

/// Two 12-frame 1 kHz tone pauses at -10 dBFS (`HoldAudio::Tone`, exact
/// synthesized sines) inserted before the base: Edit [0, 12) and [12, 24).
fn tone_pauses(project: &mut Project) -> Result<ProjectDocument> {
    project.shorten()?;
    for (at, name) in [(0, "tone-a"), (12, "tone-b")] {
        project.apply(|project, document, revision| {
            let at = ProjectFrame(at);
            let identities = match document.insert_time_target(at)?.split {
                Some(split) => project.fresh(split.required_ids)?,
                None => Vec::new(),
            };
            Ok(Command::InsertTime {
                at,
                hold: HoldRecipe {
                    picture_context: None,
                    duration: FrameDuration::new(12)?,
                    video: HoldVideo::Background,
                    audio: HoldAudio::Tone {
                        frequency_hz: 1_000,
                        level: GainDb::new(-10_000)?,
                    },
                },
                id: node(name)?,
                identities: SplitIdentities { nodes: identities },
                timing: AudioTimingId {
                    allocation: revision.clone(),
                    ordinal: 0,
                },
            })
        })?;
    }
    project.document()
}

/// Pictures of a tone-pause fixture: background, then the base from Edit 24.
fn tone_expectations() -> Vec<(u64, Expected)> {
    let mut expectations = vec![(0, Expected::Background), (23, Expected::Background)];
    expectations.extend(
        [24u64, 41, 53]
            .into_iter()
            .map(|frame| (frame, base(frame - 24))),
    );
    expectations
}

/// A window well inside tone pause `index` (0 or 1): 4,096 samples from 4
/// frames into it, clear of its 2 ms ramps.
fn tone_window(index: i64) -> (i64, i64) {
    (((index * 12 + 4) * 16_016 + 5) / 10, 4_096)
}

/// `:saturate 12dB` on the first of two -10 dBFS 1 kHz tone pauses, through
/// the headless semantic path. tanh(3.98 · 0.316 · sin) flattens the sine:
/// peak 0.851 and crest factor 1.288, against the untouched second tone's
/// peak 0.316 and crest 1.414.
pub fn saturation(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "saturation")?;
    tone_pauses(&mut project)?;
    let saved = project.run_semantic(
        json!([{"type":"set_audio","change":{"type":"saturation","drive":12000}}]),
        0,
        Some(&node("tone-a")?),
    )?;
    let treatments = &saved.nodes()[&node("tone-a")?].audio_treatments;
    if treatments.order() != [deadpan_core::AudioTreatmentStage::Saturation]
        || treatments
            .saturation()
            .map(|stage| stage.drive().millidecibels())
            != Some(12_000)
    {
        return Err(format!("unexpected treatments {treatments:?}").into());
    }
    let mut fixture = project.finish(vec!["Saturation"], tone_expectations(), Vec::new())?;
    let ((a, count), (b, _)) = (tone_window(0), tone_window(1));
    fixture.signals = vec![
        Signal {
            start: a,
            count,
            crossings_per_second: Some((2_000.0, 0.03)),
            crest: Some((1.26, 1.32)),
            peak: Some((0.83, 0.87)),
        },
        Signal {
            start: b,
            count,
            crossings_per_second: Some((2_000.0, 0.03)),
            crest: Some((1.39, 1.44)),
            peak: Some((0.30, 0.33)),
        },
    ];
    Ok(fixture)
}

/// `-` four times over Edit [3, 9) inside the first of two -10 dBFS 1 kHz
/// tone pauses, through the headless semantic path the native keys share:
/// one constant -12 dB step envelope over exactly that range of the beat.
/// The window inside it drops to peak 0.079 (0.316 / 3.98) while the second
/// tone keeps its 0.316 peak; pictures and timing are unchanged.
pub fn range_gain(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "range-gain")?;
    tone_pauses(&mut project)?;
    let step = json!({"type":"set_audio","change":{"type":"range_step","millidecibels":-3000}});
    let saved = project.run_semantic(
        json!([
            {"type":"begin_selection"},
            {"type":"move_frames","forward":true,"count":6},
            step, step, step, step,
            {"type":"clear_selection"}
        ]),
        3,
        Some(&node("tone-a")?),
    )?;
    let clip = saved.nodes()[&node("tone-a")?]
        .audio_treatments
        .clip_gain()
        .cloned()
        .ok_or("the range step adds clip gain")?;
    let range = deadpan_core::GainRange::new(ExactRatio::integer(3), ExactRatio::integer(9))?;
    if clip.trim() != GainDb::UNITY
        || clip.envelopes().len() != 1
        || clip.range_step(range) != Some(GainDb::new(-12_000)?)
    {
        return Err(format!("unexpected range gain {clip:?}").into());
    }
    let mut fixture = project.finish(
        vec!["`+` / `-` gain", "Selective emphasis"],
        tone_expectations(),
        Vec::new(),
    )?;
    let ((a, count), (b, _)) = (tone_window(0), tone_window(1));
    fixture.signals = vec![
        Signal {
            start: a,
            count,
            crossings_per_second: Some((2_000.0, 0.03)),
            crest: Some((1.39, 1.44)),
            peak: Some((0.075, 0.084)),
        },
        Signal {
            start: b,
            count,
            crossings_per_second: Some((2_000.0, 0.03)),
            crest: Some((1.39, 1.44)),
            peak: Some((0.30, 0.33)),
        },
    ];
    Ok(fixture)
}

/// `:pitch +12st` on the first of two 1 kHz tone pauses: a unity-speed
/// Retime shifting it one octave on the pitch-preserving processor. Its zero
/// crossings double (about 4,000 per second) while the untouched second tone
/// stays at 2,000; timing and pictures are unchanged.
pub fn pitch_shift(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "pitch-shift")?;
    tone_pauses(&mut project)?;
    let saved = project.apply(|_, _, _| {
        Ok(Command::WrapRetime {
            node: node("tone-a")?,
            id: NodeId::new("shift").expect("constant identity"),
            duration: FrameDuration::new(12).expect("positive duration"),
            pitch: PitchPolicy::Shift { semitones: 12 },
        })
    })?;
    if saved.duration()?.frames() != BASE_FRAMES as i64 + 24 {
        return Err("a pitch shift changed the duration".into());
    }
    let mut fixture = project.finish(
        vec!["Pitch shift", "Stretch / pitch"],
        tone_expectations(),
        Vec::new(),
    )?;
    let ((a, count), (b, _)) = (tone_window(0), tone_window(1));
    fixture.signals = vec![
        Signal {
            start: a,
            count,
            crossings_per_second: Some((4_000.0, 0.05)),
            crest: None,
            peak: None,
        },
        Signal {
            start: b,
            count,
            crossings_per_second: Some((2_000.0, 0.03)),
            crest: None,
            peak: None,
        },
    ];
    Ok(fixture)
}

/// Keep Original [10, 24) then [33, 49) as a 30-frame edit and `:jcut 6f` at
/// their cut (Edit 14). The second beat's sound starts 6 frames early, so the
/// click at Original sample 48,000 (Original frame 29.97, in that beat's
/// handle) sounds at Edit frame 10.97 (sample 17,570) while the first beat's
/// pictures still show until Edit 14. Without the J-cut nothing sounds there.
pub fn j_cut(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "j-cut")?;
    project.delete_range(49, 120)?;
    project.delete_range(24, 33)?;
    let document = project.delete_range(0, 10)?;
    let right = root_child_at(&document, 14)?;
    if right.1 != 14 || document.duration()?.frames() != 30 {
        return Err("unexpected J-cut base".into());
    }
    let saved = project.run_semantic(
        json!([{"type":"split_edit","kind":"j","length":{"unit":"frames","frames":6}}]),
        14,
        None,
    )?;
    if saved.duration()?.frames() != 30 || root_child_at(&saved, 8)?.1 != 8 {
        return Err("the J-cut did not roll the cut to Edit 8".into());
    }
    let expectations = [0u64, 7, 8, 10, 13, 14, 20, 29]
        .into_iter()
        .map(|frame| {
            (
                frame,
                original(if frame < 14 { 10 + frame } else { 19 + frame }),
            )
        })
        .collect();
    let mut fixture = project.finish(vec!["Premature sound (J-cut)"], expectations, Vec::new())?;
    fixture.audio = vec![(6_400, false), (17_408, true)];
    Ok(fixture)
}

/// Keep Original [10, 28) then [80, 92) and `:lcut 6f` at their cut (Edit
/// 18). The first beat's sound runs on 6 frames under the second beat's
/// pictures, reaching the click at Original frame 29.97 (in its handle) at
/// Edit frame 19.97 (sample 31,980). Without the L-cut nothing sounds there.
pub fn l_cut(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "l-cut")?;
    project.delete_range(92, 120)?;
    project.delete_range(28, 80)?;
    let document = project.delete_range(0, 10)?;
    if root_child_at(&document, 18)?.1 != 18 || document.duration()?.frames() != 30 {
        return Err("unexpected L-cut base".into());
    }
    let saved = project.run_semantic(
        json!([{"type":"split_edit","kind":"l","length":{"unit":"frames","frames":6}}]),
        18,
        None,
    )?;
    if saved.duration()?.frames() != 30 || root_child_at(&saved, 24)?.1 != 24 {
        return Err("the L-cut did not roll the cut to Edit 24".into());
    }
    let expectations = [0u64, 17, 18, 20, 23, 24, 29]
        .into_iter()
        .map(|frame| {
            (
                frame,
                original(if frame < 18 { 10 + frame } else { 62 + frame }),
            )
        })
        .collect();
    let mut fixture = project.finish(vec!["Lingering sound (L-cut)"], expectations, Vec::new())?;
    fixture.audio = vec![(20_000, false), (31_744, true)];
    Ok(fixture)
}

/// `:select role=audio` + `d` over Edit [15, 20), then `:select role=video` +
/// `d` over [23, 27), through the headless semantic path: the click at Edit
/// sample 28,781 is silenced while its pictures stay, and Edit 23..27 show
/// the background while time and every other picture stay.
pub fn role_delete(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "role-delete")?;
    project.shorten()?;
    let saved = project.run_semantic(
        json!([
            {"type":"begin_selection"},
            {"type":"move_frames","forward":true,"count":5},
            {"type":"delete_role","role":"audio"},
            {"type":"move_frames","forward":true,"count":3},
            {"type":"begin_selection"},
            {"type":"move_frames","forward":true,"count":4},
            {"type":"delete_role","role":"video"}
        ]),
        15,
        None,
    )?;
    if saved.duration()?.frames() != BASE_FRAMES as i64 {
        return Err("a role-only delete changed the duration".into());
    }
    let expectations = [0u64, 15, 17, 19, 22, 23, 26, 27, 29]
        .into_iter()
        .map(|frame| {
            (
                frame,
                if (23..27).contains(&frame) {
                    Expected::Background
                } else {
                    base(frame)
                },
            )
        })
        .collect();
    let mut fixture = project.finish(
        vec!["Role-only delete and `audio-shift`", "`:select role=audio`"],
        expectations,
        Vec::new(),
    )?;
    fixture.audio = vec![(20_000, false), (28_672, false)];
    Ok(fixture)
}

/// `:repeat 2 role=video` over Edit [5, 8) and `:repeat 3 role=audio` over
/// [15, 20), through the headless semantic path. No time is added: Edit 8..11
/// show Original 17..19 again while their sound continues, and the click at
/// Edit sample 28,781 sounds again 5 frames (8,008 samples) and 10 frames
/// later as root sound events of the same Original audio, over the muted beat;
/// a later ripple delete of Edit [0, 3) moves pictures, mute and repeats 3
/// frames earlier together.
pub fn role_repeat(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "role-repeat")?;
    project.shorten()?;
    let saved = project.run_semantic(
        json!([
            {"type":"begin_selection"},
            {"type":"move_frames","forward":true,"count":3},
            {"type":"role_repeat","role":"video","plays":2},
            {"type":"move_frames","forward":true,"count":7},
            {"type":"begin_selection"},
            {"type":"move_frames","forward":true,"count":5},
            {"type":"role_repeat","role":"audio","plays":3}
        ]),
        5,
        None,
    )?;
    if saved.duration()?.frames() != BASE_FRAMES as i64
        || saved.sounds().len() != 2
        || !saved.beat_sounds().is_empty()
    {
        return Err(format!(
            "unexpected role repeat: {} frames, {} root sounds",
            saved.duration()?.frames(),
            saved.sounds().len()
        )
        .into());
    }
    // A later ripple delete of Edit [0, 3) moves everything, including the
    // root repeats, 3 frames (B(3) = 4,805 samples) earlier.
    let deleted = project.delete_range(0, 3)?;
    if deleted.sounds().len() != 2 {
        return Err("the ripple delete dropped a repeated sound".into());
    }
    let mut expectations: Vec<(u64, Expected)> = [0u64, 1, 2, 4, 8, 12, 17, 26]
        .into_iter()
        .map(|frame| (frame, base(frame + 3)))
        .collect();
    expectations.extend([(5, base(5)), (6, base(6)), (7, base(7))]);
    let mut fixture = project.finish(
        vec![
            "Audio-only / video-only repeat (§6.5)",
            "Word / syllable stutter",
        ],
        expectations,
        Vec::new(),
    )?;
    fixture.audio = vec![
        (28_681 - 4_805, true),
        (36_689 - 4_805, true),
        (44_697 - 4_805, true),
        (35_195, false),
        (19_195, false),
    ];
    Ok(fixture)
}

/// A development `ffmpeg` CLI with `libx264`/`libx264rgb`
/// (`DEADPAN_BRIDGE_FFMPEG`, else Homebrew's). Tests only: it generates the
/// large fixture and encodes synthetic AI footage; Deadpan never links it.
fn development_ffmpeg() -> std::result::Result<PathBuf, String> {
    let ffmpeg = std::env::var_os("DEADPAN_BRIDGE_FFMPEG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/opt/homebrew/bin/ffmpeg"));
    if ffmpeg.is_file() {
        Ok(ffmpeg)
    } else {
        Err(format!(
            "needs a development ffmpeg with libx264 at {} (or DEADPAN_BRIDGE_FFMPEG)",
            ffmpeg.display()
        ))
    }
}

/// Frames of the generated 1080p Original.
const LARGE_FRAMES: u64 = 180;

/// Generate a large Original deterministically: `width`x`height` `testsrc2`
/// at 30000/1001 for `frames` frames, long-GOP H.264 High (GOP 150, no
/// scene cuts, 3 B frames with pyramid references, 3 references), BT.709
/// limited range with left chroma siting, and 48 kHz stereo AAC carrying a
/// 100 ms linear chirp (300 to 1500 Hz, L 0.7, R -0.6) in every half second
/// from 0.2 s, silent otherwise. The chirps are aperiodic (so alignment can
/// verify a zero offset) and lie at least 0.2 s from every cut of the recipe.
fn large_media(directory: &Path, width: u32, height: u32, frames: u64) -> Result<PathBuf> {
    fs::create_dir_all(directory)?;
    let ffmpeg = development_ffmpeg()?;
    let output = directory.join(format!("large-{height}p-source.mp4"));
    // Audio spans exactly the picture duration.
    let seconds = format!("{:.6}", frames as f64 * 1001.0 / 30000.0);
    let chirp = |gain: f64| {
        format!(
            "if(between(mod(t,0.5),0.2,0.3),{gain}*sin(2*PI*(300*(mod(t,0.5)-0.2)+6000*pow(mod(t,0.5)-0.2,2))),0)"
        )
    };
    let audio = format!(
        "aevalsrc=exprs='{}|{}':s=48000:d={seconds}:c=stereo",
        chirp(0.7),
        chirp(-0.6)
    );
    let status = Process::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args([
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc2=size={width}x{height}:rate=30000/1001"),
        ])
        .args(["-f", "lavfi", "-i", &audio])
        .args(["-frames:v", &frames.to_string()])
        .args([
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-profile:v",
            "high",
        ])
        .args(["-preset", "medium", "-crf", "20", "-threads", "1"])
        .args([
            "-g",
            "150",
            "-keyint_min",
            "150",
            "-sc_threshold",
            "0",
            "-bf",
            "3",
        ])
        .args([
            "-x264-params",
            "b-pyramid=normal:ref=3:colorprim=bt709:transfer=bt709:colormatrix=bt709",
        ])
        .args(["-color_primaries", "bt709", "-color_trc", "bt709"])
        .args(["-colorspace", "bt709", "-color_range", "tv"])
        .args(["-chroma_sample_location", "left"])
        .args(["-video_track_timescale", "30000"])
        .args(["-c:a", "aac", "-b:a", "192k", "-t", &seconds])
        .arg(&output)
        .status()?;
    if !status.success() {
        return Err(format!("ffmpeg could not generate the large fixture: {status}").into());
    }
    Ok(output)
}

/// Insert a freeze pause of `frames` at Edit `at` through the shared pause
/// provider, as the native `,h` does: it freezes the picture before `at`.
fn insert_freeze(project: &mut Project, at: i64, frames: i64, id: &str) -> Result<ProjectDocument> {
    let at = ProjectFrame(at);
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
        let identities = match document.insert_time_target(at)?.split {
            Some(split) => project.fresh(split.required_ids)?,
            None => Vec::new(),
        };
        Ok(Command::InsertTime {
            at,
            hold: HoldRecipe {
                picture_context: provider.picture_context,
                duration: FrameDuration::new(frames)?,
                video: provider.video,
                audio: HoldAudio::Silence,
            },
            id: node(id)?,
            identities: SplitIdentities { nodes: identities },
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })
}

/// Materially larger media: the generated 1920x1080 long-GOP Original
/// ([`large_media`]) shortened to Original [0, 150), a 15-frame freeze pause at
/// Edit 60 (holding Original 59), Edit [105, 135) (Original 90..120) as two
/// total plays, and a static centered 1.35x zoom on Edit [0, 60).
/// 150 + 15 + 30 = 195 frames.
pub fn large_1080p(dir: &Path) -> Result<Fixture> {
    let media = large_media(dir, 1920, 1080, LARGE_FRAMES)?;
    let mut project = Project::create_from(dir, "large-1080p", &media)?;
    project.delete_range(150, LARGE_FRAMES as i64)?;
    insert_freeze(&mut project, 60, 15, "freeze")?;
    project.split_root(105)?;
    project.split_root(135)?;
    project.apply(|_, document, _| {
        Ok(Command::WrapRepeat {
            node: root_child_at(document, 105)?.0,
            id: node("again")?,
            plays: 2,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        })
    })?;
    let half = ratio(1, 2)?;
    let zoomed = FramingPose::new(half, half, ratio(27, 20)?)?;
    project.apply(|_, document, _| {
        Ok(Command::SetFraming {
            node: root_child_at(document, 0)?.0,
            framing: Some(Framing::static_pose(zoomed)?),
        })
    })?;
    let expectations = vec![
        (0, original(0)),
        (30, original(30)),
        (59, original(59)),
        (60, original(59)),
        (67, original(59)),
        (74, original(59)),
        (75, original(60)),
        (104, original(89)),
        (105, original(90)),
        (120, original(105)),
        (134, original(119)),
        (135, original(90)),
        (164, original(119)),
        (165, original(120)),
        (194, original(149)),
    ];
    let notes = vec![format!(
        "Original {} bytes, sha256 {}",
        fs::metadata(&media)?.len(),
        sha256_hex(&fs::read(&media)?)
    )];
    let mut fixture = project.finish(
        vec!["Frozen stare", "Repeat operator", "Smash zoom"],
        expectations,
        notes,
    )?;
    // Chirp k plays Original [0.2 + 0.5k, 0.3 + 0.5k) s. Edit sample = Original
    // sample before the pause, + 24,024 (15 frames) after it, and + 48,048 more
    // in the second play.
    fixture.audio = vec![
        (9_700, true),                  // chirp 0, Original 0.2 s
        (100_000, false),               // inside the freeze pause [96,096, 120,120)
        (125_000, false),               // Original 2.104 s, between chirps
        (130_000, true),                // chirp 4, Original 2.2 s
        (153_600 + 24_024 + 100, true), // chirp 6, Original 3.2 s, first play
        (153_600 + 72_072 + 100, true), // the same chirp in the second play
        (235_200 + 72_072, false),      // Original 4.9 s, after the last chirp
    ];
    Ok(fixture)
}

/// 4K media: a generated 3840x2160 long-GOP Original of 60 frames
/// ([`large_media`]) with an 8-frame freeze pause at Edit 30 (holding Original
/// 29) and a static centered 1.35x zoom on Edit [0, 30). 60 + 8 = 68 frames.
pub fn large_2160p(dir: &Path) -> Result<Fixture> {
    let media = large_media(dir, 3840, 2160, 60)?;
    let mut project = Project::create_from(dir, "large-2160p", &media)?;
    insert_freeze(&mut project, 30, 8, "freeze")?;
    let half = ratio(1, 2)?;
    let zoomed = FramingPose::new(half, half, ratio(27, 20)?)?;
    project.apply(|_, document, _| {
        Ok(Command::SetFraming {
            node: root_child_at(document, 0)?.0,
            framing: Some(Framing::static_pose(zoomed)?),
        })
    })?;
    let expectations = vec![
        (0, original(0)),
        (15, original(15)),
        (29, original(29)),
        (30, original(29)),
        (37, original(29)),
        (38, original(30)),
        (50, original(42)),
        (67, original(59)),
    ];
    let notes = vec![format!(
        "Original {} bytes, sha256 {}",
        fs::metadata(&media)?.len(),
        sha256_hex(&fs::read(&media)?)
    )];
    let mut fixture = project.finish(vec!["Frozen stare", "Smash zoom"], expectations, notes)?;
    // The pause is Edit samples [48,048, 60,861); later Original samples move
    // 12,812.8 later.
    fixture.audio = vec![
        (9_700, true),                 // chirp 0, Original 0.2 s
        (52_000, false),               // inside the freeze pause
        (57_600 + 12_813 + 100, true), // chirp 2, Original 1.2 s
    ];
    Ok(fixture)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Build every fixture, each in its own subdirectory of `dir`; see
/// [`all_with_skips`] for fixtures whose tools are missing.
pub fn all(dir: &Path) -> Result<Vec<Fixture>> {
    let (fixtures, skipped) = all_with_skips(dir)?;
    for (name, reason) in skipped {
        eprintln!("SKIPPED fixture {name}: {reason}");
    }
    Ok(fixtures)
}

/// A fixture that needs a tool outside the build: the generated large
/// Original needs a development `ffmpeg`, accepted Generated Holds also the
/// synthetic worker (`--features synthetic-worker`) and `deadpan-media-worker`.
type Optional = (
    &'static str,
    fn() -> std::result::Result<(), String>,
    Builder,
);

/// Builds one fixture in its own directory.
type Builder = fn(&Path) -> Result<Fixture>;

/// A fixture not built, and why.
pub type Skipped = (&'static str, String);

#[cfg(feature = "synthetic-worker")]
fn generated_tools() -> std::result::Result<(), String> {
    generated::tools().map(|_| ())
}

#[cfg(not(feature = "synthetic-worker"))]
fn generated_tools() -> std::result::Result<(), String> {
    Err("built without --features synthetic-worker".into())
}

#[cfg(feature = "synthetic-worker")]
const GENERATED: [(&str, Builder); 4] = [
    ("generated-pause", generated::generated_pause),
    ("generated-repeat", generated::generated_repeat),
    ("generated-reframe", generated::generated_reframe),
    ("generated-prefix", generated::generated_prefix),
];

#[cfg(not(feature = "synthetic-worker"))]
const GENERATED: [(&str, Builder); 4] = {
    fn unavailable(_: &Path) -> Result<Fixture> {
        Err("built without --features synthetic-worker".into())
    }
    [
        ("generated-pause", unavailable),
        ("generated-repeat", unavailable),
        ("generated-reframe", unavailable),
        ("generated-prefix", unavailable),
    ]
};

/// Build every fixture whose tools are available; return the others' names
/// with the reason they were skipped.
pub fn all_with_skips(dir: &Path) -> Result<(Vec<Fixture>, Vec<Skipped>)> {
    let builders: [(&str, Builder); 38] = [
        ("triumphant-sting", triumphant_sting),
        ("micro-loop", micro_loop),
        ("gag-set", gag_set),
        ("range-gain", range_gain),
        ("saturation", saturation),
        ("role-repeat", role_repeat),
        ("role-delete", role_delete),
        ("one-more-time-varied", one_more_time_varied),
        ("pitch-shift", pitch_shift),
        ("j-cut", j_cut),
        ("l-cut", l_cut),
        ("bleep", bleep),
        ("lift", lift),
        ("are-we-done", are_we_done),
        ("delayed-caption", delayed_caption),
        ("reverse-hiccup", reverse_hiccup),
        ("ping-pong", ping_pong),
        ("hanging-tail", hanging_tail),
        ("tail-echo", tail_echo),
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
        ("exploded-repeat", exploded_repeat),
        ("exploded-escalation", exploded_escalation),
        ("duplicated-range", duplicated_range),
        ("gain-trim", gain_trim),
        ("sound-event", sound_event),
    ];
    let mut optional: Vec<Optional> = GENERATED
        .into_iter()
        .map(|(name, build)| (name, generated_tools as fn() -> _, build))
        .collect();
    for (name, builder) in [
        ("large-1080p", large_1080p as Builder),
        ("large-2160p", large_2160p),
    ] {
        optional.push((name, || development_ffmpeg().map(|_| ()), builder));
    }
    let known = || {
        builders
            .iter()
            .map(|(name, _)| *name)
            .chain(optional.iter().map(|(name, _, _)| *name))
    };
    // DEADPAN_PREVIEW_EXPORT_ONLY=a,b builds only the named fixtures, for a
    // focused rerun; the full run always builds all of them.
    let only = std::env::var("DEADPAN_PREVIEW_EXPORT_ONLY").ok();
    let only: Option<Vec<&str>> = only.as_deref().map(|names| names.split(',').collect());
    if let Some(names) = &only {
        if std::env::var_os("CI").is_some() {
            return Err(
                "DEADPAN_PREVIEW_EXPORT_ONLY must not be set in CI; it skips fixtures".into(),
            );
        }
        if let Some(unknown) = names
            .iter()
            .find(|name| !known().any(|known| known == **name))
        {
            return Err(
                format!("DEADPAN_PREVIEW_EXPORT_ONLY names unknown fixture {unknown}").into(),
            );
        }
        let total = known().count();
        eprintln!(
            "DEADPAN_PREVIEW_EXPORT_ONLY: building {} of {} fixtures; {} SKIPPED. Record results only from a full run.",
            names.len(),
            total,
            total - names.len()
        );
    }
    let selected = |name: &str| only.as_ref().is_none_or(|names| names.contains(&name));
    let build = |name: &str, build: Builder| -> Result<Fixture> {
        build(&dir.join(name)).map_err(|error| format!("{name}: {error}").into())
    };
    let mut fixtures = builders
        .into_iter()
        .filter(|(name, _)| selected(name))
        .map(|(name, builder)| build(name, builder))
        .collect::<Result<Vec<_>>>()?;
    let mut skipped = Vec::new();
    for (name, available, builder) in optional {
        if !selected(name) {
            continue;
        }
        match available() {
            Ok(()) => fixtures.push(build(name, builder)?),
            Err(reason) => skipped.push((name, reason)),
        }
    }
    Ok((fixtures, skipped))
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

/// Commit, after export, the removal of every caption on the host shown at
/// Edit frame `at`, changing nothing else. The earlier movie verified against
/// the returned revision then differs exactly where that caption was drawn.
pub fn clear_captions(fixture: &Fixture, at: i64) -> Result<String> {
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
    let saved = project.apply(|_, document, _| {
        let (beat, _) = root_child_at(document, at)?;
        let (host, _) =
            deadpan_core::cutaway_host(document, &beat).ok_or("the beat hosts no captions")?;
        Ok(Command::SetCaptions {
            node: host,
            captions: Vec::new(),
        })
    })?;
    Ok(saved.revision_id().to_string())
}
