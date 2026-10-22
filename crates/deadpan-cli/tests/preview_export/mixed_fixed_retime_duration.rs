//! Mixed Default/Play and Play/Default Hold duration beneath a fixed Retime.
use super::*;
use deadpan_core::{
    BeatNode, CommandRequest, InstancePath, IterationOrder, OccurrenceIdentities, RepeatEditBranch,
    RepeatEditStep, RepeatInstance, ScopedNodeTarget, Subtree,
};
use std::collections::BTreeMap;

const CLICK_IN_SOURCE: i64 = 1554;

fn boundary(frame: i64) -> Result<i64> {
    Ok(i64::try_from(
        ExactRatio::new(i128::from(frame) * 8008, 5)?.round_even()?,
    )?)
}

fn pcm(project: &Project, frame: i64) -> Result<Vec<[f64; 2]>> {
    let start = boundary(frame)? + CLICK_IN_SOURCE;
    let result = success(&[
        "inspect-audio",
        project.path(),
        "--samples",
        &start.to_string(),
        &(start + 256).to_string(),
        "--authored-bus",
    ])?;
    Ok(serde_json::from_value(result["audio"]["samples"].clone())?)
}

fn same_pcm(expected: &[[f64; 2]], actual: &[[f64; 2]], label: &str) {
    assert_eq!(expected.len(), 256, "{label}");
    assert_eq!(actual.len(), expected.len(), "{label}");
    assert!(
        expected.iter().flatten().any(|sample| sample.abs() > 0.001),
        "{label}"
    );
    for (index, (old, new)) in expected.iter().zip(actual).enumerate() {
        for channel in 0..2 {
            assert!(
                new[channel].is_finite(),
                "{label} sample {index} channel {channel}"
            );
            assert!(
                (old[channel] - new[channel]).abs() < 1e-6,
                "{label} sample {index} channel {channel}: {} != {}",
                new[channel],
                old[channel]
            );
        }
    }
}

#[derive(Clone)]
struct PreparedTarget {
    target: ScopedNodeTarget,
    instance: InstancePath,
    document: ProjectDocument,
    isolation_steps: usize,
    allocated_nodes: usize,
}

fn edit_scope(
    project: &mut Project,
    target: &PreparedTarget,
    duration: i64,
) -> Result<PreparedTarget> {
    let mut prepared = None;
    let after = project.apply(|project, document, revision| {
        let duration = FrameDuration::new(duration)?;
        let required = document.scoped_hold_duration_requirements(&target.target, duration)?;
        assert_eq!(required.marks, 0);
        let command = Command::SetScopedHoldDuration {
            target: target.target.clone(),
            duration,
            identities: OccurrenceIdentities {
                nodes: project.fresh(required.nodes)?,
                marks: vec![],
            },
        };
        let request = CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision.clone(),
            command: command.clone(),
        };
        let resolved =
            deadpan_core::prepare_scoped_hold_duration(document, &request, Some(&target.instance))?;
        let proof = resolved
            .isolation()
            .validate(document, &request, &resolved.document)?;
        assert_eq!(proof.map_forward(&target.target)?, resolved.target);
        assert_eq!(proof.map_backward(&resolved.target)?, target.target);
        assert_eq!(
            proof.map_instance_forward(&target.instance)?,
            resolved
                .presentation
                .clone()
                .ok_or("visible representative")?
        );
        prepared = Some(PreparedTarget {
            target: resolved.target.clone(),
            instance: resolved
                .presentation
                .clone()
                .ok_or("mapped representative")?,
            document: resolved.document.clone(),
            isolation_steps: resolved.isolation().steps().len(),
            allocated_nodes: required.nodes,
        });
        Ok(command)
    })?;
    let prepared = prepared.ok_or("duration preparation did not run")?;
    assert_eq!(
        after, prepared.document,
        "CLI commit equals prepared result"
    );
    Ok(prepared)
}

pub fn build(dir: &Path, outer_default: bool, grow: bool, affine: bool) -> Result<Fixture> {
    let name = match (outer_default, affine, grow) {
        (true, false, true) => "mixed-fixed-default-play-unit-grow",
        (true, false, false) => "mixed-fixed-default-play-unit-shrink",
        (true, true, true) => "mixed-fixed-default-play-affine-grow",
        (true, true, false) => "mixed-fixed-default-play-affine-shrink",
        (false, false, true) => "mixed-fixed-play-default-unit-grow",
        (false, false, false) => "mixed-fixed-play-default-unit-shrink",
        (false, true, true) => "mixed-fixed-play-default-affine-grow",
        (false, true, false) => "mixed-fixed-play-default-affine-shrink",
    };
    let output = if affine { 6 } else { 8 };
    // The affine 1f case has no sampled picture, so the positive affine shrink
    // keeps two frames; core tests cover the expected refusal for one frame.
    let final_length = if grow {
        5
    } else if affine {
        2
    } else {
        1
    };
    let mut project = Project::create(dir, name)?;
    project.shorten()?;
    project.split_root(16)?;
    project.split_root(20)?;
    insert_freeze(&mut project, 17, 3, "prepared-freeze")?;
    let prepared = project.document()?;
    let children = root_children(&prepared)?;
    assert_eq!(children.len(), 5);

    let ids = project.fresh(7)?;
    let [root, outer, repeat_id, stage, body, held, pad] =
        <[NodeId; 7]>::try_from(ids).map_err(|_| "seven fresh identities")?;
    let mut nodes = BTreeMap::new();
    let lead =
        fixed_retime_duration::clone_fragment(&mut project, &prepared, &children[0], &mut nodes)?;
    let pre =
        fixed_retime_duration::clone_fragment(&mut project, &prepared, &children[1], &mut nodes)?;
    let speech =
        fixed_retime_duration::clone_fragment(&mut project, &prepared, &children[3], &mut nodes)?;
    let tail =
        fixed_retime_duration::clone_fragment(&mut project, &prepared, &children[3], &mut nodes)?;
    let mut retime = BeatNode::sequence("Fixed selected-play window", vec![]);
    retime.kind = NodeKind::Retime {
        purpose: RetimePurpose::Edit,
        child: body.clone(),
        duration: FrameDuration::new(output)?,
        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(8))?,
        pitch: PitchPolicy::FollowSpeed,
    };
    nodes.extend([
        (stage.clone(), retime),
        (
            body.clone(),
            BeatNode::sequence(
                "Original / Hold / Source / padding",
                vec![pre, held.clone(), speech, pad.clone()],
            ),
        ),
        (held.clone(), prepared.nodes()[&children[2]].clone()),
        (
            pad,
            BeatNode::hold("Black padding", silent(4, HoldVideo::Background)?),
        ),
    ]);
    let order = IterationOrder::new(RevisionId::new(format!("{name}-plays"))?, 3)?;
    let mut repeat = BeatNode::sequence("Three fixed-window plays", vec![]);
    repeat.kind = NodeKind::Repeat {
        child: stage.clone(),
        iterations: order.clone(),
        gap: None,
        escalation: None,
    };
    nodes.insert(repeat_id.clone(), repeat);
    let mut outer_repeat = BeatNode::sequence("Three outer plays", vec![]);
    outer_repeat.kind = NodeKind::Repeat {
        child: repeat_id.clone(),
        iterations: IterationOrder::new(RevisionId::new(format!("{name}-outer-plays"))?, 3)?,
        gap: None,
        escalation: None,
    };
    nodes.insert(outer.clone(), outer_repeat);
    nodes.insert(
        root.clone(),
        BeatNode::sequence(
            "Mixed fixed-window fixture",
            vec![lead, outer.clone(), tail.clone()],
        ),
    );
    project.delete_range(0, prepared.duration()?.frames())?;
    let before = project.apply(|_, document, _| {
        Ok(Command::Insert {
            parent: document.root().clone(),
            index: 0,
            subtree: Subtree {
                root: root.clone(),
                nodes,
                overrides: BTreeMap::new(),
                gap_overrides: BTreeMap::new(),
            },
        })
    })?;
    let total = 19 + 9 * output;
    assert_eq!(before.duration()?.frames(), total);
    assert!(before.audio_bindings().is_empty());
    let stable = |node: &NodeId| -> Result<RepeatInstance> {
        let NodeKind::Repeat { iterations, .. } = &before.nodes()[node].kind else {
            return Err("committed Repeat".into());
        };
        Ok(RepeatInstance {
            node: node.clone(),
            iteration: iterations.at(1).ok_or("middle play")?,
        })
    };
    let outer_play = stable(&outer)?;
    let inner_play = stable(&repeat_id)?;
    let original_target = ScopedNodeTarget {
        node: held.clone(),
        repeats: vec![
            RepeatEditStep {
                repeat: outer.clone(),
                branch: if outer_default {
                    RepeatEditBranch::Default
                } else {
                    RepeatEditBranch::Play {
                        iteration: outer_play.iteration.clone(),
                    }
                },
            },
            RepeatEditStep {
                repeat: repeat_id.clone(),
                branch: if outer_default {
                    RepeatEditBranch::Play {
                        iteration: inner_play.iteration.clone(),
                    }
                } else {
                    RepeatEditBranch::Default
                },
            },
        ],
    };
    let mut target = PreparedTarget {
        target: original_target,
        instance: InstancePath {
            node: held.clone(),
            repeats: vec![outer_play.clone(), inner_play.clone()],
        },
        document: before.clone(),
        isolation_steps: 0,
        allocated_nodes: 0,
    };
    // Independent exact pool: the explicit Play clones its entire authored body.
    let clone_root = if outer_default { &stage } else { &repeat_id };
    let mut stack = vec![clone_root.clone()];
    let mut expected_pool = 0;
    while let Some(node) = stack.pop() {
        expected_pool += 1;
        stack.extend(before.children(&node).cloned());
    }
    if !grow {
        target = edit_scope(&mut project, &target, 4)?;
        assert_eq!(target.isolation_steps, 1);
        assert_eq!(target.allocated_nodes, expected_pool);
    }
    let before_final = project.document()?;
    assert_eq!(before_final, target.document);
    let old_length = if grow { 3 } else { 4 };
    let affected = |outer_ordinal: i64, inner_ordinal: i64| {
        if outer_default {
            inner_ordinal == 1
        } else {
            outer_ordinal == 1
        }
    };
    // Keep exact old/new origins for every affected use, and all unaffected uses.
    let mut old_pcm = Vec::new();
    if !affine {
        for family in 0..3 {
            for play in 0..3 {
                let changed = affected(family, play);
                let start = 16 + (family * 3 + play) * output;
                old_pcm.push((
                    start,
                    changed,
                    pcm(&project, start + 1 + if changed { old_length } else { 3 })?,
                ));
            }
        }
        old_pcm.push((16 + 9 * output, false, pcm(&project, 16 + 9 * output)?));
    }
    let after_target = edit_scope(&mut project, &target, final_length)?;
    let after = project.document()?;
    assert_eq!(after, after_target.document);
    assert_eq!(after.duration()?.frames(), total);
    assert_eq!(after_target.instance.repeats[0], outer_play);
    assert_eq!(
        after_target.instance.repeats[1].iteration,
        inner_play.iteration
    );
    assert_eq!(
        after_target
            .target
            .repeats
            .iter()
            .map(|s| &s.branch)
            .collect::<Vec<_>>(),
        target
            .target
            .repeats
            .iter()
            .map(|s| &s.branch)
            .collect::<Vec<_>>()
    );
    if grow {
        assert_eq!(after_target.isolation_steps, 1);
        assert_eq!(after_target.allocated_nodes, expected_pool);
        assert_eq!(
            after.nodes().len(),
            before_final.nodes().len() + expected_pool
        );
        assert_ne!(after_target.target.node, held);
    } else {
        assert_eq!(after_target.isolation_steps, 0);
        assert_eq!(after_target.allocated_nodes, 0);
        assert_eq!(after.nodes().len(), before_final.nodes().len());
        assert_eq!(after_target.target, target.target);
        assert_eq!(after_target.instance, target.instance);
    }
    assert_eq!(
        after
            .overrides()
            .values()
            .map(|entries| entries.len())
            .sum::<usize>(),
        1
    );
    if outer_default {
        assert_eq!(after_target.instance.repeats[1].node, repeat_id);
        assert_eq!(after.overrides()[&repeat_id].len(), 1);
        assert!(!after.overrides().contains_key(&outer));
    } else {
        assert_ne!(after_target.instance.repeats[1].node, repeat_id);
        assert_eq!(after.overrides()[&outer].len(), 1);
        assert!(
            !after
                .overrides()
                .contains_key(&after_target.instance.repeats[1].node)
        );
        let NodeKind::Repeat {
            iterations: mapped, ..
        } = &after.nodes()[&after_target.instance.repeats[1].node].kind
        else {
            return Err("mapped inner Repeat".into());
        };
        let NodeKind::Repeat {
            iterations: original,
            ..
        } = &before.nodes()[&repeat_id].kind
        else {
            return Err("original inner Repeat".into());
        };
        assert_eq!(mapped, original, "stable committed inner play IDs");
    }
    for (id, node) in before.nodes() {
        assert_eq!(
            after.nodes().get(id),
            Some(node),
            "shared definition {id:?}"
        );
    }
    let NodeKind::Hold { recipe } = &after.nodes()[&after_target.target.node].kind else {
        return Err("mapped Hold".into());
    };
    assert_eq!(recipe.duration.frames(), final_length);
    if !affine {
        for (start, changed, expected) in old_pcm {
            let frame = if start == 16 + 9 * output {
                start
            } else {
                start + 1 + if changed { final_length } else { 3 }
            };
            same_pcm(
                &expected,
                &pcm(&project, frame)?,
                &format!("{name} use at {start}"),
            );
        }
    }

    let mut pictures: Vec<_> = (12..28).map(original).collect();
    for family in 0..3 {
        let mut lengths = [3; 3];
        for (play, length) in lengths.iter_mut().enumerate() {
            if affected(family, i64::try_from(play)?) {
                *length = final_length;
            }
        }
        let inner = shared_fixed_retime_duration::layout(output, lengths);
        pictures.extend(
            inner
                .into_iter()
                .skip(16)
                .take(usize::try_from(3 * output)?),
        );
    }
    pictures.extend((29..32).map(original));
    assert_eq!(pictures.len(), usize::try_from(total)?);
    let mut fixture = project.finish(
        vec!["Hold (insert time)", "Retime", "Repeat operator"],
        pictures.into_iter().enumerate().map(|(frame, picture)| (u64::try_from(frame).unwrap(), picture)).collect(),
        vec![format!(
            "Real cfr-bframes.mp4 at 30000/1001; nested 3x3 fixed output{output}/input[0,8) stages. {} changes Hold {old_length}->{final_length}; fixed root{total}f and exterior Original29..32 remain. {} Unit cases compare nonzero 256-sample suffix probes at exact old/new origins for all affected uses plus every unaffected use and exterior within 1e-6. Affine complete emitted-file verification makes no rounded phase-shift claim.",
            if outer_default { "Default/Play affects middle inner play in all three outer families" } else { "Play/Default affects all three inner uses in the middle outer family" },
            if grow { "Fresh one-step exact-pool isolation." } else { "Already owned 3->4 then shrink; final allocation zero." }
        )],
    )?;
    fixture.audio = (0..9)
        .map(|play| Ok((boundary(16 + play * output + 1)? + 512, false)))
        .collect::<Result<_>>()?;
    Ok(fixture)
}
