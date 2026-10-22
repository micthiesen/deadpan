//! Selected disjoint Repeat-body Hold duration beneath a fixed Retime.
use super::*;
use deadpan_core::{
    BeatNode, InstancePath, IterationOrder, OccurrenceIdentities, PauseLength, RepeatEditBranch,
    RepeatEditStep, RepeatInstance, ScopedNodeTarget, SemanticAllocation,
    SemanticAllocationRequest, SemanticContext, SemanticInstruction, SemanticProgram,
    SemanticRegisterBank, SemanticSelectedHoldDurationContext, Subtree, plan_semantic,
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

#[derive(Clone, Copy, Debug)]
pub enum Selection {
    InnerDefault,
    InnerPlay,
    OuterDefault,
    OuterPlay,
}
impl Selection {
    pub fn name(self) -> &'static str {
        match self {
            Self::InnerDefault => "inner-default",
            Self::InnerPlay => "inner-play",
            Self::OuterDefault => "outer-default",
            Self::OuterPlay => "outer-play",
        }
    }
    fn affects(self, outer: i64, inner: i64) -> bool {
        match self {
            Self::InnerDefault => inner < 2,
            Self::InnerPlay => outer == 1 && inner < 2,
            Self::OuterDefault => outer < 2,
            Self::OuterPlay => outer < 2 && inner == 1,
        }
    }
}

#[derive(Clone)]
struct PreparedTargets {
    authority: SemanticSelectedHoldDurationContext,
    cursor: ProjectFrame,
    document: ProjectDocument,
    pools: Vec<usize>,
    isolation_steps: usize,
}

fn edit_selected(
    project: &mut Project,
    prior: &PreparedTargets,
    parent: &NodeId,
    length: i64,
) -> Result<PreparedTargets> {
    let mut prepared = None;
    let after = project.apply(|project, document, revision| {
        let context = SemanticContext {
            parent: parent.clone(),
            cursor: prior.cursor,
            selected_child: Some(prior.authority.root.clone()),
            selected_hold_duration: Some(Box::new(prior.authority.clone())),
            original: None,
            scoped: None,
            hold_duration: None,
            shared_hold_duration: None,
            mixed_hold_duration: None,
            visual_selection: None,
        };
        let program = SemanticProgram::new(vec![SemanticInstruction::SetHoldDuration {
            length: PauseLength::Frames {
                frames: std::num::NonZeroU32::new(u32::try_from(length)?).ok_or("positive duration")?,
            },
        }])?;
        let bank = BTreeMap::new();
        let mut pools = Vec::new();
        let plan = plan_semantic(
            document, &context, &program,
            SemanticRegisterBank { entries: &bank, version: 0 },
            revision.clone(),
            |request| {
                let SemanticAllocationRequest::Isolation { step_index, nodes, marks } = request else {
                    panic!("selected duration must allocate exact isolation pools: {request:?}");
                };
                assert_eq!(marks, 0);
                pools.push(nodes);
                Ok(SemanticAllocation::Isolation {
                    new_revision: RevisionId::new(format!("{revision}-selected-{step_index}")).unwrap(),
                    identities: OccurrenceIdentities {
                        nodes: project.fresh(nodes).map_err(|error| deadpan_core::EditError {
                            code: deadpan_core::EditErrorCode::IdentityConflict,
                            message: error.to_string(), current_revision: None,
                        })?,
                        marks: vec![],
                    },
                })
            },
            |_, _| panic!("duration needs no Original resolver"),
        )?;
        let request = plan.request.as_ref().ok_or("selected duration made no edit")?;
        assert!(matches!(&request.command, Command::Compound { transaction } if transaction.steps().len() == 2));
        let proof = deadpan_core::derive_scoped_isolation(document, request, &plan.document)?;
        let authority = plan.context.selected_hold_duration.as_ref().ok_or("mapped selection")?;
        let old_targets = std::iter::once(&prior.authority.target).chain(&prior.authority.also);
        let new_targets = std::iter::once(&authority.target).chain(&authority.also);
        for (old, new) in old_targets.zip(new_targets) {
            assert_eq!(proof.map_forward(old)?, *new);
            assert_eq!(proof.map_backward(new)?, *old);
            assert_eq!(
                old.repeats.iter().map(|step| &step.branch).collect::<Vec<_>>(),
                new.repeats.iter().map(|step| &step.branch).collect::<Vec<_>>(),
                "literal Default/Play authority is retained",
            );
            let NodeKind::Hold { recipe } = &plan.document.nodes()[&new.node].kind else {
                return Err("mapped selected Hold".into());
            };
            assert_eq!(recipe.duration.frames(), length);
        }
        let presentation = prior.authority.presentation.as_ref().ok_or("visible primary")?;
        assert_eq!(
            proof.map_instance_forward(presentation)?,
            authority.presentation.clone().ok_or("mapped primary")?,
        );
        let visible = plan.document.selected_hold_duration_visibility(parent, authority)?.ok_or("visible primary")?;
        assert_eq!(plan.context.cursor, visible.frames.start());
        assert_eq!(plan.document.duration()?, document.duration()?);
        let transaction = deadpan_core::apply(document, request)?;
        assert_eq!(transaction.forward.apply(document)?, plan.document);
        assert_eq!(transaction.inverse.apply(&plan.document)?, *document);
        assert_eq!(ProjectDocument::from_json(&plan.document.to_json()?)?, plan.document);
        prepared = Some(PreparedTargets {
            authority: authority.as_ref().clone(),
            cursor: plan.context.cursor,
            document: plan.document.clone(),
            pools,
            isolation_steps: proof.record().steps().len(),
        });
        Ok(request.command.clone())
    })?;
    let prepared = prepared.ok_or("duration preparation did not run")?;
    assert_eq!(
        after, prepared.document,
        "CLI commit equals semantic selected result"
    );
    Ok(prepared)
}

fn noop(project: &Project, prior: &PreparedTargets, parent: &NodeId, length: i64) -> Result {
    let stored = project.document()?;
    assert_eq!(stored, prior.document);
    let history =
        ProjectStore::open(&project.package, AccessMode::ReadOnly)?.history_availability()?;
    let identities = (project.step, project.ids);
    let context = SemanticContext {
        parent: parent.clone(),
        cursor: prior.cursor,
        selected_child: Some(prior.authority.root.clone()),
        selected_hold_duration: Some(Box::new(prior.authority.clone())),
        original: None,
        scoped: None,
        hold_duration: None,
        shared_hold_duration: None,
        mixed_hold_duration: None,
        visual_selection: None,
    };
    let program = SemanticProgram::new(vec![SemanticInstruction::SetHoldDuration {
        length: PauseLength::Frames {
            frames: std::num::NonZeroU32::new(u32::try_from(length)?).ok_or("positive duration")?,
        },
    }])?;
    let bank = BTreeMap::new();
    let error = plan_semantic(
        &prior.document,
        &context,
        &program,
        SemanticRegisterBank {
            entries: &bank,
            version: 0,
        },
        RevisionId::new("selected-fixed-noop")?,
        |allocation| panic!("equal selected duration allocated: {allocation:?}"),
        |_, _| panic!("duration needs no Original resolver"),
    )
    .unwrap_err();
    assert_eq!(error.code, deadpan_core::EditErrorCode::InvalidCommand);
    assert!(
        error.message.contains("already have that duration"),
        "{error:?}"
    );
    assert_eq!(
        project.document()?,
        stored,
        "no-op refusal preserves the committed revision"
    );
    assert_eq!(
        ProjectStore::open(&project.package, AccessMode::ReadOnly)?.history_availability()?,
        history,
        "no-op refusal preserves durable Undo/Redo availability",
    );
    assert_eq!(
        (project.step, project.ids),
        identities,
        "no-op refusal allocates nothing"
    );
    Ok(())
}

pub fn build(dir: &Path, selection: Selection, grow: bool, affine: bool) -> Result<Fixture> {
    let name = match (affine, grow) {
        (false, true) => "selected-fixed-unit-grow",
        (false, false) => "selected-fixed-unit-shrink",
        (true, true) => "selected-fixed-affine-grow",
        (true, false) => "selected-fixed-affine-shrink",
    };
    let output = if affine { 6 } else { 8 };
    // A one-frame affine Hold lacks a sampled picture; core covers its refusal.
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
            "Selected fixed-window fixture",
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

    let stable = |repeat: &NodeId, ordinal: u32| -> Result<RepeatInstance> {
        let NodeKind::Repeat { iterations, .. } = &before.nodes()[repeat].kind else {
            return Err("committed Repeat".into());
        };
        Ok(RepeatInstance {
            node: repeat.clone(),
            iteration: iterations.at(ordinal).ok_or("stable play")?,
        })
    };
    let selected = |ordinal: u32| -> Result<ScopedNodeTarget> {
        let (outer_branch, inner_branch) = match selection {
            Selection::InnerDefault => (
                RepeatEditBranch::Default,
                RepeatEditBranch::Play {
                    iteration: stable(&repeat_id, ordinal)?.iteration,
                },
            ),
            Selection::InnerPlay => (
                RepeatEditBranch::Play {
                    iteration: stable(&outer, 1)?.iteration,
                },
                RepeatEditBranch::Play {
                    iteration: stable(&repeat_id, ordinal)?.iteration,
                },
            ),
            Selection::OuterDefault => (
                RepeatEditBranch::Play {
                    iteration: stable(&outer, ordinal)?.iteration,
                },
                RepeatEditBranch::Default,
            ),
            Selection::OuterPlay => (
                RepeatEditBranch::Play {
                    iteration: stable(&outer, ordinal)?.iteration,
                },
                RepeatEditBranch::Play {
                    iteration: stable(&repeat_id, 1)?.iteration,
                },
            ),
        };
        Ok(ScopedNodeTarget {
            node: held.clone(),
            repeats: vec![
                RepeatEditStep {
                    repeat: outer.clone(),
                    branch: outer_branch,
                },
                RepeatEditStep {
                    repeat: repeat_id.clone(),
                    branch: inner_branch,
                },
            ],
        })
    };
    let (primary_outer, primary_inner) = (1_u32, 1_u32);
    let mut target = PreparedTargets {
        authority: SemanticSelectedHoldDurationContext {
            root: outer.clone(),
            target: selected(1)?,
            also: vec![selected(0)?],
            presentation: Some(InstancePath {
                node: held.clone(),
                repeats: vec![
                    stable(&outer, primary_outer)?,
                    stable(&repeat_id, primary_inner)?,
                ],
            }),
        },
        cursor: ProjectFrame(
            16 + (i64::from(primary_outer) * 3 + i64::from(primary_inner)) * output + 1,
        ),
        document: before.clone(),
        pools: vec![],
        isolation_steps: 0,
    };
    // Both selected addresses deliberately start with the same shared NodeId.
    assert_eq!(target.authority.target.node, target.authority.also[0].node);
    noop(&project, &target, &root, 3)?;
    let count = |node: &NodeId| -> usize {
        let mut stack = vec![node.clone()];
        let mut total = 0;
        while let Some(id) = stack.pop() {
            total += 1;
            stack.extend(before.children(&id).cloned());
        }
        total
    };
    let stage_pool = count(&stage);
    let inner_pool = count(&repeat_id);
    let expected_pools = match selection {
        Selection::InnerDefault => vec![stage_pool, stage_pool],
        Selection::InnerPlay => vec![inner_pool + stage_pool, stage_pool],
        Selection::OuterDefault => vec![inner_pool, inner_pool],
        Selection::OuterPlay => vec![inner_pool + stage_pool, inner_pool + stage_pool],
    };
    let expected_steps = match selection {
        Selection::InnerDefault | Selection::OuterDefault => 2,
        Selection::InnerPlay => 3,
        Selection::OuterPlay => 4,
    };
    if !grow {
        target = edit_selected(&mut project, &target, &root, 4)?;
        assert_eq!(target.pools, expected_pools);
        assert_eq!(target.isolation_steps, expected_steps);
        noop(&project, &target, &root, 4)?;
    }
    let before_final = project.document()?;
    assert_eq!(before_final, target.document);
    let old_length = if grow { 3 } else { 4 };
    let mut old_pcm = Vec::new();
    if !affine {
        for family in 0..3 {
            for play in 0..3 {
                let changed = selection.affects(family, play);
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
    let after_target = edit_selected(&mut project, &target, &root, final_length)?;
    let after = project.document()?;
    assert_eq!(after, after_target.document);
    assert_eq!(after.duration()?.frames(), total);
    noop(&project, &after_target, &root, final_length)?;
    if grow {
        assert_eq!(after_target.pools, expected_pools);
        assert_eq!(after_target.isolation_steps, expected_steps);
        assert_eq!(
            after.nodes().len(),
            before_final.nodes().len() + expected_pools.iter().sum::<usize>()
        );
        for (id, node) in before.nodes() {
            assert_eq!(
                after.nodes().get(id),
                Some(node),
                "shared definition {id:?}"
            );
        }
    } else {
        assert_eq!(after_target.pools, vec![0, 0]);
        assert_eq!(after_target.isolation_steps, 0);
        assert_eq!(after.nodes().len(), before_final.nodes().len());
        assert_eq!(after_target.authority, target.authority);
    }
    for (id, node) in before_final.nodes() {
        if matches!(node.kind, NodeKind::Retime { .. }) {
            assert_eq!(
                after.nodes().get(id),
                Some(node),
                "fixed crop/pitch/output {id:?}"
            );
        }
    }
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
                &format!("{name} {} use at {start}", selection.name()),
            );
        }
    }
    let mut pictures: Vec<_> = (12..28).map(original).collect();
    for family in 0..3 {
        let mut lengths = [3; 3];
        for (play, length) in lengths.iter_mut().enumerate() {
            if selection.affects(family, i64::try_from(play)?) {
                *length = final_length;
            }
        }
        pictures.extend(
            shared_fixed_retime_duration::layout(output, lengths)
                .into_iter()
                .skip(16)
                .take(usize::try_from(3 * output)?),
        );
    }
    pictures.extend((29..32).map(original));
    assert_eq!(pictures.len(), usize::try_from(total)?);
    let mut fixture = project.finish(
        vec!["Hold (insert time)", "Retime", "Repeat operator"],
        pictures.into_iter().enumerate().map(|(frame, picture)| Ok((u64::try_from(frame)?, picture))).collect::<Result<_>>()?,
        vec![format!(
            "Real cfr-bframes.mp4 at 30000/1001; selected {} over nested 3x3 fixed output{output}/input[0,8). Actual selected semantic authority maps both distinct shared-NodeId targets, presentation and inverse. Two selected members Hold {old_length}->{final_length}; root remains {total}f. {} Unit cases compare nonzero 256-sample suffix probes at exact old/new origins for all nine uses plus exterior within 1e-6. Affine complete emitted-file verification makes no rounded phase-shift claim. Equal-duration no-op validation explicitly refuses InvalidCommand before allocation and preserves the committed revision and Undo/Redo availability.",
            selection.name(), if grow { "Fresh exact-pool isolation." } else { "Already owned 3->4 then shrink; final pools [0,0]." },
        )],
    )?;
    fixture.audio = (0..9)
        .map(|play| Ok((boundary(16 + play * output + 1)? + 512, false)))
        .collect::<Result<_>>()?;
    Ok(fixture)
}
