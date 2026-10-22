//! Real Original through an All plays duration command beneath a fixed Retime.
use super::*;
use deadpan_core::{
    BeatNode, InstancePath, IterationOrder, PauseLength, PlayOverride, PlayOverrides,
    RepeatEditBranch, RepeatEditStep, RepeatInstance, ScopedNodeTarget, SemanticAllocation,
    SemanticAllocationRequest, SemanticContext, SemanticInstruction, SemanticProgram,
    SemanticRegisterBank, SemanticSharedHoldDurationContext, Subtree, plan_semantic,
};
use std::collections::BTreeMap;

fn boundary(frame: i64) -> Result<i64> {
    Ok(i64::try_from(
        ExactRatio::new(i128::from(frame) * 8008, 5)?.round_even()?,
    )?)
}

fn pcm(project: &Project, frame: i64) -> Result<Vec<[f64; 2]>> {
    let start = boundary(frame)? + 1554;
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

/// Independently sample the authored [0,8) window at each output center.
/// Child sequence is Original28 / freeze28 x length / Original29..32 / black4.
pub(super) fn layout(output: i64, lengths: [i64; 3]) -> Vec<Expected> {
    let mut pictures: Vec<_> = (12..28).map(original).collect();
    for length in lengths {
        for frame in 0..output {
            let point = (2 * frame + 1) * 4;
            pictures.push(if point < (1 + length) * output {
                original(28)
            } else if point < (4 + length) * output {
                original(u64::try_from(29 + (point - (1 + length) * output) / output).unwrap())
            } else {
                Expected::Background
            });
        }
    }
    pictures.extend((29..32).map(original));
    pictures
}

pub fn build(dir: &Path, grow: bool, affine: bool) -> Result<Fixture> {
    let name = match (affine, grow) {
        (false, true) => "shared-fixed-retime-unit-grow",
        (false, false) => "shared-fixed-retime-unit-shrink",
        (true, true) => "shared-fixed-retime-affine-grow",
        (true, false) => "shared-fixed-retime-affine-shrink",
    };
    let output = if affine { 6 } else { 8 };
    // At output6/input8, a 1f Hold occupies [.75,1.5) and contains
    // no picture center. That edit correctly refuses continuation; use 2f
    // for the positive affine shrink, with separate core negative coverage.
    let length = if grow {
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
    let [root, outer, stage, body, held, pad] =
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
    let mut retime = BeatNode::sequence("Fixed shared stage", vec![]);
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
                "Original / shared freeze / speech / pad",
                vec![pre, held.clone(), speech, pad.clone()],
            ),
        ),
        (held.clone(), prepared.nodes()[&children[2]].clone()),
        (
            pad,
            BeatNode::hold("Black padding", silent(4, HoldVideo::Background)?),
        ),
    ]);
    // Clone the complete fixed stage for an independently owned second play.
    // This raw Subtree intentionally carries no sampling bindings from preparation.
    // Lead/tail may have Partition children; collect the stage's graph explicitly.
    let mut stage_ids = Vec::new();
    let mut pending = vec![stage.clone()];
    while let Some(id) = pending.pop() {
        match &nodes[&id].kind {
            NodeKind::Sequence { children } => pending.extend(children.iter().cloned()),
            NodeKind::Repeat { child, .. } | NodeKind::Retime { child, .. } => {
                pending.push(child.clone())
            }
            NodeKind::Source { .. } | NodeKind::Hold { .. } => {}
        }
        stage_ids.push(id);
    }
    let fresh = project.fresh(stage_ids.len())?;
    let mapping: BTreeMap<_, _> = stage_ids.into_iter().zip(fresh).collect();
    let owned_stage = mapping[&stage].clone();
    let owned_held = mapping[&held].clone();
    for (old, fresh) in &mapping {
        let mut node = nodes[old].clone();
        match &mut node.kind {
            NodeKind::Sequence { children } => {
                for child in children {
                    *child = mapping[child].clone();
                }
            }
            NodeKind::Repeat { child, .. } | NodeKind::Retime { child, .. } => {
                *child = mapping[child].clone()
            }
            NodeKind::Source { .. } | NodeKind::Hold { .. } => {}
        }
        nodes.insert(fresh.clone(), node);
    }
    let order = IterationOrder::new(RevisionId::new(format!("{name}-plays"))?, 3)?;
    let mut repeat = BeatNode::sequence("All plays with owned middle play", vec![]);
    repeat.kind = NodeKind::Repeat {
        child: stage.clone(),
        iterations: order.clone(),
        gap: None,
        escalation: None,
    };
    nodes.insert(outer.clone(), repeat);
    nodes.insert(
        root.clone(),
        BeatNode::sequence(
            "Shared fixed-window fixture",
            vec![lead, outer.clone(), tail],
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
                overrides: [(
                    outer.clone(),
                    PlayOverrides::try_from(vec![PlayOverride {
                        iteration: order.at(1).unwrap(),
                        root: owned_stage.clone(),
                    }])?,
                )]
                .into(),
                gap_overrides: BTreeMap::new(),
            },
        })
    })?;
    let total = 19 + 3 * output;
    assert_eq!(before.duration()?.frames(), total);
    assert!(before.audio_bindings().is_empty());
    let old_origins = [20, 28, 36, 40];
    let old_pcm = if affine {
        Vec::new()
    } else {
        old_origins
            .into_iter()
            .map(|frame| pcm(&project, frame))
            .collect::<Result<Vec<_>>>()?
    };
    let after = project.apply(|_, document, revision| {
        let NodeKind::Repeat { iterations, .. } = &document.nodes()[&outer].kind else {
            panic!("committed Repeat");
        };
        let authority = SemanticSharedHoldDurationContext {
            root: outer.clone(),
            target: ScopedNodeTarget {
                node: held.clone(),
                repeats: vec![RepeatEditStep {
                    repeat: outer.clone(),
                    branch: RepeatEditBranch::Default,
                }],
            },
            presentation: Some(InstancePath {
                node: held.clone(),
                repeats: vec![RepeatInstance {
                    node: outer.clone(),
                    iteration: iterations.at(2).unwrap(),
                }],
            }),
        };
        let context = SemanticContext {
            original: None,
            scoped: None,
            hold_duration: None,
            shared_hold_duration: Some(authority.clone()),
            mixed_hold_duration: None,
            selected_hold_duration: None,
            parent: root.clone(),
            cursor: ProjectFrame(16 + 2 * output + 1),
            selected_child: Some(outer.clone()),
            visual_selection: None,
        };
        let program = SemanticProgram::new(vec![SemanticInstruction::SetHoldDuration {
            length: PauseLength::Frames {
                frames: std::num::NonZeroU32::new(u32::try_from(length)?).unwrap(),
            },
        }])?;
        let bank = BTreeMap::new();
        let mut allocations = 0;
        let plan = plan_semantic(
            document,
            &context,
            &program,
            SemanticRegisterBank {
                entries: &bank,
                version: 0,
            },
            revision.clone(),
            |request| {
                allocations += 1;
                match request {
                    SemanticAllocationRequest::ParameterEdit { .. } => {
                        Ok(SemanticAllocation::ParameterEdit {
                            new_revision: RevisionId::new(format!("{revision}-duration")).unwrap(),
                        })
                    }
                    other => panic!("shared fixed window must not isolate: {other:?}"),
                }
            },
            |_, _| panic!("duration does not resolve media"),
        )?;
        assert_eq!(allocations, 1);
        assert_eq!(plan.context.shared_hold_duration, Some(authority));
        assert_eq!(plan.context.cursor, context.cursor);
        assert_eq!(plan.document.duration()?.frames(), total);
        assert_eq!(plan.document.nodes().len(), document.nodes().len());
        assert_eq!(plan.document.overrides(), document.overrides());
        assert_eq!(
            plan.document.nodes()[&owned_stage],
            document.nodes()[&owned_stage]
        );
        assert_eq!(
            plan.document.nodes()[&owned_held],
            document.nodes()[&owned_held]
        );
        assert_eq!(plan.document.nodes()[&stage], document.nodes()[&stage]);
        let request = plan.request.ok_or("shared duration made no edit")?;
        let transaction = deadpan_core::apply(document, &request)?;
        assert_eq!(transaction.inverse.apply(&plan.document)?, *document);
        assert_eq!(
            ProjectDocument::from_json(&plan.document.to_json()?)?,
            plan.document
        );
        Ok(request.command)
    })?;
    assert_eq!(after.duration()?.frames(), total);
    let new_origins = [17 + length, 28, 33 + length, 40];
    if !affine {
        for (index, (expected, frame)) in old_pcm.iter().zip(new_origins).enumerate() {
            let actual = pcm(&project, frame)?;
            assert_eq!(expected.len(), 256);
            assert_eq!(actual.len(), 256);
            assert!(
                expected.iter().flatten().any(|sample| sample.abs() > 0.001),
                "{name} Source {index} old origin {} must contain the retained click",
                old_origins[index]
            );
            for (sample, (a, b)) in expected.iter().zip(&actual).enumerate() {
                for channel in 0..2 {
                    assert!(b[channel].is_finite());
                    assert!(
                        (a[channel] - b[channel]).abs() < 1e-6,
                        "{name} source {index}, frame {frame}, sample {sample}, channel {channel}"
                    );
                }
            }
        }
    }
    let pictures = layout(output, [length, 3, length]);
    assert_eq!(pictures.len(), usize::try_from(total)?);
    let mut fixture = project.finish(vec!["Hold (insert time)", "Retime", "Repeat operator"],
        pictures.into_iter().enumerate().map(|(frame, picture)| (u64::try_from(frame).unwrap(), picture)).collect(),
        vec![format!("Qualified cfr-bframes.mp4 at 30000/1001. Fresh unbound layout Original12..28 / Repeat3(fixed Edit FollowSpeed output{output}, input[0,8), child Original28 / freeze3 / Original29..32 / black4) / Original29..32. Middle stable play owns a separate stage that remains 3f. Shared All plays command changes only literal default Hold 3->{length}, with third stable presentation and no isolation. Total remains {total}. Every expected picture is derived directly from center (2f+1)*4/{output}, then literal child ranges. Unit-rate cases additionally compare four nonzero 256-sample PCM windows within 1e-6 from old origins {old_origins:?} to {new_origins:?}; affine cases use complete emitted-file audio verification and separate canonical affine phase regressions, with no claim that rounded root sample shifts preserve waveform phase.")])?;
    fixture.audio = (0..3)
        .map(|play| Ok((boundary(16 + play * output + 1)? + 512, false)))
        .collect::<Result<_>>()?;
    Ok(fixture)
}
