//! Edited playback fixtures made from a package's Original through ordinary
//! store commits. Both commit to PACKAGE, so run them on a copy.
//!
//! - `make-cuts`: splits the Original into fragments of `--every` pictures
//!   over its first `--seconds` and wraps every second fragment in a Repeat
//!   of `--plays` plays, so playback jumps back at every play restart, as a
//!   YTP edit does. Each jump needs a keyframe seek of the Original.
//! - `make-long`: splits the first `--fragments` × `--every` pictures into
//!   fragments, groups them, repeats the group `--plays` times and explodes
//!   that Repeat into independent plays: `fragments × plays` Original Source
//!   beats (10,000 by default) in one long project.

use std::time::Instant;

use deadpan_core::{
    AudioTimingId, Command, CommandRequest, NodeKind, OccurrenceIdentities, WrapAnchorPolicy,
};
use deadpan_plan::RenderPlan;
use deadpan_store::{AccessMode, ProjectStore};
use serde_json::{Value, json};

use crate::edit::{Ids, root_children, split_command};
use crate::{Options, Result, ms, round};

fn commit(store: &mut ProjectStore, ids: &mut Ids, command: Command) -> Result<()> {
    let document = store.snapshot()?;
    store.commit(&CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: ids.revision()?,
        command,
    })?;
    Ok(())
}

/// Split the root Original at `every`, `2 × every`, … below `end`.
fn split_every(store: &mut ProjectStore, ids: &mut Ids, every: i64, end: i64) -> Result<u64> {
    let mut splits = 0;
    let mut at = every;
    while at < end {
        let document = store.snapshot()?;
        if let Some(command) = split_command(&document, at, ids)? {
            commit(store, ids, command)?;
            splits += 1;
        }
        at += every;
    }
    Ok(splits)
}

fn leaves(document: &deadpan_core::ProjectDocument) -> usize {
    document
        .nodes()
        .values()
        .filter(|node| matches!(node.kind, NodeKind::Source { .. }))
        .count()
}

pub fn make_cuts(options: &Options) -> Result<Value> {
    let package = options.package()?;
    let every = i64::try_from(options.number("every", 24)?)?;
    let plays = u32::try_from(options.number("plays", 3)?)?;
    let seconds = options.number("seconds", 60)?;
    let started = Instant::now();
    let mut store = ProjectStore::open(package, AccessMode::ReadWrite)?;
    let mut ids = Ids(0);
    let document = store.snapshot()?;
    let rate = document.presentation_basis().frame_rate;
    let frames = RenderPlan::compile(&document)?.duration().frames();
    let end = frames.min(i64::try_from(
        u64::from(rate.numerator()) * seconds / u64::from(rate.denominator()),
    )?);
    let splits = split_every(&mut store, &mut ids, every, end)?;
    let fragments = root_children(&store.snapshot()?);
    let mut wraps = 0;
    for (index, node) in fragments.iter().enumerate() {
        if index % 2 == 1 && index + 1 < fragments.len() {
            let id = ids.node()?;
            commit(
                &mut store,
                &mut ids,
                Command::WrapRepeat {
                    node: node.clone(),
                    id,
                    plays,
                    gap: None,
                    anchor_policy: WrapAnchorPolicy::First,
                },
            )?;
            wraps += 1;
        }
    }
    let document = store.snapshot()?;
    Ok(json!({
        "package": package,
        "every": every,
        "plays": plays,
        "splits": splits,
        "repeats": wraps,
        "source_beats": leaves(&document),
        "frames": RenderPlan::compile(&document)?.duration().frames(),
        "creation_ms": round(ms(started)),
    }))
}

pub fn make_long(options: &Options) -> Result<Value> {
    let package = options.package()?;
    let fragments = options.number("fragments", 100)?;
    let every = i64::try_from(options.number("every", 6)?)?;
    let plays = u32::try_from(options.number("plays", 100)?)?;
    let started = Instant::now();
    let mut store = ProjectStore::open(package, AccessMode::ReadWrite)?;
    let mut ids = Ids(0);
    let span = every * i64::try_from(fragments)?;
    let splits = split_every(&mut store, &mut ids, every, span + 1)?;
    let document = store.snapshot()?;
    let root = document.root().clone();
    let group = ids.node()?;
    commit(
        &mut store,
        &mut ids,
        Command::Group {
            parent: root,
            start: 0,
            end: usize::try_from(fragments)?,
            id: group.clone(),
            label: "Fragments".into(),
        },
    )?;
    let repeat = ids.node()?;
    commit(
        &mut store,
        &mut ids,
        Command::WrapRepeat {
            node: group,
            id: repeat.clone(),
            plays,
            gap: None,
            anchor_policy: WrapAnchorPolicy::First,
        },
    )?;
    let document = store.snapshot()?;
    let requirements = document.explode_requirements(&repeat)?;
    let revision = ids.revision()?;
    let identities = OccurrenceIdentities {
        nodes: (0..requirements.nodes)
            .map(|_| ids.node())
            .collect::<Result<_>>()?,
        marks: Vec::new(),
    };
    let exploded = Instant::now();
    store.commit(&CommandRequest {
        project_id: document.project_id().clone(),
        expected_revision: document.revision_id().clone(),
        new_revision: revision.clone(),
        command: Command::Explode {
            node: repeat,
            identities,
            timing: AudioTimingId {
                allocation: revision,
                ordinal: 0,
            },
        },
    })?;
    let explode_ms = ms(exploded);
    let document = store.snapshot()?;
    Ok(json!({
        "package": package,
        "fragments": fragments,
        "every": every,
        "plays": plays,
        "splits": splits,
        "source_beats": leaves(&document),
        "nodes": document.nodes().len(),
        "frames": RenderPlan::compile(&document)?.duration().frames(),
        "explode_commit_ms": round(explode_ms),
        "creation_ms": round(ms(started)),
    }))
}
