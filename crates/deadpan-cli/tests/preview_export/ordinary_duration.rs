//! Qualified real Original A/V through a shared ordinary Hold-duration command.
use super::*;
use deadpan_core::{BeatNode, Subtree};

const BODY_STARTS: [i64; 4] = [16, 25, 34, 43];
const CLICK_IN_SUFFIX: i64 = 1554;

#[derive(Clone, Copy)]
enum Route {
    Raw,
    Shared,
    Mixed { outer_play: bool },
}

fn boundary(frame: i64) -> Result<i64> {
    Ok(i64::try_from(
        ExactRatio::new(i128::from(frame) * 8008, 5)?.round_even()?,
    )?)
}

fn pcm(project: &Project, start: i64) -> Result<Vec<[f64; 2]>> {
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

/// Authored layout: Original12..28, two outer plays of two bodies, then
/// Original32..42. Every body is Original28 / freeze / Original29..32;
/// inner and outer gaps are two silent background frames, never trailing.
fn layout(frames: [i64; 4]) -> (Vec<Expected>, Vec<i64>, Vec<i64>) {
    let mut pictures: Vec<_> = (12..28).map(original).collect();
    let mut suffixes = Vec::new();
    let mut quiet = Vec::new();
    for outer in 0..2 {
        for inner in 0..2 {
            pictures.push(original(28));
            quiet.push(pictures.len() as i64);
            pictures.extend(std::iter::repeat_n(
                original(28),
                frames[outer * 2 + inner] as usize,
            ));
            suffixes.push(pictures.len() as i64);
            pictures.extend((29..32).map(original));
            if inner == 0 {
                quiet.push(pictures.len() as i64);
                pictures.extend([Expected::Background; 2]);
            }
        }
        if outer == 0 {
            quiet.push(pictures.len() as i64);
            pictures.extend([Expected::Background; 2]);
        }
    }
    pictures.extend((32..42).map(original));
    (pictures, suffixes, quiet)
}

fn build(dir: &Path, grow: bool, route: Route) -> Result<Fixture> {
    let name = match (grow, route) {
        (true, Route::Raw) => "ordinary-hold-duration-grow",
        (false, Route::Raw) => "ordinary-hold-duration-shrink",
        (true, Route::Shared) => "shared-semantic-hold-duration-grow",
        (false, Route::Shared) => "shared-semantic-hold-duration-shrink",
        (true, Route::Mixed { outer_play: true }) => "mixed-outer-play-hold-duration-grow",
        (false, Route::Mixed { outer_play: true }) => "mixed-outer-play-hold-duration-shrink",
        (true, Route::Mixed { outer_play: false }) => "mixed-inner-play-hold-duration-grow",
        (false, Route::Mixed { outer_play: false }) => "mixed-inner-play-hold-duration-shrink",
    };
    let mut project = Project::create(dir, name)?;
    project.shorten()?;
    project.split_root(16)?;
    project.split_root(20)?;
    insert_freeze(&mut project, 17, 3, "duration-hold")?;
    project.apply(|_, document, _| {
        Ok(Command::Group {
            parent: document.root().clone(),
            start: 1,
            end: 4,
            id: node("body")?,
            label: "Original and existing Hold".into(),
        })
    })?;
    for (child, repeat) in [("body", "inner"), ("inner", "outer")] {
        project.apply(|_, _, _| {
            Ok(Command::WrapRepeat {
                node: node(child)?,
                id: node(repeat)?,
                plays: 2,
                gap: Some(silent(2, HoldVideo::Background)?),
                anchor_policy: WrapAnchorPolicy::First,
            })
        })?;
    }
    let recipes = project.document()?;
    assert_eq!(recipes.duration()?.frames(), 60);
    // Retain the qualified Source fields and exact selected media, but author
    // a genuinely new unbound tree. InsertTime and WrapRepeat above prepare
    // convenient recipes; their retained sampling bindings are deliberately
    // not part of a Subtree. Delete the old tree through a normal command,
    // then insert fresh authored IDs, preserving the actual asset snapshots.
    let old_ids: Vec<_> = recipes
        .nodes()
        .keys()
        .filter(|id| *id != recipes.root())
        .cloned()
        .collect();
    let fresh = project.fresh(old_ids.len())?;
    let mapping: std::collections::BTreeMap<_, _> = old_ids.into_iter().zip(fresh).collect();
    let target = mapping[&node("duration-hold")?].clone();
    let outer = mapping[&node("outer")?].clone();
    let inner = mapping[&node("inner")?].clone();
    let root = project.fresh(1)?.remove(0);
    let parent = root.clone();
    let mut nodes = std::collections::BTreeMap::new();
    for (old, fresh) in &mapping {
        let mut recipe = recipes.nodes()[old].clone();
        match &mut recipe.kind {
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
        nodes.insert(fresh.clone(), recipe);
    }
    nodes.insert(
        root.clone(),
        BeatNode::sequence(
            "Fresh unbound layout",
            root_children(&recipes)?
                .iter()
                .map(|id| mapping[id].clone())
                .collect(),
        ),
    );
    assert!(recipes.overrides().is_empty());
    assert!(recipes.gap_overrides().is_empty());
    project.delete_range(0, 60)?;
    let before = project.apply(|_, document, _| {
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
    assert_eq!(before.duration()?.frames(), 60);
    let mut source_owners = Vec::new();
    let mut gap_owners = Vec::new();
    for (owner, recipe) in before.nodes() {
        match &recipe.kind {
            NodeKind::Source { .. } => {
                assert!(
                    !before.audio_bindings().bindings().contains_key(owner),
                    "Source {owner} must start unbound"
                );
                source_owners.push(owner.clone());
            }
            NodeKind::Repeat { gap: Some(_), .. } => {
                assert!(
                    !before.audio_bindings().gap_bindings().contains_key(owner),
                    "Repeat gap {owner} must start unbound"
                );
                gap_owners.push(owner.clone());
            }
            _ => {}
        }
    }
    assert_eq!(source_owners.len(), 4);
    assert_eq!(gap_owners.len(), 2);
    assert!(before.audio_bindings().is_empty());
    let expected: Vec<_> = BODY_STARTS
        .into_iter()
        .map(|start| pcm(&project, boundary(start + 4)? + CLICK_IN_SUFFIX))
        .collect::<Result<_>>()?;
    let NodeKind::Hold { recipe: old } = &before.nodes()[&target].kind else {
        return Err("Hold".into());
    };
    let frames = if grow { 5 } else { 1 };
    let mut resized_node = target.clone();
    let after = project.apply(|project, document, revision| {
        if !matches!(route, Route::Raw) {
            use deadpan_core::{
                InstancePath, PauseLength, RepeatEditBranch, RepeatEditStep, RepeatInstance,
                ScopedNodeTarget, SemanticAllocation, SemanticAllocationRequest, SemanticContext,
                SemanticInstruction, SemanticMixedHoldDurationContext, SemanticProgram,
                SemanticRegisterBank, SemanticSharedHoldDurationContext, plan_semantic,
            };
            let repeats = [&outer, &inner]
                .into_iter()
                .map(|node| {
                    let NodeKind::Repeat { iterations, .. } = &document.nodes()[node].kind else {
                        panic!("captured Repeat")
                    };
                    RepeatInstance {
                        node: node.clone(),
                        iteration: iterations.at(1).unwrap(),
                    }
                })
                .collect::<Vec<_>>();
            let scoped_target = ScopedNodeTarget {
                node: target.clone(),
                repeats: repeats
                    .iter()
                    .enumerate()
                    .map(|(index, step)| RepeatEditStep {
                        repeat: step.node.clone(),
                        branch: match route {
                            Route::Mixed { outer_play } if (index == 0) == outer_play => {
                                RepeatEditBranch::Play {
                                    iteration: step.iteration.clone(),
                                }
                            }
                            _ => RepeatEditBranch::Default,
                        },
                    })
                    .collect(),
            };
            let presentation = Some(InstancePath {
                node: target.clone(),
                repeats,
            });
            if matches!(route, Route::Mixed { .. }) {
                let NodeKind::Repeat { child: body, .. } = &document.nodes()[&inner].kind else {
                    panic!("inner Repeat");
                };
                let NodeKind::Sequence { children } = &document.nodes()[body].kind else {
                    panic!("body Sequence");
                };
                assert_eq!(children.len(), 3);
                assert_eq!(children[1], target);
                // The two speech pieces retain partition wrappers around their
                // Source leaves. Body isolation copies six nodes; isolating an
                // outer play additionally copies the inner Repeat itself.
                for side in [&children[0], &children[2]] {
                    let NodeKind::Retime {
                        child,
                        purpose: RetimePurpose::Partition,
                        ..
                    } = &document.nodes()[side].kind
                    else {
                        panic!("partitioned speech");
                    };
                    assert!(matches!(
                        document.nodes()[child].kind,
                        NodeKind::Source { .. }
                    ));
                }
            }
            let requirements = if matches!(route, Route::Mixed { .. }) {
                Some(document.scoped_hold_duration_requirements(
                    &scoped_target,
                    FrameDuration::new(frames)?,
                )?)
            } else {
                None
            };
            let identities = if let Some(requirements) = &requirements {
                assert_eq!(
                    requirements.nodes,
                    if matches!(route, Route::Mixed { outer_play: true }) {
                        7
                    } else {
                        6
                    }
                );
                assert_eq!(requirements.marks, 0);
                deadpan_core::OccurrenceIdentities {
                    nodes: project.fresh(requirements.nodes)?,
                    marks: Vec::new(),
                }
            } else {
                Default::default()
            };
            let context = SemanticContext {
                original: None,
                scoped: None,
                hold_duration: None,
                mixed_hold_duration: matches!(route, Route::Mixed { .. }).then(|| {
                    Box::new(SemanticMixedHoldDurationContext {
                        root: outer.clone(),
                        target: scoped_target.clone(),
                        presentation: presentation.clone(),
                    })
                }),
                shared_hold_duration: matches!(route, Route::Shared).then_some(
                    SemanticSharedHoldDurationContext {
                        root: outer.clone(),
                        target: scoped_target,
                        presentation,
                    },
                ),
                parent: parent.clone(),
                cursor: ProjectFrame(45),
                selected_child: Some(outer.clone()),
                visual_selection: None,
            };
            let program = SemanticProgram::new(vec![SemanticInstruction::SetHoldDuration {
                length: PauseLength::Frames {
                    frames: std::num::NonZeroU32::new(u32::try_from(frames)?).unwrap(),
                },
            }])?;
            let bank = std::collections::BTreeMap::new();
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
                    let new_revision = RevisionId::new(format!("{revision}-parameter")).unwrap();
                    match request {
                        SemanticAllocationRequest::Isolation { nodes, marks, .. }
                            if matches!(route, Route::Mixed { .. }) =>
                        {
                            assert_eq!(nodes, identities.nodes.len());
                            assert_eq!(marks, identities.marks.len());
                            Ok(SemanticAllocation::Isolation {
                                new_revision,
                                identities: identities.clone(),
                            })
                        }
                        SemanticAllocationRequest::ParameterEdit { .. }
                            if matches!(route, Route::Shared) =>
                        {
                            Ok(SemanticAllocation::ParameterEdit { new_revision })
                        }
                        _ => panic!("unexpected duration allocation: {request:?}"),
                    }
                },
                |_, _| panic!("shared duration needs no media resolver"),
            )?;
            let prior_changed_uses = if matches!(route, Route::Shared) { 3 } else { 1 };
            assert_eq!(
                plan.context.cursor,
                ProjectFrame(44 + prior_changed_uses * (frames - 3))
            );
            if let Some(mixed) = &plan.context.mixed_hold_duration {
                resized_node = mixed.target.node.clone();
                assert_ne!(resized_node, target);
                assert_eq!(plan.document.nodes()[&target], document.nodes()[&target]);
                assert_eq!(
                    mixed
                        .target
                        .repeats
                        .iter()
                        .map(|s| s.branch.clone())
                        .collect::<Vec<_>>(),
                    context
                        .mixed_hold_duration
                        .as_ref()
                        .unwrap()
                        .target
                        .repeats
                        .iter()
                        .map(|s| s.branch.clone())
                        .collect::<Vec<_>>()
                );
            } else {
                assert_eq!(
                    plan.context.shared_hold_duration,
                    context.shared_hold_duration
                );
                assert_eq!(plan.document.overrides(), document.overrides());
            }
            let transaction = deadpan_core::apply(document, plan.request.as_ref().unwrap())?;
            assert_eq!(transaction.inverse.apply(&plan.document)?, *document);
            return Ok(plan
                .request
                .ok_or("semantic duration committed no edit")?
                .command);
        }
        Ok(Command::SetHoldDuration {
            node: target.clone(),
            duration: FrameDuration::new(frames)?,
        })
    })?;
    let NodeKind::Hold { recipe: current } = &after.nodes()[&resized_node].kind else {
        return Err("Hold".into());
    };
    let mut changed_duration_only = old.clone();
    changed_duration_only.duration = FrameDuration::new(frames)?;
    assert_eq!(*current, changed_duration_only);
    let lengths = match route {
        Route::Mixed { outer_play: true } => [3, 3, frames, frames],
        Route::Mixed { outer_play: false } => [3, frames, 3, frames],
        _ => [frames; 4],
    };
    let (pictures, suffixes, quiet) = layout(lengths);
    let changed_uses = if matches!(route, Route::Mixed { .. }) {
        2
    } else {
        4
    };
    let final_frames = usize::try_from(60 + changed_uses * (frames - 3))?;
    assert_eq!(pictures.len(), final_frames);
    assert_eq!(after.duration()?.frames(), final_frames as i64);
    let mut clicks = Vec::new();
    for (index, (origin, expected)) in suffixes.iter().zip(&expected).enumerate() {
        let start = boundary(*origin)? + CLICK_IN_SUFFIX;
        let actual = pcm(&project, start)?;
        assert_eq!(expected.len(), 256);
        assert_eq!(actual.len(), 256);
        assert!(expected.iter().flatten().any(|sample| sample.abs() > 0.001));
        for (offset, (expected, actual)) in expected.iter().zip(&actual).enumerate() {
            for channel in 0..2 {
                assert!(actual[channel].is_finite());
                assert!(
                    (actual[channel] - expected[channel]).abs() < 1e-6,
                    "{name} Source {index}, old origin {}, new origin {origin}, sample {offset}, channel {channel}",
                    BODY_STARTS[index] + 4
                );
            }
        }
        clicks.push((
            start,
            expected
                .iter()
                .flatten()
                .fold(0.0_f64, |peak, sample| peak.max(sample.abs())),
        ));
    }
    // These raw unbound plays have independently rounded source phases. Their
    // decoded clicks need not meet the generic >0.5 limited-peak fixture rule.
    // Each complete waveform above must instead match its own pre-edit PCM.
    let mut audio = Vec::new();
    for origin in quiet {
        audio.push((boundary(origin)? + 512, false));
    }
    let route = match route {
        Route::Shared => {
            "Duration-only All-Default semantic SetHoldDuration, committed through the headless command path"
        }
        Route::Raw => "Ordinary SetHoldDuration",
        Route::Mixed { outer_play: true } => {
            "Duration-only outer Play2 / inner Default semantic SetHoldDuration, committed through the headless command path"
        }
        Route::Mixed { outer_play: false } => {
            "Duration-only outer Default / inner Play2 semantic SetHoldDuration, committed through the headless command path"
        }
    };
    let mut fixture = project.finish(vec!["Hold (insert time)", "Repeat operator"],
        pictures.into_iter().enumerate().map(|(frame, expected)| (frame as u64, expected)).collect(),
        vec![format!("Real cfr-bframes.mp4 A/V at 30000/1001. A fresh raw Subtree contains qualified Source recipes with no retained bindings before the edit: Source owners {source_owners:?}, gap owners {gap_owners:?}. {route} changes {changed_uses} uses of the three-frame Hold to {frames} frames; four nested 2x2 plays have literal Hold lengths {lengths:?}. Literal final picture layout {final_frames} frames. Each decoded Original click window retains the old per-owner origin at 20, 29, 38 or 47 plus 1554 samples, within 1e-6. Current click windows and their pre-edit authored peaks: {clicks:?}; silent windows: {audio:?}. The complete nonzero pre/post waveforms are checked directly, without assuming every rounded play has a limited peak above 0.5. Canonical PCM tests separately cover 44.1 kHz unbound and partially bound Sources.")])?;
    fixture.audio = audio;
    Ok(fixture)
}

pub fn grow(dir: &Path) -> Result<Fixture> {
    build(dir, true, Route::Raw)
}
pub fn shrink(dir: &Path) -> Result<Fixture> {
    build(dir, false, Route::Raw)
}

pub fn semantic_grow(dir: &Path) -> Result<Fixture> {
    build(dir, true, Route::Shared)
}
pub fn semantic_shrink(dir: &Path) -> Result<Fixture> {
    build(dir, false, Route::Shared)
}

pub fn mixed_outer_grow(dir: &Path) -> Result<Fixture> {
    build(dir, true, Route::Mixed { outer_play: true })
}
pub fn mixed_outer_shrink(dir: &Path) -> Result<Fixture> {
    build(dir, false, Route::Mixed { outer_play: true })
}
pub fn mixed_inner_grow(dir: &Path) -> Result<Fixture> {
    build(dir, true, Route::Mixed { outer_play: false })
}
pub fn mixed_inner_shrink(dir: &Path) -> Result<Fixture> {
    build(dir, false, Route::Mixed { outer_play: false })
}
