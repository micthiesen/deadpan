//! One explicit middle/middle occurrence beneath nested Repeat bodies and a fixed Retime.
use super::*;
use deadpan_core::{
    BeatNode, CommandRequest, InstancePath, IterationOrder, MarkId, OccurrenceEdit,
    OccurrenceIdentities, RepeatInstance, Subtree,
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
    instance: InstancePath,
    document: ProjectDocument,
    isolation_steps: usize,
    allocated_nodes: usize,
}

fn edit_play(
    project: &mut Project,
    instance: InstancePath,
    duration: i64,
) -> Result<PreparedTarget> {
    let mut prepared = None;
    let after = project.apply(|project, document, revision| {
        let edit = OccurrenceEdit::SetHoldDuration {
            duration: FrameDuration::new(duration)?,
        };
        let required = document
            .occurrence_hold_duration_requirements(&instance, FrameDuration::new(duration)?)?;
        assert_eq!(required.marks, 0);
        let command = Command::EditOccurrence {
            instance: instance.clone(),
            edit,
            identities: OccurrenceIdentities {
                nodes: project.fresh(required.nodes)?,
                marks: (0..required.marks)
                    .map(|index| MarkId::new(format!("{revision}-mark-{index}")))
                    .collect::<std::result::Result<_, _>>()?,
            },
        };
        let request = CommandRequest {
            project_id: document.project_id().clone(),
            expected_revision: document.revision_id().clone(),
            new_revision: revision.clone(),
            command: command.clone(),
        };
        let resolved = deadpan_core::prepare_occurrence_hold_duration(document, &request)?;
        let proof = resolved
            .isolation()
            .validate(document, &request, &resolved.document)?;
        assert_eq!(
            proof.map_instance_forward(&instance)?,
            resolved.presentation
        );
        prepared = Some(PreparedTarget {
            instance: resolved.presentation.clone(),
            document: resolved.document.clone(),
            isolation_steps: resolved.isolation().steps().len(),
            allocated_nodes: required.nodes,
        });
        Ok(command)
    })?;
    let prepared = prepared.ok_or("duration preparation did not run")?;
    assert_eq!(
        after, prepared.document,
        "CLI commit equals prepared mapped result"
    );
    Ok(prepared)
}

fn build(dir: &Path, grow: bool, affine: bool, already_owned: bool) -> Result<Fixture> {
    let name = match (affine, grow) {
        (false, true) => "nested-fixed-unit-grow",
        (false, false) => "nested-fixed-unit-shrink",
        (true, true) => "nested-fixed-affine-grow",
        (true, false) => "nested-fixed-affine-shrink",
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
            "One-play fixed-window fixture",
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
    let selected_ordinal = 1;
    let stable = |document: &ProjectDocument, node: &NodeId| -> Result<RepeatInstance> {
        let NodeKind::Repeat { iterations, .. } = &document.nodes()[node].kind else {
            return Err("Repeat".into());
        };
        Ok(RepeatInstance {
            node: node.clone(),
            iteration: iterations.at(selected_ordinal).ok_or("selected play")?,
        })
    };
    // Read stable identities after Insert. Insert may re-key authored iteration seeds.
    let outer_play = stable(&before, &outer)?;
    let inner_play = stable(&before, &repeat_id)?;
    let mut target = PreparedTarget {
        instance: InstancePath {
            node: held.clone(),
            repeats: vec![outer_play.clone(), inner_play.clone()],
        },
        document: before.clone(),
        isolation_steps: 0,
        allocated_nodes: 0,
    };
    if already_owned {
        target = edit_play(&mut project, target.instance, 4)?;
        assert_eq!(target.document.duration()?.frames(), total);
        assert_eq!(target.isolation_steps, 2);
        assert!(target.allocated_nodes > 0);
    }

    let before_final = project.document()?;
    assert_eq!(before_final, target.document);
    let hold_before = match &before_final.nodes()[&target.instance.node].kind {
        NodeKind::Hold { recipe } => recipe.duration.frames(),
        _ => return Err("mapped target is not a Hold".into()),
    };
    assert_eq!(hold_before, if already_owned { 4 } else { 3 });
    let selected_play_start = 16 + 4 * output;
    let old_selected_pcm = if affine {
        None
    } else {
        Some(pcm(&project, selected_play_start + 1 + hold_before)?)
    };
    let unchanged_pcm: Vec<_> = if affine {
        Vec::new()
    } else {
        [20_i64, 44_i64, 60_i64, 76_i64, 88_i64]
            .into_iter()
            .map(|offset| pcm(&project, offset))
            .collect::<Result<Vec<_>>>()?
    };

    let after_target = edit_play(&mut project, target.instance.clone(), final_length)?;
    let after = project.document()?;
    assert_eq!(after, after_target.document);
    assert_eq!(
        after.duration()?.frames(),
        total,
        "fixed Repeat output stays fixed"
    );
    assert_eq!(after_target.instance.repeats[0], outer_play);
    assert_eq!(
        after_target.instance.repeats[1].iteration,
        inner_play.iteration
    );
    if already_owned {
        assert_eq!(
            after_target.instance, target.instance,
            "owned mapped identities stay stable"
        );
        assert_eq!(after_target.allocated_nodes, 0);
        assert_eq!(after_target.isolation_steps, 0);
        assert_eq!(after.nodes().len(), before_final.nodes().len());
    } else {
        assert_ne!(after_target.instance.node, held);
        assert_ne!(after_target.instance.repeats[1].node, repeat_id);
        assert!(after_target.allocated_nodes > 0);
        assert_eq!(after_target.isolation_steps, 2);
    }
    assert_eq!(
        after.nodes()[&repeat_id],
        before.nodes()[&repeat_id],
        "default inner Repeat"
    );
    assert_eq!(
        after.nodes()[&stage],
        before.nodes()[&stage],
        "default fixed Retime"
    );
    assert_eq!(after.overrides()[&outer].len(), 1);
    let mapped_inner = &after_target.instance.repeats[1].node;
    assert_eq!(after.overrides()[mapped_inner].len(), 1);
    assert_eq!(
        after
            .overrides()
            .values()
            .map(|entries| entries.len())
            .sum::<usize>(),
        2
    );
    let NodeKind::Repeat {
        iterations: mapped_order,
        ..
    } = &after.nodes()[mapped_inner].kind
    else {
        return Err("mapped inner Repeat".into());
    };
    let NodeKind::Repeat {
        iterations: original_order,
        ..
    } = &before.nodes()[&repeat_id].kind
    else {
        return Err("original inner Repeat".into());
    };
    assert_eq!(
        mapped_order, original_order,
        "committed inner stable Play identities"
    );
    let NodeKind::Hold { recipe } = &after.nodes()[&after_target.instance.node].kind else {
        return Err("mapped target remains a Hold".into());
    };
    assert_eq!(recipe.duration.frames(), final_length);
    let NodeKind::Hold {
        recipe: default_recipe,
    } = &after.nodes()[&held].kind
    else {
        return Err("literal default Hold remains".into());
    };
    assert_eq!(
        default_recipe.duration.frames(),
        3,
        "nonselected plays keep default"
    );
    assert_eq!(
        after.nodes()[&tail],
        before_final.nodes()[&tail],
        "exterior tail is unchanged"
    );
    if !affine {
        for (index, offset) in [20_i64, 44_i64, 60_i64, 76_i64, 88_i64]
            .into_iter()
            .enumerate()
        {
            same_pcm(
                &unchanged_pcm[index],
                &pcm(&project, offset)?,
                &format!("{name} unaffected play/exterior PCM at {offset}"),
            );
        }
        let new_selected = pcm(&project, selected_play_start + 1 + final_length)?;
        same_pcm(
            old_selected_pcm.as_deref().ok_or("old selected PCM")?,
            &new_selected,
            &format!("{name} selected suffix old/new origins"),
        );
    }

    // Reuse only the independently sampled Repeat section. The lead and exterior
    // tail occur once, outside both Repeat operators.
    let mut pictures: Vec<_> = (12..28).map(original).collect();
    for outer_ordinal in 0..3 {
        let mut lengths = [3; 3];
        if outer_ordinal == 1 {
            lengths[1] = final_length;
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
        pictures
            .into_iter()
            .enumerate()
            .map(|(frame, picture)| (u64::try_from(frame).unwrap(), picture))
            .collect(),
        vec![format!(
            "Qualified cfr-bframes.mp4 at 30000/1001. Three outer plays each contain three inner plays of a fixed unclocked Edit Retime output{output} selecting child [0,8). Explicit middle/middle play changes Hold {hold_before}->{final_length}; the other eight remain 3f, fixed nested Repeat length and Original29..32 exterior tail remain. {} Unit-rate case compares selected suffix at its old and new exact origins and checks unaffected play/exterior PCM within 1e-6. Affine case is checked by the complete emitted-file verifier without a rounded sample-phase claim.",
            if already_owned { "The selected branch was first isolated, then updated in place." } else { "The selected default branch is freshly isolated." }
        )],
    )?;
    fixture.audio = (0..9)
        .map(|play| Ok((boundary(16 + play * output + 1)? + 512, false)))
        .collect::<Result<_>>()?;
    Ok(fixture)
}

pub fn unit_grow(dir: &Path) -> Result<Fixture> {
    build(dir, true, false, false)
}

pub fn unit_shrink(dir: &Path) -> Result<Fixture> {
    build(dir, false, false, true)
}

pub fn affine_grow(dir: &Path) -> Result<Fixture> {
    build(dir, true, true, false)
}

pub fn affine_shrink(dir: &Path) -> Result<Fixture> {
    build(dir, false, true, true)
}
