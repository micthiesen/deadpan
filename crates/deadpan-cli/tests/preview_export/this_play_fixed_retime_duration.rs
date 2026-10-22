//! One selected Repeat play, isolated beneath a fixed unclocked Edit Retime.
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
        prepared = Some(PreparedTarget {
            instance: resolved.presentation.clone(),
            document: resolved.document.clone(),
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
        (false, true) => "this-play-fixed-unit-grow",
        (false, false) => "this-play-fixed-unit-shrink",
        (true, true) => "this-play-fixed-affine-grow",
        (true, false) => "this-play-fixed-affine-shrink",
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

    let ids = project.fresh(6)?;
    let [root, repeat_id, stage, body, held, pad] =
        <[NodeId; 6]>::try_from(ids).map_err(|_| "six fresh identities")?;
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
        child: stage,
        iterations: order.clone(),
        gap: None,
        escalation: None,
    };
    nodes.insert(repeat_id.clone(), repeat);
    nodes.insert(
        root.clone(),
        BeatNode::sequence(
            "One-play fixed-window fixture",
            vec![lead, repeat_id.clone(), tail.clone()],
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
    let total = 19 + 3 * output;
    assert_eq!(before.duration()?.frames(), total);
    assert!(before.audio_bindings().is_empty());
    let selected_ordinal = 1;
    let iteration = |document: &ProjectDocument| -> Result<RepeatInstance> {
        let NodeKind::Repeat { iterations, .. } = &document.nodes()[&repeat_id].kind else {
            return Err("Repeat".into());
        };
        Ok(RepeatInstance {
            node: repeat_id.clone(),
            iteration: iterations.at(selected_ordinal).ok_or("selected play")?,
        })
    };
    let mut target = PreparedTarget {
        instance: InstancePath {
            node: held.clone(),
            repeats: vec![iteration(&before)?],
        },
        document: before.clone(),
    };
    if already_owned {
        // Create the selected override once, then prove the final request
        // updates that exact owned Hold without another isolation.
        target = edit_play(&mut project, target.instance, 4)?;
        assert_eq!(target.document.duration()?.frames(), total);
        assert_eq!(
            target.document.overrides()[&repeat_id].len(),
            1,
            "selected play acquired one owned fixed-stage branch"
        );
    }

    let before_final = project.document()?;
    assert_eq!(before_final, target.document);
    let hold_before = match &before_final.nodes()[&target.instance.node].kind {
        NodeKind::Hold { recipe } => recipe.duration.frames(),
        _ => return Err("mapped target is not a Hold".into()),
    };
    assert_eq!(hold_before, if already_owned { 4 } else { 3 });
    let selected_play_start = 16 + i64::from(selected_ordinal) * output;
    let old_selected_pcm = if affine {
        None
    } else {
        Some(pcm(&project, selected_play_start + 1 + hold_before)?)
    };
    let unchanged_pcm: Vec<_> = if affine {
        Vec::new()
    } else {
        [20_i64, 36_i64, 40_i64]
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
    assert_eq!(after_target.instance.repeats, target.instance.repeats);
    if already_owned {
        assert_eq!(
            after_target.instance, target.instance,
            "owned mapped identity is stable"
        );
    } else {
        assert_ne!(
            after_target.instance.node, target.instance.node,
            "fresh isolation maps the target"
        );
    }
    let NodeKind::Repeat {
        iterations, child, ..
    } = &after.nodes()[&repeat_id].kind
    else {
        return Err("Repeat".into());
    };
    let NodeKind::Repeat {
        child: before_child,
        ..
    } = &before_final.nodes()[&repeat_id].kind
    else {
        return Err("before Repeat".into());
    };
    assert_eq!(
        child, before_child,
        "literal Repeat default remains unchanged"
    );
    let selected_id = iterations
        .at(selected_ordinal)
        .ok_or("selected iteration")?;
    let selected_override = after.overrides()[&repeat_id]
        .get(&selected_id)
        .ok_or("selected play has an owned stage")?;
    if already_owned {
        assert_eq!(
            Some(selected_override),
            before_final
                .overrides()
                .get(&repeat_id)
                .and_then(|entries| entries.get(&selected_id)),
            "an already-owned stage is updated without another clone"
        );
    } else {
        assert_ne!(
            selected_override, child,
            "fresh isolation owns the selected branch"
        );
    }
    assert_eq!(after.overrides()[&repeat_id].len(), 1);
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
        for (index, offset) in [20_i64, 36_i64, 40_i64].into_iter().enumerate() {
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

    let mut lengths = [3; 3];
    lengths[usize::try_from(selected_ordinal)?] = final_length;
    let pictures = shared_fixed_retime_duration::layout(output, lengths);
    assert_eq!(pictures.len(), usize::try_from(total)?);
    let mut fixture = project.finish(
        vec!["Hold (insert time)", "Retime", "Repeat operator"],
        pictures
            .into_iter()
            .enumerate()
            .map(|(frame, picture)| (u64::try_from(frame).unwrap(), picture))
            .collect(),
        vec![format!(
            "Qualified cfr-bframes.mp4 at 30000/1001. Three direct Repeat plays each show a fixed unclocked Edit Retime output{output} selecting child [0,8). Explicit selected play {selected_ordinal} changes Hold {hold_before}->{final_length}; the other two remain 3f, fixed Repeat length and Original29..32 exterior tail remain. {} Unit-rate case compares selected suffix at its old and new exact origins and checks unaffected play/exterior PCM within 1e-6. Affine case is checked by the complete emitted-file verifier without a rounded sample-phase claim.",
            if already_owned { "The selected branch was first isolated, then updated in place." } else { "The selected default branch is freshly isolated." }
        )],
    )?;
    fixture.audio = (0..3)
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
