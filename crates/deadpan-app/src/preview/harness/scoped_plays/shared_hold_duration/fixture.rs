use super::*;
use deadpan_core::{
    AudioTimingId, BeatNode, Command, CommandRequest, FrameDuration, HoldAudio, HoldVideo,
    IterationId, IterationOrder, PlayOverride, PlayOverrides, RevisionId, SourceAudio, SourceSpan,
    SourceTimestamp, Subtree,
};
use deadpan_store::{AccessMode, ProjectStore};

pub(in super::super) fn id(name: &str) -> Result<NodeId, String> {
    NodeId::new(format!("ui-shared-duration-{name}")).map_err(|e| e.to_string())
}
fn revision(name: &str) -> Result<RevisionId, String> {
    RevisionId::new(format!("ui-shared-duration-{name}")).map_err(|e| e.to_string())
}
fn commit(store: &mut ProjectStore, command: Command, name: &str) -> Result<(), String> {
    let before = store.snapshot().map_err(|e| e.to_string())?;
    store
        .commit(&CommandRequest {
            project_id: before.project_id().clone(),
            expected_revision: before.revision_id().clone(),
            new_revision: revision(name)?,
            command,
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn repeat(name: &str, child: &str, count: u32) -> Result<BeatNode, String> {
    let mut beat = BeatNode::sequence(name, vec![]);
    beat.kind = NodeKind::Repeat {
        child: id(child)?,
        iterations: IterationOrder::new(revision(name)?, count).map_err(|e| e.to_string())?,
        gap: None,
        escalation: None,
    };
    Ok(beat)
}
fn overrides(name: &str, entries: &[(u32, &str)]) -> Result<PlayOverrides, String> {
    PlayOverrides::try_from(
        entries
            .iter()
            .map(|(ordinal, root)| {
                Ok(PlayOverride {
                    iteration: IterationId {
                        allocation: revision(name)?,
                        ordinal: *ordinal,
                    },
                    root: id(root)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
    )
    .map_err(|e| e.to_string())
}
pub(in super::super) fn create(
    source: &Path,
    path: &Path,
    dormant: bool,
    freeze: HoldVideo,
) -> Result<(), String> {
    deadpan_store::portable::copy_portable(
        source,
        path,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .map_err(|e| e.to_string())?;
    let mut store = ProjectStore::open(path, AccessMode::ReadWrite).map_err(|e| e.to_string())?;
    let before = store.snapshot().map_err(|e| e.to_string())?;
    let original = before
        .nodes()
        .values()
        .find_map(|node| match &node.kind {
            NodeKind::Source { source } => source.audio.clone(),
            _ => None,
        })
        .ok_or("Shared-duration replay requires real Original audio")?;
    let start = original.span.start();
    if start.time_base != deadpan_core::SourceTimeBase::new(1, 48000).map_err(|e| e.to_string())?
        || original.span.end().ticks - start.ticks < 4800
        || before.assets()[&original.asset]
            .source_qualification
            .is_none()
    {
        return Err("Shared-duration replay requires a qualified 48 kHz Original".into());
    }
    let audio = SourceAudio {
        asset: original.asset,
        span: SourceSpan::new(
            start,
            SourceTimestamp {
                ticks: start
                    .ticks
                    .checked_add(4800)
                    .ok_or("Room-tone range overflow")?,
                time_base: start.time_base,
            },
        )
        .map_err(|e| e.to_string())?,
    };
    let hold = |label: &str, frames: i64, video: HoldVideo| -> Result<BeatNode, String> {
        Ok(BeatNode::hold(
            label,
            HoldRecipe {
                duration: FrameDuration::new(frames).map_err(|e| e.to_string())?,
                video,
                picture_context: None,
                audio: HoldAudio::RoomTone {
                    source: audio.clone(),
                },
            },
        ))
    };
    let NodeKind::Sequence { children } = &before.nodes()[before.root()].kind else {
        return Err("Qualified Original project has no ordinary root".into());
    };
    commit(
        &mut store,
        Command::DeleteChildren {
            parent: before.root().clone(),
            first: children.first().ok_or("Empty original edit")?.clone(),
            last: children.last().ok_or("Empty original edit")?.clone(),
            timing: AudioTimingId {
                allocation: revision("clear")?,
                ordinal: 0,
            },
        },
        "clear",
    )?;
    let mut nodes = BTreeMap::from([
        (id("outer")?, repeat("outer", "inner", 3)?),
        (id("inner")?, repeat("inner", "body", 2)?),
        (
            id("body")?,
            BeatNode::sequence(
                "Shared body",
                vec![id("before")?, id("selected")?, id("after")?],
            ),
        ),
        (
            id("before")?,
            hold("Before shared pause", 2, HoldVideo::Background)?,
        ),
        (id("selected")?, hold("Shared pause", 2, freeze.clone())?),
        (
            id("after")?,
            hold("After shared pause", 2, HoldVideo::Background)?,
        ),
        (
            id("inner-owned")?,
            BeatNode::sequence(
                "Owned inner play",
                vec![id("inner-before")?, id("inner-pause")?, id("inner-after")?],
            ),
        ),
        (
            id("inner-before")?,
            hold("Inner before", 2, HoldVideo::Background)?,
        ),
        (
            id("inner-pause")?,
            hold("Owned inner pause", 4, freeze.clone())?,
        ),
        (
            id("inner-after")?,
            hold("Inner after", 2, HoldVideo::Background)?,
        ),
    ]);
    let names = ["outer-zero", "outer-one", "outer-two"];
    let mut owned = vec![];
    for (ordinal, name) in names.iter().enumerate() {
        if dormant || ordinal == 2 {
            let first = format!("{name}-before");
            let pause = format!("{name}-pause");
            let last = format!("{name}-after");
            nodes.insert(
                id(name)?,
                BeatNode::sequence(*name, vec![id(&first)?, id(&pause)?, id(&last)?]),
            );
            nodes.insert(id(&first)?, hold("Outer before", 2, HoldVideo::Background)?);
            nodes.insert(id(&pause)?, hold("Owned outer pause", 7, freeze.clone())?);
            nodes.insert(id(&last)?, hold("Outer after", 2, HoldVideo::Background)?);
            owned.push((u32::try_from(ordinal).map_err(|e| e.to_string())?, *name));
        }
    }
    let subtree = Subtree {
        root: id("outer")?,
        nodes,
        overrides: BTreeMap::from([
            (id("inner")?, overrides("inner", &[(1, "inner-owned")])?),
            (id("outer")?, overrides("outer", &owned)?),
        ]),
        gap_overrides: BTreeMap::new(),
    };
    for (index, subtree) in [
        Subtree {
            root: id("lead")?,
            nodes: BTreeMap::from([(id("lead")?, hold("Lead", 3, HoldVideo::Background)?)]),
            overrides: BTreeMap::new(),
            gap_overrides: BTreeMap::new(),
        },
        subtree,
        Subtree {
            root: id("tail")?,
            nodes: BTreeMap::from([(id("tail")?, hold("Tail", 3, HoldVideo::Background)?)]),
            overrides: BTreeMap::new(),
            gap_overrides: BTreeMap::new(),
        },
    ]
    .into_iter()
    .enumerate()
    {
        commit(
            &mut store,
            Command::Insert {
                parent: before.root().clone(),
                index,
                subtree,
            },
            &format!("insert-{index}"),
        )?;
    }
    store
        .snapshot()
        .map_err(|e| e.to_string())?
        .validate()
        .map_err(|e| e.to_string())?;
    Ok(())
}
