//! In-memory cost against authored structure size: validation, plan compile,
//! frame lookup, anchor index, edit preflight and JSON round trips. Shows which
//! operations grow with the beat count (once per revision) and which stay
//! indexed (once per cursor movement or picture request).

use std::collections::BTreeMap;
use std::time::Instant;

use deadpan_core::{
    AnchorIndex, BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate,
    HoldAudio, HoldRecipe, HoldVideo, NodeId, NodeKind, PresentationBasis, ProjectDocument,
    ProjectFrame, ProjectId, RevisionId, Subtree,
};
use deadpan_plan::RenderPlan;
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::{Value, json};

use crate::{Lcg, Options, Result, ms, round, summary};

const LOOKUPS: usize = 2_000;

pub fn run(options: &Options) -> Result<Value> {
    let sizes: Vec<usize> = options
        .text("sizes")
        .unwrap_or("100,1000,10000,50000")
        .split(',')
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()?;
    let mut holds = Vec::new();
    for &size in &sizes {
        let (document, build_ms) = holds_document(size)?;
        let mut row = measure(&document)?;
        row["beats"] = json!(size);
        row["fixture"] = json!("root Background/Silence Holds, 12 frames each");
        row["build_by_core_insert_ms"] = json!(round(build_ms));
        holds.push(row);
    }
    let mut sources = Vec::new();
    if let Some(package) = options.text("source") {
        let fragments = usize::try_from(options.number("fragments", 10_000)?)?;
        let store = ProjectStore::open(std::path::Path::new(package), AccessMode::ReadOnly)?;
        let original = store.snapshot()?;
        for size in [fragments / 10, fragments] {
            let (document, build_ms) = sources_document(&original, size)?;
            let mut row = measure(&document)?;
            row["beats"] = json!(size);
            row["fixture"] =
                json!("root copies of the package's Original Source beat (real asset references)");
            row["build_by_core_insert_ms"] = json!(round(build_ms));
            sources.push(row);
        }
    }
    Ok(json!({"holds": holds, "sources": sources}))
}

fn repeat<T>(times: usize, mut work: impl FnMut() -> Result<T>) -> Result<(Vec<f64>, T)> {
    let mut samples = Vec::with_capacity(times);
    let mut last = None;
    for _ in 0..times {
        let started = Instant::now();
        last = Some(work()?);
        samples.push(ms(started));
    }
    Ok((samples, last.ok_or("no repetitions")?))
}

fn measure(document: &ProjectDocument) -> Result<Value> {
    let (validate, _) = repeat(5, || Ok(document.durations()?))?;
    let (compile, plan) = repeat(5, || Ok(RenderPlan::compile(document)?))?;
    let (anchor, _) = repeat(5, || Ok(AnchorIndex::new(document).map(drop)?))?;
    let frames = u64::try_from(plan.duration().frames())?;
    let mut random = Lcg::new(11);
    let mut lookups = Vec::with_capacity(LOOKUPS);
    for _ in 0..LOOKUPS {
        let frame = ProjectFrame(i64::try_from(random.below(frames))?);
        let started = Instant::now();
        std::hint::black_box(plan.picture(frame)?);
        lookups.push(ms(started) * 1000.0);
    }
    let (target, _) = repeat(5, || {
        Ok(document.insert_time_target(ProjectFrame(i64::try_from(frames / 2)?))?)
    })?;
    let (serialize, json) = repeat(3, || Ok(document.to_json()?))?;
    let (parse, _) = repeat(3, || Ok(ProjectDocument::from_json(&json)?))?;
    let (clone, _) = repeat(5, || Ok(document.clone()))?;
    let inspection = plan.inspect();
    Ok(json!({
        "nodes": document.nodes().len(),
        "frames": frames,
        "validate_durations_ms": summary(&validate),
        "plan_compile_ms": summary(&compile),
        "anchor_index_build_ms": summary(&anchor),
        "picture_lookup_us": summary(&lookups),
        "insert_time_target_ms": summary(&target),
        "document_clone_ms": summary(&clone),
        "to_json_ms": summary(&serialize),
        "from_json_ms": summary(&parse),
        "document_json_bytes": json.len(),
        "plan_metadata": serde_json::to_value(&inspection.metadata)?,
    }))
}

/// Insert one Sequence of `children` under the root and ungroup it through the
/// core reducer, exactly as a committed edit would transform the document.
fn insert_children(
    base: &ProjectDocument,
    mut nodes: BTreeMap<NodeId, BeatNode>,
    children: Vec<NodeId>,
) -> Result<ProjectDocument> {
    let group = NodeId::new("perf-scale-group")?;
    nodes.insert(group.clone(), BeatNode::sequence("Scale group", children));
    let inserted = transact(
        base,
        Command::Insert {
            parent: base.root().clone(),
            index: 0,
            subtree: Subtree {
                root: group.clone(),
                nodes,
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
        "perf-scale-inserted",
    )?;
    transact(
        &inserted,
        Command::Ungroup { node: group },
        "perf-scale-ungrouped",
    )
}

fn transact(
    document: &ProjectDocument,
    command: Command,
    revision: &str,
) -> Result<ProjectDocument> {
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: RevisionId::new(revision)?,
        command,
    };
    Ok(deadpan_core::apply(document, &request)?
        .forward
        .apply(document)?)
}

fn holds_document(beats: usize) -> Result<(ProjectDocument, f64)> {
    let root = NodeId::new("scale-root")?;
    let base = ProjectDocument::new(
        ProjectId::new(format!("perf-scale-{beats}"))?,
        RevisionId::new("perf-scale-initial")?,
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(24, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        root,
    )?;
    let mut nodes = BTreeMap::new();
    let mut children = Vec::with_capacity(beats);
    for index in 0..beats {
        let id = NodeId::new(format!("hold-{index:06}"))?;
        nodes.insert(
            id.clone(),
            BeatNode::hold(
                format!("Hold {index}"),
                HoldRecipe {
                    picture_context: None,
                    duration: FrameDuration::new(12)?,
                    video: HoldVideo::Background,
                    audio: HoldAudio::Silence,
                },
            ),
        );
        children.push(id);
    }
    let started = Instant::now();
    let document = insert_children(&base, nodes, children)?;
    Ok((document, ms(started)))
}

fn sources_document(original: &ProjectDocument, beats: usize) -> Result<(ProjectDocument, f64)> {
    let NodeKind::Sequence { children } = &original.nodes()[original.root()].kind else {
        return Err("the Original root is not a Sequence".into());
    };
    let source = children
        .iter()
        .map(|id| &original.nodes()[id])
        .find(|node| matches!(node.kind, NodeKind::Source { .. }))
        .ok_or("the package has no root Source beat")?;
    let mut nodes = BTreeMap::new();
    let mut ids = Vec::with_capacity(beats);
    for index in 0..beats {
        let id = NodeId::new(format!("source-copy-{index:06}"))?;
        nodes.insert(id.clone(), source.clone());
        ids.push(id);
    }
    let started = Instant::now();
    let document = insert_children(original, nodes, ids)?;
    Ok((document, ms(started)))
}
