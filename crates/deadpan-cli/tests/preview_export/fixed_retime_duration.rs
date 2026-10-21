//! Qualified Original A/V inside a fixed-window affine Retime.
use super::*;
use deadpan_core::{BeatNode, Subtree};

const CLICK_IN_SOURCE: i64 = 1554;

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

fn same_pcm(expected: &[[f64; 2]], actual: &[[f64; 2]], label: &str) {
    assert_eq!(expected.len(), 256);
    assert_eq!(actual.len(), expected.len());
    assert!(expected.iter().flatten().any(|value| value.abs() > 0.001));
    for (sample, (expected, actual)) in expected.iter().zip(actual).enumerate() {
        for channel in 0..2 {
            assert!(actual[channel].is_finite());
            assert!(
                (actual[channel] - expected[channel]).abs() < 1e-6,
                "{label} sample {sample} channel {channel}: {} != {}",
                actual[channel],
                expected[channel]
            );
        }
    }
}

/// Root: Original12..28, fixed eight-frame stage, Original29..32.
/// Stage child: Original28 / silent freeze / Original29..32 / background4.
/// Its ordinary FollowSpeed window always selects child [0,8), output8.
fn layout(hold_frames: i64) -> Vec<Expected> {
    let mut pictures: Vec<_> = (12..28).map(original).collect();
    let mut stage = vec![original(28)];
    stage.extend(std::iter::repeat_n(original(28), hold_frames as usize));
    stage.extend((29..32).map(original));
    stage.extend([Expected::Background; 4]);
    assert!(stage.len() >= 8);
    pictures.extend(stage.into_iter().take(8));
    pictures.extend((29..32).map(original));
    assert_eq!(pictures.len(), 27);
    pictures
}

/// Splitting the qualified Original can leave Partition wrappers around the
/// raw Source. Copy the complete owned fragment, including those mappings,
/// into fresh identities; a root-only copy would name deleted descendants.
fn clone_fragment(
    project: &mut Project,
    document: &ProjectDocument,
    root: &NodeId,
    nodes: &mut std::collections::BTreeMap<NodeId, BeatNode>,
) -> Result<NodeId> {
    assert!(document.overrides().is_empty());
    assert!(document.gap_overrides().is_empty());
    let mut order = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(owner) = pending.pop() {
        match &document.nodes()[&owner].kind {
            NodeKind::Sequence { children } => pending.extend(children.iter().cloned()),
            NodeKind::Repeat { child, .. } | NodeKind::Retime { child, .. } => {
                pending.push(child.clone());
            }
            NodeKind::Source { .. } | NodeKind::Hold { .. } => {}
        }
        order.push(owner);
    }
    let fresh = project.fresh(order.len())?;
    let mapping: std::collections::BTreeMap<_, _> = order.into_iter().zip(fresh).collect();
    for (old, fresh) in &mapping {
        let mut recipe = document.nodes()[old].clone();
        match &mut recipe.kind {
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
        assert!(nodes.insert(fresh.clone(), recipe).is_none());
    }
    Ok(mapping[root].clone())
}

fn build(dir: &Path, grow: bool) -> Result<Fixture> {
    let name = if grow {
        "fixed-retime-hold-duration-grow"
    } else {
        "fixed-retime-hold-duration-shrink"
    };
    let mut project = Project::create(dir, name)?;
    project.shorten()?;
    project.split_root(16)?;
    project.split_root(20)?;
    insert_freeze(&mut project, 17, 3, "prepared-freeze")?;
    let prepared = project.document()?;
    let children = root_children(&prepared)?;
    assert_eq!(children.len(), 5);
    // Use qualified Source/Freeze fields from real commands, but never their
    // authored sampling bindings. A fresh Subtree has new physical owners.
    let ids = project.fresh(5)?;
    let [root, rate, body, pause, pad] =
        <[NodeId; 5]>::try_from(ids).map_err(|_| "five fresh identities")?;
    let mut nodes = std::collections::BTreeMap::new();
    let lead = clone_fragment(&mut project, &prepared, &children[0], &mut nodes)?;
    let pre = clone_fragment(&mut project, &prepared, &children[1], &mut nodes)?;
    let speech = clone_fragment(&mut project, &prepared, &children[3], &mut nodes)?;
    let outside = clone_fragment(&mut project, &prepared, &children[3], &mut nodes)?;
    let mut fixed = BeatNode::sequence("Fixed eight-frame window", vec![]);
    fixed.kind = NodeKind::Retime {
        purpose: RetimePurpose::Edit,
        child: body.clone(),
        duration: FrameDuration::new(8)?,
        mapping: FrameRange::new(ProjectFrame(0), ProjectFrame(8))?,
        pitch: PitchPolicy::FollowSpeed,
    };
    let mut background = BeatNode::sequence("Background padding", vec![]);
    background.kind = NodeKind::Hold {
        recipe: silent(4, HoldVideo::Background)?,
    };
    nodes.extend([
        (
            root.clone(),
            BeatNode::sequence(
                "Fresh fixed-window layout",
                vec![lead.clone(), rate.clone(), outside.clone()],
            ),
        ),
        (rate.clone(), fixed),
        (
            body,
            BeatNode::sequence(
                "Original / Hold / Source / padding",
                vec![pre.clone(), pause.clone(), speech.clone(), pad.clone()],
            ),
        ),
        (pause.clone(), prepared.nodes()[&children[2]].clone()),
        (pad, background),
    ]);
    // Every reachable child now names the complete fresh inventory. Validate
    // this before deleting the convenient preparation tree through the CLI.
    for recipe in nodes.values() {
        match &recipe.kind {
            NodeKind::Sequence { children } => {
                assert!(children.iter().all(|child| nodes.contains_key(child)));
            }
            NodeKind::Repeat { child, .. } | NodeKind::Retime { child, .. } => {
                assert!(nodes.contains_key(child));
            }
            NodeKind::Source { .. } | NodeKind::Hold { .. } => {}
        }
    }
    project.delete_range(0, prepared.duration()?.frames())?;
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
    assert_eq!(before.duration()?.frames(), 27);
    assert!(before.audio_bindings().is_empty());
    let source_owners: Vec<_> = before
        .nodes()
        .iter()
        .filter_map(|(owner, node)| {
            matches!(node.kind, NodeKind::Source { .. }).then_some(owner.clone())
        })
        .collect();
    assert_eq!(source_owners.len(), 4);
    for owner in &source_owners {
        assert!(
            !before.audio_bindings().bindings().contains_key(owner),
            "Source {owner} must start unbound"
        );
    }
    let original_audio_clock = before.assets()[&project.asset]
        .audio
        .ok_or("Original audio")?;
    // These are literal authored origins, not plan/event-derived offsets:
    // old interior Source starts at20; outside Source always starts at24.
    let old_interior = pcm(&project, boundary(20)? + CLICK_IN_SOURCE)?;
    let old_outside = pcm(&project, boundary(24)? + CLICK_IN_SOURCE)?;
    let frames = if grow { 5 } else { 1 };
    let new_origin = if grow { 22 } else { 18 };
    let after = project.apply(|_, _, _| {
        Ok(Command::SetHoldDuration {
            node: pause.clone(),
            duration: FrameDuration::new(frames)?,
        })
    })?;
    assert_eq!(after.duration()?.frames(), 27);
    assert_eq!(after.nodes()[&rate], before.nodes()[&rate]);
    assert_eq!(after.nodes()[&speech], before.nodes()[&speech]);
    assert_eq!(after.nodes()[&outside], before.nodes()[&outside]);
    let NodeKind::Hold { recipe: old } = &before.nodes()[&pause].kind else {
        return Err("Hold".into());
    };
    let NodeKind::Hold { recipe: current } = &after.nodes()[&pause].kind else {
        return Err("Hold".into());
    };
    let mut expected = old.clone();
    expected.duration = FrameDuration::new(frames)?;
    assert_eq!(*current, expected);
    let interior_start = boundary(new_origin)? + CLICK_IN_SOURCE;
    let outside_start = boundary(24)? + CLICK_IN_SOURCE;
    same_pcm(
        &old_interior,
        &pcm(&project, interior_start)?,
        "fixed-window interior Source",
    );
    same_pcm(
        &old_outside,
        &pcm(&project, outside_start)?,
        "unmoved outside Source",
    );
    let peaks: Vec<_> = [&old_interior, &old_outside]
        .into_iter()
        .map(|samples| {
            samples
                .iter()
                .flatten()
                .fold(0.0_f64, |peak, value| peak.max(value.abs()))
        })
        .collect();
    let mut audio = vec![(boundary(17)? + 512, false)];
    if !grow {
        audio.push((boundary(21)? + 512, false));
    }
    let pictures = layout(frames);
    let mut fixture = project.finish(
        vec!["Hold (insert time)", "Retime"],
        pictures.into_iter().enumerate().map(|(frame, picture)| (frame as u64, picture)).collect(),
        vec![format!("Real retained cfr-bframes.mp4 A/V, project 30000/1001; Original audio span {original_audio_clock:?}. Convenience split/freeze recipes are inserted as a fresh raw Subtree with unbound qualified Source owners {source_owners:?}; no captured bindings are copied. Ordinary Hold 3->{frames} inside an Edit/FollowSpeed Retime keeps fixed child window [0,8), output 8 and root 27 frames. Literal every-picture layout: Original 12..28, the first 8 pictures of [Original 28, freeze 28 x{frames}, Original 29..32, background 4], then Original 29..32. Strict nonzero authored PCM comparisons retain the interior pre-edit origin 20 at new origin {new_origin}, and outside origin 24 unchanged, each at +1554 samples for 256 samples within 1e-6. These windows stay far from both finite crop edges. Current windows {interior_start}/{outside_start}; pre-edit peaks {peaks:?}; quiet windows {audio:?}. Generic loud-peak thresholds are not the phase oracle. Full emitted-file picture/audio verification still requires offset 0. Separate canonical regressions cover 44.1 kHz affine phases and current finite sinc support; this case uses the real qualified Original without audio-rate preprocessing.")],
    )?;
    fixture.audio = audio;
    Ok(fixture)
}

pub fn grow(dir: &Path) -> Result<Fixture> {
    build(dir, true)
}
pub fn shrink(dir: &Path) -> Result<Fixture> {
    build(dir, false)
}
