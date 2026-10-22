//! Qualified real Original A/V through a shared ordinary Hold-duration command.
use super::*;
use deadpan_core::{BeatNode, Subtree};

const BODY_STARTS: [i64; 4] = [16, 25, 34, 43];
const CLICK_IN_SUFFIX: i64 = 1554;

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
fn layout(frames: i64) -> (Vec<Expected>, Vec<i64>, Vec<i64>) {
    let mut pictures: Vec<_> = (12..28).map(original).collect();
    let mut suffixes = Vec::new();
    let mut quiet = Vec::new();
    for outer in 0..2 {
        for inner in 0..2 {
            pictures.push(original(28));
            quiet.push(pictures.len() as i64);
            pictures.extend(std::iter::repeat_n(original(28), frames as usize));
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

fn build(dir: &Path, grow: bool, semantic: bool) -> Result<Fixture> {
    let name = match (grow, semantic) {
        (true, false) => "ordinary-hold-duration-grow",
        (false, false) => "ordinary-hold-duration-shrink",
        (true, true) => "shared-semantic-hold-duration-grow",
        (false, true) => "shared-semantic-hold-duration-shrink",
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
    let after = project.apply(|_, document, revision| {
        if semantic {
            use deadpan_core::{
                InstancePath, PauseLength, RepeatEditBranch, RepeatEditStep, RepeatInstance,
                ScopedNodeTarget, SemanticAllocation, SemanticAllocationRequest, SemanticContext,
                SemanticInstruction, SemanticProgram, SemanticRegisterBank,
                SemanticSharedHoldDurationContext, plan_semantic,
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
            let context = SemanticContext {
                original: None,
                scoped: None,
                hold_duration: None,
                shared_hold_duration: Some(SemanticSharedHoldDurationContext {
                    root: outer.clone(),
                    target: ScopedNodeTarget {
                        node: target.clone(),
                        repeats: repeats
                            .iter()
                            .map(|step| RepeatEditStep {
                                repeat: step.node.clone(),
                                branch: RepeatEditBranch::Default,
                            })
                            .collect(),
                    },
                    presentation: Some(InstancePath {
                        node: target.clone(),
                        repeats,
                    }),
                }),
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
                    assert!(matches!(
                        request,
                        SemanticAllocationRequest::ParameterEdit { .. }
                    ));
                    Ok(SemanticAllocation::ParameterEdit {
                        new_revision: RevisionId::new(format!("{revision}-parameter")).unwrap(),
                    })
                },
                |_, _| panic!("shared duration needs no media resolver"),
            )?;
            assert_eq!(plan.context.cursor, ProjectFrame(44 + 3 * (frames - 3)));
            assert_eq!(
                plan.context.shared_hold_duration,
                context.shared_hold_duration
            );
            assert_eq!(plan.document.overrides(), document.overrides());
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
    let NodeKind::Hold { recipe: current } = &after.nodes()[&target].kind else {
        return Err("Hold".into());
    };
    let mut changed_duration_only = old.clone();
    changed_duration_only.duration = FrameDuration::new(frames)?;
    assert_eq!(*current, changed_duration_only);
    let (pictures, suffixes, quiet) = layout(frames);
    let final_frames = if grow { 68 } else { 52 };
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
    let route = if semantic {
        "Duration-only All-Default semantic SetHoldDuration, committed through the headless command path"
    } else {
        "Ordinary SetHoldDuration"
    };
    let mut fixture = project.finish(vec!["Hold (insert time)", "Repeat operator"],
        pictures.into_iter().enumerate().map(|(frame, expected)| (frame as u64, expected)).collect(),
        vec![format!("Real cfr-bframes.mp4 A/V at 30000/1001. A fresh raw Subtree contains qualified Source recipes with no retained bindings before the edit: Source owners {source_owners:?}, gap owners {gap_owners:?}. {route} changes the shared Hold definition from 3 to {frames} frames in four nested 2x2 plays. Literal final picture layout {final_frames} frames. Each decoded Original click window retains the old per-owner origin at 20, 29, 38 or 47 plus 1554 samples, within 1e-6. Current click windows and their pre-edit authored peaks: {clicks:?}; silent windows: {audio:?}. The complete nonzero pre/post waveforms are checked directly, without assuming every rounded play has a limited peak above 0.5. Canonical PCM tests separately cover 44.1 kHz unbound and partially bound Sources.")])?;
    fixture.audio = audio;
    Ok(fixture)
}

pub fn grow(dir: &Path) -> Result<Fixture> {
    build(dir, true, false)
}
pub fn shrink(dir: &Path) -> Result<Fixture> {
    build(dir, false, false)
}

pub fn semantic_grow(dir: &Path) -> Result<Fixture> {
    build(dir, true, true)
}
pub fn semantic_shrink(dir: &Path) -> Result<Fixture> {
    build(dir, false, true)
}
