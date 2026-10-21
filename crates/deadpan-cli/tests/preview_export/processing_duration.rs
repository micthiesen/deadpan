//! Real qualified Original through fixed Preserve and Shift input windows.
use super::*;
use deadpan_core::{AudioClockRoot, BeatNode, Subtree};

fn boundary(frame: i64) -> Result<i64> {
    Ok(i64::try_from(
        ExactRatio::new(i128::from(frame) * 8008, 5)?.round_even()?,
    )?)
}

fn source_peak(project: &Project, range: std::ops::Range<i64>) -> Result<f64> {
    let mut peak = 0.0_f64;
    let mut start = range.start;
    while start < range.end {
        let end = (start + 256).min(range.end);
        let audio = success(&[
            "inspect-audio",
            project.path(),
            "--samples",
            &start.to_string(),
            &end.to_string(),
            "--authored-bus",
        ])?;
        let samples: Vec<[f64; 2]> = serde_json::from_value(audio["audio"]["samples"].clone())?;
        assert_eq!(samples.len(), (end - start) as usize);
        for sample in samples.into_iter().flatten() {
            assert!(sample.is_finite());
            peak = peak.max(sample.abs());
        }
        start = end;
    }
    Ok(peak)
}

fn source_in(document: &ProjectDocument, root: &NodeId) -> Result<NodeId> {
    let mut pending = vec![root];
    let mut sources = Vec::new();
    while let Some(owner) = pending.pop() {
        if matches!(document.nodes()[owner].kind, NodeKind::Source { .. }) {
            sources.push(owner.clone());
        }
        pending.extend(document.children(owner));
    }
    if sources.len() != 1 {
        return Err("the qualified fragment must contain one raw Source".into());
    }
    Ok(sources.remove(0))
}

fn layout(preserve: bool, hold: i64) -> Vec<Expected> {
    let mut pictures: Vec<_> = (12..28).map(original).collect();
    // Retime samples output centers. Preserve selects [1,9) at 2x, so
    // centers 1+(n+1/2)*2 are exactly body points 2,4,6,8. Shift+3
    // selects [0,8) at unity, with centers n+1/2 in body frames 0..8.
    let selected = if preserve {
        vec![2, 4, 6, 8]
    } else {
        (0..8).collect()
    };
    for input in selected {
        let source_start = 1 + hold;
        pictures.push(if input < source_start {
            original(28)
        } else if input < source_start + 3 {
            original((29 + input - source_start) as u64)
        } else {
            Expected::Background
        });
    }
    pictures.extend((29..32).map(original));
    assert_eq!(pictures.len(), if preserve { 23 } else { 27 });
    pictures
}

fn build(dir: &Path, preserve: bool, grow: bool) -> Result<Fixture> {
    let (name, pitch, crop, output) = match (preserve, grow) {
        (true, true) => (
            "processing-duration-preserve-grow",
            PitchPolicy::Preserve,
            1,
            4,
        ),
        (true, false) => (
            "processing-duration-preserve-shrink",
            PitchPolicy::Preserve,
            1,
            4,
        ),
        (false, true) => (
            "processing-duration-shift-grow",
            PitchPolicy::Shift { semitones: 3 },
            0,
            8,
        ),
        (false, false) => (
            "processing-duration-shift-shrink",
            PitchPolicy::Shift { semitones: 3 },
            0,
            8,
        ),
    };
    let mut project = Project::create(dir, name)?;
    project.shorten()?;
    project.split_root(16)?;
    project.split_root(20)?;
    insert_freeze(&mut project, 17, 3, "prepared-freeze")?;
    let prepared = project.document()?;
    let children = root_children(&prepared)?;
    assert_eq!(children.len(), 5);
    let [root, rate, body, pause, pad] =
        <[NodeId; 5]>::try_from(project.fresh(5)?).map_err(|_| "five fresh identities")?;
    let mut nodes = std::collections::BTreeMap::new();
    let lead = super::fixed_retime_duration::clone_fragment(
        &mut project,
        &prepared,
        &children[0],
        &mut nodes,
    )?;
    let pre = super::fixed_retime_duration::clone_fragment(
        &mut project,
        &prepared,
        &children[1],
        &mut nodes,
    )?;
    let speech = super::fixed_retime_duration::clone_fragment(
        &mut project,
        &prepared,
        &children[3],
        &mut nodes,
    )?;
    let outside = super::fixed_retime_duration::clone_fragment(
        &mut project,
        &prepared,
        &children[3],
        &mut nodes,
    )?;
    let mut processor = BeatNode::sequence("Fixed processing window", vec![]);
    processor.kind = NodeKind::Retime {
        purpose: RetimePurpose::Edit,
        child: body.clone(),
        duration: FrameDuration::new(output)?,
        mapping: FrameRange::new(ProjectFrame(crop), ProjectFrame(crop + 8))?,
        pitch,
    };
    nodes.extend([
        (
            root.clone(),
            BeatNode::sequence(
                "Fresh processing layout",
                vec![lead, rate.clone(), outside.clone()],
            ),
        ),
        (rate.clone(), processor),
        (
            body,
            BeatNode::sequence(
                "Original / Hold / Source / background",
                vec![pre, pause.clone(), speech.clone(), pad.clone()],
            ),
        ),
        (pause.clone(), prepared.nodes()[&children[2]].clone()),
        (
            pad,
            BeatNode::hold("Background", silent(4, HoldVideo::Background)?),
        ),
    ]);
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
    assert_eq!(before.duration()?.frames(), 19 + output);
    assert!(before.audio_bindings().is_empty());
    assert_eq!(
        before
            .nodes()
            .values()
            .filter(|node| matches!(node.kind, NodeKind::Source { .. }))
            .count(),
        4
    );
    let interior = source_in(&before, &speech)?;
    let outer = source_in(&before, &outside)?;
    let hold = if grow { 5 } else { 1 };
    let after = project.apply(|_, _, _| {
        Ok(Command::SetHoldDuration {
            node: pause.clone(),
            duration: FrameDuration::new(hold)?,
        })
    })?;
    assert_eq!(after.duration()?, before.duration()?);
    for owner in [&rate, &speech, &interior, &outside, &outer] {
        assert_eq!(after.nodes()[owner], before.nodes()[owner]);
    }
    let mut expected_hold = before.nodes()[&pause].clone();
    let NodeKind::Hold { recipe } = &mut expected_hold.kind else {
        return Err("Hold".into());
    };
    recipe.duration = FrameDuration::new(hold)?;
    assert_eq!(after.nodes()[&pause], expected_hold);
    let inside = &after.audio_bindings().bindings()[&interior];
    assert_eq!(
        inside.lattice.reference.root,
        AudioClockRoot::PreserveInputPointCeil {
            stage: rate.clone()
        }
    );
    for owner in [&rate, &outer] {
        let binding = &after.audio_bindings().bindings()[owner];
        assert_eq!(
            binding.lattice.reference.root,
            AudioClockRoot::ProjectRootRoundEven
        );
        assert!(
            binding.reanchors.is_empty(),
            "the fixed outer output must not move"
        );
    }
    // Only the current processed Source interval contributes to this guard;
    // an audible outside suffix cannot hide a muted processor. These literal
    // RootRoundEven endpoints are 18.5..20 / 16.5..18 for Preserve and
    // 22..24 / 18..21 for Shift grow/shrink. This is an energy admission
    // check, separate from the exact phase and canonical DSP oracles.
    let active = match (preserve, grow) {
        (true, true) => 29630..32032,
        (true, false) => 26426..28829,
        (false, true) => 35235..38438,
        (false, false) => 28829..33634,
    };
    let peak = source_peak(&project, active.clone())?;
    assert!(
        peak > 0.001,
        "{name} processed Source {active:?} must be audible, peak={peak}"
    );
    let pictures = layout(preserve, hold);
    let mut fixture = project.finish(
        vec!["Hold (insert time)", "Retime", "Stretch / pitch"],
        pictures.into_iter().enumerate().map(|(frame, picture)| (frame as u64, picture)).collect(),
        vec![format!("Real retained cfr-bframes.mp4 A/V, 30000/1001 project rate. Complete qualified Partition/Source fragments are cloned to fresh IDs with FOUR initially unbound raw Source owners. Ordinary Hold3->{hold} is inside fixed {pitch:?} input[{crop},{}), output{output}; root remains{} frames. Preserve samples body points2,4,6,8; unity Shift samples body frames0..8. The interior Source uses its own PreserveInputPointCeil clock; processor and outside Source remain on unmoved RootRoundEven outputs. Processed Source-only window {active:?} has measured peak {peak}; the outside suffix cannot supply that energy. Full canonical input, historical crop/phase and fractional-resume output controls are tested separately with independent PCM and canonical DSP. This media fixture does not compare changed processed audio against old processed audio; every emitted picture/provenance and full current emitted audio must verify at offset0.", crop+8, 19+output)],
    )?;
    // Preserve's selected input begins in the Hold. Shift first plays the
    // one-frame Original prefix, then enters it. Inspect away from either seam.
    fixture
        .audio
        .push((boundary(if preserve { 16 } else { 17 })? + 256, false));
    Ok(fixture)
}

pub fn preserve_grow(dir: &Path) -> Result<Fixture> {
    build(dir, true, true)
}
pub fn preserve_shrink(dir: &Path) -> Result<Fixture> {
    build(dir, true, false)
}
pub fn shift_grow(dir: &Path) -> Result<Fixture> {
    build(dir, false, true)
}
pub fn shift_shrink(dir: &Path) -> Result<Fixture> {
    build(dir, false, false)
}
