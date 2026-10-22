//! Real Original A/V through selected explicit gap definitions, including dormancy.
use super::*;
use deadpan_core::{
    BeatNode, InstancePath, IterationOrder, OccurrenceIdentities, PauseLength, PlayOverride,
    PlayOverrides, RepeatEditBranch, RepeatEditStep, RepeatInstance, ScopedNodeTarget,
    SemanticAllocation, SemanticAllocationRequest, SemanticContext, SemanticInstruction,
    SemanticProgram, SemanticRegisterBank, SemanticSelectedHoldDurationContext, Subtree,
    plan_semantic,
};
use std::collections::BTreeMap;

fn boundary(frame: i64) -> Result<i64> {
    Ok(i64::try_from(
        ExactRatio::new(i128::from(frame) * 8008, 5)?.round_even()?,
    )?)
}

fn pcm(project: &Project, frame: i64, original_entry: i64) -> Result<Vec<[f64; 2]>> {
    // This is inside Original frame 29, near its known click. Every comparison
    // retains its own old waveform and exact independently derived new origin.
    // Retain the old entry-relative sample offset. Re-rounding frame + 1 at
    // the new root origin can change this offset by one NTSC sample.
    let offset = boundary(original_entry + 1)? - boundary(original_entry)? + 1554;
    let start = boundary(frame)? + offset;
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

fn layout(lengths: [[i64; 2]; 2]) -> (Vec<Expected>, Vec<i64>, Vec<i64>, [[i64; 2]; 2]) {
    let mut pictures: Vec<_> = (12..28).map(original).collect();
    let mut sources = Vec::new();
    let mut quiet = Vec::new();
    let mut gaps = [[0; 2]; 2];
    for outer in 0..2 {
        for inner in 0..3 {
            sources.push(i64::try_from(pictures.len()).unwrap());
            pictures.extend((28..32).map(original));
            if inner < 2 {
                let start = i64::try_from(pictures.len()).unwrap();
                gaps[outer][inner] = start;
                quiet.push(start);
                pictures.extend(std::iter::repeat_n(
                    Expected::Background,
                    usize::try_from(lengths[outer][inner]).unwrap(),
                ));
            }
        }
        if outer == 0 {
            quiet.push(i64::try_from(pictures.len()).unwrap());
            pictures.extend([Expected::Background; 2]);
        }
    }
    pictures.extend((32..42).map(original));
    (pictures, sources, quiet, gaps)
}

fn build(dir: &Path, outer_play: bool, length: i64, dormant: bool) -> Result<Fixture> {
    let name = match (outer_play, length, dormant) {
        (true, 7, false) => "owned-gap-selected-play-grow",
        (true, 1, false) => "owned-gap-selected-play-shrink",
        (false, 7, false) => "owned-gap-selected-shared-grow",
        (false, 1, false) => "owned-gap-selected-shared-shrink",
        (true, 4, true) => "owned-gap-dormant-play",
        (false, 4, true) => "owned-gap-dormant-shared",
        _ => return Err("unexpected owned-gap fixture".into()),
    };
    let mut project = Project::create(dir, name)?;
    project.shorten()?;
    project.split_root(16)?;
    project.split_root(20)?;
    let recipes = project.document()?;
    let children = root_children(&recipes)?;
    assert_eq!(children.len(), 3);
    let old_ids: Vec<_> = recipes
        .nodes()
        .keys()
        .filter(|id| *id != recipes.root())
        .cloned()
        .collect();
    let mapping: BTreeMap<_, _> = old_ids
        .iter()
        .cloned()
        .zip(project.fresh(old_ids.len())?)
        .collect();
    let root = project.fresh(1)?.remove(0);
    let inner = project.fresh(1)?.remove(0);
    let outer = project.fresh(1)?.remove(0);
    let gap_nodes = project.fresh(3)?;
    let inner_order = IterationOrder::new(RevisionId::new(format!("{name}-inner"))?, 3)?;
    let outer_order = IterationOrder::new(RevisionId::new(format!("{name}-outer"))?, 2)?;
    let mut nodes = BTreeMap::new();
    for (old, fresh) in &mapping {
        let mut beat = recipes.nodes()[old].clone();
        match &mut beat.kind {
            NodeKind::Sequence { children } => {
                for child in children {
                    *child = mapping[child].clone();
                }
            }
            NodeKind::Repeat { child, .. } | NodeKind::Retime { child, .. } => {
                *child = mapping[child].clone();
            }
            NodeKind::Source { .. } | NodeKind::Hold { .. } => {}
        }
        nodes.insert(fresh.clone(), beat);
    }
    for (index, frames) in [3, 5, 7].into_iter().enumerate() {
        nodes.insert(
            gap_nodes[index].clone(),
            BeatNode::hold(
                format!("Explicit gap {index}"),
                silent(frames, HoldVideo::Background)?,
            ),
        );
    }
    let repeat = |label: &str, child: NodeId, order: IterationOrder, gap| {
        let mut beat = BeatNode::sequence(label, Vec::new());
        beat.kind = NodeKind::Repeat {
            child,
            iterations: order,
            gap,
            escalation: None,
        };
        beat
    };
    nodes.insert(
        inner.clone(),
        repeat(
            "Inner",
            mapping[&children[1]].clone(),
            inner_order.clone(),
            Some(silent(1, HoldVideo::Background)?),
        ),
    );
    nodes.insert(
        outer.clone(),
        repeat(
            "Outer",
            inner.clone(),
            outer_order,
            Some(silent(2, HoldVideo::Background)?),
        ),
    );
    nodes.insert(
        root.clone(),
        BeatNode::sequence(
            "Owned gap layout",
            vec![
                mapping[&children[0]].clone(),
                outer.clone(),
                mapping[&children[2]].clone(),
            ],
        ),
    );
    let owned = PlayOverrides::try_from(
        gap_nodes
            .iter()
            .enumerate()
            .map(|(index, root)| PlayOverride {
                iteration: inner_order.at(u32::try_from(index).unwrap()).unwrap(),
                root: root.clone(),
            })
            .collect::<Vec<_>>(),
    )?;
    project.delete_range(0, 30)?;
    let before = project.apply(|_, document, _| {
        Ok(Command::Insert {
            parent: document.root().clone(),
            index: 0,
            subtree: Subtree {
                root: root.clone(),
                nodes,
                overrides: Default::default(),
                gap_overrides: [(inner.clone(), owned)].into(),
            },
        })
    })?;
    assert_eq!(before.duration()?.frames(), 68);
    assert!(
        before.audio_bindings().is_empty(),
        "fresh sources must begin unbound"
    );
    let (_, old_origins, _, _) = layout([[3, 5]; 2]);
    assert_eq!(old_origins, [16, 23, 32, 38, 45, 54]);
    let old_pcm = old_origins
        .iter()
        .map(|frame| pcm(&project, *frame, *frame))
        .collect::<Result<Vec<_>>>()?;

    let mut lengths = [[3, 5]; 2];
    for (index, row) in lengths.iter_mut().enumerate() {
        if !outer_play || index == 1 {
            row[0] = length;
            if !dormant {
                row[1] = length;
            }
        }
    }
    let (pictures, new_origins, quiet, gap_starts) = layout(lengths);
    let (total, cursor) = match (outer_play, length, dormant) {
        (true, 7, false) => (74, 53),
        (true, 1, false) => (62, 47),
        (false, 7, false) => (80, 59),
        (false, 1, false) => (56, 41),
        (true, 4, true) => (69, 43),
        (false, 4, true) => (70, 43),
        _ => unreachable!(),
    };
    assert_eq!(pictures.len(), usize::try_from(total)?);
    if !dormant {
        assert_eq!(gap_starts[1][1], cursor);
    }
    let mut mapped = None;
    let after = project.apply(|project, document, revision| {
        let play = |owner: &NodeId, index| {
            let NodeKind::Repeat { iterations, .. } = &document.nodes()[owner].kind else {
                panic!("qualified fixture Repeat");
            };
            iterations.at(index).unwrap()
        };
        let outer_iteration = play(&outer, 1);
        let target = |index: usize| ScopedNodeTarget {
            node: gap_nodes[index].clone(),
            repeats: vec![
                RepeatEditStep {
                    repeat: outer.clone(),
                    branch: if outer_play {
                        RepeatEditBranch::Play {
                            iteration: outer_iteration.clone(),
                        }
                    } else {
                        RepeatEditBranch::Default
                    },
                },
                RepeatEditStep {
                    repeat: inner.clone(),
                    branch: RepeatEditBranch::Play {
                        iteration: play(&inner, u32::try_from(index).unwrap()),
                    },
                },
            ],
        };
        let primary_index = if dormant { 2 } else { 1 };
        let primary = target(primary_index);
        let presentation = (!dormant).then(|| InstancePath {
            node: primary.node.clone(),
            repeats: vec![
                RepeatInstance {
                    node: outer.clone(),
                    iteration: outer_iteration.clone(),
                },
                RepeatInstance {
                    node: inner.clone(),
                    iteration: play(&inner, 1),
                },
            ],
        });
        let context = SemanticContext {
            original: None,
            scoped: None,
            hold_duration: None,
            shared_hold_duration: None,
            mixed_hold_duration: None,
            selected_hold_duration: Some(Box::new(SemanticSelectedHoldDurationContext {
                root: outer.clone(),
                target: primary.clone(),
                presentation,
                also: vec![target(0)],
            })),
            parent: root.clone(),
            cursor: ProjectFrame(if dormant { 43 } else { 50 }),
            selected_child: Some(outer.clone()),
            visual_selection: None,
        };
        let program = SemanticProgram::new(vec![SemanticInstruction::SetHoldDuration {
            length: PauseLength::Frames {
                frames: std::num::NonZeroU32::new(u32::try_from(length)?).unwrap(),
            },
        }])?;
        let bank = BTreeMap::new();
        let mut pools = Vec::new();
        let plan = plan_semantic(
            document,
            &context,
            &program,
            SemanticRegisterBank {
                entries: &bank,
                version: 0,
            },
            revision.clone(),
            |request| match request {
                SemanticAllocationRequest::Isolation {
                    step_index,
                    nodes,
                    marks,
                } => {
                    pools.push((nodes, marks));
                    Ok(SemanticAllocation::Isolation {
                        new_revision: RevisionId::new(format!("{revision}-gap-{step_index}"))
                            .unwrap(),
                        identities: OccurrenceIdentities {
                            nodes: project.fresh(nodes).map_err(|error| {
                                deadpan_core::EditError {
                                    code: deadpan_core::EditErrorCode::IdentityConflict,
                                    message: error.to_string(),
                                    current_revision: None,
                                }
                            })?,
                            marks: Vec::new(),
                        },
                    })
                }
                other => panic!("unexpected gap-duration allocation: {other:?}"),
            },
            |_, _| panic!("gap duration needs no media resolver"),
        )?;
        assert_eq!(pools, [(if outer_play { 6 } else { 0 }, 0), (0, 0)]);
        assert_eq!(plan.context.cursor, ProjectFrame(cursor));
        let authority = plan.context.selected_hold_duration.as_ref().unwrap();
        assert_eq!(authority.presentation.is_none(), dormant);
        assert_eq!(authority.also.len(), 1);
        for selected in [&authority.target, &authority.also[0]] {
            assert!(matches!(&plan.document.nodes()[&selected.node].kind,
                NodeKind::Hold { recipe } if recipe.duration.frames() == length));
        }
        if outer_play {
            for gap in &gap_nodes {
                assert_eq!(plan.document.nodes()[gap], document.nodes()[gap]);
            }
        }
        let request = plan.request.as_ref().ok_or("gap duration made no edit")?;
        let isolation = deadpan_core::derive_scoped_isolation(document, request, &plan.document)?;
        let original = context.selected_hold_duration.as_ref().unwrap();
        for (old, new) in std::iter::once(&original.target)
            .chain(&original.also)
            .zip(std::iter::once(&authority.target).chain(&authority.also))
        {
            assert_eq!(isolation.map_forward(old)?, *new);
            assert_eq!(isolation.map_backward(new)?, *old);
            let edge = new.repeats.last().unwrap();
            let RepeatEditBranch::Play { iteration } = &edge.branch else {
                panic!("an explicit owned gap must retain its Play edge");
            };
            assert_eq!(
                plan.document.gap_overrides()[&edge.repeat].get(iteration),
                Some(&new.node)
            );
        }
        assert_eq!(
            original
                .presentation
                .as_ref()
                .map(|instance| isolation.map_instance_forward(instance))
                .transpose()?,
            authority.presentation
        );
        let transaction = deadpan_core::apply(document, request)?;
        assert_eq!(transaction.inverse.apply(&plan.document)?, *document);
        assert_eq!(
            ProjectDocument::from_json(&plan.document.to_json()?)?,
            plan.document
        );
        mapped = Some((authority.target.clone(), authority.also[0].clone()));
        Ok(request.command.clone())
    })?;
    assert_eq!(after.duration()?.frames(), total);
    assert_eq!(
        after.nodes().len(),
        before.nodes().len() + if outer_play { 6 } else { 0 }
    );
    assert!(mapped.is_some());
    for (index, (old, origin)) in old_pcm.iter().zip(&new_origins).enumerate() {
        let actual = pcm(&project, *origin, old_origins[index])?;
        assert_eq!(old.len(), 256);
        assert_eq!(actual.len(), 256);
        assert!(old.iter().flatten().any(|sample| sample.abs() > 0.001));
        for (sample, (before, after)) in old.iter().zip(actual).enumerate() {
            for channel in 0..2 {
                assert!(after[channel].is_finite());
                assert!(
                    (after[channel] - before[channel]).abs() < 1e-6,
                    "{name} Source {index}, old origin {}, new origin {origin}, sample {sample}, channel {channel}",
                    old_origins[index]
                );
            }
        }
    }
    let mut fixture = project.finish(vec!["Hold (insert time)", "Repeat operator"],
        pictures.into_iter().enumerate().map(|(frame, picture)|
            (u64::try_from(frame).unwrap(), picture)).collect(),
        vec![format!("Real cfr-bframes.mp4 at 30000/1001. A fresh unbound Original 12..28 / outer 2(inner 3 Original 28..32 with explicit 3f, 5f and dormant 7f gap Holds) / Original 32..42 tree starts at 68 frames. Embedded inner 1f gap remains distinct and overridden, with no trailing gap. Outer Play={outer_play}, dormant primary={dormant}; primary gap is processed before the earlier live gap, setting both to {length}f. Literal final output {total} frames, final primary cursor {cursor}. All six independently retained nonzero Source windows compare within 1e-6 at analytic moved origins {new_origins:?}, from {old_origins:?}. Explicit gap and outer-gap silence is checked separately. Ordered targets and primary presentation match the executed isolation proof. Actual mapped target pair: {mapped:?}.")])?;
    fixture.audio = quiet
        .into_iter()
        .map(|frame| Ok((boundary(frame)? + 512, false)))
        .collect::<Result<_>>()?;
    Ok(fixture)
}

pub fn play_grow(dir: &Path) -> Result<Fixture> {
    build(dir, true, 7, false)
}
pub fn play_shrink(dir: &Path) -> Result<Fixture> {
    build(dir, true, 1, false)
}
pub fn shared_grow(dir: &Path) -> Result<Fixture> {
    build(dir, false, 7, false)
}
pub fn shared_shrink(dir: &Path) -> Result<Fixture> {
    build(dir, false, 1, false)
}
pub fn dormant_play(dir: &Path) -> Result<Fixture> {
    build(dir, true, 4, true)
}
pub fn dormant_shared(dir: &Path) -> Result<Fixture> {
    build(dir, false, 4, true)
}
