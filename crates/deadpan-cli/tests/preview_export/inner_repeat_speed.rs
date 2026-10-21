//! Real Original pictures and PCM through individually retained Repeat carriers.
use super::*;
use deadpan_core::{BeatNode, CommandRequest, IterationOrder, PauseInsertionTarget, Subtree};
use std::collections::{BTreeMap, BTreeSet};

fn descendants(document: &ProjectDocument, root: &NodeId) -> BTreeSet<NodeId> {
    let mut found = BTreeSet::new();
    let mut pending = vec![root.clone()];
    while let Some(owner) = pending.pop() {
        pending.extend(document.children(&owner).cloned());
        found.insert(owner);
    }
    found
}

fn pictures() -> Vec<(u64, Expected)> {
    // The pause changes group input from15 to33/2 frames. Stage18 centers
    // are11*(2n+1)/24. Input is Original14 on[0,1/2), black[1/2,2),
    // Original14 on[2,5/2), then default Original29..31, room1, owned gap2,
    // owned Original28..31, room1, default gap1, default Original29..31,
    // room1, and Original31. These are literal center-sampled ordinals.
    let stage = [
        Some(14),
        None,
        Some(14),
        Some(29),
        Some(30),
        None,
        None,
        None,
        Some(28),
        Some(29),
        Some(30),
        None,
        None,
        None,
        Some(29),
        Some(30),
        None,
        Some(31),
    ];
    let mut result = vec![original(12), original(13)];
    result.extend(stage.map(|ordinal| ordinal.map_or(Expected::Background, original)));
    result.extend([original(36), original(37)]);
    assert_eq!(result.len(), 22);
    result
        .into_iter()
        .enumerate()
        .map(|(frame, picture)| (frame as u64, picture))
        .collect()
}

fn source_peak(project: &Project) -> Result<f64> {
    // First default Source occupies absolute samples[7571,11066). Keep this
    // window strictly inside it, so RoomTone gaps and outside audio cannot
    // make an omitted repeated Original pass. Original29..31 contains click.
    let mut at = 7800;
    let mut peak = 0.0_f64;
    while at < 10800 {
        let end = (at + 256).min(10800);
        let result = success(&[
            "inspect-audio",
            project.path(),
            "--samples",
            &at.to_string(),
            &end.to_string(),
            "--authored-bus",
        ])?;
        let samples: Vec<[f64; 2]> = serde_json::from_value(result["audio"]["samples"].clone())?;
        assert_eq!(samples.len(), (end - at) as usize);
        for sample in samples.into_iter().flatten() {
            assert!(sample.is_finite());
            peak = peak.max(sample.abs());
        }
        at = end;
    }
    assert!(peak > 0.001, "repeated Source-only PCM peak={peak}");
    Ok(peak)
}

pub fn shrink(dir: &Path) -> Result<Fixture> {
    let mut project = Project::create(dir, "inner-repeat-speed-update")?;
    project.shorten()?;
    for at in [2, 3, 16, 19, 20, 24, 26] {
        project.split_root(at)?;
    }
    let prepared_owned = project.document()?;
    let owned_children = root_children(&prepared_owned)?;
    assert_eq!(owned_children.len(), 8);
    let mut owned_nodes = BTreeMap::new();
    let owned_source = super::fixed_retime_duration::clone_fragment(
        &mut project,
        &prepared_owned,
        &owned_children[3],
        &mut owned_nodes,
    )?;
    // Save Original28..31 intact before the next public split prepares the
    // shorter shared default Source29..31. Both are complete admitted trees.
    let prepared = project.split_root(17)?;
    let children = root_children(&prepared)?;
    assert_eq!(children.len(), 9);
    let mut nodes = BTreeMap::new();
    let mut clone = |project: &mut Project, index| {
        super::fixed_retime_duration::clone_fragment(
            project,
            &prepared,
            &children[index],
            &mut nodes,
        )
    };
    let prefix = clone(&mut project, 0)?;
    let lead = clone(&mut project, 1)?;
    let default_source = clone(&mut project, 4)?;
    let tail = clone(&mut project, 5)?;
    let suffix = clone(&mut project, 7)?;
    let tone_source = prepared
        .nodes()
        .values()
        .find_map(|node| match &node.kind {
            NodeKind::Source { source } => source.audio.clone(),
            _ => None,
        })
        .ok_or("qualified Original has no audio")?;
    let room = |frames| -> Result<HoldRecipe> {
        Ok(HoldRecipe {
            picture_context: None,
            duration: FrameDuration::new(frames)?,
            video: HoldVideo::Background,
            audio: HoldAudio::RoomTone {
                source: tone_source.clone(),
            },
        })
    };
    let ids = project.fresh(8)?;
    let [
        root,
        group,
        repeat,
        default_body,
        default_room,
        owned_body,
        owned_room,
        owned_gap,
    ] = <[NodeId; 8]>::try_from(ids).map_err(|_| "eight identities")?;
    nodes.extend([
        (
            root.clone(),
            BeatNode::sequence(
                "Qualified Repeat speed",
                vec![prefix.clone(), group.clone(), suffix.clone()],
            ),
        ),
        (
            group.clone(),
            BeatNode::sequence("Group", vec![lead, repeat.clone(), tail]),
        ),
        (
            default_body.clone(),
            BeatNode::sequence("Shared default", vec![default_source, default_room.clone()]),
        ),
        (
            default_room.clone(),
            BeatNode::hold("Default room tone", room(1)?),
        ),
    ]);
    owned_nodes.extend([
        (
            owned_body.clone(),
            BeatNode::sequence("Owned middle", vec![owned_source, owned_room.clone()]),
        ),
        (owned_room, BeatNode::hold("Owned room tone", room(1)?)),
    ]);
    project.delete_range(0, prepared.duration()?.frames())?;
    let inserted = project.apply(|_, document, revision| {
        let mut repeated = BeatNode::sequence("Three structural plays", vec![]);
        repeated.kind = NodeKind::Repeat {
            child: default_body.clone(),
            iterations: IterationOrder::new(revision.clone(), 3)?,
            gap: Some(room(1)?),
            escalation: None,
        };
        nodes.insert(repeat.clone(), repeated);
        Ok(Command::Insert {
            parent: document.root().clone(),
            index: 0,
            subtree: Subtree {
                root,
                nodes,
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        })
    })?;
    assert!(inserted.audio_bindings().is_empty());
    let NodeKind::Repeat { iterations, .. } = &inserted.nodes()[&repeat].kind else {
        return Err("structural Repeat".into());
    };
    let first = iterations.at(0).ok_or("first play")?;
    let middle = iterations.at(1).ok_or("middle play")?;
    project.apply(|_, _, _| {
        Ok(Command::SetPlayOverride {
            node: repeat.clone(),
            iteration: middle,
            subtree: Subtree {
                root: owned_body,
                nodes: owned_nodes,
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        })
    })?;
    let grouped = project.apply(|_, _, _| {
        Ok(Command::SetGapOverride {
            node: repeat.clone(),
            iteration: first,
            subtree: Subtree {
                root: owned_gap.clone(),
                nodes: BTreeMap::from([(owned_gap, BeatNode::hold("Owned first gap", room(2)?))]),
                overrides: Default::default(),
                gap_overrides: Default::default(),
            },
        })
    })?;
    assert_eq!(grouped.durations()?[&group].frames(), 15);
    let rate = project.fresh(1)?.remove(0);
    project.apply(|_, _, _| {
        Ok(Command::WrapRetime {
            node: group,
            id: rate.clone(),
            duration: FrameDuration::new(30)?,
            pitch: PitchPolicy::FollowSpeed,
        })
    })?;
    let pause = project.fresh(1)?.remove(0);
    let before = project.apply(|project, document, revision| {
        let at = ProjectFrame(3);
        let PauseInsertionTarget::Retimed(target) = document.pause_insertion_target(at)? else {
            return Err("pause must enter live stage".into());
        };
        assert_eq!(target.retime, rate);
        Ok(Command::InsertTime {
            at,
            hold: silent(3, HoldVideo::Background)?,
            id: pause,
            identities: SplitIdentities {
                nodes: project.fresh(target.required_ids)?,
            },
            timing: AudioTimingId {
                allocation: revision.clone(),
                ordinal: 0,
            },
        })
    })?;
    assert_eq!(before.duration()?.frames(), 37);
    assert_eq!(
        before.derived_clocks()[&rate].extent,
        ExactRatio::integer(33)
    );
    assert_eq!(
        before.derived_clocks()[&rate].child_end,
        ExactRatio::new(33, 2)?
    );
    let mut outside = descendants(&before, &prefix);
    outside.extend(descendants(&before, &suffix));
    let mut expected_after = None;
    let after = project.apply(|_, document, revision| {
        let command = Command::SetRetime {
            node: rate.clone(),
            duration: FrameDuration::new(18)?,
            pitch: PitchPolicy::FollowSpeed,
        };
        let transaction = deadpan_core::apply(
            document,
            &CommandRequest {
                project_id: document.project_id().clone(),
                expected_revision: document.revision_id().clone(),
                new_revision: revision.clone(),
                command: command.clone(),
            },
        )?;
        let changed = transaction.forward.apply(document)?;
        assert_eq!(transaction.inverse.apply(&changed)?, *document);
        assert_eq!(ProjectDocument::from_json(&changed.to_json()?)?, changed);
        expected_after = Some(changed);
        Ok(command)
    })?;
    // Project::apply closes the CLI process and reopens SQLite read-only.
    assert_eq!(Some(&after), expected_after.as_ref());
    assert_eq!(after.duration()?.frames(), 22);
    assert_eq!(
        after.derived_clocks()[&rate].extent,
        ExactRatio::integer(18)
    );
    assert_eq!(
        after.derived_clocks()[&rate].child_end,
        ExactRatio::new(33, 2)?
    );
    assert_eq!(after.nodes()[&repeat], before.nodes()[&repeat]);
    assert_eq!(after.overrides(), before.overrides());
    assert_eq!(after.gap_overrides(), before.gap_overrides());
    assert!(!after.audio_bindings().bindings().contains_key(&repeat));
    let mut retained = 0;
    for (owner, binding) in before.audio_bindings().bindings() {
        if !outside.contains(owner) {
            assert_eq!(
                serde_json::to_vec(&after.audio_bindings().bindings()[owner])?,
                serde_json::to_vec(binding)?
            );
            retained += 1;
        }
    }
    assert!(retained >= 5, "default/owned Source and RoomTone bindings");
    assert_eq!(
        serde_json::to_vec(after.audio_bindings().gap_bindings())?,
        serde_json::to_vec(before.audio_bindings().gap_bindings())?
    );
    for (owner, clock) in before.derived_clocks() {
        if owner != &rate {
            assert_eq!(after.derived_clocks()[owner], *clock);
        }
    }
    let peak = source_peak(&project)?;
    project.finish(
        vec!["Hold (insert time)", "Retime", "Slow delivery", "Repeat operator"],
        pictures(),
        vec![format!(
            "Qualified Original fragments: prefix12..14; group lead14..15; three structural Repeat plays with shared default29..31+RoomTone1, owned middle28..31+RoomTone1, owned first RoomTone gap2 and default middle RoomTone gap1; tail31..32; suffix36..38. Public group15->30, pause(root3,length3), speed18 leaves exact input33/2 and produces22 pictures. All picture expectations are literal center samples, independently enumerated. {retained} interior node bindings and default-gap binding bytes remain unchanged; no whole-Repeat binding is added. Core inverse/JSON and durable CLI reopen match. Source-only sample7800..10800 peak={peak}; unrelated gaps/outside cannot satisfy it. Export verifies every emitted picture/provenance and full H264/AAC audio at offset0. Native44.1k per-carrier11/6 PCM/mask oracle is a separate backend regression."
        )],
    )
}
