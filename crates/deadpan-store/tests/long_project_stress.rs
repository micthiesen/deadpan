//! Gate G long-project stress. A synthetic two-hour Original (an unqualified
//! legacy asset record: no media is decoded) is cut into thousands of Source
//! beats with interleaved Holds, then edited through the real store with
//! renames, Hold durations, Repeat wraps, Splits, ripple deletions and
//! undo/redo across hundreds of revisions. The package is reopened and
//! validated (receipt and full replay), historical revisions are read, and
//! the head is compiled into a render plan whose picture and audio range
//! queries are timed. Every stage has a wall-time budget and the test thread
//! an allocation budget; measurements are printed as one JSON line and, with
//! `DEADPAN_CHAOS_OUT`, written to `stress.json` there.
//!
//! Default scale (normal test suite): 2,000 beats and 80 edits. Full scale
//! (`DEADPAN_STRESS=full`, run by `cargo xtask chaos --stress` in release):
//! 10,000 beats and 300 edits. Budgets are bounds against pathological
//! growth, not the Section 25 interactive targets, which `cargo xtask perf`
//! measures separately. Decoding and encoding a real two-hour Original are
//! outside this test. See docs/ADVERSARIAL.md.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use deadpan_core::{
    AssetId, AssetRecord, AudioSample, BeatNode, ColorPolicy, Command, CommandRequest,
    FrameDuration, FrameRate, HoldAudio, HoldRecipe, HoldVideo, LinkRelation, NodeId, NodeKind,
    PresentationBasis, ProjectDocument, ProjectFrame, ProjectId, RevisionId, SourceAudio,
    SourceAudioMapping, SourceNode, SourceSpan, SourceTimeBase, SourceTimestamp, SourceVideo,
    SourceVideoMapping, Subtree,
};
use deadpan_plan::{AudioQueryLimits, RenderPlan};
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::{Value, json};

#[global_allocator]
static ALLOCATOR: deadpan_chaos::CountingAllocator = deadpan_chaos::CountingAllocator;

/// 24 fps over a 1/48,000 time base: 2,000 ticks per frame.
const TICKS_PER_FRAME: i64 = 2_000;
const TWO_HOURS_TICKS: i64 = 2 * 60 * 60 * 48_000;

struct Scale {
    beats: usize,
    edits: usize,
    /// (stage, budget) for the debug default; full scale runs in release.
    budgets: BTreeMap<&'static str, Duration>,
    max_alloc: u64,
}

fn scale() -> Scale {
    let full = std::env::var("DEADPAN_STRESS").is_ok_and(|value| value == "full");
    let seconds = |pairs: &[(&'static str, u64)]| {
        pairs
            .iter()
            .map(|(stage, seconds)| (*stage, Duration::from_secs(*seconds)))
            .collect()
    };
    if full {
        Scale {
            beats: 10_000,
            edits: 300,
            budgets: seconds(&[
                ("create", 60),
                ("commit_p95", 2),
                ("reopen", 30),
                ("validate_receipt", 30),
                ("validate_full", 600),
                ("history_reads", 60),
                ("plan_compile", 30),
                ("picture_lookups", 10),
                ("audio_queries", 30),
            ]),
            max_alloc: 8 << 30,
        }
    } else {
        Scale {
            beats: 2_000,
            edits: 80,
            budgets: seconds(&[
                ("create", 60),
                ("commit_p95", 5),
                ("reopen", 30),
                ("validate_receipt", 30),
                ("validate_full", 300),
                ("history_reads", 60),
                ("plan_compile", 30),
                ("picture_lookups", 20),
                ("audio_queries", 30),
            ]),
            max_alloc: 4 << 30,
        }
    }
}

/// Deterministic LCG; the stress is reproducible run to run.
struct Lcg(u64);
impl Lcg {
    fn below(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) % bound.max(1) as u64) as usize
    }
}

fn span(start: i64, end: i64) -> SourceSpan {
    let time_base = SourceTimeBase::new(1, 48_000).unwrap();
    SourceSpan::new(
        SourceTimestamp {
            ticks: start,
            time_base,
        },
        SourceTimestamp {
            ticks: end,
            time_base,
        },
    )
    .unwrap()
}

fn source_beat(asset: &AssetId, start: i64, frames: i64) -> BeatNode {
    let selected = span(start, start + frames * TICKS_PER_FRAME);
    BeatNode {
        audio_treatments: Default::default(),
        framing: None,
        label: "Original moment".into(),
        audio_editorial_edges: Default::default(),
        audio_edges: Default::default(),
        kind: NodeKind::Source {
            source: SourceNode {
                edit_window: None,
                duration: FrameDuration::new(frames).unwrap(),
                video: SourceVideo::Stream {
                    asset: asset.clone(),
                    span: selected,
                },
                video_mapping: SourceVideoMapping::FitBeat,
                audio: Some(SourceAudio {
                    asset: asset.clone(),
                    span: selected,
                }),
                audio_mapping: SourceAudioMapping::FitBeat,
                link: LinkRelation::Linked,
                audio_offset: AudioSample(0),
            },
        },
        cutaways: Vec::new(),
        captions: Vec::new(),
    }
}

fn hold(frames: i64) -> BeatNode {
    BeatNode::hold(
        "Pause",
        HoldRecipe {
            picture_context: None,
            duration: FrameDuration::new(frames).unwrap(),
            video: HoldVideo::Background,
            audio: HoldAudio::Silence,
        },
    )
}

struct Session {
    store: ProjectStore,
    serial: usize,
}

impl Session {
    fn commit(&mut self, command: Command) -> Result<Duration, String> {
        let head = self.store.snapshot().map_err(|error| error.to_string())?;
        self.serial += 1;
        let request = CommandRequest {
            project_id: head.project_id().clone(),
            expected_revision: head.revision_id().clone(),
            new_revision: RevisionId::new(format!("stress-{}", self.serial)).unwrap(),
            command,
        };
        let started = Instant::now();
        self.store
            .commit(&request)
            .map_err(|error| error.to_string())?;
        Ok(started.elapsed())
    }
}

fn root_children(document: &ProjectDocument) -> Vec<NodeId> {
    document.nodes()[document.root()].kind.children().to_vec()
}

fn percentile(samples: &mut [f64], fraction: f64) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples
        .get(((samples.len() as f64 * fraction).ceil() as usize).saturating_sub(1))
        .copied()
        .unwrap_or(0.0)
}

#[test]
fn adversarial_long_project_stays_within_time_and_memory_budgets() {
    let scale = scale();
    let baseline = deadpan_chaos::allocation_baseline();
    let scratch = tempfile::tempdir().unwrap();
    let path = scratch.path().join("long.deadpan");
    let mut timings: BTreeMap<&'static str, f64> = BTreeMap::new();
    let check =
        |stage: &'static str, elapsed: Duration, timings: &mut BTreeMap<&'static str, f64>| {
            timings.insert(stage, elapsed.as_secs_f64());
            let budget = scale.budgets[stage];
            assert!(
                elapsed <= budget,
                "{stage} took {elapsed:?}, budget {budget:?}"
            );
        };

    // Create: a generic project with one two-hour Original asset and N beats.
    let started = Instant::now();
    let asset = AssetId::new("original").unwrap();
    let initial = ProjectDocument::new(
        ProjectId::new("long-project").unwrap(),
        RevisionId::new("stress-0").unwrap(),
        PresentationBasis {
            width: 640,
            height: 360,
            frame_rate: FrameRate::new(24, 1).unwrap(),
            color_policy: ColorPolicy::SdrRec709,
        },
        NodeId::new("root").unwrap(),
    )
    .unwrap();
    let mut session = Session {
        store: ProjectStore::create(&path, &initial).unwrap(),
        serial: 0,
    };
    session
        .commit(Command::AddAsset {
            id: asset.clone(),
            asset: AssetRecord {
                label: "Two-hour Original".into(),
                content_hash: "b".repeat(64),
                video: Some(span(0, TWO_HOURS_TICKS)),
                audio: Some(span(0, TWO_HOURS_TICKS)),
                still_image: false,
                frame_count: None,
                source_qualification: None,
            },
        })
        .unwrap();
    let frames_per_beat = TWO_HOURS_TICKS / TICKS_PER_FRAME / scale.beats as i64;
    let mut nodes = BTreeMap::new();
    let mut children = Vec::with_capacity(scale.beats);
    for index in 0..scale.beats {
        let id = NodeId::new(format!("beat-{index:06}")).unwrap();
        let node = if index % 50 == 49 {
            hold(12)
        } else {
            source_beat(
                &asset,
                index as i64 * frames_per_beat * TICKS_PER_FRAME,
                frames_per_beat,
            )
        };
        nodes.insert(id.clone(), node);
        children.push(id);
    }
    let group = NodeId::new("stress-group").unwrap();
    nodes.insert(group.clone(), BeatNode::sequence("Stress", children));
    session
        .commit(Command::Insert {
            parent: initial.root().clone(),
            index: 0,
            subtree: Subtree {
                root: group.clone(),
                nodes,
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        })
        .unwrap();
    session.commit(Command::Ungroup { node: group }).unwrap();
    check("create", started.elapsed(), &mut timings);

    // Edit: hundreds of revisions through the ordinary command path.
    let mut random = Lcg(0x5eed);
    let mut commits = Vec::new();
    let mut refused: BTreeMap<String, usize> = BTreeMap::new();
    let mut fresh = 0_usize;
    let mut undone = false;
    for edit in 0..scale.edits {
        let head = session.store.snapshot().unwrap();
        let children = root_children(&head);
        let target = children[random.below(children.len())].clone();
        fresh += 1;
        let command = match random.below(8) {
            0 => Command::Rename {
                node: target,
                label: format!("Edited {edit}"),
            },
            1 => Command::WrapRepeat {
                node: target,
                id: NodeId::new(format!("repeat-{fresh}")).unwrap(),
                plays: 3,
                gap: None,
                anchor_policy: Default::default(),
            },
            2 => Command::Split {
                node: target,
                at: FrameDuration::new(frames_per_beat / 2).unwrap(),
                identities: deadpan_core::SplitIdentities {
                    nodes: (0..4)
                        .map(|part| NodeId::new(format!("split-{fresh}-{part}")).unwrap())
                        .collect(),
                },
            },
            3 => Command::DeleteRipple {
                node: target,
                timing: deadpan_core::AudioTimingId {
                    // A deletion's clock is allocated by its own new revision.
                    allocation: RevisionId::new(format!("stress-{}", session.serial + 1)).unwrap(),
                    ordinal: 0,
                },
            },
            4 => {
                // A Hold anywhere at the root; skip the beat when none is near.
                let hold = children
                    .iter()
                    .skip(random.below(children.len()))
                    .find(|id| matches!(head.nodes()[*id].kind, NodeKind::Hold { .. }))
                    .cloned()
                    .unwrap_or(target);
                Command::SetHoldDuration {
                    node: hold,
                    duration: FrameDuration::new(1 + random.below(48) as i64).unwrap(),
                }
            }
            5 | 6 if !commits.is_empty() => {
                let head = session.store.head_revision().unwrap();
                fresh += 1;
                let next = RevisionId::new(format!("history-{fresh}")).unwrap();
                let started = Instant::now();
                // Redo only directly after an Undo; otherwise undo.
                let outcome = if undone {
                    session.store.redo(&head, next)
                } else {
                    session.store.undo(&head, next)
                };
                undone = !undone && outcome.is_ok();
                match outcome {
                    Ok(_) => commits.push(started.elapsed().as_secs_f64()),
                    Err(error) => {
                        *refused
                            .entry(format!("history: {error}").chars().take(80).collect())
                            .or_default() += 1
                    }
                }
                continue;
            }
            _ => Command::Rename {
                node: target,
                label: "x".repeat(1 + random.below(200)),
            },
        };
        undone = false;
        match session.commit(command) {
            Ok(elapsed) => commits.push(elapsed.as_secs_f64()),
            Err(error) => *refused.entry(error.chars().take(80).collect()).or_default() += 1,
        }
    }
    assert!(
        commits.len() * 10 >= scale.edits * 7,
        "too many edits refused: {} of {} committed; {refused:?}",
        commits.len(),
        scale.edits
    );
    let commit_p95 = percentile(&mut commits.clone(), 0.95);
    check(
        "commit_p95",
        Duration::from_secs_f64(commit_p95),
        &mut timings,
    );
    let head = session.store.snapshot().unwrap();
    let revisions = session.serial;
    drop(session);

    // Reopen and validate.
    let started = Instant::now();
    let store = ProjectStore::open(&path, AccessMode::ReadWrite).unwrap();
    check("reopen", started.elapsed(), &mut timings);
    assert_eq!(store.snapshot().unwrap(), head);
    let started = Instant::now();
    store.validate().unwrap();
    check("validate_receipt", started.elapsed(), &mut timings);
    let started = Instant::now();
    store.validate_full().unwrap();
    check("validate_full", started.elapsed(), &mut timings);
    let started = Instant::now();
    for serial in [1, revisions / 4, revisions / 2, revisions * 3 / 4] {
        let id = RevisionId::new(format!("stress-{}", serial.max(1))).unwrap();
        let document = store.snapshot_at(&id).unwrap();
        document.validate().unwrap();
    }
    check("history_reads", started.elapsed(), &mut timings);

    // Render plan: compile, random picture lookups, audio range queries.
    let started = Instant::now();
    let plan = RenderPlan::compile(&head).unwrap();
    check("plan_compile", started.elapsed(), &mut timings);
    let frames = plan.duration().frames();
    assert!(
        frames > 150_000,
        "the project should span about two hours: {frames} frames"
    );
    let started = Instant::now();
    for _ in 0..2_000 {
        plan.picture(ProjectFrame(random.below(frames as usize) as i64))
            .unwrap();
    }
    check("picture_lookups", started.elapsed(), &mut timings);
    let started = Instant::now();
    let total = plan.audio_duration().unwrap().0;
    let limits = AudioQueryLimits {
        maximum_spans: 4096,
        maximum_work: 65_536,
    };
    let mut spans = 0;
    for _ in 0..40 {
        // Ten-second ranges, the export-preview granularity.
        let start = random.below((total - 480_000) as usize) as i64;
        let query = plan
            .audio(AudioSample(start)..AudioSample(start + 480_000), limits)
            .unwrap();
        spans += query.spans.len();
    }
    check("audio_queries", started.elapsed(), &mut timings);
    let peak = deadpan_chaos::allocation_peak_since(baseline);
    if deadpan_chaos::allocator_installed() {
        assert!(
            peak <= scale.max_alloc,
            "peak allocation {peak} bytes > {}",
            scale.max_alloc
        );
    }
    let report = json!({
        "beats": scale.beats,
        "nodes": head.nodes().len(),
        "frames": frames,
        "revisions": revisions,
        "commits": commits.len(),
        "refused": refused,
        "commit_p50_s": percentile(&mut commits.clone(), 0.5),
        "commit_p95_s": commit_p95,
        "commit_max_s": percentile(&mut commits, 1.0),
        "audio_spans": spans,
        "peak_alloc_bytes": peak,
        "package_bytes": std::fs::metadata(path.join("project.sqlite")).map(|meta| meta.len()).unwrap_or(0),
        "stages_s": timings,
    });
    eprintln!("deadpan-stress {report}");
    if let Some(directory) = std::env::var_os("DEADPAN_CHAOS_OUT") {
        let _ = std::fs::create_dir_all(&directory);
        let _ = std::fs::write(
            std::path::Path::new(&directory).join("stress.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        );
    }
    let _: Value = report;
}
