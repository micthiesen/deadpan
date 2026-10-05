//! Edit command latency through the authoritative SQLite store, as the native
//! project service commits: one revision/history transaction per edit, then a
//! workspace refresh (head snapshot and plan compile). Commits to PACKAGE.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use deadpan_core::{
    AudioTimingId, BeatNode, ColorPolicy, Command, CommandRequest, FrameDuration, FrameRate,
    HoldAudio, HoldRecipe, HoldVideo, NodeId, NodeKind, PresentationBasis, ProjectDocument,
    ProjectFrame, ProjectId, RevisionId, SplitIdentities, Subtree, ValidatedDocument,
    WrapAnchorPolicy,
};
use deadpan_plan::RenderPlan;
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::{Value, json};

use crate::{Lcg, Options, Result, ms, summary};

const PAUSE_FRAMES: i64 = 12;

struct Ids(u64);
impl Ids {
    fn node(&mut self) -> Result<NodeId> {
        self.0 += 1;
        Ok(NodeId::new(format!(
            "perf-{}-{}",
            std::process::id(),
            self.0
        ))?)
    }
    fn revision(&mut self) -> Result<RevisionId> {
        self.0 += 1;
        Ok(RevisionId::new(format!(
            "perf-rev-{}-{}",
            std::process::id(),
            self.0
        ))?)
    }
}

#[derive(Default)]
struct Samples {
    prepare: Vec<f64>,
    commit: Vec<f64>,
    refresh: Vec<f64>,
    total: Vec<f64>,
    refusals: Vec<String>,
}

impl Samples {
    fn report(&self) -> Value {
        json!({
            "command_preparation_ms": summary(&self.prepare),
            "store_commit_ms": summary(&self.commit),
            "workspace_refresh_ms": summary(&self.refresh),
            "total_ms": summary(&self.total),
            "committed": self.total.len(),
            "refused": self.refusals.len(),
            // Cost drifts as history accumulates; report it in order.
            "total_ms_in_commit_order_buckets": self.total.chunks(self.total.len().div_ceil(10).max(1))
                .map(summary).collect::<Vec<_>>(),
            "raw_total_ms": self.total.iter().map(|v| crate::round(*v)).collect::<Vec<_>>(),
            "refusals": self.refusals,
        })
    }
}

pub fn run(options: &Options) -> Result<Value> {
    let package = options.package()?;
    let cycles = options.number("cycles", 30)?;
    let mut random = Lcg::new(options.number("seed", 7)?);
    let opened = Instant::now();
    let mut store = ProjectStore::open(package, AccessMode::ReadWrite)?;
    store.set_generation_context_resolver(Arc::new(
        deadpan_cli::generation_context::BoundaryContextResolver::default(),
    ));
    let open_ms = ms(opened);
    let mut ids = Ids(0);
    let mut document = store.snapshot_validated()?;
    let mut plan = document.scope(|| RenderPlan::compile(&document))?;
    let initial = json!({
        "nodes": document.nodes().len(),
        "root_children": root_children(&document).len(),
        "frames": plan.duration().frames(),
    });
    let mut split = Samples::default();
    let mut pause = Samples::default();
    let mut wrap = Samples::default();
    let mut undo = Samples::default();
    let mut json_bytes = Vec::new();
    for _ in 0..cycles {
        // Split a physical root beat at an interior frame.
        let started = Instant::now();
        let command = document.scope(|| {
            (0..16).find_map(|_| {
                let at = random.below(u64::try_from(plan.duration().frames()).ok()?);
                split_command(&document, i64::try_from(at).ok()?, &mut ids)
                    .ok()
                    .flatten()
            })
        });
        if let Some(command) = command {
            let command = (ids.revision()?, command);
            measure(
                &mut store,
                &mut document,
                &mut plan,
                command,
                started,
                &mut split,
            )?;
        } else {
            split.refusals.push("no splittable root beat found".into());
        }

        // Insert a 12-frame freeze at a root boundary (native `,h`).
        let started = Instant::now();
        let at = ProjectFrame(i64::try_from(
            random.below(u64::try_from(plan.duration().frames())?),
        )?);
        match document.scope(|| pause_command(&store, &document, &plan, at, &mut ids)) {
            Ok(command) => {
                measure(
                    &mut store,
                    &mut document,
                    &mut plan,
                    command,
                    started,
                    &mut pause,
                )?;
            }
            Err(error) => pause.refusals.push(error.to_string()),
        }

        // Wrap a root beat in a three-play Repeat, then undo that wrap.
        let started = Instant::now();
        let children = root_children(&document);
        let target = (0..16).find_map(|_| {
            let child = &children[usize::try_from(random.below(children.len() as u64)).ok()?];
            (!matches!(document.nodes()[child].kind, NodeKind::Repeat { .. }))
                .then(|| child.clone())
        });
        let Some(target) = target else {
            wrap.refusals.push("no non-Repeat root beat".into());
            continue;
        };
        let command = (
            ids.revision()?,
            Command::WrapRepeat {
                node: target,
                id: ids.node()?,
                plays: 3,
                gap: None,
                anchor_policy: WrapAnchorPolicy::First,
            },
        );
        if measure(
            &mut store,
            &mut document,
            &mut plan,
            command,
            started,
            &mut wrap,
        )? {
            let started = Instant::now();
            let expected = document.revision_id().clone();
            let next = ids.revision()?;
            let committed = Instant::now();
            store.undo(&expected, next)?;
            undo.commit.push(ms(committed));
            let refreshed = Instant::now();
            refresh(&store, &mut document, &mut plan)?;
            undo.refresh.push(ms(refreshed));
            undo.total.push(ms(started));
        }
        json_bytes.push(document.to_json()?.len() as f64);
    }
    let database = std::fs::metadata(package.join("project.sqlite"))?.len()
        + std::fs::metadata(package.join("project.sqlite-wal")).map_or(0, |m| m.len());
    // Reopen the edited history as the app and a read-only inspector would:
    // validation, then the head document.
    drop(store);
    let reopened = Instant::now();
    let writer = ProjectStore::open(package, AccessMode::ReadWrite)?;
    let reopen_validation = writer.open_validation();
    let reopen_ms = ms(reopened);
    let head = Instant::now();
    let reopened_document = writer.snapshot_shared()?;
    let reopen_head_ms = ms(head);
    if *reopened_document != *document {
        return Err("reopened head differs from the last committed revision".into());
    }
    drop(writer);
    let read = Instant::now();
    let reader = ProjectStore::open(package, AccessMode::ReadOnly)?;
    reader.snapshot_shared()?;
    let read_only_ms = ms(read);
    drop(reader);
    Ok(json!({
        "package": package,
        "cycles": cycles,
        "open_writable_ms": crate::round(open_ms),
        "reopen_writable_ms": crate::round(reopen_ms),
        "reopen_head_ms": crate::round(reopen_head_ms),
        "reopen_validation": reopen_validation,
        "reopen_read_only_with_head_ms": crate::round(read_only_ms),
        "initial": initial,
        "final": {
            "nodes": document.nodes().len(),
            "root_children": root_children(&document).len(),
            "frames": plan.duration().frames(),
            "document_json_bytes": document.to_json()?.len(),
            "database_bytes": database,
        },
        "document_json_bytes": summary(&json_bytes),
        "split": split.report(),
        "insert_pause": pause.report(),
        "repeat_wrap": wrap.report(),
        "undo": undo.report(),
    }))
}

/// Commit, then refresh as the native service does. Refusals are recorded
/// rather than aborting the run; they are part of the evidence.
fn measure(
    store: &mut ProjectStore,
    document: &mut ValidatedDocument,
    plan: &mut RenderPlan,
    command: (RevisionId, Command),
    started: Instant,
    samples: &mut Samples,
) -> Result<bool> {
    let (new_revision, command) = command;
    let request = CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision,
        command,
    };
    samples.prepare.push(ms(started));
    let committed = Instant::now();
    if let Err(error) = store.commit(&request) {
        samples.refusals.push(error.to_string());
        return Ok(false);
    }
    samples.commit.push(ms(committed));
    let refreshed = Instant::now();
    refresh(store, document, plan)?;
    samples.refresh.push(ms(refreshed));
    samples.total.push(ms(started));
    Ok(true)
}

/// Refresh as the native service does: the validated head, its plan compiled
/// in the retained validation scope, and the workspace's owned copy.
fn refresh(
    store: &ProjectStore,
    document: &mut ValidatedDocument,
    plan: &mut RenderPlan,
) -> Result<()> {
    *document = store.snapshot_validated()?;
    *plan = document.scope(|| RenderPlan::compile(document))?;
    drop(ProjectDocument::clone(document));
    Ok(())
}

fn root_children(document: &ProjectDocument) -> Vec<NodeId> {
    match &document.nodes()[document.root()].kind {
        NodeKind::Sequence { children } => children.clone(),
        _ => Vec::new(),
    }
}

fn split_command(document: &ProjectDocument, at: i64, ids: &mut Ids) -> Result<Option<Command>> {
    let durations = document.durations()?;
    let mut start = 0;
    for child in root_children(document) {
        let length = durations[&child].frames();
        if (start..start + length).contains(&at) {
            if at == start || matches!(document.nodes()[&child].kind, NodeKind::Repeat { .. }) {
                return Ok(None);
            }
            let mut pending = vec![child.clone()];
            let mut count = 3_usize;
            while let Some(id) = pending.pop() {
                count += 1;
                pending.extend(document.children(&id).cloned());
            }
            return Ok(Some(Command::Split {
                node: child,
                at: FrameDuration::new(at - start)?,
                identities: SplitIdentities {
                    nodes: (0..count).map(|_| ids.node()).collect::<Result<_>>()?,
                },
            }));
        }
        start += length;
    }
    Ok(None)
}

fn pause_command(
    store: &ProjectStore,
    document: &ProjectDocument,
    plan: &RenderPlan,
    at: ProjectFrame,
    ids: &mut Ids,
) -> Result<(RevisionId, Command)> {
    let provider = deadpan_cli::pause::pause_provider(document, plan, at, &mut |asset| {
        store
            .source_video_index(document.revision_id(), asset)
            .map(Arc::new)
            .map_err(|error| error.to_string())
    })?;
    let target = document.insert_time_target(at)?;
    let identities = match target.split {
        Some(split) => (0..split.required_ids)
            .map(|_| ids.node())
            .collect::<Result<_>>()?,
        None => Vec::new(),
    };
    // InsertTime allocates its retained audio clock under the new revision.
    let revision = ids.revision()?;
    let timing = revision.clone();
    let command = Command::InsertTime {
        at,
        hold: HoldRecipe {
            picture_context: provider.picture_context,
            duration: FrameDuration::new(PAUSE_FRAMES)?,
            video: provider.video,
            audio: HoldAudio::Silence,
        },
        id: ids.node()?,
        identities: SplitIdentities { nodes: identities },
        timing: AudioTimingId {
            allocation: timing,
            ordinal: 0,
        },
    };
    Ok((revision, command))
}

/// The `large-project` replay's fixture shape at a chosen size: root
/// Background/Silence Holds in a real SQLite package. No source media.
pub fn make_large(options: &Options) -> Result<Value> {
    let path = options.package()?;
    let beats = usize::try_from(options.number("beats", 10_000)?)?;
    let started = Instant::now();
    let root = NodeId::new("large-root")?;
    let group = NodeId::new("fixture-group")?;
    let initial = ProjectDocument::new(
        ProjectId::new(format!("perf-large-{beats}"))?,
        RevisionId::new("large-initial")?,
        PresentationBasis {
            width: 1920,
            height: 1080,
            frame_rate: FrameRate::new(24, 1)?,
            color_policy: ColorPolicy::SdrRec709,
        },
        root.clone(),
    )?;
    let mut nodes = BTreeMap::new();
    let mut children = Vec::with_capacity(beats);
    for index in 0..beats {
        let id = NodeId::new(format!("hold-{index:06}"))?;
        nodes.insert(
            id.clone(),
            BeatNode::hold(
                format!("Scale fixture {:06}", index + 1),
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
    nodes.insert(group.clone(), BeatNode::sequence("Fixture group", children));
    let mut store = ProjectStore::create(path, &initial)?;
    store.commit(&CommandRequest {
        project_id: initial.project_id().clone(),
        expected_revision: initial.revision_id().clone(),
        new_revision: RevisionId::new("large-inserted")?,
        command: Command::Insert {
            parent: root,
            index: 0,
            subtree: Subtree {
                root: group.clone(),
                nodes,
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        },
    })?;
    let inserted = store.snapshot()?;
    store.commit(&CommandRequest {
        project_id: inserted.project_id().clone(),
        expected_revision: inserted.revision_id().clone(),
        new_revision: RevisionId::new("large-ungrouped")?,
        command: Command::Ungroup { node: group },
    })?;
    Ok(json!({"package": path, "beats": beats, "creation_ms": crate::round(ms(started))}))
}
